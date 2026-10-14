//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use uuid::Uuid;

use crate::domain::Rate;
use crate::ports::{Period, RateRepository};

#[derive(Default)]
pub struct InMemoryRateRepository {
    rows: Mutex<Vec<Rate>>,
}

fn matches(r: &Rate, project_id: Uuid, p: &Period) -> bool {
    r.project_id == project_id
        && p.from.is_none_or(|from| r.rate_date >= from)
        && p.to.is_none_or(|to| r.rate_date <= to)
}

#[async_trait]
impl RateRepository for InMemoryRateRepository {
    async fn insert(&self, item: &Rate) -> AppResult<()> {
        self.rows.lock().unwrap().push(item.clone());
        Ok(())
    }

    async fn update(&self, item: &Rate) -> AppResult<()> {
        let mut rows = self.rows.lock().unwrap();
        let row = rows.iter_mut().find(|r| r.project_id == item.project_id && r.id == item.id);
        *row.ok_or_else(|| AppError::NotFound(item.id.to_string()))? = item.clone();
        Ok(())
    }

    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Rate>> {
        Ok(self.rows.lock().unwrap().iter().find(|r| r.project_id == project_id && r.id == id).cloned())
    }

    async fn list(&self, project_id: Uuid, period: Period, limit: i64, offset: i64) -> AppResult<Vec<Rate>> {
        let rows = self.rows.lock().unwrap();
        let mut items: Vec<_> = rows.iter().filter(|r| matches(r, project_id, &period)).cloned().collect();
        items.sort_by(|a, b| b.rate_date.cmp(&a.rate_date).then(a.id.cmp(&b.id)));
        Ok(items.into_iter().skip(offset as usize).take(limit as usize).collect())
    }

    async fn count(&self, project_id: Uuid, period: Period) -> AppResult<i64> {
        Ok(self.rows.lock().unwrap().iter().filter(|r| matches(r, project_id, &period)).count() as i64)
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| !(r.project_id == project_id && r.id == id));
        Ok(rows.len() < before)
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let mut rows = self.rows.lock().unwrap();
        let n = rows.len();
        rows.retain(|r| r.project_id != project_id);
        Ok((n - rows.len()) as u64)
    }
}
