//! Доменная модель «Лимиты». Никаких sqlx/axum/serde — только правила предметной области.

#[allow(unused_imports)]
use kernel::code::{normalize, require_text};
use uuid::Uuid;

/// Период лимита
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitLimitPeriod {
    Week,
    Month,
    Year,
}

impl LimitLimitPeriod {
    pub const VALUES: &'static [&'static str] = &["WEEK", "MONTH", "YEAR"];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Week => "WEEK",
            Self::Month => "MONTH",
            Self::Year => "YEAR",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "WEEK" => Some(Self::Week),
            "MONTH" => Some(Self::Month),
            "YEAR" => Some(Self::Year),
            _ => None,
        }
    }

    /// Начало периода, в который попадает дата (неделя — с понедельника).
    pub fn start(&self, d: chrono::NaiveDate) -> chrono::NaiveDate {
        use kernel::limits::LimitPeriod as P;
        match self {
            Self::Week => P::Week,
            Self::Month => P::Month,
            Self::Year => P::Year,
        }
        .start(d)
    }
}

/// Запись журнала.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    pub id: Uuid,
    pub project_id: Uuid,
    pub valid_from: chrono::NaiveDate,
    pub category_code: String,
    pub limit_period: LimitLimitPeriod,
    /// Сумма за период в базовой валюте; 0 — тратить нельзя совсем. `None` — «лимит снят» с этой даты:
    /// дальше лимита нет, прошлые периоды его сохраняют.
    pub amount: Option<rust_decimal::Decimal>,
}

impl Limit {
    /// С какого дня правило действует на деле: с начала периода, в который попал `valid_from`
    /// (месячный лимит, заведённый 20.10, — на весь октябрь).
    pub fn effective_from(&self) -> chrono::NaiveDate {
        self.limit_period.start(self.valid_from)
    }
}

/// Данные записи, которые задаёт пользователь (для создания и изменения).
#[derive(Debug, Clone)]
pub struct LimitData {
    pub valid_from: chrono::NaiveDate,
    pub category_code: String,
    pub limit_period: LimitLimitPeriod,
    pub amount: Option<rust_decimal::Decimal>,
}

impl Limit {
    pub fn create(project_id: Uuid, data: LimitData) -> Result<Self, String> {
        not_negative(data.amount)?;
        let mut item = Self {
            id: Uuid::new_v4(),
            project_id,
            valid_from: data.valid_from,
            category_code: data.category_code.clone(),
            limit_period: data.limit_period,
            amount: data.amount,
        };
        item.change(data)?;
        Ok(item)
    }

    pub fn change(&mut self, data: LimitData) -> Result<(), String> {
        not_negative(data.amount)?;
        self.valid_from = data.valid_from;
        self.category_code = normalize("category_code", &data.category_code)?;
        self.limit_period = data.limit_period;
        self.amount = data.amount;
        Ok(())
    }
}

fn not_negative(amount: Option<rust_decimal::Decimal>) -> Result<(), String> {
    if amount.is_some_and(|a| a < rust_decimal::Decimal::ZERO) {
        return Err("amount: лимит не может быть отрицательным (0 — тратить нельзя совсем)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn data(amount: Option<Decimal>) -> LimitData {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        LimitData { valid_from: d, category_code: "delivery".into(), limit_period: LimitLimitPeriod::Month, amount }
    }

    /// Ц5: 0 — настоящий нулевой лимит, «снять лимит» — отдельное правило без суммы.
    #[test]
    fn zero_limit_and_removed_limit() {
        let zero = Limit::create(Uuid::nil(), data(Some(Decimal::ZERO))).unwrap();
        assert_eq!((zero.amount, zero.category_code.as_str()), (Some(Decimal::ZERO), "DELIVERY"));
        assert_eq!(Limit::create(Uuid::nil(), data(None)).unwrap().amount, None);
        assert!(Limit::create(Uuid::nil(), data(Some(-Decimal::ONE))).is_err());
    }

    #[test]
    fn effective_from_is_period_start() {
        let mut l = Limit::create(Uuid::nil(), data(Some(Decimal::ONE))).unwrap();
        assert_eq!(l.effective_from().to_string(), "2026-10-01");
        l.limit_period = LimitLimitPeriod::Week;
        assert_eq!(l.effective_from().to_string(), "2026-10-19", "20.10.2026 — вторник");
        l.limit_period = LimitLimitPeriod::Year;
        assert_eq!(l.effective_from().to_string(), "2026-01-01");
    }
}
