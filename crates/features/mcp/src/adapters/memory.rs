//! Исходящий адаптер: доступы ассистентов в памяти (тесты).

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use kernel::error::AppResult;
use uuid::Uuid;

use crate::domain::Grant;
use crate::ports::GrantRepository;

#[derive(Default)]
pub struct InMemoryGrantRepository {
    items: Mutex<Vec<Grant>>,
}

#[async_trait]
impl GrantRepository for InMemoryGrantRepository {
    async fn grant(&self, user_id: Uuid, client_id: &str) -> AppResult<Option<Grant>> {
        Ok(self.items.lock().unwrap().iter().find(|g| g.user_id == user_id && g.client_id == client_id).cloned())
    }

    async fn grants_of(&self, user_id: Uuid) -> AppResult<Vec<Grant>> {
        Ok(self.items.lock().unwrap().iter().filter(|g| g.user_id == user_id).cloned().collect())
    }

    async fn save(&self, grant: &Grant) -> AppResult<()> {
        let mut items = self.items.lock().unwrap();
        items.retain(|g| !(g.user_id == grant.user_id && g.client_id == grant.client_id));
        items.push(grant.clone());
        Ok(())
    }

    async fn delete(&self, user_id: Uuid, client_id: &str) -> AppResult<bool> {
        let mut items = self.items.lock().unwrap();
        let before = items.len();
        items.retain(|g| !(g.user_id == user_id && g.client_id == client_id));
        Ok(items.len() < before)
    }

    async fn touch(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        if let Some(g) = self.items.lock().unwrap().iter_mut().find(|g| g.id == id) {
            g.last_used_at = Some(at);
        }
        Ok(())
    }

    async fn delete_project(&self, project_id: Uuid) -> AppResult<u64> {
        let mut items = self.items.lock().unwrap();
        let before = items.len();
        items.retain(|g| g.project_id != project_id);
        Ok((before - items.len()) as u64)
    }
}
