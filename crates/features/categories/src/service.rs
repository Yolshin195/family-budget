//! Use cases фичи «Категории». Знает только домен и порты — ничего об HTTP и SQL.
//! Записи не удаляются физически, а архивируются: на них могут ссылаться другие фичи.

use std::sync::Arc;

#[allow(unused_imports)]
use kernel::code::{ensure_ref, normalize};
use kernel::error::{AppError, AppResult, Problem};
use kernel::i18n::Lang;
use kernel::i18n::tr;
use kernel::pagination::{PagedResponse, Pagination};
use kernel::web::Choice;
use uuid::Uuid;

use crate::domain::{Category, check_tree, flatten};
use crate::dto::{CategoryDto, CreateCategoryDto, ListQuery, UpdateCategoryDto};
use crate::ports::{CategoryRepository, Deps};

pub struct CategoryService {
    repo: Arc<dyn CategoryRepository>,
    deps: Deps,
}

impl CategoryService {
    pub fn new(repo: Arc<dyn CategoryRepository>, deps: Deps) -> Self {
        Self { repo, deps }
    }

    pub async fn create(&self, project_id: Uuid, dto: CreateCategoryDto) -> AppResult<CategoryDto> {
        let new = dto.into_domain().map_err(AppError::Validation)?;
        let item = Category::create(project_id, new).map_err(AppError::Validation)?;
        self.check_refs(project_id, &item).await?;
        if self.repo.find_by_code(project_id, &item.code).await?.is_some() {
            return Err(AppError::Conflict(format!("код '{}' уже занят", item.code)));
        }
        self.repo.insert(&item).await?;
        Ok(item.into())
    }

    pub async fn get(&self, project_id: Uuid, code: &str) -> AppResult<CategoryDto> {
        Ok(self.load(project_id, code).await?.into())
    }

    pub async fn list(
        &self,
        project_id: Uuid,
        query: ListQuery,
        pagination: Pagination,
    ) -> AppResult<PagedResponse<CategoryDto>> {
        let all = query.include_archived;
        let items = self.repo.list(project_id, all, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(project_id, all).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    pub async fn update(&self, project_id: Uuid, code: &str, dto: UpdateCategoryDto) -> AppResult<CategoryDto> {
        let mut item = self.load(project_id, code).await?;
        let kind = item.category_kind;
        item.change(dto.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        // Подкатегории отдельно проверять не нужно: у потомков тип такой же (check_tree), их меняют первыми.
        if item.category_kind != kind && self.deps.usage.in_use(project_id, &item.code).await? {
            return Err(AppError::Validation(format!(
                "category_kind: у категории {} есть операции, планы или лимиты — тип «{}» не меняется",
                item.code,
                kind.as_str()
            )));
        }
        self.check_refs(project_id, &item).await?;
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    /// Архивирование вместо удаления: код остаётся занятым, ссылки на запись не «повисают».
    pub async fn archive(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        let all = self.repo.list(project_id, false, 10_000, 0).await?;
        if all.iter().any(|c| c.parent_code.as_deref() == Some(item.code.as_str())) {
            return Err(problem("err.archive_children", &[("name", &title(&item))]));
        }
        item.archived = true;
        self.repo.update(&item).await
    }

    /// Вернуть из архива («Отменить» в тосте). Родитель должен быть активным — иначе сначала его.
    pub async fn restore(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        if let Some(parent) = &item.parent_code
            && !self.exists(project_id, parent).await?
        {
            let parent = self.load(project_id, parent).await.map(|p| title(&p)).unwrap_or_else(|_| parent.clone());
            return Err(problem("err.restore_parent", &[("name", &title(&item)), ("parent", &parent)]));
        }
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

    async fn load(&self, project_id: Uuid, code: &str) -> AppResult<Category> {
        let code = normalize("code", code).map_err(AppError::Validation)?;
        self.repo
            .find_by_code(project_id, &code)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Категории: запись '{code}' не найдена")))
    }

    #[allow(unused_variables)]
    async fn check_refs(&self, project_id: Uuid, item: &Category) -> AppResult<()> {
        let all = self.repo.list(project_id, true, 10_000, 0).await?;
        check_tree(&all, item).map_err(AppError::Validation)
    }

    /// Дерево категорий в порядке обхода: (уровень, запись).
    pub async fn tree(&self, project_id: Uuid, include_archived: bool) -> AppResult<Vec<(usize, CategoryDto)>> {
        let all = self.repo.list(project_id, include_archived, 10_000, 0).await?;
        Ok(flatten(&all).into_iter().map(|(d, c)| (d, c.clone().into())).collect())
    }

    /// Публичный контракт для других фич: тип активной категории (EXPENSE / INCOME).
    pub async fn kind_of(&self, project_id: Uuid, code: &str) -> AppResult<Option<String>> {
        let Ok(code) = kernel::code::normalize("code", code) else { return Ok(None) };
        Ok(self
            .repo
            .find_by_code(project_id, &code)
            .await?
            .filter(|c| !c.archived)
            .map(|c| c.category_kind.as_str().to_string()))
    }

    /// Публичный контракт для других фич: активные категории деревом (tag — EXPENSE/INCOME).
    pub async fn choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        Ok(self
            .tree(project_id, false)
            .await?
            .into_iter()
            .map(|(depth, c)| Choice {
                label: c.title(lang).to_string(),
                tag: c.category_kind.clone(),
                code: c.code,
                depth,
            })
            .collect())
    }

    /// Публичный контракт для других фич: все категории (с архивными) — код, родитель, тип, название.
    pub async fn nodes(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<CategoryNode>> {
        Ok(self
            .tree(project_id, true)
            .await?
            .into_iter()
            .map(|(depth, c)| CategoryNode {
                title: c.title(lang).to_string(),
                code: c.code,
                parent_code: c.parent_code,
                kind: c.category_kind,
                depth,
                archived: c.archived,
            })
            .collect())
    }
}

/// Ошибка архива на языке запроса (техдолг #9): без поля — показывается целиком, в тосте или над формой.
fn problem(key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> AppError {
    Problem::validation("", tr(Some(crate::i18n::dict), key, args)).into()
}

/// Название категории на языке запроса.
fn title(c: &Category) -> String {
    CategoryDto::from(c.clone()).title(kernel::i18n::current()).to_string()
}

/// Узел дерева для других фич (бюджет, лимиты): порядок — обход в глубину.
#[derive(Debug, Clone)]
pub struct CategoryNode {
    pub code: String,
    pub parent_code: Option<String>,
    /// EXPENSE / INCOME
    pub kind: String,
    pub title: String,
    pub depth: usize,
    pub archived: bool,
}
