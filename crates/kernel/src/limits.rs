//! Сроки действия правил лимита — одно правило для «Обзора» (budget), страницы «Лимиты» и MCP (issue #58).
//!
//! Правило действует с начала своего периода, в который попал `valid_from` (месячный лимит, заведённый
//! 20.10, — на весь октябрь). Более позднее по `valid_from` правило категории заменяет все более ранние с
//! начала своего периода — даже если его период длиннее и начинается раньше старого правила: месячное
//! снятие с 22.10 после недельного лимита с 14.10 действует с 01.10, а недельный лимит не действует вовсе.

use chrono::{Datelike, Days, NaiveDate};

/// Вид периода лимита.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitPeriod {
    Week,
    Month,
    Year,
}

impl LimitPeriod {
    /// WEEK / MONTH / YEAR (регистр не важен); неизвестное — `None`.
    pub fn parse(s: &str) -> Option<LimitPeriod> {
        match s.trim().to_uppercase().as_str() {
            "WEEK" => Some(LimitPeriod::Week),
            "MONTH" => Some(LimitPeriod::Month),
            "YEAR" => Some(LimitPeriod::Year),
            _ => None,
        }
    }

    /// Начало периода, в который попадает дата (неделя — с понедельника).
    pub fn start(self, d: NaiveDate) -> NaiveDate {
        match self {
            LimitPeriod::Week => d - Days::new(u64::from(d.weekday().num_days_from_monday())),
            LimitPeriod::Month => d.with_day(1).unwrap_or(d),
            LimitPeriod::Year => d.with_ordinal(1).unwrap_or(d),
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
}

/// Сроки действия правил одной категории, в порядке входа; `None` — правило не действует ни дня (его
/// перекрыло более позднее правило, начавшееся не позже). Правило — (период, `valid_from`).
///
/// Правило `r` действует на `[start(r); min(start(s) для s с valid_from > r.valid_from) − 1]`.
pub fn spans(rules: &[(LimitPeriod, NaiveDate)]) -> Vec<Option<Span>> {
    rules
        .iter()
        .map(|&(period, valid_from)| {
            let from = period.start(valid_from);
            let cut = rules.iter().filter(|r| r.1 > valid_from).map(|&(p, v)| p.start(v)).min();
            match cut {
                None => Some(Span { from, to: None }),
                Some(c) if c <= from => None,
                Some(c) => Some(Span { from, to: c.pred_opt() }),
            }
        })
        .collect()
}

/// Номер правила, действующего на дату (по входу `spans`), — у категории не больше одного.
pub fn in_force(rules: &[(LimitPeriod, NaiveDate)], date: NaiveDate) -> Option<usize> {
    spans(rules).iter().position(|s| s.is_some_and(|s| s.contains(date)))
}

#[cfg(test)]
mod tests {
    use super::LimitPeriod::{Month, Week, Year};
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    fn span(from: NaiveDate, to: Option<NaiveDate>) -> Option<Span> {
        Some(Span { from, to })
    }

    #[test]
    fn period_start() {
        let fri = d(10, 16);
        assert_eq!(Week.start(fri), d(10, 12));
        assert_eq!(Month.start(fri), d(10, 1));
        assert_eq!(Year.start(fri), d(1, 1));
        assert_eq!(LimitPeriod::parse("week"), Some(Week));
        assert_eq!(LimitPeriod::parse("day"), None);
    }

    /// Табличный тест: правила (в любом порядке входа) → сроки действия.
    #[test]
    fn later_rule_cuts_earlier_from_its_period_start() {
        type Case = (&'static str, Vec<(LimitPeriod, NaiveDate)>, Vec<Option<Span>>);
        let cases: Vec<Case> = vec![
            ("одно правило — бессрочно", vec![(Month, d(10, 20))], vec![span(d(10, 1), None)]),
            (
                "месяц → месяц",
                vec![(Month, d(1, 1)), (Month, d(10, 20))],
                vec![span(d(1, 1), Some(d(9, 30))), span(d(10, 1), None)],
            ),
            (
                "месяц → неделя: месячное до понедельника",
                vec![(Month, d(10, 1)), (Week, d(10, 21))],
                vec![span(d(10, 1), Some(d(10, 18))), span(d(10, 19), None)],
            ),
            (
                "неделя → месяц: месяц начинается раньше и перекрывает неделю целиком",
                vec![(Week, d(10, 14)), (Month, d(10, 22))],
                vec![None, span(d(10, 1), None)],
            ),
            (
                "неделя с сентября → месяц: неделя до конца сентября",
                vec![(Week, d(9, 2)), (Month, d(10, 22))],
                vec![span(d(8, 31), Some(d(9, 30))), span(d(10, 1), None)],
            ),
            (
                "месяц → год: год с 01.01 своего года",
                vec![(Month, d(3, 1)), (Year, d(12, 5))],
                vec![None, span(d(1, 1), None)],
            ),
            (
                "порядок входа не важен",
                vec![(Month, d(10, 22)), (Week, d(10, 14)), (Month, d(5, 1))],
                vec![span(d(10, 1), None), None, span(d(5, 1), Some(d(9, 30)))],
            ),
        ];
        for (name, rules, want) in cases {
            assert_eq!(spans(&rules), want, "{name}");
        }
    }

    #[test]
    fn rule_in_force_on_date() {
        let rules = [(Week, d(10, 14)), (Month, d(10, 22)), (Month, d(5, 1))];
        assert_eq!(in_force(&rules, d(10, 13)), Some(1), "новое правило — с начала своего месяца");
        assert_eq!(in_force(&rules, d(9, 15)), Some(2), "сентябрь — по старому");
        assert_eq!(in_force(&rules, d(4, 30)), None, "до первого правила");
    }
}
