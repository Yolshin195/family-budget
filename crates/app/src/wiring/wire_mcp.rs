//! Связка фичи `mcp`: операции (transactions), бюджет (budget), плановые операции (planning),
//! справочники (currencies, accounts, categories), курсы (rates), лимиты (limits), проверка токенов и
//! журнал (projects). Типы фич переводятся в типы MCP здесь — фичи друг о друге не знают.
//!
//! Архивировать через MCP нельзя: при изменении справочника признак `archived` берётся из текущей
//! записи, а не из того, что пришло от ассистента.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use kernel::auth::RequestMeta;
use kernel::error::AppResult;
use kernel::i18n::Lang;
use kernel::pagination::Pagination;
use rust_decimal::Decimal;
use uuid::Uuid;

use ::mcp::{
    Account, BudgetLine, BudgetReport, Category, CategoryInfo, Currency, Limit, Name, Occurrence, Place, Plan, PlanDraft, QuickInfo, Rate, Tx, TxDraft, TxFilter, TxItem,
};

pub struct LedgerAdapter(pub Arc<::transactions::Service>);

fn tx_from(d: ::transactions::dto::TransactionDto) -> Tx {
    Tx {
        id: d.id,
        occurred_on: d.occurred_on,
        kind: d.tx_kind,
        account_code: d.account_code,
        to_account_code: d.to_account_code,
        refund: d.refund,
        items: d
            .items
            .into_iter()
            .map(|x| TxItem {
                id: Some(x.id),
                category_code: Some(x.category_code),
                name: x.name,
                quantity: Some(x.quantity),
                unit: Some(x.unit),
                price: x.price_per_unit,
                discount: Some(x.discount),
                amount: Some(x.amount),
                note: x.note,
            })
            .collect(),
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
        place_code: d.place_code,
    }
}

fn input_from(d: TxDraft) -> ::transactions::dto::TransactionInput {
    ::transactions::dto::TransactionInput {
        occurred_on: d.occurred_on,
        tx_kind: d.kind,
        account_code: d.account_code,
        refund: d.refund,
        items: d
            .items
            .into_iter()
            .map(|x| ::transactions::dto::ItemInput {
                id: x.id,
                category_code: x.category_code,
                name: x.name,
                quantity: x.quantity,
                unit: x.unit,
                price_per_unit: x.price,
                discount: x.discount,
                amount: x.amount,
                note: x.note,
            })
            .collect(),
        replace_items: d.replace_items,
        remove_item_ids: d.remove_item_ids,
        discount_total: d.discount_total,
        lines: Vec::new(),
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
        place_code: d.place_code,
    }
}

#[async_trait]
impl ::mcp::Ledger for LedgerAdapter {
    async fn quick(&self, project_id: Uuid, user_id: Uuid, base_currency: &str, lang: Lang, date: NaiveDate) -> AppResult<QuickInfo> {
        let q = self.0.quick_defaults(project_id, user_id, base_currency, lang, date).await?;
        Ok(QuickInfo {
            default_account: q.default_account,
            accounts: q.accounts.into_iter().map(|a| ::mcp::AccountInfo { has_rate: a.rate_to_base.is_some(), code: a.code, title: a.title, currency: a.currency, symbol: a.symbol }).collect(),
            categories: q
                .categories
                .into_iter()
                .map(|c| CategoryInfo { code: c.code, parent_code: c.parent_code, kind: c.kind, title: c.title, depth: c.depth, search: c.search, limit: c.limit, available: c.available })
                .collect(),
            frequent_expense: q.frequent_expense,
            frequent_income: q.frequent_income,
            places: q
                .places
                .into_iter()
                .map(|p| ::mcp::PlaceInfo { code: p.code, title: p.title, search: p.search, category_code: p.category, account_code: p.account })
                .collect(),
            frequent_places: q.frequent_places,
        })
    }

