//! Тесты use cases фичи «MCP для ассистентов»: вход по токенам, доступы ассистентов, права в
//! инструментах (чтение / запись / роль / 2FA), запись операций. Без БД: memory-репозиторий и фейковые порты.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use kernel::auth::{
    Authenticator, Credential, Identity, Memberships, ProjectRef, RequestMeta, Role, SessionInfo, Theme, Via as AuthVia,
};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::adapters::memory::InMemoryGrantRepository;
use crate::domain::*;
use crate::dto::*;
use crate::ports::{Accounts, Budget, Deps, Directories, Ledger, Limits, Plans, Rates};
use crate::service::{Assistant, Auth, McpService, Settings, tool_right};

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
    let cat = |code: &str, parent: Option<&str>, kind: &str, title: &str, names: &[&str]| CategoryInfo {
        code: code.into(),
        parent_code: parent.map(Into::into),
        kind: kind.into(),
        title: title.into(),
        depth: usize::from(parent.is_some()),
        search: kernel::search::join(names.iter().copied().chain([code])),
        limit: (code == "FOOD").then(|| d("1000")),
        available: (code == "FOOD" || code == "CAFE").then(|| d("700")),
    };
    QuickInfo {
        default_account: Some("CASH".into()),
        accounts: vec![
            AccountInfo {
                code: "CASH".into(),
                title: "Наличные".into(),
                currency: "THB".into(),
                symbol: "฿".into(),
                has_rate: true,
            },
            AccountInfo {
                code: "CARD".into(),
                title: "Карта".into(),
                currency: "THB".into(),
                symbol: "฿".into(),
                has_rate: true,
            },
            AccountInfo {
                code: "WISE".into(),
                title: "Wise".into(),
                currency: "USD".into(),
                symbol: "$".into(),
                has_rate: false,
            },
        ],
        categories: vec![
            cat("FOOD", None, "EXPENSE", "Еда", &["Еда", "Food", "อาหาร"]),
            cat("CAFE", Some("FOOD"), "EXPENSE", "Кафе", &["Кафе", "Café", "คาเฟ่"]),
            cat("CAR", None, "EXPENSE", "Машина", &["Машина", "Car", "รถ"]),
            cat("CARWASH", Some("CAR"), "EXPENSE", "Мойка", &["Мойка", "Car wash", "ล้างรถ"]),
            cat("SALARY", None, "INCOME", "Зарплата", &["Зарплата", "Salary", "เงินเดือน"]),
        ],
        frequent_expense: vec!["CAFE".into()],
        frequent_income: vec![],
        places: vec![
            PlaceInfo {
                code: "SEVEN_ELEVEN".into(),
                title: "7-Eleven · Сукхумвит 24".into(),
                search: kernel::search::join(["7-Eleven", "7-11", "เซเว่น", "SEVEN_ELEVEN"]),
                category_code: Some("FOOD".into()),
                account_code: Some("CARD".into()),
            },
            PlaceInfo {
                code: "MAKRO".into(),
                title: "Makro".into(),
                search: kernel::search::join(["Makro", "แม็คโคร", "MAKRO"]),
                category_code: None,
                account_code: None,
            },
        ],
        frequent_places: vec!["SEVEN_ELEVEN".into()],
    }
}

/// Позиции после правки — упрощённо, как в transactions: с id — заменить заданные поля, без id — добавить.
fn items_of(old: Option<&Tx>, d: &TxDraft) -> Vec<TxItem> {
    let mut items: Vec<TxItem> = match old {
        Some(t) if !d.replace_items => {
            t.items.iter().filter(|x| !d.remove_item_ids.contains(&x.id.unwrap_or_default())).cloned().collect()
        }
        _ => Vec::new(),
    };
    // Для кого — как в transactions: шапка достаётся позициям с прежним общим получателем и новым без своего.
    let old_common = old.and_then(|t| t.for_person_code.clone());
    if let Some(h) = &d.for_person {
        for i in items.iter_mut().filter(|i| i.for_person.clone().flatten() == old_common) {
            i.for_person = Some(h.clone());
        }
    }
    for x in &d.items {
        match items.iter_mut().find(|i| x.id.is_some() && i.id == x.id) {
            Some(i) => {
                i.amount = x.amount.or(i.amount);
                i.category_code = x.category_code.clone().or(i.category_code.clone());
                i.name = x.name.clone().or(i.name.clone());
                i.for_person = x.for_person.clone().or(i.for_person.clone());
            }
            None => items.push(TxItem {
                id: Some(Uuid::new_v4()),
                quantity: x.quantity.or(Some(Decimal::ONE)),
                for_person: Some(x.for_person.clone().or(d.for_person.clone()).unwrap_or(old_common.clone())),
                ..x.clone()
            }),
        }
    }
    items
}

fn tx_of(id: Uuid, user: Uuid, d: &TxDraft, old: Option<&Tx>) -> Tx {
    let items = items_of(old, d);
    let total =
        if d.kind == "TRANSFER" { d.amount.unwrap_or_default() } else { items.iter().filter_map(|x| x.amount).sum() };
    Tx {
        id,
        occurred_on: d.occurred_on,
        kind: d.kind.clone(),
        account_code: d.account_code.clone(),
        to_account_code: d.to_account_code.clone(),
        refund: d.refund,
        total,
        currency: if d.account_code == "WISE" { "USD" } else { "THB" }.into(),
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
        place_code: d.place_code.clone(),
        for_person_code: items
            .first()
            .and_then(|f| f.for_person.clone().flatten())
            .filter(|c| items.iter().all(|x| x.for_person.clone().flatten().as_ref() == Some(c))),
        recipients_differ: items.iter().any(|x| x.for_person != items[0].for_person),
        // Правка связи с внешними записями не меняет.
        external_refs: old.map(|o| o.external_refs.clone()).unwrap_or_default(),
        items,
    }
}

