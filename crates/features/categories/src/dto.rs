//! Контракт HTTP API фичи «Категории» и преобразования DTO ⇄ домен.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[allow(unused_imports)]
use crate::domain::{Category, CategoryCategoryKind, CategoryChanges, CategoryNameLang, NewCategory};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCategoryDto {
    /// Уникальный код: латиница, цифры и `_`, до 40 знаков
    #[schema(example = "FOOD")]
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
    /// Расход или доход
    #[schema(example = "EXPENSE")]
    pub category_kind: String,
    /// Код родителя; пусто у корня
    #[serde(default)]
    pub parent_code: Option<String>,
    /// Порядок среди соседей
    pub sort_order: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCategoryDto {
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
    /// Расход или доход
    #[schema(example = "EXPENSE")]
    pub category_kind: String,
    /// Код родителя; пусто у корня
    #[serde(default)]
    pub parent_code: Option<String>,
    /// Порядок среди соседей
    pub sort_order: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CategoryDto {
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
    /// Расход или доход
    #[schema(example = "EXPENSE")]
    pub category_kind: String,
    /// Код родителя; пусто у корня
    pub parent_code: Option<String>,
    /// Порядок среди соседей
    pub sort_order: i32,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ListQuery {
    /// Показать и архивные записи
    #[serde(default)]
    pub include_archived: bool,
}

impl From<Category> for CategoryDto {
    fn from(x: Category) -> Self {
        Self {
            id: x.id,
            code: x.code,
            name: x.name,
            archived: x.archived,
            name_lang: x.name_lang.as_str().to_string(),
            name_ru: x.name_ru,
            name_en: x.name_en,
            name_th: x.name_th,
            category_kind: x.category_kind.as_str().to_string(),
            parent_code: x.parent_code,
            sort_order: x.sort_order,
        }
    }
}

impl CreateCategoryDto {
    pub fn into_domain(self) -> Result<NewCategory, String> {
        Ok(NewCategory {
            code: self.code,
            name: self.name,
            name_lang: parse_name_lang(&self.name_lang)?,
            name_ru: self.name_ru,
            name_en: self.name_en,
            name_th: self.name_th,
            category_kind: parse_category_kind(&self.category_kind)?,
            parent_code: self.parent_code,
            sort_order: self.sort_order,
        })
    }
}

impl UpdateCategoryDto {
    pub fn into_domain(self) -> Result<CategoryChanges, String> {
        Ok(CategoryChanges {
            name: self.name,
            archived: self.archived,
            name_lang: parse_name_lang(&self.name_lang)?,
            name_ru: self.name_ru,
            name_en: self.name_en,
            name_th: self.name_th,
            category_kind: parse_category_kind(&self.category_kind)?,
            parent_code: self.parent_code,
            sort_order: self.sort_order,
        })
    }
}

fn parse_name_lang(value: &str) -> Result<CategoryNameLang, String> {
    CategoryNameLang::parse(value).ok_or_else(|| {
        format!("name_lang: недопустимое значение '{value}', ожидается одно из {:?}", CategoryNameLang::VALUES)
    })
}

fn parse_category_kind(value: &str) -> Result<CategoryCategoryKind, String> {
    CategoryCategoryKind::parse(value).ok_or_else(|| {
        format!("category_kind: недопустимое значение '{value}', ожидается одно из {:?}", CategoryCategoryKind::VALUES)
    })
}

impl CategoryDto {
    fn localized(&self) -> kernel::i18n::Localized<'_> {
        kernel::i18n::Localized {
            original: &self.name,
            lang: kernel::i18n::Lang::parse(&self.name_lang).unwrap_or_default(),
            ru: self.name_ru.as_deref(),
            en: self.name_en.as_deref(),
            th: self.name_th.as_deref(),
        }
    }

    /// `name` на языке интерфейса.
    pub fn title(&self, lang: impl Into<kernel::i18n::Lang>) -> &str {
        self.localized().show(lang.into())
    }

    /// Нет перевода `name` на язык интерфейса.
    pub fn missing_tr(&self, lang: impl Into<kernel::i18n::Lang>) -> bool {
        self.localized().missing(lang.into())
    }
}
