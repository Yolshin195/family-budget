//! Исходящие порты фичи «Счета»: что ей нужно от хранилища и от других фич.

#[allow(unused_imports)]
use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::Account;

#[async_trait]
pub trait AccountRepository: Send + Sync {
    async fn insert(&self, item: &Account) -> AppResult<()>;
    async fn update(&self, item: &Account) -> AppResult<()>;
    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Account>>;
    async fn list(&self, project_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Account>>;
    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64>;
    /// Удалить все записи проекта (удаление проекта или аккаунта).
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
}

/// Зависимости фичи от других фич. Порты объявляет эта фича (потребитель),
/// реализации живут в приложении: crates/app/src/wiring/wire_accounts.rs.
pub struct Deps {
    pub currencies: Arc<dyn CurrenciesLookup>,
    /// Журнал операций (фича `transactions`): остатки и начальный остаток счёта.
    pub ledger: Arc<dyn Ledger>,
}

/// Что этой фиче нужно от журнала операций. Остаток счёта — сумма его проводок, начальный
/// остаток — транзакция `OPENING` (ТЗ, разделы 3 и 4).
#[async_trait]
pub trait Ledger: Send + Sync {
    /// Остатки счетов на дату включительно: (счёт, остаток).
    async fn balances(&self, project_id: Uuid, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>>;
    /// Начальный остаток счёта и его дата.
    async fn opening(&self, project_id: Uuid, code: &str) -> AppResult<Option<(Decimal, NaiveDate)>>;
    /// Задать начальный остаток (нулевой — удалить).
    async fn set_opening(&self, project_id: Uuid, user_id: Uuid, code: &str, amount: Decimal, date: NaiveDate) -> AppResult<()>;
    /// Есть ли по счёту проводки.
    async fn has_postings(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
}

/// Что этой фиче нужно от справочника `currencies`.
#[async_trait]
pub trait CurrenciesLookup: Send + Sync {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
    /// Активные валюты для выпадающего списка на языке `lang`.
    async fn choices(&self, _project_id: Uuid, _lang: Lang) -> AppResult<Vec<Choice>> {
        Ok(Vec::new())
    }
}

