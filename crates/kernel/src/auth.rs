//! Текущий пользователь, активный проект и проверка доступа. Принадлежит проекту (upgrade не трогает).
//!
//! Контракт для фич:
//! - `Extension(scope): Extension<Scope>` — чей это запрос и в каком проекте. Все данные фич
//!   изолированы по `scope.project_id`.
//! - `Extension(session): Extension<Session>` — то же + имя, язык, тема, проекты (для страниц).
//!
//! Вход (раздел 12–13 ТЗ): пользователя подтверждает Rauthy (OIDC), приложение держит **серверные
//! сессии** — в cookie случайный id 256 бит, в БД его хеш. Клиенты без браузера — личные API-токены
//! (`Authorization: Bearer fbt_…`) с правами и одним проектом. Оба пути дают фичам один `Scope`.
//! Проверку сессий и токенов делает фича `projects` через порт [`Authenticator`].
//!
//! Меняющие запросы по cookie-сессии проходят CSRF-проверку: заголовок `Origin` + токен
//! (поле формы `_csrf` или заголовок `X-CSRF-Token`). VIEWER получает 403 на любое изменение.
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::header::{ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, CONTENT_TYPE, COOKIE, ORIGIN, REFERER, SET_COOKIE, USER_AGENT};
use axum::http::{HeaderMap, HeaderName, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use uuid::Uuid;

use crate::config::Config;
use crate::crypto::ct_eq;
use crate::error::{AppError, AppResult};
use crate::i18n::Lang;
use crate::throttle::RateLimiter;

pub const PROJECT_COOKIE: &str = "fb_project";
pub const LANG_COOKIE: &str = "fb_lang";
pub const PROJECT_HEADER: &str = "x-project-id";
pub const CSRF_HEADER: &str = "x-csrf-token";
pub const CSRF_FIELD: &str = "_csrf";
/// Префикс личных API-токенов: по нему токен отличим от чего-то другого в заголовке.
pub const API_TOKEN_PREFIX: &str = "fbt_";
/// Максимальный размер тела запроса (ТЗ, раздел 13).
pub const MAX_BODY: usize = 64 * 1024;

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

/// Тема интерфейса: светлая, тёмная или по системной настройке.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

impl Theme {
    pub const VALUES: [&'static str; 3] = ["SYSTEM", "LIGHT", "DARK"];

    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Light => "LIGHT",
            Theme::Dark => "DARK",
            Theme::System => "SYSTEM",
        }
    }

    pub fn parse(value: &str) -> Option<Theme> {
        match value.trim().to_ascii_uppercase().as_str() {
            "LIGHT" => Some(Theme::Light),
            "DARK" => Some(Theme::Dark),
            "SYSTEM" => Some(Theme::System),
            _ => None,
        }
    }

    /// Значение атрибута `data-theme` у `<html>` (пусто — по системной настройке).
    pub fn attr(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "",
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

/// Чем подтверждён запрос.
#[derive(Debug, Clone, PartialEq)]
pub enum Via {
    /// Cookie-сессия браузера; `id_hash` — SHA-256 id сессии.
    Session { id_hash: String },
    /// Личный API-токен: проект фиксирован, права — `scopes`.
    Token { token_id: Uuid, project_id: Uuid, scopes: Vec<String> },
}

/// Подтверждённый пользователь (до выбора проекта).
#[derive(Debug, Clone)]
pub struct Identity {
    pub user_id: Uuid,
    /// Был ли второй фактор при входе (claim `amr` ID-токена Rauthy).
    pub mfa: bool,
    pub via: Via,
}

impl Identity {
    /// Ключ для лимита запросов: сессия или токен.
    pub fn throttle_key(&self) -> String {
        match &self.via {
            Via::Session { id_hash } => format!("s:{id_hash}"),
            Via::Token { token_id, .. } => format!("t:{token_id}"),
        }
    }
}

/// Предъявленные учётные данные.
#[derive(Debug, Clone)]
pub enum Credential {
    /// Сырой id сессии из cookie.
    Session(String),
    /// Сырой API-токен из `Authorization: Bearer`.
    ApiToken(String),
}

/// IP и браузер — для списка устройств и журнала безопасности.
#[derive(Debug, Clone, Default)]
pub struct RequestMeta {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl RequestMeta {
    pub fn from_request(headers: &HeaderMap, peer: Option<SocketAddr>, trust_proxy: bool) -> Self {
        let forwarded = trust_proxy
            .then(|| headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.split(',').next()).map(|v| v.trim().to_string()))
            .flatten();
        RequestMeta {
            ip: forwarded.or_else(|| peer.map(|p| p.ip().to_string())),
            user_agent: headers.get(USER_AGENT).and_then(|v| v.to_str().ok()).map(|v| v.chars().take(300).collect()),
        }
    }
}

/// Всё, что нужно странице о пользователе и проекте.
#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: Uuid,
    pub user_name: String,
    /// Язык интерфейса: настройка пользователя → cookie → Accept-Language → ru.
    pub lang: Lang,
    pub theme: Theme,
    pub project_id: Uuid,
    pub project_name: String,
    /// Базовая валюта проекта — в ней считается бюджет.
    pub base_currency: String,
    /// Как показывать валюты проекта рядом с суммами: код → символ (см. `currency_marks`).
    pub currency_marks: HashMap<String, String>,
    pub role: Role,
    pub projects: Vec<ProjectRef>,
    /// Участники активного проекта.
    pub members: Vec<MemberRef>,
    /// Проект требует второй фактор от участников.
    pub require_2fa: bool,
    /// Вход был со вторым фактором.
    pub mfa: bool,
    /// CSRF-токен для форм и htmx-запросов (пусто у API-токенов).
    pub csrf: String,
    /// Запрос по API-токену, а не из браузера.
    pub via_token: bool,
}

impl Session {
    /// Валюта рядом с суммой: символ, если он есть, иначе код.
    pub fn cur<'a>(&'a self, code: &'a str) -> &'a str {
        self.currency_marks.get(code).map(String::as_str).unwrap_or(code)
    }

    pub fn member_name(&self, user_id: &Uuid) -> String {
        self.members.iter().find(|m| m.user_id == *user_id).map(|m| m.name.clone()).unwrap_or_else(|| "—".into())
    }

    pub fn scope(&self) -> Scope {
        Scope { user_id: self.user_id, project_id: self.project_id, role: self.role }
    }

    /// Данные проекта закрыты: проект требует 2FA, а вход был без второго фактора.
    pub fn mfa_blocked(&self) -> bool {
        self.require_2fa && !self.mfa
    }
}

/// Данные о пользователе и его проекте, которые даёт фича `projects` (адаптер — в app/wiring).
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub user_name: String,
    pub user_lang: Lang,
    pub theme: Theme,
    pub project_id: Uuid,
    pub project_name: String,
    pub base_currency: String,
    /// Код → символ валют проекта; пусто — показываются коды.
    pub currency_marks: HashMap<String, String>,
    pub role: Role,
    pub require_2fa: bool,
    pub projects: Vec<ProjectRef>,
    pub members: Vec<MemberRef>,
}

