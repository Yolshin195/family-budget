//! Связка фичи `mcp`: операции (transactions), бюджет (budget), плановые платежи (planning),
//! проверка токенов и журнал (projects). Типы фич переводятся в типы MCP здесь — фичи друг о друге
//! не знают.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::auth::RequestMeta;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::pagination::Pagination;
use rust_decimal::Decimal;
use uuid::Uuid;

use ::mcp::{BudgetLine, BudgetReport, CategoryInfo, Occurrence, QuickInfo, Tx, TxDraft, TxFilter, TxLine};

pub struct LedgerAdapter(pub Arc<::transactions::Service>);

fn tx_from(d: ::transactions::dto::TransactionDto) -> Tx {
    Tx {
        id: d.id,
        occurred_on: d.occurred_on,
        kind: d.tx_kind,
        account_code: d.account_code,
        to_account_code: d.to_account_code,
        refund: d.refund,
        lines: d.lines.into_iter().map(|l| TxLine { category_code: l.category_code, amount: l.amount, note: l.note }).collect(),
        total: d.total,
        currency: d.currency,
        to_amount: d.to_amount,
        to_currency: d.to_currency,
        fee_category_code: d.fee_category_code,
        fee_amount: d.fee_amount,
        description: d.description,
        description_lang: Lang::parse(&d.description_lang).unwrap_or_default(),
        description_ru: d.description_ru,
        description_en: d.description_en,
        description_th: d.description_th,
        created_by: d.created_by,
        plan_id: d.plan_id,
        plan_date: d.plan_date,
    }
}

fn input_from(d: TxDraft) -> ::transactions::dto::TransactionInput {
    ::transactions::dto::TransactionInput {
        occurred_on: d.occurred_on,
        tx_kind: d.kind,
        account_code: d.account_code,
        refund: d.refund,
        lines: d.lines.into_iter().map(|l| ::transactions::dto::LineDto { category_code: l.category_code, amount: l.amount, note: l.note }).collect(),
        to_account_code: d.to_account_code,
        amount: d.amount,
        to_amount: d.to_amount,
        fee_category_code: d.fee_category_code,
        fee_amount: d.fee_amount,
        description: d.description,
        description_lang: d.description_lang.upper().to_string(),
        description_ru: d.description_ru,
        description_en: d.description_en,
        description_th: d.description_th,
        plan_id: d.plan_id,
        plan_date: d.plan_date,
    }
}

#[async_trait]
impl ::mcp::Ledger for LedgerAdapter {
    async fn quick(&self, project_id: Uuid, user_id: Uuid, base_currency: &str, lang: Lang, date: NaiveDate) -> AppResult<QuickInfo> {
        let q = self.0.quick_defaults(project_id, user_id, base_currency, lang, date).await?;
        Ok(QuickInfo {
            default_account: q.default_account,
            accounts: q.accounts.into_iter().map(|a| ::mcp::AccountInfo { code: a.code, title: a.title, currency: a.currency, symbol: a.symbol }).collect(),
            categories: q
                .categories
                .into_iter()
                .map(|c| CategoryInfo { code: c.code, parent_code: c.parent_code, kind: c.kind, title: c.title, depth: c.depth, search: c.search, limit: c.limit, available: c.available })
                .collect(),
            frequent_expense: q.frequent_expense,
            frequent_income: q.frequent_income,
        })
    }

    async fn list(&self, project_id: Uuid, filter: TxFilter, limit: i64, offset: i64) -> AppResult<(Vec<Tx>, i64)> {
        let size = limit.max(1);
        let query = ::transactions::dto::ListQuery { from: filter.from, to: filter.to, account_code: filter.account_code, tx_kind: filter.kind, created_by: None, untranslated: None };
        let page = self.0.list(project_id, query, Pagination { page: offset / size, size }).await?;
        Ok((page.content.into_iter().map(tx_from).collect(), page.page.total_elements))
    }

    async fn get(&self, project_id: Uuid, id: Uuid) -> AppResult<Tx> {
        Ok(tx_from(self.0.get(project_id, id).await?))
    }

    async fn create(&self, project_id: Uuid, user_id: Uuid, draft: TxDraft) -> AppResult<Tx> {
        Ok(tx_from(self.0.create(project_id, user_id, input_from(draft)).await?))
    }

    async fn update(&self, project_id: Uuid, id: Uuid, draft: TxDraft) -> AppResult<Tx> {
        Ok(tx_from(self.0.update(project_id, id, input_from(draft)).await?))
    }

    async fn balances(&self, project_id: Uuid, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        self.0.balances(project_id, date).await
    }
}

pub struct BudgetAdapter(pub Arc<::budget::Service>);

#[async_trait]
impl ::mcp::Budget for BudgetAdapter {
    async fn report(&self, project_id: Uuid, base_currency: &str, view: &str, date: NaiveDate, lang: Lang) -> AppResult<BudgetReport> {
        let today = chrono::Local::now().date_naive();
        let period = ::budget::Period::containing(::budget::View::parse(view), date);
        let r = self.0.report(project_id, base_currency, period, lang, None, today).await?;
        let lines = r
            .expenses
            .into_iter()
            .chain(r.incomes)
            .map(|x| BudgetLine {
                code: x.code,
                parent_code: x.parent_code,
                title: x.title,
                kind: x.kind,
                depth: x.depth,
                limit: x.limit,
                fact: x.fact,
                planned: x.planned,
                forecast: x.forecast,
                available: x.available,
                level: x.level.code().into(),
            })
            .collect();
        Ok(BudgetReport {
            from: r.period.from,
            to: r.period.to,
            base_currency: r.base_currency,
            expense_fact: r.expense_fact,
            expense_planned: r.expense_planned,
            income_fact: r.income_fact,
            income_planned: r.income_planned,
            lines,
            missing_rates: r.missing_rates,
        })
    }
}

pub struct PlansAdapter(pub Arc<::planning::Service>);

#[async_trait]
impl ::mcp::Plans for PlansAdapter {
    async fn occurrences(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate, today: NaiveDate, lang: Lang) -> AppResult<Vec<Occurrence>> {
        Ok(self
            .0
            .occurrences(project_id, from, to, today)
            .await?
            .into_iter()
            .map(|o| Occurrence {
                plan_id: o.plan.id,
                name: o.plan.title(lang).to_string(),
                date: o.date,
                status: o.status,
                kind: o.plan.tx_kind.clone(),
                amount: o.paid_amount.unwrap_or(o.plan.amount),
                account_code: o.plan.account_code.clone(),
                category_code: o.plan.category_code.clone(),
                to_account_code: o.plan.to_account_code.clone(),
                transaction_id: o.transaction_id,
            })
            .collect())
    }

    async fn skip(&self, project_id: Uuid, user_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<()> {
        self.0.skip(project_id, user_id, plan_id, date).await
    }
}

pub struct AccountsAdapter(pub Arc<::projects::Service>);

#[async_trait]
impl ::mcp::Accounts for AccountsAdapter {
    async fn verify_access_token(&self, token: &str, audience: &str) -> AppResult<Option<(Uuid, String)>> {
        self.0.verify_access_token(token, audience).await
    }
    async fn issuer(&self) -> Option<String> {
        self.0.token_issuer().await
    }
    async fn is_guest(&self, user_id: Uuid) -> AppResult<bool> {
        self.0.is_guest(user_id).await
    }
    async fn record(&self, user_id: Uuid, project_id: Option<Uuid>, action: &str, meta: &RequestMeta, details: &[(&str, String)]) {
        self.0.record_event(user_id, project_id, action, meta, details).await
    }
}
