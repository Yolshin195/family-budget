//! Исходящие порты фичи «Проекты».

use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use uuid::Uuid;

use crate::domain::{Member, Project, User};

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn insert_user(&self, user: &User) -> AppResult<()>;
    async fn update_user_lang(&self, user_id: Uuid, lang: Lang) -> AppResult<()>;
    async fn user_by_email(&self, email: &str) -> AppResult<Option<User>>;
    async fn user(&self, id: Uuid) -> AppResult<Option<User>>;
    /// Проект и его первый участник (владелец) — атомарно.
    async fn insert_project(&self, project: &Project, owner: &Member) -> AppResult<()>;
    async fn rename_project(&self, project_id: Uuid, name: &str) -> AppResult<()>;
    async fn project(&self, id: Uuid) -> AppResult<Option<Project>>;
    /// Проекты пользователя с его ролью: личные сначала, затем по дате создания.
    async fn projects_of(&self, user_id: Uuid) -> AppResult<Vec<(Project, Member)>>;
    /// Участники проекта с пользователями.
    async fn members(&self, project_id: Uuid) -> AppResult<Vec<(Member, User)>>;
    async fn upsert_member(&self, member: &Member) -> AppResult<()>;
    async fn remove_member(&self, project_id: Uuid, user_id: Uuid) -> AppResult<()>;
}

/// Начальное наполнение нового проекта (базовая валюта, категории) — реализуется в app/wiring
/// через публичные API фич `currencies` и `categories`.
#[async_trait]
pub trait ProjectBootstrap: Send + Sync {
    async fn init(&self, project_id: Uuid, base_currency: &str, lang: Lang) -> AppResult<()>;
}

pub struct Deps {
    pub bootstrap: Arc<dyn ProjectBootstrap>,
}
