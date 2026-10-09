//! Доменная модель «Курсы валют». Никаких sqlx/axum/serde — только правила предметной области.

#[allow(unused_imports)]
use kernel::code::{normalize, require_text};
use uuid::Uuid;

/// Запись журнала.
#[derive(Debug, Clone, PartialEq)]
pub struct Rate {
    pub id: Uuid,
    pub project_id: Uuid,
    pub rate_date: chrono::NaiveDate,
    pub currency_code: String,
    pub rate: rust_decimal::Decimal,
}

/// Данные записи, которые задаёт пользователь (для создания и изменения).
#[derive(Debug, Clone)]
pub struct RateData {
    pub rate_date: chrono::NaiveDate,
    pub currency_code: String,
    pub rate: rust_decimal::Decimal,
}

impl Rate {
    pub fn create(project_id: Uuid, data: RateData) -> Result<Self, String> {
        let mut item = Self {
            id: Uuid::new_v4(),
            project_id,
            rate_date: data.rate_date.clone(),
            currency_code: data.currency_code.clone(),
            rate: data.rate.clone(),
        };
        item.change(data)?;
        Ok(item)
    }

    pub fn change(&mut self, data: RateData) -> Result<(), String> {
        self.rate_date = data.rate_date;
        self.currency_code = normalize("currency_code", &data.currency_code)?;
        self.rate = data.rate;
        Ok(())
    }
}
