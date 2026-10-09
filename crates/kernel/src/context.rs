//! Что фича получает при сборке. Принадлежит проекту: добавляй сюда общие зависимости (конфиг, клиенты).
use sqlx::PgPool;

use crate::auth::Tokens;

#[derive(Clone)]
pub struct AppContext {
    pub pool: PgPool,
    /// Выпуск/проверка JWT — нужен фиче `projects` для входа.
    pub tokens: Tokens,
}
