//! Тесты use cases фичи «Транзакции» на in-memory адаптере: без БД и без других фич.

use std::str::FromStr;
use std::sync::Arc;

use chrono::NaiveDate;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::pagination::Pagination;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::adapters::memory::InMemoryTransactionRepository;
use crate::dto::{LineDto, ListQuery, TransactionInput, TranslationInput};
use crate::ports::{AccountInfo, AccountsLookup, BudgetView, CategoriesLookup, CategoryInfo, CategoryLimit, Deps};
use crate::service::TransactionService;

struct FakeAccounts;
struct FakeCategories;

#[async_trait::async_trait]
impl AccountsLookup for FakeAccounts {
    async fn exists(&self, _p: Uuid, code: &str) -> AppResult<bool> {
        Ok(code == "CARD" || code == "WISE")
    }
    async fn choices(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<Choice>> {
        Ok(vec![])
    }
    async fn titles(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(vec![])
    }
    async fn infos(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<AccountInfo>> {
        Ok(vec![
            AccountInfo { code: "CARD".into(), title: "Карта".into(), currency: "THB".into(), symbol: "฿".into() },
            AccountInfo { code: "WISE".into(), title: "Wise".into(), currency: "USD".into(), symbol: "$".into() },
        ])
    }
}

#[async_trait::async_trait]
impl CategoriesLookup for FakeCategories {
    async fn kind_of(&self, _p: Uuid, code: &str) -> AppResult<Option<String>> {
        Ok(match code {
            "FOOD" | "CAFE" | "HOME" => Some("EXPENSE".into()),
            "SALARY" => Some("INCOME".into()),
            _ => None,
        })
    }
    async fn choices(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<Choice>> {
        Ok(vec![])
    }
    async fn titles(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(vec![])
    }
    async fn infos(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<CategoryInfo>> {
        let c = |code: &str, parent: Option<&str>, kind: &str, depth: usize, names: &str| CategoryInfo {
            code: code.into(),
            parent_code: parent.map(Into::into),
            kind: kind.into(),
            title: code.into(),
            depth,
            names: names.into(),
        };
        Ok(vec![
            c("FOOD", None, "EXPENSE", 0, "Еда Food อาหาร"),
            c("CAFE", Some("FOOD"), "EXPENSE", 1, "Кафе Cafe ร้านอาหาร"),
            c("HOME", None, "EXPENSE", 0, "Жильё Housing ที่อยู่อาศัย"),
            c("SALARY", None, "INCOME", 0, "Зарплата Salary เงินเดือน"),
        ])
    }
}

struct FakeBudget;

#[async_trait::async_trait]
impl BudgetView for FakeBudget {
    async fn limits(&self, _p: Uuid, _base: &str, _date: NaiveDate, _l: Lang) -> AppResult<Vec<CategoryLimit>> {
        Ok(vec![
            CategoryLimit { code: "FOOD".into(), limit: Some(d("10000")), forecast: d("9500"), available: Some(d("500")) },
            CategoryLimit { code: "CAFE".into(), limit: None, forecast: d("1000"), available: Some(d("500")) },
        ])
    }
    async fn rates(&self, _p: Uuid, _base: &str, _date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        Ok(vec![("USD".into(), d("33.25"))])
    }
}

fn service() -> TransactionService {
    TransactionService::new(
        Arc::new(InMemoryTransactionRepository::default()),
        Deps { accounts: Arc::new(FakeAccounts), categories: Arc::new(FakeCategories), budget: Arc::new(FakeBudget) },
    )
}

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, n).unwrap()
}

fn receipt(lines: &[(&str, &str)]) -> TransactionInput {
    TransactionInput {
        occurred_on: day(3),
        account_code: "card".into(),
        tx_kind: "EXPENSE".into(),
        description: Some("Big C".into()),
        description_lang: "EN".into(),
        description_ru: None,
        description_en: None,
        description_th: None,
        plan_id: None,
        plan_date: None,
        lines: lines.iter().map(|(c, a)| LineDto { category_code: c.to_string(), amount: d(a), note: None }).collect(),
    }
}

#[tokio::test]
async fn receipt_with_several_lines() {
    let svc = service();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let t = svc.create(p, u, receipt(&[("food", "900"), ("home", "250"), ("cafe", "100")])).await.unwrap();
    assert_eq!(t.total, d("1250"));
    assert_eq!(t.lines.len(), 3);
    assert_eq!(t.created_by, u);
    let facts = svc.lines(p, day(1), day(31)).await.unwrap();
    assert_eq!(facts.iter().map(|f| f.amount).sum::<Decimal>(), d("1250"));
}

#[tokio::test]
async fn category_kind_must_match() {
    let svc = service();
    let err = svc.create(Uuid::new_v4(), Uuid::new_v4(), receipt(&[("food", "10"), ("salary", "5")])).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(m) if m.contains("lines[2]")));
    let err = svc.create(Uuid::new_v4(), Uuid::new_v4(), receipt(&[("nope", "10")])).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn unknown_account_is_validation_error() {
    let svc = service();
    let mut input = receipt(&[("food", "10")]);
    input.account_code = "CASH".into();
    assert!(matches!(svc.create(Uuid::new_v4(), Uuid::new_v4(), input).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn plan_occurrence_is_paid_once_and_freed_on_delete() {
    let svc = service();
    let (p, u, plan) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let mut input = receipt(&[("home", "15000")]);
    input.plan_id = Some(plan);
    input.plan_date = Some(day(5));
    let t = svc.create(p, u, input).await.unwrap();
    assert_eq!(svc.payments(p, day(1), day(31)).await.unwrap().len(), 1);
    let mut again = receipt(&[("home", "15000")]);
    again.plan_id = Some(plan);
    again.plan_date = Some(day(5));
    assert!(matches!(svc.create(p, u, again).await.unwrap_err(), AppError::Conflict(_)));
    svc.delete(p, t.id).await.unwrap();
    assert!(svc.payments(p, day(1), day(31)).await.unwrap().is_empty());
}

#[tokio::test]
async fn translation_filter() {
    let svc = service();
    let p = Uuid::new_v4();
    let t = svc.create(p, Uuid::new_v4(), receipt(&[("food", "10")])).await.unwrap();
    let q = || ListQuery { untranslated: Some("th".into()), ..Default::default() };
    assert_eq!(svc.list(p, q(), Pagination { page: 0, size: 10 }).await.unwrap().content.len(), 1);
    let tr = TranslationInput { description_ru: Some("Биг Си".into()), description_en: Some("ignored: original".into()), description_th: Some("บิ๊กซี".into()) };
    let t = svc.translate(p, t.id, tr).await.unwrap();
    assert_eq!(t.description_en, None, "оригинал на EN — перевод на EN не хранится");
    assert_eq!(t.title(Lang::Th), "บิ๊กซี");
    assert_eq!(svc.list(p, q(), Pagination { page: 0, size: 10 }).await.unwrap().content.len(), 0);
}

#[tokio::test]
async fn projects_are_isolated() {
    let svc = service();
    let t = svc.create(Uuid::new_v4(), Uuid::new_v4(), receipt(&[("food", "10")])).await.unwrap();
    assert!(matches!(svc.get(Uuid::new_v4(), t.id).await.unwrap_err(), AppError::NotFound(_)));
}

/// Быстрый расход: последний счёт пользователя, частые категории за 30 дней (дополняются корнями
/// дерева), остатки лимитов и курсы — из транзакций, без новых таблиц.
#[tokio::test]
async fn quick_defaults_follow_user_habits() {
    let svc = service();
    let (p, me, anna) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let fresh = svc.quick_defaults(p, me, "THB", Lang::Ru, day(13)).await.unwrap();
    assert_eq!(fresh.default_account.as_deref(), Some("CARD"), "нет истории — первый счёт");
    assert_eq!(fresh.frequent_expense, vec!["FOOD", "HOME"], "нет истории — корни дерева расходов");

    for _ in 0..3 {
        svc.create(p, me, receipt(&[("CAFE", "100")])).await.unwrap();
    }
    svc.create(p, me, TransactionInput { account_code: "WISE".into(), ..receipt(&[("HOME", "50")]) }).await.unwrap();
    svc.create(p, anna, receipt(&[("FOOD", "999")])).await.unwrap();
    let q = svc.quick_defaults(p, me, "THB", Lang::Ru, day(13)).await.unwrap();
    assert_eq!(q.default_account.as_deref(), Some("WISE"), "последний использованный этим пользователем");
    assert_eq!(q.frequent_expense, vec!["CAFE", "HOME", "FOOD"], "сначала частые этого пользователя");
    assert_eq!(q.frequent_income, vec!["SALARY"]);
    let cafe = q.categories.iter().find(|c| c.code == "CAFE").unwrap();
    assert_eq!((cafe.available, cafe.search.contains("Cafe")), (Some(d("500")), true));
    assert_eq!(q.accounts.iter().find(|a| a.code == "WISE").unwrap().rate_to_base, d("33.25"));
    // через 30 дней привычки «остывают»
    let later = svc.quick_defaults(p, me, "THB", Lang::Ru, day(3) + chrono::Days::new(40)).await.unwrap();
    assert_eq!(later.frequent_expense, vec!["FOOD", "HOME"]);
}

/// ТЗ, раздел 13: чужой проект → 404; удаление отменяется с тем же id; проект очищается целиком.
#[tokio::test]
async fn foreign_project_undo_and_purge() {
    let svc = service();
    let (mine, other, me) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let t = svc.create(mine, me, receipt(&[("FOOD", "900"), ("CAFE", "100")])).await.unwrap();
    assert!(matches!(svc.get(other, t.id).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.update(other, t.id, receipt(&[("FOOD", "1")])).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.delete(other, t.id).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.delete_with_undo(other, t.id).await.unwrap_err(), AppError::NotFound(_)));
    assert!(svc.lines(other, day(1), day(31)).await.unwrap().is_empty());

    let token = svc.delete_with_undo(mine, t.id).await.unwrap();
    assert!(svc.get(mine, t.id).await.is_err());
    assert!(matches!(svc.undo_delete(other, token).await.unwrap_err(), AppError::NotFound(_)));
    let back = svc.undo_delete(mine, token).await.unwrap();
    assert_eq!((back.id, back.total, back.lines.len()), (t.id, d("1000"), 2));

    svc.create(other, me, receipt(&[("FOOD", "1")])).await.unwrap();
    assert_eq!(svc.purge_project(mine).await.unwrap(), 1);
    assert!(svc.lines(mine, day(1), day(31)).await.unwrap().is_empty());
    assert_eq!(svc.lines(other, day(1), day(31)).await.unwrap().len(), 1);
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing = kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
}
