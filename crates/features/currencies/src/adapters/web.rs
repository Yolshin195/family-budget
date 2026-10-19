//! Входящий адаптер: веб-страницы фичи «Валюты» (Askama + Tailwind).

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use kernel::web::{Flash, see_other_flash, PageCtx, PageError, PageResult, render, see_other};
use serde::Deserialize;

use crate::dto::{CreateCurrencyDto, CurrencyDto, UpdateCurrencyDto};
use crate::service::CurrencyService;

type Svc = State<Arc<CurrencyService>>;

pub fn router(service: Arc<CurrencyService>) -> Router {
    Router::new()
        .route("/", get(list))
        .route("/new", get(new_form).post(create))
        .route("/{code}/edit", get(edit_form).post(update))
        .route("/{code}/archive", post(archive))
        .route("/{code}/restore", post(restore))
        .with_state(service)
}

fn ctx(page: PageCtx) -> PageCtx {
    page.for_feature(crate::NAME, crate::i18n::dict)
}

/// Вложенная страница (форма): хлебная крошка на список.
fn form_ctx(page: PageCtx) -> PageCtx {
    let page = ctx(page);
    let title = page.t("title");
    page.crumb(title, crate::MOUNT)
}

#[derive(Template)]
#[template(path = "currencies/list.html")]
struct ListPage {
    page: PageCtx,
    rows: Vec<CurrencyDto>,
}

#[derive(Template)]
#[template(path = "currencies/form.html")]
struct FormPage {
    page: PageCtx,
    is_new: bool,
    f: CurrencyForm,
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CurrencyForm {
    code: String,
    name: String,
    name_lang: String,
    name_ru: String,
    name_en: String,
    name_th: String,
    symbol: String,
    minor_units: String,
}

impl CurrencyForm {
    fn from_dto(c: CurrencyDto) -> Self {
        Self {
            code: c.code,
            name: c.name,
            name_lang: c.name_lang,
            name_ru: c.name_ru.unwrap_or_default(),
            name_en: c.name_en.unwrap_or_default(),
            name_th: c.name_th.unwrap_or_default(),
            symbol: c.symbol,
            minor_units: c.minor_units.to_string(),
        }
    }

    fn minor(&self) -> Result<i32, AppError> {
        self.minor_units
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|v| (0..=4).contains(v))
            .ok_or_else(|| AppError::Validation("minor_units: целое от 0 до 4".into()))
    }

    fn tr(v: &str) -> Option<String> {
        Some(v.to_string()).filter(|v| !v.trim().is_empty())
    }
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let rows = svc.all(page.session.project_id, true).await?;
    render(&ListPage { page: ctx(page), rows })
}

async fn new_form(Extension(page): Extension<PageCtx>) -> PageResult {
    let f = CurrencyForm { name_lang: page.lang.upper().into(), minor_units: "2".into(), ..Default::default() };
    render(&FormPage { page: form_ctx(page), is_new: true, f, error: None })
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<CurrencyForm>) -> PageResult {
    let result = async {
        let dto = CreateCurrencyDto {
            code: f.code.clone(),
            name: f.name.clone(),
            name_lang: f.name_lang.clone(),
            name_ru: CurrencyForm::tr(&f.name_ru),
            name_en: CurrencyForm::tr(&f.name_en),
            name_th: CurrencyForm::tr(&f.name_th),
            symbol: f.symbol.clone(),
            minor_units: f.minor()?,
        };
        svc.create(page.session.project_id, dto).await
    }
    .await;
    match result {
        Ok(_) => Ok(see_other("/currencies")),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => render(&FormPage { page: form_ctx(page), is_new: true, f, error: Some(m) }),
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    let c = svc.get(page.session.project_id, &code).await?;
    render(&FormPage { page: form_ctx(page), is_new: false, f: CurrencyForm::from_dto(c), error: None })
}

async fn update(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>, Form(mut f): Form<CurrencyForm>) -> PageResult {
    f.code = code.clone();
    let result = async {
        let archived = svc.get(page.session.project_id, &code).await?.archived;
        let dto = UpdateCurrencyDto {
            name: f.name.clone(),
            archived,
            name_lang: f.name_lang.clone(),
            name_ru: CurrencyForm::tr(&f.name_ru),
            name_en: CurrencyForm::tr(&f.name_en),
            name_th: CurrencyForm::tr(&f.name_th),
            symbol: f.symbol.clone(),
            minor_units: f.minor()?,
        };
        svc.update(page.session.project_id, &code, dto).await
    }
    .await;
    match result {
        Ok(_) => Ok(see_other("/currencies")),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => render(&FormPage { page: form_ctx(page), is_new: false, f, error: Some(m) }),
        Err(e) => Err(PageError(e)),
    }
}

/// Архив без «Вы уверены?»: тост с «Отменить» на 5 секунд.
async fn archive(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    // Базовая валюта и валюта активных счетов в архив не уходят: тост с причиной.
    match svc.archive(page.session.project_id, &code).await {
        Ok(()) => {}
        Err(AppError::Conflict(m)) => {
            let text = m.split_once(": ").map(|x| x.1.to_string()).unwrap_or(m);
            return Ok(see_other_flash(&page.config, &format!("/currencies/{}/edit", kernel::web::url_encode(&code)), Flash::error(text)));
        }
        Err(e) => return Err(PageError(e)),
    }
    let flash = Flash::with_undo(format!("{} · {code}", page.t("toast.archived")), format!("/currencies/{}/restore", kernel::web::url_encode(&code)));
    Ok(see_other_flash(&page.config, "/currencies", flash))
}

async fn restore(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    svc.restore(page.session.project_id, &code).await?;
    Ok(see_other_flash(&page.config, "/currencies", Flash::ok(format!("{} · {code}", page.t("toast.restored")))))
}
