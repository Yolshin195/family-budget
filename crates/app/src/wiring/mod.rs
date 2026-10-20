//! @managed rust-hex-api — сборка фич и связки между ними (composition root).
//! Блоки между маркерами @arch:* ведёт `arch.py`: каждая строка помечена `// @f:<фича>`.
//! Связки (адаптеры портов «потребитель → поставщик») лежат в wire_<потребитель>.rs.
//!
//! В этом проекте строки фич projects, currencies, accounts, categories, places, people, transactions, planning, budget, mcp
//! написаны вручную (у этих фич нет feature.json — `upgrade` их не трогает): их порты шире, чем `exists`.

#[allow(unused_imports)]
use std::sync::Arc;

use kernel::context::AppContext;
use kernel::feature::Feature;

// @arch:mods:begin
#[cfg(feature = "projects")] pub mod wire_projects; // @f:projects
#[cfg(feature = "currencies")] mod wire_currencies; // @f:currencies
#[cfg(feature = "categories")] mod wire_categories; // @f:categories
#[cfg(feature = "rates")] mod wire_rates; // @f:rates
#[cfg(feature = "accounts")] mod wire_accounts; // @f:accounts
#[cfg(feature = "places")] mod wire_places; // @f:places
#[cfg(feature = "transactions")] mod wire_transactions; // @f:transactions
#[cfg(feature = "limits")] mod wire_limits; // @f:limits
#[cfg(feature = "planning")] mod wire_planning; // @f:planning
#[cfg(feature = "budget")] mod wire_budget; // @f:budget
#[cfg(feature = "site")] mod wire_site; // @f:site
#[cfg(feature = "mcp")] mod wire_mcp; // @f:mcp
// @arch:mods:end

/// Собранные фичи + сервисы, которые нужны приложению напрямую (auth, демо-данные).
pub struct Wired {
    pub features: Vec<Feature>,
    pub projects: Arc<projects::Service>,
    /// Сессия страницы: проект и роль + символы валют проекта.
    pub memberships: Arc<dyn kernel::auth::Memberships>,
    #[cfg(feature = "site")]
    pub site: Arc<site::Service>,
    #[cfg(feature = "seed")]
    pub seed: crate::seed::Services,
}

