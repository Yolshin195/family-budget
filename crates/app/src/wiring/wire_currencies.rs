//! Связка фичи `currencies`: проводки в валюте (transactions), активные счета (accounts), базовая
//! валюта проекта (projects). Все они собираются после валют — подключаются через `OnceLock`.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct UsageAdapter {
    #[cfg(feature = "transactions")]
    pub transactions: Arc<OnceLock<Arc<::transactions::Service>>>,
    #[cfg(feature = "accounts")]
    pub accounts: Arc<OnceLock<Arc<::accounts::Service>>>,
    #[cfg(feature = "projects")]
    pub projects: Arc<OnceLock<Arc<::projects::Service>>>,
}

#[async_trait]
impl ::currencies::CurrencyUsage for UsageAdapter {
    async fn has_postings(&self, project_id: Uuid, currency: &str) -> AppResult<bool> {
        let _ = (project_id, currency);
        #[cfg(feature = "transactions")]
        if let Some(t) = self.transactions.get() {
            return t.uses_currency(project_id, currency).await;
        }
        Ok(false)
    }

    async fn active_accounts(&self, project_id: Uuid, currency: &str) -> AppResult<Vec<String>> {
        let _ = (project_id, currency);
        #[cfg(feature = "accounts")]
        if let Some(a) = self.accounts.get() {
            return Ok(a.all(project_id, false).await?.into_iter().filter(|x| x.currency_code == currency).map(|x| x.code).collect());
        }
        Ok(Vec::new())
    }

    async fn base_currency(&self, project_id: Uuid) -> AppResult<Option<String>> {
        let _ = project_id;
        #[cfg(feature = "projects")]
        if let Some(p) = self.projects.get() {
            return p.base_currency(project_id).await;
        }
        Ok(None)
    }
}
