//! Входящий адаптер: веб-страницы фичи «Транзакции».

use std::collections::HashMap;
use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Router};
use axum_extra::extract::Form;
use chrono::{Datelike, Months, NaiveDate};
use kernel::error::AppError;
use kernel::i18n::{Lang, month_name, weekdays};
use kernel::pagination::Pagination;
use kernel::web::{Choice, Flash, PageCtx, PageError, PageResult, money_in, parse_money, render, see_other, see_other_flash};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::Description;
use crate::dto::{LineDto, ListQuery, TransactionDto, TransactionInput, TranslationInput};
use crate::service::TransactionService;

type Svc = State<Arc<TransactionService>>;

pub fn router(service: Arc<TransactionService>) -> Router {
    Router::new()
        .route("/", get(list))
        .route("/new", get(new_form).post(create))
        .route("/reconcile", get(reconcile_form).post(reconcile))
        .route("/{id}/edit", get(edit_form).post(update))
        .route("/{id}/delete", post(delete))
        .route("/undo/{token}", post(undo))
        .route("/{id}/translate", get(translate_form).post(translate))
        .with_state(service)
}

fn ctx(page: PageCtx) -> PageCtx {
    page.for_feature(crate::NAME, crate::i18n::dict)
}

/// Вложенная страница: хлебная крошка на «Операции».
fn nested(page: PageCtx) -> PageCtx {
    let page = ctx(page);
    let title = page.t("title");
    page.crumb(title, crate::MOUNT)
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Подпись дня: «Пт 09.10».
fn day_label(lang: Lang, d: NaiveDate) -> String {
    format!("{} {:02}.{:02}", weekdays(lang)[d.weekday().num_days_from_monday() as usize], d.day(), d.month())
}

/// Сумма со знаком: «+300,00» / «−1 250,00».
fn signed(lang: Lang, v: Decimal) -> String {
    let s = money_in(lang, v.abs());
    if v < Decimal::ZERO { format!("−{s}") } else { format!("+{s}") }
}

/// Курс перевода в направлении, где число больше 1: «1 ฿ = 2,556 ₽» (сколько `from` за 1 `to`).
pub(crate) fn rate_text(lang: Lang, sent: Decimal, from: &str, got: Decimal, to: &str) -> String {
    if sent.is_zero() || got.is_zero() {
        return String::new();
    }
    let (base, quote, r) = if sent >= got { (to, from, sent / got) } else { (from, to, got / sent) };
    let r = r.round_dp(4).normalize();
    let s = if lang == Lang::Ru { r.to_string().replace('.', ",") } else { r.to_string() };
    format!("1 {base} = {s} {quote}")
}

/// Название системного счёта на языке интерфейса.
fn system_title(page: &PageCtx, code: &str) -> String {
    page.t(&format!("sys.{code}"))
}

// ---------------------------------------------------------------- список

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ListParams {
    /// YYYY-MM
    month: String,
    account: String,
    kind: String,
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
    /// expense / income / transfer / opening — для цвета точки.
    css: &'static str,
    desc: String,
    missing: bool,
    /// Счёт (у перевода — «откуда → куда») и автор.
    meta: String,
    lines: Vec<LineView>,
    /// Категория единственной позиции без уточнения.
    category: String,
    /// Главная сумма со знаком и её валюта.
    amount: String,
    currency: String,
    positive: bool,
    /// Вторая сумма перевода с обменом: «+3 912,40» и валюта.
    second: Option<(String, String)>,
    /// Пояснения: курс, комиссия, возврат.
    notes: Vec<String>,
    /// Остаток счёта после операции (выписка).
    running: Option<String>,
    edit_href: String,
}

struct DayGroup {
    label: String,
    items: Vec<TxView>,
}

/// Плитка сводки: подпись и суммы по валютам (сумма, валюта, отрицательная).
struct Stat {
    label: String,
    values: Vec<(String, String, bool)>,
    income: bool,
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
    kinds: Vec<(&'static str, String)>,
    days: Vec<DayGroup>,
    stats: Vec<Stat>,
    /// Выписка по счёту: (название счёта, валюта).
    statement: Option<(String, String)>,
    /// Включён фильтр (пустой список — «ничего не найдено», а не «операций нет»).
    filtered: bool,
    /// Для «+ Перевод» из выписки.
    account_query: String,
}

fn month_start(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").unwrap_or_else(|_| today().with_day(1).unwrap_or(today()))
}

/// Итоги «Доходы / Расходы / Итог» по проводкам на категории, отдельно по каждой валюте.
fn category_totals(items: &[TransactionDto]) -> Vec<(String, Decimal, Decimal)> {
    let mut sums: Vec<(String, Decimal, Decimal)> = Vec::new();
    for t in items {
        for p in t.postings.iter().filter(|p| p.ref_kind == "CATEGORY") {
            let i = match sums.iter().position(|x| x.0 == p.currency_code) {
                Some(i) => i,
                None => {
                    sums.push((p.currency_code.clone(), Decimal::ZERO, Decimal::ZERO));
                    sums.len() - 1
                }
            };
            if t.is_income() { sums[i].1 -= p.amount } else { sums[i].2 += p.amount }
        }
    }
    sums
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(params): Query<ListParams>) -> PageResult {
    let p = page.session.project_id;
    let lang = page.lang;
    let from = month_start(&params.month);
    let to = from.checked_add_months(Months::new(1)).and_then(|d| d.pred_opt()).unwrap_or(from);
    let account = Some(params.account.clone()).filter(|v| !v.is_empty());
    let query = || ListQuery {
        from: Some(from),
        to: Some(to),
        account_code: account.clone(),
        tx_kind: Some(params.kind.clone()).filter(|v| !v.is_empty()),
        created_by: Uuid::parse_str(&params.member).ok(),
        untranslated: (params.untranslated == "1").then(|| lang.code().to_string()),
    };
    // Месяц целиком: страницы API по 200 записей (предел 25 страниц — защита от бесконечного цикла).
    let mut items = Vec::new();
    for page_no in 0..25 {
        let chunk = svc.list(p, query(), Pagination { page: page_no, size: kernel::pagination::MAX_PAGE }).await?;
        let last = chunk.page.number + 1 >= chunk.page.total_pages;
        items.extend(chunk.content);
        if last {
            break;
        }
    }
    let cats: HashMap<String, String> = svc.category_titles(p, lang).await?.into_iter().collect();
    let accs: HashMap<String, String> = svc.account_titles(p, lang).await?.into_iter().collect();
    let accounts = svc.account_choices(p, lang).await?;
    let acc_title = |code: &str| accs.get(code).cloned().unwrap_or_else(|| code.to_string());
    let cat_title = |code: &str| cats.get(code).cloned().unwrap_or_else(|| code.to_string());

    // Выписка: остаток на начало месяца и нарастающий остаток — только без прочих фильтров.
    let only_account = params.kind.is_empty() && params.member.is_empty() && params.untranslated != "1";
    let mut running: HashMap<Uuid, Decimal> = HashMap::new();
    let stats: Vec<Stat>;
    let mut statement = None;
    if let Some(acc) = &account {
        let currency = items
            .iter()
            .flat_map(|t| t.postings.iter())
            .find(|x| x.ref_kind == "ACCOUNT" && &x.ref_code == acc)
            .map(|x| x.currency_code.clone())
            .or_else(|| accounts.iter().find(|a| &a.code == acc).map(|a| a.tag.clone()))
            .unwrap_or_default();
        let start = match from.pred_opt() {
            Some(d) => svc.balance(p, acc, d).await?,
            None => Decimal::ZERO,
        };
        let end = svc.balance(p, acc, to).await?;
        let mut bal = start;
        for t in items.iter().rev() {
            bal += t.amount_on(acc);
            running.insert(t.id, bal);
        }
        let inflow: Decimal = items.iter().map(|t| t.amount_on(acc)).filter(|v| *v > Decimal::ZERO).sum();
        let outflow: Decimal = items.iter().map(|t| t.amount_on(acc)).filter(|v| *v < Decimal::ZERO).sum();
        let one = |label: &str, v: Decimal, income: bool| Stat {
            label: page_t(&page, label),
            values: vec![(money_in(lang, v), currency.clone(), v < Decimal::ZERO)],
            income,
        };
        stats = vec![
            one("balance_start", start, false),
            Stat { label: page_t(&page, "inflow"), values: vec![(signed(lang, inflow), currency.clone(), false)], income: true },
            Stat { label: page_t(&page, "outflow"), values: vec![(signed(lang, outflow), currency.clone(), false)], income: false },
            one("balance_end", end, false),
        ];
        statement = Some((acc_title(acc), currency));
    } else {
        let mut sums = category_totals(&items);
        if sums.is_empty() {
            sums.push((page.session.base_currency.clone(), Decimal::ZERO, Decimal::ZERO));
        }
        // Базовая валюта первой.
        sums.sort_by_key(|x| x.0 != page.session.base_currency);
        stats = vec![
            Stat { label: page_t(&page, "income_total"), values: sums.iter().map(|(c, i, _)| (signed(lang, *i), c.clone(), false)).collect(), income: true },
            Stat {
                label: page_t(&page, "expense_total"),
                values: sums.iter().map(|(c, _, e)| (signed(lang, -*e), c.clone(), false)).collect(),
                income: false,
            },
            Stat { label: page_t(&page, "net"), values: sums.iter().map(|(c, i, e)| (money_in(lang, i - e), c.clone(), i < e)).collect(), income: false },
        ];
    }

    let mut days: Vec<DayGroup> = Vec::new();
    let mut last: Option<NaiveDate> = None;
    for t in items {
        let author = page.session.member_name(&t.created_by);
        let mut notes = Vec::new();
        let (css, meta, desc_fallback) = match t.tx_kind.as_str() {
            "TRANSFER" => {
                let to = t.to_account_code.clone().unwrap_or_default();
                ("transfer", format!("{} → {} · {author}", acc_title(&t.account_code), acc_title(&to)), page_t(&page, "transfer"))
            }
            "OPENING" => ("opening", format!("{} · {author}", acc_title(&t.account_code)), page_t(&page, "opening")),
            "INCOME" => ("income", format!("{} · {author}", acc_title(&t.account_code)), String::new()),
            _ => ("expense", format!("{} · {author}", acc_title(&t.account_code)), String::new()),
        };
        let lines: Vec<LineView> = t
            .lines
            .iter()
            .map(|l| LineView { category: cat_title(&l.category_code), amount: money_in(lang, l.amount), note: l.note.clone().unwrap_or_default() })
            .collect();
        if t.refund {
            notes.push(page_t(&page, "refund"));
        }
        if let (Some(to_cur), Some(got)) = (&t.to_currency, t.to_amount) {
            notes.push(format!("{} {}", page_t(&page, "rate"), rate_text(lang, t.sent(), &t.currency, got, to_cur)));
        }
        if let (Some(fee), Some(cat)) = (t.fee_amount, &t.fee_category_code) {
            notes.push(format!("{} {} {} · {}", page_t(&page, "fee_short"), money_in(lang, fee), t.currency, cat_title(cat)));
        }
        let desc = match t.title(lang) {
            "" if !desc_fallback.is_empty() => desc_fallback,
            "" => lines.first().map(|l| l.category.clone()).unwrap_or_default(),
            s => s.to_string(),
        };
        // В выписке — как операция изменила этот счёт; иначе — сумма операции со знаком.
        let (value, currency, second) = match &account {
            Some(acc) => {
                let cur = t.postings.iter().find(|x| x.ref_kind == "ACCOUNT" && &x.ref_code == acc).map(|x| x.currency_code.clone()).unwrap_or_default();
                (t.amount_on(acc), cur, None)
            }
            None if t.is_transfer() => {
                let to = t.to_account_code.clone().unwrap_or_default();
                let second = t.to_currency.clone().filter(|c| *c != t.currency).map(|c| (signed(lang, t.amount_on(&to)), c));
                (-t.total, t.currency.clone(), second)
            }
            None => (t.amount_on(&t.account_code), t.currency.clone(), None),
        };
        let view = TxView {
            css,
            desc,
            missing: t.missing_tr(lang),
            meta,
            category: if t.lines.len() == 1 && t.lines[0].note.is_none() { lines[0].category.clone() } else { String::new() },
            lines: if t.lines.len() > 1 || t.lines.first().is_some_and(|l| l.note.is_some()) { lines } else { Vec::new() },
            amount: signed(lang, value),
            currency,
            positive: value > Decimal::ZERO && !t.is_transfer(),
            second,
            notes,
            running: if only_account { running.get(&t.id).map(|b| money_in(lang, *b)) } else { None },
            edit_href: if t.is_opening() {
                format!("/accounts/{}/edit", kernel::web::url_encode(&t.account_code))
            } else {
                format!("/transactions/{}/edit?back={}", t.id, kernel::web::url_encode(&page.back()))
            },
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
    let fmt = |d: NaiveDate| format!("{}-{:02}", d.year(), d.month());
    let filtered = account.is_some() || !params.kind.is_empty() || !params.member.is_empty() || params.untranslated == "1";
    let kinds = ["EXPENSE", "INCOME", "TRANSFER", "OPENING"].into_iter().map(|k| (k, page_t(&page, &format!("kinds.{k}")))).collect();
    render(&ListPage {
        filtered,
        month_title: format!("{} {}", month_name(lang, from.month()), from.year()),
        month: fmt(from),
        prev: fmt(from.checked_sub_months(Months::new(1)).unwrap_or(from)),
        next: fmt(from.checked_add_months(Months::new(1)).unwrap_or(from)),
        account_query: account.map(|a| format!("&account={}", kernel::web::url_encode(&a))).unwrap_or_default(),
        params,
        accounts,
        kinds,
        days,
        stats,
        statement,
        page: ctx(page),
    })
}

/// Строка словаря фичи (до того, как `PageCtx` переключён на фичу).
fn page_t(page: &PageCtx, key: &str) -> String {
    kernel::i18n::t(page.lang, Some(crate::i18n::dict), key)
}

// ---------------------------------------------------------------- форма

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TxForm {
    occurred_on: String,
    tx_kind: String,
    account_code: String,
    refund: String,
    to_account_code: String,
    amount: String,
    to_amount: String,
    fee_category_code: String,
    fee_amount: String,
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

/// Предзаполнение формы (из календаря планов: «Оплатил»; со счёта: «Перевести»; из быстрого диалога).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Prefill {
    plan_id: String,
    plan_date: String,
    account: String,
    to_account: String,
    category: String,
    amount: String,
    kind: String,
    desc: String,
    desc_lang: String,
    /// Дата операции (из быстрого расхода: «Разбить на позиции»).
    date: String,
    back: String,
}

struct LineForm {
    category: String,
    amount: String,
    note: String,
}

/// Проводка для блока «Проводки» в карточке транзакции.
struct PostingView {
    title: String,
    system: bool,
    amount: String,
    currency: String,
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
    /// Курс 1 единицы валюты к базовой на дату формы (для курса справочника рядом с курсом перевода).
    rates: HashMap<String, String>,
    postings: Vec<PostingView>,
    error: Option<String>,
}

impl FormPage {
    fn rate_of(&self, currency: &str) -> String {
        self.rates.get(currency).cloned().unwrap_or_default()
    }
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

    fn kind(&self) -> &str {
        match self.tx_kind.as_str() {
            "INCOME" => "INCOME",
            "TRANSFER" => "TRANSFER",
            _ => "EXPENSE",
        }
    }

    fn to_input(&self) -> Result<TransactionInput, AppError> {
        let v = |m: String| AppError::Validation(m);
        let money = |field: &str, s: &str| opt(s).map(|s| parse_money(field, &s)).transpose().map_err(v);
        let transfer = self.kind() == "TRANSFER";
        let mut lines = Vec::new();
        if !transfer {
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
        }
        Ok(TransactionInput {
            occurred_on: self.occurred_on.parse().map_err(|_| v("occurred_on: неверная дата".into()))?,
            tx_kind: self.kind().to_string(),
            account_code: self.account_code.clone(),
            refund: !transfer && self.refund == "1",
            lines,
            to_account_code: if transfer { opt(&self.to_account_code) } else { None },
            amount: if transfer { money("amount", &self.amount)? } else { None },
            to_amount: if transfer { money("to_amount", &self.to_amount)? } else { None },
            fee_category_code: if transfer { opt(&self.fee_category_code) } else { None },
            fee_amount: if transfer { money("fee_amount", &self.fee_amount)? } else { None },
            description: opt(&self.description),
            description_lang: self.description_lang.clone(),
            description_ru: opt(&self.description_ru),
            description_en: opt(&self.description_en),
            description_th: opt(&self.description_th),
            plan_id: opt(&self.plan_id).map(|s| Uuid::parse_str(&s)).transpose().map_err(|_| v("plan_id: неверный id".into()))?,
            plan_date: opt(&self.plan_date).map(|s| s.parse()).transpose().map_err(|_| v("plan_date: неверная дата".into()))?,
        })
    }

    fn from_dto(t: &TransactionDto) -> Self {
        let num = |d: Option<Decimal>| d.map(|d| d.normalize().to_string()).unwrap_or_default();
        Self {
            occurred_on: t.occurred_on.to_string(),
            tx_kind: t.tx_kind.clone(),
            account_code: t.account_code.clone(),
            refund: if t.refund { "1".into() } else { String::new() },
            to_account_code: t.to_account_code.clone().unwrap_or_default(),
            amount: if t.is_transfer() { num(Some(t.total)) } else { String::new() },
            to_amount: num(t.to_amount),
            fee_category_code: t.fee_category_code.clone().unwrap_or_default(),
            fee_amount: num(t.fee_amount),
            description: t.description.clone().unwrap_or_default(),
            description_lang: t.description_lang.clone(),
            description_ru: t.description_ru.clone().unwrap_or_default(),
            description_en: t.description_en.clone().unwrap_or_default(),
            description_th: t.description_th.clone().unwrap_or_default(),
            plan_id: t.plan_id.map(|u| u.to_string()).unwrap_or_default(),
            plan_date: t.plan_date.map(|d| d.to_string()).unwrap_or_default(),
            back: String::new(),
            line_category: t.lines.iter().map(|l| l.category_code.clone()).collect(),
            line_amount: t.lines.iter().map(|l| l.amount.normalize().to_string()).collect(),
            line_note: t.lines.iter().map(|l| l.note.clone().unwrap_or_default()).collect(),
        }
    }
}

async fn form_page(svc: &TransactionService, page: PageCtx, existing: Option<&TransactionDto>, f: TxForm, error: Option<String>) -> PageResult {
    let p = page.session.project_id;
    let lang = page.lang;
    let accounts = svc.account_choices(p, lang).await?;
    let categories = svc.category_choices(p, lang).await?;
    let date = f.occurred_on.parse::<NaiveDate>().unwrap_or_else(|_| today());
    let base = page.session.base_currency.clone();
    let mut rates: HashMap<String, String> = svc.rates(p, &base, date).await?.into_iter().map(|(c, r)| (c, r.normalize().to_string())).collect();
    rates.insert(base, "1".into());
    let postings = match existing {
        None => Vec::new(),
        Some(t) => {
            let accs: HashMap<String, String> = svc.account_titles(p, lang).await?.into_iter().collect();
            let cats: HashMap<String, String> = svc.category_titles(p, lang).await?.into_iter().collect();
            let page = ctx(page.clone());
            t.postings
                .iter()
                .map(|x| PostingView {
                    title: match x.ref_kind.as_str() {
                        "ACCOUNT" => accs.get(&x.ref_code).cloned().unwrap_or_else(|| x.ref_code.clone()),
                        "CATEGORY" => cats.get(&x.ref_code).cloned().unwrap_or_else(|| x.ref_code.clone()),
                        _ => system_title(&page, &x.ref_code),
                    },
                    system: x.ref_kind == "SYSTEM",
                    amount: signed(lang, x.amount),
                    currency: x.currency_code.clone(),
                })
                .collect()
        }
    };
    let lines = f.lines();
    render(&FormPage { page: nested(page), id: existing.map(|t| t.id), f, lines, accounts, categories, rates, postings, error })
}

async fn new_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(q): Query<Prefill>) -> PageResult {
    let date = q.date.parse::<NaiveDate>().unwrap_or_else(|_| today());
    let kind = match q.kind.as_str() {
        "INCOME" | "TRANSFER" => q.kind.clone(),
        _ => "EXPENSE".into(),
    };
    let transfer = kind == "TRANSFER";
    let f = TxForm {
        occurred_on: date.to_string(),
        account_code: q.account,
        to_account_code: q.to_account,
        amount: if transfer { q.amount.clone() } else { String::new() },
        tx_kind: kind,
        description: q.desc,
        description_lang: Lang::parse(&q.desc_lang).unwrap_or(page.lang).upper().into(),
        plan_id: q.plan_id,
        plan_date: q.plan_date,
        back: q.back,
        line_category: vec![q.category],
        line_amount: vec![if transfer { String::new() } else { q.amount }],
        line_note: vec![String::new()],
        ..Default::default()
    };
    form_page(&svc, page, None, f, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<TxForm>) -> PageResult {
    let s = page.session.scope();
    let result = async { svc.create(s.project_id, s.user_id, f.to_input()?).await }.await;
    match result {
        Ok(_) => Ok(see_other_flash(&page.config, &back_or_list(&f.back), Flash::ok(page.t("toast.saved")))),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, None, f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let t = svc.get(page.session.project_id, id).await?;
    // Начальный остаток ведётся из формы счёта.
    if t.is_opening() {
        return Ok(see_other(&format!("/accounts/{}/edit", kernel::web::url_encode(&t.account_code))));
    }
    let f = TxForm::from_dto(&t);
    form_page(&svc, page, Some(&t), f, None).await
}

async fn update(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<TxForm>) -> PageResult {
    let p = page.session.project_id;
    let result = async { svc.update(p, id, f.to_input()?).await }.await;
    match result {
        Ok(_) => Ok(see_other_flash(&page.config, &back_or_list(&f.back), Flash::ok(page.t("toast.saved")))),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => {
            let t = svc.get(p, id).await?;
            form_page(&svc, page, Some(&t), f, Some(m)).await
        }
        Err(e) => Err(PageError(e)),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BackForm {
    back: String,
}

/// Удаление без «Вы уверены?»: тост с «Отменить» на 5 секунд. Удаление оплаты плана возвращает
/// вхождение в «ожидается», «Отменить» — снова в «оплачено».
async fn delete(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<BackForm>) -> PageResult {
    let token = svc.delete_with_undo(page.session.project_id, id).await?;
    let back = back_or_list(&f.back);
    let undo = format!("/transactions/undo/{token}?back={}", kernel::web::url_encode(&back));
    Ok(see_other_flash(&page.config, &back, Flash::with_undo(page.t("toast.deleted"), undo)))
}

async fn undo(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(token): Path<Uuid>, Query(q): Query<BackForm>) -> PageResult {
    let flash = match svc.undo_delete(page.session.project_id, token).await {
        Ok(_) => Flash::ok(page.t("toast.restored")),
        Err(AppError::NotFound(_)) => Flash::error(page.t("toast.undo_expired")),
        Err(AppError::Conflict(m)) => Flash::error(m),
        Err(e) => return Err(PageError(e)),
    };
    Ok(see_other_flash(&page.config, &back_or_list(&q.back), flash))
}

// ---------------------------------------------------------------- сверка остатка

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ReconcileForm {
    account: String,
    occurred_on: String,
    actual: String,
    category_code: String,
}

#[derive(Template)]
#[template(path = "transactions/reconcile.html")]
struct ReconcilePage {
    page: PageCtx,
    f: ReconcileForm,
    account_title: String,
    currency: String,
    /// Остаток по проводкам на дату формы.
    book: String,
    book_raw: String,
    categories: Vec<Choice>,
    error: Option<String>,
}

async fn reconcile_page(svc: &TransactionService, page: PageCtx, f: ReconcileForm, error: Option<String>) -> PageResult {
    let p = page.session.project_id;
    let lang = page.lang;
    let accounts = svc.account_choices(p, lang).await?;
    let acc = accounts.iter().find(|a| a.code == f.account).ok_or_else(|| AppError::NotFound(format!("счёт {} не найден", f.account)))?;
    let date = f.occurred_on.parse::<NaiveDate>().unwrap_or_else(|_| today());
    let book = svc.balance(p, &acc.code, date).await?;
    let categories = svc.category_choices(p, lang).await?;
    let page = ctx(page);
    let title = page.t("nav_accounts");
    render(&ReconcilePage {
        account_title: acc.label.clone(),
        currency: acc.tag.clone(),
        book: money_in(lang, book),
        book_raw: book.normalize().to_string(),
        categories,
        f,
        error,
        page: page.crumb(title, "/accounts"),
    })
}

async fn reconcile_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(mut f): Query<ReconcileForm>) -> PageResult {
    if f.occurred_on.is_empty() {
        f.occurred_on = today().to_string();
    }
    reconcile_page(&svc, page, f, None).await
}

async fn reconcile(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<ReconcileForm>) -> axum::response::Response {
    let s = page.session.scope();
    let lang = page.lang;
    let result = async {
        let date: NaiveDate = f.occurred_on.parse().map_err(|_| AppError::Validation("occurred_on: неверная дата".into()))?;
        let actual = parse_money("actual", &f.actual).map_err(AppError::Validation)?;
        let text = crate::i18n::dict("reconcile_desc").map(|t| lang.pick(t).to_string());
        let description = Description { text, lang, ..Default::default() };
        svc.reconcile(s.project_id, s.user_id, &f.account, actual, date, &f.category_code, description).await
    }
    .await;
    let back = format!("/transactions?account={}&month={}", kernel::web::url_encode(&f.account), f.occurred_on.get(..7).unwrap_or_default());
    match result {
        Ok(Some(_)) => see_other_flash(&page.config, &back, Flash::ok(page_t(&page, "reconcile_done"))),
        Ok(None) => see_other_flash(&page.config, &back, Flash::ok(page_t(&page, "reconcile_same"))),
        Err(AppError::Validation(m)) => reconcile_page(&svc, page, f, Some(m)).await.into_response(),
        Err(e) => PageError(e).into_response(),
    }
}

// ---------------------------------------------------------------- перевод описания

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
    render(&TranslatePage { page: nested(page), t, fields, error })
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
        Ok(_) => Ok(see_other_flash(&page.config, "/transactions", Flash::ok(page.t("toast.saved")))),
        Err(AppError::Validation(m)) => {
            let t = svc.get(page.session.project_id, id).await?;
            translate_page(page, t, Some(m))
        }
        Err(e) => Err(PageError(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn rate_is_shown_above_one() {
        let d = |s: &str| Decimal::from_str(s).unwrap();
        assert_eq!(rate_text(Lang::Ru, d("10000"), "RUB", d("3912.40"), "THB"), "1 THB = 2,556 RUB");
        assert_eq!(rate_text(Lang::En, d("100"), "USD", d("3325"), "THB"), "1 USD = 33.25 THB");
    }
}
