//! Доменная модель «Проекты»: пользователи, проекты (бюджеты), участники и их роли.

use kernel::auth::Role;
use kernel::code::require_text;
use kernel::i18n::Lang;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub password_hash: String,
    pub lang: Lang,
}

/// Личный бюджет — один участник; семейный — несколько.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Personal,
    Family,
}

impl ProjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectKind::Personal => "PERSONAL",
            ProjectKind::Family => "FAMILY",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "PERSONAL" => Some(ProjectKind::Personal),
            "FAMILY" => Some(ProjectKind::Family),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub kind: ProjectKind,
    pub base_currency: String,
    pub created_by: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub role: Role,
}

/// Email: непустой, с одной `@` и точкой в домене; хранится в нижнем регистре.
pub fn normalize_email(value: &str) -> Result<String, String> {
    let e = value.trim().to_lowercase();
    let ok = e.split_once('@').is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.') && !domain.contains('@'));
    if ok { Ok(e) } else { Err(format!("email: '{value}' — неверный адрес")) }
}

/// Минимальная длина пароля.
pub const MIN_PASSWORD: usize = 6;

pub fn check_password(p: &str) -> Result<(), String> {
    if p.chars().count() < MIN_PASSWORD {
        return Err(format!("password: не короче {MIN_PASSWORD} символов"));
    }
    Ok(())
}

/// Код базовой валюты: три латинские буквы (ISO 4217).
pub fn normalize_currency(value: &str) -> Result<String, String> {
    let c = value.trim().to_uppercase();
    if c.len() == 3 && c.chars().all(|ch| ch.is_ascii_alphabetic()) { Ok(c) } else { Err(format!("base_currency: '{value}' — нужен код ISO 4217, например THB")) }
}

impl Project {
    pub fn new(name: &str, kind: ProjectKind, base_currency: &str, created_by: Uuid) -> Result<Self, String> {
        Ok(Self {
            id: Uuid::new_v4(),
            name: require_text("name", name)?,
            kind,
            base_currency: normalize_currency(base_currency)?,
            created_by,
        })
    }
}

/// Проверка изменения состава: в личном проекте один участник, у проекта всегда есть владелец.
pub fn check_members(kind: ProjectKind, members: &[Member]) -> Result<(), String> {
    if kind == ProjectKind::Personal && members.len() > 1 {
        return Err("личный проект — только для одного человека; создайте семейный проект".into());
    }
    if !members.iter().any(|m| m.role == Role::Owner) {
        return Err("у проекта должен остаться хотя бы один владелец (OWNER)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_and_currency() {
        assert_eq!(normalize_email(" Anna@Demo.Local ").unwrap(), "anna@demo.local");
        assert!(normalize_email("anna").is_err());
        assert!(normalize_email("@x.y").is_err());
        assert_eq!(normalize_currency("thb").unwrap(), "THB");
        assert!(normalize_currency("бат").is_err());
    }

    #[test]
    fn member_rules() {
        let p = Uuid::new_v4();
        let m = |role| Member { project_id: p, user_id: Uuid::new_v4(), role };
        assert!(check_members(ProjectKind::Family, &[m(Role::Owner), m(Role::Viewer)]).is_ok());
        assert!(check_members(ProjectKind::Personal, &[m(Role::Owner), m(Role::Editor)]).is_err());
        assert!(check_members(ProjectKind::Family, &[m(Role::Editor)]).is_err());
    }
}
