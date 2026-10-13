//! Фича «Планирование» (`planning`): плановые и регулярные операции, календарь, подтверждение оплаты.
//! Написана вручную (без feature.json): upgrade эталона её не перегенерирует.
//!
//! Гексагональная схема: adapters/{http,web} → service → ports ← adapters/postgres | adapters/memory; domain — чистые правила.
//! Оплата вхождения хранится в транзакции (plan_id + plan_date) — фича читает её через порт `PaymentsLookup`.

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
use kernel::web::{NavGroup, NavItem, Web};
use utoipa::OpenApi;

pub use dto::{OccurrenceDto, PlanDto};
pub use ports::{AccountsLookup, CategoriesLookup, Deps, PaidRef, PaymentsLookup};
/// Use cases фичи.
pub use service::PlanningService as Service;

pub const NAME: &str = "planning";
pub const MOUNT: &str = "/planning";

pub fn api(ctx: &AppContext, deps: Deps) -> Arc<Service> {
    let repo = adapters::postgres::PostgresPlanRepository::new(ctx.pool.clone());
    Arc::new(Service::new(Arc::new(repo), deps))
}

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
            nav: Some(NavItem { feature: NAME, href: MOUNT, label: i18n::dict("nav").unwrap_or((NAME, NAME, NAME)), icon: "calendar-days", group: NavGroup::Main, order: 30, mobile: true }),
            root: None,
            account: None,
        }),
    }
}
