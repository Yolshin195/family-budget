//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use uuid::Uuid;

use crate::domain::Person;
use crate::ports::PersonRepository;

#[derive(Default)]
pub struct InMemoryPersonRepository {
    rows: Mutex<Vec<Person>>,
}

#[async_trait]
impl PersonRepository for InMemoryPersonRepository {
    async fn insert(&self, item: &Person) -> AppResult<()> {
        let mut rows = self.rows.lock().unwrap();
        let same_user = |r: &Person| item.user_id.is_some() && r.user_id == item.user_id;
        if rows.iter().any(|r| r.project_id == item.project_id && (r.code == item.code || same_user(r))) {
            return Err(AppError::Conflict("duplicate code or member".into()));
        }
        rows.push(item.clone());
        Ok(())
    }

    async fn update(&self, item: &Person) -> AppResult<()> {
        let mut rows = self.rows.lock().unwrap();
        let row = rows.iter_mut().find(|r| r.project_id == item.project_id && r.code == item.code);
        *row.ok_or_else(|| AppError::NotFound(item.code.clone()))? = item.clone();
        Ok(())
    }

    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Person>> {
        Ok(self.rows.lock().unwrap().iter().find(|r| r.project_id == project_id && r.code == code).cloned())
    }

    async fn find_by_user(&self, project_id: Uuid, user_id: Uuid) -> AppResult<Option<Person>> {
        Ok(self.rows.lock().unwrap().iter().find(|r| r.project_id == project_id && r.user_id == Some(user_id)).cloned())
    }

    async fn of_user(&self, user_id: Uuid) -> AppResult<Vec<Person>> {
        Ok(self.rows.lock().unwrap().iter().filter(|r| r.user_id == Some(user_id)).cloned().collect())
    }

    async fn list(&self, project_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Person>> {
        let rows = self.rows.lock().unwrap();
        let mut items: Vec<_> =
            rows.iter().filter(|r| r.project_id == project_id && (include_archived || !r.archived)).cloned().collect();
        items.sort_by(|a, b| (a.name.to_lowercase(), &a.code).cmp(&(b.name.to_lowercase(), &b.code)));
        Ok(items.into_iter().skip(offset as usize).take(limit as usize).collect())
    }

    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.project_id == project_id && (include_archived || !r.archived))
            .count() as i64)
    }

    async fn codes(&self, project_id: Uuid) -> AppResult<Vec<String>> {
        Ok(self.rows.lock().unwrap().iter().filter(|r| r.project_id == project_id).map(|r| r.code.clone()).collect())
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let mut rows = self.rows.lock().unwrap();
        let n = rows.len();
        rows.retain(|r| r.project_id != project_id);
        Ok((n - rows.len()) as u64)
    }
}
