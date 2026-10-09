//! Входящий адаптер: веб-страницы фичи «Транзакции».

use std::collections::HashMap;
use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Extension, Router};
use axum_extra::extract::Form;
use chrono::{Datelike, Months, NaiveDate};
use kernel::error::AppError;
use kernel::i18n::{Lang, month_name, weekdays};
use kernel::pagination::Pagination;
use kernel::web::{Choice, PageCtx, PageError, PageResult, money, parse_money, render, see_other};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::dto::{LineDto, ListQuery, TransactionDto, TransactionInput, TranslationInput};
use crate::service::TransactionService;

type Svc = State<Arc<TransactionService>>;

pub fn router(service: Arc<TransactionService>) -> Router {
    Router::new()
        .route("/", get(list))
        .route("/new", get(new_form).post(create))
        .route("/{id}/edit", get(edit_form).post(update))
        .route("/{id}/delete", post(delete))
        .route("/{id}/translate", get(translate_form).post(translate))
        .with_state(service)
}

fn ctx(page: PageCtx) -> PageCtx {
    page.for_feature(crate::NAME, crate::i18n::dict)
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Подпись дня: «Пт 09.10».
fn day_label(lang: Lang, d: NaiveDate) -> String {
    format!("{} {:02}.{:02}", weekdays(lang)[d.weekday().num_days_from_monday() as usize], d.day(), d.month())
}

// ---------------------------------------------------------------- список

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ListParams {
    /// YYYY-MM
    month: String,
    account: String,
    member: String,
    untranslated: String,
}

struct LineView {
    category: String,
    amount: String,
    note: String,
}

struct TxView {
    dto: TransactionDto,
    currency: String,
    desc: String,
    missing: bool,
    account: String,
    author: String,
    lines: Vec<LineView>,
    total: String,
}

struct DayGroup {
    label: String,
    items: Vec<TxView>,
}

#[derive(Template)]
#[template(path = "transactions/list.html")]
struct ListPage {
    page: PageCtx,
    month_title: String,
    month: String,
    prev: String,
    next: String,
    params: ListParams,
    accounts: Vec<Choice>,
    days: Vec<DayGroup>,
    /// Итоги по валютам счетов: (валюта, доходы, расходы, итог, итог < 0).
    totals: Vec<(String, String, String, String, bool)>,
}

fn month_start(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").unwrap_or_else(|_| today().with_day(1).unwrap_or(today()))
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(params): Query<ListParams>) -> PageResult {
    let p = page.session.project_id;
    let lang = page.lang;
    let from = month_start(&params.month);
    let to = from.checked_add_months(Months::new(1)).and_then(|d| d.pred_opt()).unwrap_or(from);
    let query = ListQuery {
        from: Some(from),
        to: Some(to),
        account_code: Some(params.account.clone()).filter(|v| !v.is_empty()),
        created_by: Uuid::parse_str(&params.member).ok(),
        untranslated: (params.untranslated == "1").then(|| lang.code().to_string()),
    };
    let items = svc.list(p, query, Pagination { page: 0, size: 1000 }).await?.content;
    let cats: HashMap<String, String> = svc.category_titles(p, lang).await?.into_iter().collect();
    let accs: HashMap<String, String> = svc.account_titles(p, lang).await?.into_iter().collect();
    let accounts = svc.account_choices(p, lang).await?;

    let cur_of: HashMap<String, String> = accounts.iter().map(|a| (a.code.clone(), a.tag.clone())).collect();
    // Итоги считаются отдельно по каждой валюте: суммы в разных валютах не складываются.
    let mut sums: Vec<(String, Decimal, Decimal)> = Vec::new();
    let mut days: Vec<DayGroup> = Vec::new();
    let mut last: Option<NaiveDate> = None;
    for t in items {
        let currency = cur_of.get(&t.account_code).cloned().unwrap_or_else(|| page.session.base_currency.clone());
        let i = match sums.iter().position(|x| x.0 == currency) {
            Some(i) => i,
            None => {
                sums.push((currency.clone(), Decimal::ZERO, Decimal::ZERO));
                sums.len() - 1
            }
        };
        if t.is_expense() { sums[i].2 += t.total } else { sums[i].1 += t.total }
        let lines: Vec<LineView> = t
            .lines
            .iter()
            .map(|l| LineView {
                category: cats.get(&l.category_code).cloned().unwrap_or_else(|| l.category_code.clone()),
                amount: money(l.amount),
                note: l.note.clone().unwrap_or_default(),
            })
            .collect();
        let desc = match t.title(lang) {
            "" => lines.first().map(|l| l.category.clone()).unwrap_or_default(),
            s => s.to_string(),
        };
        let view = TxView {
            currency,
            desc,
            missing: t.missing_tr(lang),
            account: accs.get(&t.account_code).cloned().unwrap_or_else(|| t.account_code.clone()),
            author: page.session.member_name(&t.created_by),
            total: format!("{}{}", if t.is_expense() { "−" } else { "+" }, money(t.total)),
            lines,
            dto: t,
        };
        if last != Some(view.dto.occurred_on) {
            last = Some(view.dto.occurred_on);
            days.push(DayGroup { label: day_label(lang, view.dto.occurred_on), items: Vec::new() });
        }
        if let Some(g) = days.last_mut() {
            g.items.push(view);
        }
    }
    if sums.is_empty() {
        sums.push((page.session.base_currency.clone(), Decimal::ZERO, Decimal::ZERO));
    }
    // Базовая валюта первой.
    sums.sort_by_key(|x| x.0 != page.session.base_currency);
    let totals = sums.into_iter().map(|(c, i, e)| (c, money(i), money(e), money(i - e), i < e)).collect();
    let fmt = |d: NaiveDate| format!("{}-{:02}", d.year(), d.month());
    render(&ListPage {
        month_title: format!("{} {}", month_name(lang, from.month()), from.year()),
        month: fmt(from),
        prev: fmt(from.checked_sub_months(Months::new(1)).unwrap_or(from)),
        next: fmt(from.checked_add_months(Months::new(1)).unwrap_or(from)),
        params,
        accounts,
        days,
        totals,
        page: ctx(page),
    })
}

// ---------------------------------------------------------------- форма

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TxForm {
    occurred_on: String,
    account_code: String,
    tx_kind: String,
    description: String,
    description_lang: String,
    description_ru: String,
    description_en: String,
    description_th: String,
    plan_id: String,
    plan_date: String,
    back: String,
    line_category: Vec<String>,
    line_amount: Vec<String>,
    line_note: Vec<String>,
}

/// Предзаполнение формы (например, из календаря планов: «Оплатил»).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Prefill {
    plan_id: String,
    plan_date: String,
    account: String,
    category: String,
    amount: String,
    kind: String,
    desc: String,
    desc_lang: String,
    back: String,
}

