//! Веб-интерфейс: общий контекст страницы, меню, рендер Askama-шаблонов, тосты, статика,
//! страницы ошибок и заголовки безопасности.
//!
//! Каждая фича кладёт свои страницы в `adapters/web.rs` + `templates/` и наследует `base.html`
//! из `crates/kernel/templates` (askama.toml фичи: `dirs = ["templates", "../../kernel/templates"]`).
//! Каркас (боковое меню, нижняя панель телефона, диалог быстрого расхода, тосты) живёт в ядре.
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use askama::Template;
use axum::extract::{Path, Request, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, SET_COOKIE};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth::{Csrf, Session, cookie, request_lang};
use crate::config::Config;
use crate::error::AppError;
use crate::i18n::{Dict, Lang, Tr};

// ---------------------------------------------------------------- меню

/// Группа бокового меню.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavGroup {
    /// Основные разделы: обзор, операции, календарь, лимиты.
    Main,
    /// «Справочники»: категории, счета, валюты, курсы.
    Directory,
}

/// Пункт главного меню, который приносит фича.
#[derive(Debug, Clone)]
pub struct NavItem {
    pub feature: &'static str,
    pub href: &'static str,
    pub label: Tr,
    /// Иконка Lucide (имя символа в спрайте `icons.svg`).
    pub icon: &'static str,
    pub group: NavGroup,
    /// Порядок в группе (меньше — выше).
    pub order: i32,
    /// Есть на нижней панели телефона (не больше трёх; четвёртый пункт — «Ещё»).
    pub mobile: bool,
}

/// Веб-часть фичи: страницы (под авторизацией, монтируются в `Feature::mount`),
/// публичные страницы (без авторизации, от корня сайта), страницы аккаунта (вход есть, проекта
/// может не быть — онбординг) и пункт меню.
pub struct Web {
    pub pages: axum::Router,
    /// Страницы под авторизацией от корня сайта (например, `/profile` у фичи `projects`).
    pub root: Option<axum::Router>,
    pub public: Option<axum::Router>,
    pub account: Option<axum::Router>,
    pub nav: Option<NavItem>,
}

/// Меню всех включённых фич — собирается при старте.
#[derive(Debug, Clone, Default)]
pub struct Nav(pub Arc<Vec<NavItem>>);

/// Состояние веб-слоя: меню и настройки.
#[derive(Clone)]
pub struct WebState {
    pub nav: Nav,
    pub config: Arc<Config>,
}

// ---------------------------------------------------------------- тосты (flash)

pub const FLASH_COOKIE: &str = "fb_flash";

/// Сообщение после действия: показывается тостом 5 секунд; `undo` — POST-адрес «Отменить».
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Flash {
    #[serde(rename = "t")]
    pub text: String,
    #[serde(rename = "u", default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
    /// `ok` / `error`.
    #[serde(rename = "k", default)]
    pub kind: String,
}

impl Flash {
    pub fn ok(text: impl Into<String>) -> Self {
        Self { text: text.into(), undo: None, kind: "ok".into() }
    }

    pub fn with_undo(text: impl Into<String>, undo: impl Into<String>) -> Self {
        Self { text: text.into(), undo: Some(undo.into()), kind: "ok".into() }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self { text: text.into(), undo: None, kind: "error".into() }
    }
}

/// Set-Cookie с тостом (подписан: подделать «Отменить» на чужой адрес нельзя).
pub fn flash_cookie(config: &Config, flash: &Flash) -> (HeaderName, String) {
    let json = serde_json::to_string(flash).unwrap_or_default();
    crate::auth::set_cookie(config, FLASH_COOKIE, &config.signer.seal("flash", &json), 60)
}

fn read_flash(config: &Config, headers: &HeaderMap) -> Option<Flash> {
    let raw = cookie(headers, FLASH_COOKIE)?;
    let json = config.signer.open("flash", &raw)?;
    let f: Flash = serde_json::from_str(&json).ok()?;
    // «Отменить» — только относительный адрес нашего сайта.
    let undo_ok = f.undo.as_deref().is_none_or(|u| u.starts_with('/') && !u.starts_with("//"));
    undo_ok.then_some(f)
}

