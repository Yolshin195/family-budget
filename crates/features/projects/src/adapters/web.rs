//! Входящий адаптер: вход через Rauthy (публично), онбординг (вход есть, проекта нет), профиль и
//! безопасность (`/profile`), проекты и участники (`/projects`), приглашение по ссылке (`/join/{code}`).

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{AppendHeaders, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use chrono::{DateTime, Utc};
use kernel::auth::{
    Csrf, CurrentUser, Identity, LANG_COOKIE, MaybeUser, PROJECT_COOKIE, RequestMeta, Role, Theme, Via, cookie, request_lang, same_origin, same_origin_strict,
    session_cookie, session_credential, session_csrf, set_cookie, set_cookie_at,
};
use kernel::config::Config;
use kernel::crypto::ct_eq;
use kernel::error::AppError;
use kernel::i18n::Lang;
use kernel::web::{Flash, PageCtx, PageError, PageResult, START_PAGE, flash_cookie, render, see_other, see_other_flash};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{ApiToken, AuditEvent, InviteState, PendingLogin, ProjectKind, SessionRecord, TOKEN_TERMS, split_invite_code};
use crate::service::{InvitePreview, ProjectService, ProjectView};

type Svc = State<Arc<ProjectService>>;

const YEAR: i64 = 365 * 24 * 3600;
const SESSION_AGE: i64 = 30 * 24 * 3600;
const OIDC_COOKIE: &str = "fb_oidc";
/// Приглашение, открытое до входа: после входа или регистрации в Rauthy человек вернётся к нему.
const JOIN_COOKIE: &str = "fb_join";
const JOIN_AGE: i64 = 24 * 3600;

/// Публичные страницы (без авторизации), от корня сайта.
pub fn public(service: Arc<ProjectService>) -> Router {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/register", get(register))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", get(logout_page).post(logout))
        .route("/auth/dev", post(dev_login))
        .route("/demo", post(demo))
        .route("/lang/{code}", get(set_lang))
        .route("/join/{code}", get(join_page))
        .with_state(service)
}

/// Страницы аккаунта: вход есть, проекта может не быть (онбординг, принять приглашение).
pub fn account(service: Arc<ProjectService>) -> Router {
    Router::new().route("/onboarding", get(onboarding_form).post(onboarding)).route("/join/{code}", post(join)).with_state(service)
}

/// Страницы под авторизацией (монтируются в /projects).
pub fn pages(service: Arc<ProjectService>) -> Router {
    Router::new()
        .route("/", get(overview))
        .route("/create", post(create))
        .route("/switch/{id}", get(switch))
        .route("/{id}/rename", post(rename))
        .route("/{id}/require-2fa", post(require_2fa))
        .route("/{id}/delete", post(delete_project))
        .route("/{id}/members/{user}/role", post(set_role))
        .route("/{id}/members/{user}/remove", post(remove_member))
        .route("/{id}/make-family", post(make_family))
        .route("/{id}/invites", post(create_invite))
        .route("/{id}/invites/{invite}/revoke", post(revoke_invite))
        .with_state(service)
}

/// Профиль и безопасность — от корня сайта, под авторизацией.
pub fn root(service: Arc<ProjectService>) -> Router {
    Router::new()
        .route("/profile", get(profile).post(save_profile))
        .route("/profile/delete", post(delete_account))
        .route("/profile/security", get(security))
        .route("/profile/security/logout-all", post(logout_all))
        .route("/profile/security/sessions/{hash}/revoke", post(revoke_session))
        .route("/profile/security/tokens", post(create_token))
        .route("/profile/security/tokens/{id}/revoke", post(revoke_token))
        .with_state(service)
}

fn lang_of(headers: &HeaderMap) -> Lang {
    request_lang(headers).unwrap_or_default()
}

fn t(lang: Lang, key: &str) -> String {
    kernel::i18n::t(lang, Some(crate::i18n::dict), key)
}

// ---------------------------------------------------------------- вход через Rauthy

#[derive(Serialize, Deserialize)]
struct PendingCookie {
    s: String,
    n: String,
    v: String,
    x: String,
    t: i64,
}

fn seal_pending(cfg: &Config, p: &PendingLogin) -> String {
    let c = PendingCookie { s: p.state.clone(), n: p.nonce.clone(), v: p.pkce_verifier.clone(), x: p.next.clone(), t: p.started };
    cfg.signer.seal("oidc", &serde_json::to_string(&c).unwrap_or_default())
}

fn open_pending(cfg: &Config, headers: &HeaderMap) -> Option<PendingLogin> {
    let json = cfg.signer.open("oidc", &cookie(headers, OIDC_COOKIE)?)?;
    let c: PendingCookie = serde_json::from_str(&json).ok()?;
    Some(PendingLogin { state: c.s, nonce: c.n, pkce_verifier: c.v, next: c.x, started: c.t })
}

