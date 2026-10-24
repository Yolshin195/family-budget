//! Тесты use cases фичи «Курсы валют» на in-memory адаптере: без БД и без других фич.

use std::sync::Arc;

use kernel::error::AppError;
use kernel::pagination::Pagination;
use uuid::Uuid;

use crate::adapters::memory::InMemoryRateRepository;
use crate::dto::{PeriodQuery, RateInput};
use crate::service::RateService;

/// Фейковый справочник для тестов: знает любой код, кроме UNKNOWN.
struct FakeLookup;

#[async_trait::async_trait]
impl super::ports::CurrenciesLookup for FakeLookup {
    async fn exists(&self, _project_id: Uuid, code: &str) -> kernel::error::AppResult<bool> {
        Ok(code != "UNKNOWN")
    }
}

fn deps() -> super::ports::Deps {
    super::ports::Deps { currencies: Arc::new(FakeLookup) }
}

fn service() -> RateService {
    RateService::new(Arc::new(InMemoryRateRepository::default()), deps())
}

/// Значение поля `rate_date` для «дня N».
fn day(n: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 1, n).unwrap()
}

fn input(n: u32) -> RateInput {
    RateInput {
        rate_date: day(n),
        currency_code: "REF1".to_string(),
        rate: rust_decimal::Decimal::new(1050, 2),
        inverted: false,
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
async fn unknown_currency_code_is_validation_error() {
    let mut dto = input(1);
    dto.currency_code = "unknown".to_string();
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
    assert_eq!(result.content[0].rate_date, day(10));
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
    assert_eq!(updated.rate_date, day(3));
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

// ---------------------------------------------------------------- направление ввода (issue #22)

fn d(s: &str) -> rust_decimal::Decimal {
    s.parse().unwrap()
}

fn rate_in(n: u32, currency: &str, rate: &str, inverted: bool) -> RateInput {
    RateInput { rate_date: day(n), currency_code: currency.into(), rate: d(rate), inverted }
}

#[test]
fn entered_converts_to_base_rate_both_ways() {
    use crate::domain::base_rate;
    // 1 USD = 36.5 THB — прямое направление, курс к базовой равен введённому.
    assert_eq!(base_rate(d("36.5"), false).unwrap(), (d("36.5"), d("36.5")));
    // 1 THB = 2.50 RUB — обратное: 1 RUB = 0.4 THB, лишние нули введённого числа не важны.
    assert_eq!(base_rate(d("2.50"), true).unwrap(), (d("2.5"), d("0.4")));
    let (_, r) = base_rate(d("2.53"), true).unwrap();
    assert_eq!(r, d("0.395256916996"), "12 знаков после запятой");
    // Обратно в 6-м знаке не расходится.
    assert_eq!(crate::domain::invert(r).round_dp(6), d("2.53"));
}

#[test]
fn entered_must_be_positive_and_fit_the_column() {
    use crate::domain::base_rate;
    for inverted in [false, true] {
        assert!(base_rate(d("0"), inverted).is_err());
        assert!(base_rate(d("-2.5"), inverted).is_err());
    }
    assert!(base_rate(d("0.0000001"), false).is_err(), "больше 6 знаков");
    assert!(base_rate(d("2.5000000"), false).is_ok(), "нули в конце не считаются");
}

#[tokio::test]
async fn rate_entered_inverted_is_stored_as_typed() {
    let svc = service();
    let p = Uuid::new_v4();
    let r = svc.create(p, rate_in(1, "rub", "2.50", true)).await.unwrap();
    assert_eq!((r.currency_code.as_str(), r.entered, r.inverted), ("RUB", d("2.5"), true));
    assert_eq!(r.rate, d("0.4"), "1 RUB = 0.4 THB");
    assert_eq!(r.inverse_rate, d("2.5"), "обратный курс — ровно введённое число");
    // 1000 ₽ в бюджете = 400 ฿.
    assert_eq!(d("1000") * r.rate, d("400"));

    let usd = svc.create(p, rate_in(1, "USD", "36.5", false)).await.unwrap();
    assert_eq!((usd.rate, usd.entered, usd.inverted), (d("36.5"), d("36.5"), false));
    assert_eq!(usd.inverse_rate, d("0.027397260274"));
}

#[tokio::test]
async fn same_entry_again_gives_same_numbers() {
    let svc = service();
    let p = Uuid::new_v4();
    let a = svc.create(p, rate_in(1, "RUB", "2.53", true)).await.unwrap();
    let b = svc.update(p, a.id, rate_in(1, "RUB", "2.53", true)).await.unwrap();
    assert_eq!((a.rate, a.entered, a.inverse_rate), (b.rate, b.entered, b.inverse_rate));
    assert_eq!(svc.get(p, a.id).await.unwrap().entered, d("2.53"));
}

#[tokio::test]
async fn old_clients_without_inverted_enter_base_units() {
    let body = serde_json::json!({"rate_date": "2026-01-01", "currency_code": "USD", "rate": "36.5"});
    let input: RateInput = serde_json::from_value(body).unwrap();
    assert!(!input.inverted);
    let r = service().create(Uuid::new_v4(), input).await.unwrap();
    assert_eq!((r.rate, r.entered, r.inverted), (d("36.5"), d("36.5"), false));
}

#[tokio::test]
async fn default_direction_comes_from_the_latest_rate_of_the_currency() {
    let svc = service();
    let p = Uuid::new_v4();
    assert!(!svc.default_inverted(p, "RUB").await.unwrap(), "курсов нет — прямое направление");
    svc.create(p, rate_in(1, "RUB", "0.4", false)).await.unwrap();
    assert!(!svc.default_inverted(p, "RUB").await.unwrap());
    svc.create(p, rate_in(5, "RUB", "2.48", true)).await.unwrap();
    svc.create(p, rate_in(9, "USD", "36.5", false)).await.unwrap();
    assert!(svc.default_inverted(p, "rub").await.unwrap(), "последний курс рубля введён наоборот");
    assert!(!svc.default_inverted(p, "USD").await.unwrap());
    assert!(!svc.default_inverted(p, "EUR").await.unwrap());
    // Более старый курс в другом направлении не влияет.
    svc.create(p, rate_in(3, "USD", "0.0274", true)).await.unwrap();
    assert!(!svc.default_inverted(p, "USD").await.unwrap());
    let latest = svc.latest_by_currency(p).await.unwrap();
    assert_eq!(
        latest.iter().map(|r| (r.currency_code.as_str(), r.rate_date)).collect::<Vec<_>>(),
        vec![("USD", day(9)), ("RUB", day(5))]
    );
    assert!(!svc.default_inverted(Uuid::new_v4(), "RUB").await.unwrap(), "чужой проект не влияет");
}

/// Миграция `rate_direction` на живой PostgreSQL: у старых строк `entered = rate`, `inverted = false`.
/// Нужен `TEST_DATABASE_URL`, без него тест пропускается (`cargo test` по-прежнему без БД).
/// Всё выполняется во временной схеме внутри транзакции и откатывается.
#[tokio::test]
async fn migration_fills_entered_from_rate() {
    use sqlx::{Connection, Row};
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL не задан — тест миграции пропущен");
        return;
    };
    let migrator = sqlx::migrate!("./migrations");
    let sql = |desc: &str| {
        migrator.iter().find(|m| m.description == desc && m.migration_type.is_up_migration()).unwrap().sql.to_string()
    };
    let mut conn = sqlx::PgConnection::connect(&url).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    let schema = format!("rates_mig_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE SCHEMA {schema}; SET LOCAL search_path TO {schema}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(&sql("create fb rates")).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO fb_rates (id, project_id, rate_date, currency_code, rate) VALUES ($1, $2, $3, 'USD', 36.5), ($4, $2, $3, 'RUB', 0.4)")
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(day(1))
        .bind(Uuid::new_v4())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(&sql("rate direction")).execute(&mut *tx).await.unwrap();
    let rows = sqlx::query("SELECT currency_code, rate, entered, inverted FROM fb_rates ORDER BY currency_code")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let got: Vec<(String, rust_decimal::Decimal, rust_decimal::Decimal, bool)> = rows
        .iter()
        .map(|r| {
            (
                r.get(0),
                r.get::<rust_decimal::Decimal, _>(1).normalize(),
                r.get::<rust_decimal::Decimal, _>(2).normalize(),
                r.get(3),
            )
        })
        .collect();
    assert_eq!(got, vec![("RUB".into(), d("0.4"), d("0.4"), false), ("USD".into(), d("36.5"), d("36.5"), false)]);
    // Новый курс «1 THB = 2.53 RUB» хранится с 12 знаками и не расходится с введённым.
    sqlx::query("UPDATE fb_rates SET entered = 2.53, inverted = true, rate = $1 WHERE currency_code = 'RUB'")
        .bind(crate::domain::invert(d("2.53")))
        .execute(&mut *tx)
        .await
        .unwrap();
    let (rate,): (rust_decimal::Decimal,) =
        sqlx::query_as("SELECT rate FROM fb_rates WHERE currency_code = 'RUB'").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(rate.normalize(), d("0.395256916996"));
    tx.rollback().await.unwrap();
}