// ---------------------------------------------------------------- контекст страницы

/// Контекст любой страницы: язык, пользователь, проект, меню, словарь фичи.
#[derive(Clone)]
pub struct PageCtx {
    pub lang: Lang,
    pub session: Arc<Session>,
    pub nav: Nav,
    pub active: &'static str,
    pub path: String,
    pub flash: Option<Flash>,
    pub config: Arc<Config>,
    /// Хлебные крошки вложенных страниц: (подпись, адрес).
    pub crumbs: Vec<(String, String)>,
    dict: Option<Dict>,
}

impl PageCtx {
    /// Привязать контекст к фиче: её пункт меню подсвечен, её словарь используется первым.
    pub fn for_feature(mut self, feature: &'static str, dict: Dict) -> Self {
        self.active = feature;
        self.dict = Some(dict);
        self
    }

    /// Добавить хлебную крошку (родительская страница).
    pub fn crumb(mut self, label: impl Into<String>, href: impl Into<String>) -> Self {
        self.crumbs.push((label.into(), href.into()));
        self
    }

    /// Строка интерфейса на языке страницы.
    pub fn t(&self, key: &str) -> String {
        crate::i18n::t(self.lang, self.dict, key)
    }

    pub fn can_write(&self) -> bool {
        self.session.scope().can_write()
    }

    pub fn csrf(&self) -> &str {
        &self.session.csrf
    }

    pub fn nav_items(&self) -> &[NavItem] {
        &self.nav.0
    }

    pub fn nav_main(&self) -> Vec<&NavItem> {
        self.nav.0.iter().filter(|n| n.group == NavGroup::Main).collect()
    }

    pub fn nav_directory(&self) -> Vec<&NavItem> {
        self.nav.0.iter().filter(|n| n.group == NavGroup::Directory).collect()
    }

    /// Пункты нижней панели телефона (до «+» — первые два, после — остальные).
    pub fn nav_mobile(&self) -> Vec<&NavItem> {
        self.nav.0.iter().filter(|n| n.mobile).take(3).collect()
    }

    /// Пункты, которые на телефоне спрятаны в «Ещё».
    pub fn nav_more(&self) -> Vec<&NavItem> {
        let shown: Vec<&str> = self.nav_mobile().iter().map(|n| n.feature).collect();
        self.nav.0.iter().filter(|n| !shown.contains(&n.feature)).collect()
    }

    pub fn nav_label(&self, item: &NavItem) -> &'static str {
        self.lang.pick(item.label)
    }

    pub fn is_active(&self, item: &NavItem) -> bool {
        item.feature == self.active
    }

    pub fn langs(&self) -> [Lang; 3] {
        Lang::ALL
    }

    /// Текущий путь, закодированный для параметра `back=`.
    pub fn back(&self) -> String {
        url_encode(&self.path)
    }

    pub fn is_lang(&self, lang: &Lang) -> bool {
        self.lang == *lang
    }

    pub fn other_projects(&self) -> Vec<&crate::auth::ProjectRef> {
        self.session.projects.iter().filter(|p| p.id != self.session.project_id).collect()
    }

    /// Адрес статического файла с хешем в имени.
    pub fn asset(&self, name: &str) -> String {
        asset_url(name)
    }

    /// Ссылка на иконку в спрайте.
    pub fn icon(&self, name: &str) -> String {
        icon_href(name)
    }

    /// Значение `data-theme` у `<html>`.
    pub fn theme(&self) -> &'static str {
        self.session.theme.attr()
    }

    pub fn money(&self, value: impl AsDecimal) -> String {
        money_in(self.lang, value)
    }

    /// Валюта рядом с суммой: символ (`฿`), иначе код.
    pub fn cur<'a>(&'a self, code: &'a str) -> &'a str {
        self.session.cur(code)
    }

    pub fn date(&self, d: &NaiveDate) -> String {
        date_in(self.lang, *d)
    }

    /// Первая буква названия проекта — для свёрнутого меню.
    pub fn project_initial(&self) -> String {
        self.session.project_name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
    }

    /// Инициалы пользователя для аватара.
    pub fn initials(&self) -> String {
        initials(&self.session.user_name)
    }
}

