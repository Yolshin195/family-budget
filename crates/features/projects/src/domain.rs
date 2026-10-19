//! Доменная модель «Проекты»: пользователи (из Rauthy), проекты, участники и роли; доступ —
//! серверные сессии, личные API-токены, журнал безопасности.

use chrono::{DateTime, Duration, Utc};
use kernel::auth::{Role, Theme};
use kernel::code::require_text;
use kernel::i18n::Lang;
use uuid::Uuid;

/// Пользователь приложения. Паролей нет: вход подтверждает Rauthy, связь — по `rauthy_sub`.
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: Uuid,
    pub rauthy_sub: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub lang: Lang,
    pub theme: Theme,
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
    /// Участники без второго фактора не видят данных проекта.
    pub require_2fa: bool,
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

/// Код базовой валюты: три латинские буквы (ISO 4217).
pub fn normalize_currency(value: &str) -> Result<String, String> {
    let c = value.trim().to_uppercase();
    if c.len() == 3 && c.chars().all(|ch| ch.is_ascii_alphabetic()) { Ok(c) } else { Err(format!("base_currency: '{value}' — нужен код ISO 4217, например THB")) }
}

/// Имя пользователя: непустое, до 100 символов.
pub fn normalize_name(value: &str) -> Result<String, String> {
    let n = require_text("display_name", value)?;
    if n.chars().count() > 100 {
        return Err("display_name: не длиннее 100 символов".into());
    }
    Ok(n)
}

impl Project {
    pub fn new(name: &str, kind: ProjectKind, base_currency: &str, created_by: Uuid) -> Result<Self, String> {
        Ok(Self {
            id: Uuid::new_v4(),
            name: require_text("name", name)?,
            kind,
            base_currency: normalize_currency(base_currency)?,
            created_by,
            require_2fa: false,
        })
    }
}

/// Название личного проекта на языке пользователя.
pub fn personal_name(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Личный бюджет",
        Lang::En => "Personal budget",
        Lang::Th => "งบส่วนตัว",
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

/// Название семейного проекта, в который превратили личный с названием по умолчанию.
pub fn family_name(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Семейный бюджет",
        Lang::En => "Family budget",
        Lang::Th => "งบครอบครัว",
    }
}

/// Название личного проекта по умолчанию (на любом языке) — его стоит сменить, когда проект станет семейным.
pub fn is_default_personal_name(name: &str) -> bool {
    Lang::ALL.iter().any(|l| personal_name(*l) == name)
}

// ---------------------------------------------------------------- приглашения

/// Срок действия приглашения.
pub const INVITE_TTL: Duration = Duration::days(7);
/// Действующих приглашений на проект — не больше (защита от забытых ссылок).
pub const MAX_ACTIVE_INVITES: usize = 20;

/// Приглашение в семейный проект по ссылке или QR-коду: одно приглашение — один человек.
#[derive(Debug, Clone, PartialEq)]
pub struct Invite {
    pub id: Uuid,
    pub project_id: Uuid,
    pub role: Role,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_by: Option<Uuid>,
    pub used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteState {
    Active,
    Used,
    Expired,
    Revoked,
}

impl InviteState {
    pub fn as_str(self) -> &'static str {
        match self {
            InviteState::Active => "active",
            InviteState::Used => "used",
            InviteState::Expired => "expired",
            InviteState::Revoked => "revoked",
        }
    }
}

impl Invite {
    /// Новое приглашение. Владельцем по ссылке не стать — только редактор или наблюдатель.
    pub fn new(project_id: Uuid, role: Role, created_by: Uuid, now: DateTime<Utc>) -> Result<Self, String> {
        if role == Role::Owner {
            return Err("role: по приглашению можно стать редактором или наблюдателем; владельца назначают в «Участниках»".into());
        }
        Ok(Self { id: Uuid::new_v4(), project_id, role, created_by, created_at: now, expires_at: now + INVITE_TTL, used_by: None, used_at: None, revoked_at: None })
    }

    pub fn state(&self, now: DateTime<Utc>) -> InviteState {
        if self.used_at.is_some() {
            InviteState::Used
        } else if self.revoked_at.is_some() {
            InviteState::Revoked
        } else if now >= self.expires_at {
            InviteState::Expired
        } else {
            InviteState::Active
        }
    }
}

/// Код приглашения в ссылке: `<id без дефисов>.<HMAC>`; разбор без проверки подписи.
pub fn split_invite_code(code: &str) -> Option<(Uuid, &str)> {
    let (id, sig) = code.split_once('.')?;
    if id.len() != 32 || sig.is_empty() || !sig.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    Some((Uuid::try_parse(id).ok()?, sig))
}

// ---------------------------------------------------------------- вход через Rauthy

