//! Доменная модель «Валюты». Никаких sqlx/axum/serde — только правила предметной области.

use kernel::code::{new_code, require_text};
use uuid::Uuid;

/// Язык оригинала названия
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrencyNameLang {
    Ru,
    En,
    Th,
}

impl CurrencyNameLang {
    pub const VALUES: &'static [&'static str] = &["RU", "EN", "TH"];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ru => "RU",
            Self::En => "EN",
            Self::Th => "TH",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "RU" => Some(Self::Ru),
            "EN" => Some(Self::En),
            "TH" => Some(Self::Th),
            _ => None,
        }
    }
}

/// Запись справочника. Уникальна в пределах проекта по `code`.
#[derive(Debug, Clone, PartialEq)]
pub struct Currency {
    pub id: Uuid,
    pub project_id: Uuid,
    pub code: String,
    pub name: String,
    pub archived: bool,
    pub name_lang: CurrencyNameLang,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub symbol: String,
    pub minor_units: i32,
}

/// Данные для создания записи.
#[derive(Debug, Clone)]
pub struct NewCurrency {
    pub code: String,
    pub name: String,
    pub name_lang: CurrencyNameLang,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub symbol: String,
    pub minor_units: i32,
}

/// Данные для изменения записи (код — ключ, не меняется).
#[derive(Debug, Clone)]
pub struct CurrencyChanges {
    pub name: String,
    pub archived: bool,
    pub name_lang: CurrencyNameLang,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub symbol: String,
    pub minor_units: i32,
}

/// Знаков после запятой — не больше 4: суммы хранятся в NUMERIC(18,4).
pub const MAX_MINOR_UNITS: i32 = 4;

fn minor_units(v: i32) -> Result<i32, String> {
    if (0..=MAX_MINOR_UNITS).contains(&v) {
        Ok(v)
    } else {
        Err(format!("minor_units: целое от 0 до {MAX_MINOR_UNITS}"))
    }
}

impl Currency {
    pub fn create(project_id: Uuid, new: NewCurrency) -> Result<Self, String> {
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            code: new_code("code", &new.code)?,
            name: require_text("name", &new.name)?,
            archived: false,
            name_lang: new.name_lang,
            name_ru: kernel::i18n::clean(new.name_ru.clone()),
            name_en: kernel::i18n::clean(new.name_en.clone()),
            name_th: kernel::i18n::clean(new.name_th.clone()),
            symbol: require_text("symbol", &new.symbol)?,
            minor_units: minor_units(new.minor_units)?,
        })
    }

    pub fn change(&mut self, changes: CurrencyChanges) -> Result<(), String> {
        self.name = require_text("name", &changes.name)?;
        self.archived = changes.archived;
        self.name_lang = changes.name_lang;
        self.name_ru = kernel::i18n::clean(changes.name_ru.clone());
        self.name_en = kernel::i18n::clean(changes.name_en.clone());
        self.name_th = kernel::i18n::clean(changes.name_th.clone());
        self.symbol = require_text("symbol", &changes.symbol)?;
        self.minor_units = minor_units(changes.minor_units)?;
        Ok(())
    }
}
