//! Тесты use cases фичи «MCP для ассистентов»: вход по токенам, доступы ассистентов, права в
//! инструментах (чтение / запись / роль / 2FA), запись операций. Без БД: memory-репозиторий и фейковые порты.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use kernel::auth::{Authenticator, Credential, Identity, Memberships, ProjectRef, RequestMeta, Role, SessionInfo, Theme, Via as AuthVia};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::adapters::memory::InMemoryGrantRepository;
use crate::domain::*;
use crate::dto::*;
use crate::ports::{Accounts, Budget, Deps, Directories, Ledger, Limits, Plans, Rates};
use crate::service::{Auth, McpService, Settings, tool_right};

const RESOURCE: &str = "https://budget.test/mcp";

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

// ---------------------------------------------------------------- фейки

#[derive(Default)]
struct FakeLedger {
    txs: Mutex<Vec<Tx>>,
}

fn quick_info() -> QuickInfo {
    let cat = |code: &str, parent: Option<&str>, kind: &str, title: &str, search: &str| CategoryInfo {
        code: code.into(),
        parent_code: parent.map(Into::into),
        kind: kind.into(),
        title: title.into(),
        depth: usize::from(parent.is_some()),
        search: search.into(),
        limit: (code == "FOOD").then(|| d("1000")),
        available: (code == "FOOD" || code == "CAFE").then(|| d("700")),
    };
    QuickInfo {
        default_account: Some("CASH".into()),
        accounts: vec![
            AccountInfo { code: "CASH".into(), title: "Наличные".into(), currency: "THB".into(), symbol: "฿".into() },
            AccountInfo { code: "CARD".into(), title: "Карта".into(), currency: "THB".into(), symbol: "฿".into() },
        ],
        categories: vec![
            cat("FOOD", None, "EXPENSE", "Еда", "Еда Food อาหาร"),
            cat("CAFE", Some("FOOD"), "EXPENSE", "Кафе", "Кафе Cafe คาเฟ่"),
            cat("CAR", None, "EXPENSE", "Машина", "Машина Car รถ"),
            cat("CARWASH", Some("CAR"), "EXPENSE", "Мойка", "Мойка Car wash ล้างรถ"),
            cat("SALARY", None, "INCOME", "Зарплата", "Зарплата Salary เงินเดือน"),
        ],
        frequent_expense: vec!["CAFE".into()],
        frequent_income: vec![],
    }
}

fn tx_of(id: Uuid, user: Uuid, d: &TxDraft) -> Tx {
    let total = if d.kind == "TRANSFER" { d.amount.unwrap_or_default() } else { d.lines.iter().map(|l| l.amount).sum() };
    Tx {
        id,
        occurred_on: d.occurred_on,
        kind: d.kind.clone(),
        account_code: d.account_code.clone(),
        to_account_code: d.to_account_code.clone(),
        refund: d.refund,
        lines: d.lines.clone(),
        total,
        currency: "THB".into(),
        to_amount: d.to_amount,
        to_currency: d.to_account_code.as_ref().map(|_| "THB".into()),
        fee_category_code: d.fee_category_code.clone(),
        fee_amount: d.fee_amount,
        description: d.description.clone(),
        description_lang: d.description_lang,
        description_ru: d.description_ru.clone(),
        description_en: d.description_en.clone(),
        description_th: d.description_th.clone(),
        created_by: user,
        plan_id: d.plan_id,
        plan_date: d.plan_date,
    }
}

#[async_trait]
impl Ledger for Arc<FakeLedger> {
    async fn quick(&self, _p: Uuid, _u: Uuid, _base: &str, _lang: Lang, _date: NaiveDate) -> AppResult<QuickInfo> {
        Ok(quick_info())
    }
    async fn list(&self, _p: Uuid, filter: TxFilter, limit: i64, offset: i64) -> AppResult<(Vec<Tx>, i64)> {
        let all: Vec<Tx> = self.txs.lock().unwrap().iter().filter(|t| filter.kind.as_ref().is_none_or(|k| &t.kind == k)).cloned().collect();
        let total = all.len() as i64;
        Ok((all.into_iter().skip(offset as usize).take(limit as usize).collect(), total))
    }
    async fn get(&self, _p: Uuid, id: Uuid) -> AppResult<Tx> {
        self.txs.lock().unwrap().iter().find(|t| t.id == id).cloned().ok_or_else(|| AppError::NotFound("transaction".into()))
    }
    async fn create(&self, _p: Uuid, user: Uuid, draft: TxDraft) -> AppResult<Tx> {
        let tx = tx_of(Uuid::new_v4(), user, &draft);
        self.txs.lock().unwrap().push(tx.clone());
        Ok(tx)
    }
    async fn update(&self, _p: Uuid, id: Uuid, draft: TxDraft) -> AppResult<Tx> {
        let mut txs = self.txs.lock().unwrap();
        let t = txs.iter_mut().find(|t| t.id == id).ok_or_else(|| AppError::NotFound("transaction".into()))?;
        *t = tx_of(id, t.created_by, &draft);
        Ok(t.clone())
    }
    async fn balances(&self, _p: Uuid, _date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        Ok(vec![("CASH".into(), d("500")), ("CARD".into(), d("12000"))])
    }
}

struct FakeBudget;

#[async_trait]
impl Budget for FakeBudget {
    async fn report(&self, _p: Uuid, base: &str, _view: &str, date: NaiveDate, _lang: Lang) -> AppResult<BudgetReport> {
        Ok(BudgetReport {
            from: date,
            to: date,
            base_currency: base.into(),
            expense_fact: d("300"),
            expense_planned: Decimal::ZERO,
            income_fact: Decimal::ZERO,
            income_planned: Decimal::ZERO,
            lines: vec![],
            missing_rates: vec![],
        })
    }
}

#[derive(Default)]
struct FakePlans {
    skipped: Mutex<Vec<(Uuid, NaiveDate)>>,
    plans: Mutex<Vec<Plan>>,
}

