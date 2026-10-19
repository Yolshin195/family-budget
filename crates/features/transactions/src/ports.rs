//! Исходящие порты фичи «Транзакции»: хранилище и то, что нужно от других фич.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::{AccountCurrency, Transaction, TxKind};

/// Чем занят уникальный ключ, если хранилище вернуло `Conflict`.
pub mod taken {
    /// Вхождение плана уже оплачено другой транзакцией.
    pub const PLAN: &str = "plan";
    /// У счёта уже есть начальный остаток.
    pub const OPENING: &str = "opening";
}

/// Фильтр списка: период по `occurred_on` (включительно) + необязательные условия.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Транзакции с проводкой на этот счёт (выписка).
    pub account_code: Option<String>,
    pub kind: Option<TxKind>,
    pub created_by: Option<Uuid>,
    /// Только с описанием без перевода на этот язык.
    pub untranslated: Option<Lang>,
}

/// Проводка на категорию — факт для бюджета и лимитов.
#[derive(Debug, Clone, PartialEq)]
pub struct FactLine {
    pub transaction_id: Uuid,
    pub occurred_on: NaiveDate,
    /// Валюта проводки (= валюта счёта, с которого платили).
    pub currency: String,
    /// Тип категории: EXPENSE (в том числе комиссия перевода) или INCOME.
    pub kind: String,
    pub category_code: String,
    /// Сколько потрачено (расход) или получено (доход); возврат — отрицательная сумма.
    pub amount: Decimal,
    pub created_by: Uuid,
}

/// Проводка на системный счёт «Обмен валют» — для курсовой разницы.
#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeLine {
    /// Обе стороны обмена — проводки одной транзакции.
    pub transaction_id: Uuid,
    pub occurred_on: NaiveDate,
    pub currency: String,
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
    /// Валюта `amount` (счёт операции, у перевода — счёт списания).
    pub currency: String,
}

#[async_trait]
pub trait TransactionRepository: Send + Sync {
    /// Шапка + проводки в одной SQL-транзакции. Занятый ключ — `Conflict` с одним из [`taken`].
    async fn insert(&self, item: &Transaction) -> AppResult<()>;
    /// Шапка + замена набора проводок в одной SQL-транзакции. Занятый ключ — как у `insert`.
    async fn update(&self, item: &Transaction) -> AppResult<()>;
    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Transaction>>;
    /// Новые сначала.
    async fn list(&self, project_id: Uuid, filter: &Filter, limit: i64, offset: i64) -> AppResult<Vec<Transaction>>;
    async fn count(&self, project_id: Uuid, filter: &Filter) -> AppResult<i64>;
    /// Итоги по проводкам на категории всех операций фильтра: (валюта, доходы, расходы); возврат уменьшает итог.
    async fn category_totals(&self, project_id: Uuid, filter: &Filter) -> AppResult<Vec<(String, Decimal, Decimal)>>;
    /// Поступления и списания по счёту `account` у операций фильтра: (сумма «+», сумма «−»).
    async fn account_flows(&self, project_id: Uuid, filter: &Filter, account: &str) -> AppResult<(Decimal, Decimal)>;
    /// `true`, если запись была удалена.
    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool>;
    /// Проводки на категории за период (для бюджета).
    async fn lines(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<FactLine>>;
    /// Проводки «Обмен валют» за период.
    async fn exchange(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<ExchangeLine>>;
    /// Оплаченные вхождения планов с `plan_date` в периоде.
    async fn payments(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<Payment>>;
    /// Остатки счетов на дату включительно: (счёт, сумма проводок).
    async fn balances(&self, project_id: Uuid, to: NaiveDate) -> AppResult<Vec<(String, Decimal)>>;
    /// Есть ли у счёта хотя бы одна проводка.
    async fn has_postings(&self, project_id: Uuid, account_code: &str) -> AppResult<bool>;
    /// Транзакция начального остатка счёта.
    async fn find_opening(&self, project_id: Uuid, account_code: &str) -> AppResult<Option<Transaction>>;
    /// Есть ли проводки на категорию (тогда её тип не меняется).
    async fn uses_category(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
    /// Есть ли проводки в валюте (тогда её точность не уменьшается).
    async fn uses_currency(&self, project_id: Uuid, currency: &str) -> AppResult<bool>;
    /// Для быстрого расхода: последний счёт пользователя в расходах и доходах и его частые категории
    /// (по числу проводок с `since`, чаще — первые) для транзакций типа `kind`.
    async fn quick_stats(&self, project_id: Uuid, user_id: Uuid, kind: &str, since: NaiveDate) -> AppResult<QuickStats>;
    /// Удалить все операции проекта (удаление проекта или аккаунта).
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
    /// Сколько пользователей внесли первую операцию за период — для воронки лендинга
    /// (начальные остатки не считаются).
    async fn count_first(&self, from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>) -> AppResult<i64>;
}

/// Привычки пользователя в проекте — из проводок, без отдельных таблиц.
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
    /// Планы, вхождения которых оплачивают операции (из фичи `planning`).
    pub plans: Arc<dyn PlansLookup>,
}

/// Вхождение плана, которое оплачивает операция.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanOccurrence {
    /// Тип плана: EXPENSE / INCOME / TRANSFER.
    pub kind: String,
    /// Вхождение пропущено — оплатить его можно, только вернув в календаре.
    pub skipped: bool,
}

#[async_trait]
pub trait PlansLookup: Send + Sync {
    /// Вхождение плана проекта на дату. `None` — такого плана нет или дата не из его правила.
    async fn occurrence(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<Option<PlanOccurrence>>;
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
    /// Активный счёт с таким кодом есть.
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
    /// Валюты всех счетов, включая архивные: старые проводки остаются валидными.
    async fn currencies(&self, project_id: Uuid) -> AppResult<Vec<AccountCurrency>>;
    /// Активные счета; `tag` — валюта счёта.
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>>;
    /// Названия всех счетов, включая архивные: (код, название).
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>>;
    /// Активные счета с валютой и её символом.
    async fn infos(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<AccountInfo>>;
}

/// Категория, на которую ссылается операция.
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryRef {
    /// EXPENSE / INCOME
    pub kind: String,
    pub archived: bool,
}

/// Что этой фиче нужно от справочника `categories`.
#[async_trait]
pub trait CategoriesLookup: Send + Sync {
    /// Тип категории и признак архива (архивные — тоже: тип проверяется и у старых ссылок, техдолг #3);
    /// `None` — такой категории нет.
    async fn kind_of(&self, project_id: Uuid, code: &str) -> AppResult<Option<CategoryRef>>;
    /// Активные категории деревом; `tag` — тип.
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>>;
    /// Названия всех категорий, включая архивные: (код, название).
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>>;
    /// Активные категории деревом с названиями на всех языках.
    async fn infos(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<CategoryInfo>>;
}
