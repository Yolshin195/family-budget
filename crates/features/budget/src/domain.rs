//! Расчёт бюджета: факт, план, лимит по дереву категорий за неделю / месяц / год.
//! Чистые функции без ввода-вывода — всё, что нужно, приходит параметрами.

use std::cmp::Ordering;
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
    /// `None` — «лимит снят» с `valid_from`; 0 — настоящий нулевой лимит (тратить нельзя совсем).
    pub amount: Option<Decimal>,
    pub valid_from: NaiveDate,
}

/// Лимит, приведённый к периоду отчёта.
///
/// Правило действует с начала своего периода, в который попал `valid_from` (месячный лимит, заведённый
/// 20.10, — на весь октябрь), до начала периода следующего правила этой категории. Вклад правила — его
/// сумма × доля каждого его периода, попавшая в отчёт и в срок правила: месячный 31 000 в недельном отчёте
/// за 05.10–11.10 = 31 000 × 7/31 = 7 000, в годовом — 12 × 31 000. Округление — один раз, у итога.
/// «Лимит снят» вклада не даёт; если в отчёт не попало ни одного действующего правила — лимита нет.
pub fn effective_limit(rules: &[LimitRule], category: &str, period: &Period) -> Option<Decimal> {
    let start = |r: &LimitRule| Period::containing(r.period, r.valid_from).from;
    let mut mine: Vec<&LimitRule> = rules.iter().filter(|r| r.category_code == category).collect();
    mine.sort_by_key(|r| (start(r), r.valid_from));
    let mut total: Option<Decimal> = None;
    for (i, r) in mine.iter().enumerate() {
        let Some(amount) = r.amount else { continue };
        let from = start(r).max(period.from);
        // Срок правила кончается накануне периода следующего; следующее с тем же началом заменяет его целиком.
        let to = match mine.get(i + 1).map(|next| start(next).pred_opt()) {
            Some(Some(end)) => end.min(period.to),
            Some(None) => continue,
            None => period.to,
        };
        if from > to {
            continue;
        }
        let mut sum = Decimal::ZERO;
        let mut p = Period::containing(r.period, from);
        loop {
            let days = (p.to.min(to) - p.from.max(from)).num_days() + 1;
            sum += amount * Decimal::from(days) / Decimal::from(p.days());
            if p.to >= to {
                break;
            }
            p = p.next();
        }
        *total.get_or_insert(Decimal::ZERO) += sum;
    }
    total.map(|t| t.round_dp(2))
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
            // У нулевого лимита «почти исчерпан» не бывает: любая трата — сразу перерасход.
            (Some(l), _) if l > Decimal::ZERO && forecast * Decimal::from(10) >= l * Decimal::from(8) => Level::Warn,
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

/// Колонка сортировки таблицы бюджета.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Category,
    Limit,
    Fact,
    Plan,
    Forecast,
    Available,
    Percent,
}

impl SortKey {
    pub const ALL: [SortKey; 7] = [SortKey::Category, SortKey::Limit, SortKey::Fact, SortKey::Plan, SortKey::Forecast, SortKey::Available, SortKey::Percent];

    pub fn parse(s: &str) -> Option<SortKey> {
        SortKey::ALL.into_iter().find(|k| k.code() == s)
    }

    pub fn code(self) -> &'static str {
        match self {
            SortKey::Category => "category",
            SortKey::Limit => "limit",
            SortKey::Fact => "fact",
            SortKey::Plan => "plan",
            SortKey::Forecast => "forecast",
            SortKey::Available => "available",
            SortKey::Percent => "percent",
        }
    }

    /// Первый клик: названия — по алфавиту, суммы — крупные сверху.
    pub fn default_dir(self) -> Dir {
        if self == SortKey::Category { Dir::Asc } else { Dir::Desc }
    }
}

/// Направление сортировки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Asc,
    Desc,
}

impl Dir {
    pub fn parse(s: &str) -> Option<Dir> {
        match s {
            "asc" => Some(Dir::Asc),
            "desc" => Some(Dir::Desc),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Dir::Asc => "asc",
            Dir::Desc => "desc",
        }
    }
}

/// Сортировка после клика по колонке `col`: направление колонки по умолчанию → обратное → исходный порядок.
pub fn next_sort(current: Option<(SortKey, Dir)>, col: SortKey) -> Option<(SortKey, Dir)> {
    match current {
        Some((k, d)) if k == col && d == col.default_dir() => Some((col, if d == Dir::Asc { Dir::Desc } else { Dir::Asc })),
        Some((k, _)) if k == col => None,
        _ => Some((col, col.default_dir())),
    }
}

