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

fn matches(r: &Rate, user_id: Uuid, p: &Period) -> bool {
    r.user_id == user_id
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
        let row = rows.iter_mut().find(|r| r.user_id == item.user_id && r.id == item.id);
        *row.ok_or_else(|| AppError::NotFound(item.id.to_string()))? = item.clone();
        Ok(())
    }

    async fn find(&self, user_id: Uuid, id: Uuid) -> AppResult<Option<Rate>> {
        Ok(self.rows.lock().unwrap().iter().find(|r| r.user_id == user_id && r.id == id).cloned())
    }

    async fn list(&self, user_id: Uuid, period: Period, limit: i64, offset: i64) -> AppResult<Vec<Rate>> {
        let rows = self.rows.lock().unwrap();
        let mut items: Vec<_> = rows.iter().filter(|r| matches(r, user_id, &period)).cloned().collect();
        items.sort_by(|a, b| b.rate_date.cmp(&a.rate_date).then(a.id.cmp(&b.id)));
        Ok(items.into_iter().skip(offset as usize).take(limit as usize).collect())
    }

    async fn count(&self, user_id: Uuid, period: Period) -> AppResult<i64> {
        Ok(self.rows.lock().unwrap().iter().filter(|r| matches(r, user_id, &period)).count() as i64)
    }

    async fn delete(&self, user_id: Uuid, id: Uuid) -> AppResult<bool> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| !(r.user_id == user_id && r.id == id));
        Ok(rows.len() < before)
    }
}