/// Middleware после auth: кладёт `PageCtx` в запрос, снимает показанный тост, закрывает данные
/// проекта, требующего 2FA, если вход был без второго фактора.
pub async fn attach_page(State(state): State<WebState>, mut req: Request, next: Next) -> Response {
    let Some(session) = req.extensions().get::<Session>().cloned() else {
        return next.run(req).await;
    };
    let path = req.uri().path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| "/".into());
    let flash = read_flash(&state.config, req.headers());
    let page = PageCtx {
        lang: session.lang,
        session: Arc::new(session),
        nav: state.nav.clone(),
        active: "",
        path: path.clone(),
        flash: flash.clone(),
        config: state.config.clone(),
        crumbs: Vec::new(),
        dict: None,
    };
    // Проект требует 2FA: без второго фактора вместо данных — просьба включить 2FA.
    // Управление проектами, профиль и выход остаются доступны — чтобы переключиться или включить 2FA.
    let exempt = ["/projects", "/api/projects", "/profile", "/auth", "/onboarding"].iter().any(|p| path.starts_with(p));
    if page.session.mfa_blocked() && !exempt {
        if path.starts_with("/api") {
            return AppError::Forbidden(
                "проект требует второй фактор (2FA): войдите с кодом из письма или passkey".into(),
            )
            .into_response();
        }
        return match render(&MfaRequiredPage { page }) {
            Ok(mut r) => {
                *r.status_mut() = StatusCode::FORBIDDEN;
                r
            }
            Err(e) => e.into_response(),
        };
    }
    req.extensions_mut().insert(page);
    let mut resp = next.run(req).await;
    // Тост показан на этой странице — снять cookie (если ответ не ставит новый тост).
    let sets_flash =
        resp.headers().get_all(SET_COOKIE).iter().any(|v| v.to_str().is_ok_and(|s| s.starts_with(FLASH_COOKIE)));
    let is_page =
        resp.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("text/html"));
    if flash.is_some()
        && !sets_flash
        && is_page
        && let Ok(v) = crate::auth::set_cookie(&state.config, FLASH_COOKIE, "", 0).1.parse()
    {
        resp.headers_mut().append(SET_COOKIE, v);
    }
    resp
}

#[derive(Template)]
#[template(path = "mfa_required.html")]
struct MfaRequiredPage {
    page: PageCtx,
}

/// Отрендерить шаблон в ответ.
pub fn render<T: Template>(t: &T) -> Result<Response, PageError> {
    let html = t.render().map_err(|e| PageError(AppError::Internal(anyhow::anyhow!("шаблон: {e}"))))?;
    Ok(Html(html).into_response())
}

/// Редирект после POST (303 See Other).
pub fn see_other(to: &str) -> Response {
    Redirect::to(to).into_response()
}

/// Редирект после POST с тостом.
pub fn see_other_flash(config: &Config, to: &str, flash: Flash) -> Response {
    ([flash_cookie(config, &flash)], Redirect::to(to)).into_response()
}

/// Ответ htmx-запросу: перейти на адрес (полная навигация) или обновить текущую страницу.
pub fn is_htmx(headers: &HeaderMap) -> bool {
    headers.contains_key("hx-request")
}

// ---------------------------------------------------------------- ошибки

/// Ошибка страницы: та же `AppError`, но ответ — HTML-страница ошибки на языке пользователя.
#[derive(Debug)]
pub struct PageError(pub AppError);

impl From<AppError> for PageError {
    fn from(e: AppError) -> Self {
        PageError(e)
    }
}

pub type PageResult = Result<Response, PageError>;

/// Метка ответа-ошибки: `error_pages` превратит его в HTML-страницу на языке запроса.
#[derive(Debug, Clone)]
pub struct ErrorInfo {
    pub message: String,
}

