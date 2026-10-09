//! Исходящий адаптер: PostgreSQL (fb_users, fb_projects, fb_project_members).

use async_trait::async_trait;
use kernel::auth::Role;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{Member, Project, ProjectKind, User};
use crate::ports::ProjectRepository;

pub struct PostgresProjectRepository {
    pool: PgPool,
}

impl PostgresProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

type UserRow = (Uuid, String, String, String, String);
type ProjectRow = (Uuid, String, String, String, Uuid);

fn bad(c: &str, v: &str) -> AppError {
    AppError::Internal(anyhow::anyhow!("{c}: неизвестное значение '{v}'"))
}

fn user(r: UserRow) -> AppResult<User> {
    Ok(User { id: r.0, email: r.1, display_name: r.2, password_hash: r.3, lang: Lang::parse(&r.4).ok_or_else(|| bad("fb_users.lang", &r.4))? })
}

fn project(r: ProjectRow) -> AppResult<Project> {
    Ok(Project { id: r.0, name: r.1, kind: ProjectKind::parse(&r.2).ok_or_else(|| bad("fb_projects.project_kind", &r.2))?, base_currency: r.3, created_by: r.4 })
}

fn role(v: &str) -> AppResult<Role> {
    Role::parse(v).ok_or_else(|| bad("fb_project_members.role", v))
}

const USER_COLS: &str = "id, email, display_name, password_hash, lang";
const PROJECT_COLS: &str = "p.id, p.name, p.project_kind, p.base_currency, p.created_by";

#[async_trait]
impl ProjectRepository for PostgresProjectRepository {
    async fn insert_user(&self, u: &User) -> AppResult<()> {
        sqlx::query("INSERT INTO fb_users (id, email, display_name, password_hash, lang) VALUES ($1, $2, $3, $4, $5)")
            .bind(u.id)
            .bind(&u.email)
            .bind(&u.display_name)
            .bind(&u.password_hash)
            .bind(u.lang.upper())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_user_lang(&self, user_id: Uuid, lang: Lang) -> AppResult<()> {
        sqlx::query("UPDATE fb_users SET lang = $2 WHERE id = $1").bind(user_id).bind(lang.upper()).execute(&self.pool).await?;
        Ok(())
    }

    async fn user_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let r: Option<UserRow> = sqlx::query_as(&format!("SELECT {USER_COLS} FROM fb_users WHERE lower(email) = lower($1)"))
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        r.map(user).transpose()
    }

    async fn user(&self, id: Uuid) -> AppResult<Option<User>> {
        let r: Option<UserRow> = sqlx::query_as(&format!("SELECT {USER_COLS} FROM fb_users WHERE id = $1")).bind(id).fetch_optional(&self.pool).await?;
        r.map(user).transpose()
    }

    async fn insert_project(&self, p: &Project, owner: &Member) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO fb_projects (id, name, project_kind, base_currency, created_by) VALUES ($1, $2, $3, $4, $5)")
            .bind(p.id)
            .bind(&p.name)
            .bind(p.kind.as_str())
            .bind(&p.base_currency)
            .bind(p.created_by)
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

    async fn project(&self, id: Uuid) -> AppResult<Option<Project>> {
        let r: Option<ProjectRow> = sqlx::query_as(&format!("SELECT {PROJECT_COLS} FROM fb_projects p WHERE p.id = $1")).bind(id).fetch_optional(&self.pool).await?;
        r.map(project).transpose()
    }

    async fn projects_of(&self, user_id: Uuid) -> AppResult<Vec<(Project, Member)>> {
        let rows: Vec<(Uuid, String, String, String, Uuid, String)> = sqlx::query_as(&format!(
            "SELECT {PROJECT_COLS}, m.role FROM fb_projects p JOIN fb_project_members m ON m.project_id = p.id \
             WHERE m.user_id = $1 ORDER BY (p.project_kind = 'PERSONAL') DESC, p.created_at, p.id"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let m = Member { project_id: r.0, user_id, role: role(&r.5)? };
                Ok((project((r.0, r.1, r.2, r.3, r.4))?, m))
            })
            .collect()
    }

    async fn members(&self, project_id: Uuid) -> AppResult<Vec<(Member, User)>> {
        let rows: Vec<(String, Uuid, String, String, String, String)> = sqlx::query_as(
            "SELECT m.role, u.id, u.email, u.display_name, u.password_hash, u.lang FROM fb_project_members m \
             JOIN fb_users u ON u.id = m.user_id WHERE m.project_id = $1 ORDER BY m.joined_at, u.display_name",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| Ok((Member { project_id, user_id: r.1, role: role(&r.0)? }, user((r.1, r.2, r.3, r.4, r.5))?)))
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
}
