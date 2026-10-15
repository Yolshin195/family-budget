//! Входящий адаптер: лендинг, правовые страницы, sitemap.xml, robots.txt, манифест PWA (всё публично).

use std::sync::Arc;

use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Extension, Router};
use kernel::auth::{LANG_COOKIE, MaybeUser, request_lang, set_cookie};
use kernel::config::Config;
use kernel::i18n::Lang;
use kernel::web::{PageResult, asset_url, render};
use serde::Deserialize;

use crate::domain::{localized_url, robots, sitemap};
use crate::service::SiteService;

#[derive(Clone)]
struct SiteState {
    svc: Arc<SiteService>,
    config: Arc<Config>,
}

type St = State<SiteState>;

pub fn public(svc: Arc<SiteService>, config: Arc<Config>) -> Router {
    Router::new()
        .route("/", get(landing))
        .route("/privacy", get(privacy))
        .route("/terms", get(terms))
        .route("/data-deletion", get(deletion))
        .route("/sitemap.xml", get(sitemap_xml))
        .route("/robots.txt", get(robots_txt))
        .route("/manifest.webmanifest", get(manifest))
        .with_state(SiteState { svc, config })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LangQuery {
    lang: String,
    done: String,
    rauthy: String,
}

/// Язык страницы: `?lang=` (ссылки индексируются) → cookie → Accept-Language → ru.
fn page_lang(q: &LangQuery, headers: &HeaderMap) -> (Lang, bool) {
    match Lang::parse(&q.lang) {
        Some(l) => (l, true),
        None => (request_lang(headers).unwrap_or_default(), false),
    }
}

/// Общие поля публичных страниц: язык, адреса hreflang, контакты.
struct Public {
    lang: Lang,
    langs: [Lang; 3],
    base: String,
    path: &'static str,
    contact: String,
}

impl Public {
    fn new(cfg: &Config, lang: Lang, path: &'static str) -> Self {
        Self { lang, langs: Lang::ALL, base: cfg.base_url.clone(), path, contact: cfg.contact_email.clone() }
    }

    fn t(&self, key: &str) -> String {
        kernel::i18n::t(self.lang, Some(crate::i18n::dict), key)
    }

    fn url(&self, lang: &Lang) -> String {
        localized_url(&self.base, self.path, *lang)
    }

    fn canonical(&self) -> String {
        self.url(&self.lang)
    }

    fn og_locale(&self) -> &'static str {
        match self.lang {
            Lang::Ru => "ru_RU",
            Lang::En => "en_US",
            Lang::Th => "th_TH",
        }
    }

    fn asset(&self, name: &str) -> String {
        asset_url(name)
    }

    fn abs_asset(&self, name: &str) -> String {
        format!("{}{}", self.base, asset_url(name))
    }

    fn icon(&self, name: &str) -> String {
        kernel::web::icon_href(name)
    }
}

/// Явно выбранный язык запоминаем — дальше в приложении он же.
fn with_lang_cookie(cfg: &Config, lang: Lang, explicit: bool, resp: PageResult) -> PageResult {
    let mut r = resp?;
    if explicit {
        if let Ok(v) = set_cookie(cfg, LANG_COOKIE, lang.code(), 365 * 24 * 3600).1.parse() {
            r.headers_mut().append(axum::http::header::SET_COOKIE, v);
        }
    }
    // Публичные страницы кэшируются ненадолго: язык зависит от URL и cookie.
    r.headers_mut().insert(axum::http::header::VARY, axum::http::HeaderValue::from_static("Cookie, Accept-Language"));
    Ok(r)
}

// ---------------------------------------------------------------- лендинг

#[derive(Template)]
#[template(path = "site/landing.html")]
struct LandingPage {
    lang: Lang,
    p: Public,
    guest_demo: bool,
    open_registration: bool,
}

impl LandingPage {
    fn t(&self, key: &str) -> String {
        self.p.t(key)
    }
    /// Главная кнопка: «Создать бюджет» при свободной регистрации, иначе «Войти».
    fn cta_href(&self) -> &'static str {
        if self.open_registration { "/auth/register" } else { "/auth/login" }
    }
    fn cta_text(&self) -> String {
        self.t(if self.open_registration { "cta.create" } else { "nav.login" })
    }
    fn asset(&self, name: &str) -> String {
        self.p.asset(name)
    }
    fn icon(&self, name: &str) -> String {
        self.p.icon(name)
    }
}

/// `/`: гостю — лендинг на его языке, вошедшему — сразу «Обзор».
async fn landing(State(st): St, Extension(user): Extension<MaybeUser>, headers: HeaderMap, Query(q): Query<LangQuery>) -> PageResult {
    if user.0.is_some() {
        return Ok(Redirect::to("/budget").into_response());
    }
    st.svc.record_visit();
    let (lang, explicit) = page_lang(&q, &headers);
    let (guest_demo, open_registration) = (st.config.guest_demo, st.config.open_registration);
    let page = LandingPage { lang, p: Public::new(&st.config, lang, "/"), guest_demo, open_registration };
    with_lang_cookie(&st.config, lang, explicit, render(&page))
}

