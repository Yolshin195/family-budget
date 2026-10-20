//! Входящий адаптер: веб-страницы фичи «Люди» (список и форма).

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use kernel::web::{Flash, PageCtx, PageError, PageResult, render, see_other, see_other_flash, url_segment};
use serde::Deserialize;

use crate::domain::split_aliases;
use crate::dto::{CreatePersonDto, PersonDto, PersonFields};
use crate::service::PersonService;

type Svc = State<Arc<PersonService>>;

pub fn router(service: Arc<PersonService>) -> Router {
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
#[template(path = "people/list.html")]
struct ListPage {
    page: PageCtx,
    rows: Vec<PersonDto>,
}

impl ListPage {
    fn title(&self, p: &PersonDto) -> String {
        p.title(self.page.lang)
    }

    /// Запись того, кто смотрит, — в выборе она «Для себя».
    fn is_me(&self, p: &PersonDto) -> bool {
        p.user_id == Some(self.page.session.user_id)
    }
}

#[derive(Template)]
#[template(path = "people/form.html")]
struct FormPage {
    page: PageCtx,
    is_new: bool,
    f: PersonForm,
    /// Запись участника проекта: в архив уходит сама, когда он выходит из проекта.
    member: bool,
    /// Удалённый участник: пустое имя — «Удалённый участник».
    former: bool,
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PersonForm {
    code: String,
    name: String,
    /// Через запятую или с новой строки.
    aliases: String,
}

impl PersonForm {
    fn from_dto(p: PersonDto) -> Self {
        Self { code: p.code, name: p.name, aliases: p.aliases.join(", ") }
    }

    fn fields(&self) -> PersonFields {
        PersonFields { name: self.name.clone(), aliases: split_aliases(&self.aliases) }
    }
}

async fn form_page(svc: &PersonService, page: PageCtx, is_new: bool, f: PersonForm, error: Option<String>) -> PageResult {
    let (member, former) = match is_new {
        true => (false, false),
        false => svc.get(page.session.project_id, &f.code).await.map(|p| (p.is_member(), p.former_member)).unwrap_or_default(),
    };
    render(&FormPage { page: form_ctx(page), is_new, f, member, former, error })
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let rows = svc.all(page.session.project_id, true).await?;
    render(&ListPage { page: ctx(page), rows })
}

async fn new_form(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    form_page(&svc, page, true, PersonForm::default(), None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<PersonForm>) -> PageResult {
    match svc.create(page.session.project_id, CreatePersonDto { code: f.code.clone(), data: f.fields() }).await {
        Ok(_) => Ok(see_other(crate::MOUNT)),
        Err(e @ (AppError::Validation(_) | AppError::Invalid(_) | AppError::Conflict(_))) => form_page(&svc, page, true, f, Some(e.to_string())).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    let p = svc.get(page.session.project_id, &code).await?;
    form_page(&svc, page, false, PersonForm::from_dto(p), None).await
}

async fn update(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>, Form(mut f): Form<PersonForm>) -> PageResult {
    f.code = code.clone();
    match svc.update(page.session.project_id, &code, f.fields()).await {
        Ok(_) => Ok(see_other(crate::MOUNT)),
        Err(e @ (AppError::Validation(_) | AppError::Invalid(_))) => form_page(&svc, page, false, f, Some(e.to_string())).await,
        Err(e) => Err(PageError(e)),
    }
}

/// Архив без «Вы уверены?»: тост с «Отменить».
async fn archive(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    match svc.archive(page.session.project_id, &code).await {
        Ok(()) => {
            let flash = Flash::with_undo(format!("{} · {code}", page.t("toast.archived")), format!("{}/{}/restore", crate::MOUNT, url_segment(&code)));
            Ok(see_other_flash(&page.config, crate::MOUNT, flash))
        }
        Err(e @ (AppError::Validation(_) | AppError::Invalid(_))) => Ok(see_other_flash(&page.config, crate::MOUNT, Flash::error(kernel::web::FormError::parse(&e.to_string()).message))),
        Err(e) => Err(PageError(e)),
    }
}

async fn restore(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    svc.restore(page.session.project_id, &code).await?;
    Ok(see_other_flash(&page.config, crate::MOUNT, Flash::ok(format!("{} · {code}", page.t("toast.restored")))))
}
