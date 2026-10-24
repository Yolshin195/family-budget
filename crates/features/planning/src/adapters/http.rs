//! Входящий адаптер: REST API фичи «Планирование» (под `/api/planning`).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::NaiveDate;
use kernel::auth::Scope;
use kernel::error::AppResult;
use utoipa::OpenApi;
use uuid::Uuid;

use crate::dto::{OccurrenceDto, PlanDto, PlanInput, RangeQuery};
use crate::service::PlanningService;

type Svc = State<Arc<PlanningService>>;

pub fn router(service: Arc<PlanningService>) -> Router {
    Router::new()
        .route("/plans", get(list).post(create))
        .route("/plans/{id}", get(get_one).put(update).delete(delete))
        .route("/plans/{id}/skips/{date}", post(skip).delete(unskip))
        .route("/occurrences", get(occurrences))
        .with_state(service)
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_one, update, delete, occurrences, skip, unskip),
    components(schemas(PlanInput, PlanDto, OccurrenceDto)),
    tags((name = "Планирование", description = "Плановые и регулярные операции, вхождения в календаре"))
)]
pub struct ApiDoc;

#[utoipa::path(post, path = "/api/planning/plans", tag = "Планирование", request_body = PlanInput,
    responses(
        (status = 201, body = PlanDto, description = "План создан"),
        (status = 422, description = "Ошибка валидации"),
    ), security(("bearer_auth" = [])))]
pub async fn create(
    State(svc): Svc,
    Extension(s): Extension<Scope>,
    Json(body): Json<PlanInput>,
) -> AppResult<(StatusCode, Json<PlanDto>)> {
    Ok((StatusCode::CREATED, Json(svc.create(s.project_id, s.user_id, body).await?)))
}

#[utoipa::path(get, path = "/api/planning/plans", tag = "Планирование",
    responses((status = 200, body = Vec<PlanDto>, description = "Планы проекта")), security(("bearer_auth" = [])))]
pub async fn list(State(svc): Svc, Extension(s): Extension<Scope>) -> AppResult<Json<Vec<PlanDto>>> {
    Ok(Json(svc.list(s.project_id).await?))
}

#[utoipa::path(get, path = "/api/planning/plans/{id}", tag = "Планирование", params(("id" = Uuid, Path)),
    responses((status = 200, body = PlanDto, description = "План"), (status = 404, description = "Не найден")), security(("bearer_auth" = [])))]
pub async fn get_one(
    State(svc): Svc,
    Extension(s): Extension<Scope>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PlanDto>> {
    Ok(Json(svc.get(s.project_id, id).await?))
}

#[utoipa::path(put, path = "/api/planning/plans/{id}", tag = "Планирование", params(("id" = Uuid, Path)), request_body = PlanInput,
    responses(
        (status = 200, body = PlanDto, description = "План обновлён"),
        (status = 404, description = "Не найден"),
        (status = 422, description = "Ошибка валидации"),
    ), security(("bearer_auth" = [])))]
pub async fn update(
    State(svc): Svc,
    Extension(s): Extension<Scope>,
    Path(id): Path<Uuid>,
    Json(body): Json<PlanInput>,
) -> AppResult<Json<PlanDto>> {
    Ok(Json(svc.update(s.project_id, id, body).await?))
}

#[utoipa::path(delete, path = "/api/planning/plans/{id}", tag = "Планирование", params(("id" = Uuid, Path)),
    responses((status = 204, description = "План удалён"), (status = 404, description = "Не найден")), security(("bearer_auth" = [])))]
pub async fn delete(State(svc): Svc, Extension(s): Extension<Scope>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    svc.delete(s.project_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/planning/occurrences", tag = "Планирование", params(RangeQuery),
    responses((status = 200, body = Vec<OccurrenceDto>, description = "Вхождения планов за период")), security(("bearer_auth" = [])))]
pub async fn occurrences(
    State(svc): Svc,
    Extension(s): Extension<Scope>,
    Query(q): Query<RangeQuery>,
) -> AppResult<Json<Vec<OccurrenceDto>>> {
    Ok(Json(svc.occurrences(s.project_id, q.from, q.to, chrono::Local::now().date_naive()).await?))
}

#[utoipa::path(post, path = "/api/planning/plans/{id}/skips/{date}", tag = "Планирование",
    params(("id" = Uuid, Path), ("date" = NaiveDate, Path)),
    responses(
        (status = 204, description = "Вхождение пропущено"),
        (status = 404, description = "Не найден"),
        (status = 409, description = "Вхождение уже оплачено"),
    ), security(("bearer_auth" = [])))]
pub async fn skip(
    State(svc): Svc,
    Extension(s): Extension<Scope>,
    Path((id, date)): Path<(Uuid, NaiveDate)>,
) -> AppResult<StatusCode> {
    svc.skip(s.project_id, s.user_id, id, date).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/api/planning/plans/{id}/skips/{date}", tag = "Планирование",
    params(("id" = Uuid, Path), ("date" = NaiveDate, Path)),
    responses((status = 204, description = "Пропуск отменён")), security(("bearer_auth" = [])))]
pub async fn unskip(
    State(svc): Svc,
    Extension(s): Extension<Scope>,
    Path((id, date)): Path<(Uuid, NaiveDate)>,
) -> AppResult<StatusCode> {
    svc.unskip(s.project_id, id, date).await?;
    Ok(StatusCode::NO_CONTENT)
}