// ---------------------------------------------------------------- правовые страницы

#[derive(Template)]
#[template(path = "site/legal.html")]
struct LegalPage {
    lang: Lang,
    p: Public,
    /// privacy / terms / deletion
    kind: &'static str,
    sections: Vec<(String, String)>,
    notice: Option<String>,
}

impl LegalPage {
    fn t(&self, key: &str) -> String {
        self.p.t(key)
    }
    /// Ключ страницы: `privacy.title`, `terms.intro`…
    fn k(&self, suffix: &str) -> String {
        self.p.t(&format!("{}.{suffix}", self.kind))
    }
    fn asset(&self, name: &str) -> String {
        self.p.asset(name)
    }
    fn icon(&self, name: &str) -> String {
        self.p.icon(name)
    }
}

fn legal(st: &SiteState, headers: &HeaderMap, q: &LangQuery, kind: &'static str, path: &'static str, notice: Option<String>) -> PageResult {
    let (lang, explicit) = page_lang(q, headers);
    let p = Public::new(&st.config, lang, path);
    let sections = (1..=5)
        .filter_map(|i| {
            let h = kernel::i18n::t(lang, Some(crate::i18n::dict), &format!("{kind}.h{i}"));
            (!h.starts_with(kind)).then(|| (h, p.t(&format!("{kind}.p{i}"))))
        })
        .collect();
    with_lang_cookie(&st.config, lang, explicit, render(&LegalPage { lang, p, kind, sections, notice }))
}

async fn privacy(State(st): St, headers: HeaderMap, Query(q): Query<LangQuery>) -> PageResult {
    legal(&st, &headers, &q, "privacy", "/privacy", None)
}

async fn terms(State(st): St, headers: HeaderMap, Query(q): Query<LangQuery>) -> PageResult {
    legal(&st, &headers, &q, "terms", "/terms", None)
}

/// Инструкция по удалению данных (обязательна для Facebook); после удаления аккаунта — подтверждение.
async fn deletion(State(st): St, headers: HeaderMap, Query(q): Query<LangQuery>) -> PageResult {
    let lang = page_lang(&q, &headers).0;
    let notice = (q.done == "1").then(|| {
        let mut n = kernel::i18n::t(lang, Some(crate::i18n::dict), "deletion.done");
        if q.rauthy == "0" {
            n.push(' ');
            n.push_str(&kernel::i18n::t(lang, Some(crate::i18n::dict), "deletion.rauthy_manual"));
        }
        n
    });
    legal(&st, &headers, &q, "deletion", "/data-deletion", notice)
}

// ---------------------------------------------------------------- SEO и PWA

fn text(content_type: &'static str, body: String) -> Response {
    ([(CONTENT_TYPE, content_type), (axum::http::header::CACHE_CONTROL, "public, max-age=3600")], body).into_response()
}

async fn sitemap_xml(State(st): St) -> Response {
    text("application/xml; charset=utf-8", sitemap(&st.config.base_url, chrono::Local::now().date_naive()))
}

async fn robots_txt(State(st): St) -> Response {
    text("text/plain; charset=utf-8", robots(&st.config.base_url))
}

/// Манифест PWA: иконки, цвета, старт с «Обзора».
async fn manifest(State(st): St, headers: HeaderMap) -> Response {
    let lang = request_lang(&headers).unwrap_or_default();
    let t = |k: &str| kernel::i18n::t(lang, None, k);
    let m = serde_json::json!({
        "name": t("app.title"),
        "short_name": t("app.title"),
        "lang": lang.code(),
        "start_url": "/budget",
        "scope": "/",
        "display": "standalone",
        "background_color": "#fafaf9",
        "theme_color": "#4338ca",
        "icons": [
            {"src": asset_url("icon-192.png"), "sizes": "192x192", "type": "image/png"},
            {"src": asset_url("icon-512.png"), "sizes": "512x512", "type": "image/png"},
            {"src": asset_url("icon-maskable-512.png"), "sizes": "512x512", "type": "image/png", "purpose": "maskable"}
        ]
    });
    let _ = &st;
    text("application/manifest+json; charset=utf-8", m.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn landing_html(lang: Lang, open_registration: bool) -> String {
        let cfg = Config::test();
        LandingPage { lang, p: Public::new(&cfg, lang, "/"), guest_demo: true, open_registration }.render().unwrap()
    }

    #[test]
    fn landing_registration_follows_config() {
        let closed = landing_html(Lang::Ru, false);
        assert!(!closed.contains("/auth/register"), "регистрация закрыта — «Создать бюджет» не показываем");
        assert!(closed.contains(r#"href="/auth/login""#));
        assert!(landing_html(Lang::Ru, true).contains(r#"href="/auth/register""#));
    }

    #[test]
    fn landing_has_no_github_or_facebook_sign_in() {
        for lang in Lang::ALL {
            for open in [false, true] {
                let html = landing_html(lang, open);
                assert!(!html.contains("GitHub") && !html.contains("Facebook"), "{lang:?}: вход через GitHub/Facebook на лендинге");
            }
        }
    }
}
