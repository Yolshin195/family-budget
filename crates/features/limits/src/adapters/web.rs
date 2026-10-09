//! Входящий адаптер: страница «Лимиты расходов».

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use kernel::web::{Choice, PageCtx, PageError, PageResult, parse_money, render, see_other};
use serde::Deserialize;
use uuid::Uuid;

use crate::dto::{LimitDto, LimitInput};
use crate::service::LimitService;

type Svc = State<Arc<LimitService>>;

pub fn router(service: Arc<LimitService>) -> Router {
    Router::new().route("/", get(list).post(create)).route("/{id}/delete", post(delete)).with_state(service)
}

struct Row {
    dto: LimitDto,
    category: String,
    indent: String,
    /// Есть более поздний лимит этой категории, уже вступивший в силу.
    superseded: bool,
}

#[derive(Template)]
#[template(path = "limits/list.html")]
struct ListPage {
    page: PageCtx,
    rows: Vec<Row>,
    categories: Vec<Choice>,
    today: chrono::NaiveDate,
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LimitForm {
    valid_from: String,
    category_code: String,
    limit_period: String,
    amount: String,
}

async fn show(svc: &LimitService, page: PageCtx, error: Option<String>) -> PageResult {
    let p = page.session.project_id;
    let today = chrono::Local::now().date_naive();
    let categories = svc.ref_choices(p, page.lang).await?;
    let all = svc.all(p).await?;
    // Порядок строк — как в дереве категорий; внутри категории новые сначала.
    let mut rows = Vec::new();
    for c in &categories {
        let mut mine: Vec<&LimitDto> = all.iter().filter(|l| l.category_code == c.code).collect();
        mine.sort_by(|a, b| b.valid_from.cmp(&a.valid_from));
        let current = mine.iter().find(|l| l.valid_from <= today).map(|l| l.id);
        for l in mine {
            let superseded = l.valid_from <= today && Some(l.id) != current;
            rows.push(Row { dto: clone(l), category: c.label.clone(), indent: c.indent(), superseded });
        }
    }
    let page = page.for_feature(crate::NAME, crate::i18n::dict);
    render(&ListPage { page, rows, categories, today, error })
}

fn clone(l: &LimitDto) -> LimitDto {
    LimitDto { id: l.id, valid_from: l.valid_from, category_code: l.category_code.clone(), limit_period: l.limit_period.clone(), amount: l.amount }
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    show(&svc, page, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<LimitForm>) -> PageResult {
    let result = async {
        let input = LimitInput {
            valid_from: f.valid_from.parse().map_err(|_| AppError::Validation("valid_from: неверная дата".into()))?,
            category_code: f.category_code.clone(),
            limit_period: f.limit_period.clone(),
            amount: parse_money("amount", &f.amount).map_err(AppError::Validation)?,
        };
        svc.create(page.session.project_id, input).await
    }
    .await;
    match result {
        Ok(_) => Ok(see_other("/limits")),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => show(&svc, page, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn delete(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    svc.delete(page.session.project_id, id).await?;
    Ok(see_other("/limits"))
}
