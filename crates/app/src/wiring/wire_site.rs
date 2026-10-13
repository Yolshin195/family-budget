//! Связка фичи `site`: счётчики воронки из `projects` (регистрации) и `transactions` (первый расход).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use kernel::error::AppResult;

pub struct FunnelAdapter {
    pub projects: Arc<::projects::Service>,
    #[cfg(feature = "transactions")]
    pub transactions: Arc<::transactions::Service>,
}

#[async_trait]
impl ::site::FunnelSource for FunnelAdapter {
    async fn signups(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> AppResult<i64> {
        self.projects.signups_between(from, to).await
    }
    async fn first_expenses(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> AppResult<i64> {
        #[cfg(feature = "transactions")]
        return self.transactions.first_expenses_between(from, to).await;
        #[cfg(not(feature = "transactions"))]
        return {
            let _ = (from, to);
            Ok(0)
        };
    }
}
