//! Настройки приложения из окружения. Секреты — только из окружения или файлов секретов
//! (`<ИМЯ>_FILE`), не из репозитория. В продакшене без секретов или со значениями по умолчанию
//! приложение не стартует ([`Config::validate`]).
use std::env;

use crate::crypto::Signer;

/// Секрет по умолчанию для разработки — в продакшене запрещён.
pub const DEV_SECRET: &str = "dev-only-secret-change-me-dev-only-secret";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Env {
    Dev,
    Prod,
}

/// Подключение к Rauthy (OpenID Connect).
#[derive(Debug, Clone)]
pub struct Oidc {
    /// Issuer, например `https://auth.example.com/auth/v1`.
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// `https://<домен>/auth/callback`.
    pub redirect_url: String,
    /// Страница регистрации Rauthy («Создать бюджет»).
    pub register_url: String,
    /// Личный кабинет Rauthy: пароль, второй фактор, привязки GitHub и Facebook.
    pub account_url: String,
    /// API-ключ Rauthy с правом удаления пользователей (удаление аккаунта). Необязателен.
    pub admin_api_key: Option<String>,
    /// Внутренний адрес Rauthy для запросов сервера (discovery, токены, JWKS), если он отличается от
    /// публичного: в compose браузер ходит на `localhost:8443`, а приложение — на `rauthy:8443`.
    pub internal_url: Option<String>,
}

impl Oidc {
    /// Issuer без слеша в конце — для построения адресов (`/users/{id}`, discovery).
    pub fn base(&self) -> &str {
        self.issuer.trim_end_matches('/')
    }

