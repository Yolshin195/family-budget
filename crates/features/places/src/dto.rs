//! Контракт HTTP API фичи «Места» и преобразования DTO ⇄ домен.

use kernel::error::Problem;
use kernel::i18n::tr;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::domain::{Place, PlaceData, PlaceKind};

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CreatePlaceDto {
    /// Уникальный код: латиница, цифры и `_`, до 40 знаков
    #[schema(example = "SEVEN_ELEVEN")]
    pub code: String,
    #[serde(flatten)]
    pub data: PlaceFields,
}

/// Поля места, кроме кода. В `PUT` передаются все: пропущенное поле очищается.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PlaceFields {
    /// Название, как на вывеске; не переводится
    #[schema(example = "7-Eleven")]
    pub name: String,
    /// Варианты написания для поиска, в том числе на тайском
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Категория позиции по умолчанию
    #[serde(default)]
    pub default_category_code: Option<String>,
    /// Счёт по умолчанию
    #[serde(default)]
    pub default_account_code: Option<String>,
    /// SHOP, CAFE, MARKET, ONLINE, SERVICE или OTHER (по умолчанию)
    #[serde(default)]
    #[schema(example = "SHOP")]
    pub place_kind: String,
    /// Филиал или адрес
    #[serde(default)]
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlaceDto {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub default_category_code: Option<String>,
    pub default_account_code: Option<String>,
    #[schema(example = "SHOP")]
    pub place_kind: String,
    pub location: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ListQuery {
    /// Показать и архивные записи
    #[serde(default)]
    pub include_archived: bool,
}

impl From<Place> for PlaceDto {
    fn from(x: Place) -> Self {
        Self {
            id: x.id,
            code: x.code,
            name: x.name,
            aliases: x.aliases,
            default_category_code: x.default_category_code,
            default_account_code: x.default_account_code,
            place_kind: x.kind.as_str().to_string(),
            location: x.location,
            archived: x.archived,
        }
    }
}

impl PlaceFields {
    pub fn into_domain(self) -> Result<PlaceData, Problem> {
        let kind = PlaceKind::parse(&self.place_kind).ok_or_else(|| {
            Problem::validation("place_kind", tr(Some(crate::i18n::dict), "err.kind", &[("values", &PlaceKind::VALUES.join(", "))]))
        })?;
        Ok(PlaceData {
            name: self.name,
            aliases: self.aliases,
            default_category_code: self.default_category_code,
            default_account_code: self.default_account_code,
            kind,
            location: self.location,
        })
    }
}

impl PlaceDto {
    /// «7-Eleven · Сукхумвит 24» — чтобы отличить филиалы.
    pub fn title(&self) -> String {
        match &self.location {
            Some(l) => format!("{} · {l}", self.name),
            None => self.name.clone(),
        }
    }

    /// Название, варианты написания и код через `kernel::search::SEP` — для поиска.
    pub fn search(&self) -> String {
        kernel::search::join(std::iter::once(self.name.as_str()).chain(self.aliases.iter().map(String::as_str)).chain([self.code.as_str()]))
    }
}