/// Символы валют для показа рядом с суммами из пар (код, символ). Пустой символ и символ, общий
/// для нескольких валют проекта (`$` у USD и CAD), не берутся — у таких валют остаётся код.
pub fn currency_marks(list: impl IntoIterator<Item = (String, String)>) -> HashMap<String, String> {
    let list: Vec<(String, String)> = list.into_iter().map(|(c, s)| (c, s.trim().to_string())).filter(|(_, s)| !s.is_empty()).collect();
    list.iter().filter(|(_, s)| list.iter().filter(|(_, x)| x == s).count() == 1).cloned().collect()
}

/// Порт ядра: кто пользователь и в каких проектах состоит.
#[async_trait]
pub trait Memberships: Send + Sync {
    /// Сессия для пользователя в проекте `project` (или в проекте по умолчанию, если `None`
    /// либо пользователь в нём не состоит). `None` — пользователь не найден или у него нет проектов.
    async fn session(&self, user_id: Uuid, project: Option<Uuid>) -> AppResult<Option<SessionInfo>>;
}

/// Порт ядра: проверка сессий и API-токенов (реализует фича `projects`).
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// `None` — сессия/токен неизвестны, отозваны или истекли.
    async fn authenticate(&self, credential: &Credential) -> AppResult<Option<Identity>>;
}

#[derive(Clone)]
pub struct AuthState {
    pub config: Arc<Config>,
    pub authenticator: Arc<dyn Authenticator>,
    pub memberships: Arc<dyn Memberships>,
    pub limiter: Arc<RateLimiter>,
}

