//! Исходящие порты фичи «Транзакции»: хранилище и то, что нужно от других фич.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::Transaction;

/// Фильтр списка: период по `occurred_on` (включительно) + необязательные условия.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub account_code: Option<String>,
    pub created_by: Option<Uuid>,
    /// Только с описанием без перевода на этот язык.
    pub untranslated: Option<Lang>,
}

/// Позиция в развёрнутом виде — для отчётов (бюджет).
#[derive(Debug, Clone, PartialEq)]
pub struct FactLine {
    pub transaction_id: Uuid,
    pub occurred_on: NaiveDate,
    pub account_code: String,
    /// EXPENSE / INCOME
    pub tx_kind: String,
    pub category_code: String,
    pub amount: Decimal,
    pub created_by: Uuid,
}

/// Оплаченное вхождение плана.
#[derive(Debug, Clone, PartialEq)]
pub struct Payment {
    pub plan_id: Uuid,
    pub plan_date: NaiveDate,
    pub transaction_id: Uuid,
    pub occurred_on: NaiveDate,
    pub amount: Decimal,
}

#[async_trait]
pub trait TransactionRepository: Send + Sync {
    /// Шапка + позиции в одной SQL-транзакции.
    async fn insert(&self, item: &Transaction) -> AppResult<()>;
    /// Шапка + замена набора позиций в одной SQL-транзакции.
    async fn update(&self, item: &Transaction) -> AppResult<()>;
    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Transaction>>;
    /// Новые сначала.
    async fn list(&self, project_id: Uuid, filter: &Filter, limit: i64, offset: i64) -> AppResult<Vec<Transaction>>;
    async fn count(&self, project_id: Uuid, filter: &Filter) -> AppResult<i64>;
    /// `true`, если запись была удалена.
    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool>;
    /// Все позиции за период (для бюджета).
    async fn lines(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<FactLine>>;
    /// Оплаченные вхождения планов с `plan_date` в периоде.
    async fn payments(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<Payment>>;
    /// Обороты по счетам по дату включительно: (счёт, доходы, расходы).
    async fn turnover(&self, project_id: Uuid, to: NaiveDate) -> AppResult<Vec<(String, Decimal, Decimal)>>;
}

/// Зависимости фичи от других фич. Реализации — crates/app/src/wiring/wire_transactions.rs.
pub struct Deps {
    pub accounts: Arc<dyn AccountsLookup>,
    pub categories: Arc<dyn CategoriesLookup>,
}

/// Что этой фиче нужно от справочника `accounts`.
#[async_trait]
pub trait AccountsLookup: Send + Sync {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
    /// Активные счета; `tag` — валюта счёта.
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>>;
    /// Названия всех счетов, включая архивные: (код, название).
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>>;
}

/// Что этой фиче нужно от справочника `categories`.
#[async_trait]
pub trait CategoriesLookup: Send + Sync {
    /// Тип активной категории (EXPENSE / INCOME) или `None`, если её нет или она в архиве.
    async fn kind_of(&self, project_id: Uuid, code: &str) -> AppResult<Option<String>>;
    /// Активные категории деревом; `tag` — тип.
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>>;
    /// Названия всех категорий, включая архивные: (код, название).
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>>;
}
