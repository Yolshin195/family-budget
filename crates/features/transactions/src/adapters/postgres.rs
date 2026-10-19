//! Исходящий адаптер: порт на PostgreSQL (таблицы `fb_transactions`, `fb_postings`).
//! Сумму проводок по каждой валюте дополнительно проверяет отложенный триггер БД.

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction as SqlTx};
use uuid::Uuid;

use crate::domain::{Description, Posting, RefKind, Transaction, TxKind, system};
use crate::ports::{ExchangeLine, FactLine, Filter, Payment, QuickStats, TransactionRepository, taken};

const COLUMNS: &str = "id, project_id, occurred_on, tx_kind, description, description_lang, \
                       description_ru, description_en, description_th, created_by, plan_id, plan_date";
/// Транзакция с проводкой на счёт $N.
macro_rules! touches {
    ($n:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM fb_postings p WHERE p.transaction_id = fb_transactions.id ",
            "AND p.ref_kind = 'ACCOUNT' AND p.ref_code = $",
            $n,
            ")"
        )
    };
}
/// $1 проект, $2..$3 период, $4 счёт, $5 тип, $6 автор, $7 язык «без перевода» (NULL — условие выключено).
const FILTER: &str = concat!(
    "project_id = $1 \
    AND ($2::date IS NULL OR occurred_on >= $2) AND ($3::date IS NULL OR occurred_on <= $3) \
    AND ($4::text IS NULL OR ",
    touches!("4"),
    ") AND ($5::text IS NULL OR tx_kind = $5) AND ($6::uuid IS NULL OR created_by = $6) \
    AND ($7::text IS NULL OR (description IS NOT NULL AND description_lang <> $7 AND \
         (CASE $7 WHEN 'RU' THEN description_ru WHEN 'EN' THEN description_en ELSE description_th END) IS NULL))"
);

pub struct PostgresTransactionRepository {
    pool: PgPool,
}

