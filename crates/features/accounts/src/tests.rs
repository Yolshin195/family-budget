//! Тесты use cases фичи «Счета» на in-memory адаптере: без БД и без других фич.

use std::sync::Arc;

use kernel::error::AppError;
use kernel::pagination::Pagination;
use uuid::Uuid;

use crate::adapters::memory::InMemoryAccountRepository;
use crate::dto::{CreateAccountDto, ListQuery, UpdateAccountDto};
use crate::service::AccountService;

/// Фейковый справочник для тестов: знает любой код, кроме UNKNOWN.
struct FakeLookup;

#[async_trait::async_trait]
impl super::ports::CurrenciesLookup for FakeLookup {
    async fn exists(&self, _project_id: Uuid, code: &str) -> kernel::error::AppResult<bool> {
        Ok(code != "UNKNOWN")
    }
}

/// Фейковый журнал операций: остатки счетов задаются в тесте, начальный остаток — тоже проводка.
#[derive(Default)]
struct FakeLedger(std::sync::Mutex<Vec<(String, rust_decimal::Decimal, chrono::NaiveDate)>>);

#[async_trait::async_trait]
impl super::ports::Ledger for Arc<FakeLedger> {
    async fn balances(
        &self,
        _p: Uuid,
        date: chrono::NaiveDate,
    ) -> kernel::error::AppResult<Vec<(String, rust_decimal::Decimal)>> {
        Ok(self.0.lock().unwrap().iter().filter(|x| x.2 <= date).map(|x| (x.0.clone(), x.1)).collect())
    }
    async fn opening(
        &self,
        _p: Uuid,
        code: &str,
    ) -> kernel::error::AppResult<Option<(rust_decimal::Decimal, chrono::NaiveDate)>> {
        Ok(self.0.lock().unwrap().iter().find(|x| x.0 == code).map(|x| (x.1, x.2)))
    }
    async fn set_opening(
        &self,
        _p: Uuid,
        _u: Uuid,
        code: &str,
        amount: rust_decimal::Decimal,
        date: chrono::NaiveDate,
    ) -> kernel::error::AppResult<()> {
        let mut rows = self.0.lock().unwrap();
        rows.retain(|x| x.0 != code);
        if !amount.is_zero() {
            rows.push((code.to_string(), amount, date));
        }
        Ok(())
    }
    async fn has_postings(&self, _p: Uuid, code: &str) -> kernel::error::AppResult<bool> {
        Ok(self.0.lock().unwrap().iter().any(|x| x.0 == code))
    }
}

fn deps() -> super::ports::Deps {
    super::ports::Deps { currencies: Arc::new(FakeLookup), ledger: Arc::new(Arc::new(FakeLedger::default())) }
}

fn service() -> AccountService {
    AccountService::new(Arc::new(InMemoryAccountRepository::default()), deps())
}

/// Остаток — из журнала: валюту счёта с проводками не сменить, счёт с остатком не архивировать.
#[tokio::test]
async fn balance_rules() {
    let svc = service();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    svc.create(p, create_dto("CARD")).await.unwrap();
    svc.set_opening(p, u, "card", rust_decimal::Decimal::from(500), day).await.unwrap();
    assert_eq!(svc.opening(p, "CARD").await.unwrap(), Some((rust_decimal::Decimal::from(500), day)));
    let mut other = update_dto("Карта");
    other.currency_code = "REF2".into();
    let err = svc.update(p, "CARD", other).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(m) if m.starts_with("currency_code")));
    assert!(matches!(svc.archive(p, "CARD").await.unwrap_err(), AppError::Conflict(_)), "остаток не нулевой");
    svc.set_opening(p, u, "CARD", rust_decimal::Decimal::ZERO, day).await.unwrap();
    svc.archive(p, "CARD").await.unwrap();
    assert!(svc.set_opening(p, u, "NOPE", rust_decimal::Decimal::ONE, day).await.is_err());
}

fn create_dto(code: &str) -> CreateAccountDto {
    CreateAccountDto {
        code: code.to_string(),
        name: format!("Запись {code}"),
        name_lang: "RU".to_string(),
        name_ru: Some("Пример".to_string()),
        name_en: Some("Пример".to_string()),
        name_th: Some("Пример".to_string()),
        account_kind: "CASH".to_string(),
        currency_code: "REF1".to_string(),
        holder_user_id: Some(uuid::Uuid::new_v4()),
    }
}

fn update_dto(name: &str) -> UpdateAccountDto {
    UpdateAccountDto {
        name: name.to_string(),
        archived: false,
        name_lang: "RU".to_string(),
        name_ru: Some("Пример".to_string()),
        name_en: Some("Пример".to_string()),
        name_th: Some("Пример".to_string()),
        account_kind: "CASH".to_string(),
        currency_code: "REF1".to_string(),
        holder_user_id: Some(uuid::Uuid::new_v4()),
    }
}

fn page(size: i64) -> Pagination {
    Pagination { page: 0, size }
}

