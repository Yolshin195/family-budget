//! Веб-интерфейс: общий контекст страницы, меню, рендер Askama-шаблонов, ошибки в HTML.
//!
//! Каждая фича кладёт свои страницы в `adapters/web.rs` + `templates/` и наследует `base.html`
//! из `crates/kernel/templates` (askama.toml фичи: `dirs = ["templates", "../../kernel/templates"]`).
use std::sync::Arc;

use askama::Template;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use rust_decimal::Decimal;

use crate::auth::Session;
use crate::error::AppError;
use crate::i18n::{Dict, Lang, Tr};

/// Пункт главного меню, который приносит фича.
#[derive(Debug, Clone)]
pub struct NavItem {
    pub feature: &'static str,
    pub href: &'static str,
    pub label: Tr,
    /// Порядок в меню (меньше — левее).
    pub order: i32,
}

/// Веб-часть фичи: страницы (под авторизацией, монтируются в `Feature::mount`),
/// публичные страницы (без авторизации, от корня сайта) и пункт меню.
pub struct Web {
    pub pages: axum::Router,
    pub public: Option<axum::Router>,
    pub nav: Option<NavItem>,
}

/// Меню всех включённых фич — собирается при старте.
#[derive(Debug, Clone, Default)]
pub struct Nav(pub Arc<Vec<NavItem>>);

/// Контекст любой страницы: язык, пользователь, проект, меню, словарь фичи.
#[derive(Clone)]
pub struct PageCtx {
    pub lang: Lang,
    pub session: Arc<Session>,
    pub nav: Nav,
    pub active: &'static str,
    pub path: String,
    dict: Option<Dict>,
}

impl PageCtx {
    /// Привязать контекст к фиче: её пункт меню подсвечен, её словарь используется первым.
    pub fn for_feature(mut self, feature: &'static str, dict: Dict) -> Self {
        self.active = feature;
        self.dict = Some(dict);
        self
    }

    /// Строка интерфейса на языке страницы.
    pub fn t(&self, key: &str) -> String {
        crate::i18n::t(self.lang, self.dict, key)
    }

    pub fn can_write(&self) -> bool {
        self.session.scope().can_write()
    }

    pub fn nav_items(&self) -> &[NavItem] {
        &self.nav.0
    }

    pub fn nav_label(&self, item: &NavItem) -> &'static str {
        self.lang.pick(item.label)
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
}

/// Middleware после auth: кладёт `PageCtx` в запрос.
pub async fn attach_page(State(nav): State<Nav>, mut req: Request, next: Next) -> Response {
    if let Some(session) = req.extensions().get::<Session>().cloned() {
        let path = req.uri().path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| "/".into());
        let page = PageCtx { lang: session.lang, session: Arc::new(session), nav, active: "", path, dict: None };
        req.extensions_mut().insert(page);
    }
    next.run(req).await
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

/// Ошибка страницы: та же `AppError`, но ответ — HTML.
#[derive(Debug)]
pub struct PageError(pub AppError);

impl From<AppError> for PageError {
    fn from(e: AppError) -> Self {
        PageError(e)
    }
}

pub type PageResult = Result<Response, PageError>;

impl IntoResponse for PageError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Internal(e) => {
                eprintln!("internal error: {e:?}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        let msg = match &self.0 {
            AppError::Internal(_) => "Internal server error".to_string(),
            other => other.to_string(),
        };
        let body = format!(
            "<!doctype html><meta charset=utf-8><link rel=\"stylesheet\" href=\"/static/app.css\">\
             <div class=\"max-w-xl mx-auto mt-24 p-6 rounded-xl border border-rose-200 bg-rose-50 text-rose-900\">\
             <div class=\"font-semibold mb-2\">{} {}</div><div>{}</div>\
             <a class=\"inline-block mt-4 text-sm underline\" href=\"javascript:history.back()\">←</a></div>",
            status.as_u16(),
            status.canonical_reason().unwrap_or(""),
            escape(&msg)
        );
        (status, Html(body)).into_response()
    }
}

/// Экранирование для текста, собранного вне шаблона.
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Процентное кодирование для параметров URL.
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Деньги для показа: `12 345.50` (неразрывные пробелы между тысячами), всегда 2 знака.
pub fn money(value: impl AsDecimal) -> String {
    let v = value.decimal().round_dp(2);
    let neg = v.is_sign_negative() && !v.is_zero();
    let s = format!("{:.2}", v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, "00"));
    let mut out = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push('\u{202F}');
        }
        out.push(ch);
    }
    format!("{}{}.{}", if neg { "−" } else { "" }, out, frac)
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

/// Разбор суммы из формы: допускает пробелы, запятую как разделитель дроби.
pub fn parse_money(field: &str, value: &str) -> Result<Decimal, String> {
    let cleaned: String = value.chars().filter(|c| !c.is_whitespace() && *c != '\u{202F}').collect::<String>().replace(',', ".");
    cleaned.parse::<Decimal>().map_err(|_| format!("{field}: '{value}' — не число"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn money_format() {
        assert_eq!(money(Decimal::from_str("1234567.5").unwrap()), "1\u{202F}234\u{202F}567.50");
        assert_eq!(money(Decimal::from_str("-12").unwrap()), "−12.00");
        assert_eq!(money(Decimal::ZERO), "0.00");
    }

    #[test]
    fn money_parse() {
        assert_eq!(parse_money("a", "1 250,5").unwrap(), Decimal::from_str("1250.5").unwrap());
        assert!(parse_money("a", "abc").is_err());
    }
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
