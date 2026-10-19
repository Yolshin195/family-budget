//! Тесты use cases «Планирования»: in-memory хранилище + фейковые порты.

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::adapters::memory::InMemoryPlanRepository;
use crate::dto::PlanInput;
use crate::ports::{AccountsLookup, CategoriesLookup, Deps, PaidRef, PaymentsLookup, RatesLookup};
use crate::service::PlanningService;

#[derive(Default)]
struct FakePayments(Mutex<Vec<PaidRef>>);
struct FakeAccounts;
struct FakeCategories;

#[async_trait::async_trait]
impl PaymentsLookup for Arc<FakePayments> {
    async fn paid(&self, _p: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<PaidRef>> {
        Ok(self.0.lock().unwrap().iter().filter(|x| x.plan_date >= from && x.plan_date <= to).cloned().collect())
    }
}

#[async_trait::async_trait]
impl AccountsLookup for FakeAccounts {
    async fn exists(&self, _p: Uuid, code: &str) -> AppResult<bool> {
        Ok(code == "CARD" || code == "SAVINGS")
    }
    async fn choices(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<Choice>> {
        Ok(vec![])
    }
    async fn currencies(&self, _p: Uuid) -> AppResult<Vec<(String, String)>> {
        Ok(vec![("CARD".into(), "THB".into()), ("SAVINGS".into(), "RUB".into())])
    }
}

struct FakeRates;

#[async_trait::async_trait]
impl RatesLookup for FakeRates {
    async fn all(&self, _p: Uuid) -> AppResult<Vec<(String, NaiveDate, Decimal)>> {
        Ok(vec![("RUB".into(), NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), Decimal::from_str("0.4").unwrap())])
    }
}

#[async_trait::async_trait]
impl CategoriesLookup for FakeCategories {
    async fn kind_of(&self, _p: Uuid, code: &str) -> AppResult<Option<String>> {
        Ok(match code {
            "RENT" => Some("EXPENSE".into()),
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

fn setup() -> (PlanningService, Arc<FakePayments>) {
    let payments = Arc::new(FakePayments::default());
    let svc = PlanningService::new(
        Arc::new(InMemoryPlanRepository::default()),
        Deps { payments: Arc::new(payments.clone()), accounts: Arc::new(FakeAccounts), categories: Arc::new(FakeCategories), rates: Arc::new(FakeRates) },
    );
    (svc, payments)
}

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}

fn rent() -> PlanInput {
    PlanInput {
        name: "Аренда".into(),
        name_lang: "RU".into(),
        name_ru: None,
        name_en: Some("Rent".into()),
        name_th: None,
        tx_kind: "EXPENSE".into(),
        account_code: "card".into(),
        category_code: Some("rent".into()),
        to_account_code: None,
        amount: Decimal::from(15000),
        recurrence: "MONTHLY".into(),
        interval: 1,
        start_on: d(1, 5),
        end_on: None,
        active: true,
    }
}

#[tokio::test]
async fn calendar_statuses() {
    let (svc, payments) = setup();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let plan = svc.create(p, u, rent()).await.unwrap();
    // сентябрь оплачен, август пропущен, октябрь — сегодня 09.10 → просрочен (5-е прошло), ноябрь ожидается
    payments.0.lock().unwrap().push(PaidRef {
        plan_id: plan.id,
        plan_date: d(9, 5),
        transaction_id: Uuid::new_v4(),
        amount: Decimal::from_str("15200").unwrap(),
        currency: "THB".into(),
        paid_on: d(9, 6),
    });
    svc.skip(p, u, plan.id, d(8, 5)).await.unwrap();
    let occ = svc.occurrences(p, d(8, 1), d(11, 30), d(10, 9)).await.unwrap();
    let st: Vec<_> = occ.iter().map(|o| (o.date, o.status.as_str())).collect();
    assert_eq!(st, vec![(d(8, 5), "SKIPPED"), (d(9, 5), "PAID"), (d(10, 5), "OVERDUE"), (d(11, 5), "EXPECTED")]);
    assert_eq!(occ[1].paid_amount, Some(Decimal::from(15200)));
    assert_eq!(occ[1].shown_amount(), (Decimal::from(15200), "THB"), "у оплаченного — сумма и валюта оплаты");
    assert_eq!(occ[3].shown_amount(), (Decimal::from(15000), "THB"), "у ожидаемого — план в валюте счёта");
    // нельзя пропустить оплаченное и дату вне правила
    assert!(matches!(svc.skip(p, u, plan.id, d(9, 5)).await.unwrap_err(), AppError::Conflict(_)));
    assert!(matches!(svc.skip(p, u, plan.id, d(9, 6)).await.unwrap_err(), AppError::NotFound(_)));
    svc.unskip(p, plan.id, d(8, 5)).await.unwrap();
    assert_eq!(svc.occurrence(p, plan.id, d(8, 5), d(10, 9)).await.unwrap().status, "OVERDUE");
}

#[tokio::test]
async fn category_kind_and_refs() {
    let (svc, _) = setup();
    let mut bad = rent();
    bad.category_code = Some("SALARY".into());
    assert!(matches!(svc.create(Uuid::new_v4(), Uuid::new_v4(), bad).await.unwrap_err(), AppError::Validation(_)));
    let mut bad = rent();
    bad.account_code = "NOPE".into();
    assert!(matches!(svc.create(Uuid::new_v4(), Uuid::new_v4(), bad).await.unwrap_err(), AppError::Validation(_)));
}

/// Плановый перевод (в копилку каждый месяц): счёт зачисления вместо категории.
#[tokio::test]
async fn transfer_plan() {
    let (svc, _) = setup();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let savings = || PlanInput { tx_kind: "TRANSFER".into(), category_code: Some("ignored".into()), to_account_code: Some("savings".into()), ..rent() };
    let plan = svc.create(p, u, savings()).await.unwrap();
    assert_eq!((plan.category_code, plan.to_account_code.as_deref()), (None, Some("SAVINGS")));
    assert!(svc.create(p, u, PlanInput { to_account_code: None, ..savings() }).await.is_err(), "нужен счёт зачисления");
    assert!(svc.create(p, u, PlanInput { to_account_code: Some("card".into()), ..savings() }).await.is_err(), "тот же счёт");
    assert!(svc.create(p, u, PlanInput { to_account_code: Some("nope".into()), ..savings() }).await.is_err(), "счёт не найден");
    assert!(svc.create(p, u, PlanInput { category_code: None, ..rent() }).await.is_err(), "у расхода нужна категория");
}

#[tokio::test]
async fn inactive_plan_hides_open_occurrences() {
    let (svc, _) = setup();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let mut input = rent();
    input.active = false;
    svc.create(p, u, input).await.unwrap();
    assert!(svc.occurrences(p, d(1, 1), d(12, 31), d(10, 9)).await.unwrap().is_empty());
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing = kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
}

/// ТЗ, раздел 13: чужой проект → 404; удаление плана и пропуск отменяются; проект очищается.
#[tokio::test]
async fn foreign_project_undo_and_purge() {
    let (svc, _) = setup();
    let (mine, other, user) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let plan = svc.create(mine, user, rent()).await.unwrap();
    assert!(matches!(svc.get(other, plan.id).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.update(other, plan.id, rent()).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.delete_with_undo(other, plan.id).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.skip(other, user, plan.id, d(1, 5)).await.unwrap_err(), AppError::NotFound(_)));
    assert!(svc.occurrences(other, d(1, 1), d(12, 31), d(1, 1)).await.unwrap().is_empty());

    svc.skip(mine, user, plan.id, d(2, 5)).await.unwrap();
    let token = svc.delete_with_undo(mine, plan.id).await.unwrap();
    assert!(svc.get(mine, plan.id).await.is_err());
    svc.undo_delete(mine, user, token).await.unwrap();
    let feb = svc.occurrences(mine, d(2, 1), d(2, 28), d(1, 1)).await.unwrap();
    assert_eq!(feb[0].status, "SKIPPED", "пропуски вернулись вместе с планом");

    svc.set_active(mine, plan.id, false).await.unwrap();
    assert!(!svc.get(mine, plan.id).await.unwrap().active);
    assert_eq!(svc.purge_project(mine).await.unwrap(), 1);
    assert!(svc.list(mine).await.unwrap().is_empty());
}

/// Ц4: «Электричество» с 16.10 оплачено 16.10; дату начала поменяли на 01.10 —
/// оплата 16.10 должна остаться в календаре, а 01.10 не должно стать «просроченным».
#[tokio::test]
#[ignore = "Ц4: исправление ждёт утверждения разбора (docs/stage-0.md)"]
async fn rule_change_keeps_payment_history() {
    let (svc, payments) = setup();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let plan = svc.create(p, u, PlanInput { start_on: d(10, 16), amount: Decimal::from(2000), ..rent() }).await.unwrap();
    payments.0.lock().unwrap().push(PaidRef {
        plan_id: plan.id,
        plan_date: d(10, 16),
        transaction_id: Uuid::new_v4(),
        amount: Decimal::from_str("1539.87").unwrap(),
        currency: "THB".into(),
        paid_on: d(10, 17),
    });
    svc.update(p, plan.id, PlanInput { start_on: d(10, 1), amount: Decimal::from(2000), ..rent() }).await.unwrap();
    let occ = svc.occurrences(p, d(10, 1), d(10, 31), d(10, 19)).await.unwrap();
    let st: Vec<_> = occ.iter().map(|o| (o.date, o.status.as_str())).collect();
    assert_eq!(st, vec![(d(10, 16), "PAID")], "история оплат потерялась");
}