/// Проверенный access-токен провайдера входа (например, выданный ассистенту для MCP).
#[derive(Debug, Clone, PartialEq)]
pub struct AccessToken {
    /// Пользователь у провайдера (`sub`).
    pub sub: String,
    /// OAuth-клиент, которому выдан токен (`azp` / `client_id`).
    pub client_id: String,
}

/// Пользователь, подтверждённый Rauthy (проверенный ID-токен).
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedLogin {
    pub sub: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    /// Методы входа из claim `amr` (RFC 8176).
    pub amr: Vec<String>,
}

impl VerifiedLogin {
    /// Был ли второй фактор: код по email (`otp`), passkey (`hwk`/`swk`), биометрия или явный `mfa`.
    pub fn mfa(&self) -> bool {
        const SECOND: [&str; 9] = ["mfa", "otp", "hwk", "swk", "sms", "fpt", "face", "iris", "pop"];
        self.amr.iter().any(|m| SECOND.contains(&m.to_ascii_lowercase().as_str()))
    }

    /// Имя для показа: из токена, иначе часть email до `@`.
    pub fn display_name(&self) -> String {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(|n| n.chars().take(100).collect())
            .unwrap_or_else(|| self.email.split('@').next().unwrap_or("user").to_string())
    }
}

/// Начатый вход: что лежит в подписанной cookie на время похода в Rauthy (10 минут).
#[derive(Debug, Clone, PartialEq)]
pub struct PendingLogin {
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
    /// Куда вернуть пользователя после входа (относительный адрес).
    pub next: String,
    /// Unix-время начала входа.
    pub started: i64,
}

/// Срок на возврат из Rauthy.
pub const LOGIN_TTL_SECS: i64 = 10 * 60;

impl PendingLogin {
    pub fn expired(&self, now: DateTime<Utc>) -> bool {
        now.timestamp() - self.started > LOGIN_TTL_SECS || now.timestamp() < self.started - 60
    }
}

/// Только относительный адрес этого сайта — без открытых редиректов.
pub fn safe_next(next: &str) -> String {
    let ok = next.starts_with('/') && !next.starts_with("//") && !next.starts_with("/\\") && !next.contains("://");
    if ok && !next.starts_with("/auth/") { next.to_string() } else { "/budget".to_string() }
}

// ---------------------------------------------------------------- сессии

/// Неактивность, после которой сессия закрывается.
pub const SESSION_IDLE: Duration = Duration::days(7);
/// Абсолютный срок жизни сессии.
pub const SESSION_MAX: Duration = Duration::days(30);
/// Как часто обновлять `last_seen_at` (чтобы не писать в БД на каждый запрос).
pub const TOUCH_EVERY: Duration = Duration::minutes(5);

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    pub id_hash: String,
    pub user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub mfa: bool,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl SessionRecord {
    pub fn new(id_hash: String, user_id: Uuid, mfa: bool, ip: Option<String>, user_agent: Option<String>, now: DateTime<Utc>) -> Self {
        Self { id_hash, user_id, created_at: now, last_seen_at: now, expires_at: now + SESSION_MAX, mfa, ip, user_agent, revoked_at: None }
    }

    /// Действует: не отозвана, не истекла, неактивность меньше 7 дней.
    pub fn active(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && now < self.expires_at && now - self.last_seen_at < SESSION_IDLE
    }

    pub fn needs_touch(&self, now: DateTime<Utc>) -> bool {
        now - self.last_seen_at >= TOUCH_EVERY
    }
}

// ---------------------------------------------------------------- API-токены

/// Сроки, которые можно выбрать при создании токена (дни; 0 — бессрочно).
pub const TOKEN_TERMS: [i64; 4] = [30, 90, 365, 0];

#[derive(Debug, Clone, PartialEq)]
pub struct ApiToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Выпущен из сессии со вторым фактором: в проекте с обязательной 2FA токен без него не действует.
    pub mfa: bool,
}

impl ApiToken {
    pub fn active(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.expires_at.is_none_or(|e| now < e)
    }
}