/// CSRF-токен, положенный в запрос (для страниц без проекта — онбординг).
#[derive(Debug, Clone)]
pub struct Csrf(pub String);

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

/// Заголовок Set-Cookie: HttpOnly, SameSite=Lax, `Secure` на HTTPS; `max_age = 0` удаляет.
pub fn set_cookie(config: &Config, name: &str, value: &str, max_age: i64) -> (HeaderName, String) {
    set_cookie_at(config, name, value, max_age, "/")
}

pub fn set_cookie_at(config: &Config, name: &str, value: &str, max_age: i64, path: &str) -> (HeaderName, String) {
    let secure = if config.secure_cookies() { "; Secure" } else { "" };
    (SET_COOKIE, format!("{name}={value}; Path={path}; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"))
}

/// Cookie сессии: на HTTPS — `__Host-session` (Secure, Path=/, без Domain).
pub fn session_cookie(config: &Config, raw_id: &str, max_age: i64) -> (HeaderName, String) {
    set_cookie(config, config.session_cookie(), raw_id, max_age)
}

/// Язык интерфейса из cookie / Accept-Language (без настройки пользователя — для публичных страниц).
pub fn request_lang(headers: &HeaderMap) -> Option<Lang> {
    cookie(headers, LANG_COOKIE).and_then(|v| Lang::parse(&v)).or_else(|| {
        headers.get(ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()).and_then(Lang::from_accept_language)
    })
}

/// CSRF-токен сессии: HMAC от хеша её id. Хранить не нужно — проверяется пересчётом.
pub fn csrf_token(config: &Config, id_hash: &str) -> String {
    config.signer.sign("csrf", id_hash)
}

/// Учётные данные запроса: Bearer API-токен или cookie сессии.
pub fn credential(config: &Config, headers: &HeaderMap) -> Option<Credential> {
    let bearer = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(str::trim);
    if let Some(token) = bearer {
        return Some(Credential::ApiToken(token.to_string()));
    }
    session_credential(config, headers)
}

/// Только cookie-сессия — для страниц браузера вне `/api`: API-токен там не действует.
pub fn session_credential(config: &Config, headers: &HeaderMap) -> Option<Credential> {
    cookie(headers, config.session_cookie()).filter(|v| !v.is_empty()).map(Credential::Session)
}

fn wants_html(headers: &HeaderMap) -> bool {
    headers.get(ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("text/html"))
}

