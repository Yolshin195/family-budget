//! Исходящие порты фичи «Люди»: что ей нужно от хранилища. От других фич ей ничего не нужно —
//! записи участников создаёт и архивирует фича `projects` через публичный контракт сервиса.

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

use crate::domain::Person;

#[async_trait]
pub trait PersonRepository: Send + Sync {
    /// Занятый код или участник, у которого уже есть запись, — `Conflict`.
    async fn insert(&self, item: &Person) -> AppResult<()>;
    async fn update(&self, item: &Person) -> AppResult<()>;
    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Person>>;
    /// Запись участника в проекте (и архивная).
    async fn find_by_user(&self, project_id: Uuid, user_id: Uuid) -> AppResult<Option<Person>>;
    /// Записи пользователя во всех проектах (удаление аккаунта).
    async fn of_user(&self, user_id: Uuid) -> AppResult<Vec<Person>>;
    /// Люди проекта по имени (без учёта регистра), затем по коду.
    async fn list(&self, project_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Person>>;
    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64>;
    /// Все коды проекта, с архивными, — чтобы подобрать свободный код участнику.
    async fn codes(&self, project_id: Uuid) -> AppResult<Vec<String>>;
    /// Удалить все записи проекта (удаление проекта или аккаунта).
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
}
