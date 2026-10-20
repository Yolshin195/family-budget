//! Доменная модель «Места». Никаких sqlx/axum/serde — только правила предметной области.
//! Ошибки — [`Problem`] на языке запроса (техдолг #9): тексты в `i18n.rs`.

use kernel::error::Problem;
use kernel::i18n::tr;
use uuid::Uuid;

/// Самое длинное название, вариант написания или адрес.
pub const MAX_TEXT: usize = 200;
/// Сколько вариантов написания у одного места.
pub const MAX_ALIASES: usize = 20;
/// Глубина дерева мест (корень — уровень 1): сеть → торговый центр → магазин.
pub const MAX_DEPTH: usize = 3;

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
/// Места образуют дерево: сеть «7-Eleven» → конкретный магазин; магазин берёт у сети то, что не задано у него.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub id: Uuid,
    pub project_id: Uuid,
    pub code: String,
    pub name: String,
    /// Сеть или торговый центр, в который входит место.
    pub parent_code: Option<String>,
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
    pub parent_code: Option<String>,
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
            parent_code: None,
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
        self.parent_code = reference("parent_code", data.parent_code.as_deref())?;
        self.default_category_code = reference("default_category_code", data.default_category_code.as_deref())?;
        self.default_account_code = reference("default_account_code", data.default_account_code.as_deref())?;
        self.kind = data.kind;
        self.location = text("location", data.location.as_deref().unwrap_or_default())?;
        Ok(())
    }
}

/// Проверка места узла `item` в дереве `all` (все места проекта; сам `item` может там быть в старом виде):
/// родитель существует и активен, нет цикла, глубина вместе с поддеревом — не больше [`MAX_DEPTH`].
pub fn check_tree(all: &[Place], item: &Place) -> Result<(), Problem> {
    let others: Vec<&Place> = all.iter().filter(|p| p.code != item.code).collect();
    let find = |code: &str| others.iter().copied().find(|p| p.code == code);
    let problem = |key: &str, args: &[(&str, &dyn std::fmt::Display)]| Problem::validation("parent_code", tr(Some(crate::i18n::dict), key, args));
    let mut depth = 1;
    if let Some(parent_code) = &item.parent_code {
        if *parent_code == item.code {
            return Err(problem("err.parent_self", &[]));
        }
        let parent = find(parent_code).ok_or_else(|| Problem::validation("parent_code", tr(None, "err.ref_not_found", &[("code", parent_code)])))?;
        if parent.archived && !item.archived {
            return Err(problem("err.parent_archived", &[("name", &parent.name)]));
        }
        // Подъём к корню: цикл — встретили сами себя.
        let mut cur = Some(parent);
        while let Some(p) = cur {
            depth += 1;
            if depth > MAX_DEPTH + 1 {
                break;
            }
            cur = match &p.parent_code {
                Some(c) if *c == item.code => return Err(problem("err.parent_cycle", &[])),
                Some(c) => find(c),
                None => None,
            };
        }
    }
    fn height(others: &[&Place], code: &str, guard: usize) -> usize {
        if guard > MAX_DEPTH {
            return guard;
        }
        others.iter().filter(|p| p.parent_code.as_deref() == Some(code)).map(|p| 1 + height(others, &p.code, guard + 1)).max().unwrap_or(0)
    }
    if depth + height(&others, &item.code, 0) > MAX_DEPTH {
        return Err(problem("err.depth", &[("max", &MAX_DEPTH)]));
    }
    Ok(())
}

/// Обход дерева в глубину: (уровень, место), соседи — по названию без учёта регистра, затем по коду.
/// Место с несуществующим родителем показывается как корень.
pub fn flatten(all: &[Place]) -> Vec<(usize, &Place)> {
    fn order(list: &mut [&Place]) {
        list.sort_by_cached_key(|p| (p.name.to_lowercase(), p.code.clone()));
    }
    fn walk<'a>(all: &'a [Place], node: &'a Place, depth: usize, out: &mut Vec<(usize, &'a Place)>) {
        out.push((depth, node));
        if depth >= MAX_DEPTH {
            return;
        }
        let mut kids: Vec<&Place> = all.iter().filter(|p| p.parent_code.as_deref() == Some(node.code.as_str())).collect();
        order(&mut kids);
        for k in kids {
            walk(all, k, depth + 1, out);
        }
    }
    let has = |code: &str| all.iter().any(|p| p.code == code);
    let mut roots: Vec<&Place> = all.iter().filter(|p| p.parent_code.as_deref().is_none_or(|c| !has(c))).collect();
    order(&mut roots);
    let mut out = Vec::with_capacity(all.len());
    for r in roots {
        walk(all, r, 0, &mut out);
    }
    out
}

/// Цепочка родителей места от корня (без самого места); обрывается на цикле или отсутствующем родителе.
pub fn ancestors<'a>(all: &'a [Place], item: &Place) -> Vec<&'a Place> {
    let mut out: Vec<&Place> = Vec::new();
    let mut cur = item.parent_code.as_deref();
    while let Some(code) = cur {
        match all.iter().find(|p| p.code == code) {
            Some(p) if p.code != item.code && !out.iter().any(|x| x.code == p.code) && out.len() < MAX_DEPTH => {
                out.push(p);
                cur = p.parent_code.as_deref();
            }
            _ => break,
        }
    }
    out.reverse();
    out
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
