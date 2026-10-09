//! @generated rust-hex-api: связка фичи `limits` с фичами, от которых она зависит.
//! Адаптеры реализуют порты `limits` через публичный API поставщиков.

use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

pub struct CategoriesLookupAdapter(pub Arc<::categories::Service>);

#[async_trait]
impl ::limits::CategoriesLookup for CategoriesLookupAdapter {
    async fn exists(&self, user_id: Uuid, code: &str) -> AppResult<bool> {
        self.0.exists(user_id, code).await
    }
}
