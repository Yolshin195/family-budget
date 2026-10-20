//! Фича «Люди» (`people`, группа 1, шаг 6): для кого покупка — участники проекта и люди без аккаунта.
//! Позиция операции ссылается на человека («Для кого»); пусто — «Для всех». Написана вручную по образцу
//! places: справочник с ключом `code`, архив вместо удаления. Поля и эндпоинты — FEATURE.md.
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

/// Use cases фичи. Другие фичи получают к ним доступ только через свои порты и адаптеры в app/wiring.
pub use service::PersonService as Service;

pub const NAME: &str = "people";
pub const MOUNT: &str = "/people";

/// Собрать сервис фичи (composition root фичи: выбор адаптеров). Зависимостей от других фич нет.
pub fn api(ctx: &AppContext) -> Arc<Service> {
    let repo = adapters::postgres::PostgresPersonRepository::new(ctx.pool.clone());
    Arc::new(Service::new(Arc::new(repo)))
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
            nav: Some(NavItem { feature: NAME, href: MOUNT, label: i18n::dict("nav").unwrap_or((NAME, NAME, NAME)), icon: "users", group: NavGroup::Directory, order: 27, mobile: false }),
            root: None,
            account: None,
        }),
    }
}