/// Права токена: `read`, `write`, `<фича>:read`, `<фича>:write` (фича — латиница и `_`).
pub fn normalize_scopes(scopes: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for s in scopes.iter().map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()) {
        let ok = match s.split_once(':') {
            None => s == "read" || s == "write",
            Some((f, a)) => !f.is_empty() && f.chars().all(|c| c.is_ascii_lowercase() || c == '_') && (a == "read" || a == "write"),
        };
        if !ok {
            return Err(format!("scopes: '{s}' — ожидается read, write или <фича>:read|write"));
        }
        if !out.contains(&s) {
            out.push(s);
        }
    }
    if out.is_empty() {
        return Err("scopes: выберите хотя бы одно право".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------- журнал безопасности

/// События журнала (свои пользователь видит в профиле). Входы и 2FA дополнительно ведёт Rauthy.
pub mod action {
    pub const LOGIN: &str = "login";
    /// Вход не удался (ошибка от Rauthy, неверный state, ID-токен не прошёл проверку). Без пользователя.
    pub const LOGIN_FAILED: &str = "login_failed";
    pub const LOGOUT: &str = "logout";
    pub const LOGOUT_ALL: &str = "logout_all";
    pub const SESSION_REVOKED: &str = "session_revoked";
    pub const PROJECT_CREATED: &str = "project_created";
    pub const PROJECT_DELETED: &str = "project_deleted";
    pub const MEMBER_ADDED: &str = "member_added";
    pub const MEMBER_ROLE: &str = "member_role_changed";
    pub const MEMBER_REMOVED: &str = "member_removed";
    pub const MEMBER_JOINED: &str = "member_joined";
    pub const INVITE_CREATED: &str = "invite_created";
    pub const INVITE_REVOKED: &str = "invite_revoked";
    pub const PROJECT_MADE_FAMILY: &str = "project_made_family";
    pub const REQUIRE_2FA: &str = "require_2fa_changed";
    pub const TOKEN_CREATED: &str = "api_token_created";
    pub const TOKEN_REVOKED: &str = "api_token_revoked";
    pub const ACCOUNT_DELETED: &str = "account_deleted";
}

/// Хранить журнал — год.
pub const AUDIT_RETENTION: Duration = Duration::days(365);

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub id: Uuid,
    pub at: DateTime<Utc>,
    pub user_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub action: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// Подробности события: пары ключ — значение (в БД — JSONB).
    pub details: Vec<(String, String)>,
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

    #[test]
    fn invite_rules_and_states() {
        let now = Utc::now();
        assert!(Invite::new(Uuid::nil(), Role::Owner, Uuid::nil(), now).is_err(), "владельцем по ссылке не стать");
        let i = Invite::new(Uuid::nil(), Role::Editor, Uuid::nil(), now).unwrap();
        assert_eq!(i.state(now), InviteState::Active);
        assert_eq!(i.state(now + Duration::days(7)), InviteState::Expired);
        assert_eq!(Invite { revoked_at: Some(now), ..i.clone() }.state(now), InviteState::Revoked);
        assert_eq!(Invite { used_at: Some(now), revoked_at: Some(now), ..i.clone() }.state(now), InviteState::Used);
        let id = Uuid::new_v4();
        assert_eq!(split_invite_code(&format!("{}.abc-_9", id.simple())), Some((id, "abc-_9")));
        assert_eq!(split_invite_code(&format!("{id}.abc")), None, "id только без дефисов");
        assert_eq!(split_invite_code(&format!("{}./x", id.simple())), None);
        assert_eq!(split_invite_code("abc"), None);
        assert!(is_default_personal_name("Personal budget") && !is_default_personal_name("Дом"));
    }

    #[test]
    fn second_factor_from_amr() {
        let l = |amr: &[&str]| VerifiedLogin { sub: "s".into(), email: "a@b.c".into(), name: None, picture: None, amr: amr.iter().map(|s| s.to_string()).collect() };
        assert!(!l(&["pwd"]).mfa());
        assert!(l(&["pwd", "otp"]).mfa());
        assert!(l(&["hwk"]).mfa(), "passkey");
        assert!(l(&["MFA"]).mfa());
        assert_eq!(l(&[]).display_name(), "a");
    }

    #[test]
    fn sessions_expire_by_idle_and_lifetime() {
        let now = Utc::now();
        let s = SessionRecord::new("h".into(), Uuid::nil(), false, None, None, now);
        assert!(s.active(now));
        assert!(!s.active(now + Duration::days(8)), "7 дней неактивности");
        let mut busy = s.clone();
        busy.last_seen_at = now + Duration::days(29);
        assert!(busy.active(now + Duration::days(29)));
        assert!(!busy.active(now + Duration::days(31)), "30 дней — предел");
        let revoked = SessionRecord { revoked_at: Some(now), ..s };
        assert!(!revoked.active(now));
    }

    #[test]
    fn token_scopes_and_redirects() {
        assert_eq!(normalize_scopes(&["Transactions:Write".into(), "read".into(), "read".into()]).unwrap(), vec!["transactions:write", "read"]);
        assert!(normalize_scopes(&["admin".into()]).is_err());
        assert!(normalize_scopes(&[]).is_err());
        assert_eq!(safe_next("/transactions?month=2026-10"), "/transactions?month=2026-10");
        assert_eq!(safe_next("//evil.example"), "/budget");
        assert_eq!(safe_next("https://evil.example"), "/budget");
        assert_eq!(safe_next("/auth/callback"), "/budget");
        let p = PendingLogin { state: "s".into(), nonce: "n".into(), pkce_verifier: "v".into(), next: "/".into(), started: Utc::now().timestamp() };
        assert!(!p.expired(Utc::now()));
        assert!(p.expired(Utc::now() + Duration::minutes(11)));
    }
}
