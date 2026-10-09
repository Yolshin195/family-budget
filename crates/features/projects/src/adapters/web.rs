//! Входящий адаптер: вход, регистрация, язык (публичные) и страница «Проекты и участники».

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{AppendHeaders, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::auth::{LANG_COOKIE, PROJECT_COOKIE, Role, TOKEN_COOKIE, cookie, request_lang, set_cookie};
use kernel::error::AppError;
use kernel::i18n::Lang;
use kernel::web::{PageCtx, PageError, PageResult, render, see_other};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::ProjectKind;
use crate::service::{ProjectService, ProjectView};

type Svc = State<Arc<ProjectService>>;

const YEAR: i64 = 365 * 24 * 3600;

/// Публичные страницы (без авторизации), от корня сайта.
pub fn public(service: Arc<ProjectService>) -> Router {
    Router::new()
        .route("/login", get(login_form).post(login))
        .route("/register", get(register_form).post(register))
        .route("/logout", get(logout))
        .route("/lang/{code}", get(set_lang))
        .with_state(service)
}

/// Страницы под авторизацией (монтируются в /projects).
pub fn pages(service: Arc<ProjectService>) -> Router {
    Router::new()
        .route("/", get(overview))
        .route("/create", post(create))
        .route("/switch/{id}", get(switch))
        .route("/{id}/rename", post(rename))
        .route("/{id}/members", post(add_member))
        .route("/{id}/members/{user}/role", post(set_role))
        .route("/{id}/members/{user}/remove", post(remove_member))
        .with_state(service)
}

// ---------------------------------------------------------------- вход и регистрация

#[derive(Template)]
#[template(path = "projects/auth.html")]
struct AuthPage {
    lang: Lang,
    langs: [Lang; 3],
    register: bool,
    name: String,
    email: String,
    error: Option<String>,
}

impl AuthPage {
    fn new(lang: Lang, register: bool) -> Self {
        Self { lang, langs: Lang::ALL, register, name: String::new(), email: String::new(), error: None }
    }

    fn t(&self, key: &str) -> String {
        kernel::i18n::t(self.lang, Some(crate::i18n::dict), key)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AuthForm {
    name: String,
    email: String,
    password: String,
    base_currency: String,
}

fn lang_of(headers: &HeaderMap) -> Lang {
    request_lang(headers).unwrap_or_default()
}

fn signed_in(svc: &ProjectService, user_id: Uuid) -> Response {
    let token = svc.issue_token(user_id);
    (
        AppendHeaders([set_cookie(TOKEN_COOKIE, &token, 30 * 24 * 3600), set_cookie(PROJECT_COOKIE, "", 0)]),
        Redirect::to("/budget"),
    )
        .into_response()
}

async fn login_form(headers: HeaderMap) -> PageResult {
    render(&AuthPage::new(lang_of(&headers), false))
}

async fn login(State(svc): Svc, headers: HeaderMap, Form(f): Form<AuthForm>) -> PageResult {
    match svc.login(&f.email, &f.password).await {
        Ok(id) => Ok(signed_in(&svc, id)),
        Err(AppError::Validation(m)) => render(&AuthPage { email: f.email, error: Some(m), ..AuthPage::new(lang_of(&headers), false) }),
        Err(e) => Err(PageError(e)),
    }
}

async fn register_form(headers: HeaderMap) -> PageResult {
    render(&AuthPage::new(lang_of(&headers), true))
}

async fn register(State(svc): Svc, headers: HeaderMap, Form(f): Form<AuthForm>) -> PageResult {
    let lang = lang_of(&headers);
    match svc.register(&f.email, &f.name, &f.password, lang, &f.base_currency).await {
        Ok(id) => Ok(signed_in(&svc, id)),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => {
            render(&AuthPage { name: f.name, email: f.email, error: Some(m), ..AuthPage::new(lang, true) })
        }
        Err(e) => Err(PageError(e)),
    }
}

async fn logout() -> Response {
    (AppendHeaders([set_cookie(TOKEN_COOKIE, "", 0), set_cookie(PROJECT_COOKIE, "", 0)]), Redirect::to("/login")).into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BackQuery {
    back: String,
}

/// Переключить язык: cookie + настройка пользователя (если он вошёл).
async fn set_lang(State(svc): Svc, headers: HeaderMap, Path(code): Path<String>, Query(q): Query<BackQuery>) -> Response {
    let lang = Lang::parse(&code).unwrap_or_default();
    if let Some(user) = cookie(&headers, TOKEN_COOKIE).and_then(|t| svc.verify(&t)) {
        let _ = svc.set_lang(user, lang).await;
    }
    let back = if q.back.starts_with('/') && !q.back.starts_with("//") { q.back } else { "/budget".into() };
    ([set_cookie(LANG_COOKIE, lang.code(), YEAR)], Redirect::to(&back)).into_response()
}

// ---------------------------------------------------------------- проекты

#[derive(Template)]
#[template(path = "projects/overview.html")]
struct OverviewPage {
    page: PageCtx,
    projects: Vec<ProjectView>,
    roles: &'static [&'static str],
    error: Option<String>,
}

async fn show(svc: &ProjectService, page: PageCtx, error: Option<String>) -> PageResult {
    let projects = svc.overview(page.session.user_id).await?;
    render(&OverviewPage { page: page.for_feature(crate::NAME, crate::i18n::dict), projects, roles: &Role::VALUES, error })
}

async fn overview(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    show(&svc, page, None).await
}

/// Ошибки валидации и прав показываем на той же странице.
async fn done(svc: &ProjectService, page: PageCtx, r: Result<(), AppError>, to: &str) -> PageResult {
    match r {
        Ok(()) => Ok(see_other(to)),
        Err(AppError::Validation(m) | AppError::Conflict(m) | AppError::Forbidden(m)) => show(svc, page, Some(m)).await,
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

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<CreateForm>) -> PageResult {
    let kind = ProjectKind::parse(&f.kind).unwrap_or(ProjectKind::Family);
    match svc.create_project(page.session.user_id, &f.name, kind, &f.base_currency).await {
        Ok(id) => Ok(([set_cookie(PROJECT_COOKIE, &id.to_string(), YEAR)], Redirect::to("/projects")).into_response()),
        Err(e) => done(&svc, page, Err(e), "/projects").await,
    }
}

async fn switch(Path(id): Path<Uuid>) -> Response {
    // Проверку членства сделает middleware на следующем запросе: чужой проект просто не выберется.
    ([set_cookie(PROJECT_COOKIE, &id.to_string(), YEAR)], Redirect::to("/budget")).into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct NameForm {
    name: String,
}

async fn rename(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<NameForm>) -> PageResult {
    let r = svc.rename(page.session.user_id, id, &f.name).await;
    done(&svc, page, r, "/projects").await
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MemberForm {
    email: String,
    role: String,
}

fn parse_role(v: &str) -> Result<Role, AppError> {
    Role::parse(v).ok_or_else(|| AppError::Validation(format!("role: '{v}'")))
}

async fn add_member(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<MemberForm>) -> PageResult {
    let r = async { svc.add_member(page.session.user_id, id, &f.email, parse_role(&f.role)?).await }.await;
    done(&svc, page, r, "/projects").await
}

async fn set_role(State(svc): Svc, Extension(page): Extension<PageCtx>, Path((id, user)): Path<(Uuid, Uuid)>, Form(f): Form<MemberForm>) -> PageResult {
    let r = async { svc.set_role(page.session.user_id, id, user, parse_role(&f.role)?).await }.await;
    done(&svc, page, r, "/projects").await
}

async fn remove_member(State(svc): Svc, Extension(page): Extension<PageCtx>, Path((id, user)): Path<(Uuid, Uuid)>) -> PageResult {
    let me = page.session.user_id;
    let r = svc.remove_member(me, id, user).await;
    // Вышел сам из активного проекта — сбросить выбор проекта.
    if r.is_ok() && me == user && id == page.session.project_id {
        return Ok(([set_cookie(PROJECT_COOKIE, "", 0)], Redirect::to("/projects")).into_response());
    }
    done(&svc, page, r, "/projects").await
}
