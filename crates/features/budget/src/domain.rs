//! Расчёт бюджета: факт, план, лимит по дереву категорий за неделю / месяц / год.
//! Чистые функции без ввода-вывода — всё, что нужно, приходит параметрами.

use std::collections::HashMap;

use chrono::{Datelike, Days, Months, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

/// Вид периода.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Week,
    Month,
    Year,
}

impl View {
    pub fn parse(s: &str) -> View {
        match s {
            "week" | "WEEK" => View::Week,
            "year" | "YEAR" => View::Year,
            _ => View::Month,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            View::Week => "week",
            View::Month => "month",
            View::Year => "year",
        }
    }
}

/// Период отчёта, обе границы включительно.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    pub view: View,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl Period {
    /// Период вида `view`, содержащий дату `d` (неделя — с понедельника).
    pub fn containing(view: View, d: NaiveDate) -> Period {
        let (from, to) = match view {
            View::Week => {
                let from = d - Days::new(d.weekday().num_days_from_monday() as u64);
                (from, from + Days::new(6))
            }
            View::Month => {
                let from = d.with_day(1).unwrap_or(d);
                (from, from.checked_add_months(Months::new(1)).and_then(|x| x.pred_opt()).unwrap_or(from))
            }
            View::Year => {
                let from = NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap_or(d);
                (from, NaiveDate::from_ymd_opt(d.year(), 12, 31).unwrap_or(d))
            }
        };
        Period { view, from, to }
    }

    pub fn days(&self) -> i64 {
        (self.to - self.from).num_days() + 1
    }

    pub fn prev(&self) -> Period {
        Period::containing(self.view, self.from.pred_opt().unwrap_or(self.from))
    }

    pub fn next(&self) -> Period {
        Period::containing(self.view, self.to.succ_opt().unwrap_or(self.to))
    }
}

/// Лимит категории (сумма в базовой валюте за свой период).
#[derive(Debug, Clone, PartialEq)]
pub struct LimitRule {
    pub category_code: String,
    pub period: View,
    pub amount: Decimal,
    pub valid_from: NaiveDate,
}

/// Лимит, приведённый к периоду отчёта.
///
/// Берётся лимит категории, действующий на конец периода (последний с `valid_from ≤ to`).
/// Если вид лимита не совпадает с видом отчёта, сумма пересчитывается по дням:
/// месячный 31 000 в недельном отчёте за октябрь = 31 000 × 7 / 31 = 7 000.
pub fn effective_limit(rules: &[LimitRule], category: &str, period: &Period) -> Option<Decimal> {
    let rule = rules
        .iter()
        .filter(|r| r.category_code == category && r.valid_from <= period.to)
        .max_by_key(|r| r.valid_from)?;
    if rule.period == period.view {
        return Some(rule.amount);
    }
    let base = Period::containing(rule.period, period.from);
    Some((rule.amount * Decimal::from(period.days()) / Decimal::from(base.days())).round_dp(2))
}

/// Узел дерева категорий (порядок — обход в глубину).
#[derive(Debug, Clone)]
pub struct Node {
    pub code: String,
    pub parent_code: Option<String>,
    /// EXPENSE / INCOME
    pub kind: String,
    pub title: String,
    pub depth: usize,
    pub archived: bool,
}

/// Сумма по категории в базовой валюте.
#[derive(Debug, Clone)]
pub struct Amount {
    pub category_code: String,
    pub amount: Decimal,
}

/// Уровень заполнения лимита.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Лимита нет ни у узла, ни у предков.
    None,
    Ok,
    /// Прогноз ≥ 80 % лимита.
    Warn,
    /// Прогноз > 100 % лимита.
    Over,
}

impl Level {
    pub fn code(self) -> &'static str {
        match self {
            Level::None => "none",
            Level::Ok => "ok",
            Level::Warn => "warn",
            Level::Over => "over",
        }
    }
}

/// Строка бюджета по категории (суммы — по всему поддереву).
#[derive(Debug, Clone)]
pub struct Row {
    pub code: String,
    pub title: String,
    pub kind: String,
    pub depth: usize,
    pub parent_code: Option<String>,
    pub has_children: bool,
    pub limit: Option<Decimal>,
    pub fact: Decimal,
    pub planned: Decimal,
    pub forecast: Decimal,
    /// Лимит − прогноз (если у узла есть лимит).
    pub remaining: Option<Decimal>,
    /// min(остаток узла, доступно родителя): сколько ещё можно потратить с учётом предков.
    pub available: Option<Decimal>,
    /// Прогноз / лимит, %.
    pub percent: Option<u32>,
    pub level: Level,
    /// Сумма лимитов детей больше лимита узла.
    pub children_over_limit: bool,
}

