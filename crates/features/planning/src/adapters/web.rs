//! Входящий адаптер: календарь платежей и плановые операции.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use chrono::{Datelike, Days, Months, NaiveDate};
use kernel::error::AppError;
use kernel::i18n::{Lang, month_name, weekdays};
use kernel::web::{
    Choice, Flash, PageCtx, PageError, PageResult, money_in, parse_money, render, see_other_flash, url_encode,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::dto::{OccurrenceDto, PlanDto, PlanInput};
use crate::service::PlanningService;

type Svc = State<Arc<PlanningService>>;

pub fn router(service: Arc<PlanningService>) -> Router {
    Router::new()
        .route("/", get(calendar))
        .route("/occurrence/{plan}/{date}", get(occurrence))
        .route("/occurrence/{plan}/{date}/skip", post(skip))
        .route("/occurrence/{plan}/{date}/unskip", post(unskip))
        .route("/plans", get(plans))
        .route("/plans/new", get(new_form).post(create))
        .route("/plans/{id}/edit", get(edit_form).post(update))
        .route("/plans/{id}/delete", post(delete))
        .route("/plans/{id}/toggle", post(toggle))
        .route("/plans/undo/{token}", post(undo))
        .with_state(service)
}

fn ctx(page: PageCtx) -> PageCtx {
    page.for_feature(crate::NAME, crate::i18n::dict)
}

/// Вложенная страница: хлебные крошки «Календарь → …».
fn nested(page: PageCtx, plans: bool) -> PageCtx {
    let page = ctx(page);
    let cal = page.t("title");
    let page = page.crumb(cal, "/planning");
    if plans {
        let p = page.t("plans");
        page.crumb(p, "/planning/plans")
    } else {
        page
    }
}

/// Строка словаря фичи (для тостов до привязки контекста к фиче).
fn tr(page: &PageCtx, key: &str) -> String {
    kernel::i18n::t(page.lang, Some(crate::i18n::dict), key)
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

fn ym(d: NaiveDate) -> String {
    format!("{}-{:02}", d.year(), d.month())
}

fn month_start(s: &str) -> NaiveDate {
    let t = today();
    NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").unwrap_or_else(|_| t.with_day(1).unwrap_or(t))
}

/// Ссылка «Оплатил»: форма транзакции, заполненная из плана (у перевода — форма перевода; сумму
/// зачисления при обмене валют вводят при оплате).
fn pay_href(o: &OccurrenceDto, lang: Lang, back: &str) -> String {
    let (desc, desc_lang) = o.plan.title_with_lang(lang);
    let target = match (&o.plan.to_account_code, &o.plan.category_code) {
        (Some(to), _) => format!("to_account={}", url_encode(to)),
        (None, Some(c)) => format!("category={}", url_encode(c)),
        (None, None) => String::new(),
    };
    format!(
        "/transactions/new?plan_id={}&plan_date={}&account={}&{target}&amount={}&kind={}&desc={}&desc_lang={}&back={}",
        o.plan.id,
        o.date,
        url_encode(&o.plan.account_code),
        o.plan.amount.normalize(),
        o.plan.tx_kind,
        url_encode(&desc),
        desc_lang.upper(),
        url_encode(back)
    )
}

/// Куда идут деньги плана: категория или «→ счёт зачисления».
fn target(plan: &PlanDto, categories: &HashMap<String, String>, accounts: &HashMap<String, String>) -> String {
    match (&plan.to_account_code, &plan.category_code) {
        (Some(to), _) => format!("→ {}", accounts.get(to).cloned().unwrap_or_else(|| to.clone())),
        (None, Some(c)) => categories.get(c).cloned().unwrap_or_else(|| c.clone()),
        (None, None) => String::new(),
    }
}

// ---------------------------------------------------------------- календарь

#[derive(Clone)]
struct OccView {
    href: String,
    title: String,
    amount: String,
    /// Валюта суммы (валюта счёта).
    currency: String,
    /// «≈ сумма» в базовой валюте — только если валюта счёта не базовая.
    approx: String,
    status: String,
    is_expense: bool,
    is_transfer: bool,
}

struct DayCell {
    day: u32,
    in_month: bool,
    is_today: bool,
    items: Vec<OccView>,
}

/// День со вхождениями — для списка на телефоне.
struct AgendaDay {
    label: String,
    is_today: bool,
    items: Vec<OccView>,
}

#[derive(Template)]
#[template(path = "planning/calendar.html")]
struct CalendarPage {
    page: PageCtx,
    month_title: String,
    prev: String,
    next: String,
    this_month: String,
    weekdays: [&'static str; 7],
    weeks: Vec<Vec<DayCell>>,
    agenda: Vec<AgendaDay>,
    still_to_pay: String,
    paid: String,
    income: String,
    overdue: usize,
    base: String,
    /// Валюты без курса к базовой — их суммы не вошли в итоги месяца.
    missing_rates: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MonthQuery {
    month: String,
}

async fn calendar(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(q): Query<MonthQuery>) -> PageResult {
    let lang = page.lang;
    let first = month_start(&q.month);
    let last = first.checked_add_months(Months::new(1)).and_then(|d| d.pred_opt()).unwrap_or(first);
    // Сетка с понедельника по воскресенье, захватывая края соседних месяцев.
    let grid_from = first - Days::new(first.weekday().num_days_from_monday() as u64);
    let grid_to = last + Days::new(6 - last.weekday().num_days_from_monday() as u64);
    let t = today();
    let occ = svc.occurrences(page.session.project_id, grid_from, grid_to, t).await?;
    let fx = svc.to_base(page.session.project_id, &page.session.base_currency).await?;
    let mut missing = BTreeSet::new();
    // Итоги — в базовой валюте по курсу на дату вхождения, как в «Обзоре». Без курса сумма в итоги не
    // входит (этап 0, Ц3): у самого платежа видна сумма в его валюте.
    let mut to_base = |cur: &str, amount: Decimal, date: NaiveDate| {
        let v = fx.convert(cur, amount, date);
        if v.is_none() {
            missing.insert(cur.to_string());
        }
        v
    };

    let (mut to_pay, mut paid, mut income, mut overdue) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, 0);
    let mut by_day: HashMap<NaiveDate, Vec<OccView>> = HashMap::new();
    let back = format!("/planning?month={}", ym(first));
    for o in &occ {
        let in_month = o.date >= first && o.date <= last;
        // Плановый перевод не расход и не доход: в сводке месяца только как просроченный.
        let (amount, currency) = o.shown_amount();
        let converted = to_base(currency, amount, o.date);
        let in_base = converted.unwrap_or_default();
        if in_month && o.plan.is_transfer() {
            if o.status == "OVERDUE" {
                overdue += 1;
            }
        } else if in_month {
            match (o.plan.is_expense(), o.is_open(), o.status.as_str()) {
                (true, true, s) => {
                    to_pay += in_base;
                    if s == "OVERDUE" {
                        overdue += 1;
                    }
                }
                (true, false, "PAID") => paid += in_base,
                (false, true, _) | (false, false, "PAID") => income += in_base,
                _ => {}
            }
        }
        by_day.entry(o.date).or_default().push(OccView {
            href: format!("/planning/occurrence/{}/{}?back={}", o.plan.id, o.date, url_encode(&back)),
            title: o.plan.title(lang).to_string(),
            amount: money_in(lang, amount),
            approx: match converted {
                Some(v) if currency != fx.base() => money_in(lang, v),
                _ => String::new(),
            },
            currency: currency.to_string(),
            status: o.status.clone(),
            is_expense: o.plan.is_expense(),
            is_transfer: o.plan.is_transfer(),
        });
    }
    let mut weeks = Vec::new();
    let mut agenda = Vec::new();
    let mut d = grid_from;
    while d <= grid_to {
        let mut week = Vec::with_capacity(7);
        for _ in 0..7 {
            let items = by_day.remove(&d).unwrap_or_default();
            let in_month = d.month() == first.month();
            if in_month && !items.is_empty() {
                let wd = weekdays(lang)[d.weekday().num_days_from_monday() as usize];
                agenda.push(AgendaDay {
                    label: format!("{wd}, {}", page.date(&d)),
                    is_today: d == t,
                    items: items.clone(),
                });
            }
            week.push(DayCell { day: d.day(), in_month, is_today: d == t, items });
            d = d.succ_opt().unwrap_or(d);
        }
        weeks.push(week);
    }
    render(&CalendarPage {
        month_title: format!("{} {}", month_name(lang, first.month()), first.year()),
        prev: ym(first.checked_sub_months(Months::new(1)).unwrap_or(first)),
        next: ym(first.checked_add_months(Months::new(1)).unwrap_or(first)),
        this_month: ym(t),
        weekdays: weekdays(lang),
        weeks,
        agenda,
        still_to_pay: money_in(lang, to_pay),
        paid: money_in(lang, paid),
        income: money_in(lang, income),
        overdue,
        base: fx.base().to_string(),
        missing_rates: missing.into_iter().collect::<Vec<_>>().join(", "),
        page: ctx(page),
    })
}

// ---------------------------------------------------------------- вхождение

#[derive(Template)]
#[template(path = "planning/occurrence.html")]
struct OccurrencePage {
    page: PageCtx,
    o: OccurrenceDto,
    category: String,
    pay_href: String,
    back: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BackQuery {
    back: String,
}

fn safe_back(back: &str) -> String {
    if back.starts_with("/planning") { back.to_string() } else { "/planning".to_string() }
}

async fn occurrence(
    State(svc): Svc,
    Extension(page): Extension<PageCtx>,
    Path((plan, date)): Path<(Uuid, NaiveDate)>,
    Query(q): Query<BackQuery>,
) -> PageResult {
    let p = page.session.project_id;
    let o = svc.occurrence(p, plan, date, today()).await?;
    let titles: HashMap<String, String> = svc.category_titles(p, page.lang).await?.into_iter().collect();
    let accounts: HashMap<String, String> =
        svc.account_choices(p, page.lang).await?.into_iter().map(|a| (a.code, a.label)).collect();
    let back = safe_back(&q.back);
    render(&OccurrencePage {
        category: target(&o.plan, &titles, &accounts),
        pay_href: pay_href(&o, page.lang, &back),
        o,
        back,
        page: nested(page, false),
    })
}

/// «Пропустить»: без транзакции, тост с «Отменить» (снять пропуск).
async fn skip(
    State(svc): Svc,
    Extension(page): Extension<PageCtx>,
    Path((plan, date)): Path<(Uuid, NaiveDate)>,
    Form(q): Form<BackQuery>,
) -> PageResult {
    let s = page.session.scope();
    svc.skip(s.project_id, s.user_id, plan, date).await?;
    let back = safe_back(&q.back);
    let undo = format!("/planning/occurrence/{plan}/{date}/unskip?back={}", url_encode(&back));
    Ok(see_other_flash(&page.config, &back, Flash::with_undo(tr(&page, "skipped_toast"), undo)))
}

async fn unskip(
    State(svc): Svc,
    Extension(page): Extension<PageCtx>,
    Path((plan, date)): Path<(Uuid, NaiveDate)>,
    Query(q): Query<BackQuery>,
    Form(f): Form<BackQuery>,
) -> PageResult {
    svc.unskip(page.session.project_id, plan, date).await?;
    let back = if f.back.is_empty() { safe_back(&q.back) } else { safe_back(&f.back) };
    Ok(see_other_flash(&page.config, &back, Flash::ok(tr(&page, "unskipped_toast"))))
}

// ---------------------------------------------------------------- планы

#[derive(Template)]
#[template(path = "planning/plans.html")]
struct PlansPage {
    page: PageCtx,
    /// (план, категория или счёт зачисления, сумма, валюта счёта).
    plans: Vec<(PlanDto, String, String, String)>,
}

async fn plans(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let p = page.session.project_id;
    let titles: HashMap<String, String> = svc.category_titles(p, page.lang).await?.into_iter().collect();
    let accounts: HashMap<String, String> =
        svc.account_choices(p, page.lang).await?.into_iter().map(|a| (a.code, a.label)).collect();
    let currencies = svc.account_currencies(p).await?;
    let plans = svc
        .list(p)
        .await?
        .into_iter()
        .map(|x| {
            let cat = target(&x, &titles, &accounts);
            let amount = money_in(page.lang, x.amount);
            let currency = currencies.get(&x.account_code).cloned().unwrap_or_default();
            (x, cat, amount, currency)
        })
        .collect();
    render(&PlansPage { page: nested(page, false), plans })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PlanForm {
    name: String,
    name_lang: String,
    name_ru: String,
    name_en: String,
    name_th: String,
    tx_kind: String,
    account_code: String,
    category_code: String,
    to_account_code: String,
    amount: String,
    recurrence: String,
    interval: String,
    start_on: String,
    end_on: String,
    active: String,
}

#[derive(Template)]
#[template(path = "planning/plan_form.html")]
struct PlanFormPage {
    page: PageCtx,
    id: Option<Uuid>,
    f: PlanForm,
    recurrences: &'static [&'static str],
    accounts: Vec<Choice>,
    categories: Vec<Choice>,
    error: Option<String>,
}

fn opt(v: &str) -> Option<String> {
    Some(v.trim().to_string()).filter(|v| !v.is_empty())
}

impl PlanForm {
    fn to_input(&self) -> Result<PlanInput, AppError> {
        let v = AppError::Validation;
        Ok(PlanInput {
            name: self.name.clone(),
            name_lang: self.name_lang.clone(),
            name_ru: opt(&self.name_ru),
            name_en: opt(&self.name_en),
            name_th: opt(&self.name_th),
            tx_kind: self.tx_kind.clone(),
            account_code: self.account_code.clone(),
            category_code: if self.tx_kind == "TRANSFER" { None } else { opt(&self.category_code) },
            to_account_code: if self.tx_kind == "TRANSFER" { opt(&self.to_account_code) } else { None },
            amount: parse_money("amount", &self.amount).map_err(v)?,
            recurrence: self.recurrence.clone(),
            interval: self.interval.trim().parse().map_err(|_| v("interval: целое число ≥ 1".into()))?,
            start_on: self.start_on.parse().map_err(|_| v("start_on: неверная дата".into()))?,
            end_on: opt(&self.end_on).map(|s| s.parse()).transpose().map_err(|_| v("end_on: неверная дата".into()))?,
            active: self.active == "on" || self.active == "true",
        })
    }

    fn from_dto(p: PlanDto) -> Self {
        Self {
            name: p.name,
            name_lang: p.name_lang,
            name_ru: p.name_ru.unwrap_or_default(),
            name_en: p.name_en.unwrap_or_default(),
            name_th: p.name_th.unwrap_or_default(),
            tx_kind: p.tx_kind,
            account_code: p.account_code,
            category_code: p.category_code.unwrap_or_default(),
            to_account_code: p.to_account_code.unwrap_or_default(),
            amount: p.amount.normalize().to_string(),
            recurrence: p.recurrence,
            interval: p.interval.to_string(),
            start_on: p.start_on.to_string(),
            end_on: p.end_on.map(|d| d.to_string()).unwrap_or_default(),
            active: if p.active { "on".into() } else { String::new() },
        }
    }
}

async fn form_page(
    svc: &PlanningService,
    page: PageCtx,
    id: Option<Uuid>,
    f: PlanForm,
    error: Option<String>,
) -> PageResult {
    let p = page.session.project_id;
    let accounts = svc.account_choices(p, page.lang).await?;
    let categories = svc.category_choices(p, page.lang).await?;
    render(&PlanFormPage {
        page: nested(page, true),
        id,
        f,
        recurrences: crate::domain::Recurrence::VALUES,
        accounts,
        categories,
        error,
    })
}

async fn new_form(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    let f = PlanForm {
        name_lang: page.lang.upper().into(),
        tx_kind: "EXPENSE".into(),
        recurrence: "MONTHLY".into(),
        interval: "1".into(),
        start_on: today().to_string(),
        active: "on".into(),
        ..Default::default()
    };
    form_page(&svc, page, None, f, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<PlanForm>) -> PageResult {
    let s = page.session.scope();
    let result = async { svc.create(s.project_id, s.user_id, f.to_input()?).await }.await;
    match result {
        Ok(_) => Ok(see_other_flash(&page.config, "/planning/plans", Flash::ok(page.t("toast.saved")))),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, None, f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let p = svc.get(page.session.project_id, id).await?;
    form_page(&svc, page, Some(id), PlanForm::from_dto(p), None).await
}

async fn update(
    State(svc): Svc,
    Extension(page): Extension<PageCtx>,
    Path(id): Path<Uuid>,
    Form(f): Form<PlanForm>,
) -> PageResult {
    let p = page.session.project_id;
    let result = async { svc.update(p, id, f.to_input()?).await }.await;
    match result {
        // Сменили правило дат при прошедших вхождениях: план продолжен новым (этап 0, Ц4).
        Ok(next) if next.id != id => {
            let from = page.date(&next.effective_from.unwrap_or(next.start_on));
            Ok(see_other_flash(
                &page.config,
                "/planning/plans",
                Flash::ok(tr(&page, "series_toast").replace("{date}", &from)),
            ))
        }
        Ok(_) => Ok(see_other_flash(&page.config, "/planning/plans", Flash::ok(page.t("toast.saved")))),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => form_page(&svc, page, Some(id), f, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

/// Удаление плана без «Вы уверены?»: тост с «Отменить».
async fn delete(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let token = svc.delete_with_undo(page.session.project_id, id).await?;
    Ok(see_other_flash(
        &page.config,
        "/planning/plans",
        Flash::with_undo(page.t("toast.deleted"), format!("/planning/plans/undo/{token}")),
    ))
}

async fn undo(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(token): Path<Uuid>) -> PageResult {
    let s = page.session.scope();
    let flash = match svc.undo_delete(s.project_id, s.user_id, token).await {
        Ok(()) => Flash::ok(page.t("toast.restored")),
        Err(AppError::NotFound(_)) => Flash::error(page.t("toast.undo_expired")),
        Err(e) => return Err(PageError(e)),
    };
    Ok(see_other_flash(&page.config, "/planning/plans", flash))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ToggleForm {
    active: String,
}

/// «Остановить» / «Возобновить» план из списка.
async fn toggle(
    State(svc): Svc,
    Extension(page): Extension<PageCtx>,
    Path(id): Path<Uuid>,
    Form(f): Form<ToggleForm>,
) -> PageResult {
    let active = f.active == "1";
    svc.set_active(page.session.project_id, id, active).await?;
    let msg = tr(&page, if active { "resumed_toast" } else { "stopped_toast" });
    Ok(see_other_flash(&page.config, "/planning/plans", Flash::ok(msg)))
}
