//! Тесты use cases фичи «Валюты» на in-memory адаптере: без БД и без других фич.

use std::sync::Arc;

use kernel::error::AppError;
use kernel::pagination::Pagination;
use uuid::Uuid;

use crate::adapters::memory::InMemoryCurrencyRepository;
use crate::dto::{CreateCurrencyDto, ListQuery, UpdateCurrencyDto};
use crate::service::CurrencyService;

/// Базовая валюта — THB; в RUB есть проводки и счёт `CARD_TBANK`.
struct FakeUsage;

#[async_trait::async_trait]
impl crate::ports::CurrencyUsage for FakeUsage {
    async fn has_postings(&self, _p: Uuid, currency: &str) -> kernel::error::AppResult<bool> {
        Ok(currency == "RUB")
    }
    async fn active_accounts(&self, _p: Uuid, currency: &str) -> kernel::error::AppResult<Vec<String>> {
        Ok(if currency == "RUB" { vec!["CARD_TBANK".into()] } else { Vec::new() })
    }
    async fn base_currency(&self, _p: Uuid) -> kernel::error::AppResult<Option<String>> {
        Ok(Some("THB".into()))
    }
}

fn deps() -> super::ports::Deps {
    super::ports::Deps { usage: Arc::new(FakeUsage) }
}

fn service() -> CurrencyService {
    CurrencyService::new(Arc::new(InMemoryCurrencyRepository::default()), deps())
}

fn create_dto(code: &str) -> CreateCurrencyDto {
    CreateCurrencyDto {
        code: code.to_string(),
        name: format!("Запись {code}"),
        name_lang: "RU".to_string(),
        name_ru: Some("Пример".to_string()),
        name_en: Some("Пример".to_string()),
        name_th: Some("Пример".to_string()),
        symbol: "Пример".to_string(),
        minor_units: 1,
    }
}

fn update_dto(name: &str) -> UpdateCurrencyDto {
    UpdateCurrencyDto {
        name: name.to_string(),
        archived: false,
        name_lang: "RU".to_string(),
        name_ru: Some("Пример".to_string()),
        name_en: Some("Пример".to_string()),
        name_th: Some("Пример".to_string()),
        symbol: "Пример".to_string(),
        minor_units: 1,
    }
}

fn page(size: i64) -> Pagination {
    Pagination { page: 0, size }
}

#[tokio::test]
async fn create_and_get_normalizes_code() {
    let svc = service();
    let user = Uuid::new_v4();
    let created = svc.create(user, create_dto(" thb ")).await.unwrap();
    assert_eq!(created.code, "THB");
    assert_eq!(svc.get(user, "thb").await.unwrap().id, created.id);
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
    svc.create(mine, create_dto("THB")).await.unwrap();
    assert!(matches!(svc.get(other, "THB").await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.update(other, "THB", update_dto("x")).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.archive(other, "THB").await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.restore(other, "THB").await.unwrap_err(), AppError::NotFound(_)));
    assert!(!svc.exists(other, "THB").await.unwrap());
    assert_eq!(svc.list(other, ListQuery::default(), page(10)).await.unwrap().page.total_elements, 0);
}

#[tokio::test]
async fn archive_can_be_undone_and_project_purged() {
    let svc = service();
    let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
    svc.create(mine, create_dto("USD")).await.unwrap();
    svc.create(other, create_dto("USD")).await.unwrap();
    svc.archive(mine, "USD").await.unwrap();
    assert!(!svc.exists(mine, "USD").await.unwrap());
    svc.restore(mine, "USD").await.unwrap();
    assert!(svc.exists(mine, "USD").await.unwrap(), "«Отменить» возвращает запись");
    assert_eq!(svc.purge_project(mine).await.unwrap(), 1);
    assert!(!svc.exists(mine, "USD").await.unwrap());
    assert!(svc.exists(other, "USD").await.unwrap(), "чужой проект не задет");
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing =
        kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
}

/// Ц8: знаков после запятой — от 0 до 4 (колонки NUMERIC(18,4)); меньше знаков нельзя, если в валюте
/// уже есть суммы; больше — можно.
#[tokio::test]
async fn minor_units_are_bounded() {
    let svc = service();
    let p = Uuid::new_v4();
    let is = |e: AppError| matches!(e, AppError::Validation(m) if m.starts_with("minor_units"));
    assert!(
        is(svc.create(p, CreateCurrencyDto { minor_units: 7, ..create_dto("XXX") }).await.unwrap_err()),
        "7 знаков"
    );
    assert!(
        is(svc.create(p, CreateCurrencyDto { minor_units: -1, ..create_dto("YYY") }).await.unwrap_err()),
        "−1 знак"
    );
    svc.create(p, CreateCurrencyDto { minor_units: 2, ..create_dto("RUB") }).await.unwrap();
    svc.create(p, CreateCurrencyDto { minor_units: 2, ..create_dto("USD") }).await.unwrap();
    let minor = |n: i32| UpdateCurrencyDto { minor_units: n, ..update_dto("Рубль") };
    assert!(is(svc.update(p, "RUB", minor(0)).await.unwrap_err()), "в RUB есть суммы с копейками");
    assert_eq!(svc.update(p, "RUB", minor(3)).await.unwrap().minor_units, 3, "больше знаков — можно");
    assert_eq!(svc.update(p, "USD", minor(0)).await.unwrap().minor_units, 0, "сумм в USD нет — можно и меньше");
}

/// Ц8: базовую валюту и валюту активных счетов не архивировать — ни кнопкой, ни через `archived` в PUT.
#[tokio::test]
async fn currency_in_use_is_not_archived() {
    let svc = service();
    let p = Uuid::new_v4();
    for code in ["THB", "RUB", "USD"] {
        svc.create(p, create_dto(code)).await.unwrap();
    }
    let conflict = |e: AppError, what: &str| matches!(e, AppError::Conflict(m) if m.contains(what));
    assert!(conflict(svc.archive(p, "THB").await.unwrap_err(), "базовая"));
    assert!(conflict(svc.archive(p, "RUB").await.unwrap_err(), "CARD_TBANK"));
    assert!(conflict(
        svc.update(p, "RUB", UpdateCurrencyDto { archived: true, ..update_dto("Рубль") }).await.unwrap_err(),
        "CARD_TBANK"
    ));
    svc.archive(p, "USD").await.unwrap();
    assert!(svc.get(p, "USD").await.unwrap().archived);
}
