//! Входящий адаптер: HTTP-роуты, хендлеры и OpenAPI фичи «Лимиты».

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use kernel::auth::Scope;
use kernel::error::AppResult;
use kernel::pagination::{PagedResponse, Pagination};
use utoipa::OpenApi;
use uuid::Uuid;

use crate::dto::{PeriodQuery, LimitDto, LimitInput};
use crate::service::LimitService;

type Svc = State<Arc<LimitService>>;

pub fn router(service: Arc<LimitService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).put(update).delete(remove))
        .with_state(service)
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_one, update, remove),
    components(schemas(LimitInput, LimitDto)),
    tags((name = "Лимиты", description = "Лимиты"))
)]
pub struct ApiDoc;

#[utoipa::path(
    post, path = "/api/limits", tag = "Лимиты",
    request_body = LimitInput,
    responses((status = 201, description = "Запись создана", body = LimitDto), (status = 422, description = "Ошибка валидации")),
    security(("bearer_auth" = []))
)]
pub async fn create(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Json(body): Json<LimitInput>,
) -> AppResult<(StatusCode, Json<LimitDto>)> {
    Ok((StatusCode::CREATED, Json(svc.create(scope.project_id, body).await?)))
}

#[utoipa::path(
    get, path = "/api/limits", tag = "Лимиты",
    params(Pagination, PeriodQuery),
    responses((status = 200, description = "Записи за период, новые сначала", body = PagedResponse<LimitDto>)),
    security(("bearer_auth" = []))
)]
pub async fn list(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Query(pagination): Query<Pagination>,
    Query(period): Query<PeriodQuery>,
) -> AppResult<Json<PagedResponse<LimitDto>>> {
    Ok(Json(svc.list(scope.project_id, period, pagination).await?))
}

#[utoipa::path(
    get, path = "/api/limits/{id}", tag = "Лимиты",
    params(("id" = Uuid, Path, description = "Идентификатор записи")),
    responses((status = 200, description = "Запись", body = LimitDto), (status = 404, description = "Не найдена")),
    security(("bearer_auth" = []))
)]
pub async fn get_one(State(svc): Svc, Extension(scope): Extension<Scope>, Path(id): Path<Uuid>) -> AppResult<Json<LimitDto>> {
    Ok(Json(svc.get(scope.project_id, id).await?))
}

#[utoipa::path(
    put, path = "/api/limits/{id}", tag = "Лимиты",
    params(("id" = Uuid, Path, description = "Идентификатор записи")),
    request_body = LimitInput,
    responses(
        (status = 200, description = "Запись обновлена", body = LimitDto),
        (status = 404, description = "Не найдена"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn update(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Path(id): Path<Uuid>,
    Json(body): Json<LimitInput>,
) -> AppResult<Json<LimitDto>> {
    Ok(Json(svc.update(scope.project_id, id, body).await?))
}

#[utoipa::path(
    delete, path = "/api/limits/{id}", tag = "Лимиты",
    params(("id" = Uuid, Path, description = "Идентификатор записи")),
    responses((status = 204, description = "Запись удалена"), (status = 404, description = "Не найдена")),
    security(("bearer_auth" = []))
)]
pub async fn remove(State(svc): Svc, Extension(scope): Extension<Scope>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    svc.delete(scope.project_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