impl PostgresTransactionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn with_postings(&self, rows: Vec<Row>) -> AppResult<Vec<Transaction>> {
        let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
        let postings: Vec<PostingRow> = sqlx::query_as(
            "SELECT transaction_id, ref_kind, ref_code, currency_code, amount, note FROM fb_postings \
             WHERE transaction_id = ANY($1) ORDER BY transaction_id, position",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let mine = postings.iter().filter(|p| p.transaction_id == r.id).map(PostingRow::to_domain).collect::<AppResult<Vec<_>>>()?;
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
struct PostingRow {
    transaction_id: Uuid,
    ref_kind: String,
    ref_code: String,
    currency_code: String,
    amount: Decimal,
    note: Option<String>,
}

impl PostingRow {
    fn to_domain(&self) -> AppResult<Posting> {
        Ok(Posting {
            ref_kind: RefKind::parse(&self.ref_kind).ok_or_else(|| bad_value("fb_postings.ref_kind", &self.ref_kind))?,
            ref_code: self.ref_code.clone(),
            currency: self.currency_code.clone(),
            amount: self.amount,
            note: self.note.clone(),
        })
    }
}

impl Row {
    fn into_domain(self, postings: Vec<Posting>) -> AppResult<Transaction> {
        Ok(Transaction {
            id: self.id,
            project_id: self.project_id,
            occurred_on: self.occurred_on,
            tx_kind: TxKind::parse(&self.tx_kind).ok_or_else(|| bad_value("fb_transactions.tx_kind", &self.tx_kind))?,
            description: Description {
                text: self.description,
                lang: Lang::parse(&self.description_lang).ok_or_else(|| bad_value("fb_transactions.description_lang", &self.description_lang))?,
                ru: self.description_ru,
                en: self.description_en,
                th: self.description_th,
            },
            created_by: self.created_by,
            plan: self.plan_id.zip(self.plan_date),
            postings,
        })
    }
}

/// Нарушение уникального индекса шапки → какой ключ занят (план или начальный остаток).
fn unique(e: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &e {
        match db.constraint() {
            Some("fb_transactions_plan_uniq") => return AppError::Conflict(taken::PLAN.into()),
            Some("fb_transactions_opening_uniq") => return AppError::Conflict(taken::OPENING.into()),
            _ => {}
        }
    }
    e.into()
}

fn bad_value(column: &str, value: &str) -> AppError {
    AppError::Internal(anyhow::anyhow!("{column}: неизвестное значение '{value}'"))
}

async fn write_postings(tx: &mut SqlTx<'_, Postgres>, item: &Transaction) -> AppResult<()> {
    sqlx::query("DELETE FROM fb_postings WHERE transaction_id = $1").bind(item.id).execute(&mut **tx).await?;
    for (i, p) in item.postings.iter().enumerate() {
        sqlx::query(
            "INSERT INTO fb_postings (transaction_id, position, ref_kind, ref_code, currency_code, amount, note) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(item.id)
        .bind(i as i32 + 1)
        .bind(p.ref_kind.as_str())
        .bind(&p.ref_code)
        .bind(&p.currency)
        .bind(p.amount)
        .bind(&p.note)
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
        .bind(f.kind.map(|k| k.as_str()))
        .bind(f.created_by)
        .bind(f.untranslated.map(|l| l.upper()))
}

#[async_trait]
impl TransactionRepository for PostgresTransactionRepository {
    async fn insert(&self, item: &Transaction) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO fb_transactions (id, project_id, occurred_on, tx_kind, description, description_lang, \
             description_ru, description_en, description_th, created_by, plan_id, plan_date, opening_account) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(item.id)
        .bind(item.project_id)
        .bind(item.occurred_on)
        .bind(item.tx_kind.as_str())
        .bind(&item.description.text)
        .bind(item.description.lang.upper())
        .bind(&item.description.ru)
        .bind(&item.description.en)
        .bind(&item.description.th)
        .bind(item.created_by)
        .bind(item.plan.map(|p| p.0))
        .bind(item.plan.map(|p| p.1))
        .bind(item.opening_account())
        .execute(&mut *tx)
        .await
        .map_err(unique)?;
        write_postings(&mut tx, item).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn update(&self, item: &Transaction) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        let res = sqlx::query(
            "UPDATE fb_transactions SET occurred_on = $3, tx_kind = $4, description = $5, description_lang = $6, \
             description_ru = $7, description_en = $8, description_th = $9, plan_id = $10, plan_date = $11, \
             opening_account = $12 WHERE project_id = $1 AND id = $2",
        )
        .bind(item.project_id)
        .bind(item.id)
        .bind(item.occurred_on)
        .bind(item.tx_kind.as_str())
        .bind(&item.description.text)
        .bind(item.description.lang.upper())
        .bind(&item.description.ru)
        .bind(&item.description.en)
        .bind(&item.description.th)
        .bind(item.plan.map(|p| p.0))
        .bind(item.plan.map(|p| p.1))
        .bind(item.opening_account())
        .execute(&mut *tx)
        .await
        .map_err(unique)?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound(item.id.to_string()));
        }
        write_postings(&mut tx, item).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Transaction>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_transactions WHERE project_id = $1 AND id = $2");
        let row: Option<Row> = sqlx::query_as(&sql).bind(project_id).bind(id).fetch_optional(&self.pool).await?;
        match row {
            None => Ok(None),
            Some(r) => Ok(self.with_postings(vec![r]).await?.pop()),
        }
    }

    async fn list(&self, project_id: Uuid, filter: &Filter, limit: i64, offset: i64) -> AppResult<Vec<Transaction>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM fb_transactions WHERE {FILTER} ORDER BY occurred_on DESC, created_at DESC, id LIMIT $8 OFFSET $9"
        );
        let rows: Vec<Row> = bind_filter(sqlx::query_as(&sql), project_id, filter).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        self.with_postings(rows).await
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
            "SELECT t.id, t.occurred_on, p.currency_code, t.tx_kind, p.ref_code, p.amount, t.created_by \
             FROM fb_transactions t JOIN fb_postings p ON p.transaction_id = t.id \
             WHERE t.project_id = $1 AND t.occurred_on BETWEEN $2 AND $3 AND p.ref_kind = 'CATEGORY' \
             ORDER BY t.occurred_on, t.id, p.position",
        )
        .bind(project_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(transaction_id, occurred_on, currency, tx_kind, category_code, amount, created_by)| fact(transaction_id, occurred_on, currency, &tx_kind, category_code, amount, created_by)).collect())
    }

    async fn exchange(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<ExchangeLine>> {
        let rows: Vec<(Uuid, NaiveDate, String, Decimal, Uuid)> = sqlx::query_as(
            "SELECT t.id, t.occurred_on, p.currency_code, p.amount, t.created_by \
             FROM fb_transactions t JOIN fb_postings p ON p.transaction_id = t.id \
             WHERE t.project_id = $1 AND t.occurred_on BETWEEN $2 AND $3 AND p.ref_kind = 'SYSTEM' AND p.ref_code = $4 \
             ORDER BY t.occurred_on, t.id, p.position",
        )
        .bind(project_id)
        .bind(from)
        .bind(to)
        .bind(system::EXCHANGE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(transaction_id, occurred_on, currency, amount, created_by)| ExchangeLine { transaction_id, occurred_on, currency, amount, created_by })
            .collect())
    }

    async fn payments(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<Payment>> {
        let sql = format!("SELECT {COLUMNS} FROM fb_transactions WHERE project_id = $1 AND plan_id IS NOT NULL AND plan_date BETWEEN $2 AND $3");
        let rows: Vec<Row> = sqlx::query_as(&sql).bind(project_id).bind(from).bind(to).fetch_all(&self.pool).await?;
        Ok(self
            .with_postings(rows)
            .await?
            .into_iter()
            .filter_map(|t| {
                let (plan_id, plan_date) = t.plan?;
                Some(Payment { plan_id, plan_date, transaction_id: t.id, occurred_on: t.occurred_on, amount: t.total(), currency: t.currency().to_string() })
            })
            .collect())
    }

    async fn balances(&self, project_id: Uuid, to: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        Ok(sqlx::query_as(
            "SELECT p.ref_code, SUM(p.amount) FROM fb_postings p JOIN fb_transactions t ON t.id = p.transaction_id \
             WHERE t.project_id = $1 AND t.occurred_on <= $2 AND p.ref_kind = 'ACCOUNT' GROUP BY p.ref_code",
        )
        .bind(project_id)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn has_postings(&self, project_id: Uuid, account_code: &str) -> AppResult<bool> {
        let (yes,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM fb_postings p JOIN fb_transactions t ON t.id = p.transaction_id \
             WHERE t.project_id = $1 AND p.ref_kind = 'ACCOUNT' AND p.ref_code = $2)",
        )
        .bind(project_id)
        .bind(account_code)
        .fetch_one(&self.pool)
        .await?;
        Ok(yes)
    }

    async fn uses_category(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        let (yes,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM fb_postings p JOIN fb_transactions t ON t.id = p.transaction_id \
             WHERE t.project_id = $1 AND p.ref_kind = 'CATEGORY' AND p.ref_code = $2)",
        )
        .bind(project_id)
        .bind(code)
        .fetch_one(&self.pool)
        .await?;
        Ok(yes)
    }

    async fn uses_currency(&self, project_id: Uuid, currency: &str) -> AppResult<bool> {
        let (yes,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM fb_postings p JOIN fb_transactions t ON t.id = p.transaction_id \
             WHERE t.project_id = $1 AND p.currency_code = $2)",
        )
        .bind(project_id)
        .bind(currency)
        .fetch_one(&self.pool)
        .await?;
        Ok(yes)
    }

    async fn find_opening(&self, project_id: Uuid, account_code: &str) -> AppResult<Option<Transaction>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM fb_transactions WHERE project_id = $1 AND tx_kind = 'OPENING' AND {} ORDER BY occurred_on LIMIT 1",
            touches!("2")
        );
        let row: Option<Row> = sqlx::query_as(&sql).bind(project_id).bind(account_code).fetch_optional(&self.pool).await?;
        match row {
            None => Ok(None),
            Some(r) => Ok(self.with_postings(vec![r]).await?.pop()),
        }
    }

    async fn quick_stats(&self, project_id: Uuid, user_id: Uuid, kind: &str, since: NaiveDate) -> AppResult<QuickStats> {
        // Переводы и начальные остатки не сбивают «последний счёт» быстрого расхода.
        let last: Option<(String,)> = sqlx::query_as(
            "SELECT p.ref_code FROM fb_transactions t JOIN fb_postings p ON p.transaction_id = t.id \
             WHERE t.project_id = $1 AND t.created_by = $2 AND t.tx_kind IN ('EXPENSE', 'INCOME') AND p.ref_kind = 'ACCOUNT' \
             ORDER BY t.created_at DESC LIMIT 1",
        )
        .bind(project_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        let frequent: Vec<(String,)> = sqlx::query_as(
            "SELECT p.ref_code FROM fb_postings p JOIN fb_transactions t ON t.id = p.transaction_id \
             WHERE t.project_id = $1 AND t.created_by = $2 AND t.tx_kind = $3 AND t.occurred_on >= $4 AND p.ref_kind = 'CATEGORY' \
             GROUP BY p.ref_code ORDER BY count(*) DESC, max(t.occurred_on) DESC LIMIT 12",
        )
        .bind(project_id)
        .bind(user_id)
        .bind(kind)
        .bind(since)
        .fetch_all(&self.pool)
        .await?;
        Ok(QuickStats { last_account: last.map(|r| r.0), frequent: frequent.into_iter().map(|r| r.0).collect() })
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        // Проводки удаляются каскадом (FK внутри фичи).
        let r = sqlx::query("DELETE FROM fb_transactions WHERE project_id = $1").bind(project_id).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }

    async fn count_first(&self, from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>) -> AppResult<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM (SELECT created_by, min(created_at) AS first FROM fb_transactions \
             WHERE tx_kind <> 'OPENING' GROUP BY created_by) x WHERE x.first >= $1 AND x.first < $2",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }
}

/// Проводка на категорию → факт бюджета: доходы с плюсом, возвраты с минусом.
pub(crate) fn fact(transaction_id: Uuid, occurred_on: NaiveDate, currency: String, tx_kind: &str, category_code: String, amount: Decimal, created_by: Uuid) -> FactLine {
    let income = tx_kind == TxKind::Income.as_str();
    FactLine {
        transaction_id,
        occurred_on,
        currency,
        kind: if income { "INCOME" } else { "EXPENSE" }.into(),
        category_code,
        amount: if income { -amount } else { amount },
        created_by,
    }
}
