//! Тесты use cases «Проекты»: вход через Rauthy (фейковый провайдер), сессии, API-токены, 2FA,
//! семейный проект и роли, изоляция проектов, удаление аккаунта.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use kernel::auth::{Authenticator, Credential, Memberships, RequestMeta, Role, Via};
use kernel::config::Config;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use uuid::Uuid;

use crate::adapters::memory::InMemoryProjectRepository;
use crate::domain::{PendingLogin, ProjectKind, VerifiedLogin};
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
