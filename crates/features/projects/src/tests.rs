//! Тесты use cases «Проекты»: вход через Rauthy (фейковый провайдер), сессии, API-токены, 2FA,
//! семейный проект и роли, приглашения по ссылке, изоляция проектов, удаление аккаунта.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use kernel::auth::{Authenticator, Credential, Memberships, RequestMeta, Role, Via};
use kernel::config::Config;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use uuid::Uuid;

use crate::adapters::memory::InMemoryProjectRepository;
use crate::domain::{AccessToken, InviteState, PendingLogin, ProjectKind, VerifiedLogin};
use crate::ports::{AuthRequest, Deps, IdentityProvider, ProjectBootstrap, ProjectPurge};
use crate::service::ProjectService;

#[derive(Default)]
struct FakeBootstrap(Mutex<Vec<(Uuid, String)>>);

#[async_trait]
impl ProjectBootstrap for Arc<FakeBootstrap> {
    async fn init(&self, project_id: Uuid, base_currency: &str, _lang: Lang) -> AppResult<()> {
        self.0.lock().unwrap().push((project_id, base_currency.into()));
        Ok(())
    }
}

#[derive(Default)]
struct FakePurge(Mutex<Vec<Uuid>>);

#[async_trait]
impl ProjectPurge for Arc<FakePurge> {
    async fn purge(&self, project_id: Uuid) -> AppResult<()> {
        self.0.lock().unwrap().push(project_id);
        Ok(())
    }
}

/// Rauthy понарошку: код входа = sub пользователя, nonce проверяется как настоящий.
struct FakeRauthy {
    logins: Mutex<Vec<VerifiedLogin>>,
}

#[async_trait]
impl IdentityProvider for Arc<FakeRauthy> {
    fn enabled(&self) -> bool {
        true
    }
    async fn authorize(&self, _lang: Lang) -> AppResult<AuthRequest> {
        Ok(AuthRequest { url: "https://auth.test/authorize".into(), state: "state-1".into(), nonce: "nonce-1".into(), pkce_verifier: "pkce-1".into() })
    }
    async fn exchange(&self, code: &str, pkce: &str, nonce: &str) -> AppResult<VerifiedLogin> {
        if nonce != "nonce-1" || pkce != "pkce-1" {
            return Err(AppError::Forbidden("nonce".into()));
        }
        self.logins.lock().unwrap().iter().find(|l| l.sub == code).cloned().ok_or_else(|| AppError::Forbidden("code".into()))
    }
    async fn logout_url(&self, post: &str) -> Option<String> {
        Some(format!("https://auth.test/logout?post_logout_redirect_uri={post}"))
    }
    async fn delete_user(&self, _sub: &str) -> AppResult<bool> {
        Ok(true)
    }
    async fn issuer(&self) -> Option<String> {
        Some("https://auth.test/".into())
    }
    /// Access-токен понарошку: `at:<sub>:<клиент>:<ресурс>`.
    async fn verify_access_token(&self, token: &str, audience: &str) -> AppResult<Option<AccessToken>> {
        Ok(match token.splitn(4, ':').collect::<Vec<_>>()[..] {
            ["at", sub, client, aud] if aud == audience => Some(AccessToken { sub: sub.into(), client_id: client.into() }),
            _ => None,
        })
    }
}

struct Env {
    svc: ProjectService,
    repo: Arc<InMemoryProjectRepository>,
    boot: Arc<FakeBootstrap>,
    purge: Arc<FakePurge>,
}

fn login(sub: &str, email: &str, name: &str, amr: &[&str]) -> VerifiedLogin {
    VerifiedLogin { sub: sub.into(), email: email.into(), name: Some(name.into()), picture: None, amr: amr.iter().map(|s| s.to_string()).collect() }
}

