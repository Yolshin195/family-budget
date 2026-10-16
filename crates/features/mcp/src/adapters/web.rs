//! Входящий адаптер: страница «Ассистенты (MCP)» — доступ Claude к проекту и правам, подключение.
//! Права меняются только здесь: под cookie-сессией браузера, с CSRF; токен ассистента сюда не пускают.

use std::sync::Arc;

use askama::Template;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Extension, Router};
use kernel::auth::{Identity, RequestMeta, Role, Via as AuthVia};
use kernel::error::AppError;
use kernel::web::{Flash, PageCtx, PageError, PageResult, render, see_other_flash};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::{ALL_DIRECTORIES, DEFAULT_GRANT_SCOPES, DIRECTORY_SCOPES, GRANT_SCOPES, Grant, scope_group};
use crate::service::McpService;

type Svc = State<Arc<McpService>>;

pub fn router(svc: Arc<McpService>) -> Router {
    Router::new().route("/", get(show)).route("/grant", post(save)).route("/revoke", post(revoke)).with_state(svc)
}

struct ProjectOption {
    id: Uuid,
    name: String,
    role: &'static str,
    can_write: bool,
}

/// Блок прав на странице: «base» (просмотр и операции), «directories», «limits», «planning».
struct ScopeGroup {
    key: &'static str,
    scopes: Vec<&'static str>,
}

/// Права из `GRANT_SCOPES` по блокам, в порядке первого появления блока.
fn scope_groups() -> Vec<ScopeGroup> {
    let mut groups: Vec<ScopeGroup> = Vec::new();
    for s in GRANT_SCOPES {
        let key = scope_group(s);
        match groups.iter_mut().find(|g| g.key == key) {
            Some(g) => g.scopes.push(s),
            None => groups.push(ScopeGroup { key, scopes: vec![s] }),
        }
    }
    groups
}

struct GrantView {
    project: String,
    scopes: Vec<String>,
    created: String,
    last_used: String,
}

#[derive(Template)]
#[template(path = "mcp/assistants.html")]
struct AssistantsPage {
    page: PageCtx,
    mcp_url: String,
    client_id: String,
    oauth_ready: bool,
    grant: Option<GrantView>,
    projects: Vec<ProjectOption>,
    selected_project: Uuid,
    /// Есть проекты с ролью VIEWER — подсказка «только просмотр».
    has_viewer: bool,
    selected: Vec<String>,
    groups: Vec<ScopeGroup>,
    /// Значение флажка «Справочники».
    all_directories: &'static str,
    guest: bool,
    error: Option<String>,
}

impl AssistantsPage {
    fn checked(&self, scope: &&str) -> bool {
        self.selected.iter().any(|s| s == *scope)
    }

    /// Флажок «Справочники» отмечен, когда выданы все права справочников.
    fn all_directories_checked(&self) -> bool {
        DIRECTORY_SCOPES.iter().all(|d| self.checked(d))
    }
}

fn ctx(page: PageCtx) -> PageCtx {
    let profile = kernel::i18n::t(page.lang, Some(crate::i18n::dict), "profile");
    let security = kernel::i18n::t(page.lang, Some(crate::i18n::dict), "security");
    page.for_feature("profile", crate::i18n::dict).crumb(profile, "/profile").crumb(security, "/profile/security")
}

async fn render_page(svc: &McpService, page: PageCtx, error: Option<String>) -> PageResult {
    let s = svc.settings();
    let grant: Option<Grant> = svc.grants(page.session.user_id).await?.into_iter().find(|g| g.client_id == s.oauth_client_id);
    let page = ctx(page);
    let name_of = |id: Uuid| page.session.projects.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_else(|| page.t("project_gone"));
    let projects: Vec<ProjectOption> = page
        .session
        .projects
        .iter()
        .map(|p| ProjectOption { id: p.id, name: p.name.clone(), role: p.role.as_str(), can_write: p.role >= Role::Editor })
        .collect();
    let has_viewer = projects.iter().any(|p| !p.can_write);
    let view = grant.as_ref().map(|g| GrantView {
        project: name_of(g.project_id),
        scopes: g.scopes.iter().map(|x| page.t(&format!("scope.{x}"))).collect(),
        created: page.date(&g.created_at.with_timezone(&chrono::Local).date_naive()),
        last_used: g.last_used_at.map(|t| page.date(&t.with_timezone(&chrono::Local).date_naive())).unwrap_or_else(|| page.t("never")),
    });
    let selected_project = grant.as_ref().map(|g| g.project_id).unwrap_or(page.session.project_id);
    let selected = grant.as_ref().map(|g| g.scopes.clone()).unwrap_or_else(|| DEFAULT_GRANT_SCOPES.iter().map(|x| x.to_string()).collect());
    let guest = svc.is_guest(page.session.user_id).await?;
    render(&AssistantsPage {
        mcp_url: s.resource.clone(),
        client_id: s.oauth_client_id.clone(),
        oauth_ready: svc.issuer().await.is_some(),
        grant: view,
        projects,
        selected_project,
        has_viewer,
        selected,
        groups: scope_groups(),
        all_directories: ALL_DIRECTORIES,
        guest,
        error,
        page,
    })
}

async fn show(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    render_page(&svc, page, None).await
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GrantForm {
    project: String,
    scope: Vec<String>,
}

/// Только из браузера: запрос по API-токену (или токену ассистента) права не меняет.
fn browser_only(identity: &Identity) -> Result<(), AppError> {
    match identity.via {
        AuthVia::Session { .. } => Ok(()),
        AuthVia::Token { .. } => Err(AppError::Forbidden("доступ ассистентов меняется только в браузере".into())),
    }
}

async fn save(
    State(svc): Svc,
    Extension(page): Extension<PageCtx>,
    Extension(identity): Extension<Identity>,
    Extension(meta): Extension<RequestMeta>,
    axum_extra::extract::Form(f): axum_extra::extract::Form<GrantForm>,
) -> PageResult {
    browser_only(&identity)?;
    let s = svc.settings().clone();
    let Some(project) = Uuid::parse_str(&f.project).ok().and_then(|id| page.session.projects.iter().find(|p| p.id == id)).cloned() else {
        let msg = page.t("no_such_project");
        return render_page(&svc, page, Some(msg)).await;
    };
    // «Требовать 2FA» — свойство проекта; у активного проекта оно в сессии, у другого — уточняем у сервиса.
    let require_2fa = if project.id == page.session.project_id { page.session.require_2fa } else { svc.require_2fa(page.session.user_id, project.id).await? };
    match svc.save_grant(page.session.user_id, &s.oauth_client_id, project.id, project.role, require_2fa, identity.mfa, &f.scope, &meta).await {
        Ok(_) => {
            let msg = page.t("saved");
            Ok(see_other_flash(&page.config, "/assistants", Flash::ok(msg)))
        }
        Err(AppError::Forbidden(m) | AppError::Validation(m)) => render_page(&svc, page, Some(m)).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn revoke(State(svc): Svc, Extension(page): Extension<PageCtx>, Extension(identity): Extension<Identity>, Extension(meta): Extension<RequestMeta>) -> PageResult {
    browser_only(&identity)?;
    let client = svc.settings().oauth_client_id.clone();
    svc.revoke_grant(page.session.user_id, &client, &meta).await?;
    let msg = page.t("revoked");
    Ok(see_other_flash(&page.config, "/assistants", Flash::ok(msg)))
}
