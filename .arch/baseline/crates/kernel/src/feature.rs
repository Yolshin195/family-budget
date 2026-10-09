//! @managed rust-hex-api — контракт фичи и сборка всех включённых фич.
use axum::Router;
use sqlx::migrate::Migrator;
use utoipa::openapi::{OpenApi, OpenApiBuilder};

use crate::context::AppContext;

/// То, что каждая фича отдаёт приложению.
pub struct Feature {
    pub name: &'static str,
    /// Префикс маршрутов, например "/currencies".
    pub mount: &'static str,
    /// Миграции фичи (`crates/features/<name>/migrations`).
    pub migrator: Migrator,
    /// Роуты фичи; приложение монтирует их под auth-слоем.
    pub router: Router,
    pub openapi: OpenApi,
}

pub struct Assembled {
    pub router: Router,
    pub openapi: OpenApi,
    pub names: Vec<&'static str>,
}

/// Прогоняет миграции фич (в порядке регистрации), монтирует роуты, сливает OpenAPI.
pub async fn assemble(ctx: &AppContext, features: Vec<Feature>) -> anyhow::Result<Assembled> {
    let mut router = Router::new();
    let mut openapi = OpenApiBuilder::new().build();
    let mut names = Vec::new();
    for f in features {
        let mut migrator = f.migrator;
        // Все фичи пишут в общую _sqlx_migrations — чужие версии игнорируем.
        migrator.set_ignore_missing(true);
        migrator
            .run(&ctx.pool)
            .await
            .map_err(|e| anyhow::anyhow!("миграции фичи '{}': {e}", f.name))?;
        router = router.nest(f.mount, f.router);
        openapi.merge(f.openapi);
        names.push(f.name);
    }
    Ok(Assembled { router, openapi, names })
}