fn env() -> Env {
    let repo = Arc::new(InMemoryProjectRepository::default());
    let boot = Arc::new(FakeBootstrap::default());
    let purge = Arc::new(FakePurge::default());
    let rauthy = Arc::new(FakeRauthy {
        logins: Mutex::new(vec![
            login("sub-me", "me@demo.local", "Me", &["pwd"]),
            login("sub-anna", "anna@demo.local", "Anna", &["pwd", "otp"]),
            // Facebook-аккаунт с тем же email, что у «Me», — в Rauthy это другой пользователь (другой sub).
            login("sub-fb", "me@demo.local", "Me via Facebook", &["pwd"]),
        ]),
    });
    let deps = Deps { bootstrap: Arc::new(boot.clone()), purge: Arc::new(purge.clone()), identity: Arc::new(rauthy) };
    let svc = ProjectService::new(repo.clone(), repo.clone(), deps, Arc::new(Config::test()));
    Env { svc, repo, boot, purge }
}

fn pending() -> PendingLogin {
    PendingLogin { state: "state-1".into(), nonce: "nonce-1".into(), pkce_verifier: "pkce-1".into(), next: "/transactions".into(), started: Utc::now().timestamp() }
}

fn meta() -> RequestMeta {
    RequestMeta { ip: Some("10.0.0.1".into()), user_agent: Some("test".into()) }
}

/// Войти и пройти онбординг: (user_id, сырой id сессии, личный проект).
async fn signed_up(e: &Env, sub: &str) -> (Uuid, String, Uuid) {
    let r = e.svc.finish_login(&pending(), "state-1", sub, Lang::Ru, &meta()).await.unwrap();
    let p = e.svc.onboard(r.user_id, "Имя", "thb", Lang::Ru).await.unwrap();
    (r.user_id, r.raw_session, p)
}

#[tokio::test]
async fn first_login_creates_user_and_second_finds_the_same() {
    let e = env();
    let first = e.svc.finish_login(&pending(), "state-1", "sub-me", Lang::Ru, &meta()).await.unwrap();
    assert!(first.onboarding, "новый пользователь без проектов — на онбординг");
    let personal = e.svc.onboard(first.user_id, "Алексей", "thb", Lang::Ru).await.unwrap();
    assert_eq!(e.boot.0.lock().unwrap()[0], (personal, "THB".to_string()));
    let s = e.svc.session(first.user_id, None).await.unwrap().unwrap();
    assert_eq!((s.project_name.as_str(), s.role, s.user_name.as_str()), ("Личный бюджет", Role::Owner, "Алексей"));

    let second = e.svc.finish_login(&pending(), "state-1", "sub-me", Lang::En, &meta()).await.unwrap();
    assert_eq!(second.user_id, first.user_id);
    assert!(!second.onboarding);
    assert_eq!(second.next, "/transactions");
    assert_ne!(second.raw_session, first.raw_session, "новый id сессии при каждом входе");
}

#[tokio::test]
async fn facebook_account_with_same_email_gets_no_access() {
    let e = env();
    let (me, _, personal) = signed_up(&e, "sub-me").await;
    let fb = e.svc.finish_login(&pending(), "state-1", "sub-fb", Lang::Ru, &meta()).await.unwrap();
    assert_ne!(fb.user_id, me, "поиск только по sub, не по email");
    assert!(fb.onboarding);
    // чужой проект не выбирается даже по прямому id
    assert!(e.svc.session(fb.user_id, Some(personal)).await.unwrap().is_none());
}

#[tokio::test]
async fn wrong_state_or_expired_login_is_rejected() {
    let e = env();
    let err = e.svc.finish_login(&pending(), "forged", "sub-me", Lang::Ru, &meta()).await.unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)), "{err:?}");
    let old = PendingLogin { started: (Utc::now() - Duration::minutes(11)).timestamp(), ..pending() };
    assert!(matches!(e.svc.finish_login(&old, "state-1", "sub-me", Lang::Ru, &meta()).await.unwrap_err(), AppError::Validation(_)));
    let bad_nonce = PendingLogin { nonce: "other".into(), ..pending() };
    assert!(e.svc.finish_login(&bad_nonce, "state-1", "sub-me", Lang::Ru, &meta()).await.is_err());
}