#[rustfmt::skip]
pub fn features(ctx: &AppContext) -> Wired {
    #[allow(unused_mut)]
    let mut list: Vec<Feature> = Vec::new();
    let _ = ctx;
    // @arch:build:begin
    #[cfg(feature = "currencies")] let currency_usage = wire_currencies::UsageAdapter::default(); // @f:currencies
    #[cfg(feature = "currencies")] let currencies_api = currencies::api(ctx, currencies::Deps { usage: Arc::new(currency_usage.clone()) }); // @f:currencies
    #[cfg(feature = "categories")] let category_usage = wire_categories::UsageAdapter::default(); // @f:categories
    #[cfg(feature = "categories")] let categories_api = categories::api(ctx, categories::Deps { usage: Arc::new(category_usage.clone()) }); // @f:categories
    #[cfg(feature = "people")] let people_api = people::api(ctx); // @f:people
    #[cfg(feature = "projects")] let purge = wire_projects::PurgeAdapter::default(); // @f:projects
    #[cfg(feature = "projects")] let bootstrap = wire_projects::BootstrapAdapter::new(currencies_api.clone(), categories_api.clone()); // @f:projects
    #[cfg(feature = "projects")] let projects_api = projects::api(ctx, projects::Deps { bootstrap: Arc::new(bootstrap.clone()), purge: Arc::new(purge.clone()), identity: projects::identity_provider(ctx), people: Arc::new(wire_projects::MemberDirectoryAdapter { #[cfg(feature = "people")] people: Some(people_api.clone()) }) }); // @f:projects
    #[cfg(feature = "projects")] list.push(projects::feature(projects_api.clone())); // @f:projects
    #[cfg(feature = "currencies")] list.push(currencies::feature(currencies_api.clone())); // @f:currencies
    #[cfg(feature = "rates")] let rates_api = rates::api(ctx, rates::Deps { currencies: Arc::new(wire_rates::CurrenciesLookupAdapter(currencies_api.clone())) }); // @f:rates
    #[cfg(feature = "rates")] list.push(rates::feature(rates_api.clone())); // @f:rates
    #[cfg(feature = "accounts")] let ledger = wire_accounts::LedgerAdapter::default(); // @f:accounts
    #[cfg(feature = "accounts")] let accounts_api = accounts::api(ctx, accounts::Deps { currencies: Arc::new(wire_accounts::CurrenciesLookupAdapter(currencies_api.clone())), ledger: Arc::new(ledger.clone()) }); // @f:accounts
    #[cfg(feature = "accounts")] list.push(accounts::feature(accounts_api.clone())); // @f:accounts
    #[cfg(feature = "categories")] list.push(categories::feature(categories_api.clone())); // @f:categories
    #[cfg(feature = "places")] let places_api = places::api(ctx, places::Deps { directories: Arc::new(wire_places::DirectoriesAdapter { categories: categories_api.clone(), accounts: accounts_api.clone() }) }); // @f:places
    #[cfg(feature = "places")] list.push(places::feature(places_api.clone())); // @f:places
    #[cfg(feature = "people")] list.push(people::feature(people_api.clone())); // @f:people
    #[cfg(feature = "transactions")] let budget_view = wire_transactions::BudgetViewAdapter::default(); // @f:transactions
    #[cfg(feature = "transactions")] let plans_lookup = wire_transactions::PlansLookupAdapter::default(); // @f:transactions
    #[cfg(feature = "transactions")] let transactions_api = transactions::api(ctx, transactions::Deps { accounts: Arc::new(wire_transactions::AccountsLookupAdapter(accounts_api.clone(), currencies_api.clone())), categories: Arc::new(wire_transactions::CategoriesLookupAdapter(categories_api.clone())), budget: Arc::new(budget_view.clone()), plans: Arc::new(plans_lookup.clone()), places: Arc::new(wire_transactions::PlacesLookupAdapter { #[cfg(feature = "places")] places: Some(places_api.clone()) }), people: Arc::new(wire_transactions::PeopleLookupAdapter { #[cfg(feature = "people")] people: Some(people_api.clone()) }) }); // @f:transactions
    #[cfg(feature = "transactions")] let _ = ledger.0.set(transactions_api.clone()); // @f:transactions
    #[cfg(feature = "transactions")] list.push(transactions::feature(transactions_api.clone())); // @f:transactions
    #[cfg(feature = "limits")] let limits_api = limits::api(ctx, limits::Deps { categories: Arc::new(wire_limits::CategoriesLookupAdapter(categories_api.clone())) }); // @f:limits
    #[cfg(feature = "limits")] list.push(limits::feature(limits_api.clone())); // @f:limits
    #[cfg(feature = "planning")] let planning_api = planning::api(ctx, planning::Deps { payments: Arc::new(wire_planning::PaymentsAdapter(transactions_api.clone())), accounts: Arc::new(wire_planning::AccountsAdapter(accounts_api.clone())), categories: Arc::new(wire_planning::CategoriesAdapter(categories_api.clone())), rates: Arc::new(wire_planning::RatesAdapter(rates_api.clone())) }); // @f:planning
    #[cfg(feature = "planning")] let _ = plans_lookup.0.set(planning_api.clone()); // @f:planning
    #[cfg(feature = "planning")] list.push(planning::feature(planning_api.clone())); // @f:planning
    #[cfg(feature = "budget")] let budget_api = budget::api(ctx, budget::Deps { categories: Arc::new(wire_budget::CategoriesAdapter(categories_api.clone())), facts: Arc::new(wire_budget::FactsAdapter(transactions_api.clone())), plans: Arc::new(wire_budget::PlansAdapter(planning_api.clone())), limits: Arc::new(wire_budget::LimitsAdapter(limits_api.clone())), accounts: Arc::new(wire_budget::AccountsAdapter(accounts_api.clone())), rates: Arc::new(wire_budget::RatesAdapter(rates_api.clone())) }); // @f:budget
    #[cfg(feature = "budget")] let _ = budget_view.0.set(budget_api.clone()); // @f:budget
    #[cfg(feature = "budget")] list.push(budget::feature(budget_api.clone())); // @f:budget
    #[cfg(feature = "site")] let site_api = site::api(ctx, site::Deps { funnel: Arc::new(wire_site::FunnelAdapter { projects: projects_api.clone(), #[cfg(feature = "transactions")] transactions: transactions_api.clone() }) }); // @f:site
    #[cfg(feature = "site")] list.push(site::feature(ctx, site_api.clone())); // @f:site
    #[cfg(feature = "mcp")] let mcp_api = mcp::api(ctx, mcp::Deps { ledger: Arc::new(wire_mcp::LedgerAdapter(transactions_api.clone())), budget: Arc::new(wire_mcp::BudgetAdapter(budget_api.clone())), plans: Arc::new(wire_mcp::PlansAdapter(planning_api.clone())), directories: Arc::new(wire_mcp::DirectoriesAdapter { currencies: currencies_api.clone(), accounts: accounts_api.clone(), categories: categories_api.clone(), places: places_api.clone(), people: people_api.clone() }), rates: Arc::new(wire_mcp::RatesAdapter(rates_api.clone())), limits: Arc::new(wire_mcp::LimitsAdapter(limits_api.clone())), accounts: Arc::new(wire_mcp::AccountsAdapter(projects_api.clone())), authenticator: projects_api.clone(), memberships: projects_api.clone() }); // @f:mcp
    #[cfg(feature = "mcp")] list.push(mcp::feature(ctx, mcp_api.clone())); // @f:mcp
    // @arch:build:end
    // Где используются валюты и категории (этап 0, Ц2 и Ц8): фичи, собранные после справочников.
    #[cfg(all(feature = "currencies", feature = "transactions"))] let _ = currency_usage.transactions.set(transactions_api.clone());
    #[cfg(all(feature = "currencies", feature = "accounts"))] let _ = currency_usage.accounts.set(accounts_api.clone());
    #[cfg(all(feature = "currencies", feature = "projects"))] let _ = currency_usage.projects.set(projects_api.clone());
    #[cfg(feature = "categories")]
    {
        #[allow(unused_mut)]
        let mut sources: Vec<Arc<dyn wire_categories::UsageSource>> = Vec::new();
        #[cfg(feature = "transactions")] sources.push(transactions_api.clone());
        #[cfg(feature = "planning")] sources.push(planning_api.clone());
        #[cfg(feature = "limits")] sources.push(limits_api.clone());
        let _ = category_usage.0.set(sources);
    }
    // Стартовый счёт нового проекта — когда фича `accounts` включена.
    #[cfg(all(feature = "projects", feature = "accounts"))] let _ = bootstrap.accounts.set(accounts_api.clone());
    // Очистка данных проекта: сначала операции и планы, потом справочники.
    #[cfg(feature = "projects")]
    {
        #[allow(unused_mut)]
        let mut targets = wire_projects::PurgeTargets::default();
        #[cfg(feature = "transactions")] targets.list.push(transactions_api.clone());
        #[cfg(feature = "planning")] targets.list.push(planning_api.clone());
        #[cfg(feature = "limits")] targets.list.push(limits_api.clone());
        #[cfg(feature = "rates")] targets.list.push(rates_api.clone());
        #[cfg(feature = "places")] targets.list.push(places_api.clone());
        #[cfg(feature = "people")] targets.list.push(people_api.clone());
        #[cfg(feature = "accounts")] targets.list.push(accounts_api.clone());
        #[cfg(feature = "categories")] targets.list.push(categories_api.clone());
        #[cfg(feature = "currencies")] targets.list.push(currencies_api.clone());
        #[cfg(feature = "mcp")] targets.list.push(mcp_api.clone());
        let _ = purge.0.set(targets);
    }
    let memberships = Arc::new(wire_projects::SessionAdapter { projects: projects_api.clone(), currencies: currencies_api.clone() });
    Wired {
        features: list,
        #[cfg(feature = "seed")]
        seed: crate::seed::Services {
            projects: projects_api.clone(),
            currencies: currencies_api,
            rates: rates_api,
            accounts: accounts_api,
            places: places_api,
            people: people_api,
            transactions: transactions_api,
            limits: limits_api,
            planning: planning_api,
        },
        #[cfg(feature = "site")]
        site: site_api,
        projects: projects_api,
        memberships,
    }
}
