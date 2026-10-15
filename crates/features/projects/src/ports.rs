//! Исходящие порты фичи «Проекты»: хранилища, Rauthy, наполнение и очистка данных проекта.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use kernel::auth::Theme;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use uuid::Uuid;

use crate::domain::{ApiToken, AuditEvent, Invite, Member, Project, ProjectKind, SessionRecord, User, VerifiedLogin};

/// Пользователи, проекты, участники.
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn insert_user(&self, user: &User) -> AppResult<()>;
    /// Email, имя и аватар — из ID-токена при каждом входе.
    async fn update_user_identity(&self, user_id: Uuid, email: &str, name: &str, avatar_url: Option<&str>) -> AppResult<()>;
    async fn update_user_profile(&self, user_id: Uuid, name: &str, lang: Lang, theme: Theme) -> AppResult<()>;
    async fn update_user_lang(&self, user_id: Uuid, lang: Lang) -> AppResult<()>;
    async fn user_by_sub(&self, sub: &str) -> AppResult<Option<User>>;
    /// Привязать пользователя к другому `sub` (перенос пользователей, созданных до Rauthy).
    async fn set_sub(&self, user_id: Uuid, sub: &str) -> AppResult<()>;
    /// Пользователь по email (приглашение в проект); при нескольких — последний вошедший.
    async fn user_by_email(&self, email: &str) -> AppResult<Option<User>>;
    async fn user(&self, id: Uuid) -> AppResult<Option<User>>;
    /// Удалить пользователя (сессии и токены уходят каскадом).
    async fn delete_user(&self, id: Uuid) -> AppResult<()>;
    /// Проект и его первый участник (владелец) — атомарно.
    async fn insert_project(&self, project: &Project, owner: &Member) -> AppResult<()>;
    async fn rename_project(&self, project_id: Uuid, name: &str) -> AppResult<()>;
    async fn set_require_2fa(&self, project_id: Uuid, value: bool) -> AppResult<()>;
    /// Сменить вид проекта и название (личный → семейный).
    async fn set_kind(&self, project_id: Uuid, kind: ProjectKind, name: &str) -> AppResult<()>;
    /// Удалить проект (участники и токены проекта уходят каскадом).
    async fn delete_project(&self, project_id: Uuid) -> AppResult<()>;
    async fn project(&self, id: Uuid) -> AppResult<Option<Project>>;
    /// Проекты пользователя с его ролью: личные сначала, затем по дате создания.
    async fn projects_of(&self, user_id: Uuid) -> AppResult<Vec<(Project, Member)>>;
    /// Участники проекта с пользователями.
    async fn members(&self, project_id: Uuid) -> AppResult<Vec<(Member, User)>>;
    async fn upsert_member(&self, member: &Member) -> AppResult<()>;
    async fn remove_member(&self, project_id: Uuid, user_id: Uuid) -> AppResult<()>;
    async fn insert_invite(&self, invite: &Invite) -> AppResult<()>;
    async fn invite(&self, id: Uuid) -> AppResult<Option<Invite>>;
    /// Приглашения проекта, новые сначала.
    async fn invites_of(&self, project_id: Uuid) -> AppResult<Vec<Invite>>;
    /// Отозвать неиспользованное приглашение проекта; `false` — нет такого или уже использовано.
    async fn revoke_invite(&self, project_id: Uuid, id: Uuid, at: DateTime<Utc>) -> AppResult<bool>;
    /// Принять приглашение атомарно: пометить использованным (только если оно ещё действует на `at`)
    /// и добавить участника. `false` — приглашение уже использовано, отозвано или истекло.
    async fn accept_invite(&self, id: Uuid, member: &Member, at: DateTime<Utc>) -> AppResult<bool>;
    /// Удалить приглашения, истёкшие до `before`.
    async fn purge_invites(&self, before: DateTime<Utc>) -> AppResult<u64>;
    /// Сколько пользователей зарегистрировалось за период (без демо) — для воронки лендинга.
    async fn count_signups(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> AppResult<i64>;
}

