//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use uuid::Uuid;

use crate::domain::Place;
use crate::ports::PlaceRepository;

#[derive(Default)]
pub struct InMemoryPlaceRepository {
    rows: Mutex<Vec<Place>>,
}

#[async_trait]
impl PlaceRepository for InMemoryPlaceRepository {
    async fn insert(&self, item: &Place) -> AppResult<()> {
        let mut rows = self.rows.lock().unwrap();
        if rows.iter().any(|r| r.project_id == item.project_id && r.code == item.code) {
            return Err(AppError::Conflict("duplicate code".into()));
        }
        rows.push(item.clone());
        Ok(())
    }

    async fn update(&self, item: &Place) -> AppResult<()> {
        let mut rows = self.rows.lock().unwrap();
        let row = rows.iter_mut().find(|r| r.project_id == item.project_id && r.code == item.code);
        *row.ok_or_else(|| AppError::NotFound(item.code.clone()))? = item.clone();
        Ok(())
    }

    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Place>> {
        Ok(self.rows.lock().unwrap().iter().find(|r| r.project_id == project_id && r.code == code).cloned())
    }

    async fn list(&self, project_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Place>> {
        let rows = self.rows.lock().unwrap();
        let mut items: Vec<_> = rows.iter().filter(|r| r.project_id == project_id && (include_archived || !r.archived)).cloned().collect();
        items.sort_by(|a, b| (a.name.to_lowercase(), &a.code).cmp(&(b.name.to_lowercase(), &b.code)));
        Ok(items.into_iter().skip(offset as usize).take(limit as usize).collect())
    }

    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64> {
        Ok(self.rows.lock().unwrap().iter().filter(|r| r.project_id == project_id && (include_archived || !r.archived)).count() as i64)
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let mut rows = self.rows.lock().unwrap();
        let n = rows.len();
        rows.retain(|r| r.project_id != project_id);
        Ok((n - rows.len()) as u64)
    }
}
