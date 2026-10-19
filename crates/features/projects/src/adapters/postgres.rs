//! Исходящий адаптер: PostgreSQL (fb_users, fb_projects, fb_project_members, fb_project_invites,
//! fb_sessions, fb_api_tokens, fb_audit_log). Только параметризованные запросы.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use kernel::auth::{Role, Theme};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{ApiToken, AuditEvent, Invite, Member, Project, ProjectKind, SessionRecord, User};
use crate::ports::{AccessRepository, ProjectRepository};

pub struct PostgresProjectRepository {
    pool: PgPool,
}

impl PostgresProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

type UserRow = (Uuid, String, String, String, Option<String>, String, String);
type ProjectRow = (Uuid, String, String, String, Uuid, bool);

fn bad(c: &str, v: &str) -> AppError {
    AppError::Internal(anyhow::anyhow!("{c}: неизвестное значение '{v}'"))
}

fn user(r: UserRow) -> AppResult<User> {
    Ok(User {
        id: r.0,
        rauthy_sub: r.1,
        email: r.2,
        display_name: r.3,
        avatar_url: r.4,
        lang: Lang::parse(&r.5).ok_or_else(|| bad("fb_users.lang", &r.5))?,
        theme: Theme::parse(&r.6).ok_or_else(|| bad("fb_users.theme", &r.6))?,
    })
}

fn project(r: ProjectRow) -> AppResult<Project> {
    Ok(Project {
        id: r.0,
        name: r.1,
        kind: ProjectKind::parse(&r.2).ok_or_else(|| bad("fb_projects.project_kind", &r.2))?,
        base_currency: r.3,
        created_by: r.4,
        require_2fa: r.5,
    })
}

fn role(v: &str) -> AppResult<Role> {
    Role::parse(v).ok_or_else(|| bad("fb_project_members.role", v))
}

const USER_COLS: &str = "u.id, u.rauthy_sub, u.email, u.display_name, u.avatar_url, u.lang, u.theme";
const PROJECT_COLS: &str = "p.id, p.name, p.project_kind, p.base_currency, p.created_by, p.require_2fa";

type InviteRow = (Uuid, Uuid, String, Uuid, DateTime<Utc>, DateTime<Utc>, Option<Uuid>, Option<DateTime<Utc>>, Option<DateTime<Utc>>);
const INVITE_COLS: &str = "id, project_id, role, created_by, created_at, expires_at, used_by, used_at, revoked_at";

fn invite(r: InviteRow) -> AppResult<Invite> {
    let role = Role::parse(&r.2).ok_or_else(|| bad("fb_project_invites.role", &r.2))?;
    Ok(Invite { id: r.0, project_id: r.1, role, created_by: r.3, created_at: r.4, expires_at: r.5, used_by: r.6, used_at: r.7, revoked_at: r.8 })
}

