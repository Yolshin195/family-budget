//! Use cases фичи «Проекты»: регистрация и вход, проекты, участники. Реализует порт ядра
//! `Memberships` — по нему middleware определяет активный проект и роль.

use std::sync::Arc;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use async_trait::async_trait;
use kernel::auth::{MemberRef, Memberships, ProjectRef, Role, SessionInfo, Tokens};
use kernel::code::require_text;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use uuid::Uuid;

use crate::domain::{Member, Project, ProjectKind, User, check_members, check_password, normalize_email};
use crate::ports::{Deps, ProjectRepository};

pub struct ProjectService {
    repo: Arc<dyn ProjectRepository>,
    deps: Deps,
    tokens: Tokens,
}

/// Проект для страницы «Проекты»: роль текущего пользователя и участники.
#[derive(Debug, Clone)]
pub struct ProjectView {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub base_currency: String,
    pub role: Role,
    pub members: Vec<MemberView>,
}

#[derive(Debug, Clone)]
pub struct MemberView {
    pub user_id: Uuid,
    pub name: String,
    pub email: String,
    pub role: Role,
}

impl ProjectService {
    pub fn new(repo: Arc<dyn ProjectRepository>, deps: Deps, tokens: Tokens) -> Self {
        Self { repo, deps, tokens }
    }

    /// Регистрация: пользователь + его личный проект с базовой валютой и стартовыми справочниками.
    pub async fn register(&self, email: &str, name: &str, password: &str, lang: Lang, base_currency: &str) -> AppResult<Uuid> {
        let v = AppError::Validation;
        let email = normalize_email(email).map_err(v)?;
        check_password(password).map_err(v)?;
        if self.repo.user_by_email(&email).await?.is_some() {
            return Err(AppError::Conflict(format!("email '{email}' уже зарегистрирован")));
        }
        let user = User { id: Uuid::new_v4(), email, display_name: require_text("name", name).map_err(v)?, password_hash: hash(password)?, lang };
        self.repo.insert_user(&user).await?;
        let personal = match lang {
            Lang::Ru => "Личный бюджет",
            Lang::En => "Personal budget",
            Lang::Th => "งบส่วนตัว",
        };
        self.create_project(user.id, personal, ProjectKind::Personal, base_currency).await?;
        Ok(user.id)
    }

    /// Вход по email и паролю. Ошибка одна и та же для неизвестного email и неверного пароля.
    pub async fn login(&self, email: &str, password: &str) -> AppResult<Uuid> {
        let denied = || AppError::Validation("неверный email или пароль".into());
        let email = normalize_email(email).map_err(|_| denied())?;
        let user = self.repo.user_by_email(&email).await?.ok_or_else(denied)?;
        let parsed = PasswordHash::new(&user.password_hash).map_err(|e| AppError::Internal(anyhow::anyhow!("hash: {e}")))?;
        Argon2::default().verify_password(password.as_bytes(), &parsed).map_err(|_| denied())?;
        Ok(user.id)
    }

    pub fn issue_token(&self, user_id: Uuid) -> String {
        self.tokens.issue(user_id)
    }

    pub fn verify(&self, token: &str) -> Option<Uuid> {
        self.tokens.verify(token)
    }

    pub async fn set_lang(&self, user_id: Uuid, lang: Lang) -> AppResult<()> {
        self.repo.update_user_lang(user_id, lang).await
    }

    /// Новый проект; создатель — владелец. Справочники наполняются через порт `ProjectBootstrap`.
    pub async fn create_project(&self, user_id: Uuid, name: &str, kind: ProjectKind, base_currency: &str) -> AppResult<Uuid> {
        let user = self.repo.user(user_id).await?.ok_or_else(|| AppError::NotFound("пользователь не найден".into()))?;
        let project = Project::new(name, kind, base_currency, user_id).map_err(AppError::Validation)?;
        let owner = Member { project_id: project.id, user_id, role: Role::Owner };
        self.repo.insert_project(&project, &owner).await?;
        self.deps.bootstrap.init(project.id, &project.base_currency, user.lang).await?;
        Ok(project.id)
    }

