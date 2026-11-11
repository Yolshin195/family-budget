//! Входящий адаптер: страница «Лимиты расходов».

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use kernel::web::{Choice, Flash, PageCtx, PageError, PageResult, parse_money, render, see_other, see_other_flash};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::{LimitLimitPeriod, rule_states};
use crate::dto::{LimitDto, LimitInput};
use crate::service::LimitService;

type Svc = State<Arc<LimitService>>;

pub fn router(service: Arc<LimitService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/remove", post(remove))
        .route("/{id}/delete", post(delete))
        .route("/undo/{token}", post(undo))
        .with_state(service)
}

struct Row {
    dto: LimitDto,
    category: String,
    /// Уровень категории в дереве (отступ строки).
    depth: usize,
    /// Правило уже не действует: его заменило более позднее правило этой категории.
    superseded: bool,
    /// Правило вступает в силу раньше `valid_from` — с начала своего периода.
    early: bool,
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
    // Порядок строк — как в дереве категорий; внутри категории новые (по «Действует с») сначала.
    let mut rows = Vec::new();
    for c in &categories {
        let mut mine: Vec<&LimitDto> = all.iter().filter(|l| l.category_code == c.code).collect();
        mine.sort_by_key(|l| std::cmp::Reverse(l.valid_from));
        let keys: Vec<_> = mine
            .iter()
            .map(|l| (LimitLimitPeriod::parse(&l.limit_period).unwrap_or(LimitLimitPeriod::Month), l.valid_from))
            .collect();
        for (l, state) in mine.into_iter().zip(rule_states(&keys, today)) {
            let early = l.effective_from < l.valid_from;
            let superseded = state.superseded;
            rows.push(Row { dto: l.clone(), category: c.label.clone(), depth: c.depth.min(5), superseded, early });
        }
    }
    let page = page.for_feature(crate::NAME, crate::i18n::dict);
    render(&ListPage { page, rows, categories, today, error })
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    show(&svc, page, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<LimitForm>) -> PageResult {
    save(svc, page, f, false).await
}

/// «Снять лимит с даты»: правило без суммы — дальше лимита нет, прошлые периоды его сохраняют.
async fn remove(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<LimitForm>) -> PageResult {
    save(svc, page, f, true).await
}

async fn save(svc: Arc<LimitService>, page: PageCtx, f: LimitForm, removed: bool) -> PageResult {
    let result = async {
        let input = LimitInput {
            valid_from: f.valid_from.parse().map_err(|_| AppError::Validation("valid_from: неверная дата".into()))?,
            category_code: f.category_code.clone(),
            limit_period: f.limit_period.clone(),
            amount: if removed { None } else { Some(parse_money("amount", &f.amount).map_err(AppError::Validation)?) },
            removed,
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

/// Удаление без «Вы уверены?»: тост с «Отменить» на 5 секунд.
async fn delete(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let token = svc.delete_with_undo(page.session.project_id, id).await?;
    Ok(see_other_flash(
        &page.config,
        "/limits",
        Flash::with_undo(page.t("toast.deleted"), format!("/limits/undo/{token}")),
    ))
}

async fn undo(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(token): Path<Uuid>) -> PageResult {
    let flash = match svc.undo_delete(page.session.project_id, token).await {
        Ok(()) => Flash::ok(page.t("toast.restored")),
        Err(AppError::NotFound(_)) => Flash::error(page.t("toast.undo_expired")),
        Err(e) => return Err(PageError(e)),
    };
    Ok(see_other_flash(&page.config, "/limits", flash))
}
