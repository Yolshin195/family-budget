//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use kernel::error::{AppError, AppResult};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::adapters::postgres::fact;
use crate::domain::{ExternalRef, Posting, RefKind, Transaction, TxKind, check_balanced, system};
use crate::ports::{ExchangeLine, FactLine, Filter, Payment, QuickStats, TransactionRepository, taken};

#[derive(Default)]
pub struct InMemoryTransactionRepository {
    rows: Mutex<Vec<Transaction>>,
    /// Связи с внешними записями: как в БД, без внешнего ключа на операции. Блокируется после `rows`.
    refs: Mutex<Vec<ExternalRef>>,
}

impl InMemoryTransactionRepository {
    fn refs(&self) -> Vec<ExternalRef> {
        self.refs.lock().unwrap().clone()
    }
}

/// Уникальный ключ связи `(project_id, provider, external_id)`.
fn check_ref(refs: &[ExternalRef], r: &ExternalRef) -> AppResult<()> {
    if refs.iter().any(|x| x.project_id == r.project_id && x.provider == r.provider && x.external_id == r.external_id) {
        return Err(AppError::Conflict(taken::EXTERNAL_REF.into()));
    }
    Ok(())
}

fn matches(r: &Transaction, project_id: Uuid, f: &Filter, refs: &[ExternalRef]) -> bool {
    r.project_id == project_id
        && f.from.is_none_or(|from| r.occurred_on >= from)
        && f.to.is_none_or(|to| r.occurred_on <= to)
        && f.account_code.as_deref().is_none_or(|a| r.accounts().any(|x| x == a))
        && f.kind.is_none_or(|k| r.tx_kind == k)
        && f.created_by.is_none_or(|u| r.created_by == u)
        && f.untranslated.is_none_or(|l| r.description.localized().is_some_and(|d| d.missing(l)))
        && f.place_code.as_deref().is_none_or(|p| r.place_code.as_deref() == Some(p))
        && f.for_person.as_ref().is_none_or(|who| r.items.iter().any(|x| x.for_person == *who))
        && f.external_provider.as_deref().is_none_or(|p| {
            refs.iter().any(|x| x.project_id == r.project_id && x.transaction_id == r.id && x.provider == p)
        })
}

/// Проводка относится к позиции получателя из фильтра (без фильтра — любая).
fn of_recipient(filter: &Filter, r: &Transaction, p: &Posting) -> bool {
    filter.for_person.as_ref().is_none_or(|who| r.items.iter().any(|x| Some(x.id) == p.item_id && x.for_person == *who))
}

