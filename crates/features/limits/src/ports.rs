//! Исходящие порты фичи «Лимиты»: что ей нужно от хранилища и от других фич.

#[allow(unused_imports)]
use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

use crate::domain::Limit;

/// Период выборки по `valid_from` (обе границы включительно).
#[derive(Debug, Clone, Copy, Default)]
pub struct Period {
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
}

#[async_trait]
pub trait LimitRepository: Send + Sync {
    async fn insert(&self, item: &Limit) -> AppResult<()>;
    async fn update(&self, item: &Limit) -> AppResult<()>;
    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Limit>>;
    async fn list(&self, project_id: Uuid, period: Period, limit: i64, offset: i64) -> AppResult<Vec<Limit>>;
    async fn count(&self, project_id: Uuid, period: Period) -> AppResult<i64>;
    /// `true`, если запись была удалена.
    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool>;
    /// Удалить все записи проекта (удаление проекта или аккаунта).
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
}

/// Зависимости фичи от других фич. Порты объявляет эта фича (потребитель),
/// реализации живут в приложении: crates/app/src/wiring/wire_limits.rs.
pub struct Deps {
    pub categories: Arc<dyn CategoriesLookup>,
}

/// Что этой фиче нужно от справочника `categories`.
#[async_trait]
pub trait CategoriesLookup: Send + Sync {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
    /// Активные записи для выпадающего списка на языке `lang`.
    async fn choices(&self, _project_id: Uuid, _lang: kernel::i18n::Lang) -> AppResult<Vec<kernel::web::Choice>> {
        Ok(Vec::new())
    }
}
