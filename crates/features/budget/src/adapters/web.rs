//! Входящий адаптер: страница «Бюджет».

use std::sync::Arc;

use askama::Template;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Extension, Router};
use chrono::{Datelike, NaiveDate};
use kernel::i18n::{Lang, month_name};
use kernel::web::{PageCtx, PageResult, money_in, render, url_encode};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::{Dir, Level, Period, Row, SortKey, View, next_sort, sort_tree};
use crate::service::{BudgetService, Report};

type Svc = State<Arc<BudgetService>>;

pub fn router(service: Arc<BudgetService>) -> Router {
    Router::new().route("/", get(page)).with_state(service)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Params {
    view: String,
    date: String,
    member: String,
    /// fact (по умолчанию) / forecast
    mode: String,
    /// Колонка сортировки «Расходов» (`SortKey::code`); пусто — порядок справочника.
    sort: String,
    /// asc / desc; пусто — направление колонки по умолчанию.
    dir: String,
}

/// Заголовок сортируемой колонки «Расходов».
struct SortHead {
    key: &'static str,
    label: String,
    /// Подпись для чтения с экрана, если `label` — значок.
    sr: String,
    /// Ссылка на следующее состояние сортировки по этой колонке.
    href: String,
    /// ascending / descending / пусто — колонка не активна.
    aria_sort: &'static str,
    /// Стрелка направления у активной колонки, нейтральный значок у остальных.
    icon: &'static str,
    num: bool,
}

struct RowView {
    code: String,
    parent: String,
    title: String,
    depth: usize,
    has_children: bool,
    limit: String,
    fact: String,
    planned: String,
    forecast: String,
    available: String,
    available_negative: bool,
    /// Доступно ограничено предком сильнее, чем своим лимитом.
    inherited: bool,
    percent: String,
    bar: u32,
    level: &'static str,
    children_over: bool,
    is_zero: bool,
    /// Суммы без курса к базовой валюте («10 $, 500 ₽») — в факт не вошли.
    no_rate: String,
}

struct UpcomingView {
    href: String,
    date: String,
    title: String,
    amount: String,
    /// Символ валюты суммы: базовой или, если курса нет, валюты счёта.
    cur: String,
    overdue: bool,
    is_expense: bool,
    is_transfer: bool,
}

#[derive(Template)]
#[template(path = "budget/page.html")]
struct BudgetPage {
    page: PageCtx,
    view: &'static str,
    title: String,
    prev: String,
    next: String,
    today: String,
    /// Начало показанного периода (фильтр участника не сбрасывает период).
    current: String,
    member: String,
    /// Режим сводки: `fact` — что уже произошло, `forecast` — чем закончится период с учётом планов.
    mode: &'static str,
    /// Сортировка «Расходов» для скрытых полей формы участника (пусто — порядок справочника).
    sort: &'static str,
    dir: &'static str,
    /// Хвост ссылок периода и режима, сохраняющий сортировку: `&sort=…&dir=…` или пусто.
    sort_q: String,
    heads: Vec<SortHead>,
    expenses: Vec<RowView>,
    incomes: Vec<RowView>,
    exp_fact: String,
    exp_plan: String,
    exp_total: String,
    inc_fact: String,
    inc_plan: String,
    inc_total: String,
    net: String,
    net_negative: bool,
    /// Курсовая разница за период (пусто, если обменов не было).
    fx: String,
    fx_negative: bool,
    accounts: Vec<(String, String, String, bool)>,
    upcoming: Vec<UpcomingView>,
    missing_rates: String,
    base: String,
}

fn opt_money(lang: Lang, v: Option<Decimal>) -> String {
    v.map(|x| money_in(lang, x)).unwrap_or_default()
}

fn row_view(lang: Lang, cur: &dyn Fn(&str) -> String, r: Row) -> RowView {
    let money = |v: Decimal| money_in(lang, v);
    let no_rate = r.no_rate.iter().map(|(c, v)| format!("{}\u{a0}{}", money(*v), cur(c))).collect::<Vec<_>>().join(", ");
    let inherited = match (r.remaining, r.available) {
        (None, Some(_)) => true,
        (Some(rem), Some(av)) => av < rem,
        _ => false,
    };
    RowView {
        bar: r.percent.unwrap_or(0).min(100),
        percent: r.percent.map(|p| format!("{p}%")).unwrap_or_default(),
        level: r.level.code(),
        children_over: r.children_over_limit,
        available_negative: r.available.is_some_and(|a| a < Decimal::ZERO),
        is_zero: r.forecast.is_zero() && r.limit.is_none() && r.no_rate.is_empty(),
        no_rate,
        inherited: inherited && r.level != Level::None,
        code: r.code,
        parent: r.parent_code.unwrap_or_default(),
        title: r.title,
        depth: r.depth,
        has_children: r.has_children,
        limit: opt_money(lang, r.limit),
        fact: money(r.fact),
        planned: if r.planned.is_zero() { String::new() } else { money(r.planned) },
        forecast: money(r.forecast),
        available: opt_money(lang, r.available),
    }
}

fn period_title(p: &Period, page: &PageCtx) -> String {
    match p.view {
        View::Week => format!("{:02}.{:02} — {:02}.{:02}.{}", p.from.day(), p.from.month(), p.to.day(), p.to.month(), p.to.year()),
        View::Month => format!("{} {}", month_name(page.lang, p.from.month()), p.from.year()),
        View::Year => p.from.year().to_string(),
    }
}

async fn page(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(q): Query<Params>) -> PageResult {
    let today = chrono::Local::now().date_naive();
    let view = View::parse(&q.view);
    let date = q.date.parse::<NaiveDate>().unwrap_or(today);
    let period = Period::containing(view, date);
    let member = Uuid::parse_str(&q.member).ok();
    let s = page.session.clone();
    let r: Report = svc.report(s.project_id, &s.base_currency, period, page.lang, member, today).await?;
    let lang = page.lang;
    let money = |v: Decimal| money_in(lang, v);

    let forecast = q.mode == "forecast";
    let mode = if forecast { "forecast" } else { "fact" };
    let sort = SortKey::parse(&q.sort).map(|k| (k, Dir::parse(&q.dir).unwrap_or(k.default_dir())));
    let sort_q = |s: Option<(SortKey, Dir)>| s.map(|(k, d)| format!("&sort={}&dir={}", k.code(), d.code())).unwrap_or_default();
    let base_q = format!("?view={}&date={}&member={}&mode={mode}", view.code(), period.from, url_encode(&q.member));
    let exp_total = r.expense_fact + r.expense_planned;
    let inc_total = r.income_fact + r.income_planned;
    let net = if forecast { r.net_forecast() } else { r.net_fact() };
    let upcoming = r
        .upcoming
        .iter()
        .map(|u| UpcomingView {
            href: format!("/planning/occurrence/{}/{}?back=/planning?month={}-{:02}", u.plan_id, u.date, u.date.year(), u.date.month()),
            date: format!("{:02}.{:02}", u.date.day(), u.date.month()),
            title: u.title.clone(),
            amount: money(u.amount),
            cur: page.cur(u.currency.as_deref().unwrap_or(&r.base_currency)).to_string(),
            overdue: u.status == "OVERDUE",
            is_expense: u.kind == "EXPENSE",
            is_transfer: u.kind == "TRANSFER",
        })
        .collect();
    let page = page.for_feature(crate::NAME, crate::i18n::dict);
    let heads = SortKey::ALL
        .into_iter()
        .map(|k| {
            let active = sort.filter(|(s, _)| *s == k).map(|(_, d)| d);
            SortHead {
                key: k.code(),
                label: if k == SortKey::Percent { "%".into() } else { page.t(if k == SortKey::Category { "category" } else { k.code() }) },
                sr: if k == SortKey::Percent { page.t("percent") } else { String::new() },
                href: format!("{base_q}{}", sort_q(next_sort(sort, k))),
                aria_sort: match active {
                    Some(Dir::Asc) => "ascending",
                    Some(Dir::Desc) => "descending",
                    None => "",
                },
                icon: if active.is_some() { "chevron-down" } else { "chevrons-up-down" },
                num: k != SortKey::Category,
            }
        })
        .collect();
    let expenses = match sort {
        Some((k, d)) => sort_tree(r.expenses, k, d),
        None => r.expenses,
    };
    let marks = page.session.clone();
    let cur = move |code: &str| marks.cur(code).to_string();
    render(&BudgetPage {
        view: view.code(),
        title: period_title(&period, &page),
        prev: period.prev().from.to_string(),
        next: period.next().from.to_string(),
        today: today.to_string(),
        current: period.from.to_string(),
        member: q.member,
        mode,
        sort: sort.map(|(k, _)| k.code()).unwrap_or_default(),
        dir: sort.map(|(_, d)| d.code()).unwrap_or_default(),
        sort_q: sort_q(sort),
        heads,
        exp_fact: money(r.expense_fact),
        exp_plan: money(r.expense_planned),
        exp_total: money(exp_total),
        inc_fact: money(r.income_fact),
        inc_plan: money(r.income_planned),
        inc_total: money(inc_total),
        net_negative: net < Decimal::ZERO,
        net: money(net),
        fx: if r.fx_result.is_zero() { String::new() } else { money(r.fx_result) },
        fx_negative: r.fx_result < Decimal::ZERO,
        accounts: r
            .accounts
            .iter()
            .map(|a| {
                let v = if forecast { a.forecast } else { a.balance };
                (a.title.clone(), money(v), a.currency.clone(), v < Decimal::ZERO)
            })
            .collect(),
        upcoming,
        missing_rates: r.missing_rates.join(", "),
        base: r.base_currency.clone(),
        expenses: expenses.into_iter().map(|x| row_view(lang, &cur, x)).collect(),
        incomes: r.incomes.into_iter().map(|x| row_view(lang, &cur, x)).collect(),
        page,
    })
}
