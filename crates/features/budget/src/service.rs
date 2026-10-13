//! Use case фичи «Бюджет»: собрать отчёт за период из фактов, планов и лимитов.

use std::collections::{BTreeSet, HashMap};

use chrono::NaiveDate;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::{Amount, Period, Row, compute, rate_on};
use crate::ports::{Deps, PlannedRec};

pub struct BudgetService {
    deps: Deps,
}

#[derive(Debug, Clone)]
pub struct AccountBalance {
    pub code: String,
    pub title: String,
    pub currency: String,
    /// Остаток на конец периода в валюте счёта.
    pub balance: Decimal,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub period: Period,
    pub base_currency: String,
    pub expenses: Vec<Row>,
    pub incomes: Vec<Row>,
    pub expense_fact: Decimal,
    pub expense_planned: Decimal,
    pub income_fact: Decimal,
    pub income_planned: Decimal,
    pub accounts: Vec<AccountBalance>,
    /// Нерешённые вхождения планов за период (в базовой валюте), по дате.
    pub upcoming: Vec<PlannedRec>,
    /// Валюты без курса к базовой — их суммы посчитаны как 1:1.
    pub missing_rates: Vec<String>,
}

impl Report {
    pub fn net_forecast(&self) -> Decimal {
        self.income_fact + self.income_planned - self.expense_fact - self.expense_planned
    }
}

impl BudgetService {
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Отчёт за период. `member` — показать факт только операций этого участника (лимиты и планы общие).
    pub async fn report(
        &self,
        project_id: Uuid,
        base_currency: &str,
        period: Period,
        lang: Lang,
        member: Option<Uuid>,
        today: NaiveDate,
    ) -> AppResult<Report> {
        let d = &self.deps;
        let nodes = d.categories.nodes(project_id, lang).await?;
        let accounts = d.accounts.all(project_id, lang).await?;
        let rates = d.rates.all(project_id).await?;
        let limits = d.limits.rules(project_id).await?;
        let facts = d.facts.lines(project_id, period.from, period.to).await?;
        let planned = d.plans.open(project_id, period.from, period.to, today, lang).await?;

        let currency_of: HashMap<&str, &str> = accounts.iter().map(|a| (a.code.as_str(), a.currency.as_str())).collect();
        let mut missing = BTreeSet::new();
        let mut to_base = |account: &str, amount: Decimal, date: NaiveDate| -> Decimal {
            let cur = currency_of.get(account).copied().unwrap_or(base_currency);
            match rate_on(&rates, cur, base_currency, date) {
                Some(r) => (amount * r).round_dp(2),
                None => {
                    missing.insert(cur.to_string());
                    amount
                }
            }
        };

        let (mut exp_f, mut exp_p, mut inc_f, mut inc_p) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        let mut fact_amounts = Vec::new();
        for f in facts.iter().filter(|f| member.is_none_or(|m| f.created_by == m)) {
            let a = to_base(&f.account_code, f.amount, f.date);
            if f.kind == "EXPENSE" { exp_f += a } else { inc_f += a }
            fact_amounts.push(Amount { category_code: f.category_code.clone(), amount: a });
        }
        let mut plan_amounts = Vec::new();
        let mut upcoming = Vec::new();
        for p in planned {
            let a = to_base(&p.account_code, p.amount, p.date);
            if p.kind == "EXPENSE" { exp_p += a } else { inc_p += a }
            plan_amounts.push(Amount { category_code: p.category_code.clone(), amount: a });
            upcoming.push(PlannedRec { amount: a, ..p });
        }
        upcoming.sort_by_key(|p| p.date);
        drop(to_base);

        let rows = compute(&nodes, &fact_amounts, &plan_amounts, &limits, &period);
        // Архивные категории без движения не показываем.
        let visible = |r: &Row| {
            let archived = nodes.iter().any(|n| n.code == r.code && n.archived);
            !archived || !r.forecast.is_zero()
        };
        let (expenses, incomes): (Vec<Row>, Vec<Row>) = rows.into_iter().filter(visible).partition(|r| r.kind == "EXPENSE");

        let turnover: HashMap<String, (Decimal, Decimal)> =
            d.facts.turnover(project_id, period.to).await?.into_iter().map(|(c, i, e)| (c, (i, e))).collect();
        let accounts = accounts
            .into_iter()
            .filter(|a| !a.archived)
            .map(|a| {
                let (inc, exp) = turnover.get(&a.code).copied().unwrap_or_default();
                AccountBalance { balance: a.opening + inc - exp, code: a.code, title: a.title, currency: a.currency }
            })
            .collect();

        Ok(Report {
            period,
            base_currency: base_currency.to_string(),
            expenses,
            incomes,
            expense_fact: exp_f,
            expense_planned: exp_p,
            income_fact: inc_f,
            income_planned: inc_p,
            accounts,
            upcoming,
            missing_rates: missing.into_iter().collect(),
        })
    }
}

/// Лимит категории за месяц даты — для предупреждения в форме расхода (суммы в базовой валюте).
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryLimit {
    pub code: String,
    pub limit: Option<Decimal>,
    pub forecast: Decimal,
    pub available: Option<Decimal>,
}

impl BudgetService {
    /// Остатки лимитов по всем категориям за месяц, содержащий `date` (все участники).
    pub async fn month_limits(&self, project_id: Uuid, base_currency: &str, date: NaiveDate, lang: Lang) -> AppResult<Vec<CategoryLimit>> {
        let today = chrono::Local::now().date_naive();
        let period = Period::containing(crate::domain::View::Month, date);
        let r = self.report(project_id, base_currency, period, lang, None, today).await?;
        Ok(r.expenses
            .into_iter()
            .chain(r.incomes)
            .map(|x| CategoryLimit { code: x.code, limit: x.limit, forecast: x.forecast, available: x.available })
            .collect())
    }

    /// Курсы валют к базовой на дату (ближайший предыдущий) — для пересчёта суммы в форме.
    pub async fn rates_on(&self, project_id: Uuid, base_currency: &str, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        let rates = self.deps.rates.all(project_id).await?;
        let mut currencies: Vec<String> = rates.iter().map(|r| r.0.clone()).collect();
        currencies.sort();
        currencies.dedup();
        Ok(currencies.into_iter().filter_map(|c| rate_on(&rates, &c, base_currency, date).map(|r| (c, r))).collect())
    }
}
