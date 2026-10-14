//! Входящий адаптер: REST API фичи «Бюджет» (под `/api/budget`).

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::NaiveDate;
use kernel::auth::Session;
use kernel::error::AppResult;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

use crate::domain::{Period, View};
use crate::service::BudgetService;

type Svc = State<Arc<BudgetService>>;

pub fn router(service: Arc<BudgetService>) -> Router {
    Router::new().route("/", get(report)).with_state(service)
}

#[derive(OpenApi)]
#[openapi(paths(report), components(schemas(BudgetDto, BudgetRowDto)), tags((name = "Бюджет", description = "План, факт и лимиты по дереву категорий")))]
pub struct ApiDoc;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ReportQuery {
    /// week / month / year
    #[serde(default)]
    pub view: Option<String>,
    /// Любая дата внутри периода (по умолчанию — сегодня)
    #[serde(default)]
    pub date: Option<NaiveDate>,
    /// Факт только операций этого участника
    #[serde(default)]
    pub member: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BudgetRowDto {
    pub code: String,
    pub parent_code: Option<String>,
    pub title: String,
    pub kind: String,
    #[schema(value_type = Option<String>)]
    pub limit: Option<Decimal>,
    #[schema(value_type = String)]
    pub fact: Decimal,
    #[schema(value_type = String)]
    pub planned: Decimal,
    #[schema(value_type = String)]
    pub forecast: Decimal,
    #[schema(value_type = Option<String>)]
    pub available: Option<Decimal>,
    /// none / ok / warn / over
    pub level: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BudgetDto {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub base_currency: String,
    pub rows: Vec<BudgetRowDto>,
    /// Курсовая разница за период в базовой валюте («−» — потеряно на обмене); в расходы не входит
    #[schema(value_type = String)]
    pub fx_result: Decimal,
    pub missing_rates: Vec<String>,
}

#[utoipa::path(get, path = "/api/budget", tag = "Бюджет", params(ReportQuery), responses((status = 200, body = BudgetDto)), security(("bearer_auth" = [])))]
pub async fn report(State(svc): Svc, Extension(s): Extension<Session>, Query(q): Query<ReportQuery>) -> AppResult<Json<BudgetDto>> {
    let today = chrono::Local::now().date_naive();
    let period = Period::containing(View::parse(q.view.as_deref().unwrap_or("month")), q.date.unwrap_or(today));
    let r = svc.report(s.project_id, &s.base_currency, period, s.lang, q.member, today).await?;
    let rows = r
        .expenses
        .into_iter()
        .chain(r.incomes)
        .map(|x| BudgetRowDto {
            code: x.code,
            parent_code: x.parent_code,
            title: x.title,
            kind: x.kind,
            limit: x.limit,
            fact: x.fact,
            planned: x.planned,
            forecast: x.forecast,
            available: x.available,
            level: x.level.code().into(),
        })
        .collect();
    Ok(Json(BudgetDto { from: r.period.from, to: r.period.to, base_currency: r.base_currency, rows, fx_result: r.fx_result, missing_rates: r.missing_rates }))
}
