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
    async fn find_by_code(&self, user_id: Uuid, code: &str) -> AppResult<Option<Category>>;
    async fn list(&self, user_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Category>>;
    async fn count(&self, user_id: Uuid, include_archived: bool) -> AppResult<i64>;
}

/// Зависимости фичи от других фич. Порты объявляет эта фича (потребитель),
/// реализации живут в приложении: crates/app/src/wiring/wire_categories.rs.
pub struct Deps {
}

