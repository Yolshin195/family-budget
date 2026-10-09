//! @generated rust-hex-api: связка фичи `limits` с фичами, от которых она зависит.
//! Адаптеры реализуют порты `limits` через публичный API поставщиков.

use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

pub struct CategoriesLookupAdapter(pub Arc<::categories::Service>);

#[async_trait]
impl ::limits::CategoriesLookup for CategoriesLookupAdapter {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        self.0.exists(project_id, code).await
    }

    async fn choices(&self, project_id: Uuid, lang: kernel::i18n::Lang) -> AppResult<Vec<kernel::web::Choice>> {
        self.0.choices(project_id, lang).await
    }
}