fn is_api(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

fn unauthorized(headers: &HeaderMap, path_and_query: &str) -> Response {
    if wants_html(headers) && !is_api(path_and_query) {
        return Redirect::to(&format!("/auth/login?next={}", crate::web::url_encode(path_and_query))).into_response();
    }
    AppError::Unauthorized("требуется вход: cookie сессии или Authorization: Bearer <API-токен>".into()).into_response()
}

fn safe_method(m: &Method) -> bool {
    matches!(*m, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Права API-токена: `read`/`write` на всё или `<фича>:read`/`<фича>:write`; запись включает чтение.
pub fn scope_allows(scopes: &[String], feature: &str, write: bool) -> bool {
    scopes.iter().any(|s| {
        let s = s.as_str();
        s == "write"
            || (!write && s == "read")
            || s.split_once(':').is_some_and(|(f, a)| f == feature && (a == "write" || (!write && a == "read")))
    })
}

/// Фича REST-запроса: `/api/transactions/…` → `transactions`.
pub fn api_feature(path: &str) -> &str {
    path.strip_prefix("/api/").map(|p| p.split('/').next().unwrap_or("")).unwrap_or("")
}

/// Значение поля `_csrf` из тела формы `application/x-www-form-urlencoded`.
fn form_field(body: &[u8], name: &str) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    text.split('&').filter_map(|kv| kv.split_once('=')).find(|(k, _)| *k == name).map(|(_, v)| percent_decode(v))
}

fn percent_decode(v: &str) -> String {
    let bytes = v.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `Origin` (или `Referer`) меняющего запроса должен быть нашим сайтом.
pub fn same_origin(config: &Config, headers: &HeaderMap) -> bool {
    let ours = config.origin();
    if let Some(origin) = headers.get(ORIGIN).and_then(|v| v.to_str().ok()) {
        return origin == ours;
    }
    match headers.get(REFERER).and_then(|v| v.to_str().ok()) {
        Some(r) => crate::config::origin_of(r) == ours,
        // Ни Origin, ни Referer (редкие клиенты) — решает CSRF-токен.
        None => true,
    }
}

/// CSRF-проверка меняющего запроса по cookie-сессии. Тело формы буферизуется (≤ 64 КБ) и
/// возвращается в запрос нетронутым.
async fn check_csrf(config: &Config, id_hash: &str, req: Request) -> Result<Request, Response> {
    let forbidden = |m: &str| AppError::Forbidden(m.into()).into_response();
    if !same_origin(config, req.headers()) {
        return Err(forbidden("CSRF: запрос с чужого сайта (Origin)"));
    }
    let expected = csrf_token(config, id_hash);
    if let Some(h) = req.headers().get(CSRF_HEADER).and_then(|v| v.to_str().ok()) {
        return if ct_eq(h, &expected) { Ok(req) } else { Err(forbidden("CSRF: неверный токен")) };
    }
    let is_form = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/x-www-form-urlencoded"));
    if !is_form {
        return Err(forbidden("CSRF: нет токена (заголовок X-CSRF-Token)"));
    }
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, MAX_BODY).await.map_err(|_| StatusCode::PAYLOAD_TOO_LARGE.into_response())?;
    match form_field(&bytes, CSRF_FIELD) {
        Some(token) if ct_eq(&token, &expected) => Ok(Request::from_parts(parts, Body::from(bytes))),
        _ => Err(forbidden("CSRF: неверный или отсутствующий токен формы")),
    }
}

/// Аутентификация + CSRF + лимит запросов. Общая часть обоих middleware.
async fn identify(auth: &AuthState, req: Request) -> Result<(Identity, Request), Response> {
    let headers = req.headers();
    let pq = req.uri().path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| "/".into());
    let Some(cred) = credential(&auth.config, headers) else {
        return Err(unauthorized(headers, &pq));
    };
    let identity = match auth.authenticator.authenticate(&cred).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            let mut resp = unauthorized(headers, &pq);
            if matches!(cred, Credential::Session(_)) {
                // Сессия истекла или отозвана — убираем мёртвую cookie.
                if let Ok(v) = session_cookie(&auth.config, "", 0).1.parse() {
                    resp.headers_mut().append(SET_COOKIE, v);
                }
            }
            return Err(resp);
        }
        Err(e) => return Err(e.into_response()),
    };
    if !auth.limiter.allow(&identity.throttle_key(), auth.config.rate_limit) {
        return Err(AppError::TooManyRequests("слишком много запросов, повторите через минуту".into()).into_response());
    }
    let req = match (&identity.via, safe_method(req.method())) {
        (Via::Session { id_hash }, false) => check_csrf(&auth.config, id_hash, req).await?,
        _ => req,
    };
    Ok((identity, req))
}

/// Middleware для страниц без проекта (онбординг, принять приглашение): только пользователь
/// браузера, CSRF и лимит. API-токен здесь не действует — он только для `/api`.
pub async fn require_identity(State(auth): State<AuthState>, req: Request, next: Next) -> Response {
    let (identity, mut req) = match identify(&auth, req).await {
        Ok(x) => x,
        Err(r) => return r,
    };
    let csrf = match &identity.via {
        Via::Session { id_hash } => csrf_token(&auth.config, id_hash),
        Via::Token { .. } => return AppError::Forbidden("API-токен действует только для /api".into()).into_response(),
    };
    req.extensions_mut().insert(CurrentUser { id: identity.user_id });
    req.extensions_mut().insert(Csrf(csrf));
    req.extensions_mut().insert(identity);
    next.run(req).await
}

