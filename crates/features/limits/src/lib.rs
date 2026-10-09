//! @generated rust-hex-api v1 (journal). Правки допустимы — при upgrade они сливаются 3-way merge.
//! Фича «Лимиты» (`limits`): журнал (записи с датой, ключ — id). Поля и эндпоинты — FEATURE.md, спецификация — feature.json.
//!
//! Гексагональная схема: adapters/http → service → ports ← adapters/postgres | adapters/memory; domain — чистые правила.
//! Публичный API крейта (то, что видят приложение и связки с другими фичами) — только ниже.

mod adapters;
mod domain;
pub mod dto;
mod i18n;
mod ports;
mod service;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use kernel::context::AppContext;
use kernel::feature::Feature;
use kernel::web::{NavItem, Web};
use utoipa::OpenApi;

pub use ports::Deps;
pub use ports::CategoriesLookup;
/// Use cases фичи. Другие фичи получают к ним доступ только через свои порты и адаптеры в app/wiring.
pub use service::LimitService as Service;

pub const NAME: &str = "limits";
pub const MOUNT: &str = "/limits";

/// Собрать сервис фичи (composition root фичи: выбор адаптеров).
pub fn api(ctx: &AppContext, deps: Deps) -> Arc<Service> {
    let repo = adapters::postgres::PostgresLimitRepository::new(ctx.pool.clone());
    Arc::new(Service::new(Arc::new(repo), deps))
}

/// HTTP-часть и миграции фичи для приложения.
pub fn feature(api: Arc<Service>) -> Feature {
    Feature {
        name: NAME,
        mount: MOUNT,
        migrator: sqlx::migrate!("./migrations"),
        router: adapters::http::router(api.clone()),
        openapi: adapters::http::ApiDoc::openapi(),
        web: Some(Web {
            pages: adapters::web::router(api),
            public: None,
            nav: Some(NavItem { feature: NAME, href: MOUNT, label: i18n::dict("nav").unwrap_or((NAME, NAME, NAME)), order: 40 }),
        }),
    }
}
