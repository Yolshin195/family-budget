//! Исходящие порты фичи «Валюты»: что ей нужно от хранилища и от других фич.

#[allow(unused_imports)]
use std::sync::Arc;

use async_trait::async_trait;
use kernel::error::AppResult;
use uuid::Uuid;

use crate::domain::Currency;

#[async_trait]
pub trait CurrencyRepository: Send + Sync {
    async fn insert(&self, item: &Currency) -> AppResult<()>;
    async fn update(&self, item: &Currency) -> AppResult<()>;
    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Currency>>;
    async fn list(&self, project_id: Uuid, include_archived: bool, limit: i64, offset: i64)
    -> AppResult<Vec<Currency>>;
    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64>;
    /// Удалить все записи проекта (удаление проекта или аккаунта).
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
}

/// Зависимости фичи от других фич. Порты объявляет эта фича (потребитель),
/// реализации живут в приложении: crates/app/src/wiring/wire_currencies.rs.
pub struct Deps {
    pub usage: Arc<dyn CurrencyUsage>,
}

/// Где валюта уже использована (этап 0, Ц8).
#[async_trait]
pub trait CurrencyUsage: Send + Sync {
    /// Есть ли проводки в валюте — тогда знаков после запятой меньше не сделать: старые суммы
    /// перестанут проходить проверку точности.
    async fn has_postings(&self, project_id: Uuid, currency: &str) -> AppResult<bool>;
    /// Активные счета в валюте — пока они есть, валюту не архивировать.
    async fn active_accounts(&self, project_id: Uuid, currency: &str) -> AppResult<Vec<String>>;
    /// Базовая валюта проекта — её не архивировать.
    async fn base_currency(&self, project_id: Uuid) -> AppResult<Option<String>>;
}