struct LineForm {
    category: String,
    amount: String,
    note: String,
}

#[derive(Template)]
#[template(path = "transactions/form.html")]
struct FormPage {
    page: PageCtx,
    id: Option<Uuid>,
    f: TxForm,
    lines: Vec<LineForm>,
    accounts: Vec<Choice>,
    categories: Vec<Choice>,
    error: Option<String>,
}

fn opt(v: &str) -> Option<String> {
    Some(v.trim().to_string()).filter(|v| !v.is_empty())
}

fn back_or_list(back: &str) -> String {
    // Только относительные пути этого сайта — без открытых редиректов.
    if back.starts_with('/') && !back.starts_with("//") { back.to_string() } else { "/transactions".to_string() }
}

impl TxForm {
    fn lines(&self) -> Vec<LineForm> {
        let n = self.line_category.len().max(self.line_amount.len());
        let mut out: Vec<LineForm> = (0..n)
            .map(|i| LineForm {
                category: self.line_category.get(i).cloned().unwrap_or_default(),
                amount: self.line_amount.get(i).cloned().unwrap_or_default(),
                note: self.line_note.get(i).cloned().unwrap_or_default(),
            })
            .collect();
        if out.is_empty() {
            out.push(LineForm { category: String::new(), amount: String::new(), note: String::new() });
        }
        out
    }

    fn to_input(&self) -> Result<TransactionInput, AppError> {
        let v = |m: String| AppError::Validation(m);
        let mut lines = Vec::new();
        for (i, l) in self.lines().into_iter().enumerate() {
            // Полностью пустая строка формы просто пропускается.
            if l.category.trim().is_empty() && l.amount.trim().is_empty() {
                continue;
            }
            lines.push(LineDto {
                category_code: l.category,
                amount: parse_money(&format!("lines[{}].amount", i + 1), &l.amount).map_err(v)?,
                note: opt(&l.note),
            });
        }
        Ok(TransactionInput {
            occurred_on: self.occurred_on.parse().map_err(|_| v("occurred_on: неверная дата".into()))?,
            account_code: self.account_code.clone(),
            tx_kind: self.tx_kind.clone(),
            description: opt(&self.description),
            description_lang: self.description_lang.clone(),
            description_ru: opt(&self.description_ru),
            description_en: opt(&self.description_en),
            description_th: opt(&self.description_th),
            plan_id: opt(&self.plan_id).map(|s| Uuid::parse_str(&s)).transpose().map_err(|_| v("plan_id: неверный id".into()))?,
            plan_date: opt(&self.plan_date).map(|s| s.parse()).transpose().map_err(|_| v("plan_date: неверная дата".into()))?,
            lines,
        })
    }

