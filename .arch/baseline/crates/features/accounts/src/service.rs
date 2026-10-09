//! Use cases фичи «Счета». Знает только домен и порты — ничего об HTTP и SQL.
//! Записи не удаляются физически, а архивируются: на них могут ссылаться другие фичи.

use std::sync::Arc;

#[allow(unused_imports)]
use kernel::code::{ensure_ref, normalize};
use kernel::error::{AppError, AppResult};
use kernel::pagination::{PagedResponse, Pagination};
use uuid::Uuid;

use crate::domain::Account;
use crate::dto::{CreateAccountDto, ListQuery, UpdateAccountDto, AccountDto};
use crate::ports::{Deps, AccountRepository};

pub struct AccountService {
    repo: Arc<dyn AccountRepository>,
    #[allow(dead_code)]
    deps: Deps,
}

impl AccountService {
    pub fn new(repo: Arc<dyn AccountRepository>, deps: Deps) -> Self {
        Self { repo, deps }
    }

    pub async fn create(&self, user_id: Uuid, dto: CreateAccountDto) -> AppResult<AccountDto> {
        let new = dto.into_domain().map_err(AppError::Validation)?;
        let item = Account::create(user_id, new).map_err(AppError::Validation)?;
        self.check_refs(user_id, &item).await?;
        if self.repo.find_by_code(user_id, &item.code).await?.is_some() {
            return Err(AppError::Conflict(format!("код '{}' уже занят", item.code)));
        }
        self.repo.insert(&item).await?;
        Ok(item.into())
    }

    pub async fn get(&self, user_id: Uuid, code: &str) -> AppResult<AccountDto> {
        Ok(self.load(user_id, code).await?.into())
    }

    pub async fn list(&self, user_id: Uuid, query: ListQuery, pagination: Pagination) -> AppResult<PagedResponse<AccountDto>> {
        let all = query.include_archived;
        let items = self.repo.list(user_id, all, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(user_id, all).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    pub async fn update(&self, user_id: Uuid, code: &str, dto: UpdateAccountDto) -> AppResult<AccountDto> {
        let mut item = self.load(user_id, code).await?;
        item.change(dto.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        self.check_refs(user_id, &item).await?;
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    /// Архивирование вместо удаления: код остаётся занятым, ссылки на запись не «повисают».
    pub async fn archive(&self, user_id: Uuid, code: &str) -> AppResult<()> {
        let mut item = self.load(user_id, code).await?;
        item.archived = true;
        self.repo.update(&item).await
    }

    /// Публичный контракт для других фич: существует ли активная запись с таким кодом.
    pub async fn exists(&self, user_id: Uuid, code: &str) -> AppResult<bool> {
        let Ok(code) = normalize("code", code) else { return Ok(false) };
        Ok(self.repo.find_by_code(user_id, &code).await?.is_some_and(|x| !x.archived))
    }

    async fn load(&self, user_id: Uuid, code: &str) -> AppResult<Account> {
        let code = normalize("code", code).map_err(AppError::Validation)?;
        self.repo
            .find_by_code(user_id, &code)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Счета: запись '{code}' не найдена")))
    }

    #[allow(unused_variables)]
    async fn check_refs(&self, user_id: Uuid, item: &Account) -> AppResult<()> {
        ensure_ref(self.deps.currencies.exists(user_id, &item.currency_code).await?, "currency_code", &item.currency_code)?;
        Ok(())
    }
}
