//! Доменная модель «Категории». Никаких sqlx/axum/serde — только правила предметной области.

use kernel::code::{normalize, require_text};
use uuid::Uuid;

/// Язык оригинала названия
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryNameLang {
    Ru,
    En,
    Th,
}

impl CategoryNameLang {
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

/// Расход или доход
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryCategoryKind {
    Expense,
    Income,
}

impl CategoryCategoryKind {
    pub const VALUES: &'static [&'static str] = &["EXPENSE", "INCOME"];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Expense => "EXPENSE",
            Self::Income => "INCOME",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "EXPENSE" => Some(Self::Expense),
            "INCOME" => Some(Self::Income),
            _ => None,
        }
    }
}

/// Запись справочника. Уникальна в пределах пользователя по `code`.
#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    pub id: Uuid,
    pub user_id: Uuid,
    pub code: String,
    pub name: String,
    pub archived: bool,
    pub name_lang: CategoryNameLang,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub category_kind: CategoryCategoryKind,
    pub parent_code: Option<String>,
    pub sort_order: i32,
}

/// Данные для создания записи.
#[derive(Debug, Clone)]
pub struct NewCategory {
    pub code: String,
    pub name: String,
    pub name_lang: CategoryNameLang,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub category_kind: CategoryCategoryKind,
    pub parent_code: Option<String>,
    pub sort_order: i32,
}

/// Данные для изменения записи (код — ключ, не меняется).
#[derive(Debug, Clone)]
pub struct CategoryChanges {
    pub name: String,
    pub archived: bool,
    pub name_lang: CategoryNameLang,
    pub name_ru: Option<String>,
    pub name_en: Option<String>,
    pub name_th: Option<String>,
    pub category_kind: CategoryCategoryKind,
    pub parent_code: Option<String>,
    pub sort_order: i32,
}

impl Category {
    pub fn create(user_id: Uuid, new: NewCategory) -> Result<Self, String> {
        Ok(Self {
            id: Uuid::new_v4(),
            user_id,
            code: normalize("code", &new.code)?,
            name: require_text("name", &new.name)?,
            archived: false,
            name_lang: new.name_lang,
            name_ru: new.name_ru.clone(),
            name_en: new.name_en.clone(),
            name_th: new.name_th.clone(),
            category_kind: new.category_kind,
            parent_code: new.parent_code.clone(),
            sort_order: new.sort_order,
        })
    }

    pub fn change(&mut self, changes: CategoryChanges) -> Result<(), String> {
        self.name = require_text("name", &changes.name)?;
        self.archived = changes.archived;
        self.name_lang = changes.name_lang;
        self.name_ru = changes.name_ru.clone();
        self.name_en = changes.name_en.clone();
        self.name_th = changes.name_th.clone();
        self.category_kind = changes.category_kind;
        self.parent_code = changes.parent_code.clone();
        self.sort_order = changes.sort_order;
        Ok(())
    }
}