#[async_trait]
impl Plans for Arc<FakePlans> {
    async fn occurrences(&self, _p: Uuid, from: NaiveDate, _to: NaiveDate, _today: NaiveDate, _lang: Lang) -> AppResult<Vec<Occurrence>> {
        Ok(vec![Occurrence {
            plan_id: plan(),
            name: "Интернет".into(),
            date: from,
            status: "EXPECTED".into(),
            kind: "EXPENSE".into(),
            amount: d("599"),
            account_code: "CARD".into(),
            category_code: Some("CAR".into()),
            to_account_code: None,
            transaction_id: None,
        }])
    }
    async fn skip(&self, _p: Uuid, _u: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<()> {
        self.skipped.lock().unwrap().push((plan_id, date));
        Ok(())
    }
    async fn list(&self, _p: Uuid) -> AppResult<Vec<Plan>> {
        Ok(self.plans.lock().unwrap().clone())
    }
    async fn create(&self, _p: Uuid, _u: Uuid, draft: &PlanDraft) -> AppResult<Plan> {
        let plan = Plan { id: Uuid::new_v4(), draft: draft.clone() };
        self.plans.lock().unwrap().push(plan.clone());
        Ok(plan)
    }
    async fn update(&self, _p: Uuid, id: Uuid, draft: &PlanDraft) -> AppResult<Plan> {
        let mut plans = self.plans.lock().unwrap();
        let p = plans.iter_mut().find(|p| p.id == id).ok_or_else(|| AppError::NotFound("plan".into()))?;
        p.draft = draft.clone();
        Ok(p.clone())
    }
}

fn name(text: &str, ru: &str, en: &str, th: &str) -> Name {
    Name { text: text.into(), lang: Lang::Ru, ru: Some(ru.into()), en: Some(en.into()), th: Some(th.into()) }
}

/// Справочники: хранят ровно то, что пришло (признак архива проверяется на том, что передал сервис).
struct FakeDirs {
    currencies: Mutex<Vec<Currency>>,
    accounts: Mutex<Vec<Account>>,
    categories: Mutex<Vec<Category>>,
    openings: Mutex<Vec<(String, Decimal, NaiveDate)>>,
}

impl Default for FakeDirs {
    fn default() -> Self {
        let cur = |code: &str, n: Name, symbol: &str, archived: bool| Currency { code: code.into(), name: n, symbol: symbol.into(), minor_units: 2, archived };
        let acc = |code: &str, n: Name| Account { code: code.into(), name: n, kind: "CASH".into(), currency: "THB".into(), holder_user_id: None, archived: false };
        let cat = |code: &str, parent: Option<&str>, kind: &str, n: Name, order: i32| Category {
            code: code.into(),
            name: n,
            kind: kind.into(),
            parent_code: parent.map(Into::into),
            sort_order: order,
            archived: false,
        };
        FakeDirs {
            currencies: Mutex::new(vec![
                cur("THB", name("Тайский бат", "Тайский бат", "Thai baht", "บาท"), "฿", false),
                cur("USD", name("Доллар США", "Доллар США", "US dollar", "ดอลลาร์สหรัฐ"), "$", false),
                cur("EUR", name("Евро", "Евро", "Euro", "ยูโร"), "€", true),
            ]),
            accounts: Mutex::new(vec![acc("CASH", name("Наличные", "Наличные", "Cash", "เงินสด")), acc("CARD", name("Карта", "Карта", "Card", "บัตร"))]),
            categories: Mutex::new(vec![
                cat("FOOD", None, "EXPENSE", name("Еда", "Еда", "Food", "อาหาร"), 10),
                cat("CAFE", Some("FOOD"), "EXPENSE", name("Кафе", "Кафе", "Cafe", "คาเฟ่"), 10),
                cat("CAR", None, "EXPENSE", name("Машина", "Машина", "Car", "รถ"), 20),
                cat("CARWASH", Some("CAR"), "EXPENSE", name("Мойка", "Мойка", "Car wash", "ล้างรถ"), 10),
                cat("SALARY", None, "INCOME", name("Зарплата", "Зарплата", "Salary", "เงินเดือน"), 10),
            ]),
            openings: Mutex::default(),
        }
    }
}

fn upsert<T: Clone>(list: &Mutex<Vec<T>>, item: &T, same: impl Fn(&T) -> bool) -> AppResult<T> {
    let mut list = list.lock().unwrap();
    let x = list.iter_mut().find(|x| same(x)).ok_or_else(|| AppError::NotFound("record".into()))?;
    *x = item.clone();
    Ok(item.clone())
}

#[async_trait]
impl Directories for Arc<FakeDirs> {
    async fn currencies(&self, _p: Uuid) -> AppResult<Vec<Currency>> {
        Ok(self.currencies.lock().unwrap().clone())
    }
    async fn create_currency(&self, _p: Uuid, c: &Currency) -> AppResult<Currency> {
        self.currencies.lock().unwrap().push(c.clone());
        Ok(c.clone())
    }
    async fn update_currency(&self, _p: Uuid, c: &Currency) -> AppResult<Currency> {
        upsert(&self.currencies, c, |x| x.code == c.code)
    }
    async fn accounts(&self, _p: Uuid) -> AppResult<Vec<Account>> {
        Ok(self.accounts.lock().unwrap().clone())
    }
    async fn create_account(&self, _p: Uuid, a: &Account) -> AppResult<Account> {
        self.accounts.lock().unwrap().push(a.clone());
        Ok(a.clone())
    }
    async fn update_account(&self, _p: Uuid, a: &Account) -> AppResult<Account> {
        upsert(&self.accounts, a, |x| x.code == a.code)
    }
    async fn set_opening(&self, _p: Uuid, _u: Uuid, code: &str, amount: Decimal, date: NaiveDate) -> AppResult<()> {
        self.openings.lock().unwrap().push((code.to_string(), amount, date));
        Ok(())
    }
    async fn categories(&self, _p: Uuid) -> AppResult<Vec<Category>> {
        Ok(self.categories.lock().unwrap().clone())
    }
    async fn create_category(&self, _p: Uuid, c: &Category) -> AppResult<Category> {
        self.categories.lock().unwrap().push(c.clone());
        Ok(c.clone())
    }
    async fn update_category(&self, _p: Uuid, c: &Category) -> AppResult<Category> {
        upsert(&self.categories, c, |x| x.code == c.code)
    }
}

#[derive(Default)]
struct FakeRates {
    rates: Mutex<Vec<Rate>>,
}

#[async_trait]
impl Rates for Arc<FakeRates> {
    async fn list(&self, _p: Uuid, from: Option<NaiveDate>, to: Option<NaiveDate>) -> AppResult<Vec<Rate>> {
        let mut list: Vec<Rate> = self.rates.lock().unwrap().iter().filter(|r| from.is_none_or(|f| r.date >= f) && to.is_none_or(|t| r.date <= t)).cloned().collect();
        list.sort_by_key(|r| std::cmp::Reverse(r.date));
        Ok(list)
    }
    async fn create(&self, _p: Uuid, rate: &Rate) -> AppResult<Rate> {
        let mut rates = self.rates.lock().unwrap();
        if rates.iter().any(|r| r.currency == rate.currency && r.date == rate.date) {
            return Err(AppError::Conflict("курс на эту дату уже есть".into()));
        }
        let r = Rate { id: Uuid::new_v4(), ..rate.clone() };
        rates.push(r.clone());
        Ok(r)
    }
    async fn update(&self, _p: Uuid, rate: &Rate) -> AppResult<Rate> {
        upsert(&self.rates, rate, |x| x.id == rate.id)
    }
}

#[derive(Default)]
struct FakeLimits {
    limits: Mutex<Vec<Limit>>,
}

#[async_trait]
impl Limits for Arc<FakeLimits> {
    async fn list(&self, _p: Uuid) -> AppResult<Vec<Limit>> {
        Ok(self.limits.lock().unwrap().clone())
    }
    async fn create(&self, _p: Uuid, limit: &Limit) -> AppResult<Limit> {
        let mut limits = self.limits.lock().unwrap();
        if limits.iter().any(|l| l.category_code == limit.category_code && l.valid_from == limit.valid_from) {
            return Err(AppError::Conflict("лимит с этой даты уже есть".into()));
        }
        let l = Limit { id: Uuid::new_v4(), ..limit.clone() };
        limits.push(l.clone());
        Ok(l)
    }
    async fn update(&self, _p: Uuid, limit: &Limit) -> AppResult<Limit> {
        upsert(&self.limits, limit, |x| x.id == limit.id)
    }
}

fn plan() -> Uuid {
    Uuid::from_u128(77)
}

/// Событие журнала: действие и подробности.
type Event = (String, Vec<(String, String)>);

/// Пользователи и журнал: access-токен понарошку `at:<user uuid>:<клиент>:<ресурс>`.
#[derive(Default)]
struct FakeAccounts {
    events: Mutex<Vec<Event>>,
    guest: Mutex<Option<Uuid>>,
}

#[async_trait]
impl Accounts for Arc<FakeAccounts> {
    async fn verify_access_token(&self, token: &str, audience: &str) -> AppResult<Option<(Uuid, String)>> {
        Ok(match token.splitn(4, ':').collect::<Vec<_>>()[..] {
            ["at", user, client, aud] if aud == audience => Uuid::parse_str(user).ok().map(|u| (u, client.to_string())),
            _ => None,
        })
    }
    async fn issuer(&self) -> Option<String> {
        Some("https://auth.test/auth/v1/".into())
    }
    async fn is_guest(&self, user_id: Uuid) -> AppResult<bool> {
        Ok(*self.guest.lock().unwrap() == Some(user_id))
    }
    async fn record(&self, _user: Uuid, _project: Option<Uuid>, action: &str, _meta: &RequestMeta, details: &[(&str, String)]) {
        self.events.lock().unwrap().push((action.to_string(), details.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()));
    }
}

/// Личные токены: `fbt_<user>_<project>_<scope,...>`.
struct FakeAuthenticator;

#[async_trait]
impl Authenticator for FakeAuthenticator {
    async fn authenticate(&self, credential: &Credential) -> AppResult<Option<Identity>> {
        let Credential::ApiToken(raw) = credential else { return Ok(None) };
        let parts: Vec<&str> = raw.trim_start_matches("fbt_").split('_').collect();
        let [user, project, scopes] = parts[..] else { return Ok(None) };
        Ok(Some(Identity {
            user_id: Uuid::parse_str(user).unwrap(),
            mfa: true,
            via: AuthVia::Token { token_id: Uuid::new_v4(), project_id: Uuid::parse_str(project).unwrap(), scopes: scopes.split(',').map(String::from).collect() },
        }))
    }
}

/// Участие в проектах: (пользователь, проект, роль, требует 2FA).
#[derive(Default)]
struct FakeMemberships {
    members: Mutex<Vec<(Uuid, Uuid, Role, bool)>>,
}

#[async_trait]
impl Memberships for FakeMemberships {
    async fn session(&self, user_id: Uuid, project: Option<Uuid>) -> AppResult<Option<SessionInfo>> {
        let m = self.members.lock().unwrap();
        let mine: Vec<_> = m.iter().filter(|x| x.0 == user_id).collect();
        let pick = project.and_then(|p| mine.iter().find(|x| x.1 == p)).or(mine.first()).copied();
        Ok(pick.map(|(_, p, role, tfa)| SessionInfo {
            user_name: "Алексей".into(),
            user_lang: Lang::Ru,
            theme: Theme::System,
            project_id: *p,
            project_name: "Семья".into(),
            base_currency: "THB".into(),
            role: *role,
            require_2fa: *tfa,
            projects: mine.iter().map(|x| ProjectRef { id: x.1, name: "Семья".into(), role: x.2 }).collect(),
            members: vec![],
        }))
    }
}

struct Env {
    svc: McpService,
    ledger: Arc<FakeLedger>,
    plans: Arc<FakePlans>,
    dirs: Arc<FakeDirs>,
    rates: Arc<FakeRates>,
    limits: Arc<FakeLimits>,
    accounts: Arc<FakeAccounts>,
    members: Arc<FakeMemberships>,
    user: Uuid,
    project: Uuid,
}

fn env() -> Env {
    let (ledger, plans, accounts, members) = (Arc::new(FakeLedger::default()), Arc::new(FakePlans::default()), Arc::new(FakeAccounts::default()), Arc::new(FakeMemberships::default()));
    let (dirs, rates, limits) = (Arc::new(FakeDirs::default()), Arc::new(FakeRates::default()), Arc::new(FakeLimits::default()));
    let (user, project) = (Uuid::new_v4(), Uuid::new_v4());
    members.members.lock().unwrap().push((user, project, Role::Owner, false));
    let deps = Deps {
        ledger: Arc::new(ledger.clone()),
        budget: Arc::new(FakeBudget),
        plans: Arc::new(plans.clone()),
        directories: Arc::new(dirs.clone()),
        rates: Arc::new(rates.clone()),
        limits: Arc::new(limits.clone()),
        accounts: Arc::new(accounts.clone()),
        authenticator: Arc::new(FakeAuthenticator),
        memberships: members.clone(),
    };
    let settings = Settings { base_url: "https://budget.test".into(), resource: RESOURCE.into(), oauth_client_id: "claude".into(), oauth_scope: "openid".into(), rate_limit: 600 };
    let svc = McpService::new(Arc::new(InMemoryGrantRepository::default()), deps, settings);
    Env { svc, ledger, plans, dirs, rates, limits, accounts, members, user, project }
}

impl Env {
    fn oauth(&self) -> String {
        format!("at:{}:claude:{RESOURCE}", self.user)
    }

