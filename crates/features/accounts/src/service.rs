//! Use cases фичи «Счета». Знает только домен и порты — ничего об HTTP и SQL.
//! Записи не удаляются физически, а архивируются: на них могут ссылаться другие фичи.

use std::sync::Arc;

use chrono::NaiveDate;
#[allow(unused_imports)]
use kernel::code::{ensure_ref, normalize};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::pagination::{PagedResponse, Pagination};
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::Account;
use crate::dto::{AccountDto, CreateAccountDto, ListQuery, UpdateAccountDto};
use crate::ports::{AccountRepository, Deps};

pub struct AccountService {
    repo: Arc<dyn AccountRepository>,
    deps: Deps,
}

/// Дата «после всех операций» — для остатка с учётом операций будущими датами.
fn end_of_time() -> NaiveDate {
    NaiveDate::from_ymd_opt(9999, 12, 31).unwrap_or(NaiveDate::MAX)
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

    pub async fn list(
        &self,
        project_id: Uuid,
        query: ListQuery,
        pagination: Pagination,
    ) -> AppResult<PagedResponse<AccountDto>> {
        let all = query.include_archived;
        let items = self.repo.list(project_id, all, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(project_id, all).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    pub async fn update(&self, project_id: Uuid, code: &str, dto: UpdateAccountDto) -> AppResult<AccountDto> {
        let mut item = self.load(project_id, code).await?;
        let currency = item.currency_code.clone();
        item.change(dto.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        // Проводки хранят валюту: смена валюты переписала бы историю счёта.
        if item.currency_code != currency && self.deps.ledger.has_postings(project_id, &item.code).await? {
            return Err(AppError::Validation(format!(
                "currency_code: по счёту есть операции — валюта остаётся {currency}"
            )));
        }
        if item.archived {
            self.ensure_zero(project_id, &item.code).await?;
        }
        self.check_refs(project_id, &item).await?;
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    /// Архивирование вместо удаления: код остаётся занятым, ссылки на запись не «повисают».
    /// Архивировать можно только счёт с нулевым остатком.
    pub async fn archive(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        self.ensure_zero(project_id, &item.code).await?;
        item.archived = true;
        self.repo.update(&item).await
    }

    async fn ensure_zero(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        let balance = self
            .balances(project_id, end_of_time())
            .await?
            .into_iter()
            .find(|b| b.0 == code)
            .map(|b| b.1)
            .unwrap_or_default();
        if balance.is_zero() {
            Ok(())
        } else {
            Err(AppError::Conflict(format!(
                "archived: на счёте {code} остаток {} — переведите его или сверьте остаток",
                balance.normalize()
            )))
        }
    }

    // ---- Остатки (из журнала операций) ----

    /// Остатки счетов на дату: (счёт, остаток).
    pub async fn balances(&self, project_id: Uuid, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        self.deps.ledger.balances(project_id, date).await
    }

    /// Начальный остаток счёта и его дата.
    pub async fn opening(&self, project_id: Uuid, code: &str) -> AppResult<Option<(Decimal, NaiveDate)>> {
        let item = self.load(project_id, code).await?;
        self.deps.ledger.opening(project_id, &item.code).await
    }

    /// Задать начальный остаток из формы счёта.
    pub async fn set_opening(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        code: &str,
        amount: Decimal,
        date: NaiveDate,
    ) -> AppResult<()> {
        let item = self.load(project_id, code).await?;
        self.deps.ledger.set_opening(project_id, user_id, &item.code, amount, date).await
    }

    /// Вернуть из архива («Отменить» в тосте после архивирования).
    pub async fn restore(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        item.archived = false;
        self.repo.update(&item).await
    }

    /// Удалить все записи проекта (удаление проекта или аккаунта).
    pub async fn purge_project(&self, project_id: Uuid) -> AppResult<u64> {
        self.repo.purge(project_id).await
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
        ensure_ref(
            self.deps.currencies.exists(project_id, &item.currency_code).await?,
            "currency_code",
            &item.currency_code,
        )?;
        Ok(())
    }
}