/// Посчитать строки бюджета. `facts` и `planned` — уже в базовой валюте и уже отфильтрованы по периоду.
pub fn compute(nodes: &[Node], facts: &[Amount], planned: &[Amount], limits: &[LimitRule], period: &Period) -> Vec<Row> {
    let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.code.as_str(), i)).collect();
    let mut fact = vec![Decimal::ZERO; nodes.len()];
    let mut plan = vec![Decimal::ZERO; nodes.len()];
    for a in facts {
        if let Some(&i) = index.get(a.category_code.as_str()) {
            fact[i] += a.amount;
        }
    }
    for a in planned {
        if let Some(&i) = index.get(a.category_code.as_str()) {
            plan[i] += a.amount;
        }
    }
    let parent: Vec<Option<usize>> = nodes.iter().map(|n| n.parent_code.as_deref().and_then(|p| index.get(p).copied())).collect();
    // Свёртка снизу вверх: обход в глубину гарантирует, что дети идут после родителя.
    for i in (0..nodes.len()).rev() {
        if let Some(p) = parent[i] {
            let (f, pl) = (fact[i], plan[i]);
            fact[p] += f;
            plan[p] += pl;
        }
    }
    let limit: Vec<Option<Decimal>> = nodes.iter().map(|n| effective_limit(limits, &n.code, period)).collect();
    let mut child_limits = vec![Decimal::ZERO; nodes.len()];
    let mut has_children = vec![false; nodes.len()];
    for i in 0..nodes.len() {
        if let Some(p) = parent[i] {
            has_children[p] = true;
            if let Some(l) = limit[i] {
                child_limits[p] += l;
            }
        }
    }
    // Сверху вниз: «доступно» ограничено самым строгим предком.
    let mut available: Vec<Option<Decimal>> = vec![None; nodes.len()];
    let mut rows = Vec::with_capacity(nodes.len());
    for i in 0..nodes.len() {
        let forecast = fact[i] + plan[i];
        let remaining = limit[i].map(|l| l - forecast);
        let inherited = parent[i].and_then(|p| available[p]);
        available[i] = match (remaining, inherited) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let percent = limit[i].filter(|l| *l > Decimal::ZERO).and_then(|l| (forecast * Decimal::from(100) / l).round().to_u32());
        let level = match (limit[i], available[i]) {
            (Some(l), _) if forecast > l => Level::Over,
            (Some(l), _) if forecast * Decimal::from(10) >= l * Decimal::from(8) => Level::Warn,
            (Some(_), _) => Level::Ok,
            (None, Some(a)) if a < Decimal::ZERO => Level::Over,
            (None, Some(_)) => Level::Ok,
            (None, None) => Level::None,
        };
        let n = &nodes[i];
        rows.push(Row {
            code: n.code.clone(),
            title: n.title.clone(),
            kind: n.kind.clone(),
            depth: n.depth,
            parent_code: n.parent_code.clone(),
            has_children: has_children[i],
            limit: limit[i],
            fact: fact[i],
            planned: plan[i],
            forecast,
            remaining,
            available: available[i],
            percent,
            level,
            children_over_limit: limit[i].is_some_and(|l| child_limits[i] > l),
        });
    }
    rows
}

