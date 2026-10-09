//! Use cases фичи «Счета». Знает только домен и порты — ничего об HTTP и SQL.
//! Записи не удаляются физически, а архивируются: на них могут ссылаться другие фичи.

use std::sync::Arc;

#[allow(unused_imports)]
use kernel::code::{ensure_ref, normalize};
use kernel::error::{AppError, AppResult};
use kernel::pagination::{PagedResponse, Pagination};
use kernel::i18n::Lang;
use kernel::web::Choice;
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

    pub async fn create(&self, project_id: Uuid, dto: CreateAccountDto) -> AppResult<AccountDto> {
        let new = dto.into_domain().map_err(AppError::Validation)?;
        let item = Account::create(project_id, new).map_err(AppError::Validation)?;
        self.check_refs(project_id, &item).await?;
        if self.repo.find_by_code(project_id, &item.code).await?.is_some() {
            return Err(AppError::Conflict(format!("код '{}' уже занят", item.code)));
        }
        self.repo.insert(&item).await?;
        Ok(item.into())
    }

    pub async fn get(&self, project_id: Uuid, code: &str) -> AppResult<AccountDto> {
        Ok(self.load(project_id, code).await?.into())
    }

    pub async fn list(&self, project_id: Uuid, query: ListQuery, pagination: Pagination) -> AppResult<PagedResponse<AccountDto>> {
        let all = query.include_archived;
        let items = self.repo.list(project_id, all, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(project_id, all).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    pub async fn update(&self, project_id: Uuid, code: &str, dto: UpdateAccountDto) -> AppResult<AccountDto> {
        let mut item = self.load(project_id, code).await?;
        item.change(dto.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        self.check_refs(project_id, &item).await?;
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    /// Архивирование вместо удаления: код остаётся занятым, ссылки на запись не «повисают».
    pub async fn archive(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        item.archived = true;
        self.repo.update(&item).await
    }

    /// Публичный контракт для других фич: существует ли активная запись с таким кодом.
    pub async fn exists(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        let Ok(code) = normalize("code", code) else { return Ok(false) };
        Ok(self.repo.find_by_code(project_id, &code).await?.is_some_and(|x| !x.archived))
    }

    /// Все записи проекта (для страниц).
    pub async fn all(&self, project_id: Uuid, include_archived: bool) -> AppResult<Vec<AccountDto>> {
        let items = self.repo.list(project_id, include_archived, 1000, 0).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Публичный контракт для других фич: активные счета для выпадающих списков (tag — валюта счёта).
    pub async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        Ok(self
            .all(project_id, false)
            .await?
            .into_iter()
            .map(|a| Choice { label: a.title(lang).to_string(), tag: a.currency_code.clone(), code: a.code, depth: 0 })
            .collect())
    }

    /// Валюта счёта (для других фич: пересчёт в базовую валюту).
    pub async fn currency_of(&self, project_id: Uuid, code: &str) -> AppResult<Option<String>> {
        let Ok(code) = normalize("code", code) else { return Ok(None) };
        Ok(self.repo.find_by_code(project_id, &code).await?.map(|a| a.currency_code))
    }

    /// Валюты для формы счёта.
    pub async fn currency_choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.deps.currencies.choices(project_id, lang).await
    }

    async fn load(&self, project_id: Uuid, code: &str) -> AppResult<Account> {
        let code = normalize("code", code).map_err(AppError::Validation)?;
        self.repo
            .find_by_code(project_id, &code)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Счета: запись '{code}' не найдена")))
    }

    #[allow(unused_variables)]
    async fn check_refs(&self, project_id: Uuid, item: &Account) -> AppResult<()> {
        ensure_ref(self.deps.currencies.exists(project_id, &item.currency_code).await?, "currency_code", &item.currency_code)?;
        Ok(())
    }
}