#[tokio::test]
async fn sessions_are_hashed_revocable_and_logout_everywhere_keeps_current() {
    let e = env();
    let (me, raw, _) = signed_up(&e, "sub-me").await;
    let stored = e.repo.sessions.lock().unwrap().clone();
    assert!(stored.iter().all(|s| s.id_hash != raw), "в БД только хеш id сессии");

    let id = e.svc.authenticate(&Credential::Session(raw.clone())).await.unwrap().unwrap();
    assert_eq!(id.user_id, me);
    assert!(!id.mfa);
    let Via::Session { id_hash } = id.via else { panic!("сессия") };

    let other = e.svc.finish_login(&pending(), "state-1", "sub-me", Lang::Ru, &meta()).await.unwrap().raw_session;
    assert_eq!(e.svc.logout_everywhere(me, &id_hash, &meta()).await.unwrap(), 1);
    assert!(e.svc.authenticate(&Credential::Session(other)).await.unwrap().is_none());
    assert!(e.svc.authenticate(&Credential::Session(raw.clone())).await.unwrap().is_some(), "текущая сессия жива");

    let logout = e.svc.logout(&raw, &meta()).await.unwrap();
    assert!(logout.unwrap().contains("post_logout_redirect_uri"), "выход завершает и сессию Rauthy");
    assert!(e.svc.authenticate(&Credential::Session(raw)).await.unwrap().is_none());
    assert!(e.svc.authenticate(&Credential::Session("garbage".into())).await.unwrap().is_none());
}

#[tokio::test]
async fn idle_session_expires() {
    let e = env();
    let (_, raw, _) = signed_up(&e, "sub-me").await;
    for s in e.repo.sessions.lock().unwrap().iter_mut() {
        s.last_seen_at = Utc::now() - Duration::days(8);
    }
    assert!(e.svc.authenticate(&Credential::Session(raw)).await.unwrap().is_none());
}

