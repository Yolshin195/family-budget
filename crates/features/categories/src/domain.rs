//! Доменная модель «Категории». Никаких sqlx/axum/serde — только правила предметной области.

use kernel::code::{new_code, normalize, require_text};
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

/// Запись справочника. Уникальна в пределах проекта по `code`.
#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    pub id: Uuid,
    pub project_id: Uuid,
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
    pub fn create(project_id: Uuid, new: NewCategory) -> Result<Self, String> {
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
            category_kind: new.category_kind,
            parent_code: parent(&new.parent_code)?,
            sort_order: new.sort_order,
        })
    }

    pub fn change(&mut self, changes: CategoryChanges) -> Result<(), String> {
        self.name = require_text("name", &changes.name)?;
        self.archived = changes.archived;
        self.name_lang = changes.name_lang;
        self.name_ru = kernel::i18n::clean(changes.name_ru.clone());
        self.name_en = kernel::i18n::clean(changes.name_en.clone());
        self.name_th = kernel::i18n::clean(changes.name_th.clone());
        self.category_kind = changes.category_kind;
        self.parent_code = parent(&changes.parent_code)?;
        self.sort_order = changes.sort_order;
        Ok(())
    }
}

/// Код родителя: пусто → корень, иначе нормализованный код.
fn parent(value: &Option<String>) -> Result<Option<String>, String> {
    match value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => normalize("parent_code", v).map(Some),
    }
}

/// Максимальная глубина дерева категорий (корень — уровень 1).
pub const MAX_DEPTH: usize = 5;

/// Проверка места узла `item` в дереве `all` (остальные записи проекта; сам `item` может там быть
/// в старом виде): родитель существует и активен, того же типа, нет цикла, глубина ≤ [`MAX_DEPTH`],
/// у потомков тот же тип.
pub fn check_tree(all: &[Category], item: &Category) -> Result<(), String> {
    let others: Vec<&Category> = all.iter().filter(|c| c.code != item.code).collect();
    let find = |code: &str| others.iter().copied().find(|c| c.code == code);
    let mut depth = 1;
    if let Some(parent_code) = &item.parent_code {
        if *parent_code == item.code {
            return Err("parent_code: категория не может быть родителем самой себя".into());
        }
        let parent = find(parent_code).ok_or_else(|| format!("parent_code: категория '{parent_code}' не найдена"))?;
        if parent.archived && !item.archived {
            return Err(format!("parent_code: категория '{parent_code}' в архиве"));
        }
        if parent.category_kind != item.category_kind {
            return Err("parent_code: тип категории должен совпадать с типом родителя".into());
        }
        // Подъём к корню: цикл = встретили сами себя.
        let mut cur = Some(parent);
        while let Some(c) = cur {
            depth += 1;
            if depth > MAX_DEPTH + 1 {
                break;
            }
            cur = match &c.parent_code {
                Some(p) if *p == item.code => {
                    return Err("parent_code: нельзя перенести категорию в собственного потомка".into());
                }
                Some(p) => find(p),
                None => None,
            };
        }
    }
    // Потомки item: тот же тип, итоговая глубина поддерева в пределах.
    fn height(others: &[&Category], code: &str, kind: CategoryCategoryKind, guard: usize) -> Result<usize, String> {
        if guard > MAX_DEPTH + 1 {
            return Ok(guard);
        }
        let mut h = 0;
        for child in others.iter().filter(|c| c.parent_code.as_deref() == Some(code)) {
            if child.category_kind != kind {
                return Err(format!(
                    "category_kind: у подкатегории '{}' другой тип — сначала перенесите её",
                    child.code
                ));
            }
            h = h.max(1 + height(others, &child.code, kind, guard + 1)?);
        }
        Ok(h)
    }
    let below = height(&others, &item.code, item.category_kind, 0)?;
    if depth + below > MAX_DEPTH {
        return Err(format!("parent_code: глубина дерева больше {MAX_DEPTH} уровней"));
    }
    Ok(())
}

/// Обход дерева в глубину: (уровень, узел), соседи по `sort_order`, затем по коду.
/// Узлы с несуществующим родителем показываются как корни.
pub fn flatten(all: &[Category]) -> Vec<(usize, &Category)> {
    let has = |code: &str| all.iter().any(|c| c.code == code);
    let mut roots: Vec<&Category> = all.iter().filter(|c| c.parent_code.as_deref().is_none_or(|p| !has(p))).collect();
    roots.sort_by(|a, b| {
        (a.category_kind.as_str(), a.sort_order, &a.code).cmp(&(b.category_kind.as_str(), b.sort_order, &b.code))
    });
    let mut out = Vec::with_capacity(all.len());
    fn walk<'a>(all: &'a [Category], node: &'a Category, depth: usize, out: &mut Vec<(usize, &'a Category)>) {
        out.push((depth, node));
        if depth > MAX_DEPTH {
            return;
        }
        let mut kids: Vec<&Category> =
            all.iter().filter(|c| c.parent_code.as_deref() == Some(node.code.as_str())).collect();
        kids.sort_by(|a, b| (a.sort_order, &a.code).cmp(&(b.sort_order, &b.code)));
        for k in kids {
            walk(all, k, depth + 1, out);
        }
    }
    for r in roots {
        walk(all, r, 0, &mut out);
    }
    out
}
