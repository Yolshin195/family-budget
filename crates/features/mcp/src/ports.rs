//! Исходящие порты фичи «MCP для ассистентов». Реализации — в crates/app/src/wiring/wire_mcp.rs.
//! У фичи нет порта к правам, токенам, участникам и настройкам проектов: через MCP их не изменить.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use kernel::auth::{Authenticator, Memberships, RequestMeta};
use kernel::error::AppResult;
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::{BudgetReport, Grant, Occurrence, QuickInfo, Tx, TxDraft, TxFilter};

#[async_trait]
pub trait GrantRepository: Send + Sync {
    async fn grant(&self, user_id: Uuid, client_id: &str) -> AppResult<Option<Grant>>;
    async fn grants_of(&self, user_id: Uuid) -> AppResult<Vec<Grant>>;
    /// Создать или заменить доступ пользователя для клиента.
    async fn save(&self, grant: &Grant) -> AppResult<()>;
    /// `true` — доступ был и удалён.
    async fn delete(&self, user_id: Uuid, client_id: &str) -> AppResult<bool>;
    async fn touch(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<()>;
    /// Удалить все доступы к проекту (проект удалён).
    async fn delete_project(&self, project_id: Uuid) -> AppResult<u64>;
}

/// Операции и остатки (фича `transactions`).
#[async_trait]
pub trait Ledger: Send + Sync {
    async fn quick(&self, project_id: Uuid, user_id: Uuid, base_currency: &str, lang: Lang, date: NaiveDate) -> AppResult<QuickInfo>;
    /// Страница операций (новые сначала) и общее число.
    async fn list(&self, project_id: Uuid, filter: TxFilter, limit: i64, offset: i64) -> AppResult<(Vec<Tx>, i64)>;
    async fn get(&self, project_id: Uuid, id: Uuid) -> AppResult<Tx>;
    async fn create(&self, project_id: Uuid, user_id: Uuid, draft: TxDraft) -> AppResult<Tx>;
    async fn update(&self, project_id: Uuid, id: Uuid, draft: TxDraft) -> AppResult<Tx>;
    async fn balances(&self, project_id: Uuid, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>>;
}

/// Бюджет за период (фича `budget`).
#[async_trait]
pub trait Budget: Send + Sync {
    /// `view` — week / month / year; период, содержащий `date`.
    async fn report(&self, project_id: Uuid, base_currency: &str, view: &str, date: NaiveDate, lang: Lang) -> AppResult<BudgetReport>;
}

/// Плановые платежи (фича `planning`).
#[async_trait]
pub trait Plans: Send + Sync {
    async fn occurrences(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate, today: NaiveDate, lang: Lang) -> AppResult<Vec<Occurrence>>;
    async fn skip(&self, project_id: Uuid, user_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<()>;
}

/// Пользователи и журнал (фича `projects`): только проверка токенов и запись событий.
#[async_trait]
pub trait Accounts: Send + Sync {
    /// (id пользователя, OAuth-клиент) по access-токену провайдера входа для ресурса `audience`.
    async fn verify_access_token(&self, token: &str, audience: &str) -> AppResult<Option<(Uuid, String)>>;
    /// Издатель access-токенов провайдера (для метаданных ресурса). `None` — вход не настроен.
    async fn issuer(&self) -> Option<String>;
    async fn is_guest(&self, user_id: Uuid) -> AppResult<bool>;
    async fn record(&self, user_id: Uuid, project_id: Option<Uuid>, action: &str, meta: &RequestMeta, details: &[(&str, String)]);
}

/// Зависимости фичи от других фич (порты объявлены здесь, реализации — в app/wiring).
pub struct Deps {
    pub ledger: Arc<dyn Ledger>,
    pub budget: Arc<dyn Budget>,
    pub plans: Arc<dyn Plans>,
    pub accounts: Arc<dyn Accounts>,
    /// Проверка личных API-токенов (`fbt_…`) — порт ядра.
    pub authenticator: Arc<dyn Authenticator>,
    /// Роль, базовая валюта, язык в проекте — порт ядра.
    pub memberships: Arc<dyn Memberships>,
}
