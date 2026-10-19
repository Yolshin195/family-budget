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

/// Проект, в котором у фейковых источников есть данные (остальные проекты пусты).
const P: Uuid = Uuid::nil();

#[async_trait::async_trait]
impl CategoriesSource for Fake {
    async fn nodes(&self, p: Uuid, _: Lang) -> AppResult<Vec<Node>> {
        if p != P {
            return Ok(vec![]);
        }
        let n = |c: &str, p: Option<&str>, k: &str, depth| Node { code: c.into(), parent_code: p.map(Into::into), kind: k.into(), title: c.into(), depth, archived: false };
        Ok(vec![n("FOOD", None, "EXPENSE", 0), n("CAFE", Some("FOOD"), "EXPENSE", 1), n("SALARY", None, "INCOME", 0)])
    }
}

#[async_trait::async_trait]
impl FactsSource for Fake {
    async fn lines(&self, p: Uuid, _: NaiveDate, _: NaiveDate) -> AppResult<Vec<FactRec>> {
        if p != P {
            return Ok(vec![]);
        }
        let f = |cur: &str, k: &str, c: &str, a: &str, who| FactRec { date: d(10, 3), currency: cur.into(), kind: k.into(), category_code: c.into(), amount: dec(a), created_by: who };
        Ok(vec![f("THB", "EXPENSE", "CAFE", "300", self.me), f("USD", "EXPENSE", "FOOD", "10", self.partner), f("THB", "INCOME", "SALARY", "50000", self.me)])
    }
    async fn balances(&self, p: Uuid, _: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        if p != P {
            return Ok(vec![]);
        }
        // начальный остаток 1000 + 50000 − 300 − перевод 3300 на USD-карту; USD: +100 − 10
        Ok(vec![("CARD".into(), dec("47400")), ("USD_CARD".into(), dec("90"))])
    }
    async fn exchange(&self, p: Uuid, _: NaiveDate, _: NaiveDate) -> AppResult<Vec<ExchangeRec>> {
        if p != P {
            return Ok(vec![]);
        }
        // Обмен 3 400 ฿ → 100 $: по справочнику 100 $ = 3 300 ฿, потеряно 100 ฿.
        let x = |cur: &str, a: &str| ExchangeRec { date: d(10, 3), currency: cur.into(), amount: dec(a), created_by: self.me };
        Ok(vec![x("THB", "3400"), x("USD", "-100")])
    }
}

#[async_trait::async_trait]
impl PlansSource for Fake {
    async fn open(&self, p: Uuid, _: NaiveDate, _: NaiveDate, _: NaiveDate, _: Lang) -> AppResult<Vec<PlannedRec>> {
        if p != P {
            return Ok(vec![]);
        }
        let plan = |kind: &str, cat: Option<&str>, amount: &str| PlannedRec {
            plan_id: Uuid::nil(),
            date: d(10, 20),
            kind: kind.into(),
            account_code: "CARD".into(),
            category_code: cat.map(Into::into),
            to_account_code: (kind == "TRANSFER").then(|| "USD_CARD".into()),
            amount: dec(amount),
            title: "x".into(),
            status: "EXPECTED".into(),
        };
        Ok(vec![plan("EXPENSE", Some("FOOD"), "1000"), plan("TRANSFER", None, "5000")])
    }
}

#[async_trait::async_trait]
impl LimitsSource for Fake {
    async fn rules(&self, p: Uuid) -> AppResult<Vec<LimitRule>> {
        if p != P {
            return Ok(vec![]);
        }
        Ok(vec![LimitRule { category_code: "FOOD".into(), period: View::Month, amount: Some(dec("2000")), valid_from: d(1, 1) }])
    }
}

#[async_trait::async_trait]
impl AccountsSource for Fake {
    async fn all(&self, p: Uuid, _: Lang) -> AppResult<Vec<AccountRec>> {
        if p != P {
            return Ok(vec![]);
        }
        Ok(vec![
            AccountRec { code: "CARD".into(), title: "Card".into(), currency: "THB".into(), archived: false },
            AccountRec { code: "USD_CARD".into(), title: "USD".into(), currency: "USD".into(), archived: false },
        ])
    }
}

