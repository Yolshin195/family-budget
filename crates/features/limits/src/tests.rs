//! Тесты use cases фичи «Лимиты» на in-memory адаптере: без БД и без других фич.

use std::sync::Arc;

use kernel::error::AppError;
use kernel::pagination::Pagination;
use uuid::Uuid;

use crate::adapters::memory::InMemoryLimitRepository;
use crate::dto::{LimitInput, PeriodQuery};
use crate::service::LimitService;

/// Фейковый справочник для тестов: знает любой код, кроме UNKNOWN.
struct FakeLookup;

#[async_trait::async_trait]
impl super::ports::CategoriesLookup for FakeLookup {
    async fn exists(&self, _project_id: Uuid, code: &str) -> kernel::error::AppResult<bool> {
        Ok(code != "UNKNOWN")
    }
}

fn deps() -> super::ports::Deps {
    super::ports::Deps { categories: Arc::new(FakeLookup) }
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
        amount: Some(rust_decimal::Decimal::new(1050, 2)),
        removed: false,
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

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing =
        kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
}

/// ТЗ, раздел 13: чужой проект → 404; удаление отменяется из тоста; проект очищается целиком.
#[tokio::test]
async fn foreign_project_undo_and_purge() {
    let svc = service();
    let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
    let created = svc.create(mine, input(1)).await.unwrap();
    assert!(matches!(svc.get(other, created.id).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.update(other, created.id, input(2)).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.delete(other, created.id).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.delete_with_undo(other, created.id).await.unwrap_err(), AppError::NotFound(_)));

    let token = svc.delete_with_undo(mine, created.id).await.unwrap();
    assert!(svc.get(mine, created.id).await.is_err());
    assert!(
        matches!(svc.undo_delete(other, token).await.unwrap_err(), AppError::NotFound(_)),
        "чужой проект не может отменить"
    );
    svc.undo_delete(mine, token).await.unwrap();
    assert_eq!(svc.get(mine, created.id).await.unwrap().id, created.id, "тот же id");
    assert!(svc.undo_delete(mine, token).await.is_err(), "второй раз не отменить");

    svc.create(other, input(3)).await.unwrap();
    assert_eq!(svc.purge_project(mine).await.unwrap(), 1);
    assert_eq!(svc.all(other).await.unwrap().len(), 1);
}

/// Ц5: «Снять лимит» — правило без суммы; сумма и снятие вместе не задаются, без суммы лимит не создать.
#[tokio::test]
async fn removed_limit_has_no_amount() {
    let svc = service();
    let p = Uuid::new_v4();
    let removed = svc.create(p, LimitInput { amount: None, removed: true, ..input(2) }).await.unwrap();
    assert!(removed.removed && removed.amount.is_none());
    assert_eq!(
        removed.effective_from,
        chrono::NaiveDate::from_ymd_opt(2025, 12, 29).unwrap(),
        "неделя 02.01.2026 — с понедельника 29.12"
    );
    let both = svc.create(p, LimitInput { removed: true, ..input(3) }).await.unwrap_err();
    assert!(matches!(both, AppError::Validation(m) if m.starts_with("amount:")));
    let none = svc.create(p, LimitInput { amount: None, ..input(4) }).await.unwrap_err();
    assert!(matches!(none, AppError::Validation(m) if m.starts_with("amount:")));
    let zero = svc.create(p, LimitInput { amount: Some(rust_decimal::Decimal::ZERO), ..input(5) }).await.unwrap();
    assert_eq!(
        (zero.amount, zero.removed),
        (Some(rust_decimal::Decimal::ZERO), false),
        "0 — нулевой лимит, а не снятие"
    );
}

/// Issue #58: недельный лимит со среды 14.10, затем «Снять лимит» (месяц) с 22.10 — текущим считается снятие,
/// недельный лимит помечен «заменён»; более раннее месячное правило сентября тоже «заменено».
#[tokio::test]
async fn later_longer_rule_is_current() {
    use crate::domain::{LimitLimitPeriod, RuleState, rule_states};
    let svc = service();
    let p = Uuid::new_v4();
    let date = |m, d| chrono::NaiveDate::from_ymd_opt(2026, m, d).unwrap();
    let rule = |m, d, period: &str, removed: bool| LimitInput {
        valid_from: date(m, d),
        limit_period: period.into(),
        amount: (!removed).then(|| rust_decimal::Decimal::new(1000, 0)),
        removed,
        ..input(1)
    };
    svc.create(p, rule(9, 1, "MONTH", false)).await.unwrap();
    svc.create(p, rule(10, 14, "WEEK", false)).await.unwrap();
    svc.create(p, rule(10, 22, "MONTH", true)).await.unwrap();
    let mut all = svc.all(p).await.unwrap();
    all.sort_by_key(|l| std::cmp::Reverse(l.valid_from));
    let keys: Vec<_> = all.iter().map(|l| (LimitLimitPeriod::parse(&l.limit_period).unwrap(), l.valid_from)).collect();
    let states = rule_states(&keys, date(10, 25));
    let state = |current, superseded| RuleState { current, superseded };
    assert!(all[0].removed);
    assert_eq!(states, vec![state(true, false), state(false, true), state(false, true)]);
    // До 01.10 действует сентябрьский месячный лимит; снятие и неделя ещё не начались.
    assert_eq!(rule_states(&keys, date(9, 20)), vec![state(false, false), state(false, true), state(true, false)]);
}
