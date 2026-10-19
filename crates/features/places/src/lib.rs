//! Фича «Места» (`places`, группа 1, шаг 5): магазины, кафе и рынки. Место подставляет в операцию
//! категорию и счёт и ищется по названию и вариантам написания. Написана вручную по образцу categories:
//! справочник с ключом `code`, архив вместо удаления. Поля и эндпоинты — FEATURE.md.
//!
//! Гексагональная схема: adapters/http → service → ports ← adapters/postgres | adapters/memory; domain — чистые правила.

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

pub use ports::{Deps, Directories};
/// Use cases фичи. Другие фичи получают к ним доступ только через свои порты и адаптеры в app/wiring.
pub use service::PlaceService as Service;

pub const NAME: &str = "places";
pub const MOUNT: &str = "/places";

/// Собрать сервис фичи (composition root фичи: выбор адаптеров).
pub fn api(ctx: &AppContext, deps: Deps) -> Arc<Service> {
    let repo = adapters::postgres::PostgresPlaceRepository::new(ctx.pool.clone());
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
            nav: Some(NavItem { feature: NAME, href: MOUNT, label: i18n::dict("nav").unwrap_or((NAME, NAME, NAME)), icon: "map-pin", group: NavGroup::Directory, order: 25, mobile: false }),
            root: None,
            account: None,
        }),
    }
}
