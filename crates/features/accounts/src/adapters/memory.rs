//! In-memory адаптер для тестов (только cfg(test)).

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use kernel::error::{AppError, AppResult};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::{Account, BankBalance};
use crate::ports::AccountRepository;

#[derive(Default)]
pub struct InMemoryAccountRepository {
    rows: Mutex<Vec<Account>>,
}

#[async_trait]
impl AccountRepository for InMemoryAccountRepository {
    async fn insert(&self, item: &Account) -> AppResult<()> {
        let mut rows = self.rows.lock().unwrap();
        if rows.iter().any(|r| r.project_id == item.project_id && r.code == item.code) {
            return Err(AppError::Conflict("duplicate code".into()));
        }
        rows.push(item.clone());
        Ok(())
    }

    async fn update(&self, item: &Account) -> AppResult<()> {
        let mut rows = self.rows.lock().unwrap();
        let row = rows.iter_mut().find(|r| r.project_id == item.project_id && r.code == item.code);
        let row = row.ok_or_else(|| AppError::NotFound(item.code.clone()))?;
        // Как в БД: правка счёта не меняет остаток по данным банка.
        *row = Account { bank: row.bank, ..item.clone() };
        Ok(())
    }

    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Account>> {
        let rows = self.rows.lock().unwrap();
        Ok(rows.iter().find(|r| r.project_id == project_id && r.code == code).cloned())
    }

    async fn list(&self, project_id: Uuid, include_archived: bool, limit: i64, offset: i64) -> AppResult<Vec<Account>> {
        let rows = self.rows.lock().unwrap();
        let mut items: Vec<_> =
            rows.iter().filter(|r| r.project_id == project_id && (include_archived || !r.archived)).cloned().collect();
        items.sort_by(|a, b| a.code.cmp(&b.code));
        Ok(items.into_iter().skip(offset as usize).take(limit as usize).collect())
    }

    async fn count(&self, project_id: Uuid, include_archived: bool) -> AppResult<i64> {
        let rows = self.rows.lock().unwrap();
        Ok(rows.iter().filter(|r| r.project_id == project_id && (include_archived || !r.archived)).count() as i64)
    }

    async fn set_bank_balance(
        &self,
        project_id: Uuid,
        code: &str,
        amount: Decimal,
        at: DateTime<Utc>,
    ) -> AppResult<bool> {
        let mut rows = self.rows.lock().unwrap();
        let row = rows.iter_mut().find(|r| r.project_id == project_id && r.code == code);
        match row {
            Some(r) if r.bank.is_none_or(|b| b.at <= at) => {
                r.bank = Some(BankBalance { amount, at });
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
        let mut rows = self.rows.lock().unwrap();
        let n = rows.len();
        rows.retain(|r| r.project_id != project_id);
        Ok((n - rows.len()) as u64)
    }
}