/// То же, что уникальные индексы шапки в БД: оплата вхождения плана и начальный остаток счёта — по одному.
fn check_unique(rows: &[Transaction], item: &Transaction) -> AppResult<()> {
    let others = rows.iter().filter(|r| r.project_id == item.project_id && r.id != item.id);
    for r in others {
        if item.plan.is_some() && r.plan == item.plan {
            return Err(AppError::Conflict(taken::PLAN.into()));
        }
        if item.opening_account().is_some() && r.opening_account() == item.opening_account() {
            return Err(AppError::Conflict(taken::OPENING.into()));
        }
    }
    Ok(())
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
        check_unique(&rows, item)?;
        rows.push(item.clone());
        Ok(())
    }

    async fn insert_with_ref(&self, item: &Transaction, r: &ExternalRef) -> AppResult<()> {
        balanced(item)?;
        let mut rows = self.rows.lock().unwrap();
        let mut refs = self.refs.lock().unwrap();
        check_unique(&rows, item)?;
        check_ref(&refs, r)?;
        rows.push(item.clone());
        refs.push(r.clone());
        Ok(())
    }

    async fn insert_ref(&self, r: &ExternalRef) -> AppResult<()> {
        let mut refs = self.refs.lock().unwrap();
        check_ref(&refs, r)?;
        refs.push(r.clone());
        Ok(())
    }

    async fn find_ref(&self, project_id: Uuid, provider: &str, external_id: &str) -> AppResult<Option<ExternalRef>> {
        Ok(self
            .refs()
            .into_iter()
            .find(|x| x.project_id == project_id && x.provider == provider && x.external_id == external_id))
    }

    async fn refs_of(&self, project_id: Uuid, ids: &[Uuid]) -> AppResult<Vec<ExternalRef>> {
        let mut out: Vec<ExternalRef> =
            self.refs().into_iter().filter(|x| x.project_id == project_id && ids.contains(&x.transaction_id)).collect();
        out.sort_by_key(|x| x.created_at);
        Ok(out)
    }

    async fn update(&self, item: &Transaction) -> AppResult<()> {
        balanced(item)?;
        let mut rows = self.rows.lock().unwrap();
        check_unique(&rows, item)?;
        let row = rows.iter_mut().find(|r| r.project_id == item.project_id && r.id == item.id);
        *row.ok_or_else(|| AppError::NotFound(item.id.to_string()))? = item.clone();
        Ok(())
    }

    async fn find(&self, project_id: Uuid, id: Uuid) -> AppResult<Option<Transaction>> {
        Ok(self.rows.lock().unwrap().iter().find(|r| r.project_id == project_id && r.id == id).cloned())
    }

    async fn list(&self, project_id: Uuid, filter: &Filter, limit: i64, offset: i64) -> AppResult<Vec<Transaction>> {
        let refs = self.refs();
        let rows = self.rows.lock().unwrap();
        // Как в БД: новые сначала, внутри дня — по времени операции, без него — по времени внесения; при равных —
        // позже добавленные.
        let mut items: Vec<_> =
            rows.iter().enumerate().filter(|(_, r)| matches(r, project_id, filter, &refs)).collect();
        let key = |r: &Transaction| (r.occurred_on, r.occurred_at.unwrap_or(r.created_at), r.created_at);
        items.sort_by(|a, b| key(b.1).cmp(&key(a.1)).then(b.0.cmp(&a.0)));
        Ok(items.into_iter().skip(offset as usize).take(limit as usize).map(|(_, r)| r.clone()).collect())
    }

    async fn set_time(&self, project_id: Uuid, id: Uuid, at: Option<DateTime<Utc>>) -> AppResult<()> {
        if let Some(r) = self.rows.lock().unwrap().iter_mut().find(|r| r.project_id == project_id && r.id == id) {
            r.occurred_at = at;
        }
        Ok(())
    }

    async fn count(&self, project_id: Uuid, filter: &Filter) -> AppResult<i64> {
        let refs = self.refs();
        Ok(self.rows.lock().unwrap().iter().filter(|r| matches(r, project_id, filter, &refs)).count() as i64)
    }

    async fn category_totals(&self, project_id: Uuid, filter: &Filter) -> AppResult<Vec<(String, Decimal, Decimal)>> {
        let refs = self.refs();
        let rows = self.rows.lock().unwrap();
        let mut sums: Vec<(String, Decimal, Decimal)> = Vec::new();
        for r in rows.iter().filter(|r| matches(r, project_id, filter, &refs)) {
            // С фильтром «Для кого» — только позиции этого получателя.
            let mine = |p: &&Posting| of_recipient(filter, r, p);
            for p in r.postings.iter().filter(|p| p.ref_kind == RefKind::Category).filter(mine) {
                let i = match sums.iter().position(|x| x.0 == p.currency) {
                    Some(i) => i,
                    None => {
                        sums.push((p.currency.clone(), Decimal::ZERO, Decimal::ZERO));
                        sums.len() - 1
                    }
                };
                if r.tx_kind == TxKind::Income { sums[i].1 -= p.amount } else { sums[i].2 += p.amount }
            }
        }
        sums.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(sums)
    }

    async fn account_flows(&self, project_id: Uuid, filter: &Filter, account: &str) -> AppResult<(Decimal, Decimal)> {
        let refs = self.refs();
        let rows = self.rows.lock().unwrap();
        let amounts = rows.iter().filter(|r| matches(r, project_id, filter, &refs)).map(|r| r.amount_on(account));
        Ok(amounts
            .fold((Decimal::ZERO, Decimal::ZERO), |(i, o), v| if v > Decimal::ZERO { (i + v, o) } else { (i, o + v) }))
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
                r.postings.iter().filter(|p| p.ref_kind == RefKind::Category).map(move |p| {
                    fact(
                        r.id,
                        r.occurred_on,
                        p.currency.clone(),
                        r.tx_kind.as_str(),
                        p.ref_code.clone(),
                        p.amount,
                        r.created_by,
                    )
                })
            })
            .collect())
    }

    async fn exchange(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<ExchangeLine>> {
        let rows = self.rows.lock().unwrap();
        Ok(rows
            .iter()
            .filter(|r| r.project_id == project_id && r.occurred_on >= from && r.occurred_on <= to)
            .flat_map(|r| {
                r.postings.iter().filter(|p| p.ref_kind == RefKind::System && p.ref_code == system::EXCHANGE).map(
                    move |p| ExchangeLine {
                        transaction_id: r.id,
                        occurred_on: r.occurred_on,
                        currency: p.currency.clone(),
                        amount: p.amount,
                        created_by: r.created_by,
                    },
                )
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

    async fn balance_at(
        &self,
        project_id: Uuid,
        account: &str,
        date: NaiveDate,
        at: DateTime<Utc>,
    ) -> AppResult<Decimal> {
        let rows = self.rows.lock().unwrap();
        let before =
            |r: &Transaction| r.occurred_on < date || (r.occurred_on == date && r.occurred_at.is_none_or(|t| t <= at));
        Ok(rows.iter().filter(|r| r.project_id == project_id && before(r)).map(|r| r.amount_on(account)).sum())
    }

    async fn has_postings(&self, project_id: Uuid, account_code: &str) -> AppResult<bool> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.project_id == project_id && r.accounts().any(|a| a == account_code)))
    }

    async fn uses_category(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        let rows = self.rows.lock().unwrap();
        Ok(rows
            .iter()
            .filter(|r| r.project_id == project_id)
            .flat_map(|r| &r.postings)
            .any(|p| p.ref_kind == RefKind::Category && p.ref_code == code))
    }

    async fn uses_currency(&self, project_id: Uuid, currency: &str) -> AppResult<bool> {
        let rows = self.rows.lock().unwrap();
        Ok(rows.iter().filter(|r| r.project_id == project_id).flat_map(|r| &r.postings).any(|p| p.currency == currency))
    }

    async fn find_opening(&self, project_id: Uuid, account_code: &str) -> AppResult<Option<Transaction>> {
        let rows = self.rows.lock().unwrap();
        Ok(rows
            .iter()
            .find(|r| {
                r.project_id == project_id && r.tx_kind == TxKind::Opening && r.accounts().any(|a| a == account_code)
            })
            .cloned())
    }

    async fn quick_stats(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        kind: &str,
        since: NaiveDate,
    ) -> AppResult<QuickStats> {
        let rows = self.rows.lock().unwrap();
        let mine: Vec<&Transaction> =
            rows.iter().filter(|r| r.project_id == project_id && r.created_by == user_id).collect();
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
        let mut places: Vec<(String, usize)> = Vec::new();
        for code in mine
            .iter()
            .filter(|t| t.tx_kind.as_str() == kind && t.occurred_on >= since)
            .filter_map(|t| t.place_code.as_ref())
        {
            match places.iter_mut().find(|c| &c.0 == code) {
                Some(c) => c.1 += 1,
                None => places.push((code.clone(), 1)),
            }
        }
        places.sort_by_key(|c| std::cmp::Reverse(c.1));
        let last_account = mine
            .iter()
            .rev()
            .find(|t| matches!(t.tx_kind, TxKind::Expense | TxKind::Income))
            .and_then(|t| t.accounts().next().map(str::to_string));
        Ok(QuickStats {
            last_account,
            frequent: counts.into_iter().map(|c| c.0).collect(),
            places: places.into_iter().map(|c| c.0).collect(),
        })
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let mut rows = self.rows.lock().unwrap();
        self.refs.lock().unwrap().retain(|x| x.project_id != project_id);
        let n = rows.len();
        rows.retain(|r| r.project_id != project_id);
        Ok((n - rows.len()) as u64)
    }

    async fn count_first(
        &self,
        _from: chrono::DateTime<chrono::Utc>,
        _to: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<i64> {
        let rows = self.rows.lock().unwrap();
        let mut users: Vec<Uuid> = rows.iter().filter(|r| r.tx_kind != TxKind::Opening).map(|r| r.created_by).collect();
        users.sort();
        users.dedup();
        Ok(users.len() as i64)
    }
}
