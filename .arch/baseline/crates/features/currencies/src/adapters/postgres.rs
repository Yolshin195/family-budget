//! Исходящий адаптер: порт на PostgreSQL (таблица `fb_currencies`).
//! `query_as` проверяет SQL в рантайме — крейт собирается без БД и без `.sqlx`-кэша.

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use sqlx::PgPool;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::domain::{CurrencyNameLang, Currency};
use crate::ports::CurrencyRepository;

const COLUMNS: &str = "id, user_id, code, name, archived, name_lang, name_ru, name_en, name_th, symbol, minor_units";

pub struct PostgresCurrencyRepository {
    pool: PgPool,
}

impl PostgresCurrencyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    user_id: Uuid,
    code: String,
    name: String,
    archived: bool,
    name_lang: String,
    name_ru: Option<String>,
    name_en: Option<String>,
    name_th: Option<String>,
    symbol: String,
    minor_units: i32,
}

impl TryFrom<Row> for Currency {
    type Error = AppError;

    fn try_from(r: Row) -> Result<Self, Self::Error> {
        Ok(Self {
            id: r.id,
            user_id: r.user_id,
            code: r.code,
            name: r.name,
            archived: r.archived,
            name_lang: CurrencyNameLang::parse(&r.name_lang).ok_or_else(|| bad_value("name_lang", &r.name_lang))?,
            name_ru: r.name_ru,
            name_en: r.name_en,
            name_th: r.name_th,
            symbol: r.symbol,
            minor_units: r.minor_units,
        })
    }
}

#[allow(dead_code)]
fn bad_value(column: &str, value: &str) -> AppError {
    AppError::Internal(anyhow::anyhow!("fb_currencies.{column}: неизвестное значение '{value}'"))
}

#[async_trait]
impl CurrencyRepository for PostgresCurrencyRepository {
    async fn insert(&self, item: &Currency) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO fb_currencies ({COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
                .replace("{COLUMNS}", COLUMNS)
                .as_str(),
        )
        .bind(item.id)
        .bind(item.user_id)
        .bind(&item.code)
        .bind(&item.name)
        .bind(item.archived)
        .bind(item.name_lang.as_str())
        .bind(&item.name_ru)
        .bind(&item.name_en)
        .bind(&item.name_th)
        .bind(&item.symbol)
        .bind(item.minor_units)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update(&self, item: &Currency) -> AppResult<()> {
        let result = sqlx::query(
            "UPDATE fb_currencies SET name = $3, archived = $4, name_lang = $5, name_ru = $6, name_en = $7, name_th = $8, symbol = $9, minor_units = $10 \
             WHERE user_id = $1 AND code = $2",
        )
        .bind(item.user_id)
        .bind(&item.code)
        .bind(&item.name)
        .bind(item.archived)
        .bind(item.name_lang.as_str())
        .bind(&item.name_ru)
        .bind(&item.name_en)
        .bind(&item.name_th)
        .bind(&item.symbol)
        .bind(item.minor_units)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("запись '{}' не найдена", item.code)));
        }
        Ok(())
    }

    async fn find_by_code(&self, user_id: Uuid, code: &str) -> AppResult<Option<Currency>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_currencies WHERE user_id = $1 AND code = $2");
        let row: Option<Row> = sqlx::query_as(&sql).bind(user_id).bind(code).fetch_optional(&self.pool).await?;
        row.map(Currency::try_from).transpose()
    }

    async fn list(&self, user_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Currency>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM fb_currencies WHERE user_id = $1 AND ($2 OR NOT archived) ORDER BY code LIMIT $3 OFFSET $4"
        );
        let rows: Vec<Row> = sqlx::query_as(&sql)
            .bind(user_id)
            .bind(include_archived)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Currency::try_from).collect()
    }

    async fn count(&self, user_id: Uuid, include_archived: bool) -> AppResult<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM fb_currencies WHERE user_id = $1 AND ($2 OR NOT archived)")
            .bind(user_id)
            .bind(include_archived)
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }
}