    async fn list(&self, project_id: Uuid, filter: TxFilter, limit: i64, offset: i64) -> AppResult<(Vec<Tx>, i64)> {
        let size = limit.max(1);
        let query = ::transactions::dto::ListQuery { from: filter.from, to: filter.to, account_code: filter.account_code, tx_kind: filter.kind, created_by: None, untranslated: None, place_code: filter.place_code };
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
                no_rate: x.no_rate,
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

    async fn list(&self, project_id: Uuid) -> AppResult<Vec<Plan>> {
        Ok(self.0.list(project_id).await?.into_iter().map(plan_from).collect())
    }

    async fn create(&self, project_id: Uuid, user_id: Uuid, draft: &PlanDraft) -> AppResult<Plan> {
        Ok(plan_from(self.0.create(project_id, user_id, plan_input(draft)).await?))
    }

    async fn update(&self, project_id: Uuid, id: Uuid, draft: &PlanDraft) -> AppResult<Plan> {
        Ok(plan_from(self.0.update(project_id, id, plan_input(draft)).await?))
    }
}

fn name_from(text: String, lang: &str, ru: Option<String>, en: Option<String>, th: Option<String>) -> Name {
    Name { text, lang: Lang::parse(lang).unwrap_or_default(), ru, en, th }
}

fn plan_from(p: ::planning::PlanDto) -> Plan {
    Plan {
        id: p.id,
        draft: PlanDraft {
            name: name_from(p.name, &p.name_lang, p.name_ru, p.name_en, p.name_th),
            kind: p.tx_kind,
            account_code: p.account_code,
            category_code: p.category_code,
            to_account_code: p.to_account_code,
            amount: p.amount,
            recurrence: p.recurrence,
            interval: p.interval,
            start_on: p.start_on,
            end_on: p.end_on,
            active: p.active,
        },
    }
}

fn plan_input(d: &PlanDraft) -> ::planning::dto::PlanInput {
    ::planning::dto::PlanInput {
        name: d.name.text.clone(),
        name_lang: d.name.lang.upper().into(),
        name_ru: d.name.ru.clone(),
        name_en: d.name.en.clone(),
        name_th: d.name.th.clone(),
        tx_kind: d.kind.clone(),
        account_code: d.account_code.clone(),
        category_code: d.category_code.clone(),
        to_account_code: d.to_account_code.clone(),
        amount: d.amount,
        recurrence: d.recurrence.clone(),
        interval: d.interval,
        start_on: d.start_on,
        end_on: d.end_on,
        active: d.active,
    }
}

/// Справочники для MCP: валюты, счета (и начальный остаток), категории.
pub struct DirectoriesAdapter {
    pub currencies: Arc<::currencies::Service>,
    pub accounts: Arc<::accounts::Service>,
    pub categories: Arc<::categories::Service>,
    pub places: Arc<::places::Service>,
}

fn place_from(d: ::places::dto::PlaceDto) -> Place {
    Place {
        code: d.code,
        name: d.name,
        aliases: d.aliases,
        category_code: d.default_category_code,
        account_code: d.default_account_code,
        kind: d.place_kind,
        location: d.location,
        archived: d.archived,
    }
}

fn place_fields(p: &Place) -> ::places::dto::PlaceFields {
    ::places::dto::PlaceFields {
        name: p.name.clone(),
        aliases: p.aliases.clone(),
        default_category_code: p.category_code.clone(),
        default_account_code: p.account_code.clone(),
        place_kind: p.kind.clone(),
        location: p.location.clone(),
    }
}

fn currency_from(d: ::currencies::dto::CurrencyDto) -> Currency {
    Currency { code: d.code, name: name_from(d.name, &d.name_lang, d.name_ru, d.name_en, d.name_th), symbol: d.symbol, minor_units: d.minor_units, archived: d.archived }
}

fn account_from(d: ::accounts::dto::AccountDto) -> Account {
    Account {
        code: d.code,
        name: name_from(d.name, &d.name_lang, d.name_ru, d.name_en, d.name_th),
        kind: d.account_kind,
        currency: d.currency_code,
        holder_user_id: d.holder_user_id,
        archived: d.archived,
    }
}

fn category_from(d: ::categories::dto::CategoryDto) -> Category {
    Category {
        code: d.code,
        name: name_from(d.name, &d.name_lang, d.name_ru, d.name_en, d.name_th),
        kind: d.category_kind,
        parent_code: d.parent_code,
        sort_order: d.sort_order,
        archived: d.archived,
    }
}

#[async_trait]
impl ::mcp::Directories for DirectoriesAdapter {
    async fn currencies(&self, project_id: Uuid) -> AppResult<Vec<Currency>> {
        Ok(self.currencies.all(project_id, true).await?.into_iter().map(currency_from).collect())
    }

