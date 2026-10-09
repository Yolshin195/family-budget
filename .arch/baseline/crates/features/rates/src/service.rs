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
}

impl RateService {
    pub fn new(repo: Arc<dyn RateRepository>, deps: Deps) -> Self {
        Self { repo, deps }
    }

    pub async fn create(&self, user_id: Uuid, input: RateInput) -> AppResult<RateDto> {
        let item = Rate::create(user_id, input.into_domain().map_err(AppError::Validation)?)
            .map_err(AppError::Validation)?;
        self.check_refs(user_id, &item).await?;
        self.repo.insert(&item).await?;
        Ok(item.into())
    }

    pub async fn get(&self, user_id: Uuid, id: Uuid) -> AppResult<RateDto> {
        Ok(self.load(user_id, id).await?.into())
    }

    pub async fn list(&self, user_id: Uuid, query: PeriodQuery, pagination: Pagination) -> AppResult<PagedResponse<RateDto>> {
        let period = Period { from: query.from, to: query.to };
        let items = self.repo.list(user_id, period, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(user_id, period).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    pub async fn update(&self, user_id: Uuid, id: Uuid, input: RateInput) -> AppResult<RateDto> {
        let mut item = self.load(user_id, id).await?;
        item.change(input.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        self.check_refs(user_id, &item).await?;
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    pub async fn delete(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        if self.repo.delete(user_id, id).await? { Ok(()) } else { Err(not_found(id)) }
    }

    async fn load(&self, user_id: Uuid, id: Uuid) -> AppResult<Rate> {
        self.repo.find(user_id, id).await?.ok_or_else(|| not_found(id))
    }

    #[allow(unused_variables)]
    async fn check_refs(&self, user_id: Uuid, item: &Rate) -> AppResult<()> {
        ensure_ref(self.deps.currencies.exists(user_id, &item.currency_code).await?, "currency_code", &item.currency_code)?;
        Ok(())
    }
}

fn not_found(id: Uuid) -> AppError {
    AppError::NotFound(format!("Курсы валют: запись {id} не найдена"))
}
