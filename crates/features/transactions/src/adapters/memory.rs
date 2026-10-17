//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::error::{AppError, AppResult};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::adapters::postgres::fact;
use crate::domain::{RefKind, Transaction, TxKind, check_balanced, system};
use crate::ports::{ExchangeLine, FactLine, Filter, Payment, QuickStats, TransactionRepository};

#[derive(Default)]
pub struct InMemoryTransactionRepository {
    rows: Mutex<Vec<Transaction>>,
}

fn matches(r: &Transaction, project_id: Uuid, f: &Filter) -> bool {
    r.project_id == project_id
        && f.from.is_none_or(|from| r.occurred_on >= from)
        && f.to.is_none_or(|to| r.occurred_on <= to)
        && f.account_code.as_deref().is_none_or(|a| r.accounts().any(|x| x == a))
        && f.kind.is_none_or(|k| r.tx_kind == k)
        && f.created_by.is_none_or(|u| r.created_by == u)
        && f.untranslated.is_none_or(|l| r.description.localized().is_some_and(|d| d.missing(l)))
}

/// То же, что отложенный триггер БД: несбалансированная транзакция не сохраняется.
fn balanced(item: &Transaction) -> AppResult<()> {
    check_balanced(&item.postings).map_err(|m| AppError::Internal(anyhow::anyhow!(m)))
}

#[async_trait]
impl TransactionRepository for InMemoryTransactionRepository {
    async fn insert(&self, item: &Transaction) -> AppResult<()> {
        balanced(item)?;
        let mut rows = self.rows.lock().unwrap();
        if item.plan.is_some() && rows.iter().any(|r| r.project_id == item.project_id && r.plan == item.plan) {
            return Err(AppError::Conflict("plan".into()));
        }
        rows.push(item.clone());
        Ok(())
    }

    async fn update(&self, item: &Transaction) -> AppResult<()> {
        balanced(item)?;
        let mut rows = self.rows.lock().unwrap();
        if item.plan.is_some() && rows.iter().any(|r| r.project_id == item.project_id && r.plan == item.plan && r.id != item.id) {
            return Err(AppError::Conflict("plan".into()));
        }
        let row = rows.iter_mut().find(|r| r.project_id == item.project_id && r.id == item.id);
        *row.ok_or_else(|| AppError::NotFound(item.id.to_string()))? = item.clone();
        Ok(())
    }

    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Transaction>> {
        Ok(self.rows.lock().unwrap().iter().find(|r| r.project_id == project_id && r.id == id).cloned())
    }

    async fn list(&self, project_id: Uuid, filter: &Filter, limit: i64, offset: i64) -> AppResult<Vec<Transaction>> {
        let rows = self.rows.lock().unwrap();
        // Новые сначала: при одной дате — позже добавленные.
        let mut items: Vec<_> = rows.iter().enumerate().filter(|(_, r)| matches(r, project_id, filter)).collect();
        items.sort_by(|a, b| b.1.occurred_on.cmp(&a.1.occurred_on).then(b.0.cmp(&a.0)));
        Ok(items.into_iter().skip(offset as usize).take(limit as usize).map(|(_, r)| r.clone()).collect())
    }

