//! @generated rust-hex-api: связка фичи `accounts` с фичами, от которых она зависит.
//! Адаптеры реализуют порты `accounts` через публичный API поставщиков.

use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
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
