//! Входящий адаптер: HTTP-роуты, хендлеры и OpenAPI фичи «Люди».

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use kernel::auth::Scope;
use kernel::error::AppResult;
use kernel::pagination::{PagedResponse, Pagination};
use utoipa::OpenApi;

use crate::dto::{CreatePersonDto, ListQuery, PersonDto, PersonFields};
use crate::service::PersonService;

type Svc = State<Arc<PersonService>>;

pub fn router(service: Arc<PersonService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{code}", get(get_one).put(update).delete(archive))
        .route("/{code}/restore", post(restore))
        .with_state(service)
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_one, update, archive, restore),
    components(schemas(CreatePersonDto, PersonFields, PersonDto)),
    tags((name = "Люди", description = "Для кого покупка: участники проекта и люди без аккаунта"))
)]
pub struct ApiDoc;

#[utoipa::path(
    post, path = "/api/people", tag = "Люди",
    request_body = CreatePersonDto,
    responses(
        (status = 201, description = "Человек добавлен", body = PersonDto),
        (status = 409, description = "Код уже занят"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn create(State(svc): Svc, Extension(scope): Extension<Scope>, Json(body): Json<CreatePersonDto>) -> AppResult<(StatusCode, Json<PersonDto>)> {
    Ok((StatusCode::CREATED, Json(svc.create(scope.project_id, body).await?)))
}

#[utoipa::path(
    get, path = "/api/people", tag = "Люди",
    params(Pagination, ListQuery),
    responses((status = 200, description = "Люди по имени", body = PagedResponse<PersonDto>)),
    security(("bearer_auth" = []))
)]
pub async fn list(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Query(pagination): Query<Pagination>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<PagedResponse<PersonDto>>> {
    Ok(Json(svc.list(scope.project_id, query, pagination).await?))
}

#[utoipa::path(
    get, path = "/api/people/{code}", tag = "Люди",
    params(("code" = String, Path, description = "Код человека")),
    responses((status = 200, description = "Человек", body = PersonDto), (status = 404, description = "Не найдено")),
    security(("bearer_auth" = []))
)]
pub async fn get_one(State(svc): Svc, Extension(scope): Extension<Scope>, Path(code): Path<String>) -> AppResult<Json<PersonDto>> {
    Ok(Json(svc.get(scope.project_id, &code).await?))
}

#[utoipa::path(
    put, path = "/api/people/{code}", tag = "Люди",
    params(("code" = String, Path, description = "Код человека")),
    request_body = PersonFields,
    responses(
        (status = 200, description = "Изменено", body = PersonDto),
        (status = 404, description = "Не найдено"),
        (status = 422, description = "Ошибка валидации"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn update(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Path(code): Path<String>,
    Json(body): Json<PersonFields>,
) -> AppResult<Json<PersonDto>> {
    Ok(Json(svc.update(scope.project_id, &code, body).await?))
}

#[utoipa::path(
    delete, path = "/api/people/{code}", tag = "Люди",
    params(("code" = String, Path, description = "Код человека")),
    responses((status = 204, description = "В архиве"), (status = 404, description = "Не найдено"), (status = 422, description = "Запись участника проекта архивируется сама")),
    security(("bearer_auth" = []))
)]
pub async fn archive(State(svc): Svc, Extension(scope): Extension<Scope>, Path(code): Path<String>) -> AppResult<StatusCode> {
    svc.archive(scope.project_id, &code).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/people/{code}/restore", tag = "Люди",
    params(("code" = String, Path, description = "Код человека")),
    responses((status = 204, description = "Возвращено из архива"), (status = 404, description = "Не найдено"), (status = 422, description = "Запись участника проекта архивируется сама")),
    security(("bearer_auth" = []))
)]
pub async fn restore(State(svc): Svc, Extension(scope): Extension<Scope>, Path(code): Path<String>) -> AppResult<StatusCode> {
    svc.restore(scope.project_id, &code).await?;
    Ok(StatusCode::NO_CONTENT)
}