/// Middleware: пользователь (сессия или API-токен) → активный проект → роль.
/// Пишущие запросы VIEWER получают 403; API-токен ограничен своим проектом и правами.
pub async fn require_user(State(auth): State<AuthState>, req: Request, next: Next) -> Response {
    let (identity, mut req) = match identify(&auth, req).await {
        Ok(x) => x,
        Err(r) => return r,
    };
    let headers = req.headers();
    let path = req.uri().path().to_string();
    let hint = match &identity.via {
        Via::Token { project_id, .. } => Some(*project_id),
        Via::Session { .. } => headers
            .get(PROJECT_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .or_else(|| cookie(headers, PROJECT_COOKIE))
            .and_then(|v| Uuid::parse_str(&v).ok()),
    };
    let info = match auth.memberships.session(identity.user_id, hint).await {
        Ok(Some(info)) => info,
        Ok(None) if wants_html(headers) && !is_api(&path) => return Redirect::to("/onboarding").into_response(),
        Ok(None) => return AppError::Forbidden("нет ни одного проекта: пройдите онбординг в браузере".into()).into_response(),
        Err(e) => return e.into_response(),
    };
    let (csrf, via_token) = match &identity.via {
        Via::Session { id_hash } => (csrf_token(&auth.config, id_hash), false),
        Via::Token { project_id, scopes, .. } => {
            // Токен выпущен на один проект: если пользователь из него вышел — доступа нет.
            if info.project_id != *project_id {
                return AppError::Forbidden("API-токен выпущен для другого проекта".into()).into_response();
            }
            let write = !safe_method(req.method());
            if is_api(&path) && !scope_allows(scopes, api_feature(&path), write) {
                return AppError::Forbidden(format!("у API-токена нет права {}", if write { "write" } else { "read" })).into_response();
            }
            if !is_api(&path) {
                return AppError::Forbidden("API-токен действует только для /api".into()).into_response();
            }
            (String::new(), true)
        }
    };
    let lang = cookie(headers, LANG_COOKIE).and_then(|v| Lang::parse(&v)).unwrap_or(info.user_lang);
    let session = Session {
        user_id: identity.user_id,
        user_name: info.user_name,
        lang,
        theme: info.theme,
        project_id: info.project_id,
        project_name: info.project_name,
        base_currency: info.base_currency,
        currency_marks: info.currency_marks,
        role: info.role,
        projects: info.projects,
        members: info.members,
        require_2fa: info.require_2fa,
        mfa: identity.mfa,
        csrf,
        via_token,
    };
    // Управление проектами, профиль и выход проверяют права сами (роль — в том проекте, которым управляют).
    let own_rules = ["/projects", "/api/projects", "/profile", "/api/profile", "/auth"].iter().any(|p| path.starts_with(p));
    let writes = !own_rules && !safe_method(req.method());
    if writes && !session.scope().can_write() {
        return AppError::Forbidden("роль VIEWER: только просмотр".into()).into_response();
    }
    req.extensions_mut().insert(CurrentUser { id: identity.user_id });
    req.extensions_mut().insert(Csrf(session.csrf.clone()));
    req.extensions_mut().insert(identity);
    req.extensions_mut().insert(session.scope());
    req.extensions_mut().insert(session);
    next.run(req).await
}

/// Адрес клиента из `ConnectInfo` (если сервер запущен с `into_make_service_with_connect_info`).
pub fn peer(req: &Request) -> Option<SocketAddr> {
    req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0)
}

/// Вошедший ли пользователь — для публичных страниц (лендинг перенаправляет вошедшего на «Обзор»).
#[derive(Debug, Clone, Copy, Default)]
pub struct MaybeUser(pub Option<Uuid>);

/// Middleware для публичных страниц: если cookie сессии действительна — кладёт [`MaybeUser`] с id.
/// Ничего не запрещает и не требует CSRF (страницы только читают).
pub async fn maybe_user(State(auth): State<AuthState>, mut req: Request, next: Next) -> Response {
    let user = match credential(&auth.config, req.headers()) {
        Some(c @ Credential::Session(_)) => auth.authenticator.authenticate(&c).await.ok().flatten().map(|i| i.user_id),
        _ => None,
    };
    req.extensions_mut().insert(MaybeUser(user));
    next.run(req).await
}

/// Middleware (внешний): кладёт в запрос [`RequestMeta`] — IP и браузер для журнала и устройств.
pub async fn attach_meta(State(config): State<Arc<Config>>, mut req: Request, next: Next) -> Response {
    let meta = RequestMeta::from_request(req.headers(), peer(&req), config.trust_proxy);
    req.extensions_mut().insert(meta);
    next.run(req).await
}