    async fn create_currency(&self, project_id: Uuid, c: &Currency) -> AppResult<Currency> {
        let dto = ::currencies::dto::CreateCurrencyDto {
            code: c.code.clone(),
            name: c.name.text.clone(),
            name_lang: c.name.lang.upper().into(),
            name_ru: c.name.ru.clone(),
            name_en: c.name.en.clone(),
            name_th: c.name.th.clone(),
            symbol: c.symbol.clone(),
            minor_units: c.minor_units,
        };
        Ok(currency_from(self.currencies.create(project_id, dto).await?))
    }

    async fn update_currency(&self, project_id: Uuid, c: &Currency) -> AppResult<Currency> {
        let current = self.currencies.get(project_id, &c.code).await?;
        let dto = ::currencies::dto::UpdateCurrencyDto {
            name: c.name.text.clone(),
            archived: current.archived,
            name_lang: c.name.lang.upper().into(),
            name_ru: c.name.ru.clone(),
            name_en: c.name.en.clone(),
            name_th: c.name.th.clone(),
            symbol: c.symbol.clone(),
            minor_units: c.minor_units,
        };
        Ok(currency_from(self.currencies.update(project_id, &c.code, dto).await?))
    }

    async fn accounts(&self, project_id: Uuid) -> AppResult<Vec<Account>> {
        Ok(self.accounts.all(project_id, true).await?.into_iter().map(account_from).collect())
    }

    async fn create_account(&self, project_id: Uuid, a: &Account) -> AppResult<Account> {
        let dto = ::accounts::dto::CreateAccountDto {
            code: a.code.clone(),
            name: a.name.text.clone(),
            name_lang: a.name.lang.upper().into(),
            name_ru: a.name.ru.clone(),
            name_en: a.name.en.clone(),
            name_th: a.name.th.clone(),
            account_kind: a.kind.clone(),
            currency_code: a.currency.clone(),
            holder_user_id: a.holder_user_id,
        };
        Ok(account_from(self.accounts.create(project_id, dto).await?))
    }

    async fn update_account(&self, project_id: Uuid, a: &Account) -> AppResult<Account> {
        let current = self.accounts.get(project_id, &a.code).await?;
        let dto = ::accounts::dto::UpdateAccountDto {
            name: a.name.text.clone(),
            archived: current.archived,
            name_lang: a.name.lang.upper().into(),
            name_ru: a.name.ru.clone(),
            name_en: a.name.en.clone(),
            name_th: a.name.th.clone(),
            account_kind: a.kind.clone(),
            currency_code: a.currency.clone(),
            holder_user_id: current.holder_user_id,
        };
        Ok(account_from(self.accounts.update(project_id, &a.code, dto).await?))
    }

    async fn set_opening(&self, project_id: Uuid, user_id: Uuid, account_code: &str, amount: Decimal, date: NaiveDate) -> AppResult<()> {
        self.accounts.set_opening(project_id, user_id, account_code, amount, date).await
    }

    async fn categories(&self, project_id: Uuid) -> AppResult<Vec<Category>> {
        Ok(self.categories.tree(project_id, true).await?.into_iter().map(|(_, c)| category_from(c)).collect())
    }

    async fn create_category(&self, project_id: Uuid, c: &Category) -> AppResult<Category> {
        let dto = ::categories::dto::CreateCategoryDto {
            code: c.code.clone(),
            name: c.name.text.clone(),
            name_lang: c.name.lang.upper().into(),
            name_ru: c.name.ru.clone(),
            name_en: c.name.en.clone(),
            name_th: c.name.th.clone(),
            category_kind: c.kind.clone(),
            parent_code: c.parent_code.clone(),
            sort_order: c.sort_order,
        };
        Ok(category_from(self.categories.create(project_id, dto).await?))
    }

