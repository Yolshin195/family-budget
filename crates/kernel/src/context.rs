//! Что фича получает при сборке. Принадлежит проекту: добавляй сюда общие зависимости (конфиг, клиенты).
use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppContext {
    pub pool: PgPool,
    /// Настройки из окружения: адрес сайта, Rauthy, секрет подписи, режим разработки.
    pub config: Arc<Config>,
}