/// Страница входа без Rauthy (разработка) и страница ошибки входа.
#[derive(Template)]
#[template(path = "projects/auth.html")]
struct AuthPage {
    lang: Lang,
    langs: [Lang; 3],
    error: Option<String>,
    oidc: bool,
    dev: bool,
    demo_users: Vec<(&'static str, &'static str)>,
    next: String,
    /// Подтверждение выхода (GET /auth/logout): CSRF-токен формы.
    logout_csrf: Option<String>,
}

impl AuthPage {
    fn new(svc: &ProjectService, lang: Lang) -> Self {
        Self {
            lang,
            langs: Lang::ALL,
            error: None,
            oidc: svc.oidc_enabled(),
            dev: svc.config().dev_login,
            demo_users: vec![("aleksey@demo.local", "Алексей · OWNER"), ("anna@demo.local", "Anna · EDITOR")],
            next: START_PAGE.into(),
            logout_csrf: None,
        }
    }

    fn t(&self, key: &str) -> String {
        t(self.lang, key)
    }

    fn asset(&self, name: &str) -> String {
        kernel::web::asset_url(name)
    }

    fn icon(&self, name: &str) -> String {
        kernel::web::icon_href(name)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct NextQuery {
    next: String,
}

/// Есть действующая сессия — вход и регистрация не нужны.
async fn signed_in(svc: &ProjectService, headers: &HeaderMap) -> bool {
    match session_credential(svc.config(), headers) {
        Some(cred) => kernel::auth::Authenticator::authenticate(svc, &cred).await.ok().flatten().is_some(),
        None => false,
    }
}

/// «Войти» / «Создать бюджет»: переход в Rauthy (code + PKCE, state и nonce — в подписанной cookie на 10 минут).
async fn login(State(svc): Svc, headers: HeaderMap, Query(q): Query<NextQuery>) -> PageResult {
    let lang = lang_of(&headers);
    // Уже вошли — сразу в приложение.
    if signed_in(&svc, &headers).await {
        return Ok(see_other(&crate::domain::safe_next(&q.next)));
    }
    if !svc.oidc_enabled() {
        return render(&AuthPage { next: crate::domain::safe_next(&q.next), ..AuthPage::new(&svc, lang) });
    }
    let (url, pending) = svc.begin_login(&q.next, lang).await?;
    let c = set_cookie_at(svc.config(), OIDC_COOKIE, &seal_pending(svc.config(), &pending), crate::domain::LOGIN_TTL_SECS, "/auth");
    Ok((AppendHeaders([c]), Redirect::to(&url)).into_response())
}

/// «Создать бюджет»: страница регистрации Rauthy, после неё — обычный вход. Ссылки «Войти» на ней нет,
/// поэтому вошедшего туда не отправляем — сразу в приложение.
async fn register(State(svc): Svc, headers: HeaderMap) -> Response {
    if signed_in(&svc, &headers).await {
        return see_other(START_PAGE);
    }
    match &svc.config().oidc {
        Some(o) if svc.oidc_enabled() => {
            let back = format!("{}/auth/login", svc.config().base_url);
            Redirect::to(&format!("{}?redirect_uri={}", o.register_url, kernel::web::url_encode(&back))).into_response()
        }
        _ => Redirect::to("/auth/login").into_response(),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CallbackQuery {
    code: String,
    state: String,
    error: String,
    error_description: String,
}

/// Возврат из Rauthy: обмен кода, проверка ID-токена, новая сессия приложения.
async fn callback(State(svc): Svc, headers: HeaderMap, Extension(meta): Extension<RequestMeta>, Query(q): Query<CallbackQuery>) -> PageResult {
    let lang = lang_of(&headers);
    let cfg = svc.config();
    let clear_pending = set_cookie_at(cfg, OIDC_COOKIE, "", 0, "/auth");
    let fail = |msg: String| -> PageResult {
        let mut r = render(&AuthPage { error: Some(msg), ..AuthPage::new(&svc, lang) })?;
        if let Ok(v) = clear_pending.1.parse() {
            r.headers_mut().append(axum::http::header::SET_COOKIE, v);
        }
        Ok(r)
    };
    // Пользователю — общий текст: ответ Rauthy и подробности проверки уходят в журнал.
    if !q.error.is_empty() {
        svc.login_failed(&meta, &[("error", &q.error), ("description", &q.error_description)]).await;
        return fail(t(lang, "login_failed_hint"));
    }
    let Some(pending) = open_pending(cfg, &headers) else {
        return fail(t(lang, "login_failed_hint"));
    };
    match svc.finish_login(&pending, &q.state, &q.code, lang, &meta).await {
        Ok(r) => {
            // Открывал приглашение до входа — сразу к нему (новичку онбординг не нужен: проект будет семейный).
            let to = match cookie(&headers, JOIN_COOKIE).filter(|c| split_invite_code(c).is_some()) {
                Some(code) => format!("/join/{code}"),
                None if r.onboarding => "/onboarding".to_string(),
                None => r.next,
            };
            Ok((
                AppendHeaders([
                    session_cookie(cfg, &r.raw_session, SESSION_AGE),
                    clear_pending,
                    set_cookie(cfg, PROJECT_COOKIE, "", 0),
                    set_cookie(cfg, JOIN_COOKIE, "", 0),
                ]),
                Redirect::to(&to),
            )
                .into_response())
        }
        Err(AppError::Validation(m) | AppError::Forbidden(m)) => {
            svc.login_failed(&meta, &[("reason", &m)]).await;
            fail(t(lang, "login_failed_hint"))
        }
        Err(e) => Err(PageError(e)),
    }
}

/// GET /auth/logout — подтверждение (выход только POST-формой с CSRF-токеном).
async fn logout_page(State(svc): Svc, headers: HeaderMap) -> PageResult {
    match session_csrf(svc.config(), &headers) {
        Some((_, csrf)) => render(&AuthPage { logout_csrf: Some(csrf), ..AuthPage::new(&svc, lang_of(&headers)) }),
        None => Ok(see_other("/")),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CsrfForm {
    _csrf: String,
}

/// Выход: сессия приложения отзывается, затем — выход из Rauthy (`end_session_endpoint`).
async fn logout(State(svc): Svc, headers: HeaderMap, Extension(meta): Extension<RequestMeta>, Form(f): Form<CsrfForm>) -> Response {
    let cfg = svc.config();
    let clear = [session_cookie(cfg, "", 0), set_cookie(cfg, PROJECT_COOKIE, "", 0)];
    let Some((raw, csrf)) = session_csrf(cfg, &headers) else {
        return (AppendHeaders(clear), Redirect::to("/")).into_response();
    };
    if !same_origin(cfg, &headers) || !ct_eq(&f._csrf, &csrf) {
        return PageError(AppError::Forbidden("CSRF: неверный токен формы".into())).into_response();
    }
    let to = svc.logout(&raw, &meta).await.ok().flatten().unwrap_or_else(|| "/".into());
    (AppendHeaders(clear), Redirect::to(&to)).into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DevForm {
    email: String,
    next: String,
}

/// Вход под демо-пользователем без Rauthy — только в режиме разработки.
async fn dev_login(State(svc): Svc, headers: HeaderMap, Extension(meta): Extension<RequestMeta>, Form(f): Form<DevForm>) -> PageResult {
    let cfg = svc.config();
    if !same_origin(cfg, &headers) {
        return Err(PageError(AppError::Forbidden("CSRF: запрос с чужого сайта".into())));
    }
    match svc.dev_login(&f.email, &meta).await {
        Ok(raw) => Ok((
            AppendHeaders([session_cookie(cfg, &raw, SESSION_AGE), set_cookie(cfg, PROJECT_COOKIE, "", 0)]),
            Redirect::to(&crate::domain::safe_next(&f.next)),
        )
            .into_response()),
        Err(AppError::Validation(m)) => render(&AuthPage { error: Some(m), ..AuthPage::new(&svc, lang_of(&headers)) }),
        Err(e) => Err(PageError(e)),
    }
}

/// «Посмотреть демо»: вход гостем в демо-проект с ролью VIEWER, без регистрации. Ни сессии, ни
/// CSRF-токена тут нет, поэтому `Origin` (или `Referer`) обязателен.
async fn demo(State(svc): Svc, headers: HeaderMap, Extension(meta): Extension<RequestMeta>) -> PageResult {
    let cfg = svc.config();
    if !same_origin_strict(cfg, &headers) {
        return Err(PageError(AppError::Forbidden("CSRF: запрос с чужого сайта".into())));
    }
    let (raw, project) = svc.guest_login(&meta).await?;
    Ok((
        AppendHeaders([session_cookie(cfg, &raw, 24 * 3600), set_cookie(cfg, PROJECT_COOKIE, &project.to_string(), 24 * 3600)]),
        // Демо показывает бюджет целиком — начинаем с «Обзора», а не со стартовой страницы.
        Redirect::to("/budget"),
    )
        .into_response())
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BackQuery {
    back: String,
}

/// Переключить язык: cookie + настройка пользователя (если он вошёл).
async fn set_lang(State(svc): Svc, headers: HeaderMap, Path(code): Path<String>, Query(q): Query<BackQuery>) -> Response {
    let lang = Lang::parse(&code).unwrap_or_default();
    // Только сессия браузера: API-токен действует лишь в /api.
    if let Some(cred) = session_credential(svc.config(), &headers) {
        if let Ok(Some(id)) = kernel::auth::Authenticator::authenticate(svc.as_ref(), &cred).await {
            let _ = svc.set_lang(id.user_id, lang).await;
        }
    }
    let back = if q.back.starts_with('/') && !q.back.starts_with("//") { q.back } else { "/".into() };
    ([set_cookie(svc.config(), LANG_COOKIE, lang.code(), YEAR)], Redirect::to(&back)).into_response()
}

// ---------------------------------------------------------------- приглашение по ссылке

#[derive(Template)]
#[template(path = "projects/join.html")]
struct JoinPage {
    lang: Lang,
    langs: [Lang; 3],
    code: String,
    /// `None` — ссылка неверная или приглашение удалено.
    invite: Option<InvitePreview>,
    /// Вошёл — CSRF-токен формы «Присоединиться»; не вошёл — кнопки входа и регистрации.
    csrf: Option<String>,
    error: Option<String>,
}

impl JoinPage {
    fn t(&self, key: &str) -> String {
        t(self.lang, key)
    }
    fn asset(&self, name: &str) -> String {
        kernel::web::asset_url(name)
    }
    fn icon(&self, name: &str) -> String {
        kernel::web::icon_href(name)
    }
    fn state(&self) -> &'static str {
        self.invite.as_ref().map(|i| if i.member { "member" } else { i.state.as_str() }).unwrap_or("missing")
    }
    fn state_text(&self) -> String {
        self.t(&format!("join_{}", self.state()))
    }
    fn role(&self) -> String {
        self.invite.as_ref().map(|i| self.t(&format!("join_as_{}", i.role.as_str()))).unwrap_or_default()
    }
}

async fn join_view(svc: &ProjectService, headers: &HeaderMap, code: String, viewer: Option<Uuid>, lang: Lang, error: Option<String>) -> Result<(JoinPage, bool), PageError> {
    let invite = match svc.invite_preview(&code, viewer).await {
        Ok(p) => Some(p),
        Err(AppError::NotFound(_)) => None,
        Err(e) => return Err(PageError(e)),
    };
    let active = invite.as_ref().is_some_and(|i| i.state == InviteState::Active && !i.member);
    let csrf = viewer.and_then(|_| session_csrf(svc.config(), headers)).map(|(_, c)| c);
    Ok((JoinPage { lang, langs: Lang::ALL, code, invite, csrf, error }, active))
}

/// Ссылка-приглашение: кто и куда зовёт. Только показывает — расходует приглашение кнопка «Присоединиться»,
/// поэтому превью ссылки в мессенджере ничего не ломает.
async fn join_page(State(svc): Svc, headers: HeaderMap, Extension(maybe): Extension<MaybeUser>, Path(code): Path<String>) -> PageResult {
    let lang = lang_of(&headers);
    let (page, active) = join_view(&svc, &headers, code, maybe.0, lang, None).await?;
    let remember = active && maybe.0.is_none();
    let found = page.invite.is_some();
    let mut resp = render(&page)?;
    if !found {
        *resp.status_mut() = axum::http::StatusCode::NOT_FOUND;
    }
    // Не вошёл — запомнить приглашение на время входа или регистрации в Rauthy.
    if remember {
        let (name, value) = set_cookie(svc.config(), JOIN_COOKIE, &page.code, JOIN_AGE);
        if let Ok(v) = value.parse() {
            resp.headers_mut().append(name, v);
        }
    }
    Ok(resp)
}

/// «Присоединиться»: стать участником и сразу открыть семейный бюджет.
async fn join(State(svc): Svc, headers: HeaderMap, Extension(user): Extension<CurrentUser>, Extension(meta): Extension<RequestMeta>, Path(code): Path<String>) -> PageResult {
    let cfg = svc.config();
    // Язык — как во всём приложении: выбранный в переключателе (cookie), иначе из профиля.
    let lang = cookie(&headers, LANG_COOKIE).and_then(|v| Lang::parse(&v)).unwrap_or(svc.user(user.id).await?.lang);
    match svc.accept_invite(user.id, &code, &meta).await {
        Ok(project) => Ok((
            AppendHeaders([
                set_cookie(cfg, PROJECT_COOKIE, &project.to_string(), YEAR),
                set_cookie(cfg, JOIN_COOKIE, "", 0),
                flash_cookie(cfg, &Flash::ok(t(lang, "joined"))),
            ]),
            Redirect::to(START_PAGE),
        )
            .into_response()),
        Err(AppError::Validation(m) | AppError::Forbidden(m) | AppError::NotFound(m)) => {
            let (page, _) = join_view(&svc, &headers, code, Some(user.id), lang, Some(m)).await?;
            let mut resp = render(&page)?;
            *resp.status_mut() = axum::http::StatusCode::UNPROCESSABLE_ENTITY;
            Ok(resp)
        }
        Err(e) => Err(PageError(e)),
    }
}

// ---------------------------------------------------------------- онбординг

#[derive(Template)]
#[template(path = "projects/onboarding.html")]
struct OnboardingPage {
    lang: Lang,
    langs: [Lang; 3],
    csrf: String,
    name: String,
    currency: String,
    error: Option<String>,
}

impl OnboardingPage {
    fn t(&self, key: &str) -> String {
        t(self.lang, key)
    }
    fn asset(&self, name: &str) -> String {
        kernel::web::asset_url(name)
    }
    fn icon(&self, name: &str) -> String {
        kernel::web::icon_href(name)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OnboardingForm {
    name: String,
    base_currency: String,
    lang: String,
}

async fn onboarding_form(State(svc): Svc, headers: HeaderMap, Extension(user): Extension<CurrentUser>, Extension(csrf): Extension<Csrf>) -> PageResult {
    let u = svc.user(user.id).await?;
    let lang = request_lang(&headers).unwrap_or(u.lang);
    render(&OnboardingPage { lang, langs: Lang::ALL, csrf: csrf.0, name: u.display_name, currency: "THB".into(), error: None })
}

async fn onboarding(State(svc): Svc, Extension(user): Extension<CurrentUser>, Extension(csrf): Extension<Csrf>, Form(f): Form<OnboardingForm>) -> PageResult {
    let lang = Lang::parse(&f.lang).unwrap_or_default();
    match svc.onboard(user.id, &f.name, &f.base_currency, lang).await {
        Ok(project) => Ok((
            AppendHeaders([set_cookie(svc.config(), PROJECT_COOKIE, &project.to_string(), YEAR), set_cookie(svc.config(), LANG_COOKIE, lang.code(), YEAR)]),
            Redirect::to(START_PAGE),
        )
            .into_response()),
        Err(AppError::Validation(m)) => render(&OnboardingPage { lang, langs: Lang::ALL, csrf: csrf.0, name: f.name, currency: f.base_currency, error: Some(m) }),
        Err(e) => Err(PageError(e)),
    }
}

// ---------------------------------------------------------------- профиль и безопасность

#[derive(Template)]
#[template(path = "projects/profile.html")]
struct ProfilePage {
    page: PageCtx,
    name: String,
    email: String,
    user_lang: String,
    theme: String,
    error: Option<String>,
}

fn profile_ctx(page: PageCtx) -> PageCtx {
    page.for_feature("profile", crate::i18n::dict)
}

async fn profile_page(svc: &ProjectService, page: PageCtx, error: Option<String>) -> PageResult {
    let u = svc.user(page.session.user_id).await?;
    render(&ProfilePage { page: profile_ctx(page), name: u.display_name, email: u.email, user_lang: u.lang.upper().into(), theme: u.theme.as_str().into(), error })
}

async fn profile(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    profile_page(&svc, page, None).await
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ProfileForm {
    name: String,
    lang: String,
    theme: String,
    confirm: String,
}

async fn save_profile(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<ProfileForm>) -> PageResult {
    let lang = Lang::parse(&f.lang).unwrap_or(page.lang);
    let theme = Theme::parse(&f.theme).unwrap_or_default();
    match svc.update_profile(page.session.user_id, &f.name, lang, theme).await {
        Ok(()) => {
            let cfg = page.config.clone();
            let msg = kernel::i18n::t(lang, None, "toast.saved");
            Ok((AppendHeaders([set_cookie(&cfg, LANG_COOKIE, lang.code(), YEAR), flash_cookie(&cfg, &Flash::ok(msg))]), Redirect::to("/profile")).into_response())
        }
        Err(AppError::Validation(m)) => profile_page(&svc, page, Some(m.replace("display_name", "name"))).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn delete_account(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Form(f): Form<ProfileForm>) -> PageResult {
    let u = svc.user(page.session.user_id).await?;
    if !f.confirm.trim().eq_ignore_ascii_case(&u.email) {
        let msg = kernel::i18n::t(page.lang, Some(crate::i18n::dict), "confirm_email_mismatch");
        return profile_page(&svc, page, Some(msg)).await;
    }
    match svc.delete_account(u.id, &meta).await {
        Ok(d) => {
            let cfg = page.config.clone();
            let to = if d.rauthy { "/data-deletion?done=1" } else { "/data-deletion?done=1&rauthy=0" };
            Ok((AppendHeaders([session_cookie(&cfg, "", 0), set_cookie(&cfg, PROJECT_COOKIE, "", 0)]), Redirect::to(to)).into_response())
        }
        Err(AppError::Validation(m)) => profile_page(&svc, page, Some(format!("confirm: {m}"))).await,
        Err(e) => Err(PageError(e)),
    }
}

struct DeviceView {
    hash: String,
    browser: String,
    ip: String,
    created: String,
    last_seen: String,
    mfa: bool,
    current: bool,
}

struct TokenView {
    id: Uuid,
    name: String,
    project: String,
    scopes: String,
    expires: String,
    last_used: String,
    status: &'static str,
}

struct EventView {
    at: String,
    action: String,
    project: String,
    ip: String,
}

#[derive(Template)]
#[template(path = "projects/security.html")]
struct SecurityPage {
    page: PageCtx,
    devices: Vec<DeviceView>,
    tokens: Vec<TokenView>,
    events: Vec<EventView>,
    projects: Vec<(Uuid, String)>,
    terms: [i64; 4],
    new_token: Option<String>,
    error: Option<String>,
    account_url: String,
    /// Гость демо: только своё устройство, без токенов и журнала.
    guest: bool,
}

impl SecurityPage {
    fn term_label(&self, days: &i64) -> String {
        self.page.t(&format!("token_days_{days}"))
    }
}

fn when(page: &PageCtx, at: DateTime<Utc>) -> String {
    let local = at.with_timezone(&chrono::Local);
    format!("{} {}", page.date(&local.date_naive()), local.format("%H:%M"))
}

/// Краткое название браузера и системы из User-Agent.
fn browser(ua: &str) -> String {
    let b = ["Edg/", "OPR/", "Firefox/", "Chrome/", "Safari/"].iter().find(|k| ua.contains(**k)).map(|k| match *k {
        "Edg/" => "Edge",
        "OPR/" => "Opera",
        "Firefox/" => "Firefox",
        "Chrome/" => "Chrome",
        _ => "Safari",
    });
    let os = [("iPhone", "iPhone"), ("iPad", "iPad"), ("Android", "Android"), ("Mac OS X", "macOS"), ("Windows", "Windows"), ("Linux", "Linux")]
        .iter()
        .find(|(k, _)| ua.contains(k))
        .map(|(_, v)| *v);
    match (b, os) {
        (Some(b), Some(o)) => format!("{b} · {o}"),
        (Some(b), None) => b.to_string(),
        (None, Some(o)) => o.to_string(),
        (None, None) => String::new(),
    }
}

async fn security_page(svc: &ProjectService, page: PageCtx, identity: &Identity, new_token: Option<String>, error: Option<String>) -> PageResult {
    let user = page.session.user_id;
    let current = match &identity.via {
        Via::Session { id_hash } => id_hash.clone(),
        Via::Token { .. } => String::new(),
    };
    let label = kernel::i18n::t(page.lang, Some(crate::i18n::dict), "profile");
    let page = profile_ctx(page).crumb(label, "/profile");
    let unknown = page.t("unknown_device");
    // Гость демо — общий аккаунт: только своё устройство, без токенов и журнала (чужие IP не показываем).
    let guest = svc.is_guest(user).await?;
    let devices = svc
        .sessions(user)
        .await?
        .into_iter()
        .filter(|s| !guest || s.id_hash == current)
        .map(|s: SessionRecord| DeviceView {
            current: s.id_hash == current,
            browser: Some(browser(s.user_agent.as_deref().unwrap_or(""))).filter(|b| !b.is_empty()).unwrap_or_else(|| unknown.clone()),
            ip: s.ip.unwrap_or_default(),
            created: when(&page, s.created_at),
            last_seen: when(&page, s.last_seen_at),
            mfa: s.mfa,
            hash: s.id_hash,
        })
        .collect();
    let names: Vec<(Uuid, String)> = page.session.projects.iter().map(|p| (p.id, p.name.clone())).collect();
    let project_name = |id: Option<Uuid>| id.and_then(|id| names.iter().find(|(p, _)| *p == id)).map(|(_, n)| n.clone()).unwrap_or_default();
    let now = Utc::now();
    let tokens = if guest { Vec::new() } else { svc.tokens(user).await? };
    let tokens = tokens
        .into_iter()
        .map(|tk: ApiToken| TokenView {
            status: if tk.revoked_at.is_some() {
                "revoked"
            } else if !tk.active(now) {
                "expired"
            } else {
                "active"
            },
            id: tk.id,
            project: project_name(Some(tk.project_id)),
            scopes: tk.scopes.join(", "),
            expires: tk.expires_at.map(|e| page.date(&e.date_naive())).unwrap_or_else(|| page.t("token_days_0")),
            last_used: tk.last_used_at.map(|u| when(&page, u)).unwrap_or_else(|| page.t("never_used")),
            name: tk.name,
        })
        .collect();
    let events = if guest { Vec::new() } else { svc.audit_log(user).await? };
    let events = events
        .into_iter()
        .map(|e: AuditEvent| EventView { at: when(&page, e.at), action: page.t(&format!("act.{}", e.action)), project: project_name(e.project_id), ip: e.ip.unwrap_or_default() })
        .collect();
    let account_url = page.config.oidc.as_ref().map(|o| o.account_url.clone()).unwrap_or_default();
    render(&SecurityPage { page, devices, tokens, events, projects: names, terms: TOKEN_TERMS, new_token, error, account_url, guest })
}

async fn security(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(identity): Extension<Identity>) -> PageResult {
    security_page(&svc, page, &identity, None, None).await
}

async fn logout_all(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(identity): Extension<Identity>, Extension(meta): Extension<RequestMeta>) -> PageResult {
    let current = match &identity.via {
        Via::Session { id_hash } => id_hash.clone(),
        Via::Token { .. } => String::new(),
    };
    svc.logout_everywhere(page.session.user_id, &current, &meta).await?;
    let msg = kernel::i18n::t(page.lang, Some(crate::i18n::dict), "logged_out_all");
    let cfg = page.config.clone();
    Ok(see_other_flash(&cfg, "/profile/security", Flash::ok(msg)))
}

async fn revoke_session(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Path(hash): Path<String>) -> PageResult {
    svc.revoke_session(page.session.user_id, &hash, &meta).await?;
    Ok(see_other("/profile/security"))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TokenForm {
    name: String,
    project: String,
    scope: Vec<String>,
    days: String,
}

async fn create_token(
    State(svc): Svc,
    Extension(page): Extension<PageCtx>,
    Extension(identity): Extension<Identity>,
    Extension(meta): Extension<RequestMeta>,
    axum_extra::extract::Form(f): axum_extra::extract::Form<TokenForm>,
) -> PageResult {
    let project = Uuid::parse_str(&f.project).unwrap_or(page.session.project_id);
    let days: i64 = f.days.parse().unwrap_or(90);
    match svc.create_token(page.session.user_id, project, &f.name, &f.scope, days, identity.mfa, &meta).await {
        Ok((raw, _)) => security_page(&svc, page, &identity, Some(raw), None).await,
        Err(AppError::Validation(m) | AppError::Forbidden(m) | AppError::NotFound(m)) => security_page(&svc, page, &identity, None, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn revoke_token(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Path(id): Path<Uuid>) -> PageResult {
    svc.revoke_token(page.session.user_id, id, &meta).await?;
    Ok(see_other("/profile/security"))
}

// ---------------------------------------------------------------- проекты

#[derive(Template)]
#[template(path = "projects/overview.html")]
struct OverviewPage {
    page: PageCtx,
    projects: Vec<ProjectView>,
    roles: &'static [&'static str],
    error: Option<String>,
    /// Только что созданное приглашение — его карточка открыта.
    opened: Option<Uuid>,
}

impl OverviewPage {
    fn is_open(&self, id: &Uuid) -> bool {
        self.opened == Some(*id)
    }

    fn until(&self, at: &DateTime<Utc>) -> String {
        when(&self.page, *at)
    }

    fn qr(&self, url: &str) -> String {
        qr_svg(url)
    }

    /// Текст для мессенджера: «Присоединяйся к бюджету „…“».
    fn share_text(&self, project: &str) -> String {
        self.page.t("invite_share_text").replace("{project}", project)
    }

    fn telegram(&self, project: &str, url: &str) -> String {
        format!("https://t.me/share/url?url={}&text={}", kernel::web::url_encode(url), kernel::web::url_encode(&self.share_text(project)))
    }

    fn whatsapp(&self, project: &str, url: &str) -> String {
        format!("https://wa.me/?text={}", kernel::web::url_encode(&format!("{} {url}", self.share_text(project))))
    }
}

/// QR-код ссылки — inline SVG: без внешних сервисов и скриптов. Тёмные модули на белом в любой теме,
/// с полем вокруг — так его читает любая камера.
fn qr_svg(url: &str) -> String {
    use qrcode::render::svg;
    let Ok(code) = qrcode::QrCode::with_error_correction_level(url.as_bytes(), qrcode::EcLevel::M) else { return String::new() };
    let svg = code.render::<svg::Color>().min_dimensions(176, 176).dark_color(svg::Color("#000000")).light_color(svg::Color("#ffffff")).build();
    // Без XML-пролога: SVG встраивается прямо в HTML.
    svg.find("<svg").map(|i| svg[i..].to_string()).unwrap_or_default()
}

async fn show(svc: &ProjectService, page: PageCtx, error: Option<String>, opened: Option<Uuid>) -> PageResult {
    let projects = svc.overview(page.session.user_id).await?;
    render(&OverviewPage { page: page.for_feature(crate::NAME, crate::i18n::dict), projects, roles: &Role::VALUES, error, opened })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OverviewQuery {
    invite: String,
}

async fn overview(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(q): Query<OverviewQuery>) -> PageResult {
    show(&svc, page, None, Uuid::parse_str(&q.invite).ok()).await
}

/// Ошибки валидации и прав показываем на той же странице.
async fn done(svc: &ProjectService, page: PageCtx, r: Result<(), AppError>, flash: Option<&str>) -> PageResult {
    match r {
        Ok(()) => {
            let cfg = page.config.clone();
            Ok(match flash {
                Some(key) => see_other_flash(&cfg, "/projects", Flash::ok(kernel::i18n::t(page.lang, Some(crate::i18n::dict), key))),
                None => see_other("/projects"),
            })
        }
        Err(AppError::Validation(m) | AppError::Conflict(m) | AppError::Forbidden(m)) => show(svc, page, Some(m), None).await,
        Err(e) => Err(PageError(e)),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CreateForm {
    name: String,
    kind: String,
    base_currency: String,
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Form(f): Form<CreateForm>) -> PageResult {
    let kind = ProjectKind::parse(&f.kind).unwrap_or(ProjectKind::Family);
    match svc.create_project(page.session.user_id, &f.name, kind, &f.base_currency, &meta).await {
        Ok(id) => Ok(([set_cookie(&page.config, PROJECT_COOKIE, &id.to_string(), YEAR)], Redirect::to("/projects")).into_response()),
        Err(e) => done(&svc, page, Err(e), None).await,
    }
}

async fn switch(State(svc): Svc, Path(id): Path<Uuid>) -> Response {
    // Проверку членства сделает middleware на следующем запросе: чужой проект просто не выберется.
    ([set_cookie(svc.config(), PROJECT_COOKIE, &id.to_string(), YEAR)], Redirect::to(START_PAGE)).into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct NameForm {
    name: String,
    confirm: String,
    value: String,
}

async fn rename(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<NameForm>) -> PageResult {
    let r = svc.rename(page.session.user_id, id, &f.name).await;
    done(&svc, page, r, Some("toast.saved")).await
}

async fn require_2fa(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Path(id): Path<Uuid>, Form(f): Form<NameForm>) -> PageResult {
    let r = svc.set_require_2fa(page.session.user_id, id, f.value == "1", &meta).await;
    done(&svc, page, r, Some("toast.saved")).await
}

async fn delete_project(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Path(id): Path<Uuid>, Form(f): Form<NameForm>) -> PageResult {
    let r = svc.delete_project(page.session.user_id, id, &f.confirm, &meta).await;
    if r.is_ok() && id == page.session.project_id {
        let cfg = page.config.clone();
        let msg = kernel::i18n::t(page.lang, Some(crate::i18n::dict), "project_deleted");
        return Ok((AppendHeaders([set_cookie(&cfg, PROJECT_COOKIE, "", 0), flash_cookie(&cfg, &Flash::ok(msg))]), Redirect::to("/projects")).into_response());
    }
    done(&svc, page, r, Some("project_deleted")).await
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MemberForm {
    role: String,
}

fn parse_role(v: &str) -> Result<Role, AppError> {
    Role::parse(v).ok_or_else(|| AppError::Validation(format!("role: '{v}'")))
}

async fn set_role(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Path((id, user)): Path<(Uuid, Uuid)>, Form(f): Form<MemberForm>) -> PageResult {
    let r = async { svc.set_role(page.session.user_id, id, user, parse_role(&f.role)?, &meta).await }.await;
    done(&svc, page, r, Some("toast.saved")).await
}

async fn remove_member(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Path((id, user)): Path<(Uuid, Uuid)>) -> PageResult {
    let me = page.session.user_id;
    let r = svc.remove_member(me, id, user, &meta).await;
    // Вышел сам из активного проекта — сбросить выбор проекта.
    if r.is_ok() && me == user {
        let cfg = page.config.clone();
        let msg = kernel::i18n::t(page.lang, Some(crate::i18n::dict), "left_project");
        let clear = if id == page.session.project_id { set_cookie(&cfg, PROJECT_COOKIE, "", 0) } else { set_cookie(&cfg, "fb_noop", "", 0) };
        return Ok((AppendHeaders([clear, flash_cookie(&cfg, &Flash::ok(msg))]), Redirect::to("/projects")).into_response());
    }
    done(&svc, page, r, Some("toast.saved")).await
}

async fn make_family(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Path(id): Path<Uuid>) -> PageResult {
    let r = svc.make_family(page.session.user_id, id, &meta).await;
    done(&svc, page, r, Some("made_family")).await
}

async fn create_invite(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Path(id): Path<Uuid>, Form(f): Form<MemberForm>) -> PageResult {
    let r = async { svc.create_invite(page.session.user_id, id, parse_role(&f.role)?, &meta).await }.await;
    match r {
        // Карточка нового приглашения открыта: QR-код и кнопки «Поделиться» сразу на экране.
        Ok(invite) => Ok(see_other(&format!("/projects?invite={invite}#invite-{invite}"))),
        Err(e) => done(&svc, page, Err(e), None).await,
    }
}

async fn revoke_invite(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(meta): Extension<RequestMeta>, Path((id, invite)): Path<(Uuid, Uuid)>) -> PageResult {
    let r = svc.revoke_invite(page.session.user_id, id, invite, &meta).await;
    done(&svc, page, r, Some("invite_revoked")).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_is_inline_svg() {
        let svg = qr_svg("https://budget.example/join/0123456789abcdef0123456789abcdef.AAAAAAAAAAAAAAAAAAAAAA");
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"), "{svg}");
        assert!(!svg.contains("style="), "CSP: без встроенных стилей");
    }

    #[test]
    fn browser_names() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
        assert_eq!(browser(ua), "Safari · macOS");
        assert_eq!(browser("Mozilla/5.0 (X11; Linux x86_64) Chrome/130.0 Safari/537.36"), "Chrome · Linux");
        assert_eq!(browser(""), "");
    }

    #[test]
    fn pending_cookie_roundtrip() {
        let cfg = Config::test();
        let p = PendingLogin { state: "s".into(), nonce: "n".into(), pkce_verifier: "v".into(), next: "/budget".into(), started: 1 };
        let mut h = HeaderMap::new();
        h.insert(axum::http::header::COOKIE, format!("{OIDC_COOKIE}={}", seal_pending(&cfg, &p)).parse().unwrap());
        assert_eq!(open_pending(&cfg, &h), Some(p));
        let mut forged = HeaderMap::new();
        forged.insert(axum::http::header::COOKIE, format!("{OIDC_COOKIE}=eyJzIjoiZXZpbCJ9.AAAA").parse().unwrap());
        assert_eq!(open_pending(&cfg, &forged), None, "подделанная cookie не принимается");
    }
}
