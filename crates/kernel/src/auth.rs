//! Текущий пользователь, активный проект и проверка доступа. Принадлежит проекту (upgrade не трогает).
//!
//! Контракт для фич:
//! - `Extension(scope): Extension<Scope>` — чей это запрос и в каком проекте. Все данные фич
//!   изолированы по `scope.project_id`.
//! - `Extension(session): Extension<Session>` — то же + имя, язык и список проектов (для страниц).
//!
//! Токен — JWT HS256 из заголовка `Authorization: Bearer` (API) или cookie `fb_token` (веб).
//! Активный проект — заголовок `X-Project-Id` или cookie `fb_project`, иначе проект по умолчанию.
//! Для Rauthy/OIDC меняется только проверка токена здесь — фичи не изменятся.
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Request, State};
use axum::http::header::{ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n::Lang;

pub const TOKEN_COOKIE: &str = "fb_token";
pub const PROJECT_COOKIE: &str = "fb_project";
pub const LANG_COOKIE: &str = "fb_lang";
pub const PROJECT_HEADER: &str = "x-project-id";

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
}

/// Роль участника в проекте. Порядок важен: Viewer < Editor < Owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Editor,
    Owner,
}

impl Role {
    pub const VALUES: [&'static str; 3] = ["VIEWER", "EDITOR", "OWNER"];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "VIEWER",
            Role::Editor => "EDITOR",
            Role::Owner => "OWNER",
        }
    }

    pub fn parse(value: &str) -> Option<Role> {
        match value {
            "VIEWER" => Some(Role::Viewer),
            "EDITOR" => Some(Role::Editor),
            "OWNER" => Some(Role::Owner),
            _ => None,
        }
    }
}

/// Область данных запроса: пользователь + активный проект + его роль в проекте.
#[derive(Debug, Clone, Copy)]
pub struct Scope {
    pub user_id: Uuid,
    pub project_id: Uuid,
    pub role: Role,
}

impl Scope {
    pub fn can_write(&self) -> bool {
        self.role >= Role::Editor
    }
}

#[derive(Debug, Clone)]
pub struct ProjectRef {
    pub id: Uuid,
    pub name: String,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct MemberRef {
    pub user_id: Uuid,
    pub name: String,
    pub role: Role,
}

/// Всё, что нужно странице о пользователе и проекте.
#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: Uuid,
    pub user_name: String,
    /// Язык интерфейса: cookie → настройка пользователя → Accept-Language → ru.
    pub lang: Lang,
    pub project_id: Uuid,
    pub project_name: String,
    /// Базовая валюта проекта — в ней считается бюджет.
    pub base_currency: String,
    pub role: Role,
    pub projects: Vec<ProjectRef>,
    /// Участники активного проекта.
    pub members: Vec<MemberRef>,
}

impl Session {
    pub fn member_name(&self, user_id: &Uuid) -> String {
        self.members.iter().find(|m| m.user_id == *user_id).map(|m| m.name.clone()).unwrap_or_else(|| "—".into())
    }

    pub fn scope(&self) -> Scope {
        Scope { user_id: self.user_id, project_id: self.project_id, role: self.role }
    }
}

/// Данные о пользователе и его проекте, которые даёт фича `projects` (адаптер — в app/wiring).
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub user_name: String,
    pub user_lang: Lang,
    pub project_id: Uuid,
    pub project_name: String,
    pub base_currency: String,
    pub role: Role,
    pub projects: Vec<ProjectRef>,
    pub members: Vec<MemberRef>,
}

/// Порт ядра: кто пользователь и в каких проектах состоит.
#[async_trait]
pub trait Memberships: Send + Sync {
    /// Сессия для пользователя в проекте `project` (или в проекте по умолчанию, если `None`
    /// либо пользователь в нём не состоит). `None` — пользователь не найден или у него нет проектов.
    async fn session(&self, user_id: Uuid, project: Option<Uuid>) -> AppResult<Option<SessionInfo>>;
}

