//! Контракт HTTP API фичи «Курсы валют» и преобразования DTO ⇄ домен.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[allow(unused_imports)]
use crate::domain::{Rate, RateData};

/// Тело запроса на создание и изменение.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RateInput {
    /// Дата курса
    pub rate_date: chrono::NaiveDate,
    /// Валюта
    pub currency_code: String,
    /// Сколько единиц базовой валюты за 1 единицу валюты
    pub rate: rust_decimal::Decimal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RateDto {
    pub id: Uuid,
    /// Дата курса
    pub rate_date: chrono::NaiveDate,
    /// Валюта
    pub currency_code: String,
    /// Сколько единиц базовой валюты за 1 единицу валюты
    pub rate: rust_decimal::Decimal,
}

/// Фильтр по `rate_date`, границы включительно.
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct PeriodQuery {
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
}

impl From<Rate> for RateDto {
    fn from(x: Rate) -> Self {
        Self {
            id: x.id,
            rate_date: x.rate_date,
            currency_code: x.currency_code,
            rate: x.rate,
        }
    }
}

impl RateInput {
    pub fn into_domain(self) -> Result<RateData, String> {
        Ok(RateData {
            rate_date: self.rate_date,
            currency_code: self.currency_code,
            rate: self.rate,
        })
    }
}