    async fn caller(&self, token: &str) -> Caller {
        match self.svc.authenticate(Some(token), RequestMeta::default()).await.unwrap() {
            Auth::Ok(c) => *c,
            other => panic!("ожидался вход, получено {other:?}"),
        }
    }

    async fn grant(&self, scopes: &[&str]) {
        let scopes: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
        self.svc.save_grant(self.user, "claude", self.project, Role::Owner, false, false, &scopes, &RequestMeta::default()).await.unwrap();
    }

    fn set_role(&self, role: Role, require_2fa: bool) {
        let mut m = self.members.members.lock().unwrap();
        m.retain(|x| x.0 != self.user);
        m.push((self.user, self.project, role, require_2fa));
    }

    fn events(&self) -> Vec<String> {
        self.accounts.events.lock().unwrap().iter().map(|e| e.0.clone()).collect()
    }
}

fn expense(amount: &str, category: &str) -> AddFlowArgs {
    AddFlowArgs { amount: Some(amount.into()), category: Some(category.into()), ..Default::default() }
}

// ---------------------------------------------------------------- вход

#[tokio::test]
async fn missing_and_foreign_tokens_are_rejected() {
    let e = env();
    assert!(matches!(e.svc.authenticate(None, RequestMeta::default()).await.unwrap(), Auth::Missing));
    let foreign = format!("at:{}:claude:https://other.test/mcp", e.user);
    assert!(matches!(e.svc.authenticate(Some(&foreign), RequestMeta::default()).await.unwrap(), Auth::Invalid), "токен для другого ресурса");
    assert!(matches!(e.svc.authenticate(Some("garbage"), RequestMeta::default()).await.unwrap(), Auth::Invalid));
}

#[tokio::test]
async fn personal_token_brings_its_project_and_scopes() {
    let e = env();
    let c = e.caller(&format!("fbt_{}_{}_transactions:write", e.user, e.project)).await;
    assert_eq!(c.project_id, e.project);
    assert!(c.allows(Right::write("transactions")) && c.allows(Right::read("transactions")));
    assert!(!c.allows(Right::read("budget")), "transactions:write не даёт бюджет");
}

#[tokio::test]
async fn personal_token_for_a_left_project_does_not_work() {
    let e = env();
    let other = Uuid::new_v4();
    let token = format!("fbt_{}_{}_read", e.user, other);
    assert!(matches!(e.svc.authenticate(Some(&token), RequestMeta::default()).await.unwrap(), Auth::Invalid));
}

// ---------------------------------------------------------------- доступ ассистента

#[tokio::test]
async fn assistant_without_grant_sees_only_setup_hint() {
    let e = env();
    let c = e.caller(&e.oauth()).await;
    assert!(matches!(c.via, Via::Unconfigured { reason: Unconfigured::NoGrant, .. }));
    let ctx = e.svc.get_context(&c).await.unwrap();
    assert!(ctx.summary.contains("https://budget.test/assistants"), "{}", ctx.summary);
    assert!(e.svc.visible(&c, "get_context"));
    assert!(!e.svc.visible(&c, "list_transactions") && !e.svc.visible(&c, "add_expense"));
    let err = e.svc.add_expense(&c, expense("100", "CAFE")).await.unwrap_err();
    assert!(err.0.contains("/assistants"), "{}", err.0);
    assert!(e.ledger.txs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn default_grant_is_view_and_transactions() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    assert!(matches!(c.via, Via::Grant { .. }));
    assert!(e.svc.visible(&c, "add_expense") && e.svc.visible(&c, "get_budget"));
    assert!(!e.svc.visible(&c, "skip_planned"), "плановые — отдельным правом");
    let err = e.svc.skip_planned(&c, SkipPlannedArgs { plan_id: plan().to_string(), date: "2026-10-20".into() }).await.unwrap_err();
    assert!(err.0.contains("permission denied"), "{}", err.0);
    assert!(e.plans.skipped.lock().unwrap().is_empty());
    assert_eq!(e.events(), vec!["mcp_access_granted"]);
}

#[tokio::test]
async fn unknown_scopes_are_dropped_and_view_is_always_present() {
    let e = env();
    e.grant(&["write", "delete", "projects:write", "planning:write"]).await;
    let g = e.svc.grants(e.user).await.unwrap().remove(0);
    assert_eq!(g.scopes, vec!["read".to_string(), "planning:write".to_string()]);
}

#[tokio::test]
async fn viewer_role_limits_grant_and_calls() {
    let e = env();
    let scopes = vec!["read".to_string(), "transactions:write".to_string()];
    let g = e.svc.save_grant(e.user, "claude", e.project, Role::Viewer, false, false, &scopes, &RequestMeta::default()).await.unwrap();
    assert_eq!(g.scopes, vec!["read".to_string()], "права не выше роли");
    // Доступ выдан OWNER-ом, потом роль понизили до VIEWER: запись сразу недоступна.
    e.grant(&["read", "transactions:write"]).await;
    e.set_role(Role::Viewer, false);
    let c = e.caller(&e.oauth()).await;
    assert!(!e.svc.visible(&c, "add_expense"));
    assert!(e.svc.add_expense(&c, expense("100", "CAFE")).await.is_err());
}

#[tokio::test]
async fn project_with_2fa_needs_grant_saved_with_second_factor() {
    let e = env();
    let scopes = vec!["read".to_string()];
    let err = e.svc.save_grant(e.user, "claude", e.project, Role::Owner, true, false, &scopes, &RequestMeta::default()).await.unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
    // Доступ был открыт без 2FA, потом проект начал требовать 2FA — ассистент больше ничего не видит.
    e.grant(&["read"]).await;
    e.set_role(Role::Owner, true);
    let c = e.caller(&e.oauth()).await;
    assert!(matches!(c.via, Via::Unconfigured { reason: Unconfigured::Mfa, .. }));
}

#[tokio::test]
async fn left_project_and_revoked_grant_stop_access() {
    let e = env();
    e.grant(&["read"]).await;
    e.members.members.lock().unwrap().clear();
    e.members.members.lock().unwrap().push((e.user, Uuid::new_v4(), Role::Owner, false));
    assert!(matches!(e.caller(&e.oauth()).await.via, Via::Unconfigured { reason: Unconfigured::ProjectGone, .. }));
    e.set_role(Role::Owner, false);
    e.svc.revoke_grant(e.user, "claude", &RequestMeta::default()).await.unwrap();
    assert!(matches!(e.caller(&e.oauth()).await.via, Via::Unconfigured { reason: Unconfigured::NoGrant, .. }));
    assert!(e.events().contains(&"mcp_access_revoked".to_string()));
}

#[tokio::test]
async fn demo_guest_cannot_grant_access() {
    let e = env();
    *e.accounts.guest.lock().unwrap() = Some(e.user);
    let err = e.svc.save_grant(e.user, "claude", e.project, Role::Owner, false, false, &[], &RequestMeta::default()).await.unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

// ---------------------------------------------------------------- инструменты

#[tokio::test]
async fn expense_by_name_in_any_language_uses_default_account_and_is_audited() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e.svc.add_expense(&c, AddFlowArgs { description: Some("Кофе".into()), ..expense("120+45", "cafe") }).await.unwrap();
    let tx = e.ledger.txs.lock().unwrap()[0].clone();
    assert_eq!((tx.account_code.as_str(), tx.lines[0].category_code.as_str(), tx.total), ("CASH", "CAFE", d("165")));
    assert_eq!(tx.description_lang, Lang::Ru, "язык описания — язык пользователя");
    assert!(out.summary.contains("Left this month"), "{}", out.summary);
    let thai = e.svc.add_expense(&c, expense("80", "ล้างรถ")).await.unwrap();
    assert_eq!(thai.data["transaction"]["lines"][0]["category"], "CARWASH");
    let events = e.accounts.events.lock().unwrap().clone();
    let write = events.iter().find(|x| x.0 == "mcp_write").unwrap();
    assert!(write.1.contains(&("client".to_string(), "claude".to_string())));
}

#[tokio::test]
async fn ambiguous_or_unknown_category_asks_for_a_code() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let err = e.svc.add_expense(&c, expense("10", "รถ")).await.unwrap_err();
    assert!(err.0.contains("ambiguous") && err.0.contains("CARWASH"), "{}", err.0);
    let err = e.svc.add_expense(&c, expense("10", "Salary")).await.unwrap_err();
    assert!(err.0.contains("not found"), "доходная категория для расхода не подходит: {}", err.0);
    assert!(e.ledger.txs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn expense_with_items_and_conflicting_arguments() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let items = vec![ItemArgs { category: "CAFE".into(), amount: "100".into(), note: None }, ItemArgs { category: "CARWASH".into(), amount: "50.5".into(), note: Some("мойка".into()) }];
    let out = e.svc.add_expense(&c, AddFlowArgs { items: Some(items), account: Some("Карта".into()), ..Default::default() }).await.unwrap();
    assert_eq!(out.data["transaction"]["account"], "CARD");
    assert_eq!(e.ledger.txs.lock().unwrap()[0].total, d("150.5"));
    let both = AddFlowArgs { items: Some(vec![ItemArgs { category: "CAFE".into(), amount: "1".into(), note: None }]), ..expense("1", "CAFE") };
    assert!(e.svc.add_expense(&c, both).await.is_err());
    assert!(e.svc.add_expense(&c, expense("0", "CAFE")).await.is_err(), "сумма > 0");
    assert!(e.svc.add_expense(&c, expense("10", "CAFE").with_date("15.10.2026")).await.is_err(), "дата — YYYY-MM-DD");
}

impl AddFlowArgs {
    fn with_date(mut self, date: &str) -> Self {
        self.date = Some(date.into());
        self
    }
}

#[tokio::test]
async fn update_changes_only_passed_fields() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    e.svc.add_expense(&c, AddFlowArgs { description: Some("Кофе".into()), ..expense("100", "CAFE") }).await.unwrap();
    let id = e.ledger.txs.lock().unwrap()[0].id;
    e.svc.update_transaction(&c, UpdateTransactionArgs { id: id.to_string(), amount: Some("90".into()), ..Default::default() }).await.unwrap();
    let tx = e.ledger.txs.lock().unwrap()[0].clone();
    assert_eq!((tx.total, tx.lines[0].category_code.as_str(), tx.description.as_deref()), (d("90"), "CAFE", Some("Кофе")));
    let err = e.svc.update_transaction(&c, UpdateTransactionArgs { id: id.to_string(), to_amount: Some("5".into()), ..Default::default() }).await.unwrap_err();
    assert!(err.0.contains("transfers"), "{}", err.0);
}

#[tokio::test]
async fn transfer_and_scheduled_payment() {
    let e = env();
    e.grant(&["read", "transactions:write", "planning:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let t = AddTransferArgs { from_account: "CARD".into(), to_account: "CASH".into(), amount: "1000".into(), ..Default::default() };
    let out = e.svc.add_transfer(&c, t).await.unwrap();
    assert_eq!(out.data["transaction"]["to_account"], "CASH");
    let paid = e.svc.pay_planned(&c, PayPlannedArgs { plan_id: plan().to_string(), date: "2026-10-20".into(), ..Default::default() }).await.unwrap();
    assert!(paid.summary.contains("Интернет"));
    let tx = e.ledger.txs.lock().unwrap().last().cloned().unwrap();
    assert_eq!((tx.plan_id, tx.total, tx.account_code.as_str()), (Some(plan()), d("599"), "CARD"));
    e.svc.skip_planned(&c, SkipPlannedArgs { plan_id: plan().to_string(), date: "2026-11-20".into() }).await.unwrap();
    assert_eq!(e.plans.skipped.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn reading_tools_respect_rights() {
    let e = env();
    let c = e.caller(&format!("fbt_{}_{}_transactions:read", e.user, e.project)).await;
    assert!(e.svc.get_balances(&c, DateArgs::default()).await.is_ok());
    assert!(e.svc.get_budget(&c, BudgetArgs::default()).await.unwrap_err().0.contains("budget:read"));
    assert!(e.svc.add_expense(&c, expense("10", "CAFE")).await.is_err(), "чтение не даёт записи");
    let ctx = e.svc.get_context(&c).await.unwrap();
    assert_eq!(ctx.data["permissions"]["delete"], false);
    assert_eq!(ctx.data["accounts"][0]["balance"], "500", "суммы — строками, без потерь точности");
}

#[tokio::test]
async fn list_filters_by_category_subtree_and_text() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    e.svc.add_expense(&c, AddFlowArgs { description: Some("Starbucks".into()), ..expense("100", "CAFE") }).await.unwrap();
    e.svc.add_expense(&c, expense("50", "CARWASH")).await.unwrap();
    let food = e.svc.list_transactions(&c, ListTransactionsArgs { category: Some("FOOD".into()), from: Some("2000-01-01".into()), ..Default::default() }).await.unwrap();
    assert_eq!(food.data["total"], 1, "FOOD включает подкатегорию CAFE");
    let text = e.svc.list_transactions(&c, ListTransactionsArgs { text: Some("starbucks".into()), from: Some("2000-01-01".into()), ..Default::default() }).await.unwrap();
    assert_eq!(text.data["total"], 1);
}

// ---------------------------------------------------------------- справочники, курсы, лимиты, планы

/// Все инструменты записи справочников, курсов, лимитов и планов.
const NEW_WRITE_TOOLS: [&str; 10] =
    ["add_currency", "update_currency", "add_account", "update_account", "add_category", "update_category", "set_rate", "set_limit", "add_plan", "update_plan"];

fn add_account(code: &str, currency: &str) -> AddAccountArgs {
    AddAccountArgs { code: code.into(), name: "K-Bank".into(), kind: "bank".into(), currency: currency.into(), ..Default::default() }
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

#[tokio::test]
async fn directory_tools_need_their_right_and_are_hidden_without_it() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    for t in NEW_WRITE_TOOLS {
        assert!(!e.svc.visible(&c, t), "{t} без права не виден");
    }
    for t in ["list_rates", "list_limits", "list_plans"] {
        assert!(e.svc.visible(&c, t), "{t} — просмотр");
    }
    let err = e.svc.add_account(&c, add_account("KBANK", "THB")).await.unwrap_err();
    assert!(err.0.contains("accounts:write"), "{}", err.0);
    let err = e.svc.set_rate(&c, SetRateArgs { currency: "USD".into(), rate: "36.5".into(), date: None }).await.unwrap_err();
    assert!(err.0.contains("rates:write"), "{}", err.0);
    assert!(e.svc.add_plan(&c, AddPlanArgs { name: "Аренда".into(), kind: "expense".into(), account: "CARD".into(), category: Some("FOOD".into()), amount: "100".into(), recurrence: "monthly".into(), ..Default::default() }).await.is_err());
    assert_eq!(e.dirs.accounts.lock().unwrap().len(), 2, "ничего не создано");
    assert!(e.rates.rates.lock().unwrap().is_empty() && e.plans.plans.lock().unwrap().is_empty());
    // Каждое право открывает только свой раздел.
    e.grant(&["read", "categories:write"]).await;
    let c = e.caller(&e.oauth()).await;
    assert!(e.svc.visible(&c, "add_category") && e.svc.visible(&c, "update_category"));
    assert!(!e.svc.visible(&c, "add_account") && !e.svc.visible(&c, "add_currency") && !e.svc.visible(&c, "set_limit"));
}

#[tokio::test]
async fn directories_checkbox_grants_the_four_directory_rights() {
    let e = env();
    e.grant(&["directories", "limits:write"]).await;
    let g = e.svc.grants(e.user).await.unwrap().remove(0);
    assert_eq!(g.scopes, vec!["read", "currencies:write", "accounts:write", "categories:write", "rates:write", "limits:write"]);
    let c = e.caller(&e.oauth()).await;
    assert!(e.svc.visible(&c, "add_currency") && e.svc.visible(&c, "set_rate") && e.svc.visible(&c, "set_limit"));
    assert!(!e.svc.visible(&c, "add_plan") && !e.svc.visible(&c, "add_expense"));
}

#[tokio::test]
async fn viewer_cannot_write_directories() {
    let e = env();
    let all: Vec<String> = GRANT_SCOPES.iter().map(|s| s.to_string()).chain([ALL_DIRECTORIES.to_string()]).collect();
    let g = e.svc.save_grant(e.user, "claude", e.project, Role::Viewer, false, false, &all, &RequestMeta::default()).await.unwrap();
    assert_eq!(g.scopes, vec!["read".to_string()], "права не выше роли");
    // Права выдал OWNER, потом роль понизили; личный токен с `write` у VIEWER тоже только читает.
    e.grant(&GRANT_SCOPES).await;
    e.set_role(Role::Viewer, false);
    for token in [e.oauth(), format!("fbt_{}_{}_write", e.user, e.project)] {
        let c = e.caller(&token).await;
        for t in NEW_WRITE_TOOLS {
            assert!(!e.svc.visible(&c, t), "{t} не виден VIEWER");
        }
        assert!(e.svc.add_currency(&c, AddCurrencyArgs { code: "GBP".into(), name: "Фунт".into(), symbol: "£".into(), ..Default::default() }).await.is_err());
        assert!(e.svc.set_limit(&c, SetLimitArgs { category: "FOOD".into(), amount: "100".into(), ..Default::default() }).await.is_err());
        assert!(e.svc.list_limits(&c).await.is_ok(), "просмотр остаётся");
    }
    assert_eq!(e.dirs.currencies.lock().unwrap().len(), 3);
    assert!(e.limits.limits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn personal_write_token_can_do_everything() {
    let e = env();
    let c = e.caller(&format!("fbt_{}_{}_write", e.user, e.project)).await;
    for t in crate::adapters::server::tool_names() {
        assert!(e.svc.visible(&c, &t), "токен write видит {t}");
    }
    e.svc.add_currency(&c, AddCurrencyArgs { code: "gbp".into(), name: "Фунт".into(), symbol: "£".into(), ..Default::default() }).await.unwrap();
    e.svc.add_account(&c, AddAccountArgs { opening_balance: Some("500".into()), ..add_account("KBANK", "THB") }).await.unwrap();
    e.svc.add_category(&c, AddCategoryArgs { code: "PETS".into(), name: "Питомцы".into(), kind: Some("expense".into()), ..Default::default() }).await.unwrap();
    e.svc.set_rate(&c, SetRateArgs { currency: "GBP".into(), rate: "45".into(), date: None }).await.unwrap();
    e.svc.set_limit(&c, SetLimitArgs { category: "PETS".into(), amount: "3000".into(), ..Default::default() }).await.unwrap();
    let plan = AddPlanArgs { name: "Корм".into(), kind: "expense".into(), account: "KBANK".into(), category: Some("PETS".into()), amount: "900".into(), recurrence: "monthly".into(), ..Default::default() };
    e.svc.add_plan(&c, plan).await.unwrap();
    assert_eq!(e.dirs.currencies.lock().unwrap().last().unwrap().code, "GBP", "код — в верхнем регистре");
    assert_eq!(e.dirs.openings.lock().unwrap().len(), 1);
    assert_eq!((e.rates.rates.lock().unwrap().len(), e.limits.limits.lock().unwrap().len(), e.plans.plans.lock().unwrap().len()), (1, 1, 1));
}

#[tokio::test]
async fn narrow_token_opens_only_its_directory() {
    let e = env();
    let c = e.caller(&format!("fbt_{}_{}_accounts:write", e.user, e.project)).await;
    assert!(e.svc.visible(&c, "add_account") && !e.svc.visible(&c, "add_category") && !e.svc.visible(&c, "add_expense"));
    // Начальный остаток — операция в журнале: без transactions:write счёт не создаётся вовсе.
    let err = e.svc.add_account(&c, AddAccountArgs { opening_balance: Some("1000".into()), ..add_account("KBANK", "THB") }).await.unwrap_err();
    assert!(err.0.contains("opening_balance") && err.0.contains("transactions:write"), "{}", err.0);
    assert_eq!(e.dirs.accounts.lock().unwrap().len(), 2);
    e.svc.add_account(&c, add_account("KBANK", "THB")).await.unwrap();
    assert!(e.svc.add_category(&c, AddCategoryArgs { code: "PETS".into(), name: "Питомцы".into(), kind: Some("expense".into()), ..Default::default() }).await.is_err());
    let ctx = e.svc.get_context(&c).await.unwrap();
    assert_eq!(ctx.data["permissions"]["write_accounts"], true);
    assert_eq!(ctx.data["permissions"]["write_categories"], false);
    assert_eq!(ctx.data["currencies"].as_array().unwrap().len(), 2, "валюты нужны для ссылок, архивные скрыты");
}

#[tokio::test]
async fn account_with_currency_by_name_and_opening_balance_is_audited() {
    let e = env();
    e.grant(&["read", "transactions:write", "accounts:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e
        .svc
        .add_account(
            &c,
            AddAccountArgs {
                code: "kbank".into(),
                name: "K-Bank".into(),
                name_lang: Some("en".into()),
                translations: Some(NameTranslations { ru: Some("Кей-Банк".into()), en: None, th: Some("กสิกรไทย".into()) }),
                opening_balance: Some("1 000,50".into()),
                ..add_account("", "บาท")
            },
        )
        .await
        .unwrap();
    let acc = e.dirs.accounts.lock().unwrap().last().cloned().unwrap();
    assert_eq!((acc.code.as_str(), acc.kind.as_str(), acc.currency.as_str(), acc.name.lang, acc.archived), ("KBANK", "BANK", "THB", Lang::En, false));
    assert_eq!(acc.name.ru.as_deref(), Some("Кей-Банк"));
    assert_eq!(e.dirs.openings.lock().unwrap()[0], ("KBANK".to_string(), d("1000.50"), today()));
    assert!(out.summary.contains("Opening balance 1000.5 THB"), "{}", out.summary);
    let events = e.accounts.events.lock().unwrap().clone();
    let w = events.iter().find(|x| x.0 == "mcp_write").unwrap();
    assert!(w.1.contains(&("tool".to_string(), "add_account".to_string())) && w.1.contains(&("account".to_string(), "KBANK".to_string())), "{:?}", w.1);
    // Повтор кода, плохой код, архивная валюта — понятные ошибки без записи.
    assert!(e.svc.add_account(&c, add_account("KBANK", "THB")).await.unwrap_err().0.contains("update_account"));
    assert!(e.svc.add_account(&c, add_account("K-BANK", "THB")).await.unwrap_err().0.contains("UPPER_SNAKE"));
    assert!(e.svc.add_account(&c, add_account("EURO", "EUR")).await.unwrap_err().0.contains("archived"));
    assert_eq!(e.dirs.accounts.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn references_by_name_in_ru_en_th() {
    let e = env();
    e.grant(&["read", "categories:write", "planning:write", "limits:write", "rates:write"]).await;
    let c = e.caller(&e.oauth()).await;
    // Родитель — по-тайски, вид наследуется от родителя.
    let out = e.svc.add_category(&c, AddCategoryArgs { code: "STREET".into(), name: "Уличная еда".into(), parent: Some("อาหาร".into()), ..Default::default() }).await.unwrap();
    assert_eq!(out.data["category"]["parent"], "FOOD");
    assert_eq!(out.data["category"]["kind"], "EXPENSE");
    assert_eq!(out.data["category"]["sort_order"], 20, "после последнего соседа (CAFE = 10)");
    // Счёт — по-русски, категория — по-английски.
    let plan = AddPlanArgs { name: "Мойка".into(), kind: "expense".into(), account: "карта".into(), category: Some("Car wash".into()), amount: "300".into(), recurrence: "weekly".into(), interval: Some(2), ..Default::default() };
    let out = e.svc.add_plan(&c, plan).await.unwrap();
    assert_eq!((out.data["plan"]["account"].as_str(), out.data["plan"]["category"].as_str()), (Some("CARD"), Some("CARWASH")));
    assert!(out.summary.contains("every 2 weeks"), "{}", out.summary);
    // Валюта — по-английски, лимит — по-русски.
    e.svc.set_rate(&c, SetRateArgs { currency: "us dollar".into(), rate: "36.5".into(), date: None }).await.unwrap();
    assert_eq!(e.rates.rates.lock().unwrap()[0].currency, "USD");
    e.svc.set_limit(&c, SetLimitArgs { category: "кафе".into(), amount: "3000".into(), ..Default::default() }).await.unwrap();
    assert_eq!(e.limits.limits.lock().unwrap()[0].category_code, "CAFE");
    // Доходную категорию в расходный план не подставить.
    let bad = AddPlanArgs { name: "x".into(), kind: "expense".into(), account: "CARD".into(), category: Some("Salary".into()), amount: "1".into(), recurrence: "once".into(), ..Default::default() };
    assert!(e.svc.add_plan(&c, bad).await.unwrap_err().0.contains("not found"));
}

#[tokio::test]
async fn rate_is_upserted_by_currency_and_date_and_names_the_base_currency() {
    let e = env();
    e.grant(&["read", "rates:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let set = |rate: &str, date: &str| SetRateArgs { currency: "Доллар".into(), rate: rate.into(), date: Some(date.into()) };
    let out = e.svc.set_rate(&c, set("36.5", "2026-10-01")).await.unwrap();
    assert!(out.summary.contains("1 USD = 36.5 THB") && out.summary.contains("base currency"), "{}", out.summary);
    assert_eq!(out.data["base_currency"], "THB");
    let out = e.svc.set_rate(&c, set("36.7", "2026-10-01")).await.unwrap();
    assert!(out.summary.contains("Replaced the previous rate 36.5"), "{}", out.summary);
    assert_eq!(e.rates.rates.lock().unwrap().len(), 1, "та же дата — тот же курс");
    assert_eq!(e.rates.rates.lock().unwrap()[0].rate, d("36.7"));
    // Перевёрнутый курс (THB за USD вместо USD → THB) — предупреждение о направлении.
    let out = e.svc.set_rate(&c, set("0.0272", "2026-10-02")).await.unwrap();
    assert!(out.summary.contains("Check the direction"), "{}", out.summary);
    assert_eq!(e.rates.rates.lock().unwrap().len(), 2);
    let err = e.svc.set_rate(&c, SetRateArgs { currency: "THB".into(), rate: "1".into(), date: None }).await.unwrap_err();
    assert!(err.0.contains("base currency"), "{}", err.0);
    assert!(e.svc.set_rate(&c, set("-1", "2026-10-03")).await.is_err(), "курс > 0");
    let list = e.svc.list_rates(&c, ListRatesArgs { currency: Some("USD".into()), ..Default::default() }).await.unwrap();
    assert_eq!(list.data["total"], 2);
    assert_eq!(list.data["latest"][0]["date"], "2026-10-02");
}

#[tokio::test]
async fn limit_is_upserted_by_category_period_and_valid_from() {
    let e = env();
    e.grant(&["read", "limits:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e.svc.set_limit(&c, SetLimitArgs { category: "Food".into(), amount: "12000".into(), ..Default::default() }).await.unwrap();
    let month_start = period_start("MONTH", today());
    assert_eq!(e.limits.limits.lock().unwrap()[0].valid_from, month_start, "по умолчанию — с начала текущего месяца");
    assert!(out.summary.contains("12000 THB per month"), "{}", out.summary);
    let out = e.svc.set_limit(&c, SetLimitArgs { category: "FOOD".into(), amount: "15000".into(), period: Some("month".into()), valid_from: Some(month_start.to_string()) }).await.unwrap();
    assert!(out.summary.contains("Replaced 12000"), "{}", out.summary);
    assert_eq!(e.limits.limits.lock().unwrap().len(), 1);
    assert_eq!(e.limits.limits.lock().unwrap()[0].amount, d("15000"));
    // Недельный лимит с явной даты — новое правило; более позднее правило упомянуто в итоге.
    e.svc.set_limit(&c, SetLimitArgs { category: "FOOD".into(), amount: "3000".into(), period: Some("week".into()), valid_from: Some("2099-01-05".into()) }).await.unwrap();
    let out = e.svc.set_limit(&c, SetLimitArgs { category: "FOOD".into(), amount: "16000".into(), period: None, valid_from: Some(month_start.to_string()) }).await.unwrap();
    assert!(out.summary.contains("later rule from 2099-01-05"), "{}", out.summary);
    assert_eq!(e.limits.limits.lock().unwrap().len(), 2);
    let list = e.svc.list_limits(&c).await.unwrap();
    let rows = list.data["limits"].as_array().unwrap().clone();
    assert_eq!(rows.iter().filter(|r| r["current"] == true).count(), 1, "действует одно правило категории");
    assert_eq!(rows.iter().find(|r| r["current"] == true).unwrap()["amount"], "16000");
    assert!(e.svc.set_limit(&c, SetLimitArgs { category: "FOOD".into(), amount: "100".into(), period: Some("day".into()), valid_from: None }).await.is_err());
}

#[test]
fn period_start_of_week_month_year() {
    let day = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap(); // пятница
    assert_eq!(period_start("WEEK", day), NaiveDate::from_ymd_opt(2026, 10, 12).unwrap());
    assert_eq!(period_start("MONTH", day), NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
    assert_eq!(period_start("YEAR", day), NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
}

#[tokio::test]
async fn update_never_archives_and_changes_only_passed_fields() {
    let e = env();
    e.grant(&["read", "directories"]).await;
    let c = e.caller(&e.oauth()).await;
    // Ассистент прислал archived: true — поле неизвестно инструменту и игнорируется.
    let args: UpdateAccountArgs = serde_json::from_value(serde_json::json!({"code": "Карта", "archived": true, "kind": "card"})).unwrap();
    e.svc.update_account(&c, args).await.unwrap();
    let card = e.dirs.accounts.lock().unwrap().iter().find(|a| a.code == "CARD").cloned().unwrap();
    assert_eq!((card.kind.as_str(), card.archived, card.name.text.as_str()), ("CARD", false, "Карта"));
    // Архивная валюта по точному коду меняется, но остаётся в архиве.
    e.svc.update_currency(&c, UpdateCurrencyArgs { code: "EUR".into(), symbol: Some("EUR".into()), ..Default::default() }).await.unwrap();
    let eur = e.dirs.currencies.lock().unwrap().iter().find(|x| x.code == "EUR").cloned().unwrap();
    assert!(eur.archived && eur.symbol == "EUR");
    // Новое название — старые переводы уходят, если не переданы новые; без названия переводы меняются по одному.
    let tr = |en: &str| Some(NameTranslations { en: Some(en.into()), ..Default::default() });
    e.svc.update_currency(&c, UpdateCurrencyArgs { code: "USD".into(), translations: tr("U.S. dollar"), ..Default::default() }).await.unwrap();
    let usd = e.dirs.currencies.lock().unwrap().iter().find(|x| x.code == "USD").cloned().unwrap();
    assert_eq!((usd.name.text.as_str(), usd.name.en.as_deref(), usd.name.th.as_deref()), ("Доллар США", Some("U.S. dollar"), Some("ดอลลาร์สหรัฐ")));
    e.svc.update_currency(&c, UpdateCurrencyArgs { code: "USD".into(), name: Some("Доллар".into()), ..Default::default() }).await.unwrap();
    let usd = e.dirs.currencies.lock().unwrap().iter().find(|x| x.code == "USD").cloned().unwrap();
    assert_eq!((usd.name.text.as_str(), usd.name.en.as_deref()), ("Доллар", None));
    // Перенос категории под другого родителя и в корень.
    let out = e.svc.update_category(&c, UpdateCategoryArgs { code: "Мойка".into(), parent: Some("Food".into()), ..Default::default() }).await.unwrap();
    assert_eq!(out.data["category"]["parent"], "FOOD");
    assert!(out.summary.contains("Moved under FOOD"), "{}", out.summary);
    let out = e.svc.update_category(&c, UpdateCategoryArgs { code: "CARWASH".into(), parent: Some(String::new()), ..Default::default() }).await.unwrap();
    assert_eq!(out.data["category"]["parent"], serde_json::Value::Null);
    assert!(e.dirs.categories.lock().unwrap().iter().all(|c| !c.archived));
    let err = e.svc.update_category(&c, UpdateCategoryArgs { code: "CAFE".into(), parent: Some("Salary".into()), ..Default::default() }).await.unwrap_err();
    assert!(err.0.contains("SALARY is an income category"), "родитель другого вида: {}", err.0);
    assert!(e.svc.update_category(&c, UpdateCategoryArgs { code: "CAFE".into(), parent: Some("CAFE".into()), ..Default::default() }).await.is_err(), "сама себе не родитель");
    assert!(e.svc.update_currency(&c, UpdateCurrencyArgs { code: "USD".into(), ..Default::default() }).await.unwrap_err().0.contains("nothing to change"));
}

#[tokio::test]
async fn opening_balance_on_update_requires_transactions_write() {
    let e = env();
    e.grant(&["read", "accounts:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let err = e.svc.update_account(&c, UpdateAccountArgs { code: "CASH".into(), opening_balance: Some("100".into()), ..Default::default() }).await.unwrap_err();
    assert!(err.0.contains("transactions:write"), "{}", err.0);
    e.grant(&["read", "accounts:write", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e.svc.update_account(&c, UpdateAccountArgs { code: "CASH".into(), opening_balance: Some("-250".into()), opening_date: Some("2026-01-01".into()), ..Default::default() }).await.unwrap();
    assert!(out.summary.contains("-250 THB on 2026-01-01"), "{}", out.summary);
    assert_eq!(e.dirs.openings.lock().unwrap()[0], ("CASH".to_string(), d("-250"), NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()));
    assert!(e.svc.update_account(&c, UpdateAccountArgs { code: "CASH".into(), opening_date: Some("2026-01-01".into()), ..Default::default() }).await.is_err(), "дата без суммы");
}

#[tokio::test]
async fn plans_are_added_paused_and_validated() {
    let e = env();
    e.grant(&["read", "planning:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let rent = AddPlanArgs {
        name: "Аренда".into(),
        translations: Some(NameTranslations { en: Some("Rent".into()), th: Some("ค่าเช่า".into()), ru: None }),
        kind: "expense".into(),
        account: "CARD".into(),
        category: Some("CAR".into()),
        amount: "15000".into(),
        recurrence: "monthly".into(),
        start_on: Some("2026-11-01".into()),
        ..Default::default()
    };
    let out = e.svc.add_plan(&c, rent).await.unwrap();
    assert!(out.summary.contains("every month") && out.summary.contains("15000 THB"), "{}", out.summary);
    let id = e.plans.plans.lock().unwrap()[0].id;
    let out = e.svc.update_plan(&c, UpdatePlanArgs { id: id.to_string(), active: Some(false), amount: Some("16000".into()), ..Default::default() }).await.unwrap();
    assert!(out.summary.contains("paused"), "{}", out.summary);
    let p = e.plans.plans.lock().unwrap()[0].clone();
    assert_eq!((p.draft.active, p.draft.amount, p.draft.category_code.as_deref(), p.draft.name.en.as_deref()), (false, d("16000"), Some("CAR"), Some("Rent")));
    let list = e.svc.list_plans(&c).await.unwrap();
    assert_eq!(list.data["plans"][0]["active"], false);
    let transfer = AddPlanArgs { name: "Копилка".into(), kind: "transfer".into(), account: "CARD".into(), amount: "1000".into(), recurrence: "monthly".into(), ..Default::default() };
    assert!(e.svc.add_plan(&c, AddPlanArgs { category: Some("FOOD".into()), ..transfer }).await.unwrap_err().0.contains("transfers have no category"));
    let transfer = AddPlanArgs { name: "Копилка".into(), kind: "transfer".into(), account: "CARD".into(), amount: "1000".into(), recurrence: "monthly".into(), ..Default::default() };
    assert!(e.svc.add_plan(&c, transfer).await.unwrap_err().0.contains("to_account"));
    assert!(e.svc.update_plan(&c, UpdatePlanArgs { id: id.to_string(), to_account: Some("CASH".into()), ..Default::default() }).await.is_err());
    assert!(e.svc.update_plan(&c, UpdatePlanArgs { id: Uuid::new_v4().to_string(), active: Some(true), ..Default::default() }).await.unwrap_err().0.contains("not found"));
    let writes: Vec<String> = e.accounts.events.lock().unwrap().iter().filter(|x| x.0 == "mcp_write").filter_map(|x| x.1.iter().find(|d| d.0 == "tool").map(|d| d.1.clone())).collect();
    assert_eq!(writes, vec!["add_plan", "update_plan"], "каждое изменение — в журнал");
}

#[tokio::test]
async fn context_lists_currencies_and_all_permissions() {
    let e = env();
    e.grant(&["read", "transactions:write", "rates:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let ctx = e.svc.get_context(&c).await.unwrap();
    let codes: Vec<&str> = ctx.data["currencies"].as_array().unwrap().iter().map(|x| x["code"].as_str().unwrap()).collect();
    assert_eq!(codes, vec!["THB", "USD"], "архивные не показываются");
    assert_eq!(ctx.data["currencies"][0]["name"], "Тайский бат");
    assert_eq!(ctx.data["currencies"][0]["symbol"], "฿");
    let p = &ctx.data["permissions"];
    for (k, v) in [("write_rates", true), ("write_accounts", false), ("write_currencies", false), ("write_categories", false), ("write_limits", false), ("write_planning", false), ("delete", false)] {
        assert_eq!(p[k], v, "{k}");
    }
}

#[test]
fn grant_scopes_have_labels_and_groups() {
    let label = |key: &str| crate::i18n::dict(key).unwrap_or_else(|| panic!("нет перевода {key}"));
    for s in GRANT_SCOPES {
        label(&format!("scope.{s}"));
        let g = scope_group(s);
        if g != "base" {
            label(&format!("group.{g}"));
        }
    }
    label("scope.directories");
    assert!(DIRECTORY_SCOPES.iter().all(|s| GRANT_SCOPES.contains(s) && scope_group(s) == "directories"));
    assert_eq!((scope_group("limits:write"), scope_group("planning:write"), scope_group("transactions:write")), ("limits", "planning", "base"));
}

#[test]
fn templates_and_pages_have_translations() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let missing = kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет переводов: {missing:?}");
}

#[test]
fn reading_tools_are_read_only_and_edits_ask_for_confirmation() {
    for (name, read_only, destructive) in crate::adapters::server::tool_hints() {
        if name.starts_with("list_") || name.starts_with("get_") {
            assert!(read_only && !destructive, "{name}: только чтение");
        } else if name.starts_with("update_") {
            assert!(!read_only && destructive, "{name}: Claude спрашивает подтверждение");
        } else {
            assert!(!read_only && !destructive, "{name}: запись без лишних подтверждений");
        }
    }
}

// ---------------------------------------------------------------- инварианты

#[test]
fn every_tool_except_context_needs_a_right_and_none_deletes() {
    let tools = crate::adapters::server::tool_names();
    assert!(tools.contains(&"get_context".to_string()));
    for t in &tools {
        assert!(t == "get_context" || tool_right(t).is_some(), "у инструмента {t} нет права");
        for banned in ["delete", "remove", "archive", "grant", "token", "permission", "member", "role"] {
            assert!(!t.contains(banned), "инструмент {t}: через MCP нельзя удалять и менять права");
        }
    }
}

#[test]
fn amounts_dates_and_lookup() {
    assert_eq!(parse_amount("amount", "120+45").unwrap(), d("165"));
    assert!(parse_amount("amount", "-5").is_err());
    let today = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
    assert_eq!(parse_date("date", Some("yesterday"), today).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 14).unwrap());
    assert_eq!(parse_date("date", None, today).unwrap(), today);
    let q = quick_info();
    let pick = |v: &str| resolve("category", v, &q.categories, |x| &x.code, |x| format!("{}\u{1f}{}", x.title, x.search)).map(|c| c.code.clone());
    assert_eq!(pick("кафе").unwrap(), "CAFE");
    assert_eq!(pick("Car").unwrap(), "CAR", "точный код важнее подстроки");
    assert!(pick("รถ").is_err(), "неоднозначно: Машина (รถ) и Мойка (ล้างรถ)");
    let _ = Utc::now();
}
