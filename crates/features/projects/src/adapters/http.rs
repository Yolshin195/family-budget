//! Входящий адаптер: REST API фичи «Проекты» (под `/api/projects`). Вход по паролю убран (ТЗ v3):
//! клиенты без браузера используют личный API-токен из профиля (`Authorization: Bearer fbt_…`).

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Extension, Json, Router};
use kernel::auth::Session;
use kernel::error::AppResult;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

use crate::service::ProjectService;

type Svc = State<Arc<ProjectService>>;

pub fn router(service: Arc<ProjectService>) -> Router {
    Router::new().route("/", get(list)).with_state(service)
}

#[derive(OpenApi)]
#[openapi(paths(list), components(schemas(ProjectDto, MemberDto)), tags((name = "Проекты", description = "Проекты и участники")))]
pub struct ApiDoc;

#[derive(Debug, Serialize, ToSchema)]
pub struct MemberDto {
    pub user_id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectDto {
    pub id: Uuid,
    pub name: String,
    /// PERSONAL / FAMILY
    pub kind: String,
    pub base_currency: String,
    /// Ваша роль
    pub role: String,
    /// Проект требует второй фактор от участников
    pub require_2fa: bool,
    pub members: Vec<MemberDto>,
}

#[utoipa::path(get, path = "/api/projects", tag = "Проекты",
    responses((status = 200, body = Vec<ProjectDto>, description = "Проекты пользователя; по API-токену — только его проект")), security(("bearer_auth" = [])))]
pub async fn list(State(svc): Svc, Extension(s): Extension<Session>) -> AppResult<Json<Vec<ProjectDto>>> {
    Ok(Json(
        svc.overview(s.user_id)
            .await?
            .into_iter()
            // API-токен видит только свой проект.
            .filter(|p| !s.via_token || p.id == s.project_id)
            .map(|p| ProjectDto {
                id: p.id,
                name: p.name,
                kind: p.kind,
                base_currency: p.base_currency,
                role: p.role.as_str().into(),
                require_2fa: p.require_2fa,
                members: p.members.into_iter().map(|m| MemberDto { user_id: m.user_id, name: m.name, email: m.email, role: m.role.as_str().into() }).collect(),
            })
            .collect(),
    ))
}
