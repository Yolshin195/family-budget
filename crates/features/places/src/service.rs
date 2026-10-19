//! Use cases фичи «Места». Знает только домен и порты — ничего об HTTP и SQL.
//! Места не удаляются, а архивируются: на них ссылаются операции.

use std::sync::Arc;

use kernel::error::{AppError, AppResult, Problem};
use kernel::i18n::{Lang, tr};
use kernel::pagination::{PagedResponse, Pagination};
use kernel::web::Choice;
use uuid::Uuid;

use crate::domain::Place;
use crate::dto::{CreatePlaceDto, ListQuery, PlaceDto, PlaceFields};
use crate::ports::{Deps, PlaceRepository};

pub struct PlaceService {
    repo: Arc<dyn PlaceRepository>,
    deps: Deps,
}

fn t(key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    tr(Some(crate::i18n::dict), key, args)
}

impl PlaceService {
    pub fn new(repo: Arc<dyn PlaceRepository>, deps: Deps) -> Self {
        Self { repo, deps }
    }

    pub async fn create(&self, project_id: Uuid, dto: CreatePlaceDto) -> AppResult<PlaceDto> {
        let item = Place::create(project_id, &dto.code, dto.data.into_domain()?)?;
        self.check_refs(project_id, &item).await?;
        if self.repo.find_by_code(project_id, &item.code).await?.is_some() {
            return Err(AppError::Conflict(Problem::validation("code", t("err.code_taken", &[("code", &item.code)])).to_string()));
        }
        self.repo.insert(&item).await?;
        Ok(item.into())
    }

    pub async fn get(&self, project_id: Uuid, code: &str) -> AppResult<PlaceDto> {
        Ok(self.load(project_id, code).await?.into())
    }

    pub async fn list(&self, project_id: Uuid, query: ListQuery, pagination: Pagination) -> AppResult<PagedResponse<PlaceDto>> {
        let all = query.include_archived;
        let items = self.repo.list(project_id, all, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(project_id, all).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    /// Все места проекта по названию — для выбора в операциях, поиска и MCP (их немного).
    pub async fn all(&self, project_id: Uuid, include_archived: bool) -> AppResult<Vec<PlaceDto>> {
        Ok(self.repo.list(project_id, include_archived, 10_000, 0).await?.into_iter().map(Into::into).collect())
    }

    pub async fn update(&self, project_id: Uuid, code: &str, dto: PlaceFields) -> AppResult<PlaceDto> {
        let mut item = self.load(project_id, code).await?;
        item.change(dto.into_domain()?)?;
        self.check_refs(project_id, &item).await?;
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    /// Архив вместо удаления: место пропадает из выбора, операции его сохраняют.
    pub async fn archive(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        self.set_archived(project_id, code, true).await
    }

    /// Вернуть из архива («Отменить» в тосте).
    pub async fn restore(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        self.set_archived(project_id, code, false).await
    }

    async fn set_archived(&self, project_id: Uuid, code: &str, archived: bool) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        item.archived = archived;
        self.repo.update(&item).await
    }

    /// Удалить все записи проекта (удаление проекта или аккаунта).
    pub async fn purge_project(&self, project_id: Uuid) -> AppResult<u64> {
        self.repo.purge(project_id).await
    }

    /// Публичный контракт для других фич: активное место с таким кодом.
    pub async fn active(&self, project_id: Uuid, code: &str) -> AppResult<Option<PlaceDto>> {
        let Ok(code) = kernel::code::normalize("code", code) else { return Ok(None) };
        Ok(self.repo.find_by_code(project_id, &code).await?.filter(|p| !p.archived).map(Into::into))
    }

    /// Категории и счета для формы места (активные).
    pub async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<(Vec<Choice>, Vec<Choice>)> {
        Ok((self.deps.directories.categories(project_id, lang).await?, self.deps.directories.accounts(project_id, lang).await?))
    }

    async fn load(&self, project_id: Uuid, code: &str) -> AppResult<Place> {
        let code = kernel::code::code("code", code)?;
        self.repo.find_by_code(project_id, &code).await?.ok_or_else(|| AppError::NotFound(t("err.not_found", &[("code", &code)])))
    }

    /// Категория и счёт по умолчанию — активные записи проекта. Старое значение, которое с тех пор
    /// архивировали, остаётся: правка названия не должна требовать сначала сменить категорию.
    async fn check_refs(&self, project_id: Uuid, item: &Place) -> AppResult<()> {
        let old = self.repo.find_by_code(project_id, &item.code).await?;
        let kept = |new: &Option<String>, old: Option<&Option<String>>| old.is_some_and(|o| o == new);
        if let Some(c) = &item.default_category_code
            && !kept(&item.default_category_code, old.as_ref().map(|o| &o.default_category_code))
            && !self.deps.directories.categories(project_id, Lang::default()).await?.iter().any(|x| x.code == *c)
        {
            return Err(Problem::validation("default_category_code", tr(None, "err.ref_not_found", &[("code", c)])).into());
        }
        if let Some(a) = &item.default_account_code
            && !kept(&item.default_account_code, old.as_ref().map(|o| &o.default_account_code))
            && !self.deps.directories.accounts(project_id, Lang::default()).await?.iter().any(|x| x.code == *a)
        {
            return Err(Problem::validation("default_account_code", tr(None, "err.ref_not_found", &[("code", a)])).into());
        }
        Ok(())
    }
}
