//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use kernel::auth::{Role, Theme};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use uuid::Uuid;

use crate::domain::{ApiToken, AuditEvent, Invite, InviteState, Member, Project, ProjectKind, SessionRecord, User};
use crate::ports::{AccessRepository, ProjectRepository};

#[derive(Default)]
pub struct InMemoryProjectRepository {
    users: Mutex<Vec<User>>,
    projects: Mutex<Vec<Project>>,
    members: Mutex<Vec<Member>>,
    pub invites: Mutex<Vec<Invite>>,
    pub sessions: Mutex<Vec<SessionRecord>>,
    tokens: Mutex<Vec<ApiToken>>,
    pub audit: Mutex<Vec<AuditEvent>>,
}

#[async_trait]
impl ProjectRepository for InMemoryProjectRepository {
    async fn insert_user(&self, user: &User) -> AppResult<()> {
        let mut u = self.users.lock().unwrap();
        if u.iter().any(|x| x.rauthy_sub == user.rauthy_sub) {
            return Err(AppError::Conflict("sub".into()));
        }
        u.push(user.clone());
        Ok(())
    }
    async fn update_user_identity(&self, user_id: Uuid, email: &str, name: &str, avatar_url: Option<&str>) -> AppResult<()> {
        for u in self.users.lock().unwrap().iter_mut().filter(|u| u.id == user_id) {
            u.email = email.into();
            u.display_name = name.into();
            u.avatar_url = avatar_url.map(Into::into);
        }
        Ok(())
    }
    async fn update_user_profile(&self, user_id: Uuid, name: &str, lang: Lang, theme: Theme) -> AppResult<()> {
        for u in self.users.lock().unwrap().iter_mut().filter(|u| u.id == user_id) {
            u.display_name = name.into();
            u.lang = lang;
            u.theme = theme;
        }
        Ok(())
    }
    async fn update_user_lang(&self, user_id: Uuid, lang: Lang) -> AppResult<()> {
        for u in self.users.lock().unwrap().iter_mut().filter(|u| u.id == user_id) {
            u.lang = lang;
        }
        Ok(())
    }
    async fn user_by_sub(&self, sub: &str) -> AppResult<Option<User>> {
        Ok(self.users.lock().unwrap().iter().find(|u| u.rauthy_sub == sub).cloned())
    }
    async fn set_sub(&self, user_id: Uuid, sub: &str) -> AppResult<()> {
        for u in self.users.lock().unwrap().iter_mut().filter(|u| u.id == user_id) {
            u.rauthy_sub = sub.into();
        }
        Ok(())
    }
    async fn user_by_email(&self, email: &str) -> AppResult<Option<User>> {
        Ok(self.users.lock().unwrap().iter().rev().find(|u| u.email.eq_ignore_ascii_case(email)).cloned())
    }
    async fn user(&self, id: Uuid) -> AppResult<Option<User>> {
        Ok(self.users.lock().unwrap().iter().find(|u| u.id == id).cloned())
    }
    async fn delete_user(&self, id: Uuid) -> AppResult<()> {
        self.users.lock().unwrap().retain(|u| u.id != id);
        self.members.lock().unwrap().retain(|m| m.user_id != id);
        self.invites.lock().unwrap().retain(|i| i.created_by != id);
        self.sessions.lock().unwrap().retain(|s| s.user_id != id);
        self.tokens.lock().unwrap().retain(|t| t.user_id != id);
        Ok(())
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
    async fn set_require_2fa(&self, project_id: Uuid, value: bool) -> AppResult<()> {
        for p in self.projects.lock().unwrap().iter_mut().filter(|p| p.id == project_id) {
            p.require_2fa = value;
        }
        Ok(())
    }
    async fn set_kind(&self, project_id: Uuid, kind: ProjectKind, name: &str) -> AppResult<()> {
        for p in self.projects.lock().unwrap().iter_mut().filter(|p| p.id == project_id) {
            p.kind = kind;
            p.name = name.into();
        }
        Ok(())
    }
    async fn delete_project(&self, project_id: Uuid) -> AppResult<()> {
        self.projects.lock().unwrap().retain(|p| p.id != project_id);
        self.invites.lock().unwrap().retain(|i| i.project_id != project_id);
        self.members.lock().unwrap().retain(|m| m.project_id != project_id);
        self.tokens.lock().unwrap().retain(|t| t.project_id != project_id);
        Ok(())
    }
    async fn project(&self, id: Uuid) -> AppResult<Option<Project>> {
        Ok(self.projects.lock().unwrap().iter().find(|p| p.id == id).cloned())
    }
    async fn projects_of(&self, user_id: Uuid) -> AppResult<Vec<(Project, Member)>> {
        let ms = self.members.lock().unwrap();
        let ps = self.projects.lock().unwrap();
        let mut out: Vec<(Project, Member)> =
            ms.iter().filter(|m| m.user_id == user_id).filter_map(|m| ps.iter().find(|p| p.id == m.project_id).map(|p| (p.clone(), m.clone()))).collect();
        out.sort_by_key(|(p, _)| p.kind != ProjectKind::Personal);
        Ok(out)
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
    async fn update_role(&self, project_id: Uuid, user_id: Uuid, role: Role) -> AppResult<bool> {
        let mut ms = self.members.lock().unwrap();
        let mut found = false;
        for m in ms.iter_mut().filter(|m| m.project_id == project_id && m.user_id == user_id) {
            m.role = role;
            found = true;
        }
        Ok(found)
    }
    async fn remove_member(&self, project_id: Uuid, user_id: Uuid) -> AppResult<()> {
        self.members.lock().unwrap().retain(|m| !(m.project_id == project_id && m.user_id == user_id));
        Ok(())
    }
    async fn insert_invite(&self, invite: &Invite) -> AppResult<()> {
        self.invites.lock().unwrap().push(invite.clone());
        Ok(())
    }
    async fn invite(&self, id: Uuid) -> AppResult<Option<Invite>> {
        Ok(self.invites.lock().unwrap().iter().find(|i| i.id == id).cloned())
    }
    async fn invites_of(&self, project_id: Uuid) -> AppResult<Vec<Invite>> {
        let mut out: Vec<Invite> = self.invites.lock().unwrap().iter().filter(|i| i.project_id == project_id).cloned().collect();
        out.sort_by_key(|i| std::cmp::Reverse(i.created_at));
        Ok(out)
    }
    async fn revoke_invite(&self, project_id: Uuid, id: Uuid, at: DateTime<Utc>) -> AppResult<bool> {
        let mut is = self.invites.lock().unwrap();
        let Some(i) = is.iter_mut().find(|i| i.id == id && i.project_id == project_id && i.used_at.is_none() && i.revoked_at.is_none()) else { return Ok(false) };
        i.revoked_at = Some(at);
        Ok(true)
    }
    async fn accept_invite(&self, id: Uuid, member: &Member, at: DateTime<Utc>) -> AppResult<bool> {
        {
            let mut is = self.invites.lock().unwrap();
            let Some(i) = is.iter_mut().find(|i| i.id == id && i.project_id == member.project_id && i.state(at) == InviteState::Active) else { return Ok(false) };
            i.used_by = Some(member.user_id);
            i.used_at = Some(at);
        }
        let mut ms = self.members.lock().unwrap();
        if !ms.iter().any(|m| m.project_id == member.project_id && m.user_id == member.user_id) {
            ms.push(member.clone());
        }
        Ok(true)
    }
    async fn purge_invites(&self, before: DateTime<Utc>) -> AppResult<u64> {
        let mut is = self.invites.lock().unwrap();
        let n = is.len();
        is.retain(|i| i.expires_at >= before);
        Ok((n - is.len()) as u64)
    }
    async fn count_signups(&self, _from: DateTime<Utc>, _to: DateTime<Utc>) -> AppResult<i64> {
        Ok(self.users.lock().unwrap().iter().filter(|u| !u.rauthy_sub.starts_with("demo:")).count() as i64)
    }
}

#[async_trait]
impl AccessRepository for InMemoryProjectRepository {
    async fn insert_session(&self, s: &SessionRecord) -> AppResult<()> {
        self.sessions.lock().unwrap().push(s.clone());
        Ok(())
    }
    async fn session(&self, id_hash: &str) -> AppResult<Option<SessionRecord>> {
        Ok(self.sessions.lock().unwrap().iter().find(|s| s.id_hash == id_hash).cloned())
    }
    async fn touch_session(&self, id_hash: &str, at: DateTime<Utc>) -> AppResult<()> {
        for s in self.sessions.lock().unwrap().iter_mut().filter(|s| s.id_hash == id_hash) {
            s.last_seen_at = at;
        }
        Ok(())
    }
    async fn sessions_of(&self, user_id: Uuid, now: DateTime<Utc>) -> AppResult<Vec<SessionRecord>> {
        Ok(self.sessions.lock().unwrap().iter().filter(|s| s.user_id == user_id && s.active(now)).cloned().collect())
    }
    async fn revoke_session(&self, user_id: Uuid, id_hash: &str, at: DateTime<Utc>) -> AppResult<bool> {
        let mut n = false;
        for s in self.sessions.lock().unwrap().iter_mut().filter(|s| s.user_id == user_id && s.id_hash == id_hash && s.revoked_at.is_none()) {
            s.revoked_at = Some(at);
            n = true;
        }
        Ok(n)
    }
    async fn revoke_sessions(&self, user_id: Uuid, keep: Option<&str>, at: DateTime<Utc>) -> AppResult<u64> {
        let mut n = 0;
        for s in self.sessions.lock().unwrap().iter_mut().filter(|s| s.user_id == user_id && s.revoked_at.is_none() && Some(s.id_hash.as_str()) != keep) {
            s.revoked_at = Some(at);
            n += 1;
        }
        Ok(n)
    }
    async fn purge_sessions(&self, before: DateTime<Utc>) -> AppResult<u64> {
        let mut s = self.sessions.lock().unwrap();
        let n = s.len();
        s.retain(|x| x.expires_at >= before && x.revoked_at.is_none_or(|r| r >= before));
        Ok((n - s.len()) as u64)
    }
    async fn insert_token(&self, t: &ApiToken) -> AppResult<()> {
        self.tokens.lock().unwrap().push(t.clone());
        Ok(())
    }
    async fn token_by_hash(&self, token_hash: &str) -> AppResult<Option<ApiToken>> {
        Ok(self.tokens.lock().unwrap().iter().find(|t| t.token_hash == token_hash).cloned())
    }
    async fn touch_token(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        for t in self.tokens.lock().unwrap().iter_mut().filter(|t| t.id == id) {
            t.last_used_at = Some(at);
        }
        Ok(())
    }
    async fn tokens_of(&self, user_id: Uuid) -> AppResult<Vec<ApiToken>> {
        Ok(self.tokens.lock().unwrap().iter().filter(|t| t.user_id == user_id).cloned().collect())
    }
    async fn revoke_token(&self, user_id: Uuid, id: Uuid, at: DateTime<Utc>) -> AppResult<bool> {
        let mut n = false;
        for t in self.tokens.lock().unwrap().iter_mut().filter(|t| t.user_id == user_id && t.id == id && t.revoked_at.is_none()) {
            t.revoked_at = Some(at);
            n = true;
        }
        Ok(n)
    }
    async fn revoke_project_tokens(&self, user_id: Uuid, project_id: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        for t in self.tokens.lock().unwrap().iter_mut().filter(|t| t.user_id == user_id && t.project_id == project_id && t.revoked_at.is_none()) {
            t.revoked_at = Some(at);
        }
        Ok(())
    }
    async fn audit(&self, e: &AuditEvent) -> AppResult<()> {
        self.audit.lock().unwrap().push(e.clone());
        Ok(())
    }
    async fn audit_of(&self, user_id: Uuid, limit: i64) -> AppResult<Vec<AuditEvent>> {
        Ok(self.audit.lock().unwrap().iter().rev().filter(|e| e.user_id == Some(user_id)).take(limit as usize).cloned().collect())
    }
    async fn anonymize_audit(&self, user_id: Uuid) -> AppResult<()> {
        for e in self.audit.lock().unwrap().iter_mut().filter(|e| e.user_id == Some(user_id)) {
            e.user_id = None;
            e.ip = None;
            e.user_agent = None;
        }
        Ok(())
    }
    async fn purge_audit(&self, before: DateTime<Utc>) -> AppResult<u64> {
        let mut a = self.audit.lock().unwrap();
        let n = a.len();
        a.retain(|e| e.at >= before);
        Ok((n - a.len()) as u64)
    }
}
