//! @generated rust-hex-api: связка фичи `accounts` с фичами, от которых она зависит.
//! Адаптеры реализуют порты `accounts` через публичный API поставщиков.

use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

pub struct CurrenciesLookupAdapter(pub Arc<::currencies::Service>);

#[async_trait]
impl ::accounts::CurrenciesLookup for CurrenciesLookupAdapter {
    async fn exists(&self, user_id: Uuid, code: &str) -> AppResult<bool> {
        self.0.exists(user_id, code).await
    }
}
