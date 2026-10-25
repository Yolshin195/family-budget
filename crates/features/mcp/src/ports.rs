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

use crate::domain::{
    Account, BankBalance, BudgetReport, Category, Currency, Grant, Limit, Occurrence, Person, Place, Plan, PlanDraft,
    QuickInfo, Rate, Tx, TxDraft, TxFilter,
};

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
    async fn quick(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        base_currency: &str,
        lang: Lang,
        date: NaiveDate,
    ) -> AppResult<QuickInfo>;
    /// Страница операций (новые сначала) и общее число.
    async fn list(&self, project_id: Uuid, filter: TxFilter, limit: i64, offset: i64) -> AppResult<(Vec<Tx>, i64)>;
    async fn get(&self, project_id: Uuid, id: Uuid) -> AppResult<Tx>;
    async fn create(&self, project_id: Uuid, user_id: Uuid, draft: TxDraft) -> AppResult<Tx>;
    async fn update(&self, project_id: Uuid, id: Uuid, draft: TxDraft) -> AppResult<Tx>;
    async fn balances(&self, project_id: Uuid, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>>;
    /// Остаток счёта по данным банка и остаток по операциям на тот же момент; `None` — SMS с остатком не было.
    async fn bank_balance(&self, _project_id: Uuid, _account_code: &str) -> AppResult<Option<BankBalance>> {
        Ok(None)
    }
}

/// Бюджет за период (фича `budget`).
#[async_trait]
pub trait Budget: Send + Sync {
    /// `view` — week / month / year; период, содержащий `date`.
    async fn report(
        &self,
        project_id: Uuid,
        base_currency: &str,
        view: &str,
        date: NaiveDate,
        lang: Lang,
    ) -> AppResult<BudgetReport>;
}

/// Плановые операции и их платежи (фича `planning`). Удаления нет.
#[async_trait]
pub trait Plans: Send + Sync {
    async fn occurrences(
        &self,
        project_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
        today: NaiveDate,
        lang: Lang,
    ) -> AppResult<Vec<Occurrence>>;
    async fn skip(&self, project_id: Uuid, user_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<()>;
    /// Все планы проекта (и приостановленные).
    async fn list(&self, project_id: Uuid) -> AppResult<Vec<Plan>>;
    async fn create(&self, project_id: Uuid, user_id: Uuid, draft: &PlanDraft) -> AppResult<Plan>;
    /// Изменить план целиком (в том числе приостановить: `active = false`).
    async fn update(&self, project_id: Uuid, id: Uuid, draft: &PlanDraft) -> AppResult<Plan>;
}

/// Справочники: валюты, счета, категории (фичи `currencies`, `accounts`, `categories`). Ни удаления, ни
/// архива: при изменении записи адаптер оставляет её текущий признак архива, что бы ни пришло.
#[async_trait]
pub trait Directories: Send + Sync {
    /// Все валюты проекта, с архивными.
    async fn currencies(&self, project_id: Uuid) -> AppResult<Vec<Currency>>;
    async fn create_currency(&self, project_id: Uuid, currency: &Currency) -> AppResult<Currency>;
    async fn update_currency(&self, project_id: Uuid, currency: &Currency) -> AppResult<Currency>;
    /// Все счета проекта, с архивными.
    async fn accounts(&self, project_id: Uuid) -> AppResult<Vec<Account>>;
    async fn create_account(&self, project_id: Uuid, account: &Account) -> AppResult<Account>;
    async fn update_account(&self, project_id: Uuid, account: &Account) -> AppResult<Account>;
    /// Начальный остаток счёта на дату (операция OPENING в журнале); ноль — убрать.
    async fn set_opening(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        account_code: &str,
        amount: Decimal,
        date: NaiveDate,
    ) -> AppResult<()>;
    /// Все категории проекта, с архивными.
    async fn categories(&self, project_id: Uuid) -> AppResult<Vec<Category>>;
    async fn create_category(&self, project_id: Uuid, category: &Category) -> AppResult<Category>;
    async fn update_category(&self, project_id: Uuid, category: &Category) -> AppResult<Category>;
    /// Все места проекта, с архивными.
    async fn places(&self, project_id: Uuid) -> AppResult<Vec<Place>>;
    async fn create_place(&self, project_id: Uuid, place: &Place) -> AppResult<Place>;
    async fn update_place(&self, project_id: Uuid, place: &Place) -> AppResult<Place>;
    /// Все люди проекта, с архивными.
    async fn people(&self, project_id: Uuid) -> AppResult<Vec<Person>>;
    /// Человек без аккаунта (участников добавляет само приложение).
    async fn create_person(&self, project_id: Uuid, person: &Person) -> AppResult<Person>;
    /// Имя и другие имена.
    async fn update_person(&self, project_id: Uuid, person: &Person) -> AppResult<Person>;
}

/// Курсы валют (фича `rates`). Удаления нет.
#[async_trait]
pub trait Rates: Send + Sync {
    /// Курсы за период (границы включительно, `None` — без границы), новые сначала.
    async fn list(&self, project_id: Uuid, from: Option<NaiveDate>, to: Option<NaiveDate>) -> AppResult<Vec<Rate>>;
    async fn create(&self, project_id: Uuid, rate: &Rate) -> AppResult<Rate>;
    async fn update(&self, project_id: Uuid, rate: &Rate) -> AppResult<Rate>;
}

/// Правила лимитов (фича `limits`). Удаления нет.
#[async_trait]
pub trait Limits: Send + Sync {
    async fn list(&self, project_id: Uuid) -> AppResult<Vec<Limit>>;
    async fn create(&self, project_id: Uuid, limit: &Limit) -> AppResult<Limit>;
    async fn update(&self, project_id: Uuid, limit: &Limit) -> AppResult<Limit>;
}

/// Пользователи и журнал (фича `projects`): только проверка токенов и запись событий.
#[async_trait]
pub trait Accounts: Send + Sync {
    /// (id пользователя, OAuth-клиент) по access-токену провайдера входа для ресурса `audience`.
    async fn verify_access_token(&self, token: &str, audience: &str) -> AppResult<Option<(Uuid, String)>>;
    /// Издатель access-токенов провайдера (для метаданных ресурса). `None` — вход не настроен.
    async fn issuer(&self) -> Option<String>;
    async fn is_guest(&self, user_id: Uuid) -> AppResult<bool>;
    async fn record(
        &self,
        user_id: Uuid,
        project_id: Option<Uuid>,
        action: &str,
        meta: &RequestMeta,
        details: &[(&str, String)],
    );
}

/// Зависимости фичи от других фич (порты объявлены здесь, реализации — в app/wiring).
pub struct Deps {
    pub ledger: Arc<dyn Ledger>,
    pub budget: Arc<dyn Budget>,
    pub plans: Arc<dyn Plans>,
    pub directories: Arc<dyn Directories>,
    pub rates: Arc<dyn Rates>,
    pub limits: Arc<dyn Limits>,
    pub accounts: Arc<dyn Accounts>,
    /// Проверка личных API-токенов (`fbt_…`) — порт ядра.
    pub authenticator: Arc<dyn Authenticator>,
    /// Роль, базовая валюта, язык в проекте — порт ядра.
    pub memberships: Arc<dyn Memberships>,
}