    fn from_dto(t: TransactionDto) -> Self {
        Self {
            occurred_on: t.occurred_on.to_string(),
            account_code: t.account_code,
            tx_kind: t.tx_kind,
            description: t.description.unwrap_or_default(),
            description_lang: t.description_lang,
            description_ru: t.description_ru.unwrap_or_default(),
            description_en: t.description_en.unwrap_or_default(),
            description_th: t.description_th.unwrap_or_default(),
            plan_id: t.plan_id.map(|u| u.to_string()).unwrap_or_default(),
            plan_date: t.plan_date.map(|d| d.to_string()).unwrap_or_default(),
            back: String::new(),
            line_category: t.lines.iter().map(|l| l.category_code.clone()).collect(),
            line_amount: t.lines.iter().map(|l| l.amount.to_string()).collect(),
            line_note: t.lines.iter().map(|l| l.note.clone().unwrap_or_default()).collect(),
        }
    }
}

async fn form_page(svc: &TransactionService, page: PageCtx, id: Option<Uuid>, f: TxForm, error: Option<String>) -> PageResult {
    let p = page.session.project_id;
    let accounts = svc.account_choices(p, page.lang).await?;
    let categories = svc.category_choices(p, page.lang).await?;
    let lines = f.lines();
    render(&FormPage { page: ctx(page), id, f, lines, accounts, categories, error })
}

async fn new_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(q): Query<Prefill>) -> PageResult {
    let f = TxForm {
        occurred_on: today().to_string(),
        account_code: q.account,
        tx_kind: if q.kind == "INCOME" { "INCOME".into() } else { "EXPENSE".into() },
        description: q.desc,
        description_lang: Lang::parse(&q.desc_lang).unwrap_or(page.lang).upper().into(),
        plan_id: q.plan_id,
        plan_date: q.plan_date,
        back: q.back,
        line_category: vec![q.category],
        line_amount: vec![q.amount],
        line_note: vec![String::new()],
        ..Default::default()
    };
    form_page(&svc, page, None, f, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<TxForm>) -> PageResult {
    let s = page.session.scope();
    let result = async { svc.create(s.project_id, s.user_id, f.to_input()?).await }.await;
    match result {
        Ok(_) => Ok(see_other(&back_or_list(&f.back))),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, None, f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let t = svc.get(page.session.project_id, id).await?;
    form_page(&svc, page, Some(id), TxForm::from_dto(t), None).await
}

async fn update(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<TxForm>) -> PageResult {
    let p = page.session.project_id;
    let result = async { svc.update(p, id, f.to_input()?).await }.await;
    match result {
        Ok(_) => Ok(see_other(&back_or_list(&f.back))),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, Some(id), f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BackForm {
    back: String,
}

async fn delete(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<BackForm>) -> PageResult {
    svc.delete(page.session.project_id, id).await?;
    Ok(see_other(&back_or_list(&f.back)))
}

// ---------------------------------------------------------------- перевод

#[derive(Template)]
#[template(path = "transactions/translate.html")]
struct TranslatePage {
    page: PageCtx,
    t: TransactionDto,
    /// Поля переводов: (RU/EN/TH, имя поля, значение) — кроме языка оригинала.
    fields: Vec<(&'static str, &'static str, String)>,
    error: Option<String>,
}

fn translate_page(page: PageCtx, t: TransactionDto, error: Option<String>) -> PageResult {
    let fields = [("RU", "description_ru", &t.description_ru), ("EN", "description_en", &t.description_en), ("TH", "description_th", &t.description_th)]
        .into_iter()
        .filter(|(code, _, _)| *code != t.description_lang)
        .map(|(code, name, v)| (code, name, v.clone().unwrap_or_default()))
        .collect();
    render(&TranslatePage { page: ctx(page), t, fields, error })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TranslateForm {
    description_ru: String,
    description_en: String,
    description_th: String,
}

async fn translate_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let t = svc.get(page.session.project_id, id).await?;
    translate_page(page, t, None)
}

async fn translate(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<TranslateForm>) -> PageResult {
    let input = TranslationInput { description_ru: opt(&f.description_ru), description_en: opt(&f.description_en), description_th: opt(&f.description_th) };
    match svc.translate(page.session.project_id, id, input).await {
        Ok(_) => Ok(see_other("/transactions")),
        Err(AppError::Validation(m)) => {
            let t = svc.get(page.session.project_id, id).await?;
            translate_page(page, t, Some(m))
        }
        Err(e) => Err(PageError(e)),
    }
}