impl IntoResponse for PageError {
    fn into_response(self) -> Response {
        let status = self.0.status();
        let message = self.0.public_message();
        let mut resp = (status, message.clone()).into_response();
        resp.extensions_mut().insert(ErrorInfo { message });
        resp
    }
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorPage {
    lang: Lang,
    status: u16,
    message: String,
    signed_in: bool,
}

impl ErrorPage {
    fn t(&self, key: &str) -> String {
        crate::i18n::t(self.lang, None, key)
    }

    fn asset(&self, name: &str) -> String {
        asset_url(name)
    }

    fn icon(&self, name: &str) -> String {
        icon_href(name)
    }

    fn headline(&self) -> String {
        match self.status {
            404 => self.t("error.404"),
            403 => self.t("error.403"),
            429 => self.t("error.429"),
            503 => self.t("error.503"),
            s if s >= 500 => self.t("error.500"),
            _ => self.t("error.title"),
        }
    }

    fn hint(&self) -> String {
        match self.status {
            404 => self.t("error.404_hint"),
            403 => self.t("error.403_hint"),
            429 => self.t("error.429_hint"),
            503 => self.t("error.503_hint"),
            s if s >= 500 => self.t("error.500_hint"),
            _ => self.t("error.check_hint"),
        }
    }

    fn show_message(&self) -> bool {
        self.status < 500 && self.status != 404 && !self.message.is_empty()
    }
}

/// HTML-страница ошибки.
pub fn error_page(lang: Lang, status: StatusCode, message: &str, signed_in: bool) -> Response {
    let page = ErrorPage { lang, status: status.as_u16(), message: message.to_string(), signed_in };
    match page.render() {
        Ok(html) => (status, Html(html)).into_response(),
        Err(_) => (status, message.to_string()).into_response(),
    }
}

/// Middleware (внешний): ответы-ошибки страниц (и 404 неизвестных адресов) — свои HTML-страницы
/// на языке пользователя. REST (`/api/…`) отвечает JSON, как раньше.
pub async fn error_pages(State(config): State<Arc<Config>>, req: Request, next: Next) -> Response {
    let headers = req.headers().clone();
    let path = req.uri().path().to_string();
    let resp = next.run(req).await;
    let status = resp.status();
    if !(status.is_client_error() || status.is_server_error())
        || path.starts_with("/api")
        || path.starts_with("/static")
    {
        return resp;
    }
    let info = resp.extensions().get::<ErrorInfo>().cloned();
    let json_error = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if info.is_none() {
        // Ошибки из middleware (JSON) и 404/405 роутера оформляем только для браузера.
        let wants_html = headers
            .get(axum::http::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/html"));
        let routing = status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED;
        if !wants_html || !(json_error || routing) {
            return resp;
        }
    }
    let message = match info {
        Some(i) => i.message,
        None if json_error => {
            let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap_or_default();
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
                .unwrap_or_default()
        }
        None => String::new(),
    };
    let lang = request_lang(&headers).unwrap_or_default();
    let signed_in = cookie(&headers, config.session_cookie()).is_some_and(|v| !v.is_empty());
    error_page(lang, status, &message, signed_in)
}

/// Неизвестный адрес → 404 (страницу оформит `error_pages`).
pub async fn not_found() -> Response {
    PageError(AppError::NotFound(String::new())).into_response()
}

// ---------------------------------------------------------------- статика

/// Встроенный в бинарник статический файл.
pub struct StaticAsset {
    pub name: &'static str,
    pub bytes: &'static [u8],
    pub mime: &'static str,
}

struct AssetEntry {
    hashed: String,
    bytes: &'static [u8],
    mime: &'static str,
}

/// По имени (`app.css`) и по имени с хешем (`app.3f2a9c1b0d.css` → `app.css`).
type AssetIndex = (HashMap<&'static str, AssetEntry>, HashMap<String, &'static str>);

static ASSETS: OnceLock<AssetIndex> = OnceLock::new();

/// `app.css` + хеш содержимого → `app.3f2a9c1b0d.css`.
fn hashed_name(name: &str, bytes: &[u8]) -> String {
    let h = hex::encode(&Sha256::digest(bytes)[..5]);
    match name.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}.{h}.{ext}"),
        None => format!("{name}.{h}"),
    }
}

