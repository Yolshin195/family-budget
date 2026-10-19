//! Связка фичи `budget` с источниками данных: категории, факты, планы, лимиты, счета, курсы.

use std::sync::Arc;

use async_trait::async_trait;
use budget::{AccountRec, ExchangeRec, FactRec, LimitRule, Node, PlannedRec, View};
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use uuid::Uuid;

pub struct CategoriesAdapter(pub Arc<::categories::Service>);

#[async_trait]
impl ::budget::CategoriesSource for CategoriesAdapter {
    async fn nodes(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Node>> {
        Ok(self
            .0
            .nodes(project_id, lang)
            .await?
            .into_iter()
            .map(|n| Node { code: n.code, parent_code: n.parent_code, kind: n.kind, title: n.title, depth: n.depth, archived: n.archived })
            .collect())
    }
}

pub struct FactsAdapter(pub Arc<::transactions::Service>);

#[async_trait]
impl ::budget::FactsSource for FactsAdapter {
    async fn lines(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<FactRec>> {
        Ok(self
            .0
            .lines(project_id, from, to)
            .await?
            .into_iter()
            .map(|l| FactRec { date: l.occurred_on, currency: l.currency, kind: l.kind, category_code: l.category_code, amount: l.amount, created_by: l.created_by })
            .collect())
    }
    async fn balances(&self, project_id: Uuid, to: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        self.0.balances(project_id, to).await
    }
    async fn exchange(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<ExchangeRec>> {
        Ok(self
            .0
            .exchange(project_id, from, to)
            .await?
            .into_iter()
            .map(|x| ExchangeRec { transaction_id: x.transaction_id, date: x.occurred_on, currency: x.currency, amount: x.amount, created_by: x.created_by })
            .collect())
    }
}

pub struct PlansAdapter(pub Arc<::planning::Service>);

#[async_trait]
impl ::budget::PlansSource for PlansAdapter {
    async fn open(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate, today: NaiveDate, lang: Lang) -> AppResult<Vec<PlannedRec>> {
        Ok(self
            .0
            .occurrences(project_id, from, to, today)
            .await?
            .into_iter()
            .filter(|o| o.is_open())
            .map(|o| PlannedRec {
                plan_id: o.plan.id,
                date: o.date,
                title: o.plan.title(lang).to_string(),
                kind: o.plan.tx_kind,
                account_code: o.plan.account_code,
                category_code: o.plan.category_code,
                to_account_code: o.plan.to_account_code,
                amount: o.plan.amount,
                currency: None,
                status: o.status,
            })
            .collect())
    }
}

pub struct LimitsAdapter(pub Arc<::limits::Service>);

#[async_trait]
impl ::budget::LimitsSource for LimitsAdapter {
    async fn rules(&self, project_id: Uuid) -> AppResult<Vec<LimitRule>> {
        Ok(self
            .0
            .all(project_id)
            .await?
            .into_iter()
            .map(|l| LimitRule { period: View::parse(&l.limit_period), category_code: l.category_code, amount: l.amount, valid_from: l.valid_from })
            .collect())
    }
}

pub struct AccountsAdapter(pub Arc<::accounts::Service>);

#[async_trait]
impl ::budget::AccountsSource for AccountsAdapter {
    async fn all(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<AccountRec>> {
        Ok(self
            .0
            .all(project_id, true)
            .await?
            .into_iter()
            .map(|a| AccountRec { title: a.title(lang).to_string(), code: a.code, currency: a.currency_code, archived: a.archived })
            .collect())
    }
}

pub struct RatesAdapter(pub Arc<::rates::Service>);

#[async_trait]
impl ::budget::RatesSource for RatesAdapter {
    async fn all(&self, project_id: Uuid) -> AppResult<Vec<(String, NaiveDate, Decimal)>> {
        Ok(self.0.all(project_id).await?.into_iter().map(|r| (r.currency_code, r.rate_date, r.rate)).collect())
    }
}
