//! Use cases фичи «Проекты»: вход через Rauthy и серверные сессии, онбординг, профиль, API-токены,
//! журнал безопасности, проекты, участники и приглашения по ссылке. Реализует порты ядра `Memberships` (какой проект и
//! роль) и `Authenticator` (проверка сессий и токенов).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use kernel::auth::{API_TOKEN_PREFIX, Authenticator, Credential, Identity, MemberRef, Memberships, ProjectRef, RequestMeta, Role, SessionInfo, Theme, Via};
use kernel::code::require_text;
use kernel::config::Config;
use kernel::crypto::{ct_eq, random_token, sha256_hex};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use uuid::Uuid;

use crate::domain::{
    ApiToken, AuditEvent, Invite, InviteState, MAX_ACTIVE_INVITES, Member, PendingLogin, Project, ProjectKind, SessionRecord, TOKEN_TERMS, User, action,
    check_members, family_name, is_default_personal_name, normalize_currency, normalize_email, normalize_name, normalize_scopes, personal_name, safe_next,
    split_invite_code,
};
use crate::ports::{AccessRepository, Deps, ProjectRepository};

/// `sub` гостя демо-проекта («Посмотреть демо» на лендинге).
pub const GUEST_SUB: &str = "demo:guest";

pub struct ProjectService {
    repo: Arc<dyn ProjectRepository>,
    access: Arc<dyn AccessRepository>,
    deps: Deps,
    config: Arc<Config>,
}

/// Проект для страницы «Проекты»: роль текущего пользователя и участники.
#[derive(Debug, Clone)]
pub struct ProjectView {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub base_currency: String,
    pub role: Role,
    pub require_2fa: bool,
    pub members: Vec<MemberView>,
    /// Действующие приглашения (видит только владелец семейного проекта).
    pub invites: Vec<InviteView>,
}

/// Действующее приглашение: ссылку и QR-код можно показать в любой момент, пока оно не использовано.
#[derive(Debug, Clone)]
pub struct InviteView {
    pub id: Uuid,
    pub role: Role,
    pub expires_at: DateTime<Utc>,
    pub url: String,
}

/// Что видит человек, открывший ссылку-приглашение.
#[derive(Debug, Clone)]
pub struct InvitePreview {
    pub project_name: String,
    pub inviter_name: String,
    pub role: Role,
    pub state: InviteState,
    /// Открывший ссылку уже участник проекта.
    pub member: bool,
}

#[derive(Debug, Clone)]
pub struct MemberView {
    pub user_id: Uuid,
    pub name: String,
    pub email: String,
    pub role: Role,
}

/// Итог входа: сырой id сессии для cookie и куда вести пользователя.
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub raw_session: String,
    pub user_id: Uuid,
    /// Новый пользователь без проектов — сначала онбординг.
    pub onboarding: bool,
    pub next: String,
}

/// Итог удаления аккаунта.
#[derive(Debug, Clone, Copy)]
pub struct Deleted {
    /// Пользователь удалён и в Rauthy (иначе — удалить в личном кабинете Rauthy).
    pub rauthy: bool,
}

/// Длина подписи в коде приглашения: 132 бита HMAC — не подобрать, а QR-код остаётся крупным и читается легко.
const INVITE_SIG_LEN: usize = 22;

/// Состояние приглашения с учётом пригласившего: перестал быть владельцем проекта — приглашение не действует.
fn invite_state(invite: &Invite, members: &[Member], now: DateTime<Utc>) -> InviteState {
    let state = invite.state(now);
    let owner = members.iter().any(|m| m.user_id == invite.created_by && m.role == Role::Owner);
    if state == InviteState::Active && !owner { InviteState::Revoked } else { state }
}

fn audit_event(user: Option<Uuid>, project: Option<Uuid>, act: &str, meta: &RequestMeta, details: &[(&str, String)]) -> AuditEvent {
    AuditEvent {
        id: Uuid::new_v4(),
        at: Utc::now(),
        user_id: user,
        project_id: project,
        action: act.to_string(),
        ip: meta.ip.clone(),
        user_agent: meta.user_agent.clone(),
        details: details.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
    }
}

