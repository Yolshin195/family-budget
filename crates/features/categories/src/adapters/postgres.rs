//! Исходящий адаптер: порт на PostgreSQL (таблица `fb_categories`).
//! `query_as` проверяет SQL в рантайме — крейт собирается без БД и без `.sqlx`-кэша.

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use sqlx::PgPool;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::domain::{Category, CategoryCategoryKind, CategoryNameLang};
use crate::ports::CategoryRepository;

const COLUMNS: &str = "id, project_id, code, name, archived, name_lang, name_ru, name_en, name_th, category_kind, parent_code, sort_order";

pub struct PostgresCategoryRepository {
    pool: PgPool,
}

impl PostgresCategoryRepository {
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
    archived: bool,
    name_lang: String,
    name_ru: Option<String>,
    name_en: Option<String>,
    name_th: Option<String>,
    category_kind: String,
    parent_code: Option<String>,
    sort_order: i32,
}

impl TryFrom<Row> for Category {
    type Error = AppError;

    fn try_from(r: Row) -> Result<Self, Self::Error> {
        Ok(Self {
            id: r.id,
            project_id: r.project_id,
            code: r.code,
            name: r.name,
            archived: r.archived,
            name_lang: CategoryNameLang::parse(&r.name_lang).ok_or_else(|| bad_value("name_lang", &r.name_lang))?,
            name_ru: r.name_ru,
            name_en: r.name_en,
            name_th: r.name_th,
            category_kind: CategoryCategoryKind::parse(&r.category_kind)
                .ok_or_else(|| bad_value("category_kind", &r.category_kind))?,
            parent_code: r.parent_code,
            sort_order: r.sort_order,
        })
    }
}

#[allow(dead_code)]
fn bad_value(column: &str, value: &str) -> AppError {
    AppError::Internal(anyhow::anyhow!("fb_categories.{column}: неизвестное значение '{value}'"))
}

#[async_trait]
impl CategoryRepository for PostgresCategoryRepository {
    async fn insert(&self, item: &Category) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO fb_categories ({COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
                .replace("{COLUMNS}", COLUMNS)
                .as_str(),
        )
        .bind(item.id)
        .bind(item.project_id)
        .bind(&item.code)
        .bind(&item.name)
        .bind(item.archived)
        .bind(item.name_lang.as_str())
        .bind(&item.name_ru)
        .bind(&item.name_en)
        .bind(&item.name_th)
        .bind(item.category_kind.as_str())
        .bind(&item.parent_code)
        .bind(item.sort_order)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update(&self, item: &Category) -> AppResult<()> {
        let result = sqlx::query(
            "UPDATE fb_categories SET name = $3, archived = $4, name_lang = $5, name_ru = $6, name_en = $7, name_th = $8, category_kind = $9, parent_code = $10, sort_order = $11 \
             WHERE project_id = $1 AND code = $2",
        )
        .bind(item.project_id)
        .bind(&item.code)
        .bind(&item.name)
        .bind(item.archived)
        .bind(item.name_lang.as_str())
        .bind(&item.name_ru)
        .bind(&item.name_en)
        .bind(&item.name_th)
        .bind(item.category_kind.as_str())
        .bind(&item.parent_code)
        .bind(item.sort_order)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("запись '{}' не найдена", item.code)));
        }
        Ok(())
    }

    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Category>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_categories WHERE project_id = $1 AND code = $2");
        let row: Option<Row> = sqlx::query_as(&sql).bind(project_id).bind(code).fetch_optional(&self.pool).await?;
        row.map(Category::try_from).transpose()
    }

    async fn list(
        &self,
        project_id: Uuid,
        include_archived: bool,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Category>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM fb_categories WHERE project_id = $1 AND ($2 OR NOT archived) ORDER BY code LIMIT $3 OFFSET $4"
        );
        let rows: Vec<Row> = sqlx::query_as(&sql)
            .bind(project_id)
            .bind(include_archived)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Category::try_from).collect()
    }

    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64> {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM fb_categories WHERE project_id = $1 AND ($2 OR NOT archived)")
                .bind(project_id)
                .bind(include_archived)
                .fetch_one(&self.pool)
                .await?;
        Ok(n)
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let r =
            sqlx::query("DELETE FROM fb_categories WHERE project_id = $1").bind(project_id).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }
}
