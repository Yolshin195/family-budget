//! Исходящие порты фичи «Сайт»: счётчики воронки из других фич (без персональных данных).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use kernel::error::AppResult;

/// Регистрации и первые расходы за период — только количества (реализация — app/wiring/wire_site.rs).
#[async_trait]
pub trait FunnelSource: Send + Sync {
    /// Новые пользователи (кроме демо) с `from` до `to`.
    async fn signups(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> AppResult<i64>;
    /// Пользователи, внёсшие первую операцию с `from` до `to`.
    async fn first_expenses(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> AppResult<i64>;
}

/// Зависимости фичи от других фич. Реализации — crates/app/src/wiring/wire_site.rs.
pub struct Deps {
    pub funnel: Arc<dyn FunnelSource>,
}
