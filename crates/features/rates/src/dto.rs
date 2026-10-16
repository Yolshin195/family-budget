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
    /// Курс в направлении `inverted`: при `false` — сколько единиц базовой валюты за 1 единицу валюты,
    /// при `true` — сколько единиц валюты за 1 единицу базовой
    pub rate: rust_decimal::Decimal,
    /// Направление ввода `rate` (по умолчанию `false`: 1 валюта = rate базовой)
    #[serde(default)]
    pub inverted: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RateDto {
    pub id: Uuid,
    /// Дата курса
    pub rate_date: chrono::NaiveDate,
    /// Валюта
    pub currency_code: String,
    /// Сколько единиц базовой валюты за 1 единицу валюты (считается из entered и inverted)
    pub rate: rust_decimal::Decimal,
    /// Курс ровно в том виде, в каком его ввели
    pub entered: rust_decimal::Decimal,
    /// Направление ввода: false — 1 валюта = entered базовой; true — 1 базовая = entered валюты
    pub inverted: bool,
    /// Сколько единиц валюты за 1 единицу базовой валюты
    pub inverse_rate: rust_decimal::Decimal,
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
            rate: x.rate.normalize(),
            entered: x.entered.normalize(),
            inverted: x.inverted,
            inverse_rate: x.inverse_rate(),
            id: x.id,
            rate_date: x.rate_date,
            currency_code: x.currency_code,
        }
    }
}

impl RateInput {
    pub fn into_domain(self) -> Result<RateData, String> {
        Ok(RateData {
            rate_date: self.rate_date,
            currency_code: self.currency_code,
            entered: self.rate,
            inverted: self.inverted,
        })
    }
}
