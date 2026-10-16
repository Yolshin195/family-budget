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
use crate::ports::{Accounts, Budget, Deps, Ledger, Plans};
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
    accounts: Arc<FakeAccounts>,
    members: Arc<FakeMemberships>,
    user: Uuid,
    project: Uuid,
}

fn env() -> Env {
    let (ledger, plans, accounts, members) = (Arc::new(FakeLedger::default()), Arc::new(FakePlans::default()), Arc::new(FakeAccounts::default()), Arc::new(FakeMemberships::default()));
    let (user, project) = (Uuid::new_v4(), Uuid::new_v4());
    members.members.lock().unwrap().push((user, project, Role::Owner, false));
    let deps = Deps {
        ledger: Arc::new(ledger.clone()),
        budget: Arc::new(FakeBudget),
        plans: Arc::new(plans.clone()),
        accounts: Arc::new(accounts.clone()),
        authenticator: Arc::new(FakeAuthenticator),
        memberships: members.clone(),
    };
    let settings = Settings { base_url: "https://budget.test".into(), resource: RESOURCE.into(), oauth_client_id: "claude".into(), oauth_scope: "openid".into(), rate_limit: 600 };
    let svc = McpService::new(Arc::new(InMemoryGrantRepository::default()), deps, settings);
    Env { svc, ledger, plans, accounts, members, user, project }
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
