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
    /// Для быстрого расхода: последний счёт пользователя и его частые категории (по числу позиций
    /// с `since`, чаще — первые) для операций типа `kind`.
    async fn quick_stats(&self, project_id: Uuid, user_id: Uuid, kind: &str, since: NaiveDate) -> AppResult<QuickStats>;
    /// Удалить все операции проекта (удаление проекта или аккаунта).
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
    /// Сколько пользователей внесли первую операцию за период — для воронки лендинга.
    async fn count_first(&self, from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>) -> AppResult<i64>;
}

/// Привычки пользователя в проекте — из позиций транзакций, без отдельных таблиц.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuickStats {
    pub last_account: Option<String>,
    pub frequent: Vec<String>,
}

/// Зависимости фичи от других фич. Реализации — crates/app/src/wiring/wire_transactions.rs.
pub struct Deps {
    pub accounts: Arc<dyn AccountsLookup>,
    pub categories: Arc<dyn CategoriesLookup>,
    /// Остатки лимитов и курсы для предупреждения в форме (из фичи `budget`).
    pub budget: Arc<dyn BudgetView>,
}

/// Лимит категории в периоде (суммы в базовой валюте проекта).
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryLimit {
    pub code: String,
    pub limit: Option<Decimal>,
    /// Факт + план за период.
    pub forecast: Decimal,
    /// Сколько ещё можно потратить с учётом предков.
    pub available: Option<Decimal>,
}

/// Что нужно от фичи `budget`: остатки лимитов за месяц даты и курсы к базовой валюте.
#[async_trait]
pub trait BudgetView: Send + Sync {
    async fn limits(&self, project_id: Uuid, base_currency: &str, date: NaiveDate, lang: Lang) -> AppResult<Vec<CategoryLimit>>;
    /// Курс 1 единицы валюты к базовой на дату (ближайший предыдущий).
    async fn rates(&self, project_id: Uuid, base_currency: &str, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>>;
}

/// Счёт для быстрого расхода: валюта и её символ.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountInfo {
    pub code: String,
    pub title: String,
    pub currency: String,
    pub symbol: String,
}

/// Категория для быстрого расхода: дерево и названия на всех трёх языках (поиск).
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryInfo {
    pub code: String,
    pub parent_code: Option<String>,
    /// EXPENSE / INCOME
    pub kind: String,
    pub title: String,
    pub depth: usize,
    /// Оригинал и переводы через пробел — для поиска на любом языке.
    pub names: String,
}

/// Что этой фиче нужно от справочника `accounts`.
#[async_trait]
pub trait AccountsLookup: Send + Sync {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
    /// Активные счета; `tag` — валюта счёта.
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>>;
    /// Названия всех счетов, включая архивные: (код, название).
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>>;
    /// Активные счета с валютой и её символом.
    async fn infos(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<AccountInfo>>;
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
    /// Активные категории деревом с названиями на всех языках.
    async fn infos(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<CategoryInfo>>;
}
