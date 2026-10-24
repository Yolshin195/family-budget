//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use uuid::Uuid;

use crate::domain::Category;
use crate::ports::CategoryRepository;

#[derive(Default)]
pub struct InMemoryCategoryRepository {
    rows: Mutex<Vec<Category>>,
}

#[async_trait]
impl CategoryRepository for InMemoryCategoryRepository {
    async fn insert(&self, item: &Category) -> AppResult<()> {
        let mut rows = self.rows.lock().unwrap();
        if rows.iter().any(|r| r.project_id == item.project_id && r.code == item.code) {
            return Err(AppError::Conflict("duplicate code".into()));
        }
        rows.push(item.clone());
        Ok(())
    }

    async fn update(&self, item: &Category) -> AppResult<()> {
        let mut rows = self.rows.lock().unwrap();
        let row = rows.iter_mut().find(|r| r.project_id == item.project_id && r.code == item.code);
        *row.ok_or_else(|| AppError::NotFound(item.code.clone()))? = item.clone();
        Ok(())
    }

    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Category>> {
        let rows = self.rows.lock().unwrap();
        Ok(rows.iter().find(|r| r.project_id == project_id && r.code == code).cloned())
    }

    async fn list(
        &self,
        project_id: Uuid,
        include_archived: bool,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Category>> {
        let rows = self.rows.lock().unwrap();
        let mut items: Vec<_> =
            rows.iter().filter(|r| r.project_id == project_id && (include_archived || !r.archived)).cloned().collect();
        items.sort_by(|a, b| a.code.cmp(&b.code));
        Ok(items.into_iter().skip(offset as usize).take(limit as usize).collect())
    }

    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64> {
        let rows = self.rows.lock().unwrap();
        Ok(rows.iter().filter(|r| r.project_id == project_id && (include_archived || !r.archived)).count() as i64)
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let mut rows = self.rows.lock().unwrap();
        let n = rows.len();
        rows.retain(|r| r.project_id != project_id);
        Ok((n - rows.len()) as u64)
    }
}
