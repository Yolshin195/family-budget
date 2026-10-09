//! Исходящий адаптер: порт на PostgreSQL (таблицы `fb_transactions`, `fb_transaction_lines`).

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction as SqlTx};
use uuid::Uuid;

use crate::domain::{Description, Line, Transaction, TxKind};
use crate::ports::{FactLine, Filter, Payment, TransactionRepository};

const COLUMNS: &str = "id, project_id, occurred_on, account_code, tx_kind, description, description_lang, \
                       description_ru, description_en, description_th, created_by, plan_id, plan_date";
/// $1 проект, $2..$3 период, $4 счёт, $5 автор, $6 язык «без перевода» (NULL — условие выключено).
const FILTER: &str = "project_id = $1 \
    AND ($2::date IS NULL OR occurred_on >= $2) AND ($3::date IS NULL OR occurred_on <= $3) \
    AND ($4::text IS NULL OR account_code = $4) AND ($5::uuid IS NULL OR created_by = $5) \
    AND ($6::text IS NULL OR (description IS NOT NULL AND description_lang <> $6 AND \
         (CASE $6 WHEN 'RU' THEN description_ru WHEN 'EN' THEN description_en ELSE description_th END) IS NULL))";

pub struct PostgresTransactionRepository {
    pool: PgPool,
}

impl PostgresTransactionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn with_lines(&self, rows: Vec<Row>) -> AppResult<Vec<Transaction>> {
        let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
        let lines: Vec<LineRow> = sqlx::query_as(
            "SELECT transaction_id, position, category_code, amount, note FROM fb_transaction_lines \
             WHERE transaction_id = ANY($1) ORDER BY transaction_id, position",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let mine = lines
                    .iter()
                    .filter(|l| l.transaction_id == r.id)
                    .map(|l| Line { category_code: l.category_code.clone(), amount: l.amount, note: l.note.clone() })
                    .collect();
                r.into_domain(mine)
            })
            .collect()
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    project_id: Uuid,
    occurred_on: NaiveDate,
    account_code: String,
    tx_kind: String,
    description: Option<String>,
    description_lang: String,
    description_ru: Option<String>,
    description_en: Option<String>,
    description_th: Option<String>,
    created_by: Uuid,
    plan_id: Option<Uuid>,
    plan_date: Option<NaiveDate>,
}

#[derive(sqlx::FromRow)]
struct LineRow {
    transaction_id: Uuid,
    #[allow(dead_code)]
    position: i32,
    category_code: String,
    amount: Decimal,
    note: Option<String>,
}

impl Row {
    fn into_domain(self, lines: Vec<Line>) -> AppResult<Transaction> {
        Ok(Transaction {
            id: self.id,
            project_id: self.project_id,
            occurred_on: self.occurred_on,
            account_code: self.account_code,
            tx_kind: TxKind::parse(&self.tx_kind).ok_or_else(|| bad_value("tx_kind", &self.tx_kind))?,
            description: Description {
                text: self.description,
                lang: Lang::parse(&self.description_lang).ok_or_else(|| bad_value("description_lang", &self.description_lang))?,
                ru: self.description_ru,
                en: self.description_en,
                th: self.description_th,
            },
            created_by: self.created_by,
            plan: self.plan_id.zip(self.plan_date),
            lines,
        })
    }
}

fn bad_value(column: &str, value: &str) -> AppError {
    AppError::Internal(anyhow::anyhow!("fb_transactions.{column}: неизвестное значение '{value}'"))
}

async fn write_lines(tx: &mut SqlTx<'_, Postgres>, item: &Transaction) -> AppResult<()> {
    sqlx::query("DELETE FROM fb_transaction_lines WHERE transaction_id = $1").bind(item.id).execute(&mut **tx).await?;
    for (i, l) in item.lines.iter().enumerate() {
        sqlx::query("INSERT INTO fb_transaction_lines (transaction_id, position, category_code, amount, note) VALUES ($1, $2, $3, $4, $5)")
            .bind(item.id)
            .bind(i as i32 + 1)
            .bind(&l.category_code)
            .bind(l.amount)
            .bind(&l.note)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

fn bind_filter<'q, O>(
    q: sqlx::query::QueryAs<'q, Postgres, O, sqlx::postgres::PgArguments>,
    project_id: Uuid,
    f: &'q Filter,
) -> sqlx::query::QueryAs<'q, Postgres, O, sqlx::postgres::PgArguments> {
    q.bind(project_id)
        .bind(f.from)
        .bind(f.to)
        .bind(f.account_code.as_deref())
        .bind(f.created_by)
        .bind(f.untranslated.map(|l| l.upper()))
}