#[async_trait::async_trait]
impl RatesSource for Fake {
    async fn all(&self, p: Uuid) -> AppResult<Vec<(String, NaiveDate, Decimal)>> {
        if p != P {
            return Ok(vec![]);
        }
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
    assert_eq!((r.accounts[0].balance, r.accounts[1].balance), (dec("47400"), dec("90")), "остатки — суммы проводок");
    // Прогноз: CARD − 1 000 (расход) − 5 000 (перевод); USD_CARD + 5 000 ฿ / 33 = 151,52 $.
    assert_eq!((r.accounts[0].forecast, r.accounts[1].forecast), (dec("41400"), dec("241.52")), "прогноз остатков — с планами");
    assert_eq!(r.net_fact(), dec("49370"), "итог по факту — без планов");
    assert_eq!(r.expense_planned, dec("1000"), "плановый перевод не входит в расходы");
    assert_eq!(r.upcoming.len(), 2, "но виден в «Предстоящих»");
    assert_eq!(r.fx_result, dec("-100"), "курсовая разница — отдельно от расходов");
    assert_eq!(r.expense_fact, dec("630"));
    assert!(r.missing_rates.is_empty());
    // фильтр по участнику: факт только мои 300
    let mine = svc.report(Uuid::nil(), "THB", period, Lang::Ru, Some(fake.me), d(10, 9)).await.unwrap();
    assert_eq!(mine.expense_fact, dec("300"));
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing = kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
}

/// ТЗ, раздел 13: отчёт строится только из данных своего проекта.
#[tokio::test]
async fn foreign_project_sees_empty_budget() {
    let fake = Arc::new(Fake { me: Uuid::new_v4(), partner: Uuid::new_v4() });
    let svc = service(fake);
    let period = Period::containing(View::Month, d(10, 9));
    let other = svc.report(Uuid::new_v4(), "THB", period, Lang::Ru, None, d(10, 9)).await.unwrap();
    assert!(other.expenses.is_empty() && other.accounts.is_empty() && other.upcoming.is_empty());
    assert_eq!(other.expense_fact, Decimal::ZERO);
}

/// Остатки лимитов за месяц и курсы — для предупреждения в форме быстрого расхода.
#[tokio::test]
async fn month_limits_and_rates_for_quick_expense() {
    let fake = Arc::new(Fake { me: Uuid::new_v4(), partner: Uuid::new_v4() });
    let svc = service(fake);
    let limits = svc.month_limits(P, "THB", d(10, 9), Lang::Ru).await.unwrap();
    let cafe = limits.iter().find(|l| l.code == "CAFE").unwrap();
    assert_eq!(cafe.available, Some(dec("370")), "у «Кафе» своего лимита нет — доступно по «Еде»");
    assert_eq!(svc.rates_on(P, "THB", d(10, 9)).await.unwrap(), vec![("USD".to_string(), dec("33"))]);
}

/// Ц3: курса USD нет — 10 $ не должны войти в расходы как 10 ฿.
struct NoRates;

#[async_trait::async_trait]
impl RatesSource for NoRates {
    async fn all(&self, _: Uuid) -> AppResult<Vec<(String, NaiveDate, Decimal)>> {
        Ok(vec![])
    }
}

#[tokio::test]
#[ignore = "Ц3: исправление ждёт утверждения разбора (docs/stage-0.md)"]
async fn amount_without_rate_is_not_summed() {
    let f = Arc::new(Fake { me: Uuid::new_v4(), partner: Uuid::new_v4() });
    let svc = BudgetService::new(Deps { categories: f.clone(), facts: f.clone(), plans: f.clone(), limits: f.clone(), accounts: f.clone(), rates: Arc::new(NoRates) });
    let r = svc.report(P, "THB", Period::containing(View::Month, d(10, 9)), Lang::Ru, None, d(10, 9)).await.unwrap();
    assert_eq!(r.missing_rates, vec!["USD".to_string()]);
    assert_eq!(r.expense_fact, dec("300"), "10 $ без курса сложены с батами 1:1");
}