#[tokio::test]
async fn api_tokens_are_shown_once_scoped_and_revocable() {
    let e = env();
    let (me, _, personal) = signed_up(&e, "sub-me").await;
    let (raw, token) = e.svc.create_token(me, personal, "bot", &["transactions:write".into()], 30, false, &meta()).await.unwrap();
    assert!(raw.starts_with("fbt_") && token.token_hash != raw);
    let id = e.svc.authenticate(&Credential::ApiToken(raw.clone())).await.unwrap().unwrap();
    assert_eq!(id.via, Via::Token { token_id: token.id, project_id: personal, scopes: vec!["transactions:write".into()] });
    assert!(e.svc.authenticate(&Credential::ApiToken("fbt_nope".into())).await.unwrap().is_none());
    assert!(e.svc.authenticate(&Credential::ApiToken("not-a-token".into())).await.unwrap().is_none());

    // токен — только на свой проект
    let (anna, _, anna_personal) = signed_up(&e, "sub-anna").await;
    assert!(matches!(e.svc.create_token(me, anna_personal, "x", &["read".into()], 30, false, &meta()).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(e.svc.create_token(me, personal, "x", &["admin".into()], 30, false, &meta()).await.unwrap_err(), AppError::Validation(_)));
    assert!(matches!(e.svc.revoke_token(anna, token.id, &meta()).await.unwrap_err(), AppError::NotFound(_)), "чужой токен не отозвать");

    e.svc.revoke_token(me, token.id, &meta()).await.unwrap();
    assert!(e.svc.authenticate(&Credential::ApiToken(raw)).await.unwrap().is_none());
    let actions: Vec<String> = e.repo.audit.lock().unwrap().iter().map(|a| a.action.clone()).collect();
    assert!(actions.contains(&"api_token_created".to_string()) && actions.contains(&"api_token_revoked".to_string()));
}

/// Б1: токен помнит, был ли второй фактор у сессии, из которой его выпустили. Включили
/// «Требовать 2FA» — токен без него закрывается так же, как такая сессия.
#[tokio::test]
async fn token_keeps_mfa_of_its_session() {
    let e = env();
    let (me, _, personal) = signed_up(&e, "sub-me").await;
    let (plain, _) = e.svc.create_token(me, personal, "bot", &["read".into()], 30, false, &meta()).await.unwrap();
    let (strong, _) = e.svc.create_token(me, personal, "bot", &["read".into()], 30, true, &meta()).await.unwrap();
    e.svc.set_require_2fa(me, personal, true, &meta()).await.unwrap();
    assert!(!e.svc.authenticate(&Credential::ApiToken(plain)).await.unwrap().unwrap().mfa, "выпущен без 2FA");
    assert!(e.svc.authenticate(&Credential::ApiToken(strong)).await.unwrap().unwrap().mfa);
}

#[tokio::test]
async fn require_2fa_is_reported_and_blocks_tokens_without_mfa() {
    let e = env();
    let (me, _, _) = signed_up(&e, "sub-me").await;
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    let (anna, anna_raw, _) = signed_up(&e, "sub-anna").await;
    e.svc.add_member(me, family, "anna@demo.local", Role::Editor, &meta()).await.unwrap();
    assert!(matches!(e.svc.set_require_2fa(anna, family, true, &meta()).await.unwrap_err(), AppError::Forbidden(_)), "только владелец");
    e.svc.set_require_2fa(me, family, true, &meta()).await.unwrap();
    assert!(e.svc.session(me, Some(family)).await.unwrap().unwrap().require_2fa);
    // У Анны вход с кодом из письма (amr otp) — второй фактор есть.
    assert!(e.svc.authenticate(&Credential::Session(anna_raw)).await.unwrap().unwrap().mfa);
    assert!(matches!(e.svc.create_token(me, family, "bot", &["read".into()], 30, false, &meta()).await.unwrap_err(), AppError::Forbidden(_)));
    assert!(e.svc.create_token(me, family, "bot", &["read".into()], 30, true, &meta()).await.is_ok());
}

/// Б2: смена роли не добавляет в проект того, кто в нём не состоит, — ни зарегистрированного
/// пользователя, ни гостя демо.
#[tokio::test]
async fn set_role_does_not_add_strangers() {
    let e = env();
    let (me, _, _) = signed_up(&e, "sub-me").await;
    let (anna, _, _) = signed_up(&e, "sub-anna").await;
    let guest = e.svc.ensure_user(crate::service::GUEST_SUB, "guest@demo.local", "Guest", Lang::En).await.unwrap();
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    for stranger in [anna, guest] {
        let err = e.svc.set_role(me, family, stranger, Role::Editor, &meta()).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");
    }
    let members: Vec<Uuid> = e.svc.session(me, Some(family)).await.unwrap().unwrap().members.iter().map(|m| m.user_id).collect();
    assert_eq!(members, vec![me], "в проекте по-прежнему только владелец");
    assert!(e.svc.session(anna, Some(family)).await.unwrap().unwrap().project_id != family, "Анне проект не открылся");
}

#[tokio::test]
async fn family_project_members_and_roles() {
    let e = env();
    let (me, _, personal) = signed_up(&e, "sub-me").await;
    let (anna, _, _) = signed_up(&e, "sub-anna").await;
    // в личный проект второго участника не добавить
    assert!(matches!(e.svc.add_member(me, personal, "anna@demo.local", Role::Editor, &meta()).await.unwrap_err(), AppError::Validation(_)));
    let family = e.svc.create_project(me, "Family", ProjectKind::Family, "THB", &meta()).await.unwrap();
    e.svc.add_member(me, family, "anna@demo.local", Role::Viewer, &meta()).await.unwrap();
    // Anna видит семейный проект с ролью VIEWER; чужой личный проект ей не доступен
    let s = e.svc.session(anna, Some(family)).await.unwrap().unwrap();
    assert_eq!((s.project_id, s.role, s.members.len()), (family, Role::Viewer, 2));
    assert_ne!(e.svc.session(anna, Some(personal)).await.unwrap().unwrap().project_id, personal);
    // только владелец управляет участниками; последнего владельца не убрать
    assert!(matches!(e.svc.set_role(anna, family, anna, Role::Owner, &meta()).await.unwrap_err(), AppError::Forbidden(_)));
    assert!(matches!(e.svc.set_role(me, family, me, Role::Editor, &meta()).await.unwrap_err(), AppError::Validation(_)));
    e.svc.set_role(me, family, anna, Role::Editor, &meta()).await.unwrap();
    e.svc.remove_member(anna, family, anna, &meta()).await.unwrap();
    assert_eq!(e.svc.overview(me).await.unwrap().iter().find(|p| p.id == family).unwrap().members.len(), 1);
}

#[tokio::test]
async fn foreign_project_is_not_found() {
    let e = env();
    let (me, _, _) = signed_up(&e, "sub-me").await;
    let (anna, _, _) = signed_up(&e, "sub-anna").await;
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    for err in [
        e.svc.rename(anna, family, "x").await.unwrap_err(),
        e.svc.add_member(anna, family, "anna@demo.local", Role::Owner, &meta()).await.unwrap_err(),
        e.svc.remove_member(anna, family, me, &meta()).await.unwrap_err(),
        e.svc.delete_project(anna, family, "Семья", &meta()).await.unwrap_err(),
    ] {
        assert!(matches!(err, AppError::NotFound(_)), "чужой проект → 404, а не 403: {err:?}");
    }
}

#[tokio::test]
async fn delete_project_needs_owner_and_typed_name() {
    let e = env();
    let (me, _, personal) = signed_up(&e, "sub-me").await;
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    assert!(matches!(e.svc.delete_project(me, family, "семья?", &meta()).await.unwrap_err(), AppError::Validation(_)));
    assert!(matches!(e.svc.delete_project(me, personal, "Личный бюджет", &meta()).await.unwrap_err(), AppError::Validation(_)));
    e.svc.delete_project(me, family, "Семья", &meta()).await.unwrap();
    assert_eq!(e.purge.0.lock().unwrap().as_slice(), &[family], "данные проекта удалены во всех фичах");
}

#[tokio::test]
async fn delete_account_keeps_family_project_for_others() {
    let e = env();
    let (me, _, my_personal) = signed_up(&e, "sub-me").await;
    let (anna, _, _) = signed_up(&e, "sub-anna").await;
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    e.svc.add_member(me, family, "anna@demo.local", Role::Editor, &meta()).await.unwrap();
    // единственный владелец семейного проекта с участниками — сначала второй владелец
    assert!(matches!(e.svc.delete_account(me, &meta()).await.unwrap_err(), AppError::Validation(_)));
    e.svc.set_role(me, family, anna, Role::Owner, &meta()).await.unwrap();
    let d = e.svc.delete_account(me, &meta()).await.unwrap();
    assert!(d.rauthy, "пользователь удалён и в Rauthy");
    assert_eq!(e.purge.0.lock().unwrap().as_slice(), &[my_personal], "удалён только личный проект");
    let s = e.svc.session(anna, Some(family)).await.unwrap().unwrap();
    assert_eq!((s.project_id, s.members.len()), (family, 1), "семейный проект остался Анне");
    assert!(e.repo.audit.lock().unwrap().iter().all(|a| a.user_id != Some(me)), "журнал обезличен");
}

#[tokio::test]
async fn dev_login_only_in_development() {
    let e = env();
    signed_up(&e, "sub-me").await;
    assert!(e.svc.dev_login("me@demo.local", &meta()).await.is_ok());
    let mut prod = Config::test();
    prod.env = kernel::config::Env::Prod;
    let repo = Arc::new(InMemoryProjectRepository::default());
    let deps = Deps { bootstrap: Arc::new(Arc::new(FakeBootstrap::default())), purge: Arc::new(Arc::new(FakePurge::default())), identity: Arc::new(crate::adapters::oidc::DisabledProvider) };
    let svc = ProjectService::new(repo.clone(), repo, deps, Arc::new(prod));
    assert!(matches!(svc.dev_login("me@demo.local", &meta()).await.unwrap_err(), AppError::NotFound(_)));
}

/// Гость демо — общий аккаунт: только просмотр, ничего общего не сломать.
#[tokio::test]
async fn demo_guest_cannot_change_shared_account_or_projects() {
    let e = env();
    let (me, _, _) = signed_up(&e, "sub-me").await;
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    let guest = e.svc.ensure_user(crate::GUEST_SUB, "guest@demo.local", "Guest", Lang::En).await.unwrap();
    e.svc.add_guest(me, family).await.unwrap();
    let (raw, project) = e.svc.guest_login(&meta()).await.unwrap();
    assert_eq!(project, family);
    let Via::Session { id_hash } = e.svc.authenticate(&Credential::Session(raw)).await.unwrap().unwrap().via else { panic!() };
    let forbidden = |r: AppResult<()>| matches!(r, Err(AppError::Forbidden(_)));
    assert!(forbidden(e.svc.remove_member(guest, family, guest, &meta()).await), "выйти из демо нельзя");
    assert!(forbidden(e.svc.delete_account(guest, &meta()).await.map(|_| ())));
    assert!(forbidden(e.svc.logout_everywhere(guest, &id_hash, &meta()).await.map(|_| ())));
    assert!(forbidden(e.svc.create_project(guest, "x", ProjectKind::Family, "THB", &meta()).await.map(|_| ())));
    assert!(forbidden(e.svc.create_token(guest, family, "t", &["read".into()], 30, false, &meta()).await.map(|_| ())));
    assert!(forbidden(e.svc.update_profile(guest, "Vandal", Lang::Ru, kernel::auth::Theme::Dark).await));
    assert!(e.svc.guest_login(&meta()).await.is_ok(), "демо по-прежнему работает");
}

/// Access-токен провайдера (MCP): пользователь находится по `sub`, токен для чужого ресурса и
/// неизвестный пользователь — нет доступа.
#[tokio::test]
async fn provider_access_token_maps_to_our_user() {
    let e = env();
    let (me, _, _) = signed_up(&e, "sub-me").await;
    let resource = "https://budget.test/mcp";
    let ok = e.svc.verify_access_token(&format!("at:sub-me:claude:{resource}"), resource).await.unwrap();
    assert_eq!(ok, Some((me, "claude".to_string())));
    assert_eq!(e.svc.verify_access_token("at:sub-me:claude:https://other.test/mcp", resource).await.unwrap(), None, "чужой ресурс");
    assert_eq!(e.svc.verify_access_token(&format!("at:sub-nobody:claude:{resource}"), resource).await.unwrap(), None, "нет такого пользователя");
    assert_eq!(e.svc.token_issuer().await.as_deref(), Some("https://auth.test/"));
}

/// Код из ссылки-приглашения, которую владелец видит на странице «Проекты».
async fn invite_code(e: &Env, owner: Uuid, project: Uuid, invite: Uuid) -> String {
    let p = e.svc.overview(owner).await.unwrap().into_iter().find(|p| p.id == project).unwrap();
    let url = p.invites.into_iter().find(|i| i.id == invite).expect("приглашение в списке").url;
    url.split_once("/join/").unwrap().1.to_string()
}

#[tokio::test]
async fn invite_link_brings_a_new_user_into_the_family_budget() {
    let e = env();
    let (me, _, _) = signed_up(&e, "sub-me").await;
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    let id = e.svc.create_invite(me, family, Role::Editor, &meta()).await.unwrap();
    let code = invite_code(&e, me, family, id).await;
    // Anna впервые входит по приглашению — у неё нет проектов, онбординг ей не нужен
    let anna = e.svc.finish_login(&pending(), "state-1", "sub-anna", Lang::En, &meta()).await.unwrap().user_id;
    let preview = e.svc.invite_preview(&code, Some(anna)).await.unwrap();
    assert_eq!((preview.project_name.as_str(), preview.inviter_name.as_str(), preview.role, preview.state, preview.member), ("Семья", "Имя", Role::Editor, InviteState::Active, false));
    assert_eq!(e.svc.accept_invite(anna, &code, &meta()).await.unwrap(), family);
    let s = e.svc.session(anna, None).await.unwrap().unwrap();
    assert_eq!((s.project_id, s.role, s.members.len()), (family, Role::Editor, 2), "сразу в семейном бюджете");
    // повторно — просто проект; другому человеку ссылка уже не подходит
    assert_eq!(e.svc.accept_invite(anna, &code, &meta()).await.unwrap(), family);
    assert!(e.svc.invite_preview(&code, Some(anna)).await.unwrap().member);
    let other = e.svc.finish_login(&pending(), "state-1", "sub-fb", Lang::Ru, &meta()).await.unwrap().user_id;
    assert!(matches!(e.svc.accept_invite(other, &code, &meta()).await.unwrap_err(), AppError::Validation(_)));
    assert_eq!(e.svc.invite_preview(&code, None).await.unwrap().state, InviteState::Used);
    assert!(e.svc.overview(me).await.unwrap().iter().find(|p| p.id == family).unwrap().invites.is_empty(), "использованное пропадает из списка");
    assert!(e.repo.audit.lock().unwrap().iter().any(|a| a.action == "member_joined" && a.user_id == Some(anna)));
}

#[tokio::test]
async fn invite_code_is_signed() {
    let e = env();
    let (me, _, _) = signed_up(&e, "sub-me").await;
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    let id = e.svc.create_invite(me, family, Role::Viewer, &meta()).await.unwrap();
    let code = invite_code(&e, me, family, id).await;
    let (anna, _, _) = signed_up(&e, "sub-anna").await;
    let forged = format!("{}.{}", id.simple(), "A".repeat(22));
    let unknown = format!("{}.{}", Uuid::new_v4().simple(), code.split_once('.').unwrap().1);
    for bad in [forged.as_str(), unknown.as_str(), "nonsense", &code[..code.len() - 1]] {
        assert!(matches!(e.svc.invite_preview(bad, None).await.unwrap_err(), AppError::NotFound(_)), "{bad}");
        assert!(matches!(e.svc.accept_invite(anna, bad, &meta()).await.unwrap_err(), AppError::NotFound(_)), "{bad}");
    }
    assert!(!code.contains(&id.to_string()), "в ссылке id без дефисов");
    assert_ne!(e.svc.session(anna, Some(family)).await.unwrap().unwrap().project_id, family, "не вошла");
}

#[tokio::test]
async fn only_the_owner_invites_and_only_into_a_family_budget() {
    let e = env();
    let (me, _, personal) = signed_up(&e, "sub-me").await;
    let (anna, _, _) = signed_up(&e, "sub-anna").await;
    // личный бюджет — сначала сделать семейным; название по умолчанию меняется
    assert!(matches!(e.svc.create_invite(me, personal, Role::Editor, &meta()).await.unwrap_err(), AppError::Validation(_)));
    assert!(matches!(e.svc.make_family(anna, personal, &meta()).await.unwrap_err(), AppError::NotFound(_)), "чужой проект");
    e.svc.make_family(me, personal, &meta()).await.unwrap();
    let p = e.svc.overview(me).await.unwrap().into_iter().find(|p| p.id == personal).unwrap();
    assert_eq!((p.kind.as_str(), p.name.as_str()), ("FAMILY", "Семейный бюджет"));
    // владельцем по ссылке не стать; приглашать может только владелец
    assert!(matches!(e.svc.create_invite(me, personal, Role::Owner, &meta()).await.unwrap_err(), AppError::Validation(_)));
    let id = e.svc.create_invite(me, personal, Role::Editor, &meta()).await.unwrap();
    let code = invite_code(&e, me, personal, id).await;
    e.svc.accept_invite(anna, &code, &meta()).await.unwrap();
    assert!(matches!(e.svc.create_invite(anna, personal, Role::Viewer, &meta()).await.unwrap_err(), AppError::Forbidden(_)));
    assert!(matches!(e.svc.revoke_invite(anna, personal, id, &meta()).await.unwrap_err(), AppError::Forbidden(_)));
    assert!(e.svc.overview(anna).await.unwrap().iter().all(|p| p.invites.is_empty()), "ссылки видит только владелец");
}

#[tokio::test]
async fn revoked_expired_or_orphaned_invites_do_not_work() {
    let e = env();
    let (me, _, _) = signed_up(&e, "sub-me").await;
    let (anna, _, _) = signed_up(&e, "sub-anna").await;
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    let rejected = |r: AppResult<Uuid>| matches!(r, Err(AppError::Validation(_)));
    // отозванное
    let revoked = e.svc.create_invite(me, family, Role::Editor, &meta()).await.unwrap();
    let revoked_code = invite_code(&e, me, family, revoked).await;
    e.svc.revoke_invite(me, family, revoked, &meta()).await.unwrap();
    assert!(rejected(e.svc.accept_invite(anna, &revoked_code, &meta()).await));
    assert_eq!(e.svc.invite_preview(&revoked_code, None).await.unwrap().state, InviteState::Revoked);
    // истёкшее
    let expired = e.svc.create_invite(me, family, Role::Editor, &meta()).await.unwrap();
    let expired_code = invite_code(&e, me, family, expired).await;
    for i in e.repo.invites.lock().unwrap().iter_mut().filter(|i| i.id == expired) {
        i.expires_at = Utc::now() - Duration::minutes(1);
    }
    assert!(rejected(e.svc.accept_invite(anna, &expired_code, &meta()).await));
    assert_eq!(e.svc.invite_preview(&expired_code, None).await.unwrap().state, InviteState::Expired);
    // пригласивший больше не владелец — его ссылки не действуют
    let orphan = e.svc.create_invite(me, family, Role::Editor, &meta()).await.unwrap();
    let orphan_code = invite_code(&e, me, family, orphan).await;
    e.svc.add_member(me, family, "anna@demo.local", Role::Owner, &meta()).await.unwrap();
    e.svc.set_role(anna, family, me, Role::Editor, &meta()).await.unwrap();
    let (fb, _, _) = signed_up(&e, "sub-fb").await;
    assert!(rejected(e.svc.accept_invite(fb, &orphan_code, &meta()).await));
    assert!(e.svc.session(fb, Some(family)).await.unwrap().unwrap().project_id != family);
    // и ничего не изменилось: в проекте двое
    assert_eq!(e.svc.session(anna, Some(family)).await.unwrap().unwrap().members.len(), 2);
}

#[tokio::test]
async fn demo_guest_cannot_join_by_invite() {
    let e = env();
    let (me, _, _) = signed_up(&e, "sub-me").await;
    let family = e.svc.create_project(me, "Семья", ProjectKind::Family, "THB", &meta()).await.unwrap();
    let guest = e.svc.ensure_user(crate::GUEST_SUB, "guest@demo.local", "Guest", Lang::En).await.unwrap();
    let id = e.svc.create_invite(me, family, Role::Editor, &meta()).await.unwrap();
    let code = invite_code(&e, me, family, id).await;
    assert!(matches!(e.svc.accept_invite(guest, &code, &meta()).await.unwrap_err(), AppError::Forbidden(_)));
    assert_eq!(e.svc.invite_preview(&code, None).await.unwrap().state, InviteState::Active, "приглашение не израсходовано");
}

// ---------------------------------------------------------------- страницы входа

/// Б6: ошибка входа на странице — общим текстом; ответ Rauthy (`error_description`) и причина
/// отказа — только в журнале безопасности.
#[tokio::test]
async fn login_error_details_go_to_journal_not_to_page() {
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

    let e = env();
    let repo = e.repo.clone();
    let app = crate::adapters::web::public(Arc::new(e.svc)).layer(axum::Extension(meta()));
    let page = |uri: &'static str| {
        let app = app.clone();
        async move {
            let resp = app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
            String::from_utf8(to_bytes(resp.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap()
        }
    };
    let html = page("/auth/callback?error=server_error&error_description=upstream%20github%20token%20invalid").await;
    assert!(!html.contains("upstream github") && !html.contains("server_error"), "подробности не показываются");
    assert!(html.contains(crate::i18n::dict("login_failed_hint").unwrap().0), "общий текст ошибки");

    let audit = repo.audit.lock().unwrap().clone();
    let failed: Vec<_> = audit.iter().filter(|a| a.action == "login_failed").collect();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].user_id.is_none());
    assert!(failed[0].details.iter().any(|(k, v)| k == "description" && v.contains("upstream github")), "{:?}", failed[0].details);
}

