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
use crate::ports::{AccountsLookup, CategoriesLookup, Deps};
use crate::service::TransactionService;

struct FakeAccounts;
struct FakeCategories;

#[async_trait::async_trait]
impl AccountsLookup for FakeAccounts {
    async fn exists(&self, _p: Uuid, code: &str) -> AppResult<bool> {
        Ok(code == "CARD")
    }
    async fn choices(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<Choice>> {
        Ok(vec![])
    }
    async fn titles(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(vec![])
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
}

fn service() -> TransactionService {
    TransactionService::new(
        Arc::new(InMemoryTransactionRepository::default()),
        Deps { accounts: Arc::new(FakeAccounts), categories: Arc::new(FakeCategories) },
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