/// Зарегистрировать статику при старте (один раз).
pub fn register_assets(list: Vec<StaticAsset>) {
    let mut by_name = HashMap::new();
    let mut by_hashed = HashMap::new();
    for a in list {
        let hashed = hashed_name(a.name, a.bytes);
        by_hashed.insert(hashed.clone(), a.name);
        by_name.insert(a.name, AssetEntry { hashed, bytes: a.bytes, mime: a.mime });
    }
    let _ = ASSETS.set((by_name, by_hashed));
}

/// Адрес файла с хешем в имени: `/static/app.3f2a9c1b0d.css` (кэш на год).
pub fn asset_url(name: &str) -> String {
    match ASSETS.get().and_then(|(m, _)| m.get(name)) {
        Some(a) => format!("/static/{}", a.hashed),
        None => format!("/static/{name}"),
    }
}

/// Ссылка на символ иконки Lucide в спрайте.
pub fn icon_href(name: &str) -> String {
    format!("{}#i-{name}", asset_url("icons.svg"))
}

/// `GET /static/{file}`: по имени с хешем — кэш на год (immutable), по обычному имени — на день.
pub async fn static_file(Path(file): Path<String>) -> Response {
    let Some((by_name, by_hashed)) = ASSETS.get() else { return StatusCode::NOT_FOUND.into_response() };
    let (entry, immutable) = match by_hashed.get(&file) {
        Some(name) => (by_name.get(name), true),
        None => (by_name.get(file.as_str()), false),
    };
    let Some(a) = entry else { return StatusCode::NOT_FOUND.into_response() };
    let cache = if immutable { "public, max-age=31536000, immutable" } else { "public, max-age=86400" };
    ([(CONTENT_TYPE, a.mime), (CACHE_CONTROL, cache)], a.bytes).into_response()
}

// ---------------------------------------------------------------- заголовки безопасности

/// CSP без `unsafe-inline` и `unsafe-eval`: скрипты и стили — только файлы со своего домена.
pub fn content_security_policy(config: &Config) -> String {
    let rauthy = config.oidc.as_ref().map(|o| format!(" {}", o.origin())).unwrap_or_default();
    let upgrade = if config.secure_cookies() { "; upgrade-insecure-requests" } else { "" };
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; \
         connect-src 'self'; manifest-src 'self'; worker-src 'self'; object-src 'none'; base-uri 'self'; \
         frame-ancestors 'none'; form-action 'self'{rauthy}{upgrade}"
    )
}

/// Middleware (внешний): CSP, HSTS, nosniff, Referrer-Policy, Permissions-Policy, запрет фреймов.
pub async fn security_headers(State(config): State<Arc<Config>>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    let set = |h: &mut HeaderMap, k: &'static str, v: &str| {
        if let Ok(v) = HeaderValue::from_str(v) {
            h.insert(HeaderName::from_static(k), v);
        }
    };
    // Swagger UI (только в разработке) живёт на встроенных скриптах — ему CSP мягче.
    if path.starts_with("/docs") && config.swagger {
        set(
            h,
            "content-security-policy",
            "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; frame-ancestors 'none'",
        );
    } else {
        set(h, "content-security-policy", &content_security_policy(&config));
    }
    if config.secure_cookies() {
        set(h, "strict-transport-security", "max-age=31536000; includeSubDomains");
    }
    set(h, "x-content-type-options", "nosniff");
    set(h, "x-frame-options", "DENY");
    set(h, "referrer-policy", "strict-origin-when-cross-origin");
    set(h, "permissions-policy", "camera=(), microphone=(), geolocation=(), payment=(), usb=()");
    set(h, "cross-origin-opener-policy", "same-origin");
    resp
}