/// Значение строки для сортировки; `None` — пустая ячейка.
#[derive(PartialEq, PartialOrd)]
enum SortValue {
    Text(String),
    Num(Decimal),
}

fn sort_value(r: &Row, key: SortKey) -> Option<SortValue> {
    let num = |v: Option<Decimal>| v.map(SortValue::Num);
    match key {
        SortKey::Category => Some(SortValue::Text(r.title.to_lowercase().replace('ё', "е"))),
        SortKey::Limit => num(r.limit),
        SortKey::Fact => num(Some(r.fact)),
        // Нулевой план в таблице не показывается — это пустая ячейка.
        SortKey::Plan => num(Some(r.planned).filter(|p| !p.is_zero())),
        SortKey::Forecast => num(Some(r.forecast)),
        SortKey::Available => num(r.available),
        SortKey::Percent => num(r.percent.map(Decimal::from)),
    }
}

/// Отсортировать строки (в порядке обхода в глубину) внутри каждого уровня дерева: меняется только порядок
/// братьев, дети остаются сразу под своим родителем. Пустые значения — в конце при любом направлении,
/// при равенстве сохраняется исходный порядок.
pub fn sort_tree(rows: Vec<Row>, key: SortKey, dir: Dir) -> Vec<Row> {
    let index: HashMap<&str, usize> = rows.iter().enumerate().map(|(i, r)| (r.code.as_str(), i)).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); rows.len()];
    let mut roots = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        match r.parent_code.as_deref().and_then(|p| index.get(p)) {
            Some(&p) => children[p].push(i),
            None => roots.push(i),
        }
    }
    let values: Vec<Option<SortValue>> = rows.iter().map(|r| sort_value(r, key)).collect();
    let cmp = |a: &usize, b: &usize| match (&values[*a], &values[*b]) {
        (Some(x), Some(y)) => {
            let o = x.partial_cmp(y).unwrap_or(Ordering::Equal);
            if dir == Dir::Desc { o.reverse() } else { o }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    roots.sort_by(cmp);
    for c in &mut children {
        c.sort_by(cmp);
    }
    let mut order = Vec::with_capacity(rows.len());
    let mut stack: Vec<usize> = roots.into_iter().rev().collect();
    while let Some(i) = stack.pop() {
        order.push(i);
        stack.extend(children[i].iter().rev());
    }
    let mut slots: Vec<Option<Row>> = rows.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
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
        LimitRule { category_code: c.into(), period: v, amount: Some(dec(a)), valid_from: d(2026, 1, 1) }
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

    /// Еда → (Продукты → Овощи, Кафе), Дом → (Аренда, Свет), Связь.
    fn sample_rows() -> Vec<Row> {
        let nodes = vec![
            node("FOOD", None, 0),
            node("GROC", Some("FOOD"), 1),
            node("VEG", Some("GROC"), 2),
            node("CAFE", Some("FOOD"), 1),
            node("HOME", None, 0),
            node("RENT", Some("HOME"), 1),
            node("UTIL", Some("HOME"), 1),
            node("PHONE", None, 0),
        ];
        let facts = vec![amt("VEG", "100"), amt("GROC", "50"), amt("CAFE", "240"), amt("RENT", "300"), amt("UTIL", "80"), amt("PHONE", "20")];
        let planned = vec![amt("UTIL", "40"), amt("PHONE", "10")];
        let limits = vec![lim("FOOD", View::Month, "500"), lim("RENT", View::Month, "300"), lim("PHONE", View::Month, "100")];
        compute(&nodes, &facts, &planned, &limits, &Period::containing(View::Month, d(2026, 10, 1)))
    }
    fn codes(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|r| r.code.as_str()).collect()
    }
    /// Порядок — обход в глубину: между строкой и её родителем — только потомки родителя.
    fn assert_tree_order(rows: &[Row]) {
        for (i, r) in rows.iter().enumerate() {
            let Some(p) = r.parent_code.as_deref() else { continue };
            let pi = rows.iter().position(|x| x.code == p).expect("родитель в таблице");
            assert!(pi < i, "{} должен идти под {p}", r.code);
            assert!(rows[pi + 1..i].iter().all(|x| x.depth > rows[pi].depth), "{} оторван от {p}", r.code);
        }
    }

    #[test]
    fn sort_tree_sorts_siblings_only() {
        // факт: Еда 390 (Продукты 150, Кафе 240), Дом 380 (Аренда 300, Свет 80), Связь 20
        let rows = sort_tree(sample_rows(), SortKey::Fact, Dir::Desc);
        assert_eq!(codes(&rows), ["FOOD", "CAFE", "GROC", "VEG", "HOME", "RENT", "UTIL", "PHONE"]);
        assert_tree_order(&rows);
        let rows = sort_tree(sample_rows(), SortKey::Fact, Dir::Asc);
        assert_eq!(codes(&rows), ["PHONE", "HOME", "UTIL", "RENT", "FOOD", "GROC", "VEG", "CAFE"]);
        assert_tree_order(&rows);
    }

    #[test]
    fn sort_tree_empty_last_both_directions() {
        // лимиты: Еда 500, Связь 100, у Дома нет; среди детей Дома лимит только у Аренды
        for dir in [Dir::Desc, Dir::Asc] {
            let rows = sort_tree(sample_rows(), SortKey::Limit, dir);
            let roots: Vec<&str> = rows.iter().filter(|r| r.depth == 0).map(|r| r.code.as_str()).collect();
            assert_eq!(roots.last(), Some(&"HOME"), "{dir:?}: без лимита — в конце");
            let home_kids: Vec<&str> = rows.iter().filter(|r| r.parent_code.as_deref() == Some("HOME")).map(|r| r.code.as_str()).collect();
            assert_eq!(home_kids, ["RENT", "UTIL"], "{dir:?}");
            assert_tree_order(&rows);
        }
        // план есть только у Света и Связи (и у Дома как суммы); нулевой план — пустой
        for dir in [Dir::Desc, Dir::Asc] {
            let rows = sort_tree(sample_rows(), SortKey::Plan, dir);
            assert_eq!(rows.last().map(|r| r.code.as_str()), Some("CAFE"), "{dir:?}: Еда без плана — после Дома и Связи");
            assert_eq!(rows.iter().rfind(|r| r.depth == 0).map(|r| r.code.as_str()), Some("FOOD"), "{dir:?}");
        }
    }

    #[test]
    fn sort_tree_category_alphabetical_and_stable() {
        let mut rows = sample_rows();
        for (r, t) in rows.iter_mut().zip(["еда", "Продукты", "Овощи", "Кафе", "Ёлка", "Аренда", "Свет", "Жильё"]) {
            r.title = t.into();
        }
        // регистр не важен, «Ёлка» сравнивается как «Елка»: между «еда» и «Жильё»
        assert_eq!(codes(&sort_tree(rows.clone(), SortKey::Category, Dir::Asc)), ["FOOD", "CAFE", "GROC", "VEG", "HOME", "RENT", "UTIL", "PHONE"]);
        assert_eq!(codes(&sort_tree(rows, SortKey::Category, Dir::Desc)), ["PHONE", "HOME", "UTIL", "RENT", "FOOD", "GROC", "VEG", "CAFE"]);
        // при равенстве — исходный порядок, в обе стороны
        for dir in [Dir::Asc, Dir::Desc] {
            let same = sort_tree(sample_rows(), SortKey::Percent, dir);
            let kids: Vec<&str> = same.iter().filter(|r| r.parent_code.as_deref() == Some("FOOD")).map(|r| r.code.as_str()).collect();
            assert_eq!(kids, ["GROC", "CAFE"], "{dir:?}: процента нет ни у кого — порядок справочника");
        }
    }

    #[test]
    fn sort_click_cycle() {
        use SortKey::*;
        assert_eq!(next_sort(None, Fact), Some((Fact, Dir::Desc)), "суммы — сначала крупные");
        assert_eq!(next_sort(Some((Fact, Dir::Desc)), Fact), Some((Fact, Dir::Asc)));
        assert_eq!(next_sort(Some((Fact, Dir::Asc)), Fact), None, "третий клик — исходный порядок");
        assert_eq!(next_sort(None, Category), Some((Category, Dir::Asc)), "названия — по алфавиту");
        assert_eq!(next_sort(Some((Category, Dir::Asc)), Category), Some((Category, Dir::Desc)));
        assert_eq!(next_sort(Some((Category, Dir::Desc)), Category), None);
        assert_eq!(next_sort(Some((Fact, Dir::Asc)), Limit), Some((Limit, Dir::Desc)), "другая колонка — с её направления");
        assert_eq!(SortKey::ALL.map(|k| SortKey::parse(k.code())), SortKey::ALL.map(Some));
        assert_eq!((SortKey::parse("title"), Dir::parse("up")), (None, None));
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

    fn coffee(valid_from: NaiveDate) -> Vec<LimitRule> {
        vec![LimitRule { category_code: "FOOD_COFFEE".into(), period: View::Month, amount: Some(dec("1500")), valid_from }]
    }

    /// Ц1: месячный лимит в годовом отчёте — 12 месяцев, а не 365/31.
    #[test]
    fn monthly_limit_in_year_report_is_twelve_months() {
        let year = Period::containing(View::Year, d(2026, 10, 19));
        assert_eq!(effective_limit(&coffee(d(2026, 1, 1)), "FOOD_COFFEE", &year), Some(dec("18000")));
    }

    /// Ц5: лимит, заведённый 20.10, не действует на январь–сентябрь, но действует на весь октябрь.
    #[test]
    fn limit_counts_from_its_month_only() {
        let year = Period::containing(View::Year, d(2026, 10, 19));
        assert_eq!(effective_limit(&coffee(d(2026, 10, 20)), "FOOD_COFFEE", &year), Some(dec("4500")));
        let oct = Period::containing(View::Month, d(2026, 10, 1));
        assert_eq!(effective_limit(&coffee(d(2026, 10, 20)), "FOOD_COFFEE", &oct), Some(dec("1500")));
        let sep = Period::containing(View::Month, d(2026, 9, 1));
        assert_eq!(effective_limit(&coffee(d(2026, 10, 20)), "FOOD_COFFEE", &sep), None);
    }

    /// Неделя 28.09–04.10 — 3 дня сентября и 4 дня октября: 1 500 × 3/30 + 1 500 × 4/31.
    #[test]
    fn week_across_months_takes_both_months() {
        let week = Period::containing(View::Week, d(2026, 10, 1));
        assert_eq!((week.from, week.to), (d(2026, 9, 28), d(2026, 10, 4)));
        assert_eq!(effective_limit(&coffee(d(2026, 1, 1)), "FOOD_COFFEE", &week), Some(dec("343.55")));
    }

    /// 3 000 с января, 4 000 с 20.10: январь–сентябрь по старому правилу, октябрь–декабрь — по новому.
    #[test]
    fn rule_change_inside_year() {
        let mut rules = vec![lim("FOOD", View::Month, "3000")];
        rules.push(LimitRule { valid_from: d(2026, 10, 20), ..lim("FOOD", View::Month, "4000") });
        let year = Period::containing(View::Year, d(2026, 5, 5));
        assert_eq!(effective_limit(&rules, "FOOD", &year), Some(dec("39000")));
        // Недельный со среды 21.10 действует с понедельника 19.10: месячный — 18 дней октября, недельный — 7 + 6.
        rules.push(LimitRule { valid_from: d(2026, 10, 21), ..lim("FOOD", View::Week, "700") });
        let oct = Period::containing(View::Month, d(2026, 10, 1));
        assert_eq!(effective_limit(&rules, "FOOD", &oct), Some(dec("3622.58")), "4 000 × 18/31 + 700 + 700 × 6/7");
    }

    /// Лимит 0 — тратить нельзя совсем: любая трата — перерасход, без трат — в норме.
    #[test]
    fn zero_limit_means_no_spending() {
        let nodes = vec![node("DELIVERY", None, 0)];
        let rules = vec![lim("DELIVERY", View::Month, "0")];
        let oct = Period::containing(View::Month, d(2026, 10, 1));
        let spent = compute(&nodes, &[amt("DELIVERY", "1")], &[], &rules, &oct);
        assert_eq!((spent[0].limit, spent[0].level, spent[0].percent), (Some(Decimal::ZERO), Level::Over, None));
        let clean = compute(&nodes, &[], &[], &rules, &oct);
        assert_eq!((clean[0].level, clean[0].available), (Level::Ok, Some(Decimal::ZERO)));
    }

    /// «Лимит снят» с 01.11: октябрь его сохраняет, ноябрь без лимита, год — только действующие месяцы.
    #[test]
    fn removed_limit_keeps_the_past() {
        let mut rules = coffee(d(2026, 1, 1));
        rules.push(LimitRule { category_code: "FOOD_COFFEE".into(), period: View::Month, amount: None, valid_from: d(2026, 11, 1) });
        let at = |v: View, day: NaiveDate| effective_limit(&rules, "FOOD_COFFEE", &Period::containing(v, day));
        assert_eq!(at(View::Month, d(2026, 10, 1)), Some(dec("1500")));
        assert_eq!(at(View::Month, d(2026, 11, 1)), None);
        assert_eq!(at(View::Year, d(2026, 1, 1)), Some(dec("15000")));
        assert_eq!(at(View::Year, d(2027, 1, 1)), None, "снятый лимит не возвращается сам");
    }
}
