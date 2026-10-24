//! Фича «Сайт» (`site`): лендинг для гостя (`/`), правовые страницы (`/privacy`, `/terms`,
//! `/data-deletion`), `sitemap.xml`, `robots.txt`, манифест PWA. Своих таблиц нет.
//! Сгенерирована как blank, дальше — вручную (без feature.json: upgrade её не трогает).
//! Конверсия «визит → регистрация → первый расход» считается на сервере без персональных данных.

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

pub use domain::Funnel;
pub use ports::{Deps, FunnelSource};
/// Use cases фичи.
pub use service::SiteService as Service;

pub const NAME: &str = "site";
pub const MOUNT: &str = "/site";

#[derive(OpenApi)]
#[openapi()]
struct ApiDoc;

pub fn api(_ctx: &AppContext, deps: Deps) -> Arc<Service> {
    Arc::new(Service::new(deps))
}

/// Только публичные страницы: REST и страниц под авторизацией у фичи нет.
pub fn feature(ctx: &AppContext, api: Arc<Service>) -> Feature {
    Feature {
        name: NAME,
        mount: MOUNT,
        migrator: sqlx::migrate::Migrator::DEFAULT,
        router: Router::new(),
        openapi: ApiDoc::openapi(),
        web: Some(Web {
            pages: Router::new(),
            root: None,
            public: Some(adapters::web::public(api, ctx.config.clone())),
            account: None,
            nav: None,
        }),
    }
}