    async fn update_category(&self, project_id: Uuid, c: &Category) -> AppResult<Category> {
        let current = self.categories.get(project_id, &c.code).await?;
        let dto = ::categories::dto::UpdateCategoryDto {
            name: c.name.text.clone(),
            archived: current.archived,
            name_lang: c.name.lang.upper().into(),
            name_ru: c.name.ru.clone(),
            name_en: c.name.en.clone(),
            name_th: c.name.th.clone(),
            category_kind: current.category_kind,
            parent_code: c.parent_code.clone(),
            sort_order: c.sort_order,
        };
        Ok(category_from(self.categories.update(project_id, &c.code, dto).await?))
    }
    async fn places(&self, project_id: Uuid) -> AppResult<Vec<Place>> {
        Ok(self.places.all(project_id, true).await?.into_iter().map(place_from).collect())
    }
    async fn create_place(&self, project_id: Uuid, p: &Place) -> AppResult<Place> {
        let dto = ::places::dto::CreatePlaceDto { code: p.code.clone(), data: place_fields(p) };
        Ok(place_from(self.places.create(project_id, dto).await?))
    }
    async fn update_place(&self, project_id: Uuid, p: &Place) -> AppResult<Place> {
        Ok(place_from(self.places.update(project_id, &p.code, place_fields(p)).await?))
    }
}

pub struct RatesAdapter(pub Arc<::rates::Service>);

fn rate_from(r: ::rates::dto::RateDto) -> Rate {
    Rate { id: r.id, date: r.rate_date, currency: r.currency_code, rate: r.rate, entered: r.entered, inverted: r.inverted }
}

fn rate_input(r: &Rate) -> ::rates::dto::RateInput {
    ::rates::dto::RateInput { rate_date: r.date, currency_code: r.currency.clone(), rate: r.entered, inverted: r.inverted }
}

#[async_trait]
impl ::mcp::Rates for RatesAdapter {
    async fn list(&self, project_id: Uuid, from: Option<NaiveDate>, to: Option<NaiveDate>) -> AppResult<Vec<Rate>> {
        Ok(self
            .0
            .all(project_id)
            .await?
            .into_iter()
            .filter(|r| from.is_none_or(|f| r.rate_date >= f) && to.is_none_or(|t| r.rate_date <= t))
            .map(rate_from)
            .collect())
    }

    async fn create(&self, project_id: Uuid, rate: &Rate) -> AppResult<Rate> {
        Ok(rate_from(self.0.create(project_id, rate_input(rate)).await?))
    }

    async fn update(&self, project_id: Uuid, rate: &Rate) -> AppResult<Rate> {
        Ok(rate_from(self.0.update(project_id, rate.id, rate_input(rate)).await?))
    }
}

pub struct LimitsAdapter(pub Arc<::limits::Service>);

fn limit_from(l: ::limits::dto::LimitDto) -> Limit {
    Limit { id: l.id, category_code: l.category_code, period: l.limit_period, valid_from: l.valid_from, amount: l.amount }
}

fn limit_input(l: &Limit) -> ::limits::dto::LimitInput {
    ::limits::dto::LimitInput { valid_from: l.valid_from, category_code: l.category_code.clone(), limit_period: l.period.clone(), amount: l.amount, removed: l.amount.is_none() }
}

#[async_trait]
impl ::mcp::Limits for LimitsAdapter {
    async fn list(&self, project_id: Uuid) -> AppResult<Vec<Limit>> {
        Ok(self.0.all(project_id).await?.into_iter().map(limit_from).collect())
    }

    async fn create(&self, project_id: Uuid, limit: &Limit) -> AppResult<Limit> {
        Ok(limit_from(self.0.create(project_id, limit_input(limit)).await?))
    }

    async fn update(&self, project_id: Uuid, limit: &Limit) -> AppResult<Limit> {
        Ok(limit_from(self.0.update(project_id, limit.id, limit_input(limit)).await?))
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
