//! Фича «MCP для ассистентов» (`mcp`): MCP-сервер (`/mcp`, Streamable HTTP) с инструментами бюджета
//! для Claude и других AI-ассистентов, вход по личному API-токену или по access-токену провайдера
//! входа (OAuth, RFC 8707), доступы ассистентов (проект и права) и страница их настройки.
//! Написана вручную (без feature.json).
//!
//! Провайдер входа (сейчас Rauthy) скрыт за портом `Accounts`: при переезде на другой провайдер
//! меняется только адаптер проверки токенов в фиче `projects`, MCP-часть остаётся как есть.

mod adapters;
mod domain;
mod dto;
mod i18n;
mod ports;
mod service;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::Router;
use kernel::context::AppContext;
use kernel::feature::Feature;
use kernel::web::Web;

pub use domain::{
    Account, AccountInfo, BudgetLine, BudgetReport, Category, CategoryInfo, Currency, Limit, Name, Occurrence, Plan, PlanDraft, QuickInfo, Rate, Tx, TxDraft, TxFilter, TxItem,
};
pub use ports::{Accounts, Budget, Deps, Directories, Ledger, Limits, Plans, Rates};
pub use service::{McpService as Service, Settings};

pub const NAME: &str = "mcp";
/// Страница настройки доступа ассистентов (под входом).
pub const MOUNT: &str = "/assistants";

pub fn api(ctx: &AppContext, deps: Deps) -> Arc<Service> {
    let repo = adapters::postgres::PostgresGrantRepository::new(ctx.pool.clone());
    Arc::new(Service::new(Arc::new(repo), deps, Settings::from_config(&ctx.config)))
}

pub fn feature(ctx: &AppContext, api: Arc<Service>) -> Feature {
    Feature {
        name: NAME,
        mount: MOUNT,
        migrator: sqlx::migrate!("./migrations"),
        // REST API у фичи нет: `/mcp` и метаданные — публичный роутер со своим входом по токену.
        router: Router::new(),
        openapi: utoipa::openapi::OpenApiBuilder::new().build(),
        web: Some(Web {
            pages: adapters::web::router(api.clone()),
            root: None,
            public: Some(adapters::http::router(api, !ctx.config.is_prod())),
            account: None,
            nav: None,
        }),
    }
}
