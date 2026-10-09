//! @generated rust-hex-api: связка фичи `rates` с фичами, от которых она зависит.
//! Адаптеры реализуют порты `rates` через публичный API поставщиков.

use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

pub struct CurrenciesLookupAdapter(pub Arc<::currencies::Service>);

#[async_trait]
impl ::rates::CurrenciesLookup for CurrenciesLookupAdapter {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        self.0.exists(project_id, code).await
    }

    async fn choices(&self, project_id: Uuid, lang: kernel::i18n::Lang) -> AppResult<Vec<kernel::web::Choice>> {
        self.0.choices(project_id, lang).await
    }
}
