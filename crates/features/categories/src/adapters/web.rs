//! Входящий адаптер: веб-страницы фичи «Категории» (дерево).

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use kernel::web::{Flash, see_other_flash, Choice, PageCtx, PageError, PageResult, render, see_other};
use serde::Deserialize;

use crate::dto::{CategoryDto, CreateCategoryDto, UpdateCategoryDto};
use crate::service::CategoryService;

type Svc = State<Arc<CategoryService>>;

pub fn router(service: Arc<CategoryService>) -> Router {
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
#[template(path = "categories/list.html")]
struct ListPage {
    page: PageCtx,
    rows: Vec<(usize, CategoryDto)>,
}

#[derive(Template)]
#[template(path = "categories/form.html")]
struct FormPage {
    page: PageCtx,
    is_new: bool,
    f: CategoryForm,
    parents: Vec<Choice>,
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CategoryForm {
    code: String,
    name: String,
    name_lang: String,
    name_ru: String,
    name_en: String,
    name_th: String,
    category_kind: String,
    parent_code: String,
    sort_order: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct NewQuery {
    parent: String,
    kind: String,
}

fn tr(v: &str) -> Option<String> {
    Some(v.to_string()).filter(|v| !v.trim().is_empty())
}

impl CategoryForm {
    fn from_dto(c: CategoryDto) -> Self {
        Self {
            code: c.code,
            name: c.name,
            name_lang: c.name_lang,
            name_ru: c.name_ru.unwrap_or_default(),
            name_en: c.name_en.unwrap_or_default(),
            name_th: c.name_th.unwrap_or_default(),
            category_kind: c.category_kind,
            parent_code: c.parent_code.unwrap_or_default(),
            sort_order: c.sort_order.to_string(),
        }
    }

    fn order(&self) -> Result<i32, AppError> {
        let v = self.sort_order.trim();
        if v.is_empty() {
            return Ok(0);
        }
        v.parse().map_err(|_| AppError::Validation("sort_order: целое число".into()))
    }
}

async fn form_page(svc: &CategoryService, page: PageCtx, is_new: bool, f: CategoryForm, error: Option<String>) -> PageResult {
    let parents = svc.choices(page.session.project_id, page.lang).await?;
    render(&FormPage { page: form_ctx(page), is_new, f, parents, error })
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let rows = svc.tree(page.session.project_id, true).await?;
    render(&ListPage { page: ctx(page), rows })
}

async fn new_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(q): Query<NewQuery>) -> PageResult {
    let f = CategoryForm {
        name_lang: page.lang.upper().into(),
        category_kind: if q.kind == "INCOME" { "INCOME".into() } else { "EXPENSE".into() },
        parent_code: q.parent,
        sort_order: "10".into(),
        ..Default::default()
    };
    form_page(&svc, page, true, f, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<CategoryForm>) -> PageResult {
    let result = async {
        let dto = CreateCategoryDto {
            code: f.code.clone(),
            name: f.name.clone(),
            name_lang: f.name_lang.clone(),
            name_ru: tr(&f.name_ru),
            name_en: tr(&f.name_en),
            name_th: tr(&f.name_th),
            category_kind: f.category_kind.clone(),
            parent_code: tr(&f.parent_code),
            sort_order: f.order()?,
        };
        svc.create(page.session.project_id, dto).await
    }
    .await;
    match result {
        Ok(_) => Ok(see_other("/categories")),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, true, f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    let c = svc.get(page.session.project_id, &code).await?;
    form_page(&svc, page, false, CategoryForm::from_dto(c), None).await
}

async fn update(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>, Form(mut f): Form<CategoryForm>) -> PageResult {
    f.code = code.clone();
    let result = async {
        let archived = svc.get(page.session.project_id, &code).await?.archived;
        let dto = UpdateCategoryDto {
            name: f.name.clone(),
            archived,
            name_lang: f.name_lang.clone(),
            name_ru: tr(&f.name_ru),
            name_en: tr(&f.name_en),
            name_th: tr(&f.name_th),
            category_kind: f.category_kind.clone(),
            parent_code: tr(&f.parent_code),
            sort_order: f.order()?,
        };
        svc.update(page.session.project_id, &code, dto).await
    }
    .await;
    match result {
        Ok(_) => Ok(see_other("/categories")),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, false, f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

/// Архив без «Вы уверены?»: тост с «Отменить» на 5 секунд.
async fn archive(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    match svc.archive(page.session.project_id, &code).await {
        Ok(()) => {
            let flash = Flash::with_undo(format!("{} · {code}", page.t("toast.archived")), format!("/categories/{}/restore", kernel::web::url_segment(&code)));
            Ok(see_other_flash(&page.config, "/categories", flash))
        }
        Err(AppError::Validation(m)) => {
            let c = svc.get(page.session.project_id, &code).await?;
            form_page(&svc, page, false, CategoryForm::from_dto(c), Some(m)).await
        }
        Err(e) => Err(PageError(e)),
    }
}

async fn restore(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    match svc.restore(page.session.project_id, &code).await {
        Ok(()) => Ok(see_other_flash(&page.config, "/categories", Flash::ok(format!("{} · {code}", page.t("toast.restored"))))),
        Err(AppError::Validation(m)) => Ok(see_other_flash(&page.config, "/categories", Flash::error(m))),
        Err(e) => Err(PageError(e)),
    }
}
