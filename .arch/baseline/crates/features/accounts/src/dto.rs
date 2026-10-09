//! Контракт HTTP API фичи «Счета» и преобразования DTO ⇄ домен.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[allow(unused_imports)]
use crate::domain::{AccountNameLang, AccountAccountKind, NewAccount, Account, AccountChanges};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAccountDto {
    /// Уникальный код (верхний регистр, без пробелов)
    #[schema(example = "CARD_KBANK")]
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
    /// Тип счёта
    #[schema(example = "CASH")]
    pub account_kind: String,
    /// Валюта счёта
    pub currency_code: String,
    /// Начальный остаток
    pub opening_balance: rust_decimal::Decimal,
    /// Держатель счёта (участник проекта)
    #[serde(default)]
    pub holder_user_id: Option<uuid::Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccountDto {
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
    /// Тип счёта
    #[schema(example = "CASH")]
    pub account_kind: String,
    /// Валюта счёта
    pub currency_code: String,
    /// Начальный остаток
    pub opening_balance: rust_decimal::Decimal,
    /// Держатель счёта (участник проекта)
    #[serde(default)]
    pub holder_user_id: Option<uuid::Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AccountDto {
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
    /// Тип счёта
    #[schema(example = "CASH")]
    pub account_kind: String,
    /// Валюта счёта
    pub currency_code: String,
    /// Начальный остаток
    pub opening_balance: rust_decimal::Decimal,
    /// Держатель счёта (участник проекта)
    pub holder_user_id: Option<uuid::Uuid>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ListQuery {
    /// Показать и архивные записи
    #[serde(default)]
    pub include_archived: bool,
}

impl From<Account> for AccountDto {
    fn from(x: Account) -> Self {
        Self {
            id: x.id,
            code: x.code,
            name: x.name,
            archived: x.archived,
            name_lang: x.name_lang.as_str().to_string(),
            name_ru: x.name_ru,
            name_en: x.name_en,
            name_th: x.name_th,
            account_kind: x.account_kind.as_str().to_string(),
            currency_code: x.currency_code,
            opening_balance: x.opening_balance,
            holder_user_id: x.holder_user_id,
        }
    }
}

impl CreateAccountDto {
    pub fn into_domain(self) -> Result<NewAccount, String> {
        Ok(NewAccount {
            code: self.code,
            name: self.name,
            name_lang: parse_name_lang(&self.name_lang)?,
            name_ru: self.name_ru,
            name_en: self.name_en,
            name_th: self.name_th,
            account_kind: parse_account_kind(&self.account_kind)?,
            currency_code: self.currency_code,
            opening_balance: self.opening_balance,
            holder_user_id: self.holder_user_id,
        })
    }
}

impl UpdateAccountDto {
    pub fn into_domain(self) -> Result<AccountChanges, String> {
        Ok(AccountChanges {
            name: self.name,
            archived: self.archived,
            name_lang: parse_name_lang(&self.name_lang)?,
            name_ru: self.name_ru,
            name_en: self.name_en,
            name_th: self.name_th,
            account_kind: parse_account_kind(&self.account_kind)?,
            currency_code: self.currency_code,
            opening_balance: self.opening_balance,
            holder_user_id: self.holder_user_id,
        })
    }
}

fn parse_name_lang(value: &str) -> Result<AccountNameLang, String> {
    AccountNameLang::parse(value)
        .ok_or_else(|| format!("name_lang: недопустимое значение '{value}', ожидается одно из {:?}", AccountNameLang::VALUES))
}

fn parse_account_kind(value: &str) -> Result<AccountAccountKind, String> {
    AccountAccountKind::parse(value)
        .ok_or_else(|| format!("account_kind: недопустимое значение '{value}', ожидается одно из {:?}", AccountAccountKind::VALUES))
}
