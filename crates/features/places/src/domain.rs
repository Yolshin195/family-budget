//! Доменная модель «Места». Никаких sqlx/axum/serde — только правила предметной области.
//! Ошибки — [`Problem`] на языке запроса (техдолг #9): тексты в `i18n.rs`.

use kernel::error::Problem;
use kernel::i18n::tr;
use uuid::Uuid;

/// Самое длинное название, вариант написания или адрес.
pub const MAX_TEXT: usize = 200;
/// Сколько вариантов написания у одного места.
pub const MAX_ALIASES: usize = 20;

/// Тип места.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaceKind {
    Shop,
    Cafe,
    Market,
    Online,
    Service,
    #[default]
    Other,
}

impl PlaceKind {
    pub const VALUES: &'static [&'static str] = &["SHOP", "CAFE", "MARKET", "ONLINE", "SERVICE", "OTHER"];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Shop => "SHOP",
            Self::Cafe => "CAFE",
            Self::Market => "MARKET",
            Self::Online => "ONLINE",
            Self::Service => "SERVICE",
            Self::Other => "OTHER",
        }
    }

    /// Пусто — «другое».
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "SHOP" => Some(Self::Shop),
            "CAFE" => Some(Self::Cafe),
            "MARKET" => Some(Self::Market),
            "ONLINE" => Some(Self::Online),
            "SERVICE" => Some(Self::Service),
            "OTHER" | "" => Some(Self::Other),
            _ => None,
        }
    }
}

/// Место: магазин, кафе, рынок. Уникально в проекте по `code`; название — имя собственное, не переводится.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub id: Uuid,
    pub project_id: Uuid,
    pub code: String,
    pub name: String,
    /// Варианты написания для поиска: «7-11», «เซเว่น».
    pub aliases: Vec<String>,
    /// Категория позиции, если в операции её не указали.
    pub default_category_code: Option<String>,
    /// Счёт операции, если его не указали (рынок — наличные).
    pub default_account_code: Option<String>,
    pub kind: PlaceKind,
    /// Филиал или адрес.
    pub location: Option<String>,
    pub archived: bool,
}

/// Поля места, которые задаёт пользователь (код — только при создании).
#[derive(Debug, Clone, Default)]
pub struct PlaceData {
    pub name: String,
    pub aliases: Vec<String>,
    pub default_category_code: Option<String>,
    pub default_account_code: Option<String>,
    pub kind: PlaceKind,
    pub location: Option<String>,
}

impl Place {
    pub fn create(project_id: Uuid, code: &str, data: PlaceData) -> Result<Self, Problem> {
        let mut place = Self {
            id: Uuid::new_v4(),
            project_id,
            code: kernel::code::fresh_code("code", code)?,
            name: String::new(),
            aliases: Vec::new(),
            default_category_code: None,
            default_account_code: None,
            kind: PlaceKind::Other,
            location: None,
            archived: false,
        };
        place.change(data)?;
        Ok(place)
    }

    /// Новые значения полей (код и архив не меняются).
    pub fn change(&mut self, data: PlaceData) -> Result<(), Problem> {
        let name = text("name", &data.name)?.ok_or_else(|| Problem::validation("name", tr(None, "err.empty", &[])))?;
        self.aliases = aliases(&name, &data.aliases)?;
        self.name = name;
        self.default_category_code = reference("default_category_code", data.default_category_code.as_deref())?;
        self.default_account_code = reference("default_account_code", data.default_account_code.as_deref())?;
        self.kind = data.kind;
        self.location = text("location", data.location.as_deref().unwrap_or_default())?;
        Ok(())
    }
}

/// Необязательный текст: пусто → `None`, не длиннее [`MAX_TEXT`].
fn text(field: &str, value: &str) -> Result<Option<String>, Problem> {
    let value = value.trim();
    if value.chars().count() > MAX_TEXT {
        return Err(Problem::validation(field, tr(Some(crate::i18n::dict), "err.too_long", &[("max", &MAX_TEXT)])));
    }
    Ok(Some(value.to_string()).filter(|v| !v.is_empty()))
}

/// Варианты написания: без пустых, повторов (без учёта регистра и диакритики) и самого названия.
fn aliases(name: &str, list: &[String]) -> Result<Vec<String>, Problem> {
    let mut seen = vec![kernel::search::fold(name)];
    let mut out = Vec::new();
    for a in list {
        let Some(a) = text("aliases", a)? else { continue };
        let key = kernel::search::fold(&a);
        if !seen.contains(&key) {
            seen.push(key);
            out.push(a);
        }
    }
    if out.len() > MAX_ALIASES {
        return Err(Problem::validation("aliases", tr(Some(crate::i18n::dict), "err.too_many_aliases", &[("max", &MAX_ALIASES)])));
    }
    Ok(out)
}

/// Ссылка на категорию или счёт: пусто → `None`, иначе код (существование проверяет сервис).
fn reference(field: &str, value: Option<&str>) -> Result<Option<String>, Problem> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => kernel::code::code(field, v).map(Some),
    }
}

/// Варианты написания из одной строки формы: через запятую или с новой строки.
pub fn split_aliases(s: &str) -> Vec<String> {
    s.split([',', '\n']).map(str::trim).filter(|a| !a.is_empty()).map(String::from).collect()
}