    pub async fn rename(&self, user_id: Uuid, project_id: Uuid, name: &str) -> AppResult<()> {
        self.require_owner(user_id, project_id).await?;
        self.repo.rename_project(project_id, &require_text("name", name).map_err(AppError::Validation)?).await
    }

    /// Проекты пользователя с участниками.
    pub async fn overview(&self, user_id: Uuid) -> AppResult<Vec<ProjectView>> {
        let mut out = Vec::new();
        for (p, me) in self.repo.projects_of(user_id).await? {
            let members = self
                .repo
                .members(p.id)
                .await?
                .into_iter()
                .map(|(m, u)| MemberView { user_id: u.id, name: u.display_name, email: u.email, role: m.role })
                .collect();
            out.push(ProjectView { id: p.id, name: p.name, kind: p.kind.as_str().into(), base_currency: p.base_currency, role: me.role, members });
        }
        Ok(out)
    }

    /// Добавить участника по email (пользователь должен быть зарегистрирован). Только владелец.
    pub async fn add_member(&self, by: Uuid, project_id: Uuid, email: &str, role: Role) -> AppResult<()> {
        let project = self.require_owner(by, project_id).await?;
        let email = normalize_email(email).map_err(AppError::Validation)?;
        let user = self
            .repo
            .user_by_email(&email)
            .await?
            .ok_or_else(|| AppError::Validation(format!("email: пользователь '{email}' не зарегистрирован")))?;
        let new = Member { project_id, user_id: user.id, role };
        self.change_members(&project, |ms| {
            ms.retain(|m| m.user_id != user.id);
            ms.push(new.clone());
        })
        .await?;
        self.repo.upsert_member(&new).await
    }

    pub async fn set_role(&self, by: Uuid, project_id: Uuid, user_id: Uuid, role: Role) -> AppResult<()> {
        let project = self.require_owner(by, project_id).await?;
        let changed = Member { project_id, user_id, role };
        self.change_members(&project, |ms| {
            for m in ms.iter_mut().filter(|m| m.user_id == user_id) {
                m.role = role;
            }
        })
        .await?;
        self.repo.upsert_member(&changed).await
    }

    /// Удалить участника (владелец) или выйти самому.
    pub async fn remove_member(&self, by: Uuid, project_id: Uuid, user_id: Uuid) -> AppResult<()> {
        let project = if by == user_id { self.load_project(project_id).await? } else { self.require_owner(by, project_id).await? };
        self.change_members(&project, |ms| ms.retain(|m| m.user_id != user_id)).await?;
        self.repo.remove_member(project_id, user_id).await
    }

    async fn change_members(&self, project: &Project, f: impl FnOnce(&mut Vec<Member>)) -> AppResult<()> {
        let mut ms: Vec<Member> = self.repo.members(project.id).await?.into_iter().map(|(m, _)| m).collect();
        f(&mut ms);
        check_members(project.kind, &ms).map_err(AppError::Validation)
    }

    async fn load_project(&self, project_id: Uuid) -> AppResult<Project> {
        self.repo.project(project_id).await?.ok_or_else(|| AppError::NotFound(format!("проект {project_id} не найден")))
    }

    async fn require_owner(&self, user_id: Uuid, project_id: Uuid) -> AppResult<Project> {
        let project = self.load_project(project_id).await?;
        let is_owner = self.repo.members(project_id).await?.iter().any(|(m, _)| m.user_id == user_id && m.role == Role::Owner);
        if !is_owner {
            return Err(AppError::Forbidden("управлять участниками может только владелец проекта".into()));
        }
        Ok(project)
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
            project_id: p.id,
            project_name: p.name.clone(),
            base_currency: p.base_currency.clone(),
            role: me.role,
            projects: mine.into_iter().map(|(p, m)| ProjectRef { id: p.id, name: p.name, role: m.role }).collect(),
            members,
        }))
    }
}

fn hash(password: &str) -> AppResult<String> {
    // 16 случайных байт uuid v4 (getrandom) — соль без отдельной зависимости на rand.
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|e| AppError::Internal(anyhow::anyhow!("salt: {e}")))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("hash: {e}")))
}
