//! Исходящие порты фичи «Планирование».

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::Plan;

#[async_trait]
pub trait PlanRepository: Send + Sync {
    async fn insert(&self, plan: &Plan) -> AppResult<()>;
    async fn update(&self, plan: &Plan) -> AppResult<()>;
    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Plan>>;
    async fn list(&self, project_id: Uuid) -> AppResult<Vec<Plan>>;
    /// `true`, если план был удалён (пропуски удаляются вместе с ним).
    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool>;
    /// Пропуски (plan_id, plan_date) в периоде.
    async fn skips(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<(Uuid, NaiveDate)>>;
    async fn add_skip(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate, by: Uuid) -> AppResult<()>;
    async fn remove_skip(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<()>;
    /// Удалить все планы и пропуски проекта (удаление проекта или аккаунта).
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
}

/// Оплата вхождения плана (её хранит транзакция).
#[derive(Debug, Clone)]
pub struct PaidRef {
    pub plan_id: Uuid,
    pub plan_date: NaiveDate,
    pub transaction_id: Uuid,
    pub amount: Decimal,
    /// Валюта `amount` (счёт оплаты, у перевода — счёт списания).
    pub currency: String,
    pub paid_on: NaiveDate,
}

/// Что нужно от фичи `transactions`.
#[async_trait]
pub trait PaymentsLookup: Send + Sync {
    /// Оплаченные вхождения с датой вхождения в периоде.
    async fn paid(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<PaidRef>>;
}

/// Что нужно от справочника `accounts`.
#[async_trait]
pub trait AccountsLookup: Send + Sync {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>>;
    /// Валюты счетов: (код счёта, код валюты), включая архивные.
    async fn currencies(&self, project_id: Uuid) -> AppResult<Vec<(String, String)>>;
}

/// Что нужно от справочника `rates`.
#[async_trait]
pub trait RatesLookup: Send + Sync {
    /// Все курсы проекта: (валюта, дата, курс к базовой).
    async fn all(&self, project_id: Uuid) -> AppResult<Vec<(String, NaiveDate, Decimal)>>;
}

/// Что нужно от справочника `categories`.
#[async_trait]
pub trait CategoriesLookup: Send + Sync {
    /// Тип активной категории (EXPENSE / INCOME) или `None`.
    async fn kind_of(&self, project_id: Uuid, code: &str) -> AppResult<Option<String>>;
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>>;
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>>;
}

/// Зависимости фичи. Реализации — crates/app/src/wiring/wire_planning.rs.
pub struct Deps {
    pub payments: Arc<dyn PaymentsLookup>,
    pub accounts: Arc<dyn AccountsLookup>,
    pub categories: Arc<dyn CategoriesLookup>,
    pub rates: Arc<dyn RatesLookup>,
}
