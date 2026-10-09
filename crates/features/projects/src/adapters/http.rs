//! Входящий адаптер: REST API фичи «Проекты». Вход — публичный (`/api/auth/login`), остальное под JWT.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use kernel::auth::Scope;
use kernel::error::AppResult;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

use crate::service::ProjectService;

type Svc = State<Arc<ProjectService>>;

pub fn router(service: Arc<ProjectService>) -> Router {
    Router::new().route("/", get(list)).with_state(service)
}

pub fn public(service: Arc<ProjectService>) -> Router {
    Router::new().route("/api/auth/login", post(login)).with_state(service)
}

#[derive(OpenApi)]
#[openapi(paths(login, list), components(schemas(LoginInput, TokenDto, ProjectDto, MemberDto)), tags((name = "Проекты", description = "Вход, проекты и участники")))]
pub struct ApiDoc;

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginInput {
    #[schema(example = "aleksey@demo.local")]
    pub email: String,
    #[schema(example = "demo123")]
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TokenDto {
    /// JWT для заголовка `Authorization: Bearer`; проект — заголовок `X-Project-Id`
    pub token: String,
}

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
    pub members: Vec<MemberDto>,
}

#[utoipa::path(post, path = "/api/auth/login", tag = "Проекты", request_body = LoginInput, responses((status = 200, body = TokenDto), (status = 422)))]
pub async fn login(State(svc): Svc, Json(body): Json<LoginInput>) -> AppResult<Json<TokenDto>> {
    let id = svc.login(&body.email, &body.password).await?;
    Ok(Json(TokenDto { token: svc.issue_token(id) }))
}

#[utoipa::path(get, path = "/api/projects", tag = "Проекты", responses((status = 200, body = Vec<ProjectDto>)), security(("bearer_auth" = [])))]
pub async fn list(State(svc): Svc, Extension(s): Extension<Scope>) -> AppResult<Json<Vec<ProjectDto>>> {
    Ok(Json(
        svc.overview(s.user_id)
            .await?
            .into_iter()
            .map(|p| ProjectDto {
                id: p.id,
                name: p.name,
                kind: p.kind,
                base_currency: p.base_currency,
                role: p.role.as_str().into(),
                members: p.members.into_iter().map(|m| MemberDto { user_id: m.user_id, name: m.name, email: m.email, role: m.role.as_str().into() }).collect(),
            })
            .collect(),
    ))
}
