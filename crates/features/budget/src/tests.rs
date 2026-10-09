//! Тест use case «Бюджет»: фейковые источники, пересчёт валют, фильтр по участнику.

use std::str::FromStr;
use std::sync::Arc;

use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::{LimitRule, Node, Period, View};
use crate::ports::*;
use crate::service::BudgetService;

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}
fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

struct Fake {
    me: Uuid,
    partner: Uuid,
}

#[async_trait::async_trait]
impl CategoriesSource for Fake {
    async fn nodes(&self, _: Uuid, _: Lang) -> AppResult<Vec<Node>> {
        let n = |c: &str, p: Option<&str>, k: &str, depth| Node { code: c.into(), parent_code: p.map(Into::into), kind: k.into(), title: c.into(), depth, archived: false };
        Ok(vec![n("FOOD", None, "EXPENSE", 0), n("CAFE", Some("FOOD"), "EXPENSE", 1), n("SALARY", None, "INCOME", 0)])
    }
}

#[async_trait::async_trait]
impl FactsSource for Fake {
    async fn lines(&self, _: Uuid, _: NaiveDate, _: NaiveDate) -> AppResult<Vec<FactRec>> {
        let f = |acc: &str, k: &str, c: &str, a: &str, who| FactRec { date: d(10, 3), account_code: acc.into(), kind: k.into(), category_code: c.into(), amount: dec(a), created_by: who };
        Ok(vec![f("CARD", "EXPENSE", "CAFE", "300", self.me), f("USD_CARD", "EXPENSE", "FOOD", "10", self.partner), f("CARD", "INCOME", "SALARY", "50000", self.me)])
    }
    async fn turnover(&self, _: Uuid, _: NaiveDate) -> AppResult<Vec<(String, Decimal, Decimal)>> {
        Ok(vec![("CARD".into(), dec("50000"), dec("300"))])
    }
}

#[async_trait::async_trait]
impl PlansSource for Fake {
    async fn open(&self, _: Uuid, _: NaiveDate, _: NaiveDate, _: NaiveDate, _: Lang) -> AppResult<Vec<PlannedRec>> {
        Ok(vec![PlannedRec { plan_id: Uuid::nil(), date: d(10, 20), kind: "EXPENSE".into(), account_code: "CARD".into(), category_code: "FOOD".into(), amount: dec("1000"), title: "x".into(), status: "EXPECTED".into() }])
    }
}

#[async_trait::async_trait]
impl LimitsSource for Fake {
    async fn rules(&self, _: Uuid) -> AppResult<Vec<LimitRule>> {
        Ok(vec![LimitRule { category_code: "FOOD".into(), period: View::Month, amount: dec("2000"), valid_from: d(1, 1) }])
    }
}

#[async_trait::async_trait]
impl AccountsSource for Fake {
    async fn all(&self, _: Uuid, _: Lang) -> AppResult<Vec<AccountRec>> {
        Ok(vec![
            AccountRec { code: "CARD".into(), title: "Card".into(), currency: "THB".into(), opening: dec("1000"), archived: false },
            AccountRec { code: "USD_CARD".into(), title: "USD".into(), currency: "USD".into(), opening: dec("0"), archived: false },
        ])
    }
}

#[async_trait::async_trait]
impl RatesSource for Fake {
    async fn all(&self, _: Uuid) -> AppResult<Vec<(String, NaiveDate, Decimal)>> {
        Ok(vec![("USD".into(), d(10, 1), dec("33"))])
    }
}

fn service(f: Arc<Fake>) -> BudgetService {
    BudgetService::new(Deps { categories: f.clone(), facts: f.clone(), plans: f.clone(), limits: f.clone(), accounts: f.clone(), rates: f })
}

#[tokio::test]
async fn report_converts_currency_and_rolls_up() {
    let fake = Arc::new(Fake { me: Uuid::new_v4(), partner: Uuid::new_v4() });
    let svc = service(fake.clone());
    let period = Period::containing(View::Month, d(10, 9));
    let r = svc.report(Uuid::nil(), "THB", period, Lang::Ru, None, d(10, 9)).await.unwrap();
    // 300 THB + 10 USD × 33 = 630 факт, план 1000 → прогноз 1630 из 2000
    let food = r.expenses.iter().find(|x| x.code == "FOOD").unwrap();
    assert_eq!((food.fact, food.planned, food.forecast), (dec("630"), dec("1000"), dec("1630")));
    assert_eq!(food.available, Some(dec("370")));
    assert_eq!(r.income_fact, dec("50000"));
    assert_eq!(r.accounts[0].balance, dec("50700"));
    assert!(r.missing_rates.is_empty());
    // фильтр по участнику: факт только мои 300
    let mine = svc.report(Uuid::nil(), "THB", period, Lang::Ru, Some(fake.me), d(10, 9)).await.unwrap();
    assert_eq!(mine.expense_fact, dec("300"));
}
