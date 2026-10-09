//! Входящий адаптер: HTTP-роуты, хендлеры и OpenAPI фичи «Счета».

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use kernel::auth::CurrentUser;
use kernel::error::AppResult;
use kernel::pagination::{PagedResponse, Pagination};
use utoipa::OpenApi;

use crate::dto::{CreateAccountDto, ListQuery, UpdateAccountDto, AccountDto};
use crate::service::AccountService;

type Svc = State<Arc<AccountService>>;

pub fn router(service: Arc<AccountService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{code}", get(get_one).put(update).delete(archive))
        .with_state(service)
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_one, update, archive),
    components(schemas(CreateAccountDto, UpdateAccountDto, AccountDto)),
    tags((name = "Счета", description = "Счета"))
)]
pub struct ApiDoc;

#[utoipa::path(
    post, path = "/accounts", tag = "Счета",
    request_body = CreateAccountDto,
    responses(
        (status = 201, description = "Запись создана", body = AccountDto),
        (status = 409, description = "Код уже занят"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn create(
    State(svc): Svc,
    Extension(user): Extension<CurrentUser>,
    Json(body): Json<CreateAccountDto>,
) -> AppResult<(StatusCode, Json<AccountDto>)> {
    Ok((StatusCode::CREATED, Json(svc.create(user.id, body).await?)))
}

#[utoipa::path(
    get, path = "/accounts", tag = "Счета",
    params(Pagination, ListQuery),
    responses((status = 200, description = "Список записей", body = PagedResponse<AccountDto>)),
    security(("bearer_auth" = []))
)]
pub async fn list(
    State(svc): Svc,
    Extension(user): Extension<CurrentUser>,
    Query(pagination): Query<Pagination>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<PagedResponse<AccountDto>>> {
    Ok(Json(svc.list(user.id, query, pagination).await?))
}

#[utoipa::path(
    get, path = "/accounts/{code}", tag = "Счета",
    params(("code" = String, Path, description = "Код записи")),
    responses((status = 200, description = "Запись", body = AccountDto), (status = 404, description = "Не найдена")),
    security(("bearer_auth" = []))
)]
pub async fn get_one(
    State(svc): Svc,
    Extension(user): Extension<CurrentUser>,
    Path(code): Path<String>,
) -> AppResult<Json<AccountDto>> {
    Ok(Json(svc.get(user.id, &code).await?))
}

#[utoipa::path(
    put, path = "/accounts/{code}", tag = "Счета",
    params(("code" = String, Path, description = "Код записи")),
    request_body = UpdateAccountDto,
    responses(
        (status = 200, description = "Запись обновлена", body = AccountDto),
        (status = 404, description = "Не найдена"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn update(
    State(svc): Svc,
    Extension(user): Extension<CurrentUser>,
    Path(code): Path<String>,
    Json(body): Json<UpdateAccountDto>,
) -> AppResult<Json<AccountDto>> {
    Ok(Json(svc.update(user.id, &code, body).await?))
}

#[utoipa::path(
    delete, path = "/accounts/{code}", tag = "Счета",
    params(("code" = String, Path, description = "Код записи")),
    responses((status = 204, description = "Запись архивирована"), (status = 404, description = "Не найдена")),
    security(("bearer_auth" = []))
)]
pub async fn archive(
    State(svc): Svc,
    Extension(user): Extension<CurrentUser>,
    Path(code): Path<String>,
) -> AppResult<StatusCode> {
    svc.archive(user.id, &code).await?;
    Ok(StatusCode::NO_CONTENT)
}
