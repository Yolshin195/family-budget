//! Доменная модель «Счета». Никаких sqlx/axum/serde — только правила предметной области.

use chrono::{DateTime, Utc};
use kernel::code::{new_code, normalize, require_text};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Язык оригинала названия
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountNameLang {
    Ru,
    En,
    Th,
}

impl AccountNameLang {
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

/// Тип счёта
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountAccountKind {
    Cash,
    Card,
    Bank,
    Savings,
}

impl AccountAccountKind {
    pub const VALUES: &'static [&'static str] = &["CASH", "CARD", "BANK", "SAVINGS"];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cash => "CASH",
            Self::Card => "CARD",
            Self::Bank => "BANK",
            Self::Savings => "SAVINGS",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "CASH" => Some(Self::Cash),
            "CARD" => Some(Self::Card),
            "BANK" => Some(Self::Bank),
            "SAVINGS" => Some(Self::Savings),
            _ => None,
        }
    }
}

/// Остаток счёта по данным банка (из SMS банка): сумма и момент, на который она верна. С ним сверяется остаток по
/// операциям на тот же момент; правка счёта его не меняет.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BankBalance {
    pub amount: Decimal,
    pub at: DateTime<Utc>,
}

/// Запись справочника. Уникальна в пределах проекта по `code`.
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub id: Uuid,
    pub project_id: Uuid,
    pub code: String,
    pub name: String,
    pub archived: bool,
    pub name_lang: AccountNameLang,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub account_kind: AccountAccountKind,
    pub currency_code: String,
    pub holder_user_id: Option<uuid::Uuid>,
    /// Последний остаток по данным банка; пишет только сервис по SMS банка.
    pub bank: Option<BankBalance>,
}

/// Данные для создания записи.
#[derive(Debug, Clone)]
pub struct NewAccount {
    pub code: String,
    pub name: String,
    pub name_lang: AccountNameLang,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub account_kind: AccountAccountKind,
    pub currency_code: String,
    pub holder_user_id: Option<uuid::Uuid>,
}

/// Данные для изменения записи (код — ключ, не меняется).
#[derive(Debug, Clone)]
pub struct AccountChanges {
    pub name: String,
    pub archived: bool,
    pub name_lang: AccountNameLang,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub account_kind: AccountAccountKind,
    pub currency_code: String,
    pub holder_user_id: Option<uuid::Uuid>,
}

impl Account {
    pub fn create(project_id: Uuid, new: NewAccount) -> Result<Self, String> {
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
            account_kind: new.account_kind,
            currency_code: normalize("currency_code", &new.currency_code)?,
            holder_user_id: new.holder_user_id,
            bank: None,
        })
    }

    pub fn change(&mut self, changes: AccountChanges) -> Result<(), String> {
        self.name = require_text("name", &changes.name)?;
        self.archived = changes.archived;
        self.name_lang = changes.name_lang;
        self.name_ru = kernel::i18n::clean(changes.name_ru.clone());
        self.name_en = kernel::i18n::clean(changes.name_en.clone());
        self.name_th = kernel::i18n::clean(changes.name_th.clone());
        self.account_kind = changes.account_kind;
        self.currency_code = normalize("currency_code", &changes.currency_code)?;
        self.holder_user_id = changes.holder_user_id;
        Ok(())
    }
}
