//! Контракт HTTP API фичи «Места» и преобразования DTO ⇄ домен.

use kernel::error::Problem;
use kernel::i18n::tr;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::domain::{Place, PlaceData, PlaceKind, ancestors, flatten};

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
    /// Код сети или торгового центра, в который входит место; пусто — место верхнего уровня
    #[serde(default)]
    pub parent_code: Option<String>,
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
    pub parent_code: Option<String>,
    pub aliases: Vec<String>,
    /// Своя категория по умолчанию (для правки)
    pub default_category_code: Option<String>,
    /// Свой счёт по умолчанию (для правки)
    pub default_account_code: Option<String>,
    #[schema(example = "SHOP")]
    pub place_kind: String,
    pub location: Option<String>,
    pub archived: bool,
    /// Уровень в дереве, 0 — верхний
    pub depth: usize,
    /// Название с сетью и адресом: «7-Eleven › Asok · Сукхумвит 24»
    #[schema(example = "7-Eleven · Sukhumvit 24")]
    pub title: String,
    /// Категория, которую место подставит: своя или ближайшего родителя
    pub effective_category_code: Option<String>,
    /// Счёт, который место подставит: свой или ближайшего родителя
    pub effective_account_code: Option<String>,
    /// Названия, варианты написания (свои и родителей), адрес и код через `kernel::search::SEP` — для поиска
    #[serde(skip)]
    pub search: String,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ListQuery {
    /// Показать и архивные записи
    #[serde(default)]
    pub include_archived: bool,
}

/// Места проекта деревом (родитель, затем его места) — с полным названием и тем, что место унаследует.
pub fn views(all: &[Place]) -> Vec<PlaceDto> {
    flatten(all).into_iter().map(|(depth, p)| view(all, p, depth)).collect()
}

fn view(all: &[Place], x: &Place, depth: usize) -> PlaceDto {
    let up = ancestors(all, x);
    // Ближайший родитель с заданным значением.
    let inherit = |own: &Option<String>, get: fn(&Place) -> &Option<String>| own.clone().or_else(|| up.iter().rev().find_map(|p| get(p).clone()));
    // Путь от корня; название, повторяющее родителя («7-Eleven» внутри «7-Eleven»), не дублируется.
    let mut path: Vec<&str> = Vec::new();
    for name in up.iter().map(|p| p.name.as_str()).chain([x.name.as_str()]) {
        if path.last().is_none_or(|last| kernel::search::fold(last) != kernel::search::fold(name)) {
            path.push(name);
        }
    }
    let path = path.join(" › ");
    let title = match &x.location {
        Some(l) => format!("{path} · {l}"),
        None => path,
    };
    let names = std::iter::once(x).chain(up.iter().rev().copied()).flat_map(|p| std::iter::once(p.name.as_str()).chain(p.aliases.iter().map(String::as_str)));
    let search = kernel::search::join(names.chain(x.location.as_deref()).chain([x.code.as_str()]));
    PlaceDto {
        id: x.id,
        code: x.code.clone(),
        name: x.name.clone(),
        parent_code: x.parent_code.clone(),
        aliases: x.aliases.clone(),
        default_category_code: x.default_category_code.clone(),
        default_account_code: x.default_account_code.clone(),
        place_kind: x.kind.as_str().to_string(),
        location: x.location.clone(),
        archived: x.archived,
        depth,
        title,
        effective_category_code: inherit(&x.default_category_code, |p| &p.default_category_code),
        effective_account_code: inherit(&x.default_account_code, |p| &p.default_account_code),
        search,
    }
}

impl PlaceFields {
    pub fn into_domain(self) -> Result<PlaceData, Problem> {
        let kind = PlaceKind::parse(&self.place_kind).ok_or_else(|| {
            Problem::validation("place_kind", tr(Some(crate::i18n::dict), "err.kind", &[("values", &PlaceKind::VALUES.join(", "))]))
        })?;
        Ok(PlaceData {
            name: self.name,
            parent_code: self.parent_code,
            aliases: self.aliases,
            default_category_code: self.default_category_code,
            default_account_code: self.default_account_code,
            kind,
            location: self.location,
        })
    }
}

impl PlaceDto {
    /// Отступ для показа дерева в `<select>`.
    pub fn indent(&self) -> String {
        "\u{00A0}\u{00A0}\u{00A0}".repeat(self.depth)
    }
}
