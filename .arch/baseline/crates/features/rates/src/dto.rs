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
    /// Сколько единиц базовой валюты за 1 единицу валюты (считается из entered и inverted)
    pub rate: rust_decimal::Decimal,
    /// Курс ровно в том виде, в каком его ввели
    pub entered: rust_decimal::Decimal,
    /// Направление ввода: false — 1 валюта = entered базовой; true — 1 базовая = entered валюты
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
            entered: x.entered,
            inverted: x.inverted,
        }
    }
}

impl RateInput {
    pub fn into_domain(self) -> Result<RateData, String> {
        Ok(RateData {
            rate_date: self.rate_date,
            currency_code: self.currency_code,
            rate: self.rate,
            entered: self.entered,
            inverted: self.inverted,
        })
    }
}