/// Id сессии из cookie и её CSRF-токен — для публичных форм (выход), где нет middleware.
pub fn session_csrf(config: &Config, headers: &HeaderMap) -> Option<(String, String)> {
    let raw = cookie(headers, config.session_cookie()).filter(|v| !v.is_empty())?;
    let hash = crate::crypto::sha256_hex(&raw);
    let csrf = csrf_token(config, &hash);
    Some((raw, csrf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_scopes() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(scope_allows(&s(&["read"]), "budget", false));
        assert!(!scope_allows(&s(&["read"]), "transactions", true));
        assert!(scope_allows(&s(&["transactions:write"]), "transactions", true));
        assert!(scope_allows(&s(&["transactions:write"]), "transactions", false), "запись включает чтение");
        assert!(!scope_allows(&s(&["transactions:write"]), "limits", false));
        assert!(scope_allows(&s(&["write"]), "limits", true));
        assert_eq!(api_feature("/api/transactions/quick-defaults"), "transactions");
    }

    #[test]
    fn csrf_form_field() {
        assert_eq!(form_field(b"a=1&_csrf=abc-_9&b=2", CSRF_FIELD).as_deref(), Some("abc-_9"));
        assert_eq!(form_field(b"a=1", CSRF_FIELD), None);
        assert_eq!(percent_decode("a%2Fb+c"), "a/b c");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn origin_check() {
        let c = Config::test();
        let mut h = HeaderMap::new();
        assert!(same_origin(&c, &h), "без Origin и Referer решает токен");
        h.insert(ORIGIN, "http://localhost:8080".parse().unwrap());
        assert!(same_origin(&c, &h));
        h.insert(ORIGIN, "https://evil.example".parse().unwrap());
        assert!(!same_origin(&c, &h));
        let mut r = HeaderMap::new();
        r.insert(REFERER, "https://evil.example/x".parse().unwrap());
        assert!(!same_origin(&c, &r));
    }

    /// Пользователь с API-токеном (`Bearer fbt_ok`) или сессией (`fb_session=ok`).
    struct FakeAuth;

    #[async_trait]
    impl Authenticator for FakeAuth {
        async fn authenticate(&self, credential: &Credential) -> AppResult<Option<Identity>> {
            let via = match credential {
                Credential::ApiToken(t) if t == "fbt_ok" => Via::Token { token_id: Uuid::nil(), project_id: Uuid::nil(), scopes: vec!["write".into()] },
                Credential::Session(s) if s == "ok" => Via::Session { id_hash: "h".into() },
                _ => return Ok(None),
            };
            Ok(Some(Identity { user_id: Uuid::nil(), mfa: false, via }))
        }
    }

    struct NoProjects;

    #[async_trait]
    impl Memberships for NoProjects {
        async fn session(&self, _user_id: Uuid, _project: Option<Uuid>) -> AppResult<Option<SessionInfo>> {
            Ok(None)
        }
    }

    fn auth_state() -> AuthState {
        AuthState { config: Arc::new(Config::test()), authenticator: Arc::new(FakeAuth), memberships: Arc::new(NoProjects), limiter: Arc::new(RateLimiter::new()) }
    }

    /// Б5: онбординг и «Принять приглашение» — только из браузера; API-токен действует лишь в `/api`.
    #[tokio::test]
    async fn api_token_is_rejected_outside_api() {
        use axum::routing::get;
        use tower::ServiceExt;
        let app = axum::Router::new()
            .route("/onboarding", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(auth_state(), require_identity));
        let call = |header: (&'static str, &'static str)| {
            let app = app.clone();
            async move { app.oneshot(Request::builder().uri("/onboarding").header(header.0, header.1).body(Body::empty()).unwrap()).await.unwrap().status() }
        };
        assert_eq!(call(("authorization", "Bearer fbt_ok")).await, StatusCode::FORBIDDEN);
        assert_eq!(call(("cookie", "fb_session=ok")).await, StatusCode::OK, "сессия браузера проходит");
        let h: HeaderMap = [(AUTHORIZATION, "Bearer fbt_ok".parse().unwrap())].into_iter().collect();
        assert!(session_credential(&Config::test(), &h).is_none(), "страницы вне /api не берут токен из заголовка");
    }

    #[test]
    fn currency_symbols() {
        let p = |c: &str, s: &str| (c.to_string(), s.to_string());
        let m = currency_marks([p("THB", "฿"), p("RUB", "₽"), p("USD", "$"), p("CAD", "$"), p("XXX", " ")]);
        assert_eq!(m.get("THB").map(String::as_str), Some("฿"));
        assert_eq!(m.get("RUB").map(String::as_str), Some("₽"));
        assert!(!m.contains_key("USD") && !m.contains_key("CAD"), "общий символ — остаются коды");
        assert!(!m.contains_key("XXX"), "пустой символ — код");
    }
}
