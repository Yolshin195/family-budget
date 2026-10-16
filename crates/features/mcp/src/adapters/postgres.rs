//! Исходящий адаптер: доступы ассистентов в PostgreSQL (таблица `fb_mcp_grants`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use kernel::error::AppResult;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::Grant;
use crate::ports::GrantRepository;

const COLUMNS: &str = "id, user_id, client_id, project_id, scopes, mfa, created_at, updated_at, last_used_at";

pub struct PostgresGrantRepository {
    pool: PgPool,
}

impl PostgresGrantRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    user_id: Uuid,
    client_id: String,
    project_id: Uuid,
    scopes: Vec<String>,
    mfa: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl From<Row> for Grant {
    fn from(r: Row) -> Self {
        Grant {
            id: r.id,
            user_id: r.user_id,
            client_id: r.client_id,
            project_id: r.project_id,
            scopes: r.scopes,
            mfa: r.mfa,
            created_at: r.created_at,
            updated_at: r.updated_at,
            last_used_at: r.last_used_at,
        }
    }
}

#[async_trait]
impl GrantRepository for PostgresGrantRepository {
    async fn grant(&self, user_id: Uuid, client_id: &str) -> AppResult<Option<Grant>> {
        let row: Option<Row> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM fb_mcp_grants WHERE user_id = $1 AND client_id = $2"))
            .bind(user_id)
            .bind(client_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Into::into))
    }

    async fn grants_of(&self, user_id: Uuid) -> AppResult<Vec<Grant>> {
        let rows: Vec<Row> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM fb_mcp_grants WHERE user_id = $1 ORDER BY created_at"))
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn save(&self, g: &Grant) -> AppResult<()> {
        sqlx::query(&format!(
            "INSERT INTO fb_mcp_grants ({COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (user_id, client_id) DO UPDATE SET project_id = EXCLUDED.project_id, scopes = EXCLUDED.scopes, \
             mfa = EXCLUDED.mfa, updated_at = EXCLUDED.updated_at"
        ))
        .bind(g.id)
        .bind(g.user_id)
        .bind(&g.client_id)
        .bind(g.project_id)
        .bind(&g.scopes)
        .bind(g.mfa)
        .bind(g.created_at)
        .bind(g.updated_at)
        .bind(g.last_used_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, user_id: Uuid, client_id: &str) -> AppResult<bool> {
        let r = sqlx::query("DELETE FROM fb_mcp_grants WHERE user_id = $1 AND client_id = $2").bind(user_id).bind(client_id).execute(&self.pool).await?;
        Ok(r.rows_affected() > 0)
    }

    async fn touch(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        sqlx::query("UPDATE fb_mcp_grants SET last_used_at = $2 WHERE id = $1").bind(id).bind(at).execute(&self.pool).await?;
        Ok(())
    }

    async fn delete_project(&self, project_id: Uuid) -> AppResult<u64> {
        let r = sqlx::query("DELETE FROM fb_mcp_grants WHERE project_id = $1").bind(project_id).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }
}
