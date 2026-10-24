//! Исходящий адаптер: порт на PostgreSQL (таблица `fb_people`).
//! `query_as` проверяет SQL в рантайме — крейт собирается без БД и без `.sqlx`-кэша. Занятый код или вторая
//! запись того же участника — `Conflict` (уникальные ключи таблицы; сервис подберёт другой код).

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::Person;
use crate::ports::PersonRepository;

const COLUMNS: &str = "id, project_id, code, name, aliases, user_id, former_member, archived";

pub struct PostgresPersonRepository {
    pool: PgPool,
}

impl PostgresPersonRepository {
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
    aliases: Vec<String>,
    user_id: Option<Uuid>,
    former_member: bool,
    archived: bool,
}

impl From<Row> for Person {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            code: r.code,
            name: r.name,
            aliases: r.aliases,
            user_id: r.user_id,
            former_member: r.former_member,
            archived: r.archived,
        }
    }
}

#[async_trait]
impl PersonRepository for PostgresPersonRepository {
    async fn insert(&self, item: &Person) -> AppResult<()> {
        sqlx::query(&format!("INSERT INTO fb_people ({COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"))
            .bind(item.id)
            .bind(item.project_id)
            .bind(&item.code)
            .bind(&item.name)
            .bind(&item.aliases)
            .bind(item.user_id)
            .bind(item.former_member)
            .bind(item.archived)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update(&self, item: &Person) -> AppResult<()> {
        let result = sqlx::query(
            "UPDATE fb_people SET name = $3, aliases = $4, user_id = $5, former_member = $6, archived = $7 WHERE project_id = $1 AND code = $2",
        )
        .bind(item.project_id)
        .bind(&item.code)
        .bind(&item.name)
        .bind(&item.aliases)
        .bind(item.user_id)
        .bind(item.former_member)
        .bind(item.archived)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(item.code.clone()));
        }
        Ok(())
    }

    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Person>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_people WHERE project_id = $1 AND code = $2");
        let row: Option<Row> = sqlx::query_as(&sql).bind(project_id).bind(code).fetch_optional(&self.pool).await?;
        Ok(row.map(Into::into))
    }

    async fn find_by_user(&self, project_id: Uuid, user_id: Uuid) -> AppResult<Option<Person>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_people WHERE project_id = $1 AND user_id = $2");
        let row: Option<Row> = sqlx::query_as(&sql).bind(project_id).bind(user_id).fetch_optional(&self.pool).await?;
        Ok(row.map(Into::into))
    }

    async fn of_user(&self, user_id: Uuid) -> AppResult<Vec<Person>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_people WHERE user_id = $1");
        let rows: Vec<Row> = sqlx::query_as(&sql).bind(user_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list(&self, project_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Person>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM fb_people WHERE project_id = $1 AND ($2 OR NOT archived) ORDER BY lower(name), code LIMIT $3 OFFSET $4"
        );
        let rows: Vec<Row> = sqlx::query_as(&sql)
            .bind(project_id)
            .bind(include_archived)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64> {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM fb_people WHERE project_id = $1 AND ($2 OR NOT archived)")
                .bind(project_id)
                .bind(include_archived)
                .fetch_one(&self.pool)
                .await?;
        Ok(n)
    }

    async fn codes(&self, project_id: Uuid) -> AppResult<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT code FROM fb_people WHERE project_id = $1")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let r = sqlx::query("DELETE FROM fb_people WHERE project_id = $1").bind(project_id).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }
}
