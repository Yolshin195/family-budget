//! Исходящий адаптер: порт на PostgreSQL (таблица `fb_places`).
//! `query_as` проверяет SQL в рантайме — крейт собирается без БД и без `.sqlx`-кэша.

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{Place, PlaceKind};
use crate::ports::PlaceRepository;

const COLUMNS: &str = "id, project_id, code, name, parent_code, aliases, default_category_code, default_account_code, place_kind, location, archived";

pub struct PostgresPlaceRepository {
    pool: PgPool,
}

impl PostgresPlaceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    project_id: Uuid,
    code: String,
    name: String,
    parent_code: Option<String>,
    aliases: Vec<String>,
    default_category_code: Option<String>,
    default_account_code: Option<String>,
    place_kind: String,
    location: Option<String>,
    archived: bool,
}

impl TryFrom<Row> for Place {
    type Error = AppError;

    fn try_from(r: Row) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: PlaceKind::parse(&r.place_kind).ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("fb_places.place_kind: unknown value '{}'", r.place_kind))
            })?,
            id: r.id,
            project_id: r.project_id,
            code: r.code,
            name: r.name,
            parent_code: r.parent_code,
            aliases: r.aliases,
            default_category_code: r.default_category_code,
            default_account_code: r.default_account_code,
            location: r.location,
            archived: r.archived,
        })
    }
}

#[async_trait]
impl PlaceRepository for PostgresPlaceRepository {
    async fn insert(&self, item: &Place) -> AppResult<()> {
        sqlx::query(&format!(
            "INSERT INTO fb_places ({COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
        ))
        .bind(item.id)
        .bind(item.project_id)
        .bind(&item.code)
        .bind(&item.name)
        .bind(&item.parent_code)
        .bind(&item.aliases)
        .bind(&item.default_category_code)
        .bind(&item.default_account_code)
        .bind(item.kind.as_str())
        .bind(&item.location)
        .bind(item.archived)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update(&self, item: &Place) -> AppResult<()> {
        let result = sqlx::query(
            "UPDATE fb_places SET name = $3, aliases = $4, default_category_code = $5, default_account_code = $6, place_kind = $7, \
             location = $8, archived = $9, parent_code = $10 WHERE project_id = $1 AND code = $2",
        )
        .bind(item.project_id)
        .bind(&item.code)
        .bind(&item.name)
        .bind(&item.aliases)
        .bind(&item.default_category_code)
        .bind(&item.default_account_code)
        .bind(item.kind.as_str())
        .bind(&item.location)
        .bind(item.archived)
        .bind(&item.parent_code)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(item.code.clone()));
        }
        Ok(())
    }

    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Place>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_places WHERE project_id = $1 AND code = $2");
        let row: Option<Row> = sqlx::query_as(&sql).bind(project_id).bind(code).fetch_optional(&self.pool).await?;
        row.map(Place::try_from).transpose()
    }

    async fn all(&self, project_id: Uuid) -> AppResult<Vec<Place>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_places WHERE project_id = $1");
        let rows: Vec<Row> = sqlx::query_as(&sql).bind(project_id).fetch_all(&self.pool).await?;
        rows.into_iter().map(Place::try_from).collect()
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let r = sqlx::query("DELETE FROM fb_places WHERE project_id = $1").bind(project_id).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }
}
