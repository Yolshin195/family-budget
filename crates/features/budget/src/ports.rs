//! Исходящие порты фичи «Бюджет». Своих таблиц нет — всё читается из других фич через порты
//! (реализации — crates/app/src/wiring/wire_budget.rs).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use uuid::Uuid;

pub use crate::domain::{LimitRule, Node};

/// Позиция транзакции (в валюте счёта).
#[derive(Debug, Clone)]
pub struct FactRec {
    pub date: NaiveDate,
    pub account_code: String,
    /// EXPENSE / INCOME
    pub kind: String,
    pub category_code: String,
    pub amount: Decimal,
    pub created_by: Uuid,
}

/// Нерешённое вхождение плана (в валюте счёта).
#[derive(Debug, Clone)]
pub struct PlannedRec {
    pub plan_id: Uuid,
    pub date: NaiveDate,
    pub kind: String,
    pub account_code: String,
    pub category_code: String,
    pub amount: Decimal,
    pub title: String,
    /// EXPECTED / OVERDUE
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct AccountRec {
    pub code: String,
    pub title: String,
    pub currency: String,
    pub opening: Decimal,
    pub archived: bool,
}

#[async_trait]
pub trait CategoriesSource: Send + Sync {
    async fn nodes(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Node>>;
}

#[async_trait]
pub trait FactsSource: Send + Sync {
    async fn lines(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<FactRec>>;
    /// (счёт, доходы, расходы) по дату включительно.
    async fn turnover(&self, project_id: Uuid, to: NaiveDate) -> AppResult<Vec<(String, Decimal, Decimal)>>;
}

#[async_trait]
pub trait PlansSource: Send + Sync {
    async fn open(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate, today: NaiveDate, lang: Lang) -> AppResult<Vec<PlannedRec>>;
}

#[async_trait]
pub trait LimitsSource: Send + Sync {
    async fn rules(&self, project_id: Uuid) -> AppResult<Vec<LimitRule>>;
}

#[async_trait]
pub trait AccountsSource: Send + Sync {
    async fn all(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<AccountRec>>;
}

#[async_trait]
pub trait RatesSource: Send + Sync {
    /// (валюта, дата, курс к базовой).
    async fn all(&self, project_id: Uuid) -> AppResult<Vec<(String, NaiveDate, Decimal)>>;
}

pub struct Deps {
    pub categories: Arc<dyn CategoriesSource>,
    pub facts: Arc<dyn FactsSource>,
    pub plans: Arc<dyn PlansSource>,
    pub limits: Arc<dyn LimitsSource>,
    pub accounts: Arc<dyn AccountsSource>,
    pub rates: Arc<dyn RatesSource>,
}
