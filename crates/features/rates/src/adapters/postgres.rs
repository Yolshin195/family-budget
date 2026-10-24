//! Исходящий адаптер: порт на PostgreSQL (таблица `fb_rates`).

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use sqlx::PgPool;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::domain::Rate;
use crate::ports::{Period, RateRepository};

const COLUMNS: &str = "id, project_id, rate_date, currency_code, rate, entered, inverted";
/// Общий фильтр: пользователь + период ($2, $3 — границы или NULL).
const FILTER: &str = "project_id = $1 AND ($2::date IS NULL OR rate_date >= $2) \
                      AND ($3::date IS NULL OR rate_date <= $3)";

pub struct PostgresRateRepository {
    pool: PgPool,
}

impl PostgresRateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    project_id: Uuid,
    rate_date: chrono::NaiveDate,
    currency_code: String,
    rate: rust_decimal::Decimal,
    entered: rust_decimal::Decimal,
    inverted: bool,
}

impl TryFrom<Row> for Rate {
    type Error = AppError;

    fn try_from(r: Row) -> Result<Self, Self::Error> {
        Ok(Self {
            id: r.id,
            project_id: r.project_id,
            rate_date: r.rate_date,
            currency_code: r.currency_code,
            rate: r.rate,
            entered: r.entered,
            inverted: r.inverted,
        })
    }
}

#[allow(dead_code)]
fn bad_value(column: &str, value: &str) -> AppError {
    AppError::Internal(anyhow::anyhow!("fb_rates.{column}: неизвестное значение '{value}'"))
}

#[async_trait]
impl RateRepository for PostgresRateRepository {
    async fn insert(&self, item: &Rate) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO fb_rates (id, project_id, rate_date, currency_code, rate, entered, inverted) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(item.id)
        .bind(item.project_id)
        .bind(item.rate_date)
        .bind(&item.currency_code)
        .bind(item.rate)
        .bind(item.entered)
        .bind(item.inverted)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update(&self, item: &Rate) -> AppResult<()> {
        let result = sqlx::query(
            "UPDATE fb_rates SET rate_date = $3, currency_code = $4, rate = $5, entered = $6, inverted = $7 \
             WHERE project_id = $1 AND id = $2",
        )
        .bind(item.project_id)
        .bind(item.id)
        .bind(item.rate_date)
        .bind(&item.currency_code)
        .bind(item.rate)
        .bind(item.entered)
        .bind(item.inverted)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("запись {} не найдена", item.id)));
        }
        Ok(())
    }

    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Rate>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_rates WHERE project_id = $1 AND id = $2");
        let row: Option<Row> = sqlx::query_as(&sql).bind(project_id).bind(id).fetch_optional(&self.pool).await?;
        row.map(Rate::try_from).transpose()
    }

    async fn list(&self, project_id: Uuid, period: Period, limit: i64, offset: i64) -> AppResult<Vec<Rate>> {
        let sql =
            format!("SELECT {COLUMNS} FROM fb_rates WHERE {FILTER} ORDER BY rate_date DESC, id LIMIT $4 OFFSET $5");
        let rows: Vec<Row> = sqlx::query_as(&sql)
            .bind(project_id)
            .bind(period.from)
            .bind(period.to)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Rate::try_from).collect()
    }

    async fn count(&self, project_id: Uuid, period: Period) -> AppResult<i64> {
        let sql = format!("SELECT COUNT(*) FROM fb_rates WHERE {FILTER}");
        let (n,): (i64,) =
            sqlx::query_as(&sql).bind(project_id).bind(period.from).bind(period.to).fetch_one(&self.pool).await?;
        Ok(n)
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM fb_rates WHERE project_id = $1 AND id = $2")
            .bind(project_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let r = sqlx::query("DELETE FROM fb_rates WHERE project_id = $1").bind(project_id).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }
}
