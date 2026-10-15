//! Связка фичи `accounts` с фичами, от которых она зависит: валюты и журнал операций.
//! Адаптеры реализуют порты `accounts` через публичный API поставщиков.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

pub struct CurrenciesLookupAdapter(pub Arc<::currencies::Service>);

#[async_trait]
impl ::accounts::CurrenciesLookup for CurrenciesLookupAdapter {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        self.0.exists(project_id, code).await
    }

    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.0.choices(project_id, lang).await
    }
}

/// Журнал операций для счетов: остатки и начальный остаток. Фича `transactions` сама зависит от
/// счетов, поэтому подключается позже — через `OnceLock`.
#[derive(Clone, Default)]
pub struct LedgerAdapter(pub Arc<OnceLock<Arc<dyn LedgerSource>>>);

/// То, что адаптеру нужно от журнала (без фичи `transactions` — остатков нет).
#[async_trait]
pub trait LedgerSource: Send + Sync {
    async fn balances(&self, project_id: Uuid, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>>;
    async fn opening(&self, project_id: Uuid, code: &str) -> AppResult<Option<(Decimal, NaiveDate)>>;
    async fn set_opening(&self, project_id: Uuid, user_id: Uuid, code: &str, amount: Decimal, date: NaiveDate) -> AppResult<()>;
    async fn has_postings(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
}

#[cfg(feature = "transactions")]
#[async_trait]
impl LedgerSource for ::transactions::Service {
    async fn balances(&self, project_id: Uuid, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        ::transactions::Service::balances(self, project_id, date).await
    }
    async fn opening(&self, project_id: Uuid, code: &str) -> AppResult<Option<(Decimal, NaiveDate)>> {
        ::transactions::Service::opening(self, project_id, code).await
    }
    async fn set_opening(&self, project_id: Uuid, user_id: Uuid, code: &str, amount: Decimal, date: NaiveDate) -> AppResult<()> {
        ::transactions::Service::set_opening(self, project_id, user_id, code, amount, date).await
    }
    async fn has_postings(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        ::transactions::Service::has_postings(self, project_id, code).await
    }
}

#[async_trait]
impl ::accounts::Ledger for LedgerAdapter {
    async fn balances(&self, project_id: Uuid, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        match self.0.get() {
            Some(l) => l.balances(project_id, date).await,
            None => Ok(Vec::new()),
        }
    }
    async fn opening(&self, project_id: Uuid, code: &str) -> AppResult<Option<(Decimal, NaiveDate)>> {
        match self.0.get() {
            Some(l) => l.opening(project_id, code).await,
            None => Ok(None),
        }
    }
    async fn set_opening(&self, project_id: Uuid, user_id: Uuid, code: &str, amount: Decimal, date: NaiveDate) -> AppResult<()> {
        match self.0.get() {
            Some(l) => l.set_opening(project_id, user_id, code, amount, date).await,
            None if amount.is_zero() => Ok(()),
            None => Err(kernel::error::AppError::Validation("opening_balance: журнал операций выключен — остаток не задать".into())),
        }
    }
    async fn has_postings(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        match self.0.get() {
            Some(l) => l.has_postings(project_id, code).await,
            None => Ok(false),
        }
    }
}