/// Middleware для страниц приложения: закрыть от индексации (лендинг и правовые страницы — открыты).
pub async fn no_index(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(HeaderName::from_static("x-robots-tag"), HeaderValue::from_static("noindex, nofollow"));
    if !resp.headers().contains_key(CACHE_CONTROL) {
        resp.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    resp
}

// ---------------------------------------------------------------- форматирование

/// Инициалы для аватара: «Anna Petrova» → «AP».
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// Экранирование для текста, собранного вне шаблона.
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Процентное кодирование для параметров URL (`/` остаётся: `?back=/accounts`).
pub fn url_encode(s: &str) -> String {
    percent(s, true)
}

/// Сегмент пути (`/accounts/{code}/edit`): как [`url_encode`], но `/` тоже кодируется — код `FOOD/CAFE`,
/// созданный до строгих правил (техдолг #7), открывается.
pub fn url_segment(s: &str) -> String {
    percent(s, false)
}

fn percent(s: &str, keep_slash: bool) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b'/' if keep_slash => out.push('/'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Деньги для показа: `12 345.50` (узкие неразрывные пробелы между тысячами), всегда 2 знака.
pub fn money(value: impl AsDecimal) -> String {
    format_money(value.decimal(), '\u{202F}', '.')
}

/// Деньги в формате языка: ru — `12 345,50`, en и th — `12,345.50`.
pub fn money_in(lang: impl Into<Lang>, value: impl AsDecimal) -> String {
    match lang.into() {
        Lang::Ru => format_money(value.decimal(), '\u{202F}', ','),
        Lang::En | Lang::Th => format_money(value.decimal(), ',', '.'),
    }
}

fn format_money(v: Decimal, group: char, point: char) -> String {
    let v = v.round_dp(2);
    let neg = v.is_sign_negative() && !v.is_zero();
    let s = format!("{:.2}", v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, "00"));
    let mut out = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(group);
        }
        out.push(ch);
    }
    format!("{}{}{}{}", if neg { "−" } else { "" }, out, point, frac)
}

/// Дата в формате языка (григорианский календарь): ru `13.10.2026`, en `13 Oct 2026`, th `13/10/2026`.
pub fn date_in(lang: impl Into<Lang>, d: NaiveDate) -> String {
    match lang.into() {
        Lang::Ru => format!("{:02}.{:02}.{}", d.day(), d.month(), d.year()),
        Lang::En => format!("{} {} {}", d.day(), &crate::i18n::month_name(Lang::En, d.month())[..3], d.year()),
        Lang::Th => format!("{:02}/{:02}/{}", d.day(), d.month(), d.year()),
    }
}

/// Decimal по значению или по ссылке любой глубины (Askama передаёт поля шаблона по ссылке).
pub trait AsDecimal {
    fn decimal(&self) -> Decimal;
}

impl AsDecimal for Decimal {
    fn decimal(&self) -> Decimal {
        *self
    }
}

impl<T: AsDecimal + ?Sized> AsDecimal for &T {
    fn decimal(&self) -> Decimal {
        (**self).decimal()
    }
}

/// Разбор суммы из формы: пробелы, запятая или точка как разделитель дроби и простая арифметика
/// `120+45`, `3*40`, `500-120` (как в быстром расходе — работает и без JS).
pub fn parse_money(field: &str, value: &str) -> Result<Decimal, String> {
    Ok(parse_number(field, value)?.round_dp(4))
}