    /// Схема + хост issuer'а — для CSP `form-action` (переход в Rauthy после POST /auth/logout).
    pub fn origin(&self) -> String {
        origin_of(&self.issuer)
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub env: Env,
    /// Публичный адрес приложения без `/` в конце: `https://budget.example.com`.
    pub base_url: String,
    /// Подпись CSRF-токенов и служебных cookie.
    pub signer: Signer,
    pub oidc: Option<Oidc>,
    /// Вход без Rauthy под демо-пользователями — только для разработки.
    pub dev_login: bool,
    /// «Посмотреть демо» на лендинге: вход гостем в демо-проект с ролью VIEWER.
    pub guest_demo: bool,
    /// Swagger UI на `/docs` (в продакшене выключен).
    pub swagger: bool,
    /// Запросов в минуту на сессию / API-токен.
    pub rate_limit: u32,
    /// Запросов в минуту с одного IP к публичным страницам (лендинг, вход, демо).
    pub public_rate_limit: u32,
    /// Брать IP клиента из `X-Forwarded-For` (приложение за обратным прокси).
    pub trust_proxy: bool,
    /// Адрес для связи на лендинге и правовых страницах.
    pub contact_email: String,
}

/// Значение переменной `NAME` или содержимое файла из `NAME_FILE` (Docker/Podman secrets).
pub fn secret_var(name: &str) -> Option<String> {
    if let Ok(path) = env::var(format!("{name}_FILE")) {
        return std::fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    }
    env::var(name).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn flag(name: &str, default: bool) -> bool {
    env::var(name).map(|v| !matches!(v.trim(), "0" | "false" | "no" | "off" | "")).unwrap_or(default)
}

pub fn origin_of(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let host = rest.split('/').next().unwrap_or(rest);
    format!("{scheme}://{host}")
}

impl Config {
    pub fn from_env() -> Self {
        let env_kind = match env::var("APP_ENV").unwrap_or_default().trim().to_ascii_lowercase().as_str() {
            "prod" | "production" => Env::Prod,
            _ => Env::Dev,
        };
        let prod = env_kind == Env::Prod;
        let base_url = env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into()).trim_end_matches('/').to_string();
        let secret = secret_var("SESSION_SECRET").unwrap_or_else(|| DEV_SECRET.into());
        let oidc = match (env::var("OIDC_ISSUER").ok().filter(|v| !v.trim().is_empty()), env::var("OIDC_CLIENT_ID").ok()) {
            (Some(issuer), Some(client_id)) => {
                // Issuer — ровно как в метаданных провайдера (у Rauthy — со слешем в конце: `…/auth/v1/`).
                let issuer = issuer.trim().to_string();
                let base = issuer.trim_end_matches('/').to_string();
                Some(Oidc {
                    redirect_url: env::var("OIDC_REDIRECT_URL").unwrap_or_else(|_| format!("{base_url}/auth/callback")),
                    register_url: env::var("OIDC_REGISTER_URL").unwrap_or_else(|_| format!("{base}/users/register")),
                    account_url: env::var("OIDC_ACCOUNT_URL").unwrap_or_else(|_| format!("{base}/account")),
                    client_secret: secret_var("OIDC_CLIENT_SECRET").unwrap_or_default(),
                    admin_api_key: secret_var("RAUTHY_ADMIN_API_KEY"),
                    internal_url: env::var("OIDC_INTERNAL_URL").ok().map(|v| v.trim().trim_end_matches('/').to_string()).filter(|v| !v.is_empty()),
                    issuer,
                    client_id,
                })
            }
            _ => None,
        };
        Config {
            env: env_kind,
            signer: Signer::new(secret.as_bytes()),
            dev_login: !prod && flag("DEV_LOGIN", true),
            guest_demo: flag("GUEST_DEMO", true),
            swagger: flag("SWAGGER", !prod),
            rate_limit: env::var("RATE_LIMIT_PER_MIN").ok().and_then(|v| v.parse().ok()).unwrap_or(600),
            public_rate_limit: env::var("PUBLIC_RATE_LIMIT_PER_MIN").ok().and_then(|v| v.parse().ok()).unwrap_or(120),
            trust_proxy: flag("TRUST_PROXY", false),
            contact_email: env::var("CONTACT_EMAIL").unwrap_or_else(|_| format!("hello@{}", base_url.split("://").nth(1).unwrap_or("localhost").split([':', '/']).next().unwrap_or("localhost"))),
            oidc,
            base_url,
        }
    }

    /// Проверка продакшен-настроек: без секретов и HTTPS приложение не стартует.
    pub fn validate(&self) -> Result<(), String> {
        if self.env != Env::Prod {
            return Ok(());
        }
        let mut errors = Vec::new();
        match secret_var("SESSION_SECRET") {
            None => errors.push("SESSION_SECRET не задан".to_string()),
            Some(s) if s == DEV_SECRET || s.len() < 32 || s.contains("change-me") => {
                errors.push("SESSION_SECRET: нужен случайный секрет ≥ 32 символов, не значение по умолчанию".into())
            }
            _ => {}
        }
        if !self.base_url.starts_with("https://") {
            errors.push("APP_BASE_URL: в продакшене только https://".into());
        }
        match &self.oidc {
            None => errors.push("OIDC_ISSUER и OIDC_CLIENT_ID не заданы — вход через Rauthy обязателен".into()),
            Some(o) => {
                if o.client_secret.is_empty() || o.client_secret.contains("change-me") {
                    errors.push("OIDC_CLIENT_SECRET не задан".into());
                }
                if !o.issuer.starts_with("https://") {
                    errors.push("OIDC_ISSUER: в продакшене только https://".into());
                }
            }
        }
        if self.swagger {
            errors.push("SWAGGER=1 запрещён в продакшене".into());
        }
        if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
    }

    pub fn is_prod(&self) -> bool {
        self.env == Env::Prod
    }

    /// Cookie только по HTTPS (`Secure`) и с префиксом `__Host-` — если сайт открыт по https.
    pub fn secure_cookies(&self) -> bool {
        self.base_url.starts_with("https://")
    }

    pub fn origin(&self) -> String {
        origin_of(&self.base_url)
    }

    /// Имя cookie сессии: `__Host-session` (HTTPS) или `fb_session` (локально по http).
    pub fn session_cookie(&self) -> &'static str {
        if self.secure_cookies() { "__Host-session" } else { "fb_session" }
    }

    /// Конфигурация для тестов.
    pub fn test() -> Self {
        Config {
            env: Env::Dev,
            base_url: "http://localhost:8080".into(),
            signer: Signer::new(DEV_SECRET.as_bytes()),
            oidc: None,
            dev_login: true,
            guest_demo: true,
            swagger: false,
            rate_limit: 600,
            public_rate_limit: 120,
            trust_proxy: false,
            contact_email: "hello@localhost".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prod_requires_secrets_and_https() {
        let mut c = Config::test();
        assert!(c.validate().is_ok(), "в разработке проверок нет");
        c.env = Env::Prod;
        let err = c.validate().unwrap_err();
        assert!(err.contains("APP_BASE_URL") && err.contains("OIDC_ISSUER"), "{err}");
        assert_eq!(origin_of("https://auth.example.com/auth/v1"), "https://auth.example.com");
        assert_eq!(Config::test().session_cookie(), "fb_session");
    }
}
