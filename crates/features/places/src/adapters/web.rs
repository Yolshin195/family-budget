//! Входящий адаптер: веб-страницы фичи «Места» (список и форма).

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use kernel::web::{Choice, Flash, PageCtx, PageError, PageResult, render, see_other, see_other_flash, url_segment};
use serde::Deserialize;

use crate::domain::{PlaceKind, split_aliases};
use crate::dto::{CreatePlaceDto, PlaceDto, PlaceFields};
use crate::service::PlaceService;

type Svc = State<Arc<PlaceService>>;

pub fn router(service: Arc<PlaceService>) -> Router {
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
#[template(path = "places/list.html")]
struct ListPage {
    page: PageCtx,
    rows: Vec<PlaceDto>,
    categories: Vec<Choice>,
    accounts: Vec<Choice>,
}

impl ListPage {
    /// «Продукты · Наличные» — что место подставит; код, если записи уже нет среди активных.
    fn fills(&self, p: &PlaceDto) -> String {
        let label = |list: &[Choice], code: &Option<String>| {
            code.as_ref().map(|c| list.iter().find(|x| x.code == *c).map(|x| x.label.clone()).unwrap_or_else(|| c.clone()))
        };
        [label(&self.categories, &p.default_category_code), label(&self.accounts, &p.default_account_code)].into_iter().flatten().collect::<Vec<_>>().join(" · ")
    }
}

#[derive(Template)]
#[template(path = "places/form.html")]
struct FormPage {
    page: PageCtx,
    is_new: bool,
    f: PlaceForm,
    kinds: &'static [&'static str],
    categories: Vec<Choice>,
    accounts: Vec<Choice>,
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PlaceForm {
    code: String,
    name: String,
    /// Через запятую или с новой строки.
    aliases: String,
    default_category_code: String,
    default_account_code: String,
    place_kind: String,
    location: String,
}

fn opt(v: &str) -> Option<String> {
    Some(v.trim().to_string()).filter(|v| !v.is_empty())
}

impl PlaceForm {
    fn from_dto(p: PlaceDto) -> Self {
        Self {
            code: p.code,
            name: p.name,
            aliases: p.aliases.join(", "),
            default_category_code: p.default_category_code.unwrap_or_default(),
            default_account_code: p.default_account_code.unwrap_or_default(),
            place_kind: p.place_kind,
            location: p.location.unwrap_or_default(),
        }
    }

    fn fields(&self) -> PlaceFields {
        PlaceFields {
            name: self.name.clone(),
            aliases: split_aliases(&self.aliases),
            default_category_code: opt(&self.default_category_code),
            default_account_code: opt(&self.default_account_code),
            place_kind: self.place_kind.clone(),
            location: opt(&self.location),
        }
    }
}

async fn form_page(svc: &PlaceService, page: PageCtx, is_new: bool, f: PlaceForm, error: Option<String>) -> PageResult {
    let (categories, accounts) = svc.choices(page.session.project_id, page.lang).await?;
    let categories = categories.into_iter().filter(|c| c.tag == "EXPENSE" || c.code == f.default_category_code).collect();
    render(&FormPage { page: form_ctx(page), is_new, f, kinds: PlaceKind::VALUES, categories, accounts, error })
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let rows = svc.all(page.session.project_id, true).await?;
    let (categories, accounts) = svc.choices(page.session.project_id, page.lang).await?;
    render(&ListPage { page: ctx(page), rows, categories, accounts })
}

async fn new_form(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let f = PlaceForm { place_kind: PlaceKind::default().as_str().into(), ..Default::default() };
    form_page(&svc, page, true, f, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<PlaceForm>) -> PageResult {
    match svc.create(page.session.project_id, CreatePlaceDto { code: f.code.clone(), data: f.fields() }).await {
        Ok(_) => Ok(see_other(crate::MOUNT)),
        Err(e @ (AppError::Validation(_) | AppError::Invalid(_) | AppError::Conflict(_))) => form_page(&svc, page, true, f, Some(e.to_string())).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    let p = svc.get(page.session.project_id, &code).await?;
    form_page(&svc, page, false, PlaceForm::from_dto(p), None).await
}

async fn update(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>, Form(mut f): Form<PlaceForm>) -> PageResult {
    f.code = code.clone();
    match svc.update(page.session.project_id, &code, f.fields()).await {
        Ok(_) => Ok(see_other(crate::MOUNT)),
        Err(e @ (AppError::Validation(_) | AppError::Invalid(_))) => form_page(&svc, page, false, f, Some(e.to_string())).await,
        Err(e) => Err(PageError(e)),
    }
}

/// Архив без «Вы уверены?»: тост с «Отменить».
async fn archive(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    svc.archive(page.session.project_id, &code).await?;
    let flash = Flash::with_undo(format!("{} · {code}", page.t("toast.archived")), format!("{}/{}/restore", crate::MOUNT, url_segment(&code)));
    Ok(see_other_flash(&page.config, crate::MOUNT, flash))
}

async fn restore(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    svc.restore(page.session.project_id, &code).await?;
    Ok(see_other_flash(&page.config, crate::MOUNT, Flash::ok(format!("{} · {code}", page.t("toast.restored")))))
}
