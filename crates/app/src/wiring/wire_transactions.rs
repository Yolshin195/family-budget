//! Связка фичи `transactions` с поставщиками: счета и категории.

use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use uuid::Uuid;

pub struct AccountsLookupAdapter(pub Arc<::accounts::Service>);

#[async_trait]
impl ::transactions::AccountsLookup for AccountsLookupAdapter {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        self.0.exists(project_id, code).await
    }
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.0.choices(project_id, lang).await
    }
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(self.0.all(project_id, true).await?.into_iter().map(|a| (a.title(lang).to_string(), a.code)).map(|(t, c)| (c, t)).collect())
    }
}

pub struct CategoriesLookupAdapter(pub Arc<::categories::Service>);

#[async_trait]
impl ::transactions::CategoriesLookup for CategoriesLookupAdapter {
    async fn kind_of(&self, project_id: Uuid, code: &str) -> AppResult<Option<String>> {
        self.0.kind_of(project_id, code).await
    }
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.0.choices(project_id, lang).await
    }
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(self.0.nodes(project_id, lang).await?.into_iter().map(|n| (n.code, n.title)).collect())
    }
}
