//! Связка фичи `planning`: оплаты (из транзакций), счета, категории, курсы.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

pub struct PaymentsAdapter(pub Arc<::transactions::Service>);

#[async_trait]
impl ::planning::PaymentsLookup for PaymentsAdapter {
    async fn paid(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<::planning::PaidRef>> {
        Ok(self
            .0
            .payments(project_id, from, to)
            .await?
            .into_iter()
            .map(|p| ::planning::PaidRef { plan_id: p.plan_id, plan_date: p.plan_date, transaction_id: p.transaction_id, amount: p.amount, currency: p.currency, paid_on: p.occurred_on })
            .collect())
    }
}

pub struct AccountsAdapter(pub Arc<::accounts::Service>);

#[async_trait]
impl ::planning::AccountsLookup for AccountsAdapter {
    async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        self.0.exists(project_id, code).await
    }
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.0.choices(project_id, lang).await
    }
    async fn currencies(&self, project_id: Uuid) -> AppResult<Vec<(String, String)>> {
        Ok(self.0.all(project_id, true).await?.into_iter().map(|a| (a.code, a.currency_code)).collect())
    }
}

pub struct CategoriesAdapter(pub Arc<::categories::Service>);

#[async_trait]
impl ::planning::CategoriesLookup for CategoriesAdapter {
    async fn kind_of(&self, project_id: Uuid, code: &str) -> AppResult<Option<String>> {
        self.0.kind_of(project_id, code).await
    }
    async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.0.choices(project_id, lang).await
    }
    async fn titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(self.0.nodes(project_id, lang).await?.into_iter().map(|n| (n.code, n.title)).collect())
    }
}

pub struct RatesAdapter(pub Arc<::rates::Service>);

#[async_trait]
impl ::planning::RatesLookup for RatesAdapter {
    async fn all(&self, project_id: Uuid) -> AppResult<Vec<(String, NaiveDate, Decimal)>> {
        Ok(self.0.all(project_id).await?.into_iter().map(|r| (r.currency_code, r.rate_date, r.rate)).collect())
    }
}
