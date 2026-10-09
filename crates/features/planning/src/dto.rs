//! Контракт HTTP API фичи «Планирование» и преобразования DTO ⇄ домен.

use chrono::NaiveDate;
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::domain::{Name, Plan, PlanData, Recurrence};

/// Плановая операция: создание и изменение.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct PlanInput {
    /// Название на языке ввода
    #[schema(example = "Аренда квартиры")]
    pub name: String,
    #[schema(example = "RU")]
    pub name_lang: String,
    #[serde(default)]
    pub name_ru: Option<String>,
    #[serde(default)]
    pub name_en: Option<String>,
    #[serde(default)]
    pub name_th: Option<String>,
    #[schema(example = "EXPENSE")]
    pub tx_kind: String,
    pub account_code: String,
    pub category_code: String,
    /// Ожидаемая сумма в валюте счёта
    #[schema(value_type = String, example = "15000")]
    pub amount: Decimal,
    /// ONCE / DAILY / WEEKLY / MONTHLY / YEARLY
    #[schema(example = "MONTHLY")]
    pub recurrence: String,
    /// Каждые N единиц правила
    #[serde(default = "one")]
    pub interval: u32,
    pub start_on: NaiveDate,
    #[serde(default)]
    pub end_on: Option<NaiveDate>,
    #[serde(default = "yes")]
    pub active: bool,
}

fn one() -> u32 {
    1
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlanDto {
    pub id: Uuid,
    pub name: String,
    pub name_lang: String,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub tx_kind: String,
    pub account_code: String,
    pub category_code: String,
    #[schema(value_type = String)]
    pub amount: Decimal,
    pub recurrence: String,
    pub interval: u32,
    pub start_on: NaiveDate,
    pub end_on: Option<NaiveDate>,
    pub active: bool,
    pub created_by: Uuid,
}

/// Вхождение плана на конкретную дату.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OccurrenceDto {
    pub plan: PlanDto,
    /// Дата по плану
    pub date: NaiveDate,
    /// EXPECTED / OVERDUE / PAID / SKIPPED
    pub status: String,
    /// Транзакция оплаты (для PAID)
    pub transaction_id: Option<Uuid>,
    /// Фактически оплачено (для PAID)
    #[schema(value_type = Option<String>)]
    pub paid_amount: Option<Decimal>,
    pub paid_on: Option<NaiveDate>,
}

/// Период вхождений, обе границы включительно.
#[derive(Debug, Deserialize, IntoParams)]
pub struct RangeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl PlanInput {
    pub fn into_domain(self) -> Result<PlanData, String> {
        Ok(PlanData {
            name: Name {
                text: self.name,
                lang: Lang::parse(&self.name_lang).ok_or_else(|| format!("name_lang: '{}' — ожидается RU, EN или TH", self.name_lang))?,
                ru: self.name_ru,
                en: self.name_en,
                th: self.name_th,
            },
            tx_kind: self.tx_kind,
            account_code: self.account_code,
            category_code: self.category_code,
            amount: self.amount,
            recurrence: Recurrence::parse(&self.recurrence)
                .ok_or_else(|| format!("recurrence: '{}' — ожидается одно из {:?}", self.recurrence, Recurrence::VALUES))?,
            interval: self.interval,
            start_on: self.start_on,
            end_on: self.end_on,
            active: self.active,
        })
    }
}

impl From<Plan> for PlanDto {
    fn from(p: Plan) -> Self {
        Self {
            id: p.id,
            name: p.name.text,
            name_lang: p.name.lang.upper().into(),
            name_ru: p.name.ru,
            name_en: p.name.en,
            name_th: p.name.th,
            tx_kind: p.tx_kind,
            account_code: p.account_code,
            category_code: p.category_code,
            amount: p.amount,
            recurrence: p.recurrence.as_str().into(),
            interval: p.interval,
            start_on: p.start_on,
            end_on: p.end_on,
            active: p.active,
            created_by: p.created_by,
        }
    }
}

impl PlanDto {
    fn localized(&self) -> kernel::i18n::Localized<'_> {
        kernel::i18n::Localized {
            original: &self.name,
            lang: Lang::parse(&self.name_lang).unwrap_or_default(),
            ru: self.name_ru.as_deref(),
            en: self.name_en.as_deref(),
            th: self.name_th.as_deref(),
        }
    }

    pub fn title(&self, lang: impl Into<Lang>) -> &str {
        self.localized().show(lang.into())
    }

    pub fn missing_tr(&self, lang: impl Into<Lang>) -> bool {
        self.localized().missing(lang.into())
    }

    /// Текст названия для языка `lang` и его фактический язык (для описания транзакции при оплате).
    pub fn title_with_lang(&self, lang: Lang) -> (String, Lang) {
        let l = self.localized();
        if lang != l.lang && l.translation(lang).is_some() { (l.show(lang).to_string(), lang) } else { (self.name.clone(), l.lang) }
    }

    pub fn is_expense(&self) -> bool {
        self.tx_kind == "EXPENSE"
    }
}

impl OccurrenceDto {
    pub fn is_open(&self) -> bool {
        self.status == "EXPECTED" || self.status == "OVERDUE"
    }
}