/// [`parse_money`] без округления до 4 знаков — для количества и цены (до 6 знаков, проверяет домен).
pub fn parse_number(field: &str, value: &str) -> Result<Decimal, String> {
    let bad = || {
        crate::error::Problem::validation(field, crate::i18n::tr(None, "err.not_a_number", &[("value", &value)]))
            .to_string()
    };
    // Переполнение `Decimal` — ошибка ввода, а не паника (техдолг #20).
    let too_big = || crate::error::Problem::validation(field, crate::i18n::tr(None, "err.too_big", &[])).to_string();
    let cleaned: String = value
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{202F}' && *c != '\u{00A0}')
        .collect::<String>()
        .replace(',', ".");
    if cleaned.is_empty() {
        return Err(
            crate::error::Problem::validation(field, crate::i18n::tr(None, "err.enter_amount", &[])).to_string()
        );
    }
    // Сумма слагаемых, каждое — произведение/частное множителей.
    let mut total = Decimal::ZERO;
    let mut term = String::new();
    let mut sign = Decimal::ONE;
    let flush = |term: &str, sign: Decimal| -> Result<Decimal, String> {
        let mut acc: Option<Decimal> = None;
        let mut op = '*';
        let mut num = String::new();
        for ch in term.chars().chain(std::iter::once('*')) {
            if ch == '*' || ch == '/' {
                let n: Decimal = num.parse().map_err(|_| bad())?;
                acc = Some(match (acc, op) {
                    (None, _) => n,
                    (Some(a), '*') => a.checked_mul(n).ok_or_else(too_big)?,
                    (Some(a), _) if !n.is_zero() => a.checked_div(n).ok_or_else(too_big)?,
                    _ => return Err(bad()),
                });
                op = ch;
                num.clear();
            } else {
                num.push(ch);
            }
        }
        Ok(sign * acc.ok_or_else(bad)?)
    };
    for (i, ch) in cleaned.chars().enumerate() {
        if (ch == '+' || ch == '-') && i > 0 && !term.ends_with(['*', '/']) {
            total = total.checked_add(flush(&term, sign)?).ok_or_else(too_big)?;
            term.clear();
            sign = if ch == '-' { -Decimal::ONE } else { Decimal::ONE };
        } else {
            term.push(ch);
        }
    }
    total = total.checked_add(flush(&term, sign)?).ok_or_else(too_big)?;
    Ok(total)
}

/// Пункт выпадающего списка: запись справочника с названием на языке интерфейса.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub code: String,
    pub label: String,
    /// Уровень вложенности (для дерева категорий), 0 — корень.
    pub depth: usize,
    /// Доп. признак записи (тип категории, валюта счёта) — для фильтрации на клиенте.
    pub tag: String,
}

impl Choice {
    /// Отступ для показа дерева в `<select>`.
    pub fn indent(&self) -> String {
        "\u{00A0}\u{00A0}\u{00A0}".repeat(self.depth)
    }
}

/// Необязательное поле формы: пустая строка → `None`.
pub fn opt(value: &Option<String>) -> Option<String> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

/// Ошибка формы, привязанная к полю: сообщение валидации `поле: текст` → (поле, текст).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormError {
    pub field: String,
    pub message: String,
}

impl FormError {
    pub fn parse(message: &str) -> Self {
        let field_like = |f: &str| {
            !f.is_empty() && f.len() <= 40 && f.chars().all(|c| c.is_ascii_alphanumeric() || "_[].".contains(c))
        };
        match message.split_once(": ") {
            Some((f, m)) if field_like(f) => {
                // lines[2].amount → line_amount: имя поля формы позиций.
                let field = match f.split_once("].") {
                    Some((_, rest)) if f.starts_with("lines[") => format!("line_{rest}"),
                    _ => f.to_string(),
                };
                FormError { field, message: m.to_string() }
            }
            _ => FormError { field: String::new(), message: message.to_string() },
        }
    }

    /// Ошибка относится к полю `name`.
    pub fn is(&self, name: &str) -> bool {
        self.field == name
    }
}

/// Для шаблонов: ошибка поля `name` (если есть).
pub fn field_error(error: &Option<String>, name: &str) -> Option<String> {
    error.as_deref().map(FormError::parse).filter(|e| e.is(name)).map(|e| e.message)
}

/// Для шаблонов: ошибка не привязана ни к одному полю формы из `fields` (через запятую) —
/// показать общим блоком над формой.
pub fn general_error(error: &Option<String>, fields: &str) -> Option<String> {
    let e = FormError::parse(error.as_deref()?);
    if fields.split(',').map(str::trim).filter(|f| !f.is_empty()).any(|f| e.is(f)) {
        None
    } else {
        Some(error.clone().unwrap_or_default())
    }
}