/// Сессии, API-токены, журнал безопасности.
#[async_trait]
pub trait AccessRepository: Send + Sync {
    async fn insert_session(&self, s: &SessionRecord) -> AppResult<()>;
    async fn session(&self, id_hash: &str) -> AppResult<Option<SessionRecord>>;
    async fn touch_session(&self, id_hash: &str, at: DateTime<Utc>) -> AppResult<()>;
    /// Действующие сессии пользователя, новые сначала.
    async fn sessions_of(&self, user_id: Uuid, now: DateTime<Utc>) -> AppResult<Vec<SessionRecord>>;
    async fn revoke_session(&self, user_id: Uuid, id_hash: &str, at: DateTime<Utc>) -> AppResult<bool>;
    /// Отозвать все сессии пользователя, кроме `keep` (если задана).
    async fn revoke_sessions(&self, user_id: Uuid, keep: Option<&str>, at: DateTime<Utc>) -> AppResult<u64>;
    /// Удалить истёкшие и отозванные сессии старше `before`.
    async fn purge_sessions(&self, before: DateTime<Utc>) -> AppResult<u64>;

    async fn insert_token(&self, t: &ApiToken) -> AppResult<()>;
    async fn token_by_hash(&self, token_hash: &str) -> AppResult<Option<ApiToken>>;
    async fn touch_token(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<()>;
    async fn tokens_of(&self, user_id: Uuid) -> AppResult<Vec<ApiToken>>;
    async fn revoke_token(&self, user_id: Uuid, id: Uuid, at: DateTime<Utc>) -> AppResult<bool>;
    /// Отозвать токены пользователя в проекте (вышел из проекта).
    async fn revoke_project_tokens(&self, user_id: Uuid, project_id: Uuid, at: DateTime<Utc>) -> AppResult<()>;

    async fn audit(&self, e: &AuditEvent) -> AppResult<()>;
    async fn audit_of(&self, user_id: Uuid, limit: i64) -> AppResult<Vec<AuditEvent>>;
    /// Удаление аккаунта: журнал остаётся, но без связи с человеком (user_id, IP, браузер стираются).
    async fn anonymize_audit(&self, user_id: Uuid) -> AppResult<()>;
    /// Строки старше `before` удаляются (хранение — 1 год).
    async fn purge_audit(&self, before: DateTime<Utc>) -> AppResult<u64>;
}

/// Запрос авторизации в Rauthy: куда отправить браузер и что запомнить до возврата.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

/// Rauthy (OpenID Connect): authorization code + PKCE S256, проверка ID-токена по JWKS.
#[async_trait]
pub trait IdentityProvider: Send + Sync {
    /// Включён ли вход через провайдера (настроен ли Rauthy).
    fn enabled(&self) -> bool;
    async fn authorize(&self, lang: Lang) -> AppResult<AuthRequest>;
    /// Обменять код на токены и проверить ID-токен: подпись (JWKS), iss, aud, nonce, срок.
    async fn exchange(&self, code: &str, pkce_verifier: &str, nonce: &str) -> AppResult<VerifiedLogin>;
    /// Адрес завершения сессии Rauthy (`end_session_endpoint`) с возвратом на `post_logout`.
    async fn logout_url(&self, post_logout: &str) -> Option<String>;
    /// Удалить пользователя в Rauthy (удаление аккаунта). `Ok(false)` — нет API-ключа, удалить вручную.
    async fn delete_user(&self, sub: &str) -> AppResult<bool>;
}

/// Начальное наполнение нового проекта (базовая валюта, категории) — реализуется в app/wiring
/// через публичные API фич `currencies` и `categories`.
#[async_trait]
pub trait ProjectBootstrap: Send + Sync {
    async fn init(&self, project_id: Uuid, base_currency: &str, lang: Lang) -> AppResult<()>;
}

/// Удаление всех данных проекта в других фичах (удаление проекта, аккаунта, сброс демо).
#[async_trait]
pub trait ProjectPurge: Send + Sync {
    async fn purge(&self, project_id: Uuid) -> AppResult<()>;
}

pub struct Deps {
    pub bootstrap: Arc<dyn ProjectBootstrap>,
    pub purge: Arc<dyn ProjectPurge>,
    pub identity: Arc<dyn IdentityProvider>,
}
