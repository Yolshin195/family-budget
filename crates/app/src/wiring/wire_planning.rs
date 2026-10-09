//! Связка фичи `planning`: оплаты (из транзакций), счета, категории.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::web::Choice;
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
            .map(|p| ::planning::PaidRef { plan_id: p.plan_id, plan_date: p.plan_date, transaction_id: p.transaction_id, amount: p.amount, paid_on: p.occurred_on })
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