#[async_trait]
impl TransactionRepository for PostgresTransactionRepository {
    async fn insert(&self, item: &Transaction) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO fb_transactions (id, project_id, occurred_on, account_code, tx_kind, description, description_lang, \
             description_ru, description_en, description_th, created_by, plan_id, plan_date) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(item.id)
        .bind(item.project_id)
        .bind(item.occurred_on)
        .bind(&item.account_code)
        .bind(item.tx_kind.as_str())
        .bind(&item.description.text)
        .bind(item.description.lang.upper())
        .bind(&item.description.ru)
        .bind(&item.description.en)
        .bind(&item.description.th)
        .bind(item.created_by)
        .bind(item.plan.map(|p| p.0))
        .bind(item.plan.map(|p| p.1))
        .execute(&mut *tx)
        .await?;
        write_lines(&mut tx, item).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn update(&self, item: &Transaction) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        let res = sqlx::query(
            "UPDATE fb_transactions SET occurred_on = $3, account_code = $4, tx_kind = $5, description = $6, description_lang = $7, \
             description_ru = $8, description_en = $9, description_th = $10, plan_id = $11, plan_date = $12 \
             WHERE project_id = $1 AND id = $2",
        )
        .bind(item.project_id)
        .bind(item.id)
        .bind(item.occurred_on)
        .bind(&item.account_code)
        .bind(item.tx_kind.as_str())
        .bind(&item.description.text)
        .bind(item.description.lang.upper())
        .bind(&item.description.ru)
        .bind(&item.description.en)
        .bind(&item.description.th)
        .bind(item.plan.map(|p| p.0))
        .bind(item.plan.map(|p| p.1))
        .execute(&mut *tx)
        .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound(item.id.to_string()));
        }
        write_lines(&mut tx, item).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Transaction>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_transactions WHERE project_id = $1 AND id = $2");
        let row: Option<Row> = sqlx::query_as(&sql).bind(project_id).bind(id).fetch_optional(&self.pool).await?;
        match row {
            None => Ok(None),
            Some(r) => Ok(self.with_lines(vec![r]).await?.pop()),
        }
    }

    async fn list(&self, project_id: Uuid, filter: &Filter, limit: i64, offset: i64) -> AppResult<Vec<Transaction>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM fb_transactions WHERE {FILTER} ORDER BY occurred_on DESC, created_at DESC, id LIMIT $7 OFFSET $8"
        );
        let rows: Vec<Row> = bind_filter(sqlx::query_as(&sql), project_id, filter).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        self.with_lines(rows).await
    }

    async fn count(&self, project_id: Uuid, filter: &Filter) -> AppResult<i64> {
        let sql = format!("SELECT COUNT(*) FROM fb_transactions WHERE {FILTER}");
        let (n,): (i64,) = bind_filter(sqlx::query_as(&sql), project_id, filter).fetch_one(&self.pool).await?;
        Ok(n)
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool> {
        let res = sqlx::query("DELETE FROM fb_transactions WHERE project_id = $1 AND id = $2").bind(project_id).bind(id).execute(&self.pool).await?;
        Ok(res.rows_affected() > 0)
    }

    async fn lines(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<FactLine>> {
        let rows: Vec<(Uuid, NaiveDate, String, String, String, Decimal, Uuid)> = sqlx::query_as(
            "SELECT t.id, t.occurred_on, t.account_code, t.tx_kind, l.category_code, l.amount, t.created_by \
             FROM fb_transactions t JOIN fb_transaction_lines l ON l.transaction_id = t.id \
             WHERE t.project_id = $1 AND t.occurred_on BETWEEN $2 AND $3 ORDER BY t.occurred_on, t.id, l.position",
        )
        .bind(project_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(transaction_id, occurred_on, account_code, tx_kind, category_code, amount, created_by)| FactLine {
                transaction_id,
                occurred_on,
                account_code,
                tx_kind,
                category_code,
                amount,
                created_by,
            })
            .collect())
    }

    async fn payments(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<Payment>> {
        let rows: Vec<(Uuid, NaiveDate, Uuid, NaiveDate, Decimal)> = sqlx::query_as(
            "SELECT t.plan_id, t.plan_date, t.id, t.occurred_on, COALESCE(SUM(l.amount), 0) \
             FROM fb_transactions t LEFT JOIN fb_transaction_lines l ON l.transaction_id = t.id \
             WHERE t.project_id = $1 AND t.plan_id IS NOT NULL AND t.plan_date BETWEEN $2 AND $3 \
             GROUP BY t.id",
        )
        .bind(project_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(plan_id, plan_date, transaction_id, occurred_on, amount)| Payment { plan_id, plan_date, transaction_id, occurred_on, amount })
            .collect())
    }

    async fn turnover(&self, project_id: Uuid, to: NaiveDate) -> AppResult<Vec<(String, Decimal, Decimal)>> {
        Ok(sqlx::query_as(
            "SELECT t.account_code, \
                    COALESCE(SUM(l.amount) FILTER (WHERE t.tx_kind = 'INCOME'), 0), \
                    COALESCE(SUM(l.amount) FILTER (WHERE t.tx_kind = 'EXPENSE'), 0) \
             FROM fb_transactions t JOIN fb_transaction_lines l ON l.transaction_id = t.id \
             WHERE t.project_id = $1 AND t.occurred_on <= $2 GROUP BY t.account_code",
        )
        .bind(project_id)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }
}
