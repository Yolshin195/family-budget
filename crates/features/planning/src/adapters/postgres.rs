//! Исходящий адаптер: PostgreSQL (таблицы `fb_plans`, `fb_plan_skips`).

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{Name, Plan, Recurrence};
use crate::ports::PlanRepository;

const COLUMNS: &str = "id, project_id, name, name_lang, name_ru, name_en, name_th, tx_kind, account_code, category_code, \
                       amount, recurrence, interval_n, start_on, end_on, active, created_by, to_account_code, series_id, effective_from";

pub struct PostgresPlanRepository {
    pool: PgPool,
}

impl PostgresPlanRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    project_id: Uuid,
    name: String,
    name_lang: String,
    name_ru: Option<String>,
    name_en: Option<String>,
    name_th: Option<String>,
    tx_kind: String,
    account_code: String,
    category_code: Option<String>,
    to_account_code: Option<String>,
    amount: Decimal,
    recurrence: String,
    interval_n: i32,
    start_on: NaiveDate,
    end_on: Option<NaiveDate>,
    active: bool,
    created_by: Uuid,
    series_id: Uuid,
    effective_from: Option<NaiveDate>,
}

impl TryFrom<Row> for Plan {
    type Error = AppError;

    fn try_from(r: Row) -> Result<Self, AppError> {
        let bad = |c: &str, v: &str| AppError::Internal(anyhow::anyhow!("fb_plans.{c}: неизвестное значение '{v}'"));
        Ok(Plan {
            id: r.id,
            project_id: r.project_id,
            name: Name {
                lang: Lang::parse(&r.name_lang).ok_or_else(|| bad("name_lang", &r.name_lang))?,
                text: r.name,
                ru: r.name_ru,
                en: r.name_en,
                th: r.name_th,
            },
            tx_kind: r.tx_kind,
            account_code: r.account_code,
            category_code: r.category_code,
            to_account_code: r.to_account_code,
            amount: r.amount,
            recurrence: Recurrence::parse(&r.recurrence).ok_or_else(|| bad("recurrence", &r.recurrence))?,
            interval: r.interval_n.max(1) as u32,
            start_on: r.start_on,
            end_on: r.end_on,
            active: r.active,
            created_by: r.created_by,
            series_id: r.series_id,
            effective_from: r.effective_from,
        })
    }
}

async fn insert<'e>(db: impl sqlx::PgExecutor<'e>, p: &Plan) -> AppResult<()> {
    sqlx::query(&format!("INSERT INTO fb_plans ({COLUMNS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)"))
        .bind(p.id)
        .bind(p.project_id)
        .bind(&p.name.text)
        .bind(p.name.lang.upper())
        .bind(&p.name.ru)
        .bind(&p.name.en)
        .bind(&p.name.th)
        .bind(&p.tx_kind)
        .bind(&p.account_code)
        .bind(&p.category_code)
        .bind(p.amount)
        .bind(p.recurrence.as_str())
        .bind(p.interval as i32)
        .bind(p.start_on)
        .bind(p.end_on)
        .bind(p.active)
        .bind(p.created_by)
        .bind(&p.to_account_code)
        .bind(p.series_id)
        .bind(p.effective_from)
        .execute(db)
        .await?;
    Ok(())
}

async fn update<'e>(db: impl sqlx::PgExecutor<'e>, p: &Plan) -> AppResult<()> {
    let res = sqlx::query(
        "UPDATE fb_plans SET name=$3, name_lang=$4, name_ru=$5, name_en=$6, name_th=$7, tx_kind=$8, account_code=$9, \
         category_code=$10, amount=$11, recurrence=$12, interval_n=$13, start_on=$14, end_on=$15, active=$16, \
         to_account_code=$17, effective_from=$18 WHERE project_id=$1 AND id=$2",
    )
    .bind(p.project_id)
    .bind(p.id)
    .bind(&p.name.text)
    .bind(p.name.lang.upper())
    .bind(&p.name.ru)
    .bind(&p.name.en)
    .bind(&p.name.th)
    .bind(&p.tx_kind)
    .bind(&p.account_code)
    .bind(&p.category_code)
    .bind(p.amount)
    .bind(p.recurrence.as_str())
    .bind(p.interval as i32)
    .bind(p.start_on)
    .bind(p.end_on)
    .bind(p.active)
    .bind(&p.to_account_code)
    .bind(p.effective_from)
    .execute(db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(p.id.to_string()));
    }
    Ok(())
}

#[async_trait]
impl PlanRepository for PostgresPlanRepository {
    async fn insert(&self, p: &Plan) -> AppResult<()> {
        insert(&self.pool, p).await
    }

    async fn update(&self, p: &Plan) -> AppResult<()> {
        update(&self.pool, p).await
    }

    async fn continue_series(&self, old: &Plan, next: &Plan) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        update(&mut *tx, old).await?;
        insert(&mut *tx, next).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Plan>> {
        let row: Option<Row> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM fb_plans WHERE project_id=$1 AND id=$2"))
            .bind(project_id)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Plan::try_from).transpose()
    }

    async fn list(&self, project_id: Uuid) -> AppResult<Vec<Plan>> {
        let rows: Vec<Row> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM fb_plans WHERE project_id=$1 ORDER BY start_on, id"))
                .bind(project_id)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(Plan::try_from).collect()
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool> {
        let res = sqlx::query("DELETE FROM fb_plans WHERE project_id=$1 AND id=$2")
            .bind(project_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn skips(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<(Uuid, NaiveDate)>> {
        Ok(sqlx::query_as(
            "SELECT plan_id, plan_date FROM fb_plan_skips WHERE project_id=$1 AND plan_date BETWEEN $2 AND $3",
        )
        .bind(project_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn add_skip(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate, by: Uuid) -> AppResult<()> {
        sqlx::query("INSERT INTO fb_plan_skips (plan_id, plan_date, project_id, skipped_by) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(plan_id)
            .bind(date)
            .bind(project_id)
            .bind(by)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn remove_skip(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<()> {
        sqlx::query("DELETE FROM fb_plan_skips WHERE project_id=$1 AND plan_id=$2 AND plan_date=$3")
            .bind(project_id)
            .bind(plan_id)
            .bind(date)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM fb_plan_skips WHERE project_id = $1").bind(project_id).execute(&mut *tx).await?;
        let r = sqlx::query("DELETE FROM fb_plans WHERE project_id = $1").bind(project_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(r.rows_affected())
    }
}
