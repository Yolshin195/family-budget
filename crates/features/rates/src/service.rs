//! Use cases фичи «Курсы валют». Знает только домен и порты — ничего об HTTP и SQL.

use std::sync::Arc;

#[allow(unused_imports)]
use kernel::code::ensure_ref;
use kernel::error::{AppError, AppResult};
use kernel::pagination::{PagedResponse, Pagination};
use uuid::Uuid;

use crate::domain::Rate;
use crate::dto::{PeriodQuery, RateDto, RateInput};
use crate::ports::{Deps, Period, RateRepository};

pub struct RateService {
    repo: Arc<dyn RateRepository>,
    #[allow(dead_code)]
    deps: Deps,
    /// Удалённые записи для «Отменить» в тосте.
    undo: kernel::undo::UndoBuffer<Rate>,
}

impl RateService {
    pub fn new(repo: Arc<dyn RateRepository>, deps: Deps) -> Self {
        Self { repo, deps, undo: Default::default() }
    }

    pub async fn create(&self, project_id: Uuid, input: RateInput) -> AppResult<RateDto> {
        let item = Rate::create(project_id, input.into_domain().map_err(AppError::Validation)?)
            .map_err(AppError::Validation)?;
        self.check_refs(project_id, &item).await?;
        self.repo.insert(&item).await?;
        Ok(item.into())
    }

    pub async fn get(&self, project_id: Uuid, id: Uuid) -> AppResult<RateDto> {
        Ok(self.load(project_id, id).await?.into())
    }

    pub async fn list(
        &self,
        project_id: Uuid,
        query: PeriodQuery,
        pagination: Pagination,
    ) -> AppResult<PagedResponse<RateDto>> {
        let period = Period { from: query.from, to: query.to };
        let items = self.repo.list(project_id, period, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(project_id, period).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    pub async fn update(&self, project_id: Uuid, id: Uuid, input: RateInput) -> AppResult<RateDto> {
        let mut item = self.load(project_id, id).await?;
        item.change(input.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        self.check_refs(project_id, &item).await?;
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    pub async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<()> {
        if self.repo.delete(project_id, id).await? { Ok(()) } else { Err(not_found(id)) }
    }

    /// Удаление из интерфейса: запись запоминается на пару минут — токен для «Отменить».
    pub async fn delete_with_undo(&self, project_id: Uuid, id: Uuid) -> AppResult<Uuid> {
        let item = self.load(project_id, id).await?;
        self.delete(project_id, id).await?;
        Ok(self.undo.put(project_id, item))
    }

    /// «Отменить»: вернуть удалённую запись с тем же id.
    pub async fn undo_delete(&self, project_id: Uuid, token: Uuid) -> AppResult<()> {
        let item = self.undo.take(project_id, token).ok_or_else(|| AppError::NotFound("отменить уже нельзя".into()))?;
        self.repo.insert(&item).await
    }

    /// Удалить все записи проекта (удаление проекта или аккаунта).
    pub async fn purge_project(&self, project_id: Uuid) -> AppResult<u64> {
        self.repo.purge(project_id).await
    }

    /// Все записи проекта (для страниц и других фич), новые сначала.
    pub async fn all(&self, project_id: Uuid) -> AppResult<Vec<RateDto>> {
        let items = self.repo.list(project_id, Period::default(), 10_000, 0).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Последний (по дате) курс каждой валюты — для подсказки и направления в форме.
    pub async fn latest_by_currency(&self, project_id: Uuid) -> AppResult<Vec<RateDto>> {
        let mut latest: Vec<RateDto> = Vec::new();
        for r in self.all(project_id).await? {
            if !latest.iter().any(|l| l.currency_code == r.currency_code) {
                latest.push(r);
            }
        }
        Ok(latest)
    }

    /// Направление ввода по умолчанию: как у последнего курса валюты; курсов нет — прямое
    /// («1 валюта = x базовой»). Других правил нет: направление выбирает пользователь.
    pub async fn default_inverted(&self, project_id: Uuid, currency_code: &str) -> AppResult<bool> {
        let code = currency_code.trim().to_uppercase();
        Ok(self
            .latest_by_currency(project_id)
            .await?
            .into_iter()
            .find(|r| r.currency_code == code)
            .is_some_and(|r| r.inverted))
    }

    /// Варианты для поля-ссылки в форме.
    pub async fn ref_choices(&self, project_id: Uuid, lang: kernel::i18n::Lang) -> AppResult<Vec<kernel::web::Choice>> {
        self.deps.currencies.choices(project_id, lang).await
    }

    async fn load(&self, project_id: Uuid, id: Uuid) -> AppResult<Rate> {
        self.repo.find(project_id, id).await?.ok_or_else(|| not_found(id))
    }

    #[allow(unused_variables)]
    async fn check_refs(&self, project_id: Uuid, item: &Rate) -> AppResult<()> {
        ensure_ref(
            self.deps.currencies.exists(project_id, &item.currency_code).await?,
            "currency_code",
            &item.currency_code,
        )?;
        Ok(())
    }
}

fn not_found(id: Uuid) -> AppError {
    AppError::NotFound(format!("Курсы валют: запись {id} не найдена"))
}
