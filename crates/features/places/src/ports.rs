//! Исходящие порты фичи «Места»: что ей нужно от хранилища и от других фич.

use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use uuid::Uuid;

use crate::domain::Place;

#[async_trait]
pub trait PlaceRepository: Send + Sync {
    async fn insert(&self, item: &Place) -> AppResult<()>;
    async fn update(&self, item: &Place) -> AppResult<()>;
    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Place>>;
    /// Места проекта по названию (без учёта регистра), затем по коду.
    async fn list(&self, project_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Place>>;
    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64>;
    /// Удалить все записи проекта (удаление проекта или аккаунта).
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
}

/// Зависимости фичи от других фич. Реализация — crates/app/src/wiring/wire_places.rs.
pub struct Deps {
    pub directories: Arc<dyn Directories>,
}

/// Категории и счета, которые место может подставлять.
#[async_trait]
pub trait Directories: Send + Sync {
    /// Активные категории деревом (`tag` — EXPENSE / INCOME).
    async fn categories(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>>;
    /// Активные счета (`tag` — валюта).
    async fn accounts(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>>;
}
