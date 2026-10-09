//! Исходящие порты фичи «Курсы валют»: что ей нужно от хранилища и от других фич.

#[allow(unused_imports)]
use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

use crate::domain::Rate;

/// Период выборки по `rate_date` (обе границы включительно).
#[derive(Debug, Clone, Copy, Default)]
pub struct Period {
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
}

#[async_trait]
pub trait RateRepository: Send + Sync {
    async fn insert(&self, item: &Rate) -> AppResult<()>;
    async fn update(&self, item: &Rate) -> AppResult<()>;
    async fn find(&self, user_id: Uuid, id: Uuid) -> AppResult<Option<Rate>>;
    async fn list(&self, user_id: Uuid, period: Period, limit: i64, offset: i64) -> AppResult<Vec<Rate>>;
    async fn count(&self, user_id: Uuid, period: Period) -> AppResult<i64>;
    /// `true`, если запись была удалена.
    async fn delete(&self, user_id: Uuid, id: Uuid) -> AppResult<bool>;
}

/// Зависимости фичи от других фич. Порты объявляет эта фича (потребитель),
/// реализации живут в приложении: crates/app/src/wiring/wire_rates.rs.
pub struct Deps {
    pub currencies: Arc<dyn CurrenciesLookup>,
}

/// Что этой фиче нужно от справочника `currencies`.
#[async_trait]
pub trait CurrenciesLookup: Send + Sync {
    async fn exists(&self, user_id: Uuid, code: &str) -> AppResult<bool>;
}

