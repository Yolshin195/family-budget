//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use uuid::Uuid;

use crate::domain::{Member, Project, User};
use crate::ports::ProjectRepository;

#[derive(Default)]
pub struct InMemoryProjectRepository {
    users: Mutex<Vec<User>>,
    projects: Mutex<Vec<Project>>,
    members: Mutex<Vec<Member>>,
}

#[async_trait]
impl ProjectRepository for InMemoryProjectRepository {
    async fn insert_user(&self, user: &User) -> AppResult<()> {
        let mut u = self.users.lock().unwrap();
        if u.iter().any(|x| x.email == user.email) {
            return Err(AppError::Conflict("email".into()));
        }
        u.push(user.clone());
        Ok(())
    }
    async fn update_user_lang(&self, user_id: Uuid, lang: Lang) -> AppResult<()> {
        for u in self.users.lock().unwrap().iter_mut().filter(|u| u.id == user_id) {
            u.lang = lang;
        }
        Ok(())
    }
    async fn user_by_email(&self, email: &str) -> AppResult<Option<User>> {
        Ok(self.users.lock().unwrap().iter().find(|u| u.email == email).cloned())
    }
    async fn user(&self, id: Uuid) -> AppResult<Option<User>> {
        Ok(self.users.lock().unwrap().iter().find(|u| u.id == id).cloned())
    }
    async fn insert_project(&self, project: &Project, owner: &Member) -> AppResult<()> {
        self.projects.lock().unwrap().push(project.clone());
        self.members.lock().unwrap().push(owner.clone());
        Ok(())
    }
    async fn rename_project(&self, project_id: Uuid, name: &str) -> AppResult<()> {
        for p in self.projects.lock().unwrap().iter_mut().filter(|p| p.id == project_id) {
            p.name = name.into();
        }
        Ok(())
    }
    async fn project(&self, id: Uuid) -> AppResult<Option<Project>> {
        Ok(self.projects.lock().unwrap().iter().find(|p| p.id == id).cloned())
    }
    async fn projects_of(&self, user_id: Uuid) -> AppResult<Vec<(Project, Member)>> {
        let ms = self.members.lock().unwrap();
        let ps = self.projects.lock().unwrap();
        Ok(ms.iter().filter(|m| m.user_id == user_id).filter_map(|m| ps.iter().find(|p| p.id == m.project_id).map(|p| (p.clone(), m.clone()))).collect())
    }
    async fn members(&self, project_id: Uuid) -> AppResult<Vec<(Member, User)>> {
        let us = self.users.lock().unwrap();
        Ok(self
            .members
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.project_id == project_id)
            .filter_map(|m| us.iter().find(|u| u.id == m.user_id).map(|u| (m.clone(), u.clone())))
            .collect())
    }
    async fn upsert_member(&self, member: &Member) -> AppResult<()> {
        let mut ms = self.members.lock().unwrap();
        ms.retain(|m| !(m.project_id == member.project_id && m.user_id == member.user_id));
        ms.push(member.clone());
        Ok(())
    }
    async fn remove_member(&self, project_id: Uuid, user_id: Uuid) -> AppResult<()> {
        self.members.lock().unwrap().retain(|m| !(m.project_id == project_id && m.user_id == user_id));
        Ok(())
    }
}
