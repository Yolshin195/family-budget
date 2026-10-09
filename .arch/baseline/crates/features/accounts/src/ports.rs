//! Исходящие порты фичи «Счета»: что ей нужно от хранилища и от других фич.

#[allow(unused_imports)]
use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

use crate::domain::Account;

#[async_trait]
pub trait AccountRepository: Send + Sync {
    async fn insert(&self, item: &Account) -> AppResult<()>;
    async fn update(&self, item: &Account) -> AppResult<()>;
    async fn find_by_code(&self, user_id: Uuid, code: &str) -> AppResult<Option<Account>>;
    async fn list(&self, user_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Account>>;
    async fn count(&self, user_id: Uuid, include_archived: bool) -> AppResult<i64>;
}

/// Зависимости фичи от других фич. Порты объявляет эта фича (потребитель),
/// реализации живут в приложении: crates/app/src/wiring/wire_accounts.rs.
pub struct Deps {
    pub currencies: Arc<dyn CurrenciesLookup>,
}

/// Что этой фиче нужно от справочника `currencies`.
#[async_trait]
pub trait CurrenciesLookup: Send + Sync {
    async fn exists(&self, user_id: Uuid, code: &str) -> AppResult<bool>;
}

