//! Фича «Проекты» (`projects`): пользователи (вход через Rauthy), проекты и участники с ролями,
//! серверные сессии, личные API-токены, журнал безопасности, онбординг и профиль.
//! Сервис реализует порты ядра `kernel::auth::Memberships` и `kernel::auth::Authenticator` —
//! по ним middleware определяет пользователя, проект и роль.
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
pub use ports::{Deps, IdentityProvider, MemberDirectory, ProjectBootstrap, ProjectPurge};
pub use service::{GUEST_SUB, ProjectService as Service};

pub const NAME: &str = "projects";
pub const MOUNT: &str = "/projects";

/// Провайдер входа по настройкам: Rauthy (OIDC) или «выключен» (разработка без Rauthy).
pub fn identity_provider(ctx: &AppContext) -> Arc<dyn IdentityProvider> {
    match &ctx.config.oidc {
        Some(o) => Arc::new(adapters::oidc::RauthyProvider::new(o.clone())),
        None => Arc::new(adapters::oidc::DisabledProvider),
    }
}

pub fn api(ctx: &AppContext, deps: Deps) -> Arc<Service> {
    let repo = Arc::new(adapters::postgres::PostgresProjectRepository::new(ctx.pool.clone()));
    Arc::new(Service::new(repo.clone(), repo, deps, ctx.config.clone()))
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
            root: Some(adapters::web::root(api.clone())),
            public: Some(Router::new().merge(adapters::web::public(api.clone()))),
            account: Some(adapters::web::account(api)),
            nav: None,
        }),
    }
}
