//! Use cases фичи «Валюты». Знает только домен и порты — ничего об HTTP и SQL.
//! Записи не удаляются физически, а архивируются: на них могут ссылаться другие фичи.

use std::sync::Arc;

#[allow(unused_imports)]
use kernel::code::{ensure_ref, normalize};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::pagination::{PagedResponse, Pagination};
use kernel::web::Choice;
use uuid::Uuid;

use crate::domain::Currency;
use crate::dto::{CreateCurrencyDto, CurrencyDto, ListQuery, UpdateCurrencyDto};
use crate::ports::{CurrencyRepository, Deps};

pub struct CurrencyService {
    repo: Arc<dyn CurrencyRepository>,
    deps: Deps,
}

impl CurrencyService {
    pub fn new(repo: Arc<dyn CurrencyRepository>, deps: Deps) -> Self {
        Self { repo, deps }
    }

    pub async fn create(&self, project_id: Uuid, dto: CreateCurrencyDto) -> AppResult<CurrencyDto> {
        let new = dto.into_domain().map_err(AppError::Validation)?;
        let item = Currency::create(project_id, new).map_err(AppError::Validation)?;
        self.check_refs(project_id, &item).await?;
        if self.repo.find_by_code(project_id, &item.code).await?.is_some() {
            return Err(AppError::Conflict(format!("код '{}' уже занят", item.code)));
        }
        self.repo.insert(&item).await?;
        Ok(item.into())
    }

    pub async fn get(&self, project_id: Uuid, code: &str) -> AppResult<CurrencyDto> {
        Ok(self.load(project_id, code).await?.into())
    }

    pub async fn list(
        &self,
        project_id: Uuid,
        query: ListQuery,
        pagination: Pagination,
    ) -> AppResult<PagedResponse<CurrencyDto>> {
        let all = query.include_archived;
        let items = self.repo.list(project_id, all, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(project_id, all).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    pub async fn update(&self, project_id: Uuid, code: &str, dto: UpdateCurrencyDto) -> AppResult<CurrencyDto> {
        let mut item = self.load(project_id, code).await?;
        let (minor, archived) = (item.minor_units, item.archived);
        item.change(dto.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        // Старые суммы записаны с прежней точностью: меньше знаков — и их уже не исправить.
        if item.minor_units < minor && self.deps.usage.has_postings(project_id, &item.code).await? {
            return Err(AppError::Validation(format!(
                "minor_units: в {} уже есть суммы с {minor} знаками после запятой — меньше сделать нельзя",
                item.code
            )));
        }
        if item.archived && !archived {
            self.check_archive(project_id, &item.code).await?;
        }
        self.check_refs(project_id, &item).await?;
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    /// Архивирование вместо удаления: код остаётся занятым, ссылки на запись не «повисают».
    pub async fn archive(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        self.check_archive(project_id, &item.code).await?;
        item.archived = true;
        self.repo.update(&item).await
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
    pub async fn all(&self, project_id: Uuid, include_archived: bool) -> AppResult<Vec<CurrencyDto>> {
        let items = self.repo.list(project_id, include_archived, 1000, 0).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Публичный контракт для других фич: активные валюты для выпадающих списков.
    pub async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        Ok(self
            .all(project_id, false)
            .await?
            .into_iter()
            .map(|c| Choice {
                label: format!("{} · {}", c.code, c.title(lang)),
                tag: c.symbol.clone(),
                code: c.code,
                depth: 0,
            })
            .collect())
    }

    /// В архив — только валюту, которая не базовая и у которой нет активных счетов: иначе счёт
    /// останется в валюте, которой «нет», и его нельзя будет изменить (этап 0, Ц8).
    async fn check_archive(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        if self.deps.usage.base_currency(project_id).await?.as_deref() == Some(code) {
            return Err(AppError::Conflict(format!(
                "archived: {code} — базовая валюта проекта, её нельзя архивировать"
            )));
        }
        let accounts = self.deps.usage.active_accounts(project_id, code).await?;
        if !accounts.is_empty() {
            return Err(AppError::Conflict(format!(
                "archived: в {code} есть активные счета ({}) — сначала архивируйте их",
                accounts.join(", ")
            )));
        }
        Ok(())
    }

    async fn load(&self, project_id: Uuid, code: &str) -> AppResult<Currency> {
        let code = normalize("code", code).map_err(AppError::Validation)?;
        self.repo
            .find_by_code(project_id, &code)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Валюты: запись '{code}' не найдена")))
    }

    #[allow(unused_variables)]
    async fn check_refs(&self, project_id: Uuid, item: &Currency) -> AppResult<()> {
        Ok(())
    }
}