/// Курс валюты на дату: последний известный не позже даты, иначе самый ранний (для будущих дат — последний).
pub fn rate_on(rates: &[(String, NaiveDate, Decimal)], currency: &str, base: &str, date: NaiveDate) -> Option<Decimal> {
    if currency == base {
        return Some(Decimal::ONE);
    }
    let mine = rates.iter().filter(|r| r.0 == currency);
    mine.clone().filter(|r| r.1 <= date).max_by_key(|r| r.1).or_else(|| mine.min_by_key(|r| r.1)).map(|r| r.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }
    fn node(code: &str, parent: Option<&str>, depth: usize) -> Node {
        Node { code: code.into(), parent_code: parent.map(Into::into), kind: "EXPENSE".into(), title: code.into(), depth, archived: false }
    }
    fn amt(c: &str, a: &str) -> Amount {
        Amount { category_code: c.into(), amount: dec(a) }
    }
    fn lim(c: &str, v: View, a: &str) -> LimitRule {
        LimitRule { category_code: c.into(), period: v, amount: dec(a), valid_from: d(2026, 1, 1) }
    }

    #[test]
    fn periods() {
        let w = Period::containing(View::Week, d(2026, 10, 9));
        assert_eq!((w.from, w.to), (d(2026, 10, 5), d(2026, 10, 11)));
        let m = Period::containing(View::Month, d(2026, 2, 14));
        assert_eq!((m.from, m.to, m.days()), (d(2026, 2, 1), d(2026, 2, 28), 28));
        assert_eq!(m.next().from, d(2026, 3, 1));
        assert_eq!(Period::containing(View::Year, d(2026, 5, 5)).days(), 365);
    }

    #[test]
    fn limit_is_prorated_by_days() {
        let rules = vec![lim("FOOD", View::Month, "31000")];
        let week = Period::containing(View::Week, d(2026, 10, 9));
        assert_eq!(effective_limit(&rules, "FOOD", &week), Some(dec("7000")));
        let oct = Period::containing(View::Month, d(2026, 10, 9));
        assert_eq!(effective_limit(&rules, "FOOD", &oct), Some(dec("31000")));
        let year = vec![lim("CAR", View::Year, "36500")];
        assert_eq!(effective_limit(&year, "CAR", &oct), Some(dec("3100")));
    }

    #[test]
    fn latest_limit_wins() {
        let mut rules = vec![lim("FOOD", View::Month, "10000")];
        rules.push(LimitRule { valid_from: d(2026, 10, 15), ..lim("FOOD", View::Month, "12000") });
        assert_eq!(effective_limit(&rules, "FOOD", &Period::containing(View::Month, d(2026, 9, 1))), Some(dec("10000")));
        assert_eq!(effective_limit(&rules, "FOOD", &Period::containing(View::Month, d(2026, 10, 1))), Some(dec("12000")));
    }

    #[test]
    fn tree_rollup_and_available_from_ancestor() {
        // Еда (лимит 10 000) → Продукты, Кафе (лимит 5 000)
        let nodes = vec![node("FOOD", None, 0), node("GROC", Some("FOOD"), 1), node("CAFE", Some("FOOD"), 1)];
        let facts = vec![amt("GROC", "7000"), amt("CAFE", "1000"), amt("FOOD", "500")];
        let planned = vec![amt("GROC", "1000")];
        let limits = vec![lim("FOOD", View::Month, "10000"), lim("CAFE", View::Month, "5000")];
        let rows = compute(&nodes, &facts, &planned, &limits, &Period::containing(View::Month, d(2026, 10, 1)));
        let food = &rows[0];
        assert_eq!((food.fact, food.planned, food.forecast), (dec("8500"), dec("1000"), dec("9500")));
        assert_eq!(food.remaining, Some(dec("500")));
        assert_eq!(food.level, Level::Warn);
        // у кафе по своему лимиту осталось 4 000, но у «Еды» — 500
        let cafe = &rows[2];
        assert_eq!(cafe.remaining, Some(dec("4000")));
        assert_eq!(cafe.available, Some(dec("500")));
        assert_eq!(cafe.level, Level::Ok);
        // у продуктов своего лимита нет — доступно наследуется
        assert_eq!(rows[1].limit, None);
        assert_eq!(rows[1].available, Some(dec("500")));
        assert!(!food.children_over_limit);
    }

    #[test]
    fn over_limit_and_inconsistent_children() {
        let nodes = vec![node("HOME", None, 0), node("RENT", Some("HOME"), 1), node("UTIL", Some("HOME"), 1)];
        let limits = vec![lim("HOME", View::Month, "16000"), lim("RENT", View::Month, "15000"), lim("UTIL", View::Month, "3000")];
        let rows = compute(&nodes, &[amt("RENT", "15000")], &[amt("UTIL", "2500")], &limits, &Period::containing(View::Month, d(2026, 10, 1)));
        assert_eq!(rows[0].level, Level::Over);
        assert_eq!(rows[0].percent, Some(109));
        assert!(rows[0].children_over_limit, "15 000 + 3 000 > 16 000");
        assert_eq!(rows[2].available, Some(dec("-1500")));
    }

    #[test]
    fn rates() {
        let r = vec![("USD".to_string(), d(2026, 9, 1), dec("36")), ("USD".to_string(), d(2026, 10, 1), dec("33"))];
        assert_eq!(rate_on(&r, "THB", "THB", d(2026, 10, 5)), Some(Decimal::ONE));
        assert_eq!(rate_on(&r, "USD", "THB", d(2026, 9, 15)), Some(dec("36")));
        assert_eq!(rate_on(&r, "USD", "THB", d(2026, 12, 1)), Some(dec("33")));
        assert_eq!(rate_on(&r, "USD", "THB", d(2026, 1, 1)), Some(dec("36")));
        assert_eq!(rate_on(&r, "RUB", "THB", d(2026, 1, 1)), None);
    }
}
