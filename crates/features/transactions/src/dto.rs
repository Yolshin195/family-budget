//! Контракт HTTP API фичи «Транзакции» и преобразования DTO ⇄ домен.

use chrono::NaiveDate;
use kernel::i18n::{Lang, Localized};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::domain::{Description, Line, Transaction, TransactionData, TxKind};

/// Позиция транзакции.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct LineDto {
    /// Категория; её тип должен совпадать с `tx_kind`
    #[schema(example = "FOOD_GROCERIES")]
    pub category_code: String,
    /// Сумма > 0 в валюте счёта
    #[schema(value_type = String, example = "900.00")]
    pub amount: Decimal,
    #[serde(default)]
    pub note: Option<String>,
}

/// Тело запроса на создание и изменение. Сумма транзакции = сумма позиций.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TransactionInput {
    pub occurred_on: NaiveDate,
    #[schema(example = "CARD_KBANK")]
    pub account_code: String,
    #[schema(example = "EXPENSE")]
    pub tx_kind: String,
    /// Описание на языке ввода
    #[serde(default)]
    pub description: Option<String>,
    /// Язык оригинала описания: RU / EN / TH
    #[schema(example = "RU")]
    pub description_lang: String,
    #[serde(default)]
    pub description_ru: Option<String>,
    #[serde(default)]
    pub description_en: Option<String>,
    #[serde(default)]
    pub description_th: Option<String>,
    /// План, вхождение которого оплачено (вместе с `plan_date`)
    #[serde(default)]
    pub plan_id: Option<Uuid>,
    #[serde(default)]
    pub plan_date: Option<NaiveDate>,
    pub lines: Vec<LineDto>,
}

/// Переводы описания (оригинал не меняется).
#[derive(Debug, Deserialize, ToSchema)]
pub struct TranslationInput {
    #[serde(default)]
    pub description_ru: Option<String>,
    #[serde(default)]
    pub description_en: Option<String>,
    #[serde(default)]
    pub description_th: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TransactionDto {
    pub id: Uuid,
    pub occurred_on: NaiveDate,
    pub account_code: String,
    #[schema(example = "EXPENSE")]
    pub tx_kind: String,
    pub description: Option<String>,
    #[schema(example = "RU")]
    pub description_lang: String,
    pub description_ru: Option<String>,
    pub description_en: Option<String>,
    pub description_th: Option<String>,
    /// Кто внёс
    pub created_by: Uuid,
    pub plan_id: Option<Uuid>,
    pub plan_date: Option<NaiveDate>,
    pub lines: Vec<LineDto>,
    /// Сумма позиций
    #[schema(value_type = String)]
    pub total: Decimal,
}

/// Фильтр списка.
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ListQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub account_code: Option<String>,
    pub created_by: Option<Uuid>,
    /// Только без перевода описания на этот язык (ru / en / th)
    pub untranslated: Option<String>,
}

impl From<Transaction> for TransactionDto {
    fn from(x: Transaction) -> Self {
        let total = x.total();
        Self {
            id: x.id,
            occurred_on: x.occurred_on,
            account_code: x.account_code,
            tx_kind: x.tx_kind.as_str().to_string(),
            description: x.description.text,
            description_lang: x.description.lang.upper().to_string(),
            description_ru: x.description.ru,
            description_en: x.description.en,
            description_th: x.description.th,
            created_by: x.created_by,
            plan_id: x.plan.map(|p| p.0),
            plan_date: x.plan.map(|p| p.1),
            lines: x.lines.into_iter().map(|l| LineDto { category_code: l.category_code, amount: l.amount, note: l.note }).collect(),
            total,
        }
    }
}

impl TransactionInput {
    pub fn into_domain(self) -> Result<TransactionData, String> {
        let tx_kind = TxKind::parse(&self.tx_kind)
            .ok_or_else(|| format!("tx_kind: недопустимое значение '{}', ожидается одно из {:?}", self.tx_kind, TxKind::VALUES))?;
        let lang = Lang::parse(&self.description_lang)
            .ok_or_else(|| format!("description_lang: недопустимое значение '{}', ожидается RU, EN или TH", self.description_lang))?;
        let plan = match (self.plan_id, self.plan_date) {
            (Some(id), Some(date)) => Some((id, date)),
            (None, None) => None,
            _ => return Err("plan_id и plan_date задаются вместе".into()),
        };
        Ok(TransactionData {
            occurred_on: self.occurred_on,
            account_code: self.account_code,
            tx_kind,
            description: Description {
                text: self.description,
                lang,
                ru: self.description_ru,
                en: self.description_en,
                th: self.description_th,
            },
            plan,
            lines: self.lines.into_iter().map(|l| Line { category_code: l.category_code, amount: l.amount, note: l.note }).collect(),
        })
    }
}

impl TransactionDto {
    fn localized(&self) -> Option<Localized<'_>> {
        self.description.as_deref().map(|original| Localized {
            original,
            lang: Lang::parse(&self.description_lang).unwrap_or_default(),
            ru: self.description_ru.as_deref(),
            en: self.description_en.as_deref(),
            th: self.description_th.as_deref(),
        })
    }

    /// Описание на языке интерфейса (пусто, если описания нет).
    pub fn title(&self, lang: impl Into<Lang>) -> &str {
        let lang = lang.into();
        self.localized().map(|l| l.show(lang)).unwrap_or("")
    }

    /// Есть описание, но нет перевода на язык интерфейса.
    pub fn missing_tr(&self, lang: impl Into<Lang>) -> bool {
        let lang = lang.into();
        self.localized().is_some_and(|l| l.missing(lang))
    }

    pub fn is_expense(&self) -> bool {
        self.tx_kind == "EXPENSE"
    }
}
