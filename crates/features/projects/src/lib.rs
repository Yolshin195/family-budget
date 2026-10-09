//! Фича «Проекты» (`projects`): пользователи, проекты (бюджеты) и участники с ролями; вход и регистрация.
//! Сервис реализует порт ядра `kernel::auth::Memberships` — по нему middleware выбирает проект и роль.
//! Написана вручную (без feature.json). Регистрируется первой: её таблицы нужны остальным при старте (seed).

mod adapters;
mod domain;
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
use utoipa::OpenApi;

pub use domain::ProjectKind;
pub use ports::{Deps, ProjectBootstrap};
pub use service::ProjectService as Service;

pub const NAME: &str = "projects";
pub const MOUNT: &str = "/projects";

pub fn api(ctx: &AppContext, deps: Deps) -> Arc<Service> {
    let repo = adapters::postgres::PostgresProjectRepository::new(ctx.pool.clone());
    Arc::new(Service::new(Arc::new(repo), deps, ctx.tokens.clone()))
}

pub fn feature(api: Arc<Service>) -> Feature {
    Feature {
        name: NAME,
        mount: MOUNT,
        migrator: sqlx::migrate!("./migrations"),
        router: adapters::http::router(api.clone()),
        openapi: adapters::http::ApiDoc::openapi(),
        web: Some(Web {
            pages: adapters::web::pages(api.clone()),
            public: Some(Router::new().merge(adapters::web::public(api.clone())).merge(adapters::http::public(api))),
            nav: None,
        }),
    }
}