#[async_trait]
impl ProjectRepository for PostgresProjectRepository {
    async fn insert_user(&self, u: &User) -> AppResult<()> {
        sqlx::query("INSERT INTO fb_users (id, rauthy_sub, email, display_name, avatar_url, lang, theme) VALUES ($1, $2, $3, $4, $5, $6, $7)")
            .bind(u.id)
            .bind(&u.rauthy_sub)
            .bind(&u.email)
            .bind(&u.display_name)
            .bind(&u.avatar_url)
            .bind(u.lang.upper())
            .bind(u.theme.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_user_identity(&self, user_id: Uuid, email: &str, name: &str, avatar_url: Option<&str>) -> AppResult<()> {
        sqlx::query("UPDATE fb_users SET email = $2, display_name = $3, avatar_url = $4 WHERE id = $1")
            .bind(user_id)
            .bind(email)
            .bind(name)
            .bind(avatar_url)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_user_profile(&self, user_id: Uuid, name: &str, lang: Lang, theme: Theme) -> AppResult<()> {
        sqlx::query("UPDATE fb_users SET display_name = $2, lang = $3, theme = $4 WHERE id = $1")
            .bind(user_id)
            .bind(name)
            .bind(lang.upper())
            .bind(theme.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_user_lang(&self, user_id: Uuid, lang: Lang) -> AppResult<()> {
        sqlx::query("UPDATE fb_users SET lang = $2 WHERE id = $1").bind(user_id).bind(lang.upper()).execute(&self.pool).await?;
        Ok(())
    }

    async fn user_by_sub(&self, sub: &str) -> AppResult<Option<User>> {
        let r: Option<UserRow> = sqlx::query_as(&format!("SELECT {USER_COLS} FROM fb_users u WHERE u.rauthy_sub = $1")).bind(sub).fetch_optional(&self.pool).await?;
        r.map(user).transpose()
    }

    async fn set_sub(&self, user_id: Uuid, sub: &str) -> AppResult<()> {
        sqlx::query("UPDATE fb_users SET rauthy_sub = $2 WHERE id = $1").bind(user_id).bind(sub).execute(&self.pool).await?;
        Ok(())
    }

    async fn user_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let r: Option<UserRow> = sqlx::query_as(&format!(
            "SELECT {USER_COLS} FROM fb_users u LEFT JOIN LATERAL (SELECT max(s.created_at) AS seen FROM fb_sessions s WHERE s.user_id = u.id) x ON true \
             WHERE lower(u.email) = lower($1) ORDER BY x.seen DESC NULLS LAST, u.created_at DESC LIMIT 1"
        ))
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        r.map(user).transpose()
    }

    async fn user(&self, id: Uuid) -> AppResult<Option<User>> {
        let r: Option<UserRow> = sqlx::query_as(&format!("SELECT {USER_COLS} FROM fb_users u WHERE u.id = $1")).bind(id).fetch_optional(&self.pool).await?;
        r.map(user).transpose()
    }

    async fn delete_user(&self, id: Uuid) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        // Созданные пользователем проекты, которые остаются другим участникам, переходят к ним:
        // created_by — справочная информация, переназначаем на любого оставшегося владельца.
        sqlx::query(
            "UPDATE fb_projects p SET created_by = m.user_id FROM fb_project_members m \
             WHERE p.created_by = $1 AND m.project_id = p.id AND m.user_id <> $1 AND m.role = 'OWNER'",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM fb_users WHERE id = $1").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn insert_project(&self, p: &Project, owner: &Member) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO fb_projects (id, name, project_kind, base_currency, created_by, require_2fa) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(p.id)
            .bind(&p.name)
            .bind(p.kind.as_str())
            .bind(&p.base_currency)
            .bind(p.created_by)
            .bind(p.require_2fa)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO fb_project_members (project_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(owner.project_id)
            .bind(owner.user_id)
            .bind(owner.role.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn rename_project(&self, project_id: Uuid, name: &str) -> AppResult<()> {
        sqlx::query("UPDATE fb_projects SET name = $2 WHERE id = $1").bind(project_id).bind(name).execute(&self.pool).await?;
        Ok(())
    }

    async fn set_require_2fa(&self, project_id: Uuid, value: bool) -> AppResult<()> {
        sqlx::query("UPDATE fb_projects SET require_2fa = $2 WHERE id = $1").bind(project_id).bind(value).execute(&self.pool).await?;
        Ok(())
    }

    async fn set_kind(&self, project_id: Uuid, kind: ProjectKind, name: &str) -> AppResult<()> {
        sqlx::query("UPDATE fb_projects SET project_kind = $2, name = $3 WHERE id = $1").bind(project_id).bind(kind.as_str()).bind(name).execute(&self.pool).await?;
        Ok(())
    }

    async fn delete_project(&self, project_id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM fb_projects WHERE id = $1").bind(project_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn project(&self, id: Uuid) -> AppResult<Option<Project>> {
        let r: Option<ProjectRow> = sqlx::query_as(&format!("SELECT {PROJECT_COLS} FROM fb_projects p WHERE p.id = $1")).bind(id).fetch_optional(&self.pool).await?;
        r.map(project).transpose()
    }

    async fn projects_of(&self, user_id: Uuid) -> AppResult<Vec<(Project, Member)>> {
        let rows: Vec<(Uuid, String, String, String, Uuid, bool, String)> = sqlx::query_as(&format!(
            "SELECT {PROJECT_COLS}, m.role FROM fb_projects p JOIN fb_project_members m ON m.project_id = p.id \
             WHERE m.user_id = $1 ORDER BY (p.project_kind = 'PERSONAL') DESC, p.created_at, p.id"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let m = Member { project_id: r.0, user_id, role: role(&r.6)? };
                Ok((project((r.0, r.1, r.2, r.3, r.4, r.5))?, m))
            })
            .collect()
    }

    async fn members(&self, project_id: Uuid) -> AppResult<Vec<(Member, User)>> {
        let rows: Vec<(String, Uuid, String, String, String, Option<String>, String, String)> = sqlx::query_as(&format!(
            "SELECT m.role, {USER_COLS} FROM fb_project_members m JOIN fb_users u ON u.id = m.user_id \
             WHERE m.project_id = $1 ORDER BY m.joined_at, u.display_name"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| Ok((Member { project_id, user_id: r.1, role: role(&r.0)? }, user((r.1, r.2, r.3, r.4, r.5, r.6, r.7))?)))
            .collect()
    }

    async fn upsert_member(&self, m: &Member) -> AppResult<()> {
        sqlx::query("INSERT INTO fb_project_members (project_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT (project_id, user_id) DO UPDATE SET role = EXCLUDED.role")
            .bind(m.project_id)
            .bind(m.user_id)
            .bind(m.role.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn remove_member(&self, project_id: Uuid, user_id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM fb_project_members WHERE project_id = $1 AND user_id = $2").bind(project_id).bind(user_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn insert_invite(&self, i: &Invite) -> AppResult<()> {
        sqlx::query(&format!("INSERT INTO fb_project_invites ({INVITE_COLS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"))
            .bind(i.id)
            .bind(i.project_id)
            .bind(i.role.as_str())
            .bind(i.created_by)
            .bind(i.created_at)
            .bind(i.expires_at)
            .bind(i.used_by)
            .bind(i.used_at)
            .bind(i.revoked_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn invite(&self, id: Uuid) -> AppResult<Option<Invite>> {
        let r: Option<InviteRow> = sqlx::query_as(&format!("SELECT {INVITE_COLS} FROM fb_project_invites WHERE id = $1")).bind(id).fetch_optional(&self.pool).await?;
        r.map(invite).transpose()
    }

    async fn invites_of(&self, project_id: Uuid) -> AppResult<Vec<Invite>> {
        let rows: Vec<InviteRow> = sqlx::query_as(&format!("SELECT {INVITE_COLS} FROM fb_project_invites WHERE project_id = $1 ORDER BY created_at DESC, id"))
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(invite).collect()
    }

    async fn revoke_invite(&self, project_id: Uuid, id: Uuid, at: DateTime<Utc>) -> AppResult<bool> {
        let r = sqlx::query("UPDATE fb_project_invites SET revoked_at = $3 WHERE id = $2 AND project_id = $1 AND used_at IS NULL AND revoked_at IS NULL")
            .bind(project_id)
            .bind(id)
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn accept_invite(&self, id: Uuid, m: &Member, at: DateTime<Utc>) -> AppResult<bool> {
        let mut tx = self.pool.begin().await?;
        // Условие в UPDATE — защита от гонки: по одной ссылке войдёт только один человек.
        let claimed = sqlx::query(
            "UPDATE fb_project_invites SET used_by = $2, used_at = $3 \
             WHERE id = $1 AND project_id = $4 AND used_at IS NULL AND revoked_at IS NULL AND expires_at > $3",
        )
        .bind(id)
        .bind(m.user_id)
        .bind(at)
        .bind(m.project_id)
        .execute(&mut *tx)
        .await?;
        if claimed.rows_affected() != 1 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO fb_project_members (project_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT (project_id, user_id) DO NOTHING")
            .bind(m.project_id)
            .bind(m.user_id)
            .bind(m.role.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    async fn purge_invites(&self, before: DateTime<Utc>) -> AppResult<u64> {
        let r = sqlx::query("DELETE FROM fb_project_invites WHERE expires_at < $1").bind(before).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }

    async fn count_signups(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> AppResult<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM fb_users WHERE created_at >= $1 AND created_at < $2 AND rauthy_sub NOT LIKE 'demo:%' AND rauthy_sub NOT LIKE 'legacy:%'",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }
}

type SessionRow = (String, Uuid, DateTime<Utc>, DateTime<Utc>, DateTime<Utc>, bool, Option<String>, Option<String>, Option<DateTime<Utc>>);
const SESSION_COLS: &str = "id_hash, user_id, created_at, last_seen_at, expires_at, mfa, ip, user_agent, revoked_at";

fn session(r: SessionRow) -> SessionRecord {
    SessionRecord { id_hash: r.0, user_id: r.1, created_at: r.2, last_seen_at: r.3, expires_at: r.4, mfa: r.5, ip: r.6, user_agent: r.7, revoked_at: r.8 }
}

type TokenRow = (Uuid, Uuid, Uuid, String, String, Vec<String>, Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<DateTime<Utc>>, DateTime<Utc>, bool);
const TOKEN_COLS: &str = "id, user_id, project_id, name, token_hash, scopes, expires_at, last_used_at, revoked_at, created_at, mfa";

fn token(r: TokenRow) -> ApiToken {
    ApiToken { id: r.0, user_id: r.1, project_id: r.2, name: r.3, token_hash: r.4, scopes: r.5, expires_at: r.6, last_used_at: r.7, revoked_at: r.8, created_at: r.9, mfa: r.10 }
}

fn details_json(d: &[(String, String)]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = d.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect();
    serde_json::Value::Object(map).to_string()
}

fn details_pairs(json: &str) -> Vec<(String, String)> {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Object(m)) => m.into_iter().map(|(k, v)| (k, v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))).collect(),
        _ => Vec::new(),
    }
}

#[async_trait]
impl AccessRepository for PostgresProjectRepository {
    async fn insert_session(&self, s: &SessionRecord) -> AppResult<()> {
        sqlx::query(&format!("INSERT INTO fb_sessions ({SESSION_COLS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"))
            .bind(&s.id_hash)
            .bind(s.user_id)
            .bind(s.created_at)
            .bind(s.last_seen_at)
            .bind(s.expires_at)
            .bind(s.mfa)
            .bind(&s.ip)
            .bind(&s.user_agent)
            .bind(s.revoked_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn session(&self, id_hash: &str) -> AppResult<Option<SessionRecord>> {
        let r: Option<SessionRow> = sqlx::query_as(&format!("SELECT {SESSION_COLS} FROM fb_sessions WHERE id_hash = $1")).bind(id_hash).fetch_optional(&self.pool).await?;
        Ok(r.map(session))
    }

    async fn touch_session(&self, id_hash: &str, at: DateTime<Utc>) -> AppResult<()> {
        sqlx::query("UPDATE fb_sessions SET last_seen_at = $2 WHERE id_hash = $1").bind(id_hash).bind(at).execute(&self.pool).await?;
        Ok(())
    }

    async fn sessions_of(&self, user_id: Uuid, now: DateTime<Utc>) -> AppResult<Vec<SessionRecord>> {
        let rows: Vec<SessionRow> = sqlx::query_as(&format!(
            "SELECT {SESSION_COLS} FROM fb_sessions WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > $2 \
             AND last_seen_at > $2 - interval '7 days' ORDER BY last_seen_at DESC"
        ))
        .bind(user_id)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(session).collect())
    }

    async fn revoke_session(&self, user_id: Uuid, id_hash: &str, at: DateTime<Utc>) -> AppResult<bool> {
        let r = sqlx::query("UPDATE fb_sessions SET revoked_at = $3 WHERE id_hash = $2 AND user_id = $1 AND revoked_at IS NULL")
            .bind(user_id)
            .bind(id_hash)
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    async fn revoke_sessions(&self, user_id: Uuid, keep: Option<&str>, at: DateTime<Utc>) -> AppResult<u64> {
        let r = sqlx::query("UPDATE fb_sessions SET revoked_at = $3 WHERE user_id = $1 AND revoked_at IS NULL AND id_hash IS DISTINCT FROM $2")
            .bind(user_id)
            .bind(keep)
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    async fn purge_sessions(&self, before: DateTime<Utc>) -> AppResult<u64> {
        let r = sqlx::query("DELETE FROM fb_sessions WHERE expires_at < $1 OR revoked_at < $1 OR last_seen_at < $1").bind(before).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }

    async fn insert_token(&self, t: &ApiToken) -> AppResult<()> {
        sqlx::query(&format!("INSERT INTO fb_api_tokens ({TOKEN_COLS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"))
            .bind(t.id)
            .bind(t.user_id)
            .bind(t.project_id)
            .bind(&t.name)
            .bind(&t.token_hash)
            .bind(&t.scopes)
            .bind(t.expires_at)
            .bind(t.last_used_at)
            .bind(t.revoked_at)
            .bind(t.created_at)
            .bind(t.mfa)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn token_by_hash(&self, token_hash: &str) -> AppResult<Option<ApiToken>> {
        let r: Option<TokenRow> = sqlx::query_as(&format!("SELECT {TOKEN_COLS} FROM fb_api_tokens WHERE token_hash = $1")).bind(token_hash).fetch_optional(&self.pool).await?;
        Ok(r.map(token))
    }

    async fn touch_token(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        sqlx::query("UPDATE fb_api_tokens SET last_used_at = $2 WHERE id = $1").bind(id).bind(at).execute(&self.pool).await?;
        Ok(())
    }

    async fn tokens_of(&self, user_id: Uuid) -> AppResult<Vec<ApiToken>> {
        let rows: Vec<TokenRow> = sqlx::query_as(&format!("SELECT {TOKEN_COLS} FROM fb_api_tokens WHERE user_id = $1 ORDER BY created_at DESC")).bind(user_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(token).collect())
    }

    async fn revoke_token(&self, user_id: Uuid, id: Uuid, at: DateTime<Utc>) -> AppResult<bool> {
        let r = sqlx::query("UPDATE fb_api_tokens SET revoked_at = $3 WHERE id = $2 AND user_id = $1 AND revoked_at IS NULL")
            .bind(user_id)
            .bind(id)
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    async fn revoke_project_tokens(&self, user_id: Uuid, project_id: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        sqlx::query("UPDATE fb_api_tokens SET revoked_at = $3 WHERE user_id = $1 AND project_id = $2 AND revoked_at IS NULL")
            .bind(user_id)
            .bind(project_id)
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn audit(&self, e: &AuditEvent) -> AppResult<()> {
        sqlx::query("INSERT INTO fb_audit_log (id, at, user_id, project_id, action, ip, user_agent, details) VALUES ($1, $2, $3, $4, $5, $6, $7, $8::jsonb)")
            .bind(e.id)
            .bind(e.at)
            .bind(e.user_id)
            .bind(e.project_id)
            .bind(&e.action)
            .bind(&e.ip)
            .bind(&e.user_agent)
            .bind(details_json(&e.details))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn audit_of(&self, user_id: Uuid, limit: i64) -> AppResult<Vec<AuditEvent>> {
        let rows: Vec<(Uuid, DateTime<Utc>, Option<Uuid>, Option<Uuid>, String, Option<String>, Option<String>, String)> = sqlx::query_as(
            "SELECT id, at, user_id, project_id, action, ip, user_agent, details::text FROM fb_audit_log WHERE user_id = $1 ORDER BY at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| AuditEvent { id: r.0, at: r.1, user_id: r.2, project_id: r.3, action: r.4, ip: r.5, user_agent: r.6, details: details_pairs(&r.7) })
            .collect())
    }

    async fn anonymize_audit(&self, user_id: Uuid) -> AppResult<()> {
        sqlx::query("UPDATE fb_audit_log SET user_id = NULL, ip = NULL, user_agent = NULL WHERE user_id = $1").bind(user_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn purge_audit(&self, before: DateTime<Utc>) -> AppResult<u64> {
        let r = sqlx::query("DELETE FROM fb_audit_log WHERE at < $1").bind(before).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }
}
