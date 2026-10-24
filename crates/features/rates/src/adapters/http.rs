//! Входящий адаптер: HTTP-роуты, хендлеры и OpenAPI фичи «Курсы валют».

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

use crate::dto::{PeriodQuery, RateDto, RateInput};
use crate::service::RateService;

type Svc = State<Arc<RateService>>;

pub fn router(service: Arc<RateService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).put(update).delete(remove))
        .with_state(service)
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_one, update, remove),
    components(schemas(RateInput, RateDto)),
    tags((name = "Курсы валют", description = "Курсы валют"))
)]
pub struct ApiDoc;

#[utoipa::path(
    post, path = "/api/rates", tag = "Курсы валют",
    request_body = RateInput,
    responses((status = 201, description = "Запись создана", body = RateDto), (status = 422, description = "Ошибка валидации")),
    security(("bearer_auth" = []))
)]
pub async fn create(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Json(body): Json<RateInput>,
) -> AppResult<(StatusCode, Json<RateDto>)> {
    Ok((StatusCode::CREATED, Json(svc.create(scope.project_id, body).await?)))
}

#[utoipa::path(
    get, path = "/api/rates", tag = "Курсы валют",
    params(Pagination, PeriodQuery),
    responses((status = 200, description = "Записи за период, новые сначала", body = PagedResponse<RateDto>)),
    security(("bearer_auth" = []))
)]
pub async fn list(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Query(pagination): Query<Pagination>,
    Query(period): Query<PeriodQuery>,
) -> AppResult<Json<PagedResponse<RateDto>>> {
    Ok(Json(svc.list(scope.project_id, period, pagination).await?))
}

#[utoipa::path(
    get, path = "/api/rates/{id}", tag = "Курсы валют",
    params(("id" = Uuid, Path, description = "Идентификатор записи")),
    responses((status = 200, description = "Запись", body = RateDto), (status = 404, description = "Не найдена")),
    security(("bearer_auth" = []))
)]
pub async fn get_one(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<RateDto>> {
    Ok(Json(svc.get(scope.project_id, id).await?))
}

#[utoipa::path(
    put, path = "/api/rates/{id}", tag = "Курсы валют",
    params(("id" = Uuid, Path, description = "Идентификатор записи")),
    request_body = RateInput,
    responses(
        (status = 200, description = "Запись обновлена", body = RateDto),
        (status = 404, description = "Не найдена"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn update(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Path(id): Path<Uuid>,
    Json(body): Json<RateInput>,
) -> AppResult<Json<RateDto>> {
    Ok(Json(svc.update(scope.project_id, id, body).await?))
}

#[utoipa::path(
    delete, path = "/api/rates/{id}", tag = "Курсы валют",
    params(("id" = Uuid, Path, description = "Идентификатор записи")),
    responses((status = 204, description = "Запись удалена"), (status = 404, description = "Не найдена")),
    security(("bearer_auth" = []))
)]
pub async fn remove(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    svc.delete(scope.project_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
