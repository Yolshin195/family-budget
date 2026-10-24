//! Use cases фичи «Места». Знает только домен и порты — ничего об HTTP и SQL.
//! Места не удаляются, а архивируются: на них ссылаются операции.

use std::sync::Arc;

use kernel::error::{AppError, AppResult, Problem};
use kernel::i18n::{Lang, tr};
use kernel::pagination::{PagedResponse, Pagination};
use kernel::web::Choice;
use uuid::Uuid;

use crate::domain::{Place, check_tree};
use crate::dto::{CreatePlaceDto, ListQuery, PlaceDto, PlaceFields, views};
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
            return Err(AppError::Conflict(
                Problem::validation("code", t("err.code_taken", &[("code", &item.code)])).to_string(),
            ));
        }
        check_tree(&self.repo.all(project_id).await?, &item)?;
        self.repo.insert(&item).await?;
        self.get(project_id, &item.code).await
    }

    pub async fn get(&self, project_id: Uuid, code: &str) -> AppResult<PlaceDto> {
        let code = kernel::code::code("code", code)?;
        self.all(project_id, true)
            .await?
            .into_iter()
            .find(|p| p.code == code)
            .ok_or_else(|| AppError::NotFound(t("err.not_found", &[("code", &code)])))
    }

    /// Страница списка деревом: родитель, затем его места.
    pub async fn list(
        &self,
        project_id: Uuid,
        query: ListQuery,
        pagination: Pagination,
    ) -> AppResult<PagedResponse<PlaceDto>> {
        let all = self.all(project_id, query.include_archived).await?;
        let total = all.len() as i64;
        let page = all.into_iter().skip(pagination.offset() as usize).take(pagination.limit() as usize).collect();
        Ok(PagedResponse::new(page, &pagination, total))
    }

    /// Все места проекта деревом — для выбора в операциях, поиска и MCP (их немного).
    pub async fn all(&self, project_id: Uuid, include_archived: bool) -> AppResult<Vec<PlaceDto>> {
        Ok(views(&self.repo.all(project_id).await?).into_iter().filter(|p| include_archived || !p.archived).collect())
    }

    pub async fn update(&self, project_id: Uuid, code: &str, dto: PlaceFields) -> AppResult<PlaceDto> {
        let mut item = self.load(project_id, code).await?;
        item.change(dto.into_domain()?)?;
        self.check_refs(project_id, &item).await?;
        check_tree(&self.repo.all(project_id).await?, &item)?;
        self.repo.update(&item).await?;
        self.get(project_id, &item.code).await
    }

    /// Архив вместо удаления: место пропадает из выбора, операции его сохраняют.
    /// Сеть с активными магазинами не архивируется — иначе они остались бы без родителя.
    pub async fn archive(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        let all = self.repo.all(project_id).await?;
        if all.iter().any(|p| !p.archived && p.parent_code.as_deref() == Some(item.code.as_str())) {
            return Err(Problem::validation("", t("err.archive_children", &[("name", &item.name)])).into());
        }
        item.archived = true;
        self.repo.update(&item).await
    }

    /// Вернуть из архива («Отменить» в тосте). Родитель должен быть активным — иначе сначала его.
    pub async fn restore(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        item.archived = false;
        let all = self.repo.all(project_id).await?;
        if let Some(parent) = all.iter().find(|p| p.archived && item.parent_code.as_deref() == Some(p.code.as_str())) {
            return Err(Problem::validation(
                "",
                t("err.restore_parent", &[("name", &item.name), ("parent", &parent.name)]),
            )
            .into());
        }
        check_tree(&all, &item)?;
        self.repo.update(&item).await
    }

    /// Удалить все записи проекта (удаление проекта или аккаунта).
    pub async fn purge_project(&self, project_id: Uuid) -> AppResult<u64> {
        self.repo.purge(project_id).await
    }

    /// Публичный контракт для других фич: активное место с таким кодом.
    pub async fn active(&self, project_id: Uuid, code: &str) -> AppResult<Option<PlaceDto>> {
        let Ok(code) = kernel::code::normalize("code", code) else { return Ok(None) };
        Ok(self.all(project_id, false).await?.into_iter().find(|p| p.code == code))
    }

    /// Категории и счета для формы места (активные).
    pub async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<(Vec<Choice>, Vec<Choice>)> {
        Ok((
            self.deps.directories.categories(project_id, lang).await?,
            self.deps.directories.accounts(project_id, lang).await?,
        ))
    }

    async fn load(&self, project_id: Uuid, code: &str) -> AppResult<Place> {
        let code = kernel::code::code("code", code)?;
        self.repo
            .find_by_code(project_id, &code)
            .await?
            .ok_or_else(|| AppError::NotFound(t("err.not_found", &[("code", &code)])))
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
            return Err(
                Problem::validation("default_category_code", tr(None, "err.ref_not_found", &[("code", c)])).into()
            );
        }
        if let Some(a) = &item.default_account_code
            && !kept(&item.default_account_code, old.as_ref().map(|o| &o.default_account_code))
            && !self.deps.directories.accounts(project_id, Lang::default()).await?.iter().any(|x| x.code == *a)
        {
            return Err(
                Problem::validation("default_account_code", tr(None, "err.ref_not_found", &[("code", a)])).into()
            );
        }
        Ok(())
    }
}
