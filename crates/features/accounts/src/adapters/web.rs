//! Входящий адаптер: веб-страницы фичи «Счета».

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use std::collections::HashMap;

use chrono::NaiveDate;
use kernel::web::{
    Choice, Flash, PageCtx, PageError, PageResult, money_in, parse_money, render, see_other, see_other_flash,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::AccountAccountKind;
use crate::dto::{AccountDto, CreateAccountDto, UpdateAccountDto};
use crate::service::{AccountService, BankCheck};

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
    rows: Vec<Row>,
}

struct Row {
    a: AccountDto,
    /// Текущий остаток (сумма проводок на сегодня) и признак «меньше нуля».
    balance: String,
    negative: bool,
    bank: Option<BankView>,
}

/// Остаток по данным банка (последнее SMS) и сверка с операциями на тот же момент.
struct BankView {
    amount: String,
    /// «25.10 16:25»
    at: String,
    /// Расхождение словами («В операциях на 30,00 ฿ больше, чем в банке»); пусто — сходится.
    diff: Option<String>,
    /// «Сверить остаток» с остатком банка и его датой.
    reconcile: String,
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
    /// Начальный остаток — транзакция `OPENING` в журнале операций.
    opening_balance: String,
    opening_date: String,
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
            opening_balance: "0".into(),
            opening_date: today().to_string(),
            holder_user_id: a.holder_user_id.map(|u| u.to_string()).unwrap_or_default(),
        }
    }

    fn opening(&self) -> Result<(Decimal, NaiveDate), AppError> {
        let amount = if self.opening_balance.trim().is_empty() {
            Decimal::ZERO
        } else {
            parse_money("opening_balance", &self.opening_balance).map_err(AppError::Validation)?
        };
        let date = self.opening_date.parse().map_err(|_| AppError::Validation("opening_date: неверная дата".into()))?;
        Ok((amount, date))
    }

    fn holder(&self) -> Result<Option<Uuid>, AppError> {
        if self.holder_user_id.trim().is_empty() {
            return Ok(None);
        }
        Uuid::parse_str(self.holder_user_id.trim())
            .map(Some)
            .map_err(|_| AppError::Validation("holder_user_id: неверный id".into()))
    }
}

async fn form_page(
    svc: &AccountService,
    page: PageCtx,
    is_new: bool,
    f: AccountForm,
    error: Option<String>,
) -> PageResult {
    let currencies = svc.currency_choices(page.session.project_id, page.lang).await?;
    render(&FormPage { page: form_ctx(page), is_new, f, kinds: AccountAccountKind::VALUES, currencies, error })
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let p = page.session.project_id;
    let page = ctx(page);
    let balances: HashMap<String, Decimal> = svc.balances(p, today()).await?.into_iter().collect();
    let mut rows = Vec::new();
    for a in svc.all(p, true).await? {
        let b = balances.get(&a.code).copied().unwrap_or_default();
        let bank = svc.bank_check(p, &a).await?.map(|c| bank_view(&page, &a, &c));
        rows.push(Row { balance: money_in(page.lang, b), negative: b < Decimal::ZERO, bank, a });
    }
    render(&ListPage { page, rows })
}

fn bank_view(page: &PageCtx, a: &AccountDto, c: &BankCheck) -> BankView {
    let at = c.bank.at.with_timezone(&chrono::Local);
    let diff = c.diff();
    let amount = format!("{}\u{a0}{}", money_in(page.lang, diff.abs()), page.cur(&a.currency_code));
    BankView {
        amount: money_in(page.lang, c.bank.amount),
        at: at.format("%d.%m %H:%M").to_string(),
        diff: match diff.cmp(&Decimal::ZERO) {
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(page.t("bank_more").replace("{amount}", &amount)),
            std::cmp::Ordering::Less => Some(page.t("bank_less").replace("{amount}", &amount)),
        },
        reconcile: format!(
            "/transactions/reconcile?account={}&occurred_on={}&actual={}",
            kernel::web::url_encode(&a.code),
            at.date_naive(),
            c.bank.amount.normalize()
        ),
    }
}

async fn new_form(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let f = AccountForm {
        name_lang: page.lang.upper().into(),
        account_kind: "CARD".into(),
        currency_code: page.session.base_currency.clone(),
        opening_balance: "0".into(),
        opening_date: today().to_string(),
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
            holder_user_id: f.holder()?,
        };
        let (amount, date) = f.opening()?;
        let s = page.session.scope();
        let a = svc.create(s.project_id, dto).await?;
        Ok(svc.set_opening(s.project_id, s.user_id, &a.code, amount, date).await.err().map(|e| (a.code, e)))
    }
    .await;
    match result {
        Ok(None) => Ok(see_other("/accounts")),
        // Счёт создан, а начальный остаток не принят: дальше — форма редактирования с ошибкой.
        Ok(Some((code, AppError::Validation(m)))) => {
            let text = m.split_once(": ").map(|x| x.1.to_string()).unwrap_or(m);
            Ok(see_other_flash(
                &page.config,
                &format!("/accounts/{}/edit", kernel::web::url_segment(&code)),
                Flash::error(text),
            ))
        }
        Ok(Some((_, e))) => Err(PageError(e)),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, true, f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    let p = page.session.project_id;
    let a = svc.get(p, &code).await?;
    let mut f = AccountForm::from_dto(a);
    if let Some((amount, date)) = svc.opening(p, &code).await? {
        f.opening_balance = amount.normalize().to_string();
        f.opening_date = date.to_string();
    }
    form_page(&svc, page, false, f, None).await
}

async fn update(
    State(svc): Svc,
    Extension(page): Extension<PageCtx>,
    Path(code): Path<String>,
    Form(mut f): Form<AccountForm>,
) -> PageResult {
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
            holder_user_id: f.holder()?,
        };
        let (amount, date) = f.opening()?;
        let s = page.session.scope();
        svc.update(s.project_id, &code, dto).await?;
        svc.set_opening(s.project_id, s.user_id, &code, amount, date).await
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
    // Счёт с остатком в архив не уходит: тост с причиной.
    match svc.archive(page.session.project_id, &code).await {
        Ok(()) => {}
        Err(AppError::Conflict(m)) => {
            let text = m.split_once(": ").map(|x| x.1.to_string()).unwrap_or(m);
            return Ok(see_other_flash(
                &page.config,
                &format!("/accounts/{}/edit", kernel::web::url_segment(&code)),
                Flash::error(text),
            ));
        }
        Err(e) => return Err(PageError(e)),
    }
    let flash = Flash::with_undo(
        format!("{} · {code}", page.t("toast.archived")),
        format!("/accounts/{}/restore", kernel::web::url_segment(&code)),
    );
    Ok(see_other_flash(&page.config, "/accounts", flash))
}

async fn restore(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(code): Path<String>) -> PageResult {
    svc.restore(page.session.project_id, &code).await?;
    Ok(see_other_flash(&page.config, "/accounts", Flash::ok(format!("{} · {code}", page.t("toast.restored")))))
}
