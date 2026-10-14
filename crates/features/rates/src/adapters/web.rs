//! Входящий адаптер: страница «Курсы валют».

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use kernel::web::{Flash, see_other_flash, Choice, PageCtx, PageError, PageResult, parse_money, render, see_other};
use serde::Deserialize;
use uuid::Uuid;

use crate::dto::{RateDto, RateInput};
use crate::service::RateService;

type Svc = State<Arc<RateService>>;

pub fn router(service: Arc<RateService>) -> Router {
    Router::new()
        .route("/", get(list).post(create)).route("/{id}/delete", post(delete))
        .route("/undo/{token}", post(undo)).with_state(service)
}

#[derive(Template)]
#[template(path = "rates/list.html")]
struct ListPage {
    page: PageCtx,
    rows: Vec<RateDto>,
    currencies: Vec<Choice>,
    today: chrono::NaiveDate,
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RateForm {
    rate_date: String,
    currency_code: String,
    rate: String,
}

async fn show(svc: &RateService, page: PageCtx, error: Option<String>) -> PageResult {
    let p = page.session.project_id;
    let rows = svc.all(p).await?;
    let currencies = svc.ref_choices(p, page.lang).await?;
    let page = page.for_feature(crate::NAME, crate::i18n::dict);
    render(&ListPage { page, rows, currencies, today: chrono::Local::now().date_naive(), error })
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    show(&svc, page, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<RateForm>) -> PageResult {
    let result = async {
        let input = RateInput {
            rate_date: f.rate_date.parse().map_err(|_| AppError::Validation("rate_date: неверная дата".into()))?,
            currency_code: f.currency_code.clone(),
            rate: parse_money("rate", &f.rate).map_err(AppError::Validation)?,
        };
        if input.rate <= rust_decimal::Decimal::ZERO {
            return Err(AppError::Validation("rate: должен быть больше нуля".into()));
        }
        svc.create(page.session.project_id, input).await
    }
    .await;
    match result {
        Ok(_) => Ok(see_other("/rates")),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => show(&svc, page, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

/// Удаление без «Вы уверены?»: тост с «Отменить» на 5 секунд.
async fn delete(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let token = svc.delete_with_undo(page.session.project_id, id).await?;
    Ok(see_other_flash(&page.config, "/rates", Flash::with_undo(page.t("toast.deleted"), format!("/rates/undo/{token}"))))
}

async fn undo(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(token): Path<Uuid>) -> PageResult {
    let flash = match svc.undo_delete(page.session.project_id, token).await {
        Ok(()) => Flash::ok(page.t("toast.restored")),
        Err(AppError::NotFound(_)) => Flash::error(page.t("toast.undo_expired")),
        Err(e) => return Err(PageError(e)),
    };
    Ok(see_other_flash(&page.config, "/rates", flash))
}