/// CSRF-токен запроса для страниц без `PageCtx` (онбординг).
pub fn csrf_of(req_csrf: &Option<Csrf>) -> String {
    req_csrf.as_ref().map(|c| c.0.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn money_format() {
        assert_eq!(money(d("1234567.5")), "1\u{202F}234\u{202F}567.50");
        assert_eq!(money(d("-12")), "−12.00");
        assert_eq!(money(Decimal::ZERO), "0.00");
        assert_eq!(money_in(Lang::Ru, d("1250.5")), "1\u{202F}250,50");
        assert_eq!(money_in(Lang::En, d("1250.5")), "1,250.50");
        assert_eq!(money_in(Lang::Th, d("-999")), "−999.00");
    }

    #[test]
    fn money_parse_and_arithmetic() {
        assert_eq!(parse_money("a", "1 250,5").unwrap(), d("1250.5"));
        assert_eq!(parse_money("a", "120+45").unwrap(), d("165"));
        assert_eq!(parse_money("a", "500-120,5").unwrap(), d("379.5"));
        assert_eq!(parse_money("a", "3*40+10").unwrap(), d("130"));
        assert_eq!(parse_money("a", "100/4").unwrap(), d("25"));
        assert!(parse_money("a", "abc").is_err());
        assert!(parse_money("a", "1/0").is_err());
        assert!(parse_money("a", "").is_err());
        assert!(parse_money("a", "5+").is_err());
        // Техдолг #20: переполнение — ошибка поля, а не паника сервера.
        assert_eq!(parse_money("a", "79228162514264337593543950335*10").unwrap_err(), "a: слишком большое число");
        assert!(parse_money("a", "79228162514264337593543950335+79228162514264337593543950335").is_err());
        assert_eq!(parse_number("q", "0,123456").unwrap(), d("0.123456"), "количество — до 6 знаков, без округления");
        assert_eq!(parse_money("a", "0,123456").unwrap(), d("0.1235"));
    }

    #[test]
    fn dates() {
        let x = NaiveDate::from_ymd_opt(2026, 10, 3).unwrap();
        assert_eq!(date_in(Lang::Ru, x), "03.10.2026");
        assert_eq!(date_in(Lang::En, x), "3 Oct 2026");
        assert_eq!(date_in(Lang::Th, x), "03/10/2026");
    }

    #[test]
    fn form_errors_bind_to_fields() {
        let e = FormError::parse("account_code: запись 'X' не найдена");
        assert_eq!((e.field.as_str(), e.message.as_str()), ("account_code", "запись 'X' не найдена"));
        assert_eq!(FormError::parse("lines[2].amount: сумма должна быть больше нуля").field, "line_amount");
        assert_eq!(FormError::parse("это вхождение плана уже оплачено").field, "");
        let err = Some("name: не может быть пустым".to_string());
        assert_eq!(field_error(&err, "name").as_deref(), Some("не может быть пустым"));
        assert_eq!(general_error(&err, "code, name"), None);
        assert!(general_error(&err, "code").is_some());
    }

    #[test]
    fn hashed_asset_names() {
        assert!(hashed_name("app.css", b"x").starts_with("app.") && hashed_name("app.css", b"x").ends_with(".css"));
        assert_ne!(hashed_name("app.css", b"x"), hashed_name("app.css", b"y"));
        assert!(content_security_policy(&Config::test()).contains("frame-ancestors 'none'"));
        assert!(!content_security_policy(&Config::test()).contains("unsafe-inline"));
    }

    #[test]
    fn unavailable_page_is_friendly_and_hides_details() {
        let status = PageError(AppError::Unavailable(anyhow::anyhow!("Rauthy недоступен: connection refused")))
            .into_response()
            .status();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        for lang in Lang::ALL {
            let page = ErrorPage {
                lang,
                status: status.as_u16(),
                message: "Rauthy недоступен: connection refused".into(),
                signed_in: false,
            };
            let html = page.render().unwrap();
            assert!(html.contains(&crate::i18n::t(lang, None, "error.503")), "{lang:?}");
            assert!(!html.contains(&crate::i18n::t(lang, None, "error.500")), "{lang:?}: не «что-то пошло не так»");
            assert!(!html.contains("connection refused"), "{lang:?}: внутренний текст ошибки не показываем");
        }
    }
}
