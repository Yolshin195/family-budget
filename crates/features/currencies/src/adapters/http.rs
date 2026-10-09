//! Входящий адаптер: HTTP-роуты, хендлеры и OpenAPI фичи «Валюты».

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use kernel::auth::Scope;
use kernel::error::AppResult;
use kernel::pagination::{PagedResponse, Pagination};
use utoipa::OpenApi;

use crate::dto::{CreateCurrencyDto, ListQuery, UpdateCurrencyDto, CurrencyDto};
use crate::service::CurrencyService;

type Svc = State<Arc<CurrencyService>>;

pub fn router(service: Arc<CurrencyService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{code}", get(get_one).put(update).delete(archive))
        .with_state(service)
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_one, update, archive),
    components(schemas(CreateCurrencyDto, UpdateCurrencyDto, CurrencyDto)),
    tags((name = "Валюты", description = "Валюты"))
)]
pub struct ApiDoc;

#[utoipa::path(
    post, path = "/api/currencies", tag = "Валюты",
    request_body = CreateCurrencyDto,
    responses(
        (status = 201, description = "Запись создана", body = CurrencyDto),
        (status = 409, description = "Код уже занят"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn create(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Json(body): Json<CreateCurrencyDto>,
) -> AppResult<(StatusCode, Json<CurrencyDto>)> {
    Ok((StatusCode::CREATED, Json(svc.create(scope.project_id, body).await?)))
}

#[utoipa::path(
    get, path = "/api/currencies", tag = "Валюты",
    params(Pagination, ListQuery),
    responses((status = 200, description = "Список записей", body = PagedResponse<CurrencyDto>)),
    security(("bearer_auth" = []))
)]
pub async fn list(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Query(pagination): Query<Pagination>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<PagedResponse<CurrencyDto>>> {
    Ok(Json(svc.list(scope.project_id, query, pagination).await?))
}

#[utoipa::path(
    get, path = "/api/currencies/{code}", tag = "Валюты",
    params(("code" = String, Path, description = "Код записи")),
    responses((status = 200, description = "Запись", body = CurrencyDto), (status = 404, description = "Не найдена")),
    security(("bearer_auth" = []))
)]
pub async fn get_one(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Path(code): Path<String>,
) -> AppResult<Json<CurrencyDto>> {
    Ok(Json(svc.get(scope.project_id, &code).await?))
}

#[utoipa::path(
    put, path = "/api/currencies/{code}", tag = "Валюты",
    params(("code" = String, Path, description = "Код записи")),
    request_body = UpdateCurrencyDto,
    responses(
        (status = 200, description = "Запись обновлена", body = CurrencyDto),
        (status = 404, description = "Не найдена"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn update(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Path(code): Path<String>,
    Json(body): Json<UpdateCurrencyDto>,
) -> AppResult<Json<CurrencyDto>> {
    Ok(Json(svc.update(scope.project_id, &code, body).await?))
}

#[utoipa::path(
    delete, path = "/api/currencies/{code}", tag = "Валюты",
    params(("code" = String, Path, description = "Код записи")),
    responses((status = 204, description = "Запись архивирована"), (status = 404, description = "Не найдена")),
    security(("bearer_auth" = []))
)]
pub async fn archive(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Path(code): Path<String>,
) -> AppResult<StatusCode> {
    svc.archive(scope.project_id, &code).await?;
    Ok(StatusCode::NO_CONTENT)
}