#[tokio::test]
async fn create_and_get_normalizes_code() {
    let svc = service();
    let user = Uuid::new_v4();
    let created = svc.create(user, create_dto(" card_kbank ")).await.unwrap();
    assert_eq!(created.code, "CARD_KBANK");
    assert_eq!(svc.get(user, "card_kbank").await.unwrap().id, created.id);
}

#[tokio::test]
async fn duplicate_code_is_conflict() {
    let svc = service();
    let user = Uuid::new_v4();
    svc.create(user, create_dto("A")).await.unwrap();
    assert!(matches!(svc.create(user, create_dto("A")).await.unwrap_err(), AppError::Conflict(_)));
}

#[tokio::test]
async fn empty_name_is_validation_error() {
    let mut dto = create_dto("A");
    dto.name = "   ".into();
    assert!(matches!(service().create(Uuid::new_v4(), dto).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn invalid_name_lang_is_validation_error() {
    let mut dto = create_dto("A");
    dto.name_lang = "__WRONG__".into();
    assert!(matches!(service().create(Uuid::new_v4(), dto).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn invalid_account_kind_is_validation_error() {
    let mut dto = create_dto("A");
    dto.account_kind = "__WRONG__".into();
    assert!(matches!(service().create(Uuid::new_v4(), dto).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn unknown_currency_code_is_validation_error() {
    let mut dto = create_dto("A");
    dto.currency_code = "unknown".to_string();
    assert!(matches!(service().create(Uuid::new_v4(), dto).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn users_are_isolated() {
    let svc = service();
    let (alice, bob) = (Uuid::new_v4(), Uuid::new_v4());
    svc.create(alice, create_dto("A")).await.unwrap();
    assert!(matches!(svc.get(bob, "A").await.unwrap_err(), AppError::NotFound(_)));
    assert!(svc.create(bob, create_dto("A")).await.is_ok());
}

#[tokio::test]
async fn update_changes_name() {
    let svc = service();
    let user = Uuid::new_v4();
    svc.create(user, create_dto("A")).await.unwrap();
    assert_eq!(svc.update(user, "A", update_dto("Новое имя")).await.unwrap().name, "Новое имя");
}

#[tokio::test]
async fn archive_hides_from_list_and_exists() {
    let svc = service();
    let user = Uuid::new_v4();
    svc.create(user, create_dto("A")).await.unwrap();
    assert!(svc.exists(user, "a").await.unwrap());
    svc.archive(user, "A").await.unwrap();
    assert!(!svc.exists(user, "A").await.unwrap());
    assert_eq!(svc.list(user, ListQuery::default(), page(10)).await.unwrap().page.total_elements, 0);
    let all = ListQuery { include_archived: true };
    assert_eq!(svc.list(user, all, page(10)).await.unwrap().page.total_elements, 1);
    assert!(matches!(svc.create(user, create_dto("A")).await.unwrap_err(), AppError::Conflict(_)));
}

#[tokio::test]
async fn list_is_paged_and_sorted() {
    let svc = service();
    let user = Uuid::new_v4();
    for code in ["C", "A", "B"] {
        svc.create(user, create_dto(code)).await.unwrap();
    }
    let result = svc.list(user, ListQuery::default(), page(2)).await.unwrap();
    let codes: Vec<_> = result.content.iter().map(|x| x.code.as_str()).collect();
    assert_eq!(codes, ["A", "B"]);
    assert_eq!((result.page.total_elements, result.page.total_pages), (3, 2));
}

/// ТЗ, раздел 13: каждый запрос фильтруется по проекту — чужой проект видит 404, а не данные.
#[tokio::test]
async fn foreign_project_gets_not_found() {
    let svc = service();
    let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
    svc.create(mine, create_dto("CASH")).await.unwrap();
    assert!(matches!(svc.get(other, "CASH").await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.update(other, "CASH", update_dto("x")).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.archive(other, "CASH").await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.restore(other, "CASH").await.unwrap_err(), AppError::NotFound(_)));
    assert!(!svc.exists(other, "CASH").await.unwrap());
    assert_eq!(svc.list(other, ListQuery::default(), page(10)).await.unwrap().page.total_elements, 0);
}

#[tokio::test]
async fn archive_can_be_undone_and_project_purged() {
    let svc = service();
    let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
    svc.create(mine, create_dto("CASH")).await.unwrap();
    svc.create(other, create_dto("CASH")).await.unwrap();
    svc.archive(mine, "CASH").await.unwrap();
    assert!(!svc.exists(mine, "CASH").await.unwrap());
    svc.restore(mine, "CASH").await.unwrap();
    assert!(svc.exists(mine, "CASH").await.unwrap(), "«Отменить» возвращает запись");
    assert_eq!(svc.purge_project(mine).await.unwrap(), 1);
    assert!(!svc.exists(mine, "CASH").await.unwrap());
    assert!(svc.exists(other, "CASH").await.unwrap(), "чужой проект не задет");
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing =
        kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
}
