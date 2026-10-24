//! @managed rust-hex-api — контракт фичи и сборка всех включённых фич.
//! Отличие от эталона v1: у фичи есть веб-часть (`web`) — страницы Askama, публичные страницы и пункт меню.
use axum::Router;
use sqlx::migrate::Migrator;
use utoipa::openapi::{OpenApi, OpenApiBuilder};

use crate::context::AppContext;
use crate::web::{NavItem, Web};

/// То, что каждая фича отдаёт приложению.
pub struct Feature {
    pub name: &'static str,
    /// Префикс маршрутов, например "/currencies". REST — под `/api<mount>`, страницы — под `<mount>`.
    pub mount: &'static str,
    /// Миграции фичи (`crates/features/<name>/migrations`).
    pub migrator: Migrator,
    /// REST-роуты фичи; приложение монтирует их под auth-слоем в `/api<mount>`.
    pub router: Router,
    pub openapi: OpenApi,
    /// Веб-страницы фичи (нет — фича только с API).
    pub web: Option<Web>,
}

pub struct Assembled {
    /// REST всех фич (без префикса `/api`).
    pub router: Router,
    /// Страницы всех фич (под авторизацией).
    pub pages: Router,
    /// Публичные страницы (лендинг, вход, правовые страницы).
    pub public: Router,
    /// Страницы аккаунта: вход есть, проекта может не быть (онбординг).
    pub account: Router,
    pub nav: Vec<NavItem>,
    pub openapi: OpenApi,
    pub names: Vec<&'static str>,
}

/// Прогоняет миграции фич (в порядке регистрации), монтирует роуты, сливает OpenAPI и меню.
pub async fn assemble(ctx: &AppContext, features: Vec<Feature>) -> anyhow::Result<Assembled> {
    let mut router = Router::new();
    let mut pages = Router::new();
    let mut public = Router::new();
    let mut account = Router::new();
    let mut nav = Vec::new();
    let mut openapi = OpenApiBuilder::new().build();
    let mut names = Vec::new();
    for f in features {
        let mut migrator = f.migrator;
        // Все фичи пишут в общую _sqlx_migrations — чужие версии игнорируем.
        migrator.set_ignore_missing(true);
        migrator.run(&ctx.pool).await.map_err(|e| anyhow::anyhow!("миграции фичи '{}': {e}", f.name))?;
        router = router.nest(f.mount, f.router);
        if let Some(web) = f.web {
            pages = pages.nest(f.mount, web.pages);
            if let Some(r) = web.root {
                pages = pages.merge(r);
            }
            if let Some(p) = web.public {
                public = public.merge(p);
            }
            if let Some(a) = web.account {
                account = account.merge(a);
            }
            nav.extend(web.nav);
        }
        openapi.merge(f.openapi);
        names.push(f.name);
    }
    nav.sort_by_key(|n| (n.group as u8, n.order));
    Ok(Assembled { router, pages, public, account, nav, openapi, names })
}