    async fn count(&self, project_id: Uuid, filter: &Filter) -> AppResult<i64> {
        Ok(self.rows.lock().unwrap().iter().filter(|r| matches(r, project_id, filter)).count() as i64)
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<bool> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| !(r.project_id == project_id && r.id == id));
        Ok(rows.len() < before)
    }

    async fn lines(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<FactLine>> {
        let rows = self.rows.lock().unwrap();
        Ok(rows
            .iter()
            .filter(|r| r.project_id == project_id && r.occurred_on >= from && r.occurred_on <= to)
            .flat_map(|r| {
                r.postings
                    .iter()
                    .filter(|p| p.ref_kind == RefKind::Category)
                    .map(move |p| fact(r.id, r.occurred_on, p.currency.clone(), r.tx_kind.as_str(), p.ref_code.clone(), p.amount, r.created_by))
            })
            .collect())
    }

    async fn exchange(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<ExchangeLine>> {
        let rows = self.rows.lock().unwrap();
        Ok(rows
            .iter()
            .filter(|r| r.project_id == project_id && r.occurred_on >= from && r.occurred_on <= to)
            .flat_map(|r| {
                r.postings
                    .iter()
                    .filter(|p| p.ref_kind == RefKind::System && p.ref_code == system::EXCHANGE)
                    .map(move |p| ExchangeLine { occurred_on: r.occurred_on, currency: p.currency.clone(), amount: p.amount, created_by: r.created_by })
            })
            .collect())
    }

    async fn payments(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<Payment>> {
        let rows = self.rows.lock().unwrap();
        Ok(rows
            .iter()
            .filter(|r| r.project_id == project_id)
            .filter_map(|r| {
                let (plan_id, plan_date) = r.plan?;
                (plan_date >= from && plan_date <= to).then(|| Payment {
                    plan_id,
                    plan_date,
                    transaction_id: r.id,
                    occurred_on: r.occurred_on,
                    amount: r.total(),
                    currency: r.currency().to_string(),
                })
            })
            .collect())
    }

    async fn balances(&self, project_id: Uuid, to: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        let rows = self.rows.lock().unwrap();
        let mut out: Vec<(String, Decimal)> = Vec::new();
        for p in rows
            .iter()
            .filter(|r| r.project_id == project_id && r.occurred_on <= to)
            .flat_map(|r| r.postings.iter().filter(|p| p.ref_kind == RefKind::Account))
        {
            match out.iter_mut().find(|x| x.0 == p.ref_code) {
                Some(x) => x.1 += p.amount,
                None => out.push((p.ref_code.clone(), p.amount)),
            }
        }
        Ok(out)
    }

    async fn has_postings(&self, project_id: Uuid, account_code: &str) -> AppResult<bool> {
        Ok(self.rows.lock().unwrap().iter().any(|r| r.project_id == project_id && r.accounts().any(|a| a == account_code)))
    }

    async fn find_opening(&self, project_id: Uuid, account_code: &str) -> AppResult<Option<Transaction>> {
        let rows = self.rows.lock().unwrap();
        Ok(rows.iter().find(|r| r.project_id == project_id && r.tx_kind == TxKind::Opening && r.accounts().any(|a| a == account_code)).cloned())
    }

    async fn quick_stats(&self, project_id: Uuid, user_id: Uuid, kind: &str, since: NaiveDate) -> AppResult<QuickStats> {
        let rows = self.rows.lock().unwrap();
        let mine: Vec<&Transaction> = rows.iter().filter(|r| r.project_id == project_id && r.created_by == user_id).collect();
        let mut counts: Vec<(String, usize)> = Vec::new();
        for t in mine.iter().filter(|t| t.tx_kind.as_str() == kind && t.occurred_on >= since) {
            for p in t.postings.iter().filter(|p| p.ref_kind == RefKind::Category) {
                match counts.iter_mut().find(|c| c.0 == p.ref_code) {
                    Some(c) => c.1 += 1,
                    None => counts.push((p.ref_code.clone(), 1)),
                }
            }
        }
        counts.sort_by_key(|c| std::cmp::Reverse(c.1));
        let last_account = mine
            .iter()
            .rev()
            .find(|t| matches!(t.tx_kind, TxKind::Expense | TxKind::Income))
            .and_then(|t| t.accounts().next().map(str::to_string));
        Ok(QuickStats { last_account, frequent: counts.into_iter().map(|c| c.0).collect() })
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let mut rows = self.rows.lock().unwrap();
        let n = rows.len();
        rows.retain(|r| r.project_id != project_id);
        Ok((n - rows.len()) as u64)
    }

    async fn count_first(&self, _from: chrono::DateTime<chrono::Utc>, _to: chrono::DateTime<chrono::Utc>) -> AppResult<i64> {
        let rows = self.rows.lock().unwrap();
        let mut users: Vec<Uuid> = rows.iter().filter(|r| r.tx_kind != TxKind::Opening).map(|r| r.created_by).collect();
        users.sort();
        users.dedup();
        Ok(users.len() as i64)
    }
}
