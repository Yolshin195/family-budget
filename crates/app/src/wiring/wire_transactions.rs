//! Связка фичи `transactions` с поставщиками: счета (+ символы валют), категории, бюджет.
//! Бюджет сам читает транзакции, поэтому его сервис подключается позже — через `OnceLock`.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use rust_decimal::Decimal;
use transactions::{AccountInfo, CategoryInfo, CategoryLimit};
use uuid::Uuid;

pub struct AccountsLookupAdapter(pub Arc<::accounts::Service>, pub Arc<::currencies::Service>);

#[async_trait]
impl ::transactions::AccountsLookup for AccountsLookupAdapter {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        self.0.exists(project_id, code).await
    }
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.0.choices(project_id, lang).await
    }
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(self.0.all(project_id, true).await?.into_iter().map(|a| (a.title(lang).to_string(), a.code)).map(|(t, c)| (c, t)).collect())
    }
    async fn infos(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<AccountInfo>> {
        let symbols: Vec<(String, String)> = self.1.all(project_id, true).await?.into_iter().map(|c| (c.code, c.symbol)).collect();
        Ok(self
            .0
            .all(project_id, false)
            .await?
            .into_iter()
            .map(|a| AccountInfo {
                symbol: symbols.iter().find(|s| s.0 == a.currency_code).map(|s| s.1.clone()).unwrap_or_else(|| a.currency_code.clone()),
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
    async fn kind_of(&self, project_id: Uuid, code: &str) -> AppResult<Option<String>> {
        self.0.kind_of(project_id, code).await
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
                let names = [Some(c.name.clone()), c.name_ru.clone(), c.name_en.clone(), c.name_th.clone(), Some(c.code.clone())]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
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
