//! Исходящие порты фичи «Категории»: что ей нужно от хранилища и от других фич.

#[allow(unused_imports)]
use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

use crate::domain::Category;

#[async_trait]
pub trait CategoryRepository: Send + Sync {
    async fn insert(&self, item: &Category) -> AppResult<()>;
    async fn update(&self, item: &Category) -> AppResult<()>;
    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Category>>;
    async fn list(&self, project_id: Uuid, include_archived: bool, limit: i64, offset: i64)
    -> AppResult<Vec<Category>>;
    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64>;
    /// Удалить все записи проекта (удаление проекта или аккаунта).
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
}

/// Зависимости фичи от других фич. Порты объявляет эта фича (потребитель),
/// реализации живут в приложении: crates/app/src/wiring/wire_categories.rs.
pub struct Deps {
    pub usage: Arc<dyn CategoryUsage>,
}

/// Где категория уже использована.
#[async_trait]
pub trait CategoryUsage: Send + Sync {
    /// Есть ли у категории операции, планы или лимиты — тогда её тип (расход/доход) не меняется:
    /// иначе старые суммы уйдут в другое дерево бюджета и итоги разойдутся (этап 0, Ц2).
    async fn in_use(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
}
