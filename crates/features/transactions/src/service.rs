//! Use cases фичи «Транзакции». Знает только домен и порты — ничего об HTTP и SQL.

use std::sync::Arc;

use chrono::NaiveDate;
use kernel::code::ensure_ref;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::pagination::{PagedResponse, Pagination};
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::Transaction;
use crate::dto::{ListQuery, TransactionDto, TransactionInput, TranslationInput};
use crate::ports::{Deps, FactLine, Filter, Payment, TransactionRepository};

pub struct TransactionService {
    repo: Arc<dyn TransactionRepository>,
    deps: Deps,
}

impl TransactionService {
    pub fn new(repo: Arc<dyn TransactionRepository>, deps: Deps) -> Self {
        Self { repo, deps }
    }

    /// Создать транзакцию. `created_by` — текущий пользователь, не из тела запроса.
    pub async fn create(&self, project_id: Uuid, created_by: Uuid, input: TransactionInput) -> AppResult<TransactionDto> {
        let item = Transaction::create(project_id, created_by, input.into_domain().map_err(AppError::Validation)?)
            .map_err(AppError::Validation)?;
        self.check_refs(project_id, &item).await?;
        self.repo.insert(&item).await.map_err(plan_conflict)?;
        Ok(item.into())
    }

    pub async fn get(&self, project_id: Uuid, id: Uuid) -> AppResult<TransactionDto> {
        Ok(self.load(project_id, id).await?.into())
    }

    pub async fn list(&self, project_id: Uuid, query: ListQuery, pagination: Pagination) -> AppResult<PagedResponse<TransactionDto>> {
        let filter = Filter {
            from: query.from,
            to: query.to,
            account_code: query.account_code.filter(|v| !v.is_empty()),
            created_by: query.created_by,
            untranslated: query.untranslated.as_deref().and_then(Lang::parse),
        };
        let items = self.repo.list(project_id, &filter, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(project_id, &filter).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    pub async fn update(&self, project_id: Uuid, id: Uuid, input: TransactionInput) -> AppResult<TransactionDto> {
        let mut item = self.load(project_id, id).await?;
        item.change(input.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        self.check_refs(project_id, &item).await?;
        self.repo.update(&item).await.map_err(plan_conflict)?;
        Ok(item.into())
    }

    /// Изменить только переводы описания.
    pub async fn translate(&self, project_id: Uuid, id: Uuid, input: TranslationInput) -> AppResult<TransactionDto> {
        let mut item = self.load(project_id, id).await?;
        if item.description.text.is_none() {
            return Err(AppError::Validation("description: у транзакции нет описания — переводить нечего".into()));
        }
        item.translate(input.description_ru, input.description_en, input.description_th);
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    /// Удаление. Если транзакция оплачивала вхождение плана — оно снова «ожидается».
    pub async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<()> {
        if self.repo.delete(project_id, id).await? { Ok(()) } else { Err(not_found(id)) }
    }

    // ---- Публичный контракт для других фич (бюджет, планирование) ----

    /// Позиции за период включительно.
    pub async fn lines(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<FactLine>> {
        self.repo.lines(project_id, from, to).await
    }

    /// Оплаченные вхождения планов за период (по дате вхождения).
    pub async fn payments(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<Payment>> {
        self.repo.payments(project_id, from, to).await
    }

    /// Обороты по счетам по дату включительно: (счёт, доходы, расходы).
    pub async fn turnover(&self, project_id: Uuid, to: NaiveDate) -> AppResult<Vec<(String, Decimal, Decimal)>> {
        self.repo.turnover(project_id, to).await
    }

    // ---- Для страниц ----

    pub async fn account_choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.deps.accounts.choices(project_id, lang).await
    }

    pub async fn category_choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.deps.categories.choices(project_id, lang).await
    }

    pub async fn account_titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        self.deps.accounts.titles(project_id, lang).await
    }

    pub async fn category_titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        self.deps.categories.titles(project_id, lang).await
    }

    async fn load(&self, project_id: Uuid, id: Uuid) -> AppResult<Transaction> {
        self.repo.find(project_id, id).await?.ok_or_else(|| not_found(id))
    }

    async fn check_refs(&self, project_id: Uuid, item: &Transaction) -> AppResult<()> {
        ensure_ref(self.deps.accounts.exists(project_id, &item.account_code).await?, "account_code", &item.account_code)?;
        for (i, line) in item.lines.iter().enumerate() {
            let field = format!("lines[{}].category_code", i + 1);
            match self.deps.categories.kind_of(project_id, &line.category_code).await? {
                None => ensure_ref(false, &field, &line.category_code)?,
                Some(kind) if kind != item.tx_kind.as_str() => {
                    return Err(AppError::Validation(format!(
                        "{field}: категория '{}' — {kind}, а транзакция — {}",
                        line.category_code,
                        item.tx_kind.as_str()
                    )));
                }
                Some(_) => {}
            }
        }
        Ok(())
    }
}

fn not_found(id: Uuid) -> AppError {
    AppError::NotFound(format!("Транзакции: запись {id} не найдена"))
}

/// Уникальный индекс по (plan_id, plan_date): это вхождение уже оплачено.
fn plan_conflict(e: AppError) -> AppError {
    match e {
        AppError::Conflict(_) => AppError::Conflict("это вхождение плана уже оплачено другой транзакцией".into()),
        other => other,
    }
}