#[async_trait]
impl Ledger for Arc<FakeLedger> {
    async fn quick(&self, _p: Uuid, _u: Uuid, _base: &str, _lang: Lang, _date: NaiveDate) -> AppResult<QuickInfo> {
        Ok(quick_info())
    }
    async fn list(&self, _p: Uuid, filter: TxFilter, limit: i64, offset: i64) -> AppResult<(Vec<Tx>, i64)> {
        let all: Vec<Tx> = self
            .txs
            .lock()
            .unwrap()
            .iter()
            .filter(|t| {
                filter.kind.as_ref().is_none_or(|k| &t.kind == k)
                    && filter.place_code.as_ref().is_none_or(|p| t.place_code.as_ref() == Some(p))
            })
            .filter(|t| {
                filter.for_person.as_ref().is_none_or(|who| t.items.iter().any(|x| x.for_person.as_ref() == Some(who)))
            })
            .filter(|t| {
                filter.external_provider.as_ref().is_none_or(|p| t.external_refs.iter().any(|r| &r.provider == p))
            })
            .cloned()
            .collect();
        let total = all.len() as i64;
        Ok((all.into_iter().skip(offset as usize).take(limit as usize).collect(), total))
    }
    async fn get(&self, _p: Uuid, id: Uuid) -> AppResult<Tx> {
        self.txs
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound("transaction".into()))
    }
    async fn create(&self, _p: Uuid, user: Uuid, draft: TxDraft) -> AppResult<Tx> {
        let tx = tx_of(Uuid::new_v4(), user, &draft, None);
        self.txs.lock().unwrap().push(tx.clone());
        Ok(tx)
    }
    async fn update(&self, _p: Uuid, id: Uuid, draft: TxDraft) -> AppResult<Tx> {
        let mut txs = self.txs.lock().unwrap();
        let t = txs.iter_mut().find(|t| t.id == id).ok_or_else(|| AppError::NotFound("transaction".into()))?;
        let old = t.clone();
        *t = tx_of(id, t.created_by, &draft, Some(&old));
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
    async fn occurrences(
        &self,
        _p: Uuid,
        from: NaiveDate,
        _to: NaiveDate,
        _today: NaiveDate,
        _lang: Lang,
    ) -> AppResult<Vec<Occurrence>> {
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
    places: Mutex<Vec<Place>>,
    people: Mutex<Vec<Person>>,
    openings: Mutex<Vec<(String, Decimal, NaiveDate)>>,
}

impl Default for FakeDirs {
    fn default() -> Self {
        let cur = |code: &str, n: Name, symbol: &str, archived: bool| Currency {
            code: code.into(),
            name: n,
            symbol: symbol.into(),
            minor_units: 2,
            archived,
        };
        let acc = |code: &str, n: Name| Account {
            code: code.into(),
            name: n,
            kind: "CASH".into(),
            currency: "THB".into(),
            holder_user_id: None,
            archived: false,
        };
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
            accounts: Mutex::new(vec![
                acc("CASH", name("Наличные", "Наличные", "Cash", "เงินสด")),
                acc("CARD", name("Карта", "Карта", "Card", "บัตร")),
            ]),
            categories: Mutex::new(vec![
                cat("FOOD", None, "EXPENSE", name("Еда", "Еда", "Food", "อาหาร"), 10),
                cat("CAFE", Some("FOOD"), "EXPENSE", name("Кафе", "Кафе", "Cafe", "คาเฟ่"), 10),
                cat("CAR", None, "EXPENSE", name("Машина", "Машина", "Car", "รถ"), 20),
                cat("CARWASH", Some("CAR"), "EXPENSE", name("Мойка", "Мойка", "Car wash", "ล้างรถ"), 10),
                cat("SALARY", None, "INCOME", name("Зарплата", "Зарплата", "Salary", "เงินเดือน"), 10),
            ]),
            places: Mutex::new(vec![
                Place {
                    code: "SEVEN_ELEVEN".into(),
                    name: "7-Eleven".into(),
                    aliases: vec!["7-11".into(), "เซเว่น".into()],
                    category_code: Some("FOOD".into()),
                    account_code: Some("CARD".into()),
                    kind: "SHOP".into(),
                    location: Some("Сукхумвит 24".into()),
                    ..Default::default()
                },
                Place {
                    code: "OLD".into(),
                    name: "Old market".into(),
                    kind: "MARKET".into(),
                    archived: true,
                    ..Default::default()
                },
            ]),
            people: Mutex::new(vec![
                Person {
                    code: "PINPAN".into(),
                    name: "Pinpan".into(),
                    aliases: vec!["ปิ่นปั่น".into()],
                    user_id: None,
                    former_member: false,
                    archived: false,
                },
                Person {
                    code: "NANNY".into(),
                    name: "Old nanny".into(),
                    aliases: vec![],
                    user_id: None,
                    former_member: false,
                    archived: true,
                },
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
    async fn places(&self, _p: Uuid) -> AppResult<Vec<Place>> {
        Ok(self.places.lock().unwrap().clone())
    }
    async fn create_place(&self, _p: Uuid, x: &Place) -> AppResult<Place> {
        self.places.lock().unwrap().push(x.clone());
        Ok(x.clone())
    }
    async fn update_place(&self, _p: Uuid, x: &Place) -> AppResult<Place> {
        upsert(&self.places, x, |y| y.code == x.code)
    }
    async fn people(&self, _p: Uuid) -> AppResult<Vec<Person>> {
        Ok(self.people.lock().unwrap().clone())
    }
    async fn create_person(&self, _p: Uuid, x: &Person) -> AppResult<Person> {
        self.people.lock().unwrap().push(x.clone());
        Ok(x.clone())
    }
    async fn update_person(&self, _p: Uuid, x: &Person) -> AppResult<Person> {
        upsert(&self.people, x, |y| y.code == x.code)
    }
}

#[derive(Default)]
struct FakeRates {
    rates: Mutex<Vec<Rate>>,
}

#[async_trait]
impl Rates for Arc<FakeRates> {
    async fn list(&self, _p: Uuid, from: Option<NaiveDate>, to: Option<NaiveDate>) -> AppResult<Vec<Rate>> {
        let mut list: Vec<Rate> = self
            .rates
            .lock()
            .unwrap()
            .iter()
            .filter(|r| from.is_none_or(|f| r.date >= f) && to.is_none_or(|t| r.date <= t))
            .cloned()
            .collect();
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
    async fn record(
        &self,
        _user: Uuid,
        _project: Option<Uuid>,
        action: &str,
        _meta: &RequestMeta,
        details: &[(&str, String)],
    ) {
        self.events
            .lock()
            .unwrap()
            .push((action.to_string(), details.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()));
    }
}

/// Личные токены: `fbt_<user>_<project>_<scope,...>`; с хвостом `_nomfa` — выпущен из сессии без 2FA.
struct FakeAuthenticator;

#[async_trait]
impl Authenticator for FakeAuthenticator {
    async fn authenticate(&self, credential: &Credential) -> AppResult<Option<Identity>> {
        let Credential::ApiToken(raw) = credential else { return Ok(None) };
        let parts: Vec<&str> = raw.trim_start_matches("fbt_").split('_').collect();
        let (user, project, scopes, mfa) = match parts[..] {
            [user, project, scopes] => (user, project, scopes, true),
            [user, project, scopes, "nomfa"] => (user, project, scopes, false),
            _ => return Ok(None),
        };
        Ok(Some(Identity {
            user_id: Uuid::parse_str(user).unwrap(),
            mfa,
            via: AuthVia::Token {
                token_id: Uuid::new_v4(),
                project_id: Uuid::parse_str(project).unwrap(),
                scopes: scopes.split(',').map(String::from).collect(),
            },
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
            currency_marks: Default::default(),
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

fn assistants() -> Vec<Assistant> {
    vec![
        Assistant { key: "claude", name: "Claude", client_id: "claude".into(), client_secret: None },
        Assistant {
            key: "chatgpt",
            name: "ChatGPT",
            client_id: "chatgpt".into(),
            client_secret: Some("s3cret".into()),
        },
    ]
}

fn env() -> Env {
    let (ledger, plans, accounts, members) = (
        Arc::new(FakeLedger::default()),
        Arc::new(FakePlans::default()),
        Arc::new(FakeAccounts::default()),
        Arc::new(FakeMemberships::default()),
    );
    let (dirs, rates, limits) =
        (Arc::new(FakeDirs::default()), Arc::new(FakeRates::default()), Arc::new(FakeLimits::default()));
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
    let settings = Settings {
        base_url: "https://budget.test".into(),
        resource: RESOURCE.into(),
        assistants: assistants(),
        oauth_scope: "openid".into(),
        rate_limit: 600,
    };
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
        self.svc
            .save_grant(self.user, "claude", self.project, Role::Owner, false, false, &scopes, &RequestMeta::default())
            .await
            .unwrap();
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

/// Б1: токен, выпущенный до «Требовать 2FA» из сессии без второго фактора, больше не открывает проект.
#[tokio::test]
async fn token_without_mfa_stops_working_when_project_requires_2fa() {
    let e = env();
    let without = format!("fbt_{}_{}_read_nomfa", e.user, e.project);
    let with = format!("fbt_{}_{}_read", e.user, e.project);
    assert!(
        matches!(e.svc.authenticate(Some(&without), RequestMeta::default()).await.unwrap(), Auth::Ok(_)),
        "2FA не требуется — работает"
    );
    e.set_role(Role::Owner, true);
    assert!(matches!(e.svc.authenticate(Some(&without), RequestMeta::default()).await.unwrap(), Auth::Invalid));
    assert!(
        matches!(e.svc.authenticate(Some(&with), RequestMeta::default()).await.unwrap(), Auth::Ok(_)),
        "токен из сессии с 2FA"
    );
}

#[tokio::test]
async fn missing_and_foreign_tokens_are_rejected() {
    let e = env();
    assert!(matches!(e.svc.authenticate(None, RequestMeta::default()).await.unwrap(), Auth::Missing));
    let foreign = format!("at:{}:claude:https://other.test/mcp", e.user);
    assert!(
        matches!(e.svc.authenticate(Some(&foreign), RequestMeta::default()).await.unwrap(), Auth::Invalid),
        "токен для другого ресурса"
    );
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
    let err = e
        .svc
        .skip_planned(&c, SkipPlannedArgs { plan_id: plan().to_string(), date: "2026-10-20".into() })
        .await
        .unwrap_err();
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
    let g = e
        .svc
        .save_grant(e.user, "claude", e.project, Role::Viewer, false, false, &scopes, &RequestMeta::default())
        .await
        .unwrap();
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
    let err = e
        .svc
        .save_grant(e.user, "claude", e.project, Role::Owner, true, false, &scopes, &RequestMeta::default())
        .await
        .unwrap_err();
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

/// У каждого ассистента свой OAuth-клиент и свой доступ: доступ Claude не открывает проект ChatGPT,
/// у ChatGPT могут быть другие проект и права, а отключение одного не трогает другого.
#[tokio::test]
async fn each_assistant_has_its_own_grant() {
    let e = env();
    let gpt = format!("at:{}:chatgpt:{RESOURCE}", e.user);
    e.grant(&["read", "transactions:write"]).await;
    assert!(
        matches!(e.caller(&gpt).await.via, Via::Unconfigured { reason: Unconfigured::NoGrant, .. }),
        "доступ Claude — не доступ ChatGPT"
    );

    let other = Uuid::new_v4();
    e.members.members.lock().unwrap().push((e.user, other, Role::Owner, false));
    e.svc
        .save_grant(e.user, "chatgpt", other, Role::Owner, false, false, &["read".to_string()], &RequestMeta::default())
        .await
        .unwrap();
    let c = e.caller(&gpt).await;
    assert_eq!((c.project_id, c.client()), (other, "chatgpt"));
    assert!(c.allows(Right::read("transactions")) && !c.allows(Right::write("transactions")));
    let claude = e.caller(&e.oauth()).await;
    assert_eq!(claude.project_id, e.project);
    assert!(claude.allows(Right::write("transactions")));

    e.svc.revoke_grant(e.user, "chatgpt", &RequestMeta::default()).await.unwrap();
    assert!(matches!(e.caller(&gpt).await.via, Via::Unconfigured { .. }));
    assert!(matches!(e.caller(&e.oauth()).await.via, Via::Grant { .. }), "Claude по-прежнему подключён");
}

/// Подсказка без доступа ведёт сразу на вкладку своего ассистента.
#[tokio::test]
async fn setup_hint_opens_the_assistant_tab() {
    let e = env();
    let c = e.caller(&format!("at:{}:chatgpt:{RESOURCE}", e.user)).await;
    let ctx = e.svc.get_context(&c).await.unwrap();
    assert!(ctx.summary.contains("https://budget.test/assistants?assistant=chatgpt"), "{}", ctx.summary);
    assert_eq!(ctx.data["setup_url"], "https://budget.test/assistants?assistant=chatgpt");
    let c = e.caller(&e.oauth()).await;
    assert!(e.svc.get_context(&c).await.unwrap().summary.contains("/assistants?assistant=claude"));
}

#[tokio::test]
async fn demo_guest_cannot_grant_access() {
    let e = env();
    *e.accounts.guest.lock().unwrap() = Some(e.user);
    let err = e
        .svc
        .save_grant(e.user, "claude", e.project, Role::Owner, false, false, &[], &RequestMeta::default())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

// ---------------------------------------------------------------- инструменты

/// Группа 1, шаг 2: чек с названиями, количеством и ценой; правка позиции по id, удаление по remove_item_ids.
#[tokio::test]
async fn receipt_items_with_prices_and_edit_by_id() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let items = vec![
        ItemArgs {
            category: Some("Кафе".into()),
            name: Some("Круассан".into()),
            quantity: Some("2".into()),
            price: Some("25".into()),
            discount: Some("3".into()),
            ..Default::default()
        },
        ItemArgs {
            category: Some("FOOD".into()),
            name: Some("Бананы".into()),
            quantity: Some("0,45".into()),
            unit: Some("kg".into()),
            amount: Some("27".into()),
            ..Default::default()
        },
    ];
    let out = e
        .svc
        .add_expense(&c, AddFlowArgs { items: Some(items), discount_total: Some("5".into()), ..Default::default() })
        .await
        .unwrap();
    let tx = e.ledger.txs.lock().unwrap()[0].clone();
    let first = &tx.items[0];
    assert_eq!(
        (first.category_code.as_deref(), first.name.as_deref(), first.quantity, first.price, first.discount),
        (Some("CAFE"), Some("Круассан"), Some(d("2")), Some(d("25")), Some(d("3")))
    );
    assert_eq!((tx.items[1].quantity, tx.items[1].unit.as_deref()), (Some(d("0.45")), Some("kg")));
    assert!(out.data["transaction"]["items"][0]["id"].is_string(), "id позиции — для правки");
    assert_eq!(out.data["transaction"]["items"][0]["price"], serde_json::json!(d("25")));
    let (keep, drop) = (tx.items[0].id.unwrap(), tx.items[1].id.unwrap());
    let edit = UpdateTransactionArgs {
        id: tx.id.to_string(),
        items: Some(vec![ItemArgs { id: Some(keep.to_string()), amount: Some("40".into()), ..Default::default() }]),
        remove_item_ids: Some(vec![drop.to_string()]),
        ..Default::default()
    };
    e.svc.update_transaction(&c, edit).await.unwrap();
    let tx = e.ledger.txs.lock().unwrap()[0].clone();
    assert_eq!(tx.items.iter().map(|x| (x.id, x.amount)).collect::<Vec<_>>(), vec![(Some(keep), Some(d("40")))]);
    // Сумма и категория без items — только у операции из одной позиции.
    let single = UpdateTransactionArgs { id: tx.id.to_string(), amount: Some("41".into()), ..Default::default() };
    e.svc.update_transaction(&c, single).await.unwrap();
    assert_eq!(e.ledger.txs.lock().unwrap()[0].items[0].amount, Some(d("41")));
    let bad =
        UpdateTransactionArgs { id: tx.id.to_string(), remove_item_ids: Some(vec!["x".into()]), ..Default::default() };
    assert!(e.svc.update_transaction(&c, bad).await.unwrap_err().0.contains("remove_item_ids"));
}

#[tokio::test]
async fn expense_by_name_in_any_language_uses_default_account_and_is_audited() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e
        .svc
        .add_expense(&c, AddFlowArgs { description: Some("Кофе".into()), ..expense("120+45", "cafe") })
        .await
        .unwrap();
    let tx = e.ledger.txs.lock().unwrap()[0].clone();
    assert_eq!(
        (tx.account_code.as_str(), tx.items[0].category_code.as_deref().unwrap_or_default(), tx.total),
        ("CASH", "CAFE", d("165"))
    );
    assert_eq!(tx.description_lang, Lang::Ru, "язык описания — язык пользователя");
    assert!(out.summary.contains("Left this month"), "{}", out.summary);
    let thai = e.svc.add_expense(&c, expense("80", "ล้างรถ")).await.unwrap();
    assert_eq!(thai.data["transaction"]["items"][0]["category"], "CARWASH");
    let events = e.accounts.events.lock().unwrap().clone();
    let write = events.iter().find(|x| x.0 == "mcp_write").unwrap();
    assert!(write.1.contains(&("client".to_string(), "claude".to_string())));
}

#[tokio::test]
async fn ambiguous_or_unknown_category_asks_for_a_code() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let err = e.svc.add_expense(&c, expense("10", "ca")).await.unwrap_err();
    assert!(err.0.contains("ambiguous") && err.0.contains("CARWASH"), "{}", err.0);
    let err = e.svc.add_expense(&c, expense("10", "Salary")).await.unwrap_err();
    assert!(err.0.contains("not found"), "доходная категория для расхода не подходит: {}", err.0);
    assert!(e.ledger.txs.lock().unwrap().is_empty());
    // Техдолг #5: точное название на другом языке — не «неоднозначно», хотя «รถ» есть и в «ล้างรถ».
    e.svc.add_expense(&c, expense("10", "รถ")).await.unwrap();
    assert_eq!(e.ledger.txs.lock().unwrap()[0].items[0].category_code.as_deref(), Some("CAR"));
}

#[tokio::test]
async fn expense_with_items_and_conflicting_arguments() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let items = vec![
        ItemArgs { category: Some("CAFE".into()), amount: Some("100".into()), ..Default::default() },
        ItemArgs {
            category: Some("CARWASH".into()),
            amount: Some("50.5".into()),
            note: Some("мойка".into()),
            ..Default::default()
        },
    ];
    let out = e
        .svc
        .add_expense(&c, AddFlowArgs { items: Some(items), account: Some("Карта".into()), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(out.data["transaction"]["account"], "CARD");
    assert_eq!(e.ledger.txs.lock().unwrap()[0].total, d("150.5"));
    let both = AddFlowArgs {
        items: Some(vec![ItemArgs { category: Some("CAFE".into()), amount: Some("1".into()), ..Default::default() }]),
        ..expense("1", "CAFE")
    };
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
    e.svc
        .update_transaction(
            &c,
            UpdateTransactionArgs { id: id.to_string(), amount: Some("90".into()), ..Default::default() },
        )
        .await
        .unwrap();
    let tx = e.ledger.txs.lock().unwrap()[0].clone();
    assert_eq!(
        (tx.total, tx.items[0].category_code.as_deref().unwrap_or_default(), tx.description.as_deref()),
        (d("90"), "CAFE", Some("Кофе"))
    );
    let err = e
        .svc
        .update_transaction(
            &c,
            UpdateTransactionArgs { id: id.to_string(), to_amount: Some("5".into()), ..Default::default() },
        )
        .await
        .unwrap_err();
    assert!(err.0.contains("transfers"), "{}", err.0);
}

#[tokio::test]
async fn transfer_and_scheduled_payment() {
    let e = env();
    e.grant(&["read", "transactions:write", "planning:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let t = AddTransferArgs {
        from_account: "CARD".into(),
        to_account: "CASH".into(),
        amount: "1000".into(),
        ..Default::default()
    };
    let out = e.svc.add_transfer(&c, t).await.unwrap();
    assert_eq!(out.data["transaction"]["to_account"], "CASH");
    let paid = e
        .svc
        .pay_planned(
            &c,
            PayPlannedArgs { plan_id: plan().to_string(), date: "2026-10-20".into(), ..Default::default() },
        )
        .await
        .unwrap();
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
    assert_eq!(
        ctx.data["expense_categories"][3]["names"],
        serde_json::json!(["Мойка", "Car wash", "ล้างรถ", "CARWASH"]),
        "названия списком"
    );
}

#[tokio::test]
async fn list_filters_by_category_subtree_and_text() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    e.svc
        .add_expense(&c, AddFlowArgs { description: Some("Starbucks".into()), ..expense("100", "CAFE") })
        .await
        .unwrap();
    e.svc.add_expense(&c, expense("50", "CARWASH")).await.unwrap();
    let food = e
        .svc
        .list_transactions(
            &c,
            ListTransactionsArgs {
                category: Some("FOOD".into()),
                from: Some("2000-01-01".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(food.data["total"], 1, "FOOD включает подкатегорию CAFE");
    let text = e
        .svc
        .list_transactions(
            &c,
            ListTransactionsArgs {
                text: Some("starbucks".into()),
                from: Some("2000-01-01".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(text.data["total"], 1);
}

// ---------------------------------------------------------------- справочники, курсы, лимиты, планы

/// Все инструменты записи справочников, курсов, лимитов и планов.
const NEW_WRITE_TOOLS: [&str; 10] = [
    "add_currency",
    "update_currency",
    "add_account",
    "update_account",
    "add_category",
    "update_category",
    "set_rate",
    "set_limit",
    "add_plan",
    "update_plan",
];

fn add_account(code: &str, currency: &str) -> AddAccountArgs {
    AddAccountArgs {
        code: code.into(),
        name: "K-Bank".into(),
        kind: "bank".into(),
        currency: currency.into(),
        ..Default::default()
    }
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
    let err = e
        .svc
        .set_rate(&c, SetRateArgs { currency: "USD".into(), rate: Some("36.5".into()), ..Default::default() })
        .await
        .unwrap_err();
    assert!(err.0.contains("rates:write"), "{}", err.0);
    assert!(
        e.svc
            .add_plan(
                &c,
                AddPlanArgs {
                    name: "Аренда".into(),
                    kind: "expense".into(),
                    account: "CARD".into(),
                    category: Some("FOOD".into()),
                    amount: "100".into(),
                    recurrence: "monthly".into(),
                    ..Default::default()
                }
            )
            .await
            .is_err()
    );
    assert_eq!(e.dirs.accounts.lock().unwrap().len(), 2, "ничего не создано");
    assert!(e.rates.rates.lock().unwrap().is_empty() && e.plans.plans.lock().unwrap().is_empty());
    // Каждое право открывает только свой раздел.
    e.grant(&["read", "categories:write"]).await;
    let c = e.caller(&e.oauth()).await;
    assert!(e.svc.visible(&c, "add_category") && e.svc.visible(&c, "update_category"));
    assert!(!e.svc.visible(&c, "add_account") && !e.svc.visible(&c, "add_currency") && !e.svc.visible(&c, "set_limit"));
}

#[tokio::test]
async fn directories_checkbox_grants_all_directory_rights() {
    let e = env();
    e.grant(&["directories", "limits:write"]).await;
    let g = e.svc.grants(e.user).await.unwrap().remove(0);
    assert_eq!(
        g.scopes,
        vec![
            "read",
            "currencies:write",
            "accounts:write",
            "categories:write",
            "places:write",
            "people:write",
            "rates:write",
            "limits:write"
        ]
    );
    let c = e.caller(&e.oauth()).await;
    assert!(
        e.svc.visible(&c, "add_currency")
            && e.svc.visible(&c, "set_rate")
            && e.svc.visible(&c, "set_limit")
            && e.svc.visible(&c, "add_place")
    );
    assert!(!e.svc.visible(&c, "add_plan") && !e.svc.visible(&c, "add_expense"));
}

#[tokio::test]
async fn viewer_cannot_write_directories() {
    let e = env();
    let all: Vec<String> = GRANT_SCOPES.iter().map(|s| s.to_string()).chain([ALL_DIRECTORIES.to_string()]).collect();
    let g = e
        .svc
        .save_grant(e.user, "claude", e.project, Role::Viewer, false, false, &all, &RequestMeta::default())
        .await
        .unwrap();
    assert_eq!(g.scopes, vec!["read".to_string()], "права не выше роли");
    // Права выдал OWNER, потом роль понизили; личный токен с `write` у VIEWER тоже только читает.
    e.grant(&GRANT_SCOPES).await;
    e.set_role(Role::Viewer, false);
    for token in [e.oauth(), format!("fbt_{}_{}_write", e.user, e.project)] {
        let c = e.caller(&token).await;
        for t in NEW_WRITE_TOOLS {
            assert!(!e.svc.visible(&c, t), "{t} не виден VIEWER");
        }
        assert!(
            e.svc
                .add_currency(
                    &c,
                    AddCurrencyArgs {
                        code: "GBP".into(),
                        name: "Фунт".into(),
                        symbol: "£".into(),
                        ..Default::default()
                    }
                )
                .await
                .is_err()
        );
        assert!(
            e.svc
                .set_limit(
                    &c,
                    SetLimitArgs { category: "FOOD".into(), amount: Some("100".into()), ..Default::default() }
                )
                .await
                .is_err()
        );
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
    e.svc
        .add_currency(
            &c,
            AddCurrencyArgs { code: "gbp".into(), name: "Фунт".into(), symbol: "£".into(), ..Default::default() },
        )
        .await
        .unwrap();
    e.svc
        .add_account(&c, AddAccountArgs { opening_balance: Some("500".into()), ..add_account("KBANK", "THB") })
        .await
        .unwrap();
    e.svc
        .add_category(
            &c,
            AddCategoryArgs {
                code: "PETS".into(),
                name: "Питомцы".into(),
                kind: Some("expense".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    e.svc
        .set_rate(&c, SetRateArgs { currency: "GBP".into(), rate: Some("45".into()), ..Default::default() })
        .await
        .unwrap();
    e.svc
        .set_limit(&c, SetLimitArgs { category: "PETS".into(), amount: Some("3000".into()), ..Default::default() })
        .await
        .unwrap();
    let plan = AddPlanArgs {
        name: "Корм".into(),
        kind: "expense".into(),
        account: "KBANK".into(),
        category: Some("PETS".into()),
        amount: "900".into(),
        recurrence: "monthly".into(),
        ..Default::default()
    };
    e.svc.add_plan(&c, plan).await.unwrap();
    assert_eq!(e.dirs.currencies.lock().unwrap().last().unwrap().code, "GBP", "код — в верхнем регистре");
    assert_eq!(e.dirs.openings.lock().unwrap().len(), 1);
    assert_eq!(
        (
            e.rates.rates.lock().unwrap().len(),
            e.limits.limits.lock().unwrap().len(),
            e.plans.plans.lock().unwrap().len()
        ),
        (1, 1, 1)
    );
}

#[tokio::test]
async fn narrow_token_opens_only_its_directory() {
    let e = env();
    let c = e.caller(&format!("fbt_{}_{}_accounts:write", e.user, e.project)).await;
    assert!(
        e.svc.visible(&c, "add_account") && !e.svc.visible(&c, "add_category") && !e.svc.visible(&c, "add_expense")
    );
    // Начальный остаток — операция в журнале: без transactions:write счёт не создаётся вовсе.
    let err = e
        .svc
        .add_account(&c, AddAccountArgs { opening_balance: Some("1000".into()), ..add_account("KBANK", "THB") })
        .await
        .unwrap_err();
    assert!(err.0.contains("opening_balance") && err.0.contains("transactions:write"), "{}", err.0);
    assert_eq!(e.dirs.accounts.lock().unwrap().len(), 2);
    e.svc.add_account(&c, add_account("KBANK", "THB")).await.unwrap();
    assert!(
        e.svc
            .add_category(
                &c,
                AddCategoryArgs {
                    code: "PETS".into(),
                    name: "Питомцы".into(),
                    kind: Some("expense".into()),
                    ..Default::default()
                }
            )
            .await
            .is_err()
    );
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
                translations: Some(NameTranslations {
                    ru: Some("Кей-Банк".into()),
                    en: None,
                    th: Some("กสิกรไทย".into()),
                }),
                opening_balance: Some("1 000,50".into()),
                ..add_account("", "บาท")
            },
        )
        .await
        .unwrap();
    let acc = e.dirs.accounts.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        (acc.code.as_str(), acc.kind.as_str(), acc.currency.as_str(), acc.name.lang, acc.archived),
        ("KBANK", "BANK", "THB", Lang::En, false)
    );
    assert_eq!(acc.name.ru.as_deref(), Some("Кей-Банк"));
    assert_eq!(e.dirs.openings.lock().unwrap()[0], ("KBANK".to_string(), d("1000.50"), today()));
    assert!(out.summary.contains("Opening balance 1000.5 THB"), "{}", out.summary);
    let events = e.accounts.events.lock().unwrap().clone();
    let w = events.iter().find(|x| x.0 == "mcp_write").unwrap();
    assert!(
        w.1.contains(&("tool".to_string(), "add_account".to_string()))
            && w.1.contains(&("account".to_string(), "KBANK".to_string())),
        "{:?}",
        w.1
    );
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
    let out = e
        .svc
        .add_category(
            &c,
            AddCategoryArgs {
                code: "STREET".into(),
                name: "Уличная еда".into(),
                parent: Some("อาหาร".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(out.data["category"]["parent"], "FOOD");
    assert_eq!(out.data["category"]["kind"], "EXPENSE");
    assert_eq!(out.data["category"]["sort_order"], 20, "после последнего соседа (CAFE = 10)");
    // Счёт — по-русски, категория — по-английски.
    let plan = AddPlanArgs {
        name: "Мойка".into(),
        kind: "expense".into(),
        account: "карта".into(),
        category: Some("Car wash".into()),
        amount: "300".into(),
        recurrence: "weekly".into(),
        interval: Some(2),
        ..Default::default()
    };
    let out = e.svc.add_plan(&c, plan).await.unwrap();
    assert_eq!(
        (out.data["plan"]["account"].as_str(), out.data["plan"]["category"].as_str()),
        (Some("CARD"), Some("CARWASH"))
    );
    assert!(out.summary.contains("every 2 weeks"), "{}", out.summary);
    // Валюта — по-английски, лимит — по-русски.
    e.svc
        .set_rate(&c, SetRateArgs { currency: "us dollar".into(), rate: Some("36.5".into()), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(e.rates.rates.lock().unwrap()[0].currency, "USD");
    e.svc
        .set_limit(&c, SetLimitArgs { category: "кафе".into(), amount: Some("3000".into()), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(e.limits.limits.lock().unwrap()[0].category_code, "CAFE");
    // Доходную категорию в расходный план не подставить.
    let bad = AddPlanArgs {
        name: "x".into(),
        kind: "expense".into(),
        account: "CARD".into(),
        category: Some("Salary".into()),
        amount: "1".into(),
        recurrence: "once".into(),
        ..Default::default()
    };
    assert!(e.svc.add_plan(&c, bad).await.unwrap_err().0.contains("not found"));
}

#[tokio::test]
async fn rate_is_upserted_by_currency_and_date_and_names_the_base_currency() {
    let e = env();
    e.grant(&["read", "rates:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let set = |rate: &str, date: &str| SetRateArgs {
        currency: "Доллар".into(),
        rate: Some(rate.into()),
        date: Some(date.into()),
        ..Default::default()
    };
    let out = e.svc.set_rate(&c, set("36.5", "2026-10-01")).await.unwrap();
    assert!(out.summary.contains("1 USD = 36.5 THB") && out.summary.contains("base currency"), "{}", out.summary);
    assert_eq!(out.data["base_currency"], "THB");
    let out = e.svc.set_rate(&c, set("36.7", "2026-10-01")).await.unwrap();
    assert!(out.summary.contains("Replaced the previous rate 1 USD = 36.5 THB"), "{}", out.summary);
    assert_eq!(e.rates.rates.lock().unwrap().len(), 1, "та же дата — тот же курс");
    assert_eq!(e.rates.rates.lock().unwrap()[0].rate, d("36.7"));
    // Перевёрнутый курс (THB за USD вместо USD → THB) — предупреждение о направлении.
    let out = e.svc.set_rate(&c, set("0.0272", "2026-10-02")).await.unwrap();
    assert!(out.summary.contains("Check the direction"), "{}", out.summary);
    assert_eq!(e.rates.rates.lock().unwrap().len(), 2);
    let err = e
        .svc
        .set_rate(&c, SetRateArgs { currency: "THB".into(), rate: Some("1".into()), ..Default::default() })
        .await
        .unwrap_err();
    assert!(err.0.contains("base currency"), "{}", err.0);
    assert!(e.svc.set_rate(&c, set("-1", "2026-10-03")).await.is_err(), "курс > 0");
    let list =
        e.svc.list_rates(&c, ListRatesArgs { currency: Some("USD".into()), ..Default::default() }).await.unwrap();
    assert_eq!(list.data["total"], 2);
    assert_eq!(list.data["latest"][0]["date"], "2026-10-02");
}

/// Issue #22: курс можно передать в обратную сторону — «1 THB = 2.5 RUB».
#[tokio::test]
async fn rate_can_be_set_as_inverse_rate() {
    let e = env();
    e.grant(&["read", "currencies:write", "rates:write"]).await;
    let c = e.caller(&e.oauth()).await;
    e.svc
        .add_currency(
            &c,
            AddCurrencyArgs {
                code: "RUB".into(), name: "Рубль".into(), symbol: "₽".into(), ..Default::default()
            },
        )
        .await
        .unwrap();
    let inv = |v: &str, date: &str| SetRateArgs {
        currency: "RUB".into(),
        inverse_rate: Some(v.into()),
        date: Some(date.into()),
        ..Default::default()
    };
    let out = e.svc.set_rate(&c, inv("2.50", "2026-10-16")).await.unwrap();
    assert!(out.summary.contains("1 THB = 2.5 RUB (1 RUB = 0.4 THB)"), "{}", out.summary);
    let saved = e.rates.rates.lock().unwrap()[0].clone();
    assert_eq!((saved.rate, saved.entered, saved.inverted), (d("0.4"), d("2.5"), true));
    assert_eq!(d("1000") * saved.rate, d("400"), "1000 ₽ = 400 ฿");
    assert_eq!(out.data["rate"]["inverse_rate"], "2.5");
    assert_eq!(out.data["rate"]["meaning"], "1 RUB = 0.4 THB");

    // Число в той же стороне, но перепутано направление — подсказка про rate.
    let out = e.svc.set_rate(&c, inv("0.4", "2026-10-17")).await.unwrap();
    assert!(
        out.summary.contains("Check the direction") && out.summary.contains("rate instead of inverse_rate"),
        "{}",
        out.summary
    );

    // Ровно одно из двух.
    let both = SetRateArgs {
        currency: "RUB".into(),
        rate: Some("0.4".into()),
        inverse_rate: Some("2.5".into()),
        ..Default::default()
    };
    let err = e.svc.set_rate(&c, both).await.unwrap_err();
    assert!(err.0.contains("exactly one"), "{}", err.0);
    let err = e.svc.set_rate(&c, SetRateArgs { currency: "RUB".into(), ..Default::default() }).await.unwrap_err();
    assert!(err.0.contains("inverse_rate"), "{}", err.0);
    assert!(e.svc.set_rate(&c, inv("0", "2026-10-18")).await.is_err(), "курс > 0");
    assert!(e.svc.set_rate(&c, inv("0.0000001", "2026-10-18")).await.is_err(), "не больше 6 знаков");
    assert_eq!(e.rates.rates.lock().unwrap().len(), 2);

    // list_rates отдаёт оба направления.
    let list =
        e.svc.list_rates(&c, ListRatesArgs { currency: Some("RUB".into()), ..Default::default() }).await.unwrap();
    let first = &list.data["rates"][1];
    assert_eq!(
        (first["rate"].as_str(), first["inverse_rate"].as_str(), first["inverted"].as_bool()),
        (Some("0.4"), Some("2.5"), Some(true))
    );
    assert!(list.summary.contains("1 THB = 0.4 RUB"), "{}", list.summary);

    // JSON-аргумент числом, как его пришлёт ассистент.
    let args: SetRateArgs =
        serde_json::from_value(serde_json::json!({"currency": "RUB", "inverse_rate": 2.53, "date": "2026-10-19"}))
            .unwrap();
    assert_eq!(args.inverse_rate.as_deref(), Some("2.53"));
}

#[tokio::test]
async fn limit_is_upserted_by_category_period_and_valid_from() {
    let e = env();
    e.grant(&["read", "limits:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e
        .svc
        .set_limit(&c, SetLimitArgs { category: "Food".into(), amount: Some("12000".into()), ..Default::default() })
        .await
        .unwrap();
    let month_start = period_start("MONTH", today());
    assert_eq!(e.limits.limits.lock().unwrap()[0].valid_from, month_start, "по умолчанию — с начала текущего месяца");
    assert!(out.summary.contains("12000 THB per month"), "{}", out.summary);
    let out = e
        .svc
        .set_limit(
            &c,
            SetLimitArgs {
                category: "FOOD".into(),
                amount: Some("15000".into()),
                period: Some("month".into()),
                valid_from: Some(month_start.to_string()),
                remove: None,
            },
        )
        .await
        .unwrap();
    assert!(out.summary.contains("Replaced 12000"), "{}", out.summary);
    assert_eq!(e.limits.limits.lock().unwrap().len(), 1);
    assert_eq!(e.limits.limits.lock().unwrap()[0].amount, Some(d("15000")));
    // Недельный лимит с явной даты — новое правило; более позднее правило упомянуто в итоге.
    e.svc
        .set_limit(
            &c,
            SetLimitArgs {
                category: "FOOD".into(),
                amount: Some("3000".into()),
                period: Some("week".into()),
                valid_from: Some("2099-01-05".into()),
                remove: None,
            },
        )
        .await
        .unwrap();
    let out = e
        .svc
        .set_limit(
            &c,
            SetLimitArgs {
                category: "FOOD".into(),
                amount: Some("16000".into()),
                period: None,
                valid_from: Some(month_start.to_string()),
                remove: None,
            },
        )
        .await
        .unwrap();
    assert!(out.summary.contains("later rule from 2099-01-05"), "{}", out.summary);
    assert_eq!(e.limits.limits.lock().unwrap().len(), 2);
    let list = e.svc.list_limits(&c).await.unwrap();
    let rows = list.data["limits"].as_array().unwrap().clone();
    assert_eq!(rows.iter().filter(|r| r["current"] == true).count(), 1, "действует одно правило категории");
    assert_eq!(rows.iter().find(|r| r["current"] == true).unwrap()["amount"], "16000");
    assert!(
        e.svc
            .set_limit(
                &c,
                SetLimitArgs {
                    category: "FOOD".into(),
                    amount: Some("100".into()),
                    period: Some("day".into()),
                    valid_from: None,
                    remove: None
                }
            )
            .await
            .is_err()
    );
}

/// Ц5: 0 — настоящий нулевой лимит; remove: true — «лимит снят» с даты, без суммы; одно из двух обязательно.
#[tokio::test]
async fn zero_limit_and_removed_limit() {
    let e = env();
    e.grant(&["read", "limits:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e
        .svc
        .set_limit(
            &c,
            SetLimitArgs {
                category: "FOOD".into(),
                amount: Some("0".into()),
                period: Some("week".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(out.summary.contains("no spending allowed"), "{}", out.summary);
    // 07.01.2099 — среда: снятие в периоде действующего (недельного) правила — с понедельника 05.01.
    let out = e
        .svc
        .set_limit(
            &c,
            SetLimitArgs {
                category: "FOOD".into(),
                remove: Some(true),
                valid_from: Some("2099-01-07".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(out.summary.contains("removed from 2099-01-05"), "{}", out.summary);
    let removed = e.limits.limits.lock().unwrap().iter().find(|l| l.amount.is_none()).cloned().unwrap();
    assert_eq!(removed.period, "WEEK");
    assert!(
        e.svc
            .set_limit(
                &c,
                SetLimitArgs {
                    category: "FOOD".into(),
                    amount: Some("1".into()),
                    remove: Some(true),
                    ..Default::default()
                }
            )
            .await
            .is_err()
    );
    assert!(
        e.svc.set_limit(&c, SetLimitArgs { category: "FOOD".into(), ..Default::default() }).await.is_err(),
        "ни суммы, ни снятия"
    );
    assert!(
        e.svc
            .set_limit(&c, SetLimitArgs { category: "FOOD".into(), amount: Some("-1".into()), ..Default::default() })
            .await
            .is_err()
    );
    let list = e.svc.list_limits(&c).await.unwrap();
    let rows = list.data["limits"].as_array().unwrap().clone();
    assert!(rows.iter().any(|r| r["removed"] == true && r["amount"].is_null()));
    assert!(rows.iter().any(|r| r["current"] == true && r["amount"] == "0"), "нулевой лимит действует сегодня");
}

/// Issue #58: недельный лимит со среды, затем «снять лимит» помесячно неделей позже — снятие действует с начала
/// месяца и перекрывает недельный лимит целиком; текущим в list_limits считается снятие.
#[tokio::test]
async fn longer_later_rule_replaces_earlier_from_its_start() {
    let e = env();
    e.grant(&["read", "limits:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let set = |period: &str, valid_from: &str, amount: Option<&str>| SetLimitArgs {
        category: "FOOD".into(),
        amount: amount.map(Into::into),
        period: Some(period.into()),
        valid_from: Some(valid_from.into()),
        remove: amount.is_none().then_some(true),
    };
    e.svc.set_limit(&c, set("month", "2026-08-01", Some("4000"))).await.unwrap();
    // 16.09.2026 — среда: недельный лимит с понедельника 14.09.
    e.svc.set_limit(&c, set("week", "2026-09-16", Some("1000"))).await.unwrap();
    let out = e.svc.set_limit(&c, set("month", "2026-09-22", None)).await.unwrap();
    assert!(out.summary.contains("removed from 2026-09-01"), "{}", out.summary);
    assert!(out.summary.contains("no longer apply at all: 1000 THB per week from 2026-09-16"), "{}", out.summary);
    let list = e.svc.list_limits(&c).await.unwrap();
    let rows = list.data["limits"].as_array().unwrap().clone();
    let current: Vec<_> = rows.iter().filter(|r| r["current"] == true).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0]["removed"], true, "действует снятие, а не недельный лимит");
    assert_eq!(rows[0]["valid_from"], "2026-09-22", "новые правила сначала");
    // Прошлые периоды сохраняют своё: в августе действует месячный лимит.
    let rules = e.limits.limits.lock().unwrap().clone();
    let aug = NaiveDate::from_ymd_opt(2026, 8, 20).unwrap();
    let spans = limit_spans(&rules, "FOOD");
    let in_aug: Vec<_> = spans.iter().filter(|(_, s)| s.is_some_and(|s| s.contains(aug))).collect();
    assert_eq!(in_aug.len(), 1);
    assert_eq!(in_aug[0].0.amount, Some(d("4000")));
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
    let args: UpdateAccountArgs =
        serde_json::from_value(serde_json::json!({"code": "Карта", "archived": true, "kind": "card"})).unwrap();
    e.svc.update_account(&c, args).await.unwrap();
    let card = e.dirs.accounts.lock().unwrap().iter().find(|a| a.code == "CARD").cloned().unwrap();
    assert_eq!((card.kind.as_str(), card.archived, card.name.text.as_str()), ("CARD", false, "Карта"));
    // Архивная валюта по точному коду меняется, но остаётся в архиве.
    e.svc
        .update_currency(
            &c,
            UpdateCurrencyArgs { code: "EUR".into(), symbol: Some("EUR".into()), ..Default::default() },
        )
        .await
        .unwrap();
    let eur = e.dirs.currencies.lock().unwrap().iter().find(|x| x.code == "EUR").cloned().unwrap();
    assert!(eur.archived && eur.symbol == "EUR");
    // Новое название — старые переводы уходят, если не переданы новые; без названия переводы меняются по одному.
    let tr = |en: &str| Some(NameTranslations { en: Some(en.into()), ..Default::default() });
    e.svc
        .update_currency(
            &c,
            UpdateCurrencyArgs { code: "USD".into(), translations: tr("U.S. dollar"), ..Default::default() },
        )
        .await
        .unwrap();
    let usd = e.dirs.currencies.lock().unwrap().iter().find(|x| x.code == "USD").cloned().unwrap();
    assert_eq!(
        (usd.name.text.as_str(), usd.name.en.as_deref(), usd.name.th.as_deref()),
        ("Доллар США", Some("U.S. dollar"), Some("ดอลลาร์สหรัฐ"))
    );
    e.svc
        .update_currency(
            &c,
            UpdateCurrencyArgs { code: "USD".into(), name: Some("Доллар".into()), ..Default::default() },
        )
        .await
        .unwrap();
    let usd = e.dirs.currencies.lock().unwrap().iter().find(|x| x.code == "USD").cloned().unwrap();
    assert_eq!((usd.name.text.as_str(), usd.name.en.as_deref()), ("Доллар", None));
    // Перенос категории под другого родителя и в корень.
    let out = e
        .svc
        .update_category(
            &c,
            UpdateCategoryArgs { code: "Мойка".into(), parent: Some("Food".into()), ..Default::default() },
        )
        .await
        .unwrap();
    assert_eq!(out.data["category"]["parent"], "FOOD");
    assert!(out.summary.contains("Moved under FOOD"), "{}", out.summary);
    let out = e
        .svc
        .update_category(
            &c,
            UpdateCategoryArgs { code: "CARWASH".into(), parent: Some(String::new()), ..Default::default() },
        )
        .await
        .unwrap();
    assert_eq!(out.data["category"]["parent"], serde_json::Value::Null);
    assert!(e.dirs.categories.lock().unwrap().iter().all(|c| !c.archived));
    let err = e
        .svc
        .update_category(
            &c,
            UpdateCategoryArgs { code: "CAFE".into(), parent: Some("Salary".into()), ..Default::default() },
        )
        .await
        .unwrap_err();
    assert!(err.0.contains("SALARY is an income category"), "родитель другого вида: {}", err.0);
    assert!(
        e.svc
            .update_category(
                &c,
                UpdateCategoryArgs { code: "CAFE".into(), parent: Some("CAFE".into()), ..Default::default() }
            )
            .await
            .is_err(),
        "сама себе не родитель"
    );
    assert!(
        e.svc
            .update_currency(&c, UpdateCurrencyArgs { code: "USD".into(), ..Default::default() })
            .await
            .unwrap_err()
            .0
            .contains("nothing to change")
    );
}

#[tokio::test]
async fn opening_balance_on_update_requires_transactions_write() {
    let e = env();
    e.grant(&["read", "accounts:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let err = e
        .svc
        .update_account(
            &c,
            UpdateAccountArgs { code: "CASH".into(), opening_balance: Some("100".into()), ..Default::default() },
        )
        .await
        .unwrap_err();
    assert!(err.0.contains("transactions:write"), "{}", err.0);
    e.grant(&["read", "accounts:write", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e
        .svc
        .update_account(
            &c,
            UpdateAccountArgs {
                code: "CASH".into(),
                opening_balance: Some("-250".into()),
                opening_date: Some("2026-01-01".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(out.summary.contains("-250 THB on 2026-01-01"), "{}", out.summary);
    assert_eq!(
        e.dirs.openings.lock().unwrap()[0],
        ("CASH".to_string(), d("-250"), NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
    );
    assert!(
        e.svc
            .update_account(
                &c,
                UpdateAccountArgs {
                    code: "CASH".into(),
                    opening_date: Some("2026-01-01".into()),
                    ..Default::default()
                }
            )
            .await
            .is_err(),
        "дата без суммы"
    );
}

#[tokio::test]
async fn plans_are_added_paused_and_validated() {
    let e = env();
    e.grant(&["read", "planning:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let rent = AddPlanArgs {
        name: "Аренда".into(),
        translations: Some(NameTranslations {
            en: Some("Rent".into()), th: Some("ค่าเช่า".into()), ru: None
        }),
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
    let out = e
        .svc
        .update_plan(
            &c,
            UpdatePlanArgs {
                id: id.to_string(),
                active: Some(false),
                amount: Some("16000".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(out.summary.contains("paused"), "{}", out.summary);
    let p = e.plans.plans.lock().unwrap()[0].clone();
    assert_eq!(
        (p.draft.active, p.draft.amount, p.draft.category_code.as_deref(), p.draft.name.en.as_deref()),
        (false, d("16000"), Some("CAR"), Some("Rent"))
    );
    let list = e.svc.list_plans(&c).await.unwrap();
    assert_eq!(list.data["plans"][0]["active"], false);
    let transfer = AddPlanArgs {
        name: "Копилка".into(),
        kind: "transfer".into(),
        account: "CARD".into(),
        amount: "1000".into(),
        recurrence: "monthly".into(),
        ..Default::default()
    };
    assert!(
        e.svc
            .add_plan(&c, AddPlanArgs { category: Some("FOOD".into()), ..transfer })
            .await
            .unwrap_err()
            .0
            .contains("transfers have no category")
    );
    let transfer = AddPlanArgs {
        name: "Копилка".into(),
        kind: "transfer".into(),
        account: "CARD".into(),
        amount: "1000".into(),
        recurrence: "monthly".into(),
        ..Default::default()
    };
    assert!(e.svc.add_plan(&c, transfer).await.unwrap_err().0.contains("to_account"));
    assert!(
        e.svc
            .update_plan(
                &c,
                UpdatePlanArgs { id: id.to_string(), to_account: Some("CASH".into()), ..Default::default() }
            )
            .await
            .is_err()
    );
    assert!(
        e.svc
            .update_plan(
                &c,
                UpdatePlanArgs { id: Uuid::new_v4().to_string(), active: Some(true), ..Default::default() }
            )
            .await
            .unwrap_err()
            .0
            .contains("not found")
    );
    let writes: Vec<String> = e
        .accounts
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|x| x.0 == "mcp_write")
        .filter_map(|x| x.1.iter().find(|d| d.0 == "tool").map(|d| d.1.clone()))
        .collect();
    assert_eq!(writes, vec!["add_plan", "update_plan"], "каждое изменение — в журнал");
}

#[tokio::test]
async fn context_lists_currencies_and_all_permissions() {
    let e = env();
    e.grant(&["read", "transactions:write", "rates:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let ctx = e.svc.get_context(&c).await.unwrap();
    let codes: Vec<&str> =
        ctx.data["currencies"].as_array().unwrap().iter().map(|x| x["code"].as_str().unwrap()).collect();
    assert_eq!(codes, vec!["THB", "USD"], "архивные не показываются");
    assert_eq!(ctx.data["currencies"][0]["name"], "Тайский бат");
    assert_eq!(ctx.data["currencies"][0]["symbol"], "฿");
    let p = &ctx.data["permissions"];
    for (k, v) in [
        ("write_rates", true),
        ("write_accounts", false),
        ("write_currencies", false),
        ("write_categories", false),
        ("write_limits", false),
        ("write_planning", false),
        ("delete", false),
    ] {
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
    assert_eq!(
        (scope_group("limits:write"), scope_group("planning:write"), scope_group("transactions:write")),
        ("limits", "planning", "base")
    );
}

#[test]
fn templates_and_pages_have_translations() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let missing =
        kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
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
    let pick =
        |v: &str| resolve("category", v, &q.categories, |x| &x.code, |x| x.search.clone()).map(|c| c.code.clone());
    assert_eq!(pick("кафе").unwrap(), "CAFE");
    assert_eq!(pick("Car").unwrap(), "CAR", "точный код важнее подстроки");
    assert_eq!(pick("รถ").unwrap(), "CAR", "точное тайское название важнее части «ล้างรถ» (техдолг #5)");
    assert_eq!(pick("Car wash").unwrap(), "CARWASH", "точное название из двух слов");
    assert_eq!(pick("CAFÉ").unwrap(), "CAFE", "без диакритики и регистра");
    assert!(pick("ca").unwrap_err().contains("ambiguous"));
    let _ = Utc::now();
}

/// Ц3: у счёта в USD нет курса — расход записан, но Claude предупреждает: в бюджет сумма не войдёт.
#[tokio::test]
async fn expense_without_rate_is_flagged() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out =
        e.svc.add_expense(&c, AddFlowArgs { account: Some("WISE".into()), ..expense("10", "cafe") }).await.unwrap();
    assert!(out.summary.contains("no exchange rate USD → THB"), "{}", out.summary);
    assert_eq!(out.data["no_rate"], "USD");
    let ok = e.svc.add_expense(&c, expense("10", "cafe")).await.unwrap();
    assert!(!ok.summary.contains("no exchange rate") && ok.data.get("no_rate").is_none());
}

// ---------------------------------------------------------------- места (шаг 5)

/// Место по названию или варианту написания подставляет категорию и счёт; переданное — важнее места.
#[tokio::test]
async fn place_fills_category_and_account() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e
        .svc
        .add_expense(&c, AddFlowArgs { amount: Some("55".into()), place: Some("เซเว่น".into()), ..Default::default() })
        .await
        .unwrap();
    let tx = e.ledger.txs.lock().unwrap()[0].clone();
    assert_eq!(
        (tx.place_code.as_deref(), tx.account_code.as_str(), tx.items[0].category_code.as_deref()),
        (Some("SEVEN_ELEVEN"), "CARD", Some("FOOD"))
    );
    assert!(!out.summary.contains("default account"), "счёт из места — не «по умолчанию»: {}", out.summary);
    assert_eq!(out.data["transaction"]["place"], "SEVEN_ELEVEN");
    e.svc
        .add_expense(
            &c,
            AddFlowArgs { place: Some("7-11".into()), account: Some("CASH".into()), ..expense("45", "cafe") },
        )
        .await
        .unwrap();
    let tx = e.ledger.txs.lock().unwrap()[1].clone();
    assert_eq!((tx.account_code.as_str(), tx.items[0].category_code.as_deref()), ("CASH", Some("CAFE")));
    let no_cat = e
        .svc
        .add_expense(&c, AddFlowArgs { amount: Some("10".into()), place: Some("makro".into()), ..Default::default() })
        .await
        .unwrap_err();
    assert!(no_cat.0.contains("amount and category"), "у Makro нет категории: {}", no_cat.0);
    let unknown =
        e.svc.add_expense(&c, AddFlowArgs { place: Some("Lotus".into()), ..expense("10", "cafe") }).await.unwrap_err();
    assert!(unknown.0.contains("not found") && unknown.0.contains("add_place"), "{}", unknown.0);
    // Фильтр ленты по месту; правка: "" убирает место, у перевода места нет.
    let list = e
        .svc
        .list_transactions(
            &c,
            ListTransactionsArgs {
                place: Some("SEVEN_ELEVEN".into()),
                from: Some("2000-01-01".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(list.data["total"], 2);
    let id = e.ledger.txs.lock().unwrap()[0].id.to_string();
    e.svc
        .update_transaction(&c, UpdateTransactionArgs { id: id.clone(), place: Some("".into()), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(e.ledger.txs.lock().unwrap()[0].place_code, None);
    e.svc
        .add_transfer(
            &c,
            AddTransferArgs {
                from_account: "CASH".into(),
                to_account: "CARD".into(),
                amount: "100".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let transfer = e.ledger.txs.lock().unwrap()[2].id.to_string();
    let err = e
        .svc
        .update_transaction(
            &c,
            UpdateTransactionArgs { id: transfer, place: Some("SEVEN_ELEVEN".into()), ..Default::default() },
        )
        .await
        .unwrap_err();
    assert!(err.0.contains("no place"), "{}", err.0);
    // Архивное место, которое уже стоит у операции, можно передать снова.
    e.ledger.txs.lock().unwrap()[1].place_code = Some("OLD".into());
    let second = e.ledger.txs.lock().unwrap()[1].id.to_string();
    e.svc
        .update_transaction(
            &c,
            UpdateTransactionArgs {
                id: second,
                place: Some("old".into()),
                description: Some("Рынок".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(e.ledger.txs.lock().unwrap()[1].place_code.as_deref(), Some("OLD"));
    let ctx = e.svc.get_context(&c).await.unwrap();
    assert_eq!(
        (ctx.data["frequent_places"][0]["code"].as_str(), ctx.data["places_total"].as_u64()),
        (Some("SEVEN_ELEVEN"), Some(2))
    );
    assert_eq!(ctx.data["permissions"]["write_places"], false);
}

/// Места добавляют и меняют только с правом «Места» (или «Все справочники»); список — с поиском без регистра.
#[tokio::test]
async fn places_directory_tools() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let add = || AddPlaceArgs {
        code: "cafe_amazon".into(),
        name: "Café Amazon".into(),
        aliases: Some(vec!["คาเฟ่ อเมซอน".into()]),
        category: Some("Кафе".into()),
        account: Some("Наличные".into()),
        kind: Some("cafe".into()),
        location: Some("Он Нут".into()),
        parent: None,
    };
    assert!(e.svc.add_place(&c, add()).await.unwrap_err().0.contains("places:write"));
    e.grant(&["read", "directories"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e.svc.add_place(&c, add()).await.unwrap();
    assert!(out.summary.contains("CAFE_AMAZON 'Café Amazon · Он Нут'"), "{}", out.summary);
    let p = e.dirs.places.lock().unwrap().iter().find(|p| p.code == "CAFE_AMAZON").cloned().unwrap();
    assert_eq!(
        (p.category_code.as_deref(), p.account_code.as_deref(), p.kind.as_str()),
        (Some("CAFE"), Some("CASH"), "CAFE")
    );
    assert!(e.svc.add_place(&c, add()).await.unwrap_err().0.contains("already exists"));
    let found =
        e.svc.list_places(&c, ListPlacesArgs { text: Some("cafe".into()), ..Default::default() }).await.unwrap();
    assert_eq!(found.data["places"].as_array().unwrap().len(), 1, "Café без диакритики");
    let all =
        e.svc.list_places(&c, ListPlacesArgs { include_archived: Some(true), ..Default::default() }).await.unwrap();
    assert_eq!(all.data["places"].as_array().unwrap().len(), 3);
    e.svc
        .update_place(
            &c,
            UpdatePlaceArgs {
                code: "Café Amazon".into(),
                aliases: Some(vec![]),
                category: Some("".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let p = e.dirs.places.lock().unwrap().iter().find(|p| p.code == "CAFE_AMAZON").cloned().unwrap();
    assert_eq!((p.aliases.len(), p.category_code), (0, None));
    assert!(
        e.svc
            .update_place(&c, UpdatePlaceArgs { code: "CAFE_AMAZON".into(), ..Default::default() })
            .await
            .unwrap_err()
            .0
            .contains("nothing to change")
    );
    // Дерево: конкретный магазин сети — по названию или коду сети; "" делает место верхнего уровня.
    let shop = AddPlaceArgs {
        code: "SEVEN_ASOK".into(),
        name: "7-Eleven".into(),
        parent: Some("7-11".into()),
        location: Some("Asok".into()),
        ..Default::default()
    };
    let out = e.svc.add_place(&c, shop).await.unwrap();
    assert_eq!(out.data["place"]["parent"], "SEVEN_ELEVEN");
    e.svc
        .update_place(&c, UpdatePlaceArgs { code: "SEVEN_ASOK".into(), parent: Some("".into()), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(e.dirs.places.lock().unwrap().iter().find(|p| p.code == "SEVEN_ASOK").unwrap().parent_code, None);
}

// ---------------------------------------------------------------- люди и «Для кого» (шаг 6)

/// `for` у операции и позиций — по коду или имени; по умолчанию — для всех; правка шапки меняет позиции с прежним
/// общим получателем; фильтр ленты; get_context отдаёт людей и запись пользователя.
#[tokio::test]
async fn recipients_in_transactions() {
    let e = env();
    e.dirs.people.lock().unwrap().push(Person {
        code: "ALEKSEY".into(),
        name: "Алексей".into(),
        aliases: vec![],
        user_id: Some(e.user),
        former_member: false,
        archived: false,
    });
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let ctx = e.svc.get_context(&c).await.unwrap();
    assert_eq!(ctx.data["my_person_code"], "ALEKSEY");
    assert_eq!(ctx.data["people"].as_array().unwrap().len(), 2, "архивных нет");
    assert_eq!(ctx.data["permissions"]["write_people"], false);
    e.svc.add_expense(&c, expense("10", "cafe")).await.unwrap();
    assert_eq!(e.ledger.txs.lock().unwrap()[0].items[0].for_person, Some(None), "по умолчанию — для всех");
    let out =
        e.svc.add_expense(&c, AddFlowArgs { for_whom: Some("ปิ่นปั่น".into()), ..expense("45", "cafe") }).await.unwrap();
    assert!(out.summary.contains("For PINPAN"), "{}", out.summary);
    assert_eq!(out.data["transaction"]["for"], "PINPAN");
    let items = vec![
        ItemArgs {
            category: Some("cafe".into()),
            amount: Some("88".into()),
            for_whom: Some("all".into()),
            ..Default::default()
        },
        ItemArgs {
            category: Some("cafe".into()),
            amount: Some("45".into()),
            for_whom: Some("Алексей".into()),
            ..Default::default()
        },
        ItemArgs { category: Some("cafe".into()), amount: Some("22".into()), ..Default::default() },
    ];
    let out = e
        .svc
        .add_expense(&c, AddFlowArgs { for_whom: Some("PINPAN".into()), items: Some(items), ..Default::default() })
        .await
        .unwrap();
    let who: Vec<Option<Option<String>>> =
        e.ledger.txs.lock().unwrap()[2].items.iter().map(|x| x.for_person.clone()).collect();
    assert_eq!(who, vec![Some(None), Some(Some("ALEKSEY".into())), Some(Some("PINPAN".into()))]);
    assert_eq!(out.data["transaction"]["recipients_differ"], true);
    assert_eq!(out.data["transaction"]["items"][1]["for"], "ALEKSEY");
    for bad in ["Old nanny", "Nobody"] {
        let err = e
            .svc
            .add_expense(&c, AddFlowArgs { for_whom: Some(bad.into()), ..expense("1", "cafe") })
            .await
            .unwrap_err();
        assert!(err.0.starts_with("for:") && err.0.contains("add_person"), "{bad}: {}", err.0);
    }
    // Правка: шапка PINPAN → all меняет позиции с прежним общим получателем.
    let id = e.ledger.txs.lock().unwrap()[1].id.to_string();
    e.svc
        .update_transaction(
            &c,
            UpdateTransactionArgs { id: id.clone(), for_whom: Some("all".into()), ..Default::default() },
        )
        .await
        .unwrap();
    assert_eq!(e.ledger.txs.lock().unwrap()[1].for_person_code, None);
    let list = e
        .svc
        .list_transactions(
            &c,
            ListTransactionsArgs {
                for_whom: Some("pinpan".into()),
                from: Some("2000-01-01".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(list.data["total"], 1);
    let archived = e
        .svc
        .list_transactions(
            &c,
            ListTransactionsArgs {
                for_whom: Some("NANNY".into()),
                from: Some("2000-01-01".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(archived.data["total"], 0, "архивный человек в фильтре находится");
    let err = e
        .svc
        .add_transfer(
            &c,
            AddTransferArgs {
                from_account: "CASH".into(),
                to_account: "CARD".into(),
                amount: "1".into(),
                for_whom: Some("PINPAN".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(err.0.contains("no recipient"), "{}", err.0);
    e.svc
        .add_transfer(
            &c,
            AddTransferArgs {
                from_account: "CASH".into(),
                to_account: "CARD".into(),
                amount: "100".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let transfer = e.ledger.txs.lock().unwrap().last().unwrap().id.to_string();
    let err = e
        .svc
        .update_transaction(
            &c,
            UpdateTransactionArgs { id: transfer, for_whom: Some("PINPAN".into()), ..Default::default() },
        )
        .await
        .unwrap_err();
    assert!(err.0.contains("no recipient"), "{}", err.0);
}

/// Людей добавляют и меняют только с правом «Люди» (или «Все справочники»); код ALL занят под «Для всех».
#[tokio::test]
async fn people_directory_tools() {
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    let add = || AddPersonArgs { code: "nanny_ann".into(), name: "Ann".into(), aliases: Some(vec!["Аня".into()]) };
    assert!(e.svc.add_person(&c, add()).await.unwrap_err().0.contains("people:write"));
    e.grant(&["read", "directories"]).await;
    let c = e.caller(&e.oauth()).await;
    let out = e.svc.add_person(&c, add()).await.unwrap();
    assert!(out.summary.contains("NANNY_ANN 'Ann'"), "{}", out.summary);
    assert!(e.svc.add_person(&c, add()).await.unwrap_err().0.contains("already exists"));
    let found = e.svc.list_people(&c, ListPeopleArgs { text: Some("аня".into()), ..Default::default() }).await.unwrap();
    assert_eq!(found.data["people"].as_array().unwrap().len(), 1);
    e.svc
        .update_person(&c, UpdatePersonArgs { code: "Ann".into(), name: Some("Ann B.".into()), ..Default::default() })
        .await
        .unwrap();
    assert!(e.dirs.people.lock().unwrap().iter().any(|p| p.code == "NANNY_ANN" && p.name == "Ann B."));
    assert!(
        e.svc
            .update_person(&c, UpdatePersonArgs { code: "NANNY_ANN".into(), ..Default::default() })
            .await
            .unwrap_err()
            .0
            .contains("nothing to change")
    );
    assert!(e.svc.visible(&c, "add_person"));
}

// ---------------------------------------------------------------- SMS банка (docs/external-refs.md, раздел 4)

/// Операции из SMS банка: поле `sms` в выводе (последняя связь), фильтр `from_sms`; правка связи не трогает.
#[tokio::test]
async fn sms_transactions_are_marked_and_filtered() {
    use chrono::TimeZone;
    let e = env();
    e.grant(&["read", "transactions:write"]).await;
    let c = e.caller(&e.oauth()).await;
    for amount in ["250", "45", "10"] {
        e.svc.add_expense(&c, expense(amount, "food")).await.unwrap();
    }
    let sms_at = Utc.with_ymd_and_hms(2026, 10, 3, 3, 51, 7).unwrap();
    let r = |provider: &str, link: &str, minutes: i64| TxRef {
        provider: provider.into(),
        link: link.into(),
        received_at: Some(sms_at),
        created_at: sms_at + chrono::Duration::minutes(minutes),
    };
    {
        let mut txs = e.ledger.txs.lock().unwrap();
        txs[0].external_refs = vec![r(SMS_PROVIDER, "CREATED", 1)];
        // Две связи: в выводе — последняя.
        txs[1].external_refs =
            vec![r(SMS_PROVIDER, "CREATED", 1), TxRef { received_at: None, ..r(SMS_PROVIDER, "MATCHED", 5) }];
        txs[2].external_refs = vec![r("bank_statement", "CREATED", 1)];
    }
    let all = e
        .svc
        .list_transactions(&c, ListTransactionsArgs { from: Some("2000-01-01".into()), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(all.data["total"], 3);
    let sms = e
        .svc
        .list_transactions(
            &c,
            ListTransactionsArgs { from_sms: Some(true), from: Some("2000-01-01".into()), ..Default::default() },
        )
        .await
        .unwrap();
    assert_eq!(sms.data["total"], 2, "только из SMS банка или подтверждённые им");
    let txs = sms.data["transactions"].as_array().unwrap();
    let local = sms_at.with_timezone(&chrono::Local).to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    let by_total = |total: &str| txs.iter().find(|t| t["total"] == total).unwrap().clone();
    assert_eq!(by_total("250")["sms"], serde_json::json!({"link": "CREATED", "received_at": local}));
    assert_eq!(by_total("45")["sms"], serde_json::json!({"link": "MATCHED", "received_at": null}));
    let statement = all.data["transactions"].as_array().unwrap().iter().find(|t| t["total"] == "10").unwrap().clone();
    assert!(statement.get("sms").is_none(), "другие источники не показываются как SMS");
    let off = e
        .svc
        .list_transactions(
            &c,
            ListTransactionsArgs { from_sms: Some(false), from: Some("2000-01-01".into()), ..Default::default() },
        )
        .await
        .unwrap();
    assert_eq!(off.data["total"], 3, "from_sms: false — без фильтра");
    // Claude дополняет операцию из SMS — связь остаётся.
    let id = e.ledger.txs.lock().unwrap()[0].id.to_string();
    let out = e
        .svc
        .update_transaction(&c, UpdateTransactionArgs { id, description: Some("Big C".into()), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(out.data["transaction"]["sms"]["link"], "CREATED");
}
