//! Связка фичи `transactions` с поставщиками: счета (+ символы валют), категории, бюджет, планы.
//! Бюджет и планы сами читают транзакции, поэтому их сервисы подключаются позже — через `OnceLock`.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use rust_decimal::Decimal;
use transactions::{AccountCurrency, AccountInfo, CategoryInfo, CategoryLimit, PlanOccurrence};
use uuid::Uuid;

pub struct AccountsLookupAdapter(pub Arc<::accounts::Service>, pub Arc<::currencies::Service>);

#[async_trait]
impl ::transactions::AccountsLookup for AccountsLookupAdapter {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        self.0.exists(project_id, code).await
    }
    async fn currencies(&self, project_id: Uuid) -> AppResult<Vec<AccountCurrency>> {
        let minor: Vec<(String, i32)> = self.1.all(project_id, true).await?.into_iter().map(|c| (c.code, c.minor_units)).collect();
        Ok(self
            .0
            .all(project_id, true)
            .await?
            .into_iter()
            .map(|a| AccountCurrency {
                minor_units: minor.iter().find(|m| m.0 == a.currency_code).map(|m| m.1.max(0) as u32).unwrap_or(2),
                code: a.code,
                currency: a.currency_code,
            })
            .collect())
    }
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.0.choices(project_id, lang).await
    }
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(self.0.all(project_id, true).await?.into_iter().map(|a| (a.title(lang).to_string(), a.code)).map(|(t, c)| (c, t)).collect())
    }
    async fn infos(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<AccountInfo>> {
        // Как на страницах: символ, общий для нескольких валют проекта, заменяется кодом.
        let symbols = kernel::auth::currency_marks(self.1.all(project_id, true).await?.into_iter().map(|c| (c.code, c.symbol)));
        Ok(self
            .0
            .all(project_id, false)
            .await?
            .into_iter()
            .map(|a| AccountInfo {
                symbol: symbols.get(&a.currency_code).cloned().unwrap_or_else(|| a.currency_code.clone()),
                title: a.title(lang).to_string(),
                code: a.code,
                currency: a.currency_code,
            })
            .collect())
    }
}

pub struct CategoriesLookupAdapter(pub Arc<::categories::Service>);

#[async_trait]
impl ::transactions::CategoriesLookup for CategoriesLookupAdapter {
    async fn kind_of(&self, project_id: Uuid, code: &str) -> AppResult<Option<::transactions::CategoryRef>> {
        match self.0.get(project_id, code).await {
            Ok(c) => Ok(Some(::transactions::CategoryRef { kind: c.category_kind, archived: c.archived })),
            Err(kernel::error::AppError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.0.choices(project_id, lang).await
    }
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(self.0.nodes(project_id, lang).await?.into_iter().map(|n| (n.code, n.title)).collect())
    }
    async fn infos(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<CategoryInfo>> {
        Ok(self
            .0
            .tree(project_id, false)
            .await?
            .into_iter()
            .map(|(depth, c)| {
                let names = kernel::search::join([Some(c.name.as_str()), c.name_ru.as_deref(), c.name_en.as_deref(), c.name_th.as_deref(), Some(c.code.as_str())].into_iter().flatten());
                CategoryInfo { title: c.title(lang).to_string(), code: c.code, parent_code: c.parent_code, kind: c.category_kind, depth, names }
            })
            .collect())
    }
}

/// Остатки лимитов и курсы из фичи `budget` (подключается после сборки бюджета).
#[derive(Clone, Default)]
pub struct BudgetViewAdapter(pub Arc<OnceLock<Arc<dyn BudgetSource>>>);

/// То, что адаптеру нужно от бюджета (без бюджета — пусто).
#[async_trait]
pub trait BudgetSource: Send + Sync {
    async fn limits(&self, project_id: Uuid, base: &str, date: NaiveDate, lang: Lang) -> AppResult<Vec<CategoryLimit>>;
    async fn rates(&self, project_id: Uuid, base: &str, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>>;
}

#[cfg(feature = "budget")]
#[async_trait]
impl BudgetSource for ::budget::Service {
    async fn limits(&self, project_id: Uuid, base: &str, date: NaiveDate, lang: Lang) -> AppResult<Vec<CategoryLimit>> {
        Ok(self
            .month_limits(project_id, base, date, lang)
            .await?
            .into_iter()
            .map(|l| CategoryLimit { code: l.code, limit: l.limit, forecast: l.forecast, available: l.available })
            .collect())
    }
    async fn rates(&self, project_id: Uuid, base: &str, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        self.rates_on(project_id, base, date).await
    }
}

#[async_trait]
impl ::transactions::BudgetView for BudgetViewAdapter {
    async fn limits(&self, project_id: Uuid, base: &str, date: NaiveDate, lang: Lang) -> AppResult<Vec<CategoryLimit>> {
        match self.0.get() {
            Some(b) => b.limits(project_id, base, date, lang).await,
            None => Ok(Vec::new()),
        }
    }
    async fn rates(&self, project_id: Uuid, base: &str, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        match self.0.get() {
            Some(b) => b.rates(project_id, base, date).await,
            None => Ok(Vec::new()),
        }
    }
}

/// Планы из фичи `planning` (подключается после сборки планирования).
#[derive(Clone, Default)]
pub struct PlansLookupAdapter(pub Arc<OnceLock<Arc<dyn PlansSource>>>);

/// То, что адаптеру нужно от планирования (без него планов нет — оплату плана не принять).
#[async_trait]
pub trait PlansSource: Send + Sync {
    async fn occurrence(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<Option<PlanOccurrence>>;
}

#[cfg(feature = "planning")]
#[async_trait]
impl PlansSource for ::planning::Service {
    async fn occurrence(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<Option<PlanOccurrence>> {
        Ok(self.occurrence_kind(project_id, plan_id, date).await?.map(|(kind, skipped)| PlanOccurrence { kind, skipped }))
    }
}

#[async_trait]
impl ::transactions::PlansLookup for PlansLookupAdapter {
    async fn occurrence(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<Option<PlanOccurrence>> {
        match self.0.get() {
            Some(p) => p.occurrence(project_id, plan_id, date).await,
            None => Ok(None),
        }
    }
}
