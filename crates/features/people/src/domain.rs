//! Доменная модель «Люди». Никаких sqlx/axum/serde — только правила предметной области.
//! Ошибки — [`Problem`] на языке запроса (техдолг #9): тексты в `i18n.rs`.

use kernel::error::Problem;
use kernel::i18n::tr;
use uuid::Uuid;

/// Самое длинное имя или другое имя.
pub const MAX_TEXT: usize = 200;
/// Сколько других имён у одного человека.
pub const MAX_ALIASES: usize = 20;
/// Код записи участника, если в имени нет латиницы.
pub const MEMBER_CODE: &str = "MEMBER";

/// Человек, для которого покупка: участник проекта или человек без аккаунта (ребёнок, няня).
/// Уникален в проекте по `code`, у участника — ещё и по `user_id`. Имя не переводится.
#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub id: Uuid,
    pub project_id: Uuid,
    pub code: String,
    /// Отображаемое имя; пусто — только у удалённого участника (`former_member`).
    pub name: String,
    /// Другие имена для поиска: «Пинпан», «ปิ่นปั่น».
    pub aliases: Vec<String>,
    /// Участник проекта; `None` — человек без аккаунта.
    pub user_id: Option<Uuid>,
    /// Участник удалил аккаунт: имя стёрто, вместо него — «Удалённый участник» на языке зрителя.
    pub former_member: bool,
    pub archived: bool,
}

/// Поля, которые задаёт пользователь (код — только при создании; участник и архив — отдельно).
#[derive(Debug, Clone, Default)]
pub struct PersonData {
    pub name: String,
    pub aliases: Vec<String>,
}

impl Person {
    /// Человек без аккаунта — только явно (форма, REST, `add_person`).
    pub fn create(project_id: Uuid, code: &str, data: PersonData) -> Result<Self, Problem> {
        let mut person = Self::blank(project_id, kernel::code::fresh_code("code", code)?);
        person.change(data)?;
        Ok(person)
    }

    /// Запись участника проекта: имя из профиля, код — [`member_code`].
    pub fn member(project_id: Uuid, code: String, user_id: Uuid, name: &str) -> Self {
        let mut person = Self::blank(project_id, code);
        person.name = name.trim().chars().take(MAX_TEXT).collect();
        person.user_id = Some(user_id);
        person
    }

    fn blank(project_id: Uuid, code: String) -> Self {
        Self { id: Uuid::new_v4(), project_id, code, name: String::new(), aliases: Vec::new(), user_id: None, former_member: false, archived: false }
    }

    /// Новые имя и другие имена. Имя обязательно; у удалённого участника пустое имя оставляет «Удалённый
    /// участник», а введённое делает запись обычным человеком без аккаунта.
    pub fn change(&mut self, data: PersonData) -> Result<(), Problem> {
        let name = text("name", &data.name)?;
        let name = match name {
            Some(n) => n,
            None if self.former_member => String::new(),
            None => return Err(Problem::validation("name", tr(None, "err.empty", &[]))),
        };
        self.aliases = aliases(&name, &data.aliases)?;
        self.former_member = self.former_member && name.is_empty();
        self.name = name;
        Ok(())
    }

    /// Участник удалил аккаунт: имя стирается, связь с пользователем — тоже, запись — в архив.
    /// Операции, в которых он получатель, остаются как есть.
    pub fn forget_member(&mut self) {
        self.user_id = None;
        self.name.clear();
        self.aliases.clear();
        self.former_member = true;
        self.archived = true;
    }
}

/// Код записи участника из имени: латинские буквы и цифры в верхнем регистре (диакритика снимается: Ánna →
/// ANNA), остальное — `_`; без латиницы — [`MEMBER_CODE`]. Занятый код — с `_2`, `_3`…
pub fn member_code(name: &str, taken: &[String]) -> String {
    let mut base = String::new();
    for c in kernel::search::fold(name).chars() {
        if c.is_ascii_alphanumeric() {
            base.push(c.to_ascii_uppercase());
        } else if !base.is_empty() && !base.ends_with('_') {
            base.push('_');
        }
    }
    // Место под «_99» — код не длиннее предела.
    base.truncate(kernel::code::MAX_CODE - 3);
    let base = base.trim_end_matches('_');
    let base = if base.bytes().any(|b| b.is_ascii_uppercase()) { base.to_string() } else { MEMBER_CODE.to_string() };
    if !taken.contains(&base) {
        return base;
    }
    (2..).map(|n| format!("{base}_{n}")).find(|c| !taken.contains(c)).unwrap_or(base)
}

/// Необязательный текст: пусто → `None`, не длиннее [`MAX_TEXT`].
fn text(field: &str, value: &str) -> Result<Option<String>, Problem> {
    let value = value.trim();
    if value.chars().count() > MAX_TEXT {
        return Err(Problem::validation(field, tr(Some(crate::i18n::dict), "err.too_long", &[("max", &MAX_TEXT)])));
    }
    Ok(Some(value.to_string()).filter(|v| !v.is_empty()))
}

/// Другие имена: без пустых, повторов (без учёта регистра и диакритики) и самого имени.
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

/// Другие имена из одной строки формы: через запятую или с новой строки.
pub fn split_aliases(s: &str) -> Vec<String> {
    s.split([',', '\n']).map(str::trim).filter(|a| !a.is_empty()).map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Раздел 5.9: латиница и цифры имени в верхнем регистре, остальное — `_`; без латиницы — MEMBER; занятый — `_2`.
    #[test]
    fn member_codes() {
        let none: Vec<String> = Vec::new();
        assert_eq!(member_code("Alexey Yolshin", &none), "ALEXEY_YOLSHIN");
        assert_eq!(member_code("  Ánna K. ", &none), "ANNA_K");
        assert_eq!(member_code("Pinpan (ปิ่น) 2", &none), "PINPAN_2");
        assert_eq!(member_code("Алексей", &none), "MEMBER");
        assert_eq!(member_code("ปิ่นปั่น", &none), "MEMBER");
        assert_eq!(member_code("007", &none), "MEMBER", "цифры без латиницы — не имя");
        let taken = vec!["ANNA".to_string(), "ANNA_2".to_string(), "MEMBER".to_string()];
        assert_eq!(member_code("Anna", &taken), "ANNA_3");
        assert_eq!(member_code("Мария", &taken), "MEMBER_2");
        let long = member_code(&"Very long name ".repeat(10), &none);
        assert!(long.len() <= kernel::code::MAX_CODE - 3 && !long.ends_with('_'), "{long}");
        assert!(kernel::code::fresh_code("code", &member_code("Ёлка Ølsen", &none)).is_ok());
    }

    /// Удалённый участник: имя стёрто, пустое имя при правке его оставляет, введённое — делает обычным человеком.
    #[test]
    fn former_member_name() {
        let mut p = Person::member(Uuid::new_v4(), "ANNA".into(), Uuid::new_v4(), " Anna ");
        assert_eq!(p.name, "Anna");
        assert!(p.change(PersonData { name: " ".into(), aliases: vec![] }).is_err(), "имя обязательно");
        p.forget_member();
        assert_eq!((p.user_id, p.name.as_str(), p.former_member, p.archived), (None, "", true, true));
        p.change(PersonData { name: "".into(), aliases: vec!["A".into()] }).unwrap();
        assert!(p.former_member);
        p.change(PersonData { name: "Тётя Аня".into(), aliases: vec![] }).unwrap();
        assert!(!p.former_member);
    }
}
