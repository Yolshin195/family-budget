//! Входящий адаптер: HTTP-роуты, хендлеры и OpenAPI фичи «Места».

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use kernel::auth::Scope;
use kernel::error::AppResult;
use kernel::pagination::{PagedResponse, Pagination};
use utoipa::OpenApi;

use crate::dto::{CreatePlaceDto, ListQuery, PlaceDto, PlaceFields};
use crate::service::PlaceService;

type Svc = State<Arc<PlaceService>>;

pub fn router(service: Arc<PlaceService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{code}", get(get_one).put(update).delete(archive))
        .route("/{code}/restore", post(restore))
        .with_state(service)
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_one, update, archive, restore),
    components(schemas(CreatePlaceDto, PlaceFields, PlaceDto)),
    tags((name = "Места", description = "Магазины, кафе и рынки: подставляют в операцию категорию и счёт"))
)]
pub struct ApiDoc;

#[utoipa::path(
    post, path = "/api/places", tag = "Места",
    request_body = CreatePlaceDto,
    responses(
        (status = 201, description = "Место создано", body = PlaceDto),
        (status = 409, description = "Код уже занят"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn create(State(svc): Svc, Extension(scope): Extension<Scope>, Json(body): Json<CreatePlaceDto>) -> AppResult<(StatusCode, Json<PlaceDto>)> {
    Ok((StatusCode::CREATED, Json(svc.create(scope.project_id, body).await?)))
}

#[utoipa::path(
    get, path = "/api/places", tag = "Места",
    params(Pagination, ListQuery),
    responses((status = 200, description = "Места по названию", body = PagedResponse<PlaceDto>)),
    security(("bearer_auth" = []))
)]
pub async fn list(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Query(pagination): Query<Pagination>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<PagedResponse<PlaceDto>>> {
    Ok(Json(svc.list(scope.project_id, query, pagination).await?))
}

#[utoipa::path(
    get, path = "/api/places/{code}", tag = "Места",
    params(("code" = String, Path, description = "Код места")),
    responses((status = 200, description = "Место", body = PlaceDto), (status = 404, description = "Не найдено")),
    security(("bearer_auth" = []))
)]
pub async fn get_one(State(svc): Svc, Extension(scope): Extension<Scope>, Path(code): Path<String>) -> AppResult<Json<PlaceDto>> {
    Ok(Json(svc.get(scope.project_id, &code).await?))
}

#[utoipa::path(
    put, path = "/api/places/{code}", tag = "Места",
    params(("code" = String, Path, description = "Код места")),
    request_body = PlaceFields,
    responses(
        (status = 200, description = "Место изменено", body = PlaceDto),
        (status = 404, description = "Не найдено"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn update(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Path(code): Path<String>,
    Json(body): Json<PlaceFields>,
) -> AppResult<Json<PlaceDto>> {
    Ok(Json(svc.update(scope.project_id, &code, body).await?))
}

#[utoipa::path(
    delete, path = "/api/places/{code}", tag = "Места",
    params(("code" = String, Path, description = "Код места")),
    responses((status = 204, description = "Место в архиве"), (status = 404, description = "Не найдено")),
    security(("bearer_auth" = []))
)]
pub async fn archive(State(svc): Svc, Extension(scope): Extension<Scope>, Path(code): Path<String>) -> AppResult<StatusCode> {
    svc.archive(scope.project_id, &code).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/places/{code}/restore", tag = "Места",
    params(("code" = String, Path, description = "Код места")),
    responses((status = 204, description = "Место возвращено из архива"), (status = 404, description = "Не найдено")),
    security(("bearer_auth" = []))
)]
pub async fn restore(State(svc): Svc, Extension(scope): Extension<Scope>, Path(code): Path<String>) -> AppResult<StatusCode> {
    svc.restore(scope.project_id, &code).await?;
    Ok(StatusCode::NO_CONTENT)
}
