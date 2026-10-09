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
}

/// Запись журнала.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    pub id: Uuid,
    pub project_id: Uuid,
    pub valid_from: chrono::NaiveDate,
    pub category_code: String,
    pub limit_period: LimitLimitPeriod,
    pub amount: rust_decimal::Decimal,
}

/// Данные записи, которые задаёт пользователь (для создания и изменения).
#[derive(Debug, Clone)]
pub struct LimitData {
    pub valid_from: chrono::NaiveDate,
    pub category_code: String,
    pub limit_period: LimitLimitPeriod,
    pub amount: rust_decimal::Decimal,
}

impl Limit {
    pub fn create(project_id: Uuid, data: LimitData) -> Result<Self, String> {
        positive(data.amount)?;
        let mut item = Self {
            id: Uuid::new_v4(),
            project_id,
            valid_from: data.valid_from.clone(),
            category_code: data.category_code.clone(),
            limit_period: data.limit_period.clone(),
            amount: data.amount.clone(),
        };
        item.change(data)?;
        Ok(item)
    }

    pub fn change(&mut self, data: LimitData) -> Result<(), String> {
        positive(data.amount)?;
        self.valid_from = data.valid_from;
        self.category_code = normalize("category_code", &data.category_code)?;
        self.limit_period = data.limit_period;
        self.amount = data.amount;
        Ok(())
    }
}

fn positive(amount: rust_decimal::Decimal) -> Result<(), String> {
    if amount <= rust_decimal::Decimal::ZERO {
        return Err("amount: лимит должен быть больше нуля".into());
    }
    Ok(())
}
