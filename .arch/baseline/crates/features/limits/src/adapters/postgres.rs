//! Исходящий адаптер: порт на PostgreSQL (таблица `fb_limits`).

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use sqlx::PgPool;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::domain::{LimitLimitPeriod, Limit};
use crate::ports::{Period, LimitRepository};

const COLUMNS: &str = "id, user_id, valid_from, category_code, limit_period, amount";
/// Общий фильтр: пользователь + период ($2, $3 — границы или NULL).
const FILTER: &str = "user_id = $1 AND ($2::date IS NULL OR valid_from >= $2) \
                      AND ($3::date IS NULL OR valid_from <= $3)";

pub struct PostgresLimitRepository {
    pool: PgPool,
}

impl PostgresLimitRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    user_id: Uuid,
    valid_from: chrono::NaiveDate,
    category_code: String,
    limit_period: String,
    amount: rust_decimal::Decimal,
}

impl TryFrom<Row> for Limit {
    type Error = AppError;

    fn try_from(r: Row) -> Result<Self, Self::Error> {
        Ok(Self {
            id: r.id,
            user_id: r.user_id,
            valid_from: r.valid_from,
            category_code: r.category_code,
            limit_period: LimitLimitPeriod::parse(&r.limit_period).ok_or_else(|| bad_value("limit_period", &r.limit_period))?,
            amount: r.amount,
        })
    }
}

#[allow(dead_code)]
fn bad_value(column: &str, value: &str) -> AppError {
    AppError::Internal(anyhow::anyhow!("fb_limits.{column}: неизвестное значение '{value}'"))
}

#[async_trait]
impl LimitRepository for PostgresLimitRepository {
    async fn insert(&self, item: &Limit) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO fb_limits (id, user_id, valid_from, category_code, limit_period, amount) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(item.id)
        .bind(item.user_id)
        .bind(item.valid_from)
        .bind(&item.category_code)
        .bind(item.limit_period.as_str())
        .bind(item.amount)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update(&self, item: &Limit) -> AppResult<()> {
        let result = sqlx::query(
            "UPDATE fb_limits SET valid_from = $3, category_code = $4, limit_period = $5, amount = $6 \
             WHERE user_id = $1 AND id = $2",
        )
        .bind(item.user_id)
        .bind(item.id)
        .bind(item.valid_from)
        .bind(&item.category_code)
        .bind(item.limit_period.as_str())
        .bind(item.amount)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("запись {} не найдена", item.id)));
        }
        Ok(())
    }

    async fn find(&self, user_id: Uuid, id: Uuid) -> AppResult<Option<Limit>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_limits WHERE user_id = $1 AND id = $2");
        let row: Option<Row> = sqlx::query_as(&sql).bind(user_id).bind(id).fetch_optional(&self.pool).await?;
        row.map(Limit::try_from).transpose()
    }

    async fn list(&self, user_id: Uuid, period: Period, limit: i64, offset: i64) -> AppResult<Vec<Limit>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM fb_limits WHERE {FILTER} ORDER BY valid_from DESC, id LIMIT $4 OFFSET $5"
        );
        let rows: Vec<Row> = sqlx::query_as(&sql)
            .bind(user_id)
            .bind(period.from)
            .bind(period.to)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Limit::try_from).collect()
    }

    async fn count(&self, user_id: Uuid, period: Period) -> AppResult<i64> {
        let sql = format!("SELECT COUNT(*) FROM fb_limits WHERE {FILTER}");
        let (n,): (i64,) = sqlx::query_as(&sql)
            .bind(user_id)
            .bind(period.from)
            .bind(period.to)
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }

    async fn delete(&self, user_id: Uuid, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM fb_limits WHERE user_id = $1 AND id = $2")
            .bind(user_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