impl ProjectService {
    pub fn new(repo: Arc<dyn ProjectRepository>, access: Arc<dyn AccessRepository>, deps: Deps, config: Arc<Config>) -> Self {
        Self { repo, access, deps, config }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn oidc_enabled(&self) -> bool {
        self.deps.identity.enabled()
    }

    /// Гость демо — один пользователь на всех посетителей: ему нельзя ничего, что меняет общий
    /// аккаунт или проекты (удаление, выход из демо, токены, чужие сессии).
    async fn deny_guest(&self, user_id: Uuid) -> AppResult<()> {
        if self.is_guest(user_id).await? {
            return Err(AppError::Forbidden("демо-гость: только просмотр".into()));
        }
        Ok(())
    }

    pub async fn is_guest(&self, user_id: Uuid) -> AppResult<bool> {
        Ok(self.repo.user(user_id).await?.is_some_and(|u| u.rauthy_sub == GUEST_SUB))
    }

    async fn audit(&self, e: AuditEvent) {
        // Журнал не должен ронять основное действие; ошибка записи — в лог сервера.
        if let Err(err) = self.access.audit(&e).await {
            eprintln!("audit: {err}");
        }
    }

    // ------------------------------------------------------------ вход и сессии

    /// Начать вход: адрес Rauthy (code + PKCE S256) и что запомнить в подписанной cookie.
    pub async fn begin_login(&self, next: &str, lang: Lang) -> AppResult<(String, PendingLogin)> {
        let r = self.deps.identity.authorize(lang).await?;
        let pending = PendingLogin { state: r.state, nonce: r.nonce, pkce_verifier: r.pkce_verifier, next: safe_next(next), started: Utc::now().timestamp() };
        Ok((r.url, pending))
    }

    /// Вернулись из Rauthy: проверить `state`, обменять код, проверить ID-токен, найти или создать
    /// пользователя по `sub`, открыть новую сессию (новый id при каждом входе).
    pub async fn finish_login(&self, pending: &PendingLogin, state: &str, code: &str, lang: Lang, meta: &RequestMeta) -> AppResult<LoginResult> {
        if pending.expired(Utc::now()) {
            return Err(AppError::Validation("вход занял больше 10 минут — начните заново".into()));
        }
        if state.is_empty() || !ct_eq(state, &pending.state) {
            return Err(AppError::Forbidden("неверный state: ответ Rauthy не относится к этому входу".into()));
        }
        let login = self.deps.identity.exchange(code, &pending.pkce_verifier, &pending.nonce).await?;
        let email = normalize_email(&login.email).unwrap_or_else(|_| login.email.trim().to_lowercase());
        let name = login.display_name();
        // Только по sub: аккаунт с тем же email (например, Facebook) — это другой пользователь,
        // пока владелец не привяжет его в Rauthy.
        // Разработка: демо-пользователи (`demo:*`) стенда Rauthy сопоставляются по email — так
        // заранее настроенная копия Rauthy входит в демо-данные. В продакшене — только по sub.
        if !self.config.is_prod() && self.repo.user_by_sub(&login.sub).await?.is_none() {
            if let Some(demo) = self.repo.user_by_email(&email).await?.filter(|u| u.rauthy_sub.starts_with("demo:") && u.rauthy_sub != GUEST_SUB) {
                self.repo.set_sub(demo.id, &login.sub).await?;
            }
        }
        let user = match self.repo.user_by_sub(&login.sub).await? {
            Some(u) => {
                self.repo.update_user_identity(u.id, &email, &name, login.picture.as_deref()).await?;
                u
            }
            None => {
                let u = User {
                    id: Uuid::new_v4(),
                    rauthy_sub: login.sub.clone(),
                    email,
                    display_name: name,
                    avatar_url: login.picture.clone(),
                    lang,
                    theme: Theme::System,
                };
                self.repo.insert_user(&u).await?;
                u
            }
        };
        let raw = self.open_session(user.id, login.mfa(), meta).await?;
        self.audit(audit_event(Some(user.id), None, action::LOGIN, meta, &[("method", "rauthy".into()), ("mfa", login.mfa().to_string())])).await;
        let onboarding = self.repo.projects_of(user.id).await?.is_empty();
        Ok(LoginResult { raw_session: raw, user_id: user.id, onboarding, next: pending.next.clone() })
    }

    async fn open_session(&self, user_id: Uuid, mfa: bool, meta: &RequestMeta) -> AppResult<String> {
        let raw = random_token(32);
        let rec = SessionRecord::new(sha256_hex(&raw), user_id, mfa, meta.ip.clone(), meta.user_agent.clone(), Utc::now());
        self.access.insert_session(&rec).await?;
        Ok(raw)
    }

    /// Вход без Rauthy под существующим пользователем — только в режиме разработки (`DEV_LOGIN`).
    pub async fn dev_login(&self, email: &str, meta: &RequestMeta) -> AppResult<String> {
        if !self.config.dev_login || self.config.is_prod() {
            return Err(AppError::NotFound("вход для разработки выключен".into()));
        }
        let email = normalize_email(email).map_err(AppError::Validation)?;
        let user = self.repo.user_by_email(&email).await?.ok_or_else(|| AppError::Validation(format!("email: пользователь '{email}' не найден")))?;
        let raw = self.open_session(user.id, false, meta).await?;
        self.audit(audit_event(Some(user.id), None, action::LOGIN, meta, &[("method", "dev".into())])).await;
        Ok(raw)
    }

    /// «Посмотреть демо»: сессия гостя (роль VIEWER в демо-проекте) без регистрации.
    pub async fn guest_login(&self, meta: &RequestMeta) -> AppResult<(String, Uuid)> {
        if !self.config.guest_demo {
            return Err(AppError::NotFound("демо выключено".into()));
        }
        let guest = self.repo.user_by_sub(GUEST_SUB).await?.ok_or_else(|| AppError::NotFound("демо-проект ещё не готов".into()))?;
        let project = self.repo.projects_of(guest.id).await?.into_iter().next().ok_or_else(|| AppError::NotFound("демо-проект ещё не готов".into()))?;
        let raw = self.open_session(guest.id, false, meta).await?;
        Ok((raw, project.0.id))
    }

    /// Выход: сессия отзывается; возвращает адрес выхода из Rauthy (если настроен).
    pub async fn logout(&self, raw: &str, meta: &RequestMeta) -> AppResult<Option<String>> {
        let hash = sha256_hex(raw);
        if let Some(s) = self.access.session(&hash).await? {
            self.access.revoke_session(s.user_id, &hash, Utc::now()).await?;
            self.audit(audit_event(Some(s.user_id), None, action::LOGOUT, meta, &[])).await;
            // Гость демо не входил через Rauthy — выходить там не из чего.
            if self.repo.user(s.user_id).await?.is_some_and(|u| u.rauthy_sub.starts_with("demo:")) {
                return Ok(None);
            }
        }
        Ok(self.deps.identity.logout_url(&format!("{}/", self.config.base_url)).await)
    }

    /// «Выйти везде»: отозвать все сессии, кроме текущей.
    pub async fn logout_everywhere(&self, user_id: Uuid, current_hash: &str, meta: &RequestMeta) -> AppResult<u64> {
        self.deny_guest(user_id).await?;
        let n = self.access.revoke_sessions(user_id, Some(current_hash), Utc::now()).await?;
        self.audit(audit_event(Some(user_id), None, action::LOGOUT_ALL, meta, &[("sessions", n.to_string())])).await;
        Ok(n)
    }

    pub async fn sessions(&self, user_id: Uuid) -> AppResult<Vec<SessionRecord>> {
        self.access.sessions_of(user_id, Utc::now()).await
    }

    pub async fn revoke_session(&self, user_id: Uuid, id_hash: &str, meta: &RequestMeta) -> AppResult<()> {
        self.deny_guest(user_id).await?;
        if !self.access.revoke_session(user_id, id_hash, Utc::now()).await? {
            return Err(AppError::NotFound("сессия не найдена".into()));
        }
        self.audit(audit_event(Some(user_id), None, action::SESSION_REVOKED, meta, &[])).await;
        Ok(())
    }

    // ------------------------------------------------------------ API-токены

    /// Новый личный API-токен на один проект. Сырой токен возвращается один раз, в БД — хеш.
    pub async fn create_token(
        &self,
        user_id: Uuid,
        project_id: Uuid,
        name: &str,
        scopes: &[String],
        days: i64,
        session_mfa: bool,
        meta: &RequestMeta,
    ) -> AppResult<(String, ApiToken)> {
        self.deny_guest(user_id).await?;
        let v = AppError::Validation;
        let name = require_text("name", name).map_err(v)?;
        let scopes = normalize_scopes(scopes).map_err(v)?;
        if !TOKEN_TERMS.contains(&days) {
            return Err(AppError::Validation(format!("expires: допустимо {TOKEN_TERMS:?} дней (0 — бессрочно)")));
        }
        let (project, _) = self
            .repo
            .projects_of(user_id)
            .await?
            .into_iter()
            .find(|(p, _)| p.id == project_id)
            .ok_or_else(|| AppError::NotFound("проект не найден".into()))?;
        if project.require_2fa && !session_mfa {
            return Err(AppError::Forbidden("проект требует 2FA: войдите со вторым фактором, чтобы выпустить токен".into()));
        }
        let raw = format!("{API_TOKEN_PREFIX}{}", random_token(32));
        let now = Utc::now();
        let token = ApiToken {
            id: Uuid::new_v4(),
            user_id,
            project_id,
            name,
            token_hash: sha256_hex(&raw),
            scopes,
            expires_at: (days > 0).then(|| now + Duration::days(days)),
            last_used_at: None,
            revoked_at: None,
            created_at: now,
            mfa: session_mfa,
        };
        self.access.insert_token(&token).await?;
        self.audit(audit_event(Some(user_id), Some(project_id), action::TOKEN_CREATED, meta, &[("name", token.name.clone()), ("scopes", token.scopes.join(" "))])).await;
        Ok((raw, token))
    }

    pub async fn tokens(&self, user_id: Uuid) -> AppResult<Vec<ApiToken>> {
        self.access.tokens_of(user_id).await
    }

    pub async fn revoke_token(&self, user_id: Uuid, id: Uuid, meta: &RequestMeta) -> AppResult<()> {
        self.deny_guest(user_id).await?;
        if !self.access.revoke_token(user_id, id, Utc::now()).await? {
            return Err(AppError::NotFound("токен не найден".into()));
        }
        self.audit(audit_event(Some(user_id), None, action::TOKEN_REVOKED, meta, &[("id", id.to_string())])).await;
        Ok(())
    }

    pub async fn audit_log(&self, user_id: Uuid) -> AppResult<Vec<AuditEvent>> {
        self.access.audit_of(user_id, 100).await
    }

    // ------------------------------------------------------------ онбординг и профиль

    pub async fn user(&self, user_id: Uuid) -> AppResult<User> {
        self.repo.user(user_id).await?.ok_or_else(|| AppError::NotFound("пользователь не найден".into()))
    }

    /// Онбординг: имя, базовая валюта, язык → личный проект со стартовыми справочниками.
    pub async fn onboard(&self, user_id: Uuid, name: &str, base_currency: &str, lang: Lang) -> AppResult<Uuid> {
        let v = AppError::Validation;
        let name = normalize_name(name).map_err(v)?;
        let currency = normalize_currency(base_currency).map_err(v)?;
        let user = self.user(user_id).await?;
        self.repo.update_user_profile(user_id, &name, lang, user.theme).await?;
        if let Some((p, _)) = self.repo.projects_of(user_id).await?.into_iter().find(|(p, _)| p.kind == ProjectKind::Personal) {
            return Ok(p.id);
        }
        self.create_project(user_id, personal_name(lang), ProjectKind::Personal, &currency, &RequestMeta::default()).await
    }

    pub async fn update_profile(&self, user_id: Uuid, name: &str, lang: Lang, theme: Theme) -> AppResult<()> {
        self.deny_guest(user_id).await?;
        let name = normalize_name(name).map_err(AppError::Validation)?;
        self.repo.update_user_profile(user_id, &name, lang, theme).await
    }

    pub async fn set_lang(&self, user_id: Uuid, lang: Lang) -> AppResult<()> {
        // Язык гостя — только в cookie: общий аккаунт не меняем.
        if self.is_guest(user_id).await? {
            return Ok(());
        }
        self.repo.update_user_lang(user_id, lang).await
    }

    /// Удаление аккаунта (PDPA, GDPR): личные проекты с данными, членство в семейных, сессии,
    /// токены; журнал обезличивается; пользователь удаляется и в Rauthy.
    /// Нельзя, если пользователь — единственный владелец семейного проекта с другими участниками:
    /// сначала нужно назначить второго владельца (проект остаётся семье).
    pub async fn delete_account(&self, user_id: Uuid, meta: &RequestMeta) -> AppResult<Deleted> {
        self.deny_guest(user_id).await?;
        let user = self.user(user_id).await?;
        let mine = self.repo.projects_of(user_id).await?;
        for (p, me) in &mine {
            if p.kind == ProjectKind::Family && me.role == Role::Owner {
                let members = self.repo.members(p.id).await?;
                let other_owner = members.iter().any(|(m, _)| m.user_id != user_id && m.role == Role::Owner);
                if members.len() > 1 && !other_owner {
                    return Err(AppError::Validation(format!(
                        "в проекте «{}» вы единственный владелец: назначьте второго владельца или удалите проект",
                        p.name
                    )));
                }
            }
        }
        for (p, _) in &mine {
            let alone = self.repo.members(p.id).await?.len() <= 1;
            if p.kind == ProjectKind::Personal || alone {
                self.deps.purge.purge(p.id).await?;
                self.repo.delete_project(p.id).await?;
            } else {
                self.repo.remove_member(p.id, user_id).await?;
            }
        }
        self.audit(audit_event(None, None, action::ACCOUNT_DELETED, meta, &[])).await;
        self.access.anonymize_audit(user_id).await?;
        self.repo.delete_user(user_id).await?;
        let rauthy = if user.rauthy_sub.contains(':') { false } else { self.deps.identity.delete_user(&user.rauthy_sub).await.unwrap_or(false) };
        Ok(Deleted { rauthy })
    }

    // ------------------------------------------------------------ проекты и участники

    /// Новый проект; создатель — владелец. Справочники наполняются через порт `ProjectBootstrap`.
    pub async fn create_project(&self, user_id: Uuid, name: &str, kind: ProjectKind, base_currency: &str, meta: &RequestMeta) -> AppResult<Uuid> {
        self.deny_guest(user_id).await?;
        let user = self.user(user_id).await?;
        let project = Project::new(name, kind, base_currency, user_id).map_err(AppError::Validation)?;
        let owner = Member { project_id: project.id, user_id, role: Role::Owner };
        self.repo.insert_project(&project, &owner).await?;
        self.deps.bootstrap.init(project.id, &project.base_currency, user.lang).await?;
        self.audit(audit_event(Some(user_id), Some(project.id), action::PROJECT_CREATED, meta, &[("kind", kind.as_str().into())])).await;
        Ok(project.id)
    }

    pub async fn rename(&self, user_id: Uuid, project_id: Uuid, name: &str) -> AppResult<()> {
        self.deny_guest(user_id).await?;
        self.require_owner(user_id, project_id).await?;
        self.repo.rename_project(project_id, &require_text("name", name).map_err(AppError::Validation)?).await
    }

    /// «Требовать 2FA от участников» — только владелец.
    pub async fn set_require_2fa(&self, user_id: Uuid, project_id: Uuid, value: bool, meta: &RequestMeta) -> AppResult<()> {
        self.deny_guest(user_id).await?;
        self.require_owner(user_id, project_id).await?;
        self.repo.set_require_2fa(project_id, value).await?;
        self.audit(audit_event(Some(user_id), Some(project_id), action::REQUIRE_2FA, meta, &[("value", value.to_string())])).await;
        Ok(())
    }

    /// Удалить семейный проект со всеми данными — только владелец, необратимо (с подтверждением).
    pub async fn delete_project(&self, user_id: Uuid, project_id: Uuid, confirm_name: &str, meta: &RequestMeta) -> AppResult<()> {
        self.deny_guest(user_id).await?;
        let project = self.require_owner(user_id, project_id).await?;
        if project.kind == ProjectKind::Personal {
            return Err(AppError::Validation("личный проект удаляется только вместе с аккаунтом".into()));
        }
        if confirm_name.trim() != project.name {
            return Err(AppError::Validation(format!("confirm: введите название проекта «{}» для подтверждения", project.name)));
        }
        self.deps.purge.purge(project_id).await?;
        self.repo.delete_project(project_id).await?;
        self.audit(audit_event(Some(user_id), Some(project_id), action::PROJECT_DELETED, meta, &[("name", project.name)])).await;
        Ok(())
    }

    /// Проекты пользователя с участниками.
    pub async fn overview(&self, user_id: Uuid) -> AppResult<Vec<ProjectView>> {
        let mut out = Vec::new();
        let now = Utc::now();
        for (p, me) in self.repo.projects_of(user_id).await? {
            let rows = self.repo.members(p.id).await?;
            let raw: Vec<Member> = rows.iter().map(|(m, _)| m.clone()).collect();
            let invites = if me.role == Role::Owner && p.kind == ProjectKind::Family {
                self.repo
                    .invites_of(p.id)
                    .await?
                    .into_iter()
                    .filter(|i| invite_state(i, &raw, now) == InviteState::Active)
                    .map(|i| InviteView { url: self.invite_url(i.id), id: i.id, role: i.role, expires_at: i.expires_at })
                    .collect()
            } else {
                Vec::new()
            };
            let members = rows.into_iter().map(|(m, u)| MemberView { user_id: u.id, name: u.display_name, email: u.email, role: m.role }).collect();
            out.push(ProjectView {
                id: p.id,
                name: p.name,
                kind: p.kind.as_str().into(),
                base_currency: p.base_currency,
                role: me.role,
                require_2fa: p.require_2fa,
                members,
                invites,
            });
        }
        Ok(out)
    }

    /// Добавить участника по email (пользователь должен быть зарегистрирован). Только владелец.
    pub async fn add_member(&self, by: Uuid, project_id: Uuid, email: &str, role: Role, meta: &RequestMeta) -> AppResult<()> {
        self.deny_guest(by).await?;
        let project = self.require_owner(by, project_id).await?;
        let email = normalize_email(email).map_err(AppError::Validation)?;
        let user = self
            .repo
            .user_by_email(&email)
            .await?
            .filter(|u| !u.rauthy_sub.starts_with("demo:guest"))
            .ok_or_else(|| AppError::Validation(format!("email: пользователь '{email}' ещё не входил в приложение — пусть войдёт один раз")))?;
        let new = Member { project_id, user_id: user.id, role };
        self.change_members(&project, |ms| {
            ms.retain(|m| m.user_id != user.id);
            ms.push(new.clone());
        })
        .await?;
        self.repo.upsert_member(&new).await?;
        self.audit(audit_event(Some(by), Some(project_id), action::MEMBER_ADDED, meta, &[("member", user.id.to_string()), ("role", role.as_str().into())])).await;
        Ok(())
    }

    pub async fn set_role(&self, by: Uuid, project_id: Uuid, user_id: Uuid, role: Role, meta: &RequestMeta) -> AppResult<()> {
        self.deny_guest(by).await?;
        let project = self.require_owner(by, project_id).await?;
        self.change_members(&project, |ms| {
            for m in ms.iter_mut().filter(|m| m.user_id == user_id) {
                m.role = role;
            }
        })
        .await?;
        // Только смена роли существующего участника: «добавить» так никого нельзя (в т. ч. гостя демо).
        if !self.repo.update_role(project_id, user_id, role).await? {
            return Err(AppError::NotFound(format!("участник {user_id} не найден в проекте")));
        }
        self.audit(audit_event(Some(by), Some(project_id), action::MEMBER_ROLE, meta, &[("member", user_id.to_string()), ("role", role.as_str().into())])).await;
        Ok(())
    }

    /// Удалить участника (владелец) или выйти самому. Его API-токены этого проекта отзываются.
    pub async fn remove_member(&self, by: Uuid, project_id: Uuid, user_id: Uuid, meta: &RequestMeta) -> AppResult<()> {
        self.deny_guest(by).await?;
        let project = if by == user_id { self.load_project(project_id).await? } else { self.require_owner(by, project_id).await? };
        if !self.repo.members(project_id).await?.iter().any(|(m, _)| m.user_id == by) {
            return Err(AppError::NotFound(format!("проект {project_id} не найден")));
        }
        self.change_members(&project, |ms| ms.retain(|m| m.user_id != user_id)).await?;
        self.repo.remove_member(project_id, user_id).await?;
        self.access.revoke_project_tokens(user_id, project_id, Utc::now()).await?;
        self.audit(audit_event(Some(by), Some(project_id), action::MEMBER_REMOVED, meta, &[("member", user_id.to_string())])).await;
        Ok(())
    }

    /// Личный проект → семейный, чтобы пригласить близких. Только владелец. Название по умолчанию
    /// («Личный бюджет») меняется на «Семейный бюджет», своё название остаётся.
    pub async fn make_family(&self, by: Uuid, project_id: Uuid, meta: &RequestMeta) -> AppResult<()> {
        self.deny_guest(by).await?;
        let project = self.require_owner(by, project_id).await?;
        if project.kind == ProjectKind::Family {
            return Ok(());
        }
        let name = if is_default_personal_name(&project.name) { family_name(self.user(by).await?.lang).to_string() } else { project.name };
        self.repo.set_kind(project_id, ProjectKind::Family, &name).await?;
        self.audit(audit_event(Some(by), Some(project_id), action::PROJECT_MADE_FAMILY, meta, &[])).await;
        Ok(())
    }

    // ------------------------------------------------------------ приглашения по ссылке и QR-коду

    /// Код для ссылки: id приглашения и HMAC-подпись секретом приложения. В БД кода нет — копия базы
    /// не даёт рабочих ссылок, а владелец может показать ссылку повторно.
    fn invite_code(&self, id: Uuid) -> String {
        let id = id.simple().to_string();
        let sig = self.config.signer.sign("invite", &id);
        format!("{id}.{}", &sig[..INVITE_SIG_LEN])
    }

    /// Полная ссылка-приглашение (её же кодирует QR).
    pub fn invite_url(&self, id: Uuid) -> String {
        format!("{}/join/{}", self.config.base_url.trim_end_matches('/'), self.invite_code(id))
    }

    /// Приглашение по коду из ссылки; неверная подпись — как несуществующее приглашение.
    async fn invite_by_code(&self, code: &str) -> AppResult<Invite> {
        let gone = || AppError::NotFound("приглашение не найдено: проверьте ссылку или попросите новую".into());
        let (id, _) = split_invite_code(code).ok_or_else(gone)?;
        if !ct_eq(code, &self.invite_code(id)) {
            return Err(gone());
        }
        self.repo.invite(id).await?.ok_or_else(gone)
    }

    /// Новое приглашение в семейный проект (редактор или наблюдатель, 7 дней, один человек). Только владелец.
    pub async fn create_invite(&self, by: Uuid, project_id: Uuid, role: Role, meta: &RequestMeta) -> AppResult<Uuid> {
        self.deny_guest(by).await?;
        let project = self.require_owner(by, project_id).await?;
        if project.kind != ProjectKind::Family {
            return Err(AppError::Validation("личный проект — только для одного человека: сделайте его семейным, чтобы пригласить близких".into()));
        }
        let now = Utc::now();
        let active = self.repo.invites_of(project_id).await?.iter().filter(|i| i.state(now) == InviteState::Active).count();
        if active >= MAX_ACTIVE_INVITES {
            return Err(AppError::Validation(format!("действующих приглашений уже {MAX_ACTIVE_INVITES}: отзовите ненужные")));
        }
        let invite = Invite::new(project_id, role, by, now).map_err(AppError::Validation)?;
        self.repo.insert_invite(&invite).await?;
        self.audit(audit_event(Some(by), Some(project_id), action::INVITE_CREATED, meta, &[("invite", invite.id.to_string()), ("role", role.as_str().into())])).await;
        Ok(invite.id)
    }

    /// Отозвать приглашение — ссылка и QR-код перестают работать. Только владелец.
    pub async fn revoke_invite(&self, by: Uuid, project_id: Uuid, invite_id: Uuid, meta: &RequestMeta) -> AppResult<()> {
        self.deny_guest(by).await?;
        self.require_owner(by, project_id).await?;
        if !self.repo.revoke_invite(project_id, invite_id, Utc::now()).await? {
            return Err(AppError::NotFound("приглашение не найдено или уже использовано".into()));
        }
        self.audit(audit_event(Some(by), Some(project_id), action::INVITE_REVOKED, meta, &[("invite", invite_id.to_string())])).await;
        Ok(())
    }

    /// Страница приглашения: куда зовут, кто и с какой ролью. Ничего не меняет — превью ссылки
    /// в мессенджере приглашение не расходует.
    pub async fn invite_preview(&self, code: &str, viewer: Option<Uuid>) -> AppResult<InvitePreview> {
        let invite = self.invite_by_code(code).await?;
        let project = self.load_project(invite.project_id).await?;
        let rows = self.repo.members(project.id).await?;
        let members: Vec<Member> = rows.iter().map(|(m, _)| m.clone()).collect();
        let inviter_name = match rows.iter().find(|(m, _)| m.user_id == invite.created_by) {
            Some((_, u)) => u.display_name.clone(),
            None => self.repo.user(invite.created_by).await?.map(|u| u.display_name).unwrap_or_default(),
        };
        Ok(InvitePreview {
            project_name: project.name,
            inviter_name,
            role: invite.role,
            state: invite_state(&invite, &members, Utc::now()),
            member: viewer.is_some_and(|v| members.iter().any(|m| m.user_id == v)),
        })
    }

    /// Принять приглашение: пользователь становится участником с ролью из приглашения, приглашение
    /// расходуется (атомарно — по одной ссылке войдёт один человек). Уже участник — просто проект.
    pub async fn accept_invite(&self, user_id: Uuid, code: &str, meta: &RequestMeta) -> AppResult<Uuid> {
        self.deny_guest(user_id).await?;
        let invite = self.invite_by_code(code).await?;
        let project = self.load_project(invite.project_id).await?;
        let mut members: Vec<Member> = self.repo.members(project.id).await?.into_iter().map(|(m, _)| m).collect();
        if members.iter().any(|m| m.user_id == user_id) {
            return Ok(project.id);
        }
        let now = Utc::now();
        let used = || AppError::Validation("приглашением уже воспользовались — попросите новое".into());
        match invite_state(&invite, &members, now) {
            InviteState::Active => {}
            InviteState::Used => return Err(used()),
            InviteState::Expired => return Err(AppError::Validation("срок приглашения истёк — попросите новое".into())),
            InviteState::Revoked => return Err(AppError::Validation("приглашение отозвано — попросите новое".into())),
        }
        let member = Member { project_id: project.id, user_id, role: invite.role };
        members.push(member.clone());
        check_members(project.kind, &members).map_err(AppError::Validation)?;
        if !self.repo.accept_invite(invite.id, &member, now).await? {
            return Err(used());
        }
        let details = [("invite", invite.id.to_string()), ("role", invite.role.as_str().into()), ("invited_by", invite.created_by.to_string())];
        self.audit(audit_event(Some(user_id), Some(project.id), action::MEMBER_JOINED, meta, &details)).await;
        Ok(project.id)
    }

    async fn change_members(&self, project: &Project, f: impl FnOnce(&mut Vec<Member>)) -> AppResult<()> {
        let mut ms: Vec<Member> = self.repo.members(project.id).await?.into_iter().map(|(m, _)| m).collect();
        f(&mut ms);
        check_members(project.kind, &ms).map_err(AppError::Validation)
    }

    async fn load_project(&self, project_id: Uuid) -> AppResult<Project> {
        self.repo.project(project_id).await?.ok_or_else(|| AppError::NotFound(format!("проект {project_id} не найден")))
    }

    /// Только владелец. Чужой проект — 404 (не раскрываем, что он существует).
    async fn require_owner(&self, user_id: Uuid, project_id: Uuid) -> AppResult<Project> {
        let project = self.load_project(project_id).await?;
        let members = self.repo.members(project_id).await?;
        let Some((me, _)) = members.iter().find(|(m, _)| m.user_id == user_id) else {
            return Err(AppError::NotFound(format!("проект {project_id} не найден")));
        };
        if me.role != Role::Owner {
            return Err(AppError::Forbidden("управлять проектом и участниками может только владелец".into()));
        }
        Ok(project)
    }

    // ------------------------------------------------------------ обслуживание и демо

    /// Чистка: истёкшие сессии (старше 30 дней), приглашения, истёкшие больше 30 дней назад, и журнал старше года.
    pub async fn cleanup(&self) -> AppResult<(u64, u64)> {
        let now = Utc::now();
        let s = self.access.purge_sessions(now - Duration::days(30)).await?;
        let a = self.access.purge_audit(now - crate::domain::AUDIT_RETENTION).await?;
        self.repo.purge_invites(now - Duration::days(30)).await?;
        Ok((s, a))
    }

    /// Пользователь с заданным `sub` (демо-данные): найти или создать.
    pub async fn ensure_user(&self, sub: &str, email: &str, name: &str, lang: Lang) -> AppResult<Uuid> {
        if let Some(u) = self.repo.user_by_sub(sub).await? {
            return Ok(u.id);
        }
        // Демо-пользователь из базы до Rauthy (`legacy:<id>`) — тот же человек: переносим его с данными.
        if let Some(u) = self.repo.user_by_email(email).await?.filter(|u| u.rauthy_sub.starts_with("legacy:")) {
            self.repo.set_sub(u.id, sub).await?;
            return Ok(u.id);
        }
        let u = User {
            id: Uuid::new_v4(),
            rauthy_sub: sub.into(),
            email: normalize_email(email).map_err(AppError::Validation)?,
            display_name: normalize_name(name).map_err(AppError::Validation)?,
            avatar_url: None,
            lang,
            theme: Theme::System,
        };
        self.repo.insert_user(&u).await?;
        Ok(u.id)
    }

    pub async fn user_by_sub(&self, sub: &str) -> AppResult<Option<Uuid>> {
        Ok(self.repo.user_by_sub(sub).await?.map(|u| u.id))
    }

    /// Издатель access-токенов провайдера входа (для метаданных MCP-ресурса). `None` — вход не настроен.
    pub async fn token_issuer(&self) -> Option<String> {
        self.deps.identity.issuer().await
    }

    /// Пользователь по access-токену провайдера входа, выпущенному для ресурса `audience`:
    /// (id пользователя, OAuth-клиент). `None` — токен не прошёл проверку или пользователя у нас нет.
    pub async fn verify_access_token(&self, token: &str, audience: &str) -> AppResult<Option<(Uuid, String)>> {
        let Some(t) = self.deps.identity.verify_access_token(token, audience).await? else { return Ok(None) };
        Ok(self.repo.user_by_sub(&t.sub).await?.filter(|u| u.rauthy_sub != GUEST_SUB).map(|u| (u.id, t.client_id)))
    }

    /// Записать событие другой фичи в журнал безопасности (например, действия ассистента через MCP).
    pub async fn record_event(&self, user_id: Uuid, project_id: Option<Uuid>, action: &str, meta: &RequestMeta, details: &[(&str, String)]) {
        self.audit(audit_event(Some(user_id), project_id, action, meta, details)).await;
    }

    /// Проекты пользователя (id, вид, название).
    pub async fn projects_of(&self, user_id: Uuid) -> AppResult<Vec<(Uuid, ProjectKind, String)>> {
        Ok(self.repo.projects_of(user_id).await?.into_iter().map(|(p, _)| (p.id, p.kind, p.name)).collect())
    }

    /// Регистрации за период — для воронки лендинга (только число).
    pub async fn signups_between(&self, from: chrono::DateTime<Utc>, to: chrono::DateTime<Utc>) -> AppResult<i64> {
        self.repo.count_signups(from, to).await
    }

    /// Состоит ли гость демо в проекте.
    pub async fn has_guest(&self, project_id: Uuid) -> AppResult<bool> {
        let Some(guest) = self.repo.user_by_sub(GUEST_SUB).await? else { return Ok(false) };
        Ok(self.repo.members(project_id).await?.iter().any(|(m, _)| m.user_id == guest.id))
    }

    /// Сделать гостя демо (`demo:guest`) наблюдателем проекта — только владелец.
    pub async fn add_guest(&self, by: Uuid, project_id: Uuid) -> AppResult<()> {
        let project = self.require_owner(by, project_id).await?;
        let guest = self.repo.user_by_sub(GUEST_SUB).await?.ok_or_else(|| AppError::NotFound("гость демо не создан".into()))?;
        let member = Member { project_id, user_id: guest.id, role: Role::Viewer };
        self.change_members(&project, |ms| {
            ms.retain(|m| m.user_id != guest.id);
            ms.push(member.clone());
        })
        .await?;
        self.repo.upsert_member(&member).await
    }

    /// Удалить проект с данными без проверки прав (сброс демо раз в сутки).
    pub async fn purge_project(&self, project_id: Uuid) -> AppResult<()> {
        self.deps.purge.purge(project_id).await?;
        self.repo.delete_project(project_id).await
    }
}

#[async_trait]
impl Memberships for ProjectService {
    async fn session(&self, user_id: Uuid, project: Option<Uuid>) -> AppResult<Option<SessionInfo>> {
        let Some(user) = self.repo.user(user_id).await? else { return Ok(None) };
        let mine = self.repo.projects_of(user_id).await?;
        let Some((p, me)) = project.and_then(|id| mine.iter().find(|(p, _)| p.id == id)).or_else(|| mine.first()).cloned() else {
            return Ok(None);
        };
        let members = self
            .repo
            .members(p.id)
            .await?
            .into_iter()
            .map(|(m, u)| MemberRef { user_id: u.id, name: u.display_name, role: m.role })
            .collect();
        Ok(Some(SessionInfo {
            user_name: user.display_name,
            user_lang: user.lang,
            theme: user.theme,
            project_id: p.id,
            project_name: p.name.clone(),
            base_currency: p.base_currency.clone(),
            // Символы валют — справочник `currencies`; заполняет связка в app/wiring.
            currency_marks: Default::default(),
            role: me.role,
            require_2fa: p.require_2fa,
            projects: mine.into_iter().map(|(p, m)| ProjectRef { id: p.id, name: p.name, role: m.role }).collect(),
            members,
        }))
    }
}

#[async_trait]
impl Authenticator for ProjectService {
    async fn authenticate(&self, credential: &Credential) -> AppResult<Option<Identity>> {
        let now = Utc::now();
        match credential {
            Credential::Session(raw) => {
                let hash = sha256_hex(raw);
                let Some(s) = self.access.session(&hash).await? else { return Ok(None) };
                if !s.active(now) {
                    return Ok(None);
                }
                if s.needs_touch(now) {
                    self.access.touch_session(&hash, now).await?;
                }
                Ok(Some(Identity { user_id: s.user_id, mfa: s.mfa, via: Via::Session { id_hash: hash } }))
            }
            Credential::ApiToken(raw) => {
                if !raw.starts_with(API_TOKEN_PREFIX) {
                    return Ok(None);
                }
                let Some(t) = self.access.token_by_hash(&sha256_hex(raw)).await? else { return Ok(None) };
                if !t.active(now) {
                    return Ok(None);
                }
                if t.last_used_at.is_none_or(|at| now - at >= Duration::minutes(1)) {
                    self.access.touch_token(t.id, now).await?;
                }
                // Второй фактор — тот, что был у сессии при выпуске: если проект позже потребовал 2FA,
                // токен без него перестаёт открывать данные (как и такая сессия).
                Ok(Some(Identity { user_id: t.user_id, mfa: t.mfa, via: Via::Token { token_id: t.id, project_id: t.project_id, scopes: t.scopes } }))
            }
        }
    }
}
