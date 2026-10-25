//! Сроки действия правил лимита — одно правило для всех фич («Обзор», «Лимиты», MCP).
//!
//! Правило лимита категории действует с начала своего периода, в который попал `valid_from` (месячный
//! лимит, заведённый 20.10, — на весь октябрь). Более позднее по `valid_from` правило заменяет все более
//! ранние с начала своего периода, какой бы ни был вид периода: месячное правило после недельного
//! «обрезает» недельное с 1-го числа; если новое начинается раньше старого, старое не действует вовсе.

use chrono::{Datelike, Days, NaiveDate};

/// Вид периода лимита.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitPeriod {
    Week,
    Month,
    Year,
}

impl LimitPeriod {
    /// `WEEK` / `MONTH` / `YEAR` в любом регистре.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "WEEK" => Some(Self::Week),
            "MONTH" => Some(Self::Month),
            "YEAR" => Some(Self::Year),
            _ => None,
        }
    }

    /// Начало периода, в который попадает дата (неделя — с понедельника).
    pub fn start(self, d: NaiveDate) -> NaiveDate {
        match self {
            Self::Week => d - Days::new(u64::from(d.weekday().num_days_from_monday())),
            Self::Month => d.with_day(1).unwrap_or(d),
            Self::Year => d.with_ordinal(1).unwrap_or(d),
        }
    }
}

/// Срок действия правила, обе границы включительно; `to = None` — бессрочно.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: NaiveDate,
    pub to: Option<NaiveDate>,
}

impl Span {
    pub fn contains(&self, d: NaiveDate) -> bool {
        self.from <= d && self.to.is_none_or(|t| d <= t)
    }

    /// Срок кончился до даты `d`.
    pub fn ended_before(&self, d: NaiveDate) -> bool {
        self.to.is_some_and(|t| t < d)
    }
}

/// Сроки действия правил одной категории; `rules` — пары (начало периода правила, `valid_from`) в любом
/// порядке, результат — в том же порядке. Правило `r` действует на `[start(r); min(start(s)) − 1]` по всем
/// `s` с `valid_from` позже, чем у `r`; `None` — более позднее правило начинается не позже, и `r` не
/// действует ни дня.
pub fn spans(rules: &[(NaiveDate, NaiveDate)]) -> Vec<Option<Span>> {
    rules
        .iter()
        .map(|&(start, valid_from)| {
            let cut = rules.iter().filter(|s| s.1 > valid_from).map(|s| s.0).min();
            match cut {
                None => Some(Span { from: start, to: None }),
                Some(c) if c > start => c.pred_opt().map(|to| Span { from: start, to: Some(to) }),
                Some(_) => None,
            }
        })
        .collect()
}

/// Индекс правила, действующего на дату `d` (не больше одного), в списке как у [`spans`].
pub fn in_force(rules: &[(NaiveDate, NaiveDate)], d: NaiveDate) -> Option<usize> {
    spans(rules).iter().position(|s| s.is_some_and(|s| s.contains(d)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn span(from: NaiveDate, to: Option<NaiveDate>) -> Option<Span> {
        Some(Span { from, to })
    }

    #[test]
    fn period_start() {
        let fri = d(2026, 10, 16);
        assert_eq!(LimitPeriod::Week.start(fri), d(2026, 10, 12));
        assert_eq!(LimitPeriod::Month.start(fri), d(2026, 10, 1));
        assert_eq!(LimitPeriod::Year.start(fri), d(2026, 1, 1));
        assert_eq!(LimitPeriod::parse("month"), Some(LimitPeriod::Month));
        assert_eq!(LimitPeriod::parse("day"), None);
    }

    /// Табличный тест: (правила (начало, valid_from), ожидаемые сроки).
    #[test]
    fn later_rule_cuts_earlier_from_its_start() {
        type Case = (&'static str, Vec<(NaiveDate, NaiveDate)>, Vec<Option<Span>>);
        let cases: Vec<Case> = vec![
            ("одно правило — бессрочно", vec![(d(2026, 10, 1), d(2026, 10, 20))], vec![span(d(2026, 10, 1), None)]),
            (
                "месяц → месяц: старое до конца прошлого месяца",
                vec![(d(2026, 9, 1), d(2026, 9, 1)), (d(2026, 10, 1), d(2026, 10, 1))],
                vec![span(d(2026, 9, 1), Some(d(2026, 9, 30))), span(d(2026, 10, 1), None)],
            ),
            (
                "месяц → неделя: месяц до воскресенья перед неделей",
                vec![(d(2026, 10, 1), d(2026, 10, 1)), (d(2026, 10, 12), d(2026, 10, 14))],
                vec![span(d(2026, 10, 1), Some(d(2026, 10, 11))), span(d(2026, 10, 12), None)],
            ),
            (
                "неделя → месяц: месяц начинается раньше — неделя не действует вовсе",
                vec![(d(2026, 10, 12), d(2026, 10, 14)), (d(2026, 10, 1), d(2026, 10, 22))],
                vec![None, span(d(2026, 10, 1), None)],
            ),
            (
                "порядок входа не важен",
                vec![(d(2026, 10, 1), d(2026, 10, 22)), (d(2026, 10, 12), d(2026, 10, 14))],
                vec![span(d(2026, 10, 1), None), None],
            ),
            (
                "месяц → год: год заменяет месячные с 1 января",
                vec![(d(2025, 12, 1), d(2025, 12, 1)), (d(2026, 3, 1), d(2026, 3, 1)), (d(2026, 1, 1), d(2026, 6, 10))],
                vec![span(d(2025, 12, 1), Some(d(2025, 12, 31))), None, span(d(2026, 1, 1), None)],
            ),
            (
                "одно начало — действует более позднее по дате",
                vec![(d(2026, 10, 1), d(2026, 10, 5)), (d(2026, 10, 1), d(2026, 10, 20))],
                vec![None, span(d(2026, 10, 1), None)],
            ),
        ];
        for (name, rules, want) in cases {
            assert_eq!(spans(&rules), want, "{name}");
        }
    }

    #[test]
    fn rule_in_force_on_date() {
        let rules =
            vec![(d(2026, 9, 1), d(2026, 9, 1)), (d(2026, 10, 12), d(2026, 10, 14)), (d(2026, 10, 1), d(2026, 10, 22))];
        assert_eq!(in_force(&rules, d(2026, 9, 15)), Some(0));
        assert_eq!(in_force(&rules, d(2026, 10, 13)), Some(2));
        assert_eq!(in_force(&rules, d(2026, 8, 31)), None);
        assert!(spans(&rules)[0].unwrap().ended_before(d(2026, 10, 1)));
        assert!(!spans(&rules)[2].unwrap().ended_before(d(2099, 1, 1)));
    }
}
