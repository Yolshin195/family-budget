//! Курс валюты к базовой на дату — одно правило для всех фич (этап 0, Ц3; первый шаг техдолга №22).

use chrono::NaiveDate;
use rust_decimal::Decimal;

/// Курс `currency` к базовой валюте `base` (единиц базовой за 1 единицу валюты) на дату: последний
/// известный не позже даты, для дат раньше первого курса — самый ранний. `None` — у валюты нет ни
/// одного курса: такую сумму нельзя пересчитать, и её нельзя выдавать за сумму в базовой валюте.
/// `rates` — (валюта, дата, курс).
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

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    #[test]
    fn latest_rate_not_after_date() {
        let r = vec![
            ("RUB".to_string(), d(10, 16), Decimal::new(383083, 6)),
            ("RUB".to_string(), d(10, 18), Decimal::new(379435, 6)),
        ];
        assert_eq!(rate_on(&r, "THB", "THB", d(10, 1)), Some(Decimal::ONE));
        assert_eq!(rate_on(&r, "RUB", "THB", d(10, 17)), Some(Decimal::new(383083, 6)));
        assert_eq!(rate_on(&r, "RUB", "THB", d(10, 19)), Some(Decimal::new(379435, 6)));
        assert_eq!(
            rate_on(&r, "RUB", "THB", d(10, 15)),
            Some(Decimal::new(383083, 6)),
            "до первого курса — самый ранний"
        );
        assert_eq!(rate_on(&r, "USD", "THB", d(10, 17)), None, "курса нет — суммы нет");
    }
}
