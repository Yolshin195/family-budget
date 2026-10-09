//! Входящий адаптер: REST API фичи «Транзакции» (под `/api/transactions`).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Extension, Json, Router};
use kernel::auth::Scope;
use kernel::error::AppResult;
use kernel::pagination::{PagedResponse, Pagination};
use utoipa::OpenApi;
use uuid::Uuid;

use crate::dto::{LineDto, ListQuery, TransactionDto, TransactionInput, TranslationInput};
use crate::service::TransactionService;

type Svc = State<Arc<TransactionService>>;

pub fn router(service: Arc<TransactionService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).put(update).delete(delete))
        .route("/{id}/translation", put(translate))
        .with_state(service)
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_one, update, delete, translate),
    components(schemas(TransactionInput, TransactionDto, LineDto, TranslationInput)),
    tags((name = "Транзакции", description = "Операции с позициями; сумма = сумма позиций"))
)]
pub struct ApiDoc;

#[utoipa::path(post, path = "/api/transactions", tag = "Транзакции", request_body = TransactionInput,
    responses((status = 201, body = TransactionDto), (status = 409, description = "Вхождение плана уже оплачено"), (status = 422, description = "Ошибка валидации")),
    security(("bearer_auth" = [])))]
pub async fn create(State(svc): Svc, Extension(scope): Extension<Scope>, Json(body): Json<TransactionInput>) -> AppResult<(StatusCode, Json<TransactionDto>)> {
    Ok((StatusCode::CREATED, Json(svc.create(scope.project_id, scope.user_id, body).await?)))
}

#[utoipa::path(get, path = "/api/transactions", tag = "Транзакции", params(Pagination, ListQuery),
    responses((status = 200, body = PagedResponse<TransactionDto>)), security(("bearer_auth" = [])))]
pub async fn list(
    State(svc): Svc,
    Extension(scope): Extension<Scope>,
    Query(pagination): Query<Pagination>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<PagedResponse<TransactionDto>>> {
    Ok(Json(svc.list(scope.project_id, query, pagination).await?))
}

#[utoipa::path(get, path = "/api/transactions/{id}", tag = "Транзакции", params(("id" = Uuid, Path)),
    responses((status = 200, body = TransactionDto), (status = 404)), security(("bearer_auth" = [])))]
pub async fn get_one(State(svc): Svc, Extension(scope): Extension<Scope>, Path(id): Path<Uuid>) -> AppResult<Json<TransactionDto>> {
    Ok(Json(svc.get(scope.project_id, id).await?))
}

#[utoipa::path(put, path = "/api/transactions/{id}", tag = "Транзакции", params(("id" = Uuid, Path)), request_body = TransactionInput,
    responses((status = 200, body = TransactionDto), (status = 404), (status = 422)), security(("bearer_auth" = [])))]
pub async fn update(State(svc): Svc, Extension(scope): Extension<Scope>, Path(id): Path<Uuid>, Json(body): Json<TransactionInput>) -> AppResult<Json<TransactionDto>> {
    Ok(Json(svc.update(scope.project_id, id, body).await?))
}

#[utoipa::path(delete, path = "/api/transactions/{id}", tag = "Транзакции", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404)), security(("bearer_auth" = [])))]
pub async fn delete(State(svc): Svc, Extension(scope): Extension<Scope>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    svc.delete(scope.project_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(put, path = "/api/transactions/{id}/translation", tag = "Транзакции", params(("id" = Uuid, Path)), request_body = TranslationInput,
    responses((status = 200, body = TransactionDto), (status = 404), (status = 422)), security(("bearer_auth" = [])))]
pub async fn translate(State(svc): Svc, Extension(scope): Extension<Scope>, Path(id): Path<Uuid>, Json(body): Json<TranslationInput>) -> AppResult<Json<TransactionDto>> {
    Ok(Json(svc.translate(scope.project_id, id, body).await?))
}
