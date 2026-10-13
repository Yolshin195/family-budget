//! Входящий адаптер: веб-страницы фичи «Счета».

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use kernel::web::{Flash, see_other_flash, Choice, PageCtx, PageError, PageResult, parse_money, render, see_other};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::AccountAccountKind;
use crate::dto::{AccountDto, CreateAccountDto, UpdateAccountDto};
use crate::service::AccountService;

type Svc = State<Arc<AccountService>>;

pub fn router(service: Arc<AccountService>) -> Router {
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
#[template(path = "accounts/list.html")]
struct ListPage {
    page: PageCtx,
    rows: Vec<AccountDto>,
}

#[derive(Template)]
#[template(path = "accounts/form.html")]
struct FormPage {
    page: PageCtx,
    is_new: bool,
    f: AccountForm,
    kinds: &'static [&'static str],
    currencies: Vec<Choice>,
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AccountForm {
    code: String,
    name: String,
    name_lang: String,
    name_ru: String,
    name_en: String,
    name_th: String,
    account_kind: String,
    currency_code: String,
    opening_balance: String,
    holder_user_id: String,
}

fn tr(v: &str) -> Option<String> {
    Some(v.to_string()).filter(|v| !v.trim().is_empty())
}

impl AccountForm {
    fn from_dto(a: AccountDto) -> Self {
        Self {
            code: a.code,
            name: a.name,
            name_lang: a.name_lang,
            name_ru: a.name_ru.unwrap_or_default(),
            name_en: a.name_en.unwrap_or_default(),
            name_th: a.name_th.unwrap_or_default(),
            account_kind: a.account_kind,
            currency_code: a.currency_code,
            opening_balance: a.opening_balance.to_string(),
            holder_user_id: a.holder_user_id.map(|u| u.to_string()).unwrap_or_default(),
        }
    }

    fn holder(&self) -> Result<Option<Uuid>, AppError> {
        if self.holder_user_id.trim().is_empty() {
            return Ok(None);
        }
        Uuid::parse_str(self.holder_user_id.trim()).map(Some).map_err(|_| AppError::Validation("holder_user_id: неверный id".into()))
    }
}

async fn form_page(svc: &AccountService, page: PageCtx, is_new: bool, f: AccountForm, error: Option<String>) -> PageResult {
    let currencies = svc.currency_choices(page.session.project_id, page.lang).await?;
    render(&FormPage { page: form_ctx(page), is_new, f, kinds: &AccountAccountKind::VALUES, currencies, error })
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let rows = svc.all(page.session.project_id, true).await?;
    render(&ListPage { page: ctx(page), rows })
}

async fn new_form(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let f = AccountForm {
        name_lang: page.lang.upper().into(),
        account_kind: "CARD".into(),
        currency_code: page.session.base_currency.clone(),
        opening_balance: "0".into(),
        ..Default::default()
    };
    form_page(&svc, page, true, f, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<AccountForm>) -> PageResult {
    let result = async {
        let dto = CreateAccountDto {
            code: f.code.clone(),
            name: f.name.clone(),
            name_lang: f.name_lang.clone(),
            name_ru: tr(&f.name_ru),
            name_en: tr(&f.name_en),
            name_th: tr(&f.name_th),
            account_kind: f.account_kind.clone(),
            currency_code: f.currency_code.clone(),
            opening_balance: parse_money("opening_balance", &f.opening_balance).map_err(AppError::Validation)?,
            holder_user_id: f.holder()?,
        };
        svc.create(page.session.project_id, dto).await
    }
    .await;
    match result {
        Ok(_) => Ok(see_other("/accounts")),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, true, f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    let a = svc.get(page.session.project_id, &code).await?;
    form_page(&svc, page, false, AccountForm::from_dto(a), None).await
}

async fn update(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>, Form(mut f): Form<AccountForm>) -> PageResult {
    f.code = code.clone();
    let result = async {
        let archived = svc.get(page.session.project_id, &code).await?.archived;
        let dto = UpdateAccountDto {
            name: f.name.clone(),
            archived,
            name_lang: f.name_lang.clone(),
            name_ru: tr(&f.name_ru),
            name_en: tr(&f.name_en),
            name_th: tr(&f.name_th),
            account_kind: f.account_kind.clone(),
            currency_code: f.currency_code.clone(),
            opening_balance: parse_money("opening_balance", &f.opening_balance).map_err(AppError::Validation)?,
            holder_user_id: f.holder()?,
        };
        svc.update(page.session.project_id, &code, dto).await
    }
    .await;
    match result {
        Ok(_) => Ok(see_other("/accounts")),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, false, f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

/// Архив без «Вы уверены?»: тост с «Отменить» на 5 секунд.
async fn archive(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    svc.archive(page.session.project_id, &code).await?;
    let flash = Flash::with_undo(format!("{} · {code}", page.t("toast.archived")), format!("/accounts/{}/restore", kernel::web::url_encode(&code)));
    Ok(see_other_flash(&page.config, "/accounts", flash))
}

async fn restore(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    svc.restore(page.session.project_id, &code).await?;
    Ok(see_other_flash(&page.config, "/accounts", Flash::ok(format!("{} · {code}", page.t("toast.restored")))))
}
