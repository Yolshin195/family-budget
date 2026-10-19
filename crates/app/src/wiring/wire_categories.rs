//! Связка фичи `categories`: где категория уже использована — операции, планы, лимиты. Эти фичи
//! сами ссылаются на категории и собираются позже, поэтому подключаются через `OnceLock`.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

/// Фича, которая может ссылаться на категорию.
#[async_trait]
pub trait UsageSource: Send + Sync {
    async fn uses_category(&self, project_id: Uuid, code: &str) -> AppResult<bool>;
}

#[cfg(feature = "transactions")]
#[async_trait]
impl UsageSource for ::transactions::Service {
    async fn uses_category(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        ::transactions::Service::uses_category(self, project_id, code).await
    }
}

#[cfg(feature = "planning")]
#[async_trait]
impl UsageSource for ::planning::Service {
    async fn uses_category(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        ::planning::Service::uses_category(self, project_id, code).await
    }
}

#[cfg(feature = "limits")]
#[async_trait]
impl UsageSource for ::limits::Service {
    async fn uses_category(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        Ok(self.all(project_id).await?.iter().any(|l| l.category_code == code))
    }
}

/// Источники ставятся после сборки transactions, planning и limits (без них категория «не используется»).
#[derive(Clone, Default)]
pub struct UsageAdapter(pub Arc<OnceLock<Vec<Arc<dyn UsageSource>>>>);

#[async_trait]
impl ::categories::CategoryUsage for UsageAdapter {
    async fn in_use(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        for source in self.0.get().into_iter().flatten() {
            if source.uses_category(project_id, code).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
