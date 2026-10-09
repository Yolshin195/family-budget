//! Тесты use cases фичи «Лимиты» на in-memory адаптере: без БД и без других фич.

use std::sync::Arc;

use kernel::error::AppError;
use kernel::pagination::Pagination;
use uuid::Uuid;

use crate::adapters::memory::InMemoryLimitRepository;
use crate::dto::{PeriodQuery, LimitInput};
use crate::service::LimitService;


/// Фейковый справочник для тестов: знает любой код, кроме UNKNOWN.
struct FakeLookup;

#[async_trait::async_trait]
impl super::ports::CategoriesLookup for FakeLookup {
    async fn exists(&self, _user_id: Uuid, code: &str) -> kernel::error::AppResult<bool> {
        Ok(code != "UNKNOWN")
    }
}

fn deps() -> super::ports::Deps {
    super::ports::Deps {
        categories: Arc::new(FakeLookup),
    }
}

fn service() -> LimitService {
    LimitService::new(Arc::new(InMemoryLimitRepository::default()), deps())
}

/// Значение поля `valid_from` для «дня N».
fn day(n: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 1, n).unwrap()
}

fn input(n: u32) -> LimitInput {
    LimitInput {
        valid_from: day(n),
        category_code: "REF1".to_string(),
        limit_period: "WEEK".to_string(),
        amount: rust_decimal::Decimal::new(1050, 2),
    }
}

fn page() -> Pagination {
    Pagination { page: 0, size: 100 }
}

#[tokio::test]
async fn create_and_get() {
    let svc = service();
    let user = Uuid::new_v4();
    let created = svc.create(user, input(1)).await.unwrap();
    assert_eq!(svc.get(user, created.id).await.unwrap().id, created.id);
}

#[tokio::test]
async fn invalid_limit_period_is_validation_error() {
    let mut dto = input(1);
    dto.limit_period = "__WRONG__".into();
    assert!(matches!(service().create(Uuid::new_v4(), dto).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn unknown_category_code_is_validation_error() {
    let mut dto = input(1);
    dto.category_code = "unknown".to_string();
    assert!(matches!(service().create(Uuid::new_v4(), dto).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn list_filters_by_period_newest_first() {
    let svc = service();
    let user = Uuid::new_v4();
    for n in [1, 5, 10] {
        svc.create(user, input(n)).await.unwrap();
    }
    let period = PeriodQuery { from: Some(day(2)), to: Some(day(10)) };
    let result = svc.list(user, period, page()).await.unwrap();
    assert_eq!(result.page.total_elements, 2);
    assert_eq!(result.content[0].valid_from, day(10));
    assert_eq!(svc.list(user, PeriodQuery::default(), page()).await.unwrap().page.total_elements, 3);
}

#[tokio::test]
async fn users_are_isolated() {
    let svc = service();
    let created = svc.create(Uuid::new_v4(), input(1)).await.unwrap();
    assert!(matches!(svc.get(Uuid::new_v4(), created.id).await.unwrap_err(), AppError::NotFound(_)));
}

#[tokio::test]
async fn update_then_delete() {
    let svc = service();
    let user = Uuid::new_v4();
    let created = svc.create(user, input(1)).await.unwrap();
    let updated = svc.update(user, created.id, input(3)).await.unwrap();
    assert_eq!(updated.valid_from, day(3));
    svc.delete(user, created.id).await.unwrap();
    assert!(matches!(svc.delete(user, created.id).await.unwrap_err(), AppError::NotFound(_)));
}
