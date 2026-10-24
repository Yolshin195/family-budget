//! Контракт HTTP API фичи «Валюты» и преобразования DTO ⇄ домен.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[allow(unused_imports)]
use crate::domain::{Currency, CurrencyChanges, CurrencyNameLang, NewCurrency};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCurrencyDto {
    /// Уникальный код: латиница, цифры и `_`, до 40 знаков
    #[schema(example = "THB")]
    pub code: String,
    pub name: String,
    /// Язык оригинала названия
    #[schema(example = "RU")]
    pub name_lang: String,
    /// Перевод названия: ru
    #[serde(default)]
    pub name_ru: Option<String>,
    /// Перевод названия: en
    #[serde(default)]
    pub name_en: Option<String>,
    /// Перевод названия: th
    #[serde(default)]
    pub name_th: Option<String>,
    /// Символ, например ฿
    pub symbol: String,
    /// Знаков после запятой
    pub minor_units: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCurrencyDto {
    pub name: String,
    #[serde(default)]
    pub archived: bool,
    /// Язык оригинала названия
    #[schema(example = "RU")]
    pub name_lang: String,
    /// Перевод названия: ru
    #[serde(default)]
    pub name_ru: Option<String>,
    /// Перевод названия: en
    #[serde(default)]
    pub name_en: Option<String>,
    /// Перевод названия: th
    #[serde(default)]
    pub name_th: Option<String>,
    /// Символ, например ฿
    pub symbol: String,
    /// Знаков после запятой
    pub minor_units: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CurrencyDto {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub archived: bool,
    /// Язык оригинала названия
    #[schema(example = "RU")]
    pub name_lang: String,
    /// Перевод названия: ru
    pub name_ru: Option<String>,
    /// Перевод названия: en
    pub name_en: Option<String>,
    /// Перевод названия: th
    pub name_th: Option<String>,
    /// Символ, например ฿
    pub symbol: String,
    /// Знаков после запятой
    pub minor_units: i32,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ListQuery {
    /// Показать и архивные записи
    #[serde(default)]
    pub include_archived: bool,
}

impl From<Currency> for CurrencyDto {
    fn from(x: Currency) -> Self {
        Self {
            id: x.id,
            code: x.code,
            name: x.name,
            archived: x.archived,
            name_lang: x.name_lang.as_str().to_string(),
            name_ru: x.name_ru,
            name_en: x.name_en,
            name_th: x.name_th,
            symbol: x.symbol,
            minor_units: x.minor_units,
        }
    }
}

impl CreateCurrencyDto {
    pub fn into_domain(self) -> Result<NewCurrency, String> {
        Ok(NewCurrency {
            code: self.code,
            name: self.name,
            name_lang: parse_name_lang(&self.name_lang)?,
            name_ru: self.name_ru,
            name_en: self.name_en,
            name_th: self.name_th,
            symbol: self.symbol,
            minor_units: self.minor_units,
        })
    }
}

impl UpdateCurrencyDto {
    pub fn into_domain(self) -> Result<CurrencyChanges, String> {
        Ok(CurrencyChanges {
            name: self.name,
            archived: self.archived,
            name_lang: parse_name_lang(&self.name_lang)?,
            name_ru: self.name_ru,
            name_en: self.name_en,
            name_th: self.name_th,
            symbol: self.symbol,
            minor_units: self.minor_units,
        })
    }
}

fn parse_name_lang(value: &str) -> Result<CurrencyNameLang, String> {
    CurrencyNameLang::parse(value).ok_or_else(|| {
        format!("name_lang: недопустимое значение '{value}', ожидается одно из {:?}", CurrencyNameLang::VALUES)
    })
}

impl CurrencyDto {
    fn localized(&self) -> kernel::i18n::Localized<'_> {
        kernel::i18n::Localized {
            original: &self.name,
            lang: kernel::i18n::Lang::parse(&self.name_lang).unwrap_or_default(),
            ru: self.name_ru.as_deref(),
            en: self.name_en.as_deref(),
            th: self.name_th.as_deref(),
        }
    }

    /// Название на языке интерфейса.
    pub fn title(&self, lang: impl Into<kernel::i18n::Lang>) -> &str {
        self.localized().show(lang.into())
    }

    pub fn missing_tr(&self, lang: impl Into<kernel::i18n::Lang>) -> bool {
        self.localized().missing(lang.into())
    }
}
