//! Контракт HTTP API фичи «Лимиты» и преобразования DTO ⇄ домен.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[allow(unused_imports)]
use crate::domain::{LimitLimitPeriod, Limit, LimitData};

/// Тело запроса на создание и изменение.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LimitInput {
    /// Действует с даты
    pub valid_from: chrono::NaiveDate,
    /// Категория (лимит на всё поддерево)
    pub category_code: String,
    /// Период лимита
    #[schema(example = "WEEK")]
    pub limit_period: String,
    /// Сумма лимита в базовой валюте проекта; 0 — тратить нельзя совсем. Не задаётся при `removed`
    #[serde(default)]
    pub amount: Option<rust_decimal::Decimal>,
    /// Снять лимит с даты: дальше лимита нет, прошлые периоды его сохраняют
    #[serde(default)]
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LimitDto {
    pub id: Uuid,
    /// Действует с даты
    pub valid_from: chrono::NaiveDate,
    /// С какого дня правило действует на деле — начало периода, в который попал `valid_from`
    pub effective_from: chrono::NaiveDate,
    /// Категория (лимит на всё поддерево)
    pub category_code: String,
    /// Период лимита
    #[schema(example = "WEEK")]
    pub limit_period: String,
    /// Сумма лимита в базовой валюте проекта; `null` — лимит снят
    pub amount: Option<rust_decimal::Decimal>,
    /// Лимит снят с `valid_from`
    pub removed: bool,
}

/// Фильтр по `valid_from`, границы включительно.
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct PeriodQuery {
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
}

impl From<Limit> for LimitDto {
    fn from(x: Limit) -> Self {
        Self {
            id: x.id,
            valid_from: x.valid_from,
            effective_from: x.effective_from(),
            category_code: x.category_code,
            limit_period: x.limit_period.as_str().to_string(),
            removed: x.amount.is_none(),
            amount: x.amount,
        }
    }
}

impl LimitInput {
    pub fn into_domain(self) -> Result<LimitData, String> {
        let amount = match (self.removed, self.amount) {
            (true, Some(_)) => return Err("amount: у снятого лимита суммы нет".into()),
            (true, None) => None,
            (false, None) => return Err("amount: укажите сумму лимита (0 — тратить нельзя совсем) или снимите лимит".into()),
            (false, Some(a)) => Some(a),
        };
        Ok(LimitData {
            valid_from: self.valid_from,
            category_code: self.category_code,
            limit_period: parse_limit_period(&self.limit_period)?,
            amount,
        })
    }
}

fn parse_limit_period(value: &str) -> Result<LimitLimitPeriod, String> {
    LimitLimitPeriod::parse(value)
        .ok_or_else(|| format!("limit_period: недопустимое значение '{value}', ожидается одно из {:?}", LimitLimitPeriod::VALUES))
}