/// Выпуск и проверка токенов (HS256). Лежит в `AppContext`: логин выпускает токен, middleware проверяет.
#[derive(Clone)]
pub struct Tokens {
    decoding: DecodingKey,
    encoding: EncodingKey,
    validation: Validation,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    exp: usize,
}

impl Tokens {
    pub fn hs256(secret: &str) -> Self {
        Self {
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            validation: Validation::default(),
        }
    }

    /// Выпустить токен на 30 дней (демо; с Rauthy токены выпускает провайдер).
    pub fn issue(&self, user_id: Uuid) -> String {
        let exp = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
            + 30 * 24 * 3600) as usize;
        encode(&Header::default(), &Claims { sub: user_id, exp }, &self.encoding).unwrap_or_default()
    }

    pub fn verify(&self, token: &str) -> Option<Uuid> {
        decode::<Claims>(token, &self.decoding, &self.validation).ok().map(|d| d.claims.sub)
    }
}

#[derive(Clone)]
pub struct AuthState {
    pub tokens: Tokens,
    pub memberships: Arc<dyn Memberships>,
}

/// Значение cookie из заголовков запроса.
pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

/// Заголовок Set-Cookie (HttpOnly, SameSite=Lax, на год; `max_age = 0` удаляет).
pub fn set_cookie(name: &str, value: &str, max_age: i64) -> (axum::http::HeaderName, String) {
    (SET_COOKIE, format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}"))
}

/// Язык интерфейса из cookie / Accept-Language (без настройки пользователя — для публичных страниц).
pub fn request_lang(headers: &HeaderMap) -> Option<Lang> {
    cookie(headers, LANG_COOKIE).and_then(|v| Lang::parse(&v)).or_else(|| {
        headers.get(ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()).and_then(Lang::from_accept_language)
    })
}

fn wants_html(headers: &HeaderMap) -> bool {
    headers.get(ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("text/html"))
}

fn deny(headers: &HeaderMap, status: StatusCode) -> Response {
    if wants_html(headers) && status == StatusCode::UNAUTHORIZED {
        return Redirect::to("/login").into_response();
    }
    status.into_response()
}

/// Middleware: пользователь (JWT) → активный проект → роль. Пишущие запросы VIEWER получают 403.
pub async fn require_user(State(auth): State<AuthState>, mut req: Request, next: Next) -> Response {
    let headers = req.headers();
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
        .or_else(|| cookie(headers, TOKEN_COOKIE));
    let Some(user_id) = token.as_deref().and_then(|t| auth.tokens.verify(t)) else {
        return deny(headers, StatusCode::UNAUTHORIZED);
    };
    let hint = headers
        .get(PROJECT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| cookie(headers, PROJECT_COOKIE))
        .and_then(|v| Uuid::parse_str(&v).ok());
    let info = match auth.memberships.session(user_id, hint).await {
        Ok(Some(info)) => info,
        Ok(None) => return deny(headers, StatusCode::UNAUTHORIZED),
        Err(e) => return e.into_response(),
    };
    let lang = cookie(headers, LANG_COOKIE).and_then(|v| Lang::parse(&v)).unwrap_or(info.user_lang);
    let session = Session {
        user_id,
        user_name: info.user_name,
        lang,
        project_id: info.project_id,
        project_name: info.project_name,
        base_currency: info.base_currency,
        role: info.role,
        projects: info.projects,
        members: info.members,
    };
    // Управление проектами и участниками проверяет права само (роль — в том проекте, которым управляют).
    let path = req.uri().path();
    let own_rules = path.starts_with("/projects") || path.starts_with("/api/projects");
    let writes = !own_rules && !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if writes && !session.scope().can_write() {
        return (StatusCode::FORBIDDEN, "роль VIEWER: только просмотр").into_response();
    }
    req.extensions_mut().insert(CurrentUser { id: user_id });
    req.extensions_mut().insert(session.scope());
    req.extensions_mut().insert(session);
    next.run(req).await
}
