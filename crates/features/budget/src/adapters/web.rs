//! Входящий адаптер: страница «Бюджет».

use std::sync::Arc;

use askama::Template;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Extension, Router};
use chrono::{Datelike, NaiveDate};
use kernel::i18n::month_name;
use kernel::web::{PageCtx, PageResult, money, render};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::{Level, Period, Row, View};
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
}

struct UpcomingView {
    href: String,
    date: String,
    title: String,
    amount: String,
    overdue: bool,
    is_expense: bool,
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
    member: String,
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
    accounts: Vec<(String, String, String, bool)>,
    upcoming: Vec<UpcomingView>,
    missing_rates: String,
    base: String,
}

fn opt_money(v: Option<Decimal>) -> String {
    v.map(money).unwrap_or_default()
}

fn row_view(r: Row) -> RowView {
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
        is_zero: r.forecast.is_zero() && r.limit.is_none(),
        inherited: inherited && r.level != Level::None,
        code: r.code,
        parent: r.parent_code.unwrap_or_default(),
        title: r.title,
        depth: r.depth,
        has_children: r.has_children,
        limit: opt_money(r.limit),
        fact: money(r.fact),
        planned: if r.planned.is_zero() { String::new() } else { money(r.planned) },
        forecast: money(r.forecast),
        available: opt_money(r.available),
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

    let exp_total = r.expense_fact + r.expense_planned;
    let inc_total = r.income_fact + r.income_planned;
    let net = r.net_forecast();
    let back = format!("/budget?view={}&date={}", view.code(), period.from);
    let upcoming = r
        .upcoming
        .iter()
        .map(|u| UpcomingView {
            href: format!("/planning/occurrence/{}/{}?back=/planning?month={}-{:02}", u.plan_id, u.date, u.date.year(), u.date.month()),
            date: format!("{:02}.{:02}", u.date.day(), u.date.month()),
            title: u.title.clone(),
            amount: money(u.amount),
            overdue: u.status == "OVERDUE",
            is_expense: u.kind == "EXPENSE",
        })
        .collect();
    let _ = back;
    let page = page.for_feature(crate::NAME, crate::i18n::dict);
    render(&BudgetPage {
        view: view.code(),
        title: period_title(&period, &page),
        prev: period.prev().from.to_string(),
        next: period.next().from.to_string(),
        today: today.to_string(),
        member: q.member,
        exp_fact: money(r.expense_fact),
        exp_plan: money(r.expense_planned),
        exp_total: money(exp_total),
        inc_fact: money(r.income_fact),
        inc_plan: money(r.income_planned),
        inc_total: money(inc_total),
        net_negative: net < Decimal::ZERO,
        net: money(net),
        accounts: r
            .accounts
            .iter()
            .map(|a| (a.title.clone(), money(a.balance), a.currency.clone(), a.balance.to_f64().unwrap_or(0.0) < 0.0))
            .collect(),
        upcoming,
        missing_rates: r.missing_rates.join(", "),
        base: r.base_currency.clone(),
        expenses: r.expenses.into_iter().map(row_view).collect(),
        incomes: r.incomes.into_iter().map(row_view).collect(),
        page,
    })
}
