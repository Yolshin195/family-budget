//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::{AppError, AppResult};
use uuid::Uuid;

use crate::domain::Plan;
use crate::ports::PlanRepository;

#[derive(Default)]
pub struct InMemoryPlanRepository {
    plans: Mutex<Vec<Plan>>,
    skips: Mutex<Vec<(Uuid, Uuid, NaiveDate)>>,
}

#[async_trait]
impl PlanRepository for InMemoryPlanRepository {
    async fn insert(&self, plan: &Plan) -> AppResult<()> {
        self.plans.lock().unwrap().push(plan.clone());
        Ok(())
    }

    async fn update(&self, plan: &Plan) -> AppResult<()> {
        let mut rows = self.plans.lock().unwrap();
        let row = rows.iter_mut().find(|p| p.project_id == plan.project_id && p.id == plan.id);
        *row.ok_or_else(|| AppError::NotFound(plan.id.to_string()))? = plan.clone();
        Ok(())
    }

    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Plan>> {
        Ok(self.plans.lock().unwrap().iter().find(|p| p.project_id == project_id && p.id == id).cloned())
    }

    async fn list(&self, project_id: Uuid) -> AppResult<Vec<Plan>> {
        Ok(self.plans.lock().unwrap().iter().filter(|p| p.project_id == project_id).cloned().collect())
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool> {
        let mut rows = self.plans.lock().unwrap();
        let before = rows.len();
        rows.retain(|p| !(p.project_id == project_id && p.id == id));
        self.skips.lock().unwrap().retain(|s| s.1 != id);
        Ok(rows.len() < before)
    }

    async fn skips(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<(Uuid, NaiveDate)>> {
        Ok(self.skips.lock().unwrap().iter().filter(|s| s.0 == project_id && s.2 >= from && s.2 <= to).map(|s| (s.1, s.2)).collect())
    }

    async fn add_skip(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate, _by: Uuid) -> AppResult<()> {
        let mut s = self.skips.lock().unwrap();
        if !s.contains(&(project_id, plan_id, date)) {
            s.push((project_id, plan_id, date));
        }
        Ok(())
    }

    async fn remove_skip(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<()> {
        self.skips.lock().unwrap().retain(|s| *s != (project_id, plan_id, date));
        Ok(())
    }
}
