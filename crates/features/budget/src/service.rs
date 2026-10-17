//! Use case фичи «Бюджет»: собрать отчёт за период из фактов, планов и лимитов.

use std::collections::{BTreeSet, HashMap};

use chrono::{Datelike, Days, NaiveDate};
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
    /// Прогноз остатка на конец периода: `balance` плюс нерешённые планы по этому счёту — с начала
    /// текущего месяца (или периода, если он раньше) по конец периода.
    pub forecast: Decimal,
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
    /// Курсовая разница за период в базовой валюте: «−» — потеряно на обмене, «+» — выиграно.
    /// В доходы и расходы не входит.
    pub fx_result: Decimal,
    /// Нерешённые вхождения планов за период (в базовой валюте), по дате.
    pub upcoming: Vec<PlannedRec>,
    /// Валюты без курса к базовой — их суммы посчитаны как 1:1.
    pub missing_rates: Vec<String>,
}

impl Report {
    pub fn net_forecast(&self) -> Decimal {
        self.income_fact + self.income_planned - self.expense_fact - self.expense_planned
    }

    pub fn net_fact(&self) -> Decimal {
        self.income_fact - self.expense_fact
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
        // Для прогноза остатков нужны и нерешённые планы между текущим месяцем и началом периода.
        let open_from = period.from.min(today.with_day(1).unwrap_or(today)).max(period.to - Days::new(3 * 365));
        let planned = d.plans.open(project_id, open_from, period.to, today, lang).await?;

        let exchange = d.facts.exchange(project_id, period.from, period.to).await?;

        let currency_of: HashMap<&str, &str> = accounts.iter().map(|a| (a.code.as_str(), a.currency.as_str())).collect();
        let mut missing = BTreeSet::new();
        // Пересчёт между валютами через базовую по курсам на дату; без курса — 1:1.
        let mut convert = |from: &str, to: &str, amount: Decimal, date: NaiveDate| -> Decimal {
            if from == to {
                return amount.round_dp(2);
            }
            match (rate_on(&rates, from, base_currency, date), rate_on(&rates, to, base_currency, date)) {
                (Some(a), Some(b)) if !b.is_zero() => (amount * a / b).round_dp(2),
                (a, b) => {
                    missing.extend(a.is_none().then(|| from.to_string()));
                    missing.extend(b.is_none().then(|| to.to_string()));
                    amount
                }
            }
        };

        let mine = |who: Uuid| member.is_none_or(|m| who == m);

        let (mut exp_f, mut exp_p, mut inc_f, mut inc_p) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        let mut fact_amounts = Vec::new();
        for f in facts.iter().filter(|f| mine(f.created_by)) {
            let a = convert(&f.currency, base_currency, f.amount, f.date);
            if f.kind == "EXPENSE" { exp_f += a } else { inc_f += a }
            fact_amounts.push(Amount { category_code: f.category_code.clone(), amount: a });
        }
        // Обмен валют: проводки «Обмен валют» в базовой валюте. Отдали больше, чем получили, — потеря.
        let fx_result = -exchange.iter().filter(|x| mine(x.created_by)).map(|x| convert(&x.currency, base_currency, x.amount, x.date)).sum::<Decimal>();
        let mut plan_amounts = Vec::new();
        let mut upcoming = Vec::new();
        // Изменение остатков счетов нерешёнными планами (в валютах счетов).
        let mut pending: HashMap<String, Decimal> = HashMap::new();
        for p in planned {
            let cur = currency_of.get(p.account_code.as_str()).copied().unwrap_or(base_currency);
            let sign = if p.kind == "INCOME" { Decimal::ONE } else { -Decimal::ONE };
            *pending.entry(p.account_code.clone()).or_default() += sign * p.amount;
            if let Some(to) = p.to_account_code.as_deref().filter(|_| p.kind == "TRANSFER") {
                let to_cur = currency_of.get(to).copied().unwrap_or(base_currency);
                *pending.entry(to.to_string()).or_default() += convert(cur, to_cur, p.amount, p.date);
            }
            if p.date < period.from {
                continue;
            }
            let a = convert(cur, base_currency, p.amount, p.date);
            // Плановый перевод не меняет доходы и расходы — он только виден в «Предстоящих».
            if let Some(code) = p.category_code.clone().filter(|_| p.kind != "TRANSFER") {
                if p.kind == "EXPENSE" { exp_p += a } else { inc_p += a }
                plan_amounts.push(Amount { category_code: code, amount: a });
            }
            upcoming.push(PlannedRec { amount: a, ..p });
        }
        upcoming.sort_by_key(|p| p.date);

        let rows = compute(&nodes, &fact_amounts, &plan_amounts, &limits, &period);
        // Архивные категории без движения не показываем.
        let visible = |r: &Row| {
            let archived = nodes.iter().any(|n| n.code == r.code && n.archived);
            !archived || !r.forecast.is_zero()
        };
        let (expenses, incomes): (Vec<Row>, Vec<Row>) = rows.into_iter().filter(visible).partition(|r| r.kind == "EXPENSE");

        // Остаток счёта = сумма его проводок (начальный остаток — тоже проводка).
        let balances: HashMap<String, Decimal> = d.facts.balances(project_id, period.to).await?.into_iter().collect();
        let accounts = accounts
            .into_iter()
            .filter(|a| !a.archived)
            .map(|a| {
                let balance = balances.get(&a.code).copied().unwrap_or_default();
                let forecast = balance + pending.get(&a.code).copied().unwrap_or_default();
                AccountBalance { balance, forecast, code: a.code, title: a.title, currency: a.currency }
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
            fx_result,
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
