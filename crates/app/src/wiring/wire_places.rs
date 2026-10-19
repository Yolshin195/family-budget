//! Связка фичи `places`: категории и счета, которые место подставляет в операцию.

use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use uuid::Uuid;

pub struct DirectoriesAdapter {
    pub categories: Arc<::categories::Service>,
    pub accounts: Arc<::accounts::Service>,
}

#[async_trait]
impl ::places::Directories for DirectoriesAdapter {
    async fn categories(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.categories.choices(project_id, lang).await
    }
    async fn accounts(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.accounts.choices(project_id, lang).await
    }
}
