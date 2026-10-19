//! Входящий адаптер: REST API фичи «Транзакции» (под `/api/transactions`).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Extension, Json, Router};
use kernel::auth::{Scope, Session};
use kernel::error::AppResult;
use kernel::pagination::{PagedResponse, Pagination};
use utoipa::OpenApi;
use uuid::Uuid;

use crate::dto::{
    BalanceDto, BalanceQuery, ItemDto, ItemInput, LineDto, ListQuery, PostingDto, QuickAccountDto, QuickCategoryDto, QuickDefaultsDto, QuickQuery, TransactionDto, TransactionInput,
    TranslationInput,
};
use crate::service::TransactionService;

type Svc = State<Arc<TransactionService>>;

pub fn router(service: Arc<TransactionService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/quick-defaults", get(quick_defaults))
        .route("/balances", get(balances))
        .route("/{id}", get(get_one).put(update).delete(delete))
        .route("/{id}/translation", put(translate))
        .with_state(service)
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_one, update, delete, translate, quick_defaults, balances),
    components(schemas(TransactionInput, TransactionDto, ItemInput, ItemDto, LineDto, PostingDto, TranslationInput, QuickDefaultsDto, QuickAccountDto, QuickCategoryDto, BalanceDto)),
    tags((name = "Транзакции", description = "Двойная запись: транзакция = шапка + проводки, сумма проводок по каждой валюте = 0. \
        Клиент присылает тип и поля формы, сервер строит проводки."))
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

/// Быстрый расход: счёт по умолчанию, частые категории и остатки лимитов по ним. Сохранение —
/// обычный `POST /api/transactions`.
#[utoipa::path(get, path = "/api/transactions/quick-defaults", tag = "Транзакции", params(QuickQuery),
    responses((status = 200, body = QuickDefaultsDto)), security(("bearer_auth" = [])))]
pub async fn quick_defaults(State(svc): Svc, Extension(s): Extension<Session>, Query(q): Query<QuickQuery>) -> AppResult<Json<QuickDefaultsDto>> {
    let date = q.date.unwrap_or_else(|| chrono::Local::now().date_naive());
    Ok(Json(svc.quick_defaults(s.project_id, s.user_id, &s.base_currency, s.lang, date).await?))
}

/// Остатки счетов на дату: сумма проводок по счёту (начальный остаток — тоже проводка).
#[utoipa::path(get, path = "/api/transactions/balances", tag = "Транзакции", params(BalanceQuery),
    responses((status = 200, body = Vec<BalanceDto>)), security(("bearer_auth" = [])))]
pub async fn balances(State(svc): Svc, Extension(scope): Extension<Scope>, Query(q): Query<BalanceQuery>) -> AppResult<Json<Vec<BalanceDto>>> {
    let date = q.date.unwrap_or_else(|| chrono::Local::now().date_naive());
    let items = svc.balances(scope.project_id, date).await?;
    Ok(Json(items.into_iter().map(|(account_code, balance)| BalanceDto { account_code, balance }).collect()))
}
