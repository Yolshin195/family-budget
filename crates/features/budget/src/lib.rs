//! Фича «Бюджет» (`budget`): план, факт и лимиты по дереву категорий за неделю, месяц, год.
//! Своих таблиц нет: факты, планы, лимиты, счета и курсы читаются из других фич через порты.
//! Написана вручную (без feature.json).

mod adapters;
mod domain;
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

pub use domain::{LimitRule, Node, View};
pub use ports::{AccountRec, AccountsSource, CategoriesSource, Deps, ExchangeRec, FactRec, FactsSource, LimitsSource, PlannedRec, PlansSource, RatesSource};
pub use service::{BudgetService as Service, CategoryLimit};

pub const NAME: &str = "budget";
pub const MOUNT: &str = "/budget";

pub fn api(_ctx: &AppContext, deps: Deps) -> Arc<Service> {
    Arc::new(Service::new(deps))
}

pub fn feature(api: Arc<Service>) -> Feature {
    Feature {
        name: NAME,
        mount: MOUNT,
        migrator: sqlx::migrate::Migrator::DEFAULT,
        router: adapters::http::router(api.clone()),
        openapi: adapters::http::ApiDoc::openapi(),
        web: Some(Web {
            pages: adapters::web::router(api),
            public: None,
            nav: Some(NavItem { feature: NAME, href: MOUNT, label: i18n::dict("nav").unwrap_or((NAME, NAME, NAME)), icon: "chart-pie", group: NavGroup::Main, order: 10, mobile: true }),
            root: None,
            account: None,
        }),
    }
}
