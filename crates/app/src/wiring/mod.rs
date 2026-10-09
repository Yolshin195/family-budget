//! @managed rust-hex-api — сборка фич и связки между ними (composition root).
//! Блоки между маркерами @arch:* ведёт `arch.py`: каждая строка помечена `// @f:<фича>`.
//! Связки (адаптеры портов «потребитель → поставщик») лежат в wire_<потребитель>.rs.
//!
//! В этом проекте строки фич projects, transactions, planning, budget написаны вручную
//! (у этих фич нет feature.json — `upgrade` их не трогает): их порты шире, чем `exists`.

#[allow(unused_imports)]
use std::sync::Arc;

use kernel::context::AppContext;
use kernel::feature::Feature;

// @arch:mods:begin
#[cfg(feature = "projects")] pub mod wire_projects; // @f:projects
#[cfg(feature = "rates")] mod wire_rates; // @f:rates
#[cfg(feature = "accounts")] mod wire_accounts; // @f:accounts
#[cfg(feature = "transactions")] mod wire_transactions; // @f:transactions
#[cfg(feature = "limits")] mod wire_limits; // @f:limits
#[cfg(feature = "planning")] mod wire_planning; // @f:planning
#[cfg(feature = "budget")] mod wire_budget; // @f:budget
// @arch:mods:end

/// Собранные фичи + сервисы, которые нужны приложению напрямую (auth, демо-данные).
pub struct Wired {
    pub features: Vec<Feature>,
    pub projects: Arc<projects::Service>,
    #[cfg(feature = "seed")]
    pub seed: crate::seed::Services,
}

#[rustfmt::skip]
pub fn features(ctx: &AppContext) -> Wired {
    #[allow(unused_mut)]
    let mut list: Vec<Feature> = Vec::new();
    let _ = ctx;
    // @arch:build:begin
    #[cfg(feature = "currencies")] let currencies_api = currencies::api(ctx, currencies::Deps {}); // @f:currencies
    #[cfg(feature = "categories")] let categories_api = categories::api(ctx, categories::Deps {}); // @f:categories
    #[cfg(feature = "projects")] let projects_api = projects::api(ctx, projects::Deps { bootstrap: Arc::new(wire_projects::BootstrapAdapter { currencies: currencies_api.clone(), categories: categories_api.clone() }) }); // @f:projects
    #[cfg(feature = "projects")] list.push(projects::feature(projects_api.clone())); // @f:projects
    #[cfg(feature = "currencies")] list.push(currencies::feature(currencies_api.clone())); // @f:currencies
    #[cfg(feature = "rates")] let rates_api = rates::api(ctx, rates::Deps { currencies: Arc::new(wire_rates::CurrenciesLookupAdapter(currencies_api.clone())) }); // @f:rates
    #[cfg(feature = "rates")] list.push(rates::feature(rates_api.clone())); // @f:rates
    #[cfg(feature = "accounts")] let accounts_api = accounts::api(ctx, accounts::Deps { currencies: Arc::new(wire_accounts::CurrenciesLookupAdapter(currencies_api.clone())) }); // @f:accounts
    #[cfg(feature = "accounts")] list.push(accounts::feature(accounts_api.clone())); // @f:accounts
    #[cfg(feature = "categories")] list.push(categories::feature(categories_api.clone())); // @f:categories
    #[cfg(feature = "transactions")] let transactions_api = transactions::api(ctx, transactions::Deps { accounts: Arc::new(wire_transactions::AccountsLookupAdapter(accounts_api.clone())), categories: Arc::new(wire_transactions::CategoriesLookupAdapter(categories_api.clone())) }); // @f:transactions
    #[cfg(feature = "transactions")] list.push(transactions::feature(transactions_api.clone())); // @f:transactions
    #[cfg(feature = "limits")] let limits_api = limits::api(ctx, limits::Deps { categories: Arc::new(wire_limits::CategoriesLookupAdapter(categories_api.clone())) }); // @f:limits
    #[cfg(feature = "limits")] list.push(limits::feature(limits_api.clone())); // @f:limits
    #[cfg(feature = "planning")] let planning_api = planning::api(ctx, planning::Deps { payments: Arc::new(wire_planning::PaymentsAdapter(transactions_api.clone())), accounts: Arc::new(wire_planning::AccountsAdapter(accounts_api.clone())), categories: Arc::new(wire_planning::CategoriesAdapter(categories_api.clone())) }); // @f:planning
    #[cfg(feature = "planning")] list.push(planning::feature(planning_api.clone())); // @f:planning
    #[cfg(feature = "budget")] let budget_api = budget::api(ctx, budget::Deps { categories: Arc::new(wire_budget::CategoriesAdapter(categories_api.clone())), facts: Arc::new(wire_budget::FactsAdapter(transactions_api.clone())), plans: Arc::new(wire_budget::PlansAdapter(planning_api.clone())), limits: Arc::new(wire_budget::LimitsAdapter(limits_api.clone())), accounts: Arc::new(wire_budget::AccountsAdapter(accounts_api.clone())), rates: Arc::new(wire_budget::RatesAdapter(rates_api.clone())) }); // @f:budget
    #[cfg(feature = "budget")] list.push(budget::feature(budget_api)); // @f:budget
    // @arch:build:end
    Wired {
        features: list,
        #[cfg(feature = "seed")]
        seed: crate::seed::Services {
            projects: projects_api.clone(),
            currencies: currencies_api,
            rates: rates_api,
            accounts: accounts_api,
            transactions: transactions_api,
            limits: limits_api,
            planning: planning_api,
        },
        projects: projects_api,
    }
}
