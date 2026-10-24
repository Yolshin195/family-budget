//! Тесты use cases фичи «Транзакции»: без других фич, на in-memory адаптере и — если задан
//! `TEST_DATABASE_URL` — на PostgreSQL (`on_each_repo!`).

use std::str::FromStr;
use std::sync::Arc;

use chrono::NaiveDate;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::pagination::Pagination;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use kernel::testing::TestDb;

use crate::adapters::memory::InMemoryTransactionRepository;
use crate::adapters::postgres::PostgresTransactionRepository;
use crate::domain::{AccountCurrency, Description, Draft, ItemDraft, ItemsDraft, Transaction, TransactionData};
use crate::dto::{ExternalRefInput, ItemInput, LineDto, ListQuery, TransactionInput, TranslationInput};
use crate::ports::{
    AccountInfo, AccountsLookup, BudgetView, CategoriesLookup, CategoryInfo, CategoryLimit, CategoryRef, Deps,
    PeopleLookup, PersonInfo, PlaceInfo, PlacesLookup, PlanOccurrence, PlansLookup,
};
use crate::service::TransactionService;

struct FakeAccounts;
struct FakeCategories;

#[async_trait::async_trait]
impl AccountsLookup for FakeAccounts {
    async fn exists(&self, _p: Uuid, code: &str) -> AppResult<bool> {
        Ok(matches!(code, "CARD" | "WISE" | "CASH" | "TINKOFF"))
    }
    async fn currencies(&self, _p: Uuid) -> AppResult<Vec<AccountCurrency>> {
        let a = |code: &str, currency: &str| AccountCurrency {
            code: code.into(),
            currency: currency.into(),
            minor_units: 2,
        };
        Ok(vec![a("CARD", "THB"), a("CASH", "THB"), a("WISE", "USD"), a("TINKOFF", "RUB"), a("OLD", "THB")])
    }
    async fn choices(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<Choice>> {
        Ok(vec![])
    }
    async fn titles(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(vec![])
    }
    async fn infos(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<AccountInfo>> {
        Ok(vec![
            AccountInfo {
                code: "CARD".into(), title: "Карта".into(), currency: "THB".into(), symbol: "฿".into()
            },
            AccountInfo { code: "WISE".into(), title: "Wise".into(), currency: "USD".into(), symbol: "$".into() },
            AccountInfo {
                code: "TINKOFF".into(),
                title: "Тинькофф".into(),
                currency: "RUB".into(),
                symbol: "₽".into(),
            },
        ])
    }
}

#[async_trait::async_trait]
impl CategoriesLookup for FakeCategories {
    async fn kind_of(&self, _p: Uuid, code: &str) -> AppResult<Option<CategoryRef>> {
        let c = |kind: &str, archived: bool| Some(CategoryRef { kind: kind.into(), archived });
        Ok(match code {
            "FOOD" | "CAFE" | "HOME" | "FEES" => c("EXPENSE", false),
            "SALARY" => c("INCOME", false),
            // Архивные: старые ссылки остаются, новые нельзя.
            "OLD_FOOD" => c("EXPENSE", true),
            "OLD_BONUS" => c("INCOME", true),
            _ => None,
        })
    }
    async fn choices(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<Choice>> {
        Ok(vec![])
    }
    async fn titles(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<(String, String)>> {
        Ok(vec![])
    }
    async fn infos(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<CategoryInfo>> {
        let c = |code: &str, parent: Option<&str>, kind: &str, depth: usize, names: &str| CategoryInfo {
            code: code.into(),
            parent_code: parent.map(Into::into),
            kind: kind.into(),
            title: code.into(),
            depth,
            names: kernel::search::join(names.split(' ')),
        };
        Ok(vec![
            c("FOOD", None, "EXPENSE", 0, "Еда Food อาหาร"),
            c("CAFE", Some("FOOD"), "EXPENSE", 1, "Кафе Cafe ร้านอาหาร"),
            c("HOME", None, "EXPENSE", 0, "Жильё Housing ที่อยู่อาศัย"),
            c("SALARY", None, "INCOME", 0, "Зарплата Salary เงินเดือน"),
        ])
    }
}

struct FakeBudget;

#[async_trait::async_trait]
impl BudgetView for FakeBudget {
    async fn limits(&self, _p: Uuid, _base: &str, _date: NaiveDate, _l: Lang) -> AppResult<Vec<CategoryLimit>> {
        Ok(vec![
            CategoryLimit {
                code: "FOOD".into(),
                limit: Some(d("10000")),
                forecast: d("9500"),
                available: Some(d("500")),
            },
            CategoryLimit { code: "CAFE".into(), limit: None, forecast: d("1000"), available: Some(d("500")) },
        ])
    }
    async fn rates(&self, _p: Uuid, _base: &str, _date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        Ok(vec![("USD".into(), d("33.25"))])
    }
}

/// План «Аренда» (расход) платится 5-го числа; платёж 12.10 пропущен. Других планов нет.
const RENT: Uuid = Uuid::from_u128(5);

struct FakePlans;

#[async_trait::async_trait]
impl PlansLookup for FakePlans {
    async fn occurrence(&self, _p: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<Option<PlanOccurrence>> {
        use chrono::Datelike;
        Ok(match (plan_id == RENT, date.day()) {
            (true, 5) => Some(PlanOccurrence { kind: "EXPENSE".into(), skipped: false }),
            (true, 12) => Some(PlanOccurrence { kind: "EXPENSE".into(), skipped: true }),
            _ => None,
        })
    }
}

/// Места: SEVEN_ELEVEN — продукты наличными, OLD — в архиве.
struct FakePlaces;

#[async_trait::async_trait]
impl PlacesLookup for FakePlaces {
    async fn all(&self, _p: Uuid) -> AppResult<Vec<PlaceInfo>> {
        let p = |code: &str, cat: Option<&str>, acc: Option<&str>, archived: bool| PlaceInfo {
            code: code.into(),
            title: code.into(),
            search: code.into(),
            default_category_code: cat.map(Into::into),
            default_account_code: acc.map(Into::into),
            archived,
        };
        Ok(vec![
            p("SEVEN_ELEVEN", Some("FOOD"), Some("CASH"), false),
            p("OLD", None, None, true),
            p("PAYDAY", Some("SALARY"), Some("NOPE"), false),
        ])
    }
}

/// Люди: PINPAN и ANNA (участник), OLD — в архиве.
struct FakePeople;

#[async_trait::async_trait]
impl PeopleLookup for FakePeople {
    async fn all(&self, _p: Uuid, _lang: Lang) -> AppResult<Vec<PersonInfo>> {
        let x = |code: &str, user: Option<Uuid>, archived: bool| PersonInfo {
            code: code.into(),
            title: code.to_lowercase(),
            search: code.into(),
            user_id: user,
            archived,
        };
        Ok(vec![x("PINPAN", None, false), x("ANNA", Some(Uuid::nil()), false), x("OLD", None, true)])
    }
}

fn deps() -> Deps {
    Deps {
        accounts: Arc::new(FakeAccounts),
        categories: Arc::new(FakeCategories),
        budget: Arc::new(FakeBudget),
        plans: Arc::new(FakePlans),
        places: Arc::new(FakePlaces),
        people: Arc::new(FakePeople),
    }
}

fn service() -> TransactionService {
    TransactionService::new(Arc::new(InMemoryTransactionRepository::default()), deps())
}

/// Хранилища для теста use case: in-memory и, если задан `TEST_DATABASE_URL`, PostgreSQL (техдолг #10) —
/// SQL и memory-адаптер не должны расходиться.
async fn backends() -> Vec<(&'static str, TransactionService, Option<TestDb>)> {
    let mut out = vec![("memory", service(), None)];
    if let Some(db) = TestDb::migrated("tx", vec![sqlx::migrate!("./migrations")]).await {
        let repo = Arc::new(PostgresTransactionRepository::new(db.pool.clone()));
        out.push(("postgres", TransactionService::new(repo, deps()), Some(db)));
    }
    out
}

/// Тест use case на каждом хранилище из [`backends`]; в теле `svc` — сервис на этом хранилище.
macro_rules! on_each_repo {
    ($(#[$meta:meta])* async fn $name:ident($svc:ident) $body:block) => {
        $(#[$meta])*
        #[tokio::test]
        async fn $name() {
            for (repo, svc, db) in backends().await {
                eprintln!("хранилище: {repo}");
                let $svc = &svc;
                $body
                if let Some(db) = db {
                    db.close().await;
                }
            }
        }
    };
}

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, n).unwrap()
}

fn input(kind: &str, account: &str) -> TransactionInput {
    TransactionInput {
        occurred_on: day(3),
        tx_kind: kind.into(),
        account_code: account.into(),
        refund: false,
        lines: Vec::new(),
        to_account_code: None,
        amount: None,
        to_amount: None,
        fee_category_code: None,
        fee_amount: None,
        description: Some("Big C".into()),
        description_lang: "EN".into(),
        description_ru: None,
        description_en: None,
        description_th: None,
        plan_id: None,
        plan_date: None,
        ..Default::default()
    }
}

fn receipt(lines: &[(&str, &str)]) -> TransactionInput {
    TransactionInput {
        lines: lines.iter().map(|(c, a)| LineDto { category_code: c.to_string(), amount: d(a), note: None }).collect(),
        ..input("EXPENSE", "card")
    }
}

fn transfer(
    from: &str,
    to: &str,
    amount: &str,
    to_amount: Option<&str>,
    fee: Option<(&str, &str)>,
) -> TransactionInput {
    TransactionInput {
        to_account_code: Some(to.into()),
        amount: Some(d(amount)),
        to_amount: to_amount.map(d),
        fee_category_code: fee.map(|f| f.0.into()),
        fee_amount: fee.map(|f| d(f.1)),
        ..input("TRANSFER", from)
    }
}

/// Код и поле ошибки с кодом (техдолг #9).
fn problem(e: AppError) -> (&'static str, String) {
    match e {
        AppError::Invalid(p) => (p.code, p.field),
        other => panic!("ожидалась ошибка с кодом, а пришла {other:?}"),
    }
}

async fn balance(svc: &TransactionService, p: Uuid, account: &str) -> Decimal {
    svc.balance(p, account, day(31)).await.unwrap()
}

on_each_repo! {
    async fn receipt_with_several_lines(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let t = svc.create(p, u, receipt(&[("food", "900"), ("home", "250"), ("cafe", "100")])).await.unwrap();
        assert_eq!(t.total, d("1250"));
        assert_eq!((t.items.len(), t.postings.len(), t.currency.as_str()), (3, 4, "THB"));
        assert_eq!(t.created_by, u);
        assert_eq!(balance(svc, p, "CARD").await, d("-1250"));
        let facts = svc.lines(p, day(1), day(31)).await.unwrap();
        assert_eq!(facts.iter().map(|f| f.amount).sum::<Decimal>(), d("1250"));
    }
}

on_each_repo! {
    async fn category_kind_must_match(svc) {
        let err = svc.create(Uuid::new_v4(), Uuid::new_v4(), receipt(&[("food", "10"), ("salary", "5")])).await.unwrap_err();
        assert_eq!(problem(err), ("KIND_MISMATCH", "items[2].category_code".into()));
        let err = svc.create(Uuid::new_v4(), Uuid::new_v4(), transfer("card", "cash", "10", None, Some(("salary", "1")))).await.unwrap_err();
        assert_eq!(problem(err), ("KIND_MISMATCH", "fee_category_code".into()), "комиссия — расход");
        let err = svc.create(Uuid::new_v4(), Uuid::new_v4(), receipt(&[("nope", "10")])).await.unwrap_err();
        assert_eq!(problem(err), ("NOT_FOUND", "items[1].category_code".into()));
    }
}

on_each_repo! {
    async fn unknown_account_is_validation_error(svc) {
        let mut input = receipt(&[("food", "10")]);
        input.account_code = "NOPE".into();
        assert_eq!(problem(svc.create(Uuid::new_v4(), Uuid::new_v4(), input).await.unwrap_err()), ("NOT_FOUND", "account_code".into()));
    }
}

on_each_repo! {
    async fn plan_occurrence_is_paid_once_and_freed_on_delete(svc) {
        let (p, u, plan) = (Uuid::new_v4(), Uuid::new_v4(), RENT);
        let mut input = receipt(&[("home", "15000")]);
        input.plan_id = Some(plan);
        input.plan_date = Some(day(5));
        let t = svc.create(p, u, input).await.unwrap();
        assert_eq!(svc.payments(p, day(1), day(31)).await.unwrap().len(), 1);
        let mut again = receipt(&[("home", "15000")]);
        again.plan_id = Some(plan);
        again.plan_date = Some(day(5));
        assert!(matches!(svc.create(p, u, again).await.unwrap_err(), AppError::Conflict(_)));
        svc.delete(p, t.id).await.unwrap();
        assert!(svc.payments(p, day(1), day(31)).await.unwrap().is_empty());
    }
}

on_each_repo! {
    async fn translation_filter(svc) {
        let p = Uuid::new_v4();
        let t = svc.create(p, Uuid::new_v4(), receipt(&[("food", "10")])).await.unwrap();
        let q = || ListQuery { untranslated: Some("th".into()), ..Default::default() };
        assert_eq!(svc.list(p, q(), Pagination { page: 0, size: 10 }).await.unwrap().content.len(), 1);
        let tr = TranslationInput { description_ru: Some("Биг Си".into()), description_en: Some("ignored: original".into()), description_th: Some("บิ๊กซี".into()) };
        let t = svc.translate(p, t.id, tr).await.unwrap();
        assert_eq!(t.description_en, None, "оригинал на EN — перевод на EN не хранится");
        assert_eq!(t.title(Lang::Th), "บิ๊กซี");
        assert_eq!(svc.list(p, q(), Pagination { page: 0, size: 10 }).await.unwrap().content.len(), 0);
    }
}

on_each_repo! {
    async fn projects_are_isolated(svc) {
        let t = svc.create(Uuid::new_v4(), Uuid::new_v4(), receipt(&[("food", "10")])).await.unwrap();
        assert!(matches!(svc.get(Uuid::new_v4(), t.id).await.unwrap_err(), AppError::NotFound(_)));
    }
}

on_each_repo! {
    /// Быстрый расход: последний счёт пользователя, частые категории за 30 дней (дополняются корнями
    /// дерева), остатки лимитов и курсы — из транзакций, без новых таблиц.
    async fn quick_defaults_follow_user_habits(svc) {
        let (p, me, anna) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let fresh = svc.quick_defaults(p, me, "THB", Lang::Ru, day(13)).await.unwrap();
        assert_eq!(fresh.default_account.as_deref(), Some("CARD"), "нет истории — первый счёт");
        assert_eq!(fresh.frequent_expense, vec!["FOOD", "HOME"], "нет истории — корни дерева расходов");

        for _ in 0..3 {
            svc.create(p, me, receipt(&[("CAFE", "100")])).await.unwrap();
        }
        svc.create(p, me, TransactionInput { account_code: "WISE".into(), ..receipt(&[("HOME", "50")]) }).await.unwrap();
        svc.create(p, me, transfer("tinkoff", "card", "1000", Some("390"), None)).await.unwrap();
        svc.create(p, anna, receipt(&[("FOOD", "999")])).await.unwrap();
        let q = svc.quick_defaults(p, me, "THB", Lang::Ru, day(13)).await.unwrap();
        assert_eq!(q.default_account.as_deref(), Some("WISE"), "последний использованный этим пользователем; перевод его не сбивает");
        assert_eq!(q.last_transfer, Some(("TINKOFF".into(), "CARD".into())));
        assert_eq!(q.frequent_expense, vec!["CAFE", "HOME", "FOOD"], "сначала частые этого пользователя");
        assert_eq!(q.frequent_income, vec!["SALARY"]);
        let cafe = q.categories.iter().find(|c| c.code == "CAFE").unwrap();
        assert_eq!((cafe.available, cafe.search.contains("Cafe")), (Some(d("500")), true));
        assert_eq!(q.accounts.iter().find(|a| a.code == "WISE").unwrap().rate_to_base, Some(d("33.25")));
        // через 30 дней привычки «остывают»
        let later = svc.quick_defaults(p, me, "THB", Lang::Ru, day(3) + chrono::Days::new(40)).await.unwrap();
        assert_eq!(later.frequent_expense, vec!["FOOD", "HOME"]);
    }
}

on_each_repo! {
    /// ТЗ, раздел 13: чужой проект → 404; удаление отменяется с тем же id; проект очищается целиком.
    async fn foreign_project_undo_and_purge(svc) {
        let (mine, other, me) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let t = svc.create(mine, me, receipt(&[("FOOD", "900"), ("CAFE", "100")])).await.unwrap();
        assert!(matches!(svc.get(other, t.id).await.unwrap_err(), AppError::NotFound(_)));
        assert!(matches!(svc.update(other, t.id, receipt(&[("FOOD", "1")])).await.unwrap_err(), AppError::NotFound(_)));
        assert!(matches!(svc.delete(other, t.id).await.unwrap_err(), AppError::NotFound(_)));
        assert!(matches!(svc.delete_with_undo(other, t.id).await.unwrap_err(), AppError::NotFound(_)));
        assert!(svc.lines(other, day(1), day(31)).await.unwrap().is_empty());

        let token = svc.delete_with_undo(mine, t.id).await.unwrap();
        assert!(svc.get(mine, t.id).await.is_err());
        assert!(matches!(svc.undo_delete(other, token).await.unwrap_err(), AppError::NotFound(_)));
        let back = svc.undo_delete(mine, token).await.unwrap();
        assert_eq!((back.id, back.total, back.items.len()), (t.id, d("1000"), 2));
        assert_eq!(back.items.iter().map(|x| x.id).collect::<Vec<_>>(), t.items.iter().map(|x| x.id).collect::<Vec<_>>(), "«Отменить» возвращает позиции с теми же id");

        svc.create(other, me, receipt(&[("FOOD", "1")])).await.unwrap();
        assert_eq!(svc.purge_project(mine).await.unwrap(), 1);
        assert!(svc.lines(mine, day(1), day(31)).await.unwrap().is_empty());
        assert_eq!(svc.lines(other, day(1), day(31)).await.unwrap().len(), 1);
    }
}

on_each_repo! {
    /// ТЗ, приёмка: обмен 10 000 ₽ → 3 912,40 ฿ с комиссией 50 ₽ — одна транзакция: остатки обоих
    /// счетов верны, в факты бюджета попадает только комиссия; удаление и «Отменить» возвращают остатки.
    async fn exchange_is_one_transaction(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        svc.set_opening(p, u, "tinkoff", d("20000"), day(1)).await.unwrap();
        svc.set_opening(p, u, "card", d("1000"), day(1)).await.unwrap();
        let t = svc.create(p, u, transfer("tinkoff", "card", "10050", Some("3912.40"), Some(("fees", "50")))).await.unwrap();
        assert_eq!((t.tx_kind.as_str(), t.total, t.sent(), t.to_amount), ("TRANSFER", d("10050"), d("10000"), Some(d("3912.40"))));
        assert_eq!((t.currency.as_str(), t.to_currency.as_deref()), ("RUB", Some("THB")));
        assert_eq!(t.postings.len(), 5);
        assert_eq!((balance(svc, p, "TINKOFF").await, balance(svc, p, "CARD").await), (d("9950"), d("4912.40")));

        let facts = svc.lines(p, day(1), day(31)).await.unwrap();
        assert_eq!(facts.len(), 1, "доходы и расходы не меняются — только комиссия");
        assert_eq!((facts[0].kind.as_str(), facts[0].category_code.as_str(), facts[0].amount, facts[0].currency.as_str()), ("EXPENSE", "FEES", d("50"), "RUB"));
        let fx = svc.exchange(p, day(1), day(31)).await.unwrap();
        assert_eq!(fx.iter().map(|x| (x.currency.as_str(), x.amount)).collect::<Vec<_>>(), vec![("RUB", d("10000")), ("THB", d("-3912.40"))]);

        let token = svc.delete_with_undo(p, t.id).await.unwrap();
        assert_eq!((balance(svc, p, "TINKOFF").await, balance(svc, p, "CARD").await), (d("20000"), d("1000")));
        svc.undo_delete(p, token).await.unwrap();
        assert_eq!((balance(svc, p, "TINKOFF").await, balance(svc, p, "CARD").await), (d("9950"), d("4912.40")));
    }
}

on_each_repo! {
    async fn transfer_rules_and_type_change(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let cash = svc.create(p, u, transfer("card", "cash", "5000", None, None)).await.unwrap();
        assert_eq!((balance(svc, p, "CARD").await, balance(svc, p, "CASH").await), (d("-5000"), d("5000")));
        assert!(svc.create(p, u, transfer("card", "card", "1", None, None)).await.is_err(), "тот же счёт");
        assert!(svc.create(p, u, transfer("tinkoff", "card", "100", None, None)).await.is_err(), "при обмене нужна сумма зачисления");
        assert!(svc.create(p, u, transfer("card", "nope", "100", None, None)).await.is_err());
        // Тип можно сменить: перевод → расход, проводки пересобираются.
        let t = svc.update(p, cash.id, receipt(&[("food", "300")])).await.unwrap();
        assert_eq!((t.tx_kind.as_str(), t.to_account_code), ("EXPENSE", None));
        assert_eq!((balance(svc, p, "CARD").await, balance(svc, p, "CASH").await), (d("-300"), d("0")));
    }
}

on_each_repo! {
    /// Возврат — расход с обратными знаками: пополняет счёт и уменьшает факт категории.
    async fn refund_reduces_fact(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        svc.create(p, u, receipt(&[("food", "1000")])).await.unwrap();
        let r = svc.create(p, u, TransactionInput { refund: true, ..receipt(&[("food", "300")]) }).await.unwrap();
        assert!(r.refund);
        assert_eq!(balance(svc, p, "CARD").await, d("-700"));
        let facts = svc.lines(p, day(1), day(31)).await.unwrap();
        assert_eq!(facts.iter().map(|f| f.amount).sum::<Decimal>(), d("700"));
        let salary = svc.create(p, u, TransactionInput { lines: vec![LineDto { category_code: "salary".into(), amount: d("5000"), note: None }], ..input("INCOME", "card") }).await.unwrap();
        assert_eq!(salary.total, d("5000"));
        let facts = svc.lines(p, day(1), day(31)).await.unwrap();
        assert_eq!(facts.iter().filter(|f| f.kind == "INCOME").map(|f| f.amount).sum::<Decimal>(), d("5000"), "доход в фактах — с плюсом");
    }
}

on_each_repo! {
    /// Начальный остаток ведётся из формы счёта: один на счёт, нулевой — удаляется.
    async fn opening_balance_from_account_form(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        svc.set_opening(p, u, "card", d("12000"), day(1)).await.unwrap();
        assert_eq!(svc.opening(p, "card").await.unwrap(), Some((d("12000"), day(1))));
        svc.set_opening(p, u, "CARD", d("-150"), day(2)).await.unwrap();
        assert_eq!(svc.opening(p, "CARD").await.unwrap(), Some((d("-150"), day(2))));
        assert_eq!(balance(svc, p, "CARD").await, d("-150"));
        assert_eq!(svc.balance(p, "CARD", day(1)).await.unwrap(), Decimal::ZERO, "до даты открытия остатка нет");
        let dup = TransactionInput { amount: Some(d("1")), ..input("OPENING", "card") };
        assert!(matches!(svc.create(p, u, dup).await.unwrap_err(), AppError::Conflict(_)), "второй начальный остаток");
        assert!(svc.has_postings(p, "CARD").await.unwrap());
        svc.set_opening(p, u, "card", Decimal::ZERO, day(2)).await.unwrap();
        assert_eq!(svc.opening(p, "card").await.unwrap(), None);
        assert!(!svc.has_postings(p, "CARD").await.unwrap());
        let err = svc.set_opening(p, u, "card", d("1.005"), day(2)).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(m) if m.starts_with("opening_balance")));
        assert_eq!(svc.first_expenses_between(chrono::Utc::now(), chrono::Utc::now()).await.unwrap(), 0, "начальный остаток — не первая операция");
    }
}

on_each_repo! {
    /// «Сверить остаток»: разница вносится расходом или доходом на выбранную категорию.
    async fn reconcile_balance(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let desc = || Description { text: Some("Сверка".into()), lang: Lang::Ru, ..Default::default() };
        svc.set_opening(p, u, "card", d("1000"), day(1)).await.unwrap();
        assert!(svc.reconcile(p, u, "card", d("1000"), day(5), "food", desc()).await.unwrap().is_none(), "совпало — вносить нечего");
        let t = svc.reconcile(p, u, "card", d("940"), day(5), "food", desc()).await.unwrap().unwrap();
        assert_eq!((t.tx_kind.as_str(), t.total), ("EXPENSE", d("60")));
        let err = svc.reconcile(p, u, "card", d("1000"), day(5), "food", desc()).await.unwrap_err();
        assert!(err.to_string().contains("дохода"), "остаток больше — нужна категория дохода: {err}");
        svc.reconcile(p, u, "card", d("1000"), day(5), "salary", desc()).await.unwrap().unwrap();
        assert_eq!(balance(svc, p, "CARD").await, d("1000"));
    }
}

on_each_repo! {
    /// Фильтр по счёту — выписка: переводы видны на обоих счетах; фильтр по типу.
    async fn statement_and_kind_filters(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        svc.create(p, u, receipt(&[("food", "100")])).await.unwrap();
        svc.create(p, u, transfer("card", "cash", "500", None, None)).await.unwrap();
        let page = Pagination { page: 0, size: 50 };
        let q = |account: &str, kind: Option<&str>| ListQuery { account_code: Some(account.into()), tx_kind: kind.map(Into::into), ..Default::default() };
        let cash = svc.list(p, q("CASH", None), page.clone()).await.unwrap().content;
        assert_eq!(cash.len(), 1);
        assert_eq!(cash[0].amount_on("CASH"), d("500"));
        assert_eq!(svc.list(p, q("CARD", None), page.clone()).await.unwrap().content.len(), 2);
        assert_eq!(svc.list(p, q("CARD", Some("TRANSFER")), page.clone()).await.unwrap().content.len(), 1);
        assert!(svc.list(p, q("CARD", Some("BOGUS")), page).await.is_err());
    }
}

on_each_repo! {
    /// Ссылка на счёт, ушедший в архив, остаётся валидной при правке транзакции.
    async fn archived_refs_stay_valid_on_edit(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        assert!(svc.create(p, u, receipt(&[("food", "1")]).with_account("old")).await.is_err(), "на архивный счёт не сослаться заново");
        let t = svc.create(p, u, receipt(&[("food", "1")])).await.unwrap();
        // правка описания и суммы без смены ссылок проходит
        let mut again = receipt(&[("food", "2")]);
        again.description = Some("Big C, 2".into());
        assert_eq!(svc.update(p, t.id, again).await.unwrap().total, d("2"));
    }
}

impl TransactionInput {
    fn with_account(self, account: &str) -> Self {
        Self { account_code: account.into(), ..self }
    }
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing =
        kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
    // Ключи, которые код собирает не через `.t("…")`.
    let dynamic = [
        "balance_start",
        "balance_end",
        "inflow",
        "outflow",
        "income_total",
        "expense_total",
        "net",
        "transfer",
        "opening",
        "refund",
        "rate",
        "fee_short",
        "kinds.EXPENSE",
        "kinds.INCOME",
        "kinds.TRANSFER",
        "kinds.OPENING",
        "sys.EXCHANGE",
        "sys.OPENING",
        "reconcile_done",
        "reconcile_same",
        "reconcile_desc",
        "nav_accounts",
        "tx.sms_created",
        "tx.sms_matched",
    ];
    let absent: Vec<&str> = dynamic.into_iter().filter(|k| crate::i18n::dict(k).is_none()).collect();
    assert!(absent.is_empty(), "нет перевода для ключей: {absent:?}");
}

on_each_repo! {
    /// Ц7: оплатить можно только настоящее вхождение плана: план есть, дата из правила, тип совпадает,
    /// платёж не пропущен и ещё не оплачен.
    async fn unknown_plan_occurrence_is_rejected(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let pay = |plan: Uuid, date: u32, kind: &str| {
            let mut input = TransactionInput { lines: vec![LineDto { category_code: if kind == "INCOME" { "salary" } else { "home" }.into(), amount: d("1539.87"), note: None }], ..input(kind, "card") };
            input.plan_id = Some(plan);
            input.plan_date = Some(day(date));
            input
        };
        let err = |e: AppError, field: &str| matches!(e, AppError::Invalid(p) if p.field == field);
        assert!(err(svc.create(p, u, pay(Uuid::new_v4(), 5, "EXPENSE")).await.unwrap_err(), "plan_date"), "нет такого плана");
        assert!(err(svc.create(p, u, pay(RENT, 17, "EXPENSE")).await.unwrap_err(), "plan_date"), "17-е — не дата правила (платили 17-го, а план на 5-е)");
        assert!(err(svc.create(p, u, pay(RENT, 5, "INCOME")).await.unwrap_err(), "plan_id"), "план — расход, операция — доход");
        assert!(err(svc.create(p, u, pay(RENT, 12, "EXPENSE")).await.unwrap_err(), "plan_date"), "платёж пропущен");
        let t = svc.create(p, u, pay(RENT, 5, "EXPENSE")).await.unwrap();
        assert!(matches!(svc.create(p, u, pay(RENT, 5, "EXPENSE")).await.unwrap_err(), AppError::Conflict(_)), "уже оплачено");
        // Правка оплаты (сумма) с тем же вхождением — без повторной проверки и без конфликта с самой собой.
        let mut fix = pay(RENT, 5, "EXPENSE");
        fix.lines[0].amount = d("1600");
        assert_eq!(svc.update(p, t.id, fix).await.unwrap().total, d("1600"));
    }
}

/// Ц10: у счёта один начальный остаток — это держит уникальный индекс БД, а не только проверка в сервисе.
/// Двойной клик «Сохранить» в форме счёта больше не удваивает остаток. Только на PostgreSQL.
#[tokio::test]
async fn second_opening_is_rejected_by_db() {
    use crate::domain::{Draft, Transaction, TransactionData};
    use crate::ports::TransactionRepository;
    let Some(db) = TestDb::migrated("tx_opening", vec![sqlx::migrate!("./migrations")]).await else { return };
    let pool = db.pool.clone();
    let repo = Arc::new(crate::adapters::postgres::PostgresTransactionRepository::new(pool.clone()));
    let accounts = [AccountCurrency { code: "CARD".into(), currency: "THB".into(), minor_units: 2 }];
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    // Оба запроса прошли проверку «остатка ещё нет» до того, как другой записал свой.
    let opening = || {
        let data = TransactionData {
            occurred_on: day(15),
            description: Description::default(),
            plan: None,
            place_code: None,
            body: Draft::Opening { account: "card".into(), amount: d("1961.31") },
        };
        Transaction::create(p, u, data, &accounts).unwrap()
    };
    repo.insert(&opening()).await.unwrap();
    let second = repo.insert(&opening()).await;
    // Два одновременных сохранения из формы счёта: оба успешны, остаток один — последний.
    let svc = TransactionService::new(repo, deps());
    let q = Uuid::new_v4();
    let (a, b) = tokio::join!(
        svc.set_opening(q, u, "card", d("500"), day(15)),
        svc.set_opening(q, u, "card", d("500"), day(15))
    );
    let count = |project: Uuid| {
        sqlx::query_as::<_, (i64,)>(
            "SELECT count(*) FROM fb_transactions WHERE tx_kind = 'OPENING' AND project_id = $1",
        )
        .bind(project)
        .fetch_one(&pool)
    };
    let (n, m) = (count(p).await.unwrap().0, count(q).await.unwrap().0);
    let balance = svc.balance(q, "CARD", day(31)).await;
    db.close().await;
    assert!(matches!(second, Err(AppError::Conflict(_))), "второй начальный остаток принят: {second:?}");
    assert_eq!(n, 1, "два начальных остатка у одного счёта — остаток удвоен");
    assert!(a.is_ok() && b.is_ok(), "{a:?} {b:?}");
    assert_eq!((m, balance.unwrap()), (1, d("500")));
}

on_each_repo! {
    /// Ц10 на in-memory адаптере: он повторяет уникальность БД, и сервис отвечает понятной ошибкой.
    async fn opening_is_one_per_account(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        svc.set_opening(p, u, "card", d("1000"), day(1)).await.unwrap();
        svc.set_opening(p, u, "card", d("1200"), day(1)).await.unwrap();
        assert_eq!(balance(svc, p, "CARD").await, d("1200"), "повторное сохранение меняет остаток, а не добавляет второй");
        let mut again = input("OPENING", "card");
        again.amount = Some(d("1"));
        let err = svc.create(p, u, again).await.unwrap_err();
        assert!(matches!(err, AppError::Conflict(m) if m.contains("CARD")), "второй остаток через API");
    }
}

fn priced(category: &str, name: &str, qty: &str, unit: &str, price: &str, discount: &str) -> ItemInput {
    ItemInput {
        category_code: Some(category.into()),
        name: Some(name.into()),
        quantity: Some(d(qty)),
        unit: Some(unit.into()),
        price_per_unit: Some(d(price)),
        discount: Some(d(discount)),
        ..Default::default()
    }
}

fn items(list: Vec<ItemInput>) -> TransactionInput {
    TransactionInput { items: list, ..input("EXPENSE", "card") }
}

on_each_repo! {
    /// Группа 1, шаг 2: чек с ценами — суммы позиций считает сервер, у каждой позиции своя проводка.
    async fn receipt_with_prices(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let mut input = items(vec![
            priced("food", "Вода 1,5 л", "1", "pcs", "14", "0"),
            priced("cafe", "Круассан", "2", "pcs", "25", "3"),
            priced("food", "Бананы", "0.45", "kg", "60", "0"),
            ItemInput { category_code: Some("cafe".into()), name: Some("Латте".into()), amount: Some(d("45")), ..Default::default() },
        ]);
        input.discount_total = Some(d("13.30"));
        let t = svc.create(p, u, input).await.unwrap();
        // 14 + 47 + 27 + 45 = 133; скидка на чек 13,30 = 10 %: 1,40 + 4,70 + 2,70 + 4,50.
        assert_eq!(t.total, d("119.70"));
        assert_eq!(t.items.iter().map(|x| (x.amount, x.discount)).collect::<Vec<_>>(), vec![(d("12.60"), d("1.40")), (d("42.30"), d("7.70")), (d("24.30"), d("2.70")), (d("40.50"), d("4.50"))]);
        assert_eq!((t.items[2].unit.as_str(), t.items[2].quantity, t.items[3].price_per_unit), ("kg", d("0.45"), None));
        let again = svc.get(p, t.id).await.unwrap();
        assert_eq!(again.items.iter().map(|x| (x.id, x.amount, x.name.clone())).collect::<Vec<_>>(), t.items.iter().map(|x| (x.id, x.amount, x.name.clone())).collect::<Vec<_>>());
        let cats: Vec<_> = again.postings.iter().filter(|x| x.ref_kind == "CATEGORY").map(|x| x.amount).collect();
        assert_eq!(cats, again.items.iter().map(|x| x.amount).collect::<Vec<_>>(), "одна проводка на позицию, сумма — сумма позиции");
        assert_eq!(balance(svc, p, "CARD").await, d("-119.70"));
        // Сумма не сходится с ценой — ошибка с кодом.
        let wrong = items(vec![ItemInput { amount: Some(d("50")), ..priced("cafe", "Круассан", "2", "pcs", "25", "3") }]);
        assert_eq!(problem(svc.create(p, u, wrong).await.unwrap_err()), ("AMOUNT_MISMATCH", "items[1].amount".into()));
    }
}

on_each_repo! {
    /// Раздел 5.6: правка по id — id позиций не меняются; не названные позиции остаются, remove_item_ids удаляет.
    async fn items_are_edited_by_id(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let t = svc.create(p, u, receipt(&[("food", "100"), ("cafe", "50"), ("home", "30")])).await.unwrap();
        let ids: Vec<Uuid> = t.items.iter().map(|x| x.id).collect();
        let mut edit = items(vec![ItemInput { id: Some(ids[1]), amount: Some(d("60")), name: Some("Кофе".into()), ..Default::default() }, ItemInput { category_code: Some("food".into()), amount: Some(d("5")), ..Default::default() }]);
        edit.remove_item_ids = vec![ids[2]];
        let t = svc.update(p, t.id, edit).await.unwrap();
        assert_eq!(
            t.items.iter().map(|x| (x.category_code.as_str(), x.amount, x.name.as_deref())).collect::<Vec<_>>(),
            vec![("FOOD", d("100"), None), ("CAFE", d("60"), Some("Кофе")), ("FOOD", d("5"), None)]
        );
        assert_eq!((t.items[0].id, t.items[1].id), (ids[0], ids[1]));
        assert_eq!(svc.get(p, t.id).await.unwrap().items.iter().map(|x| x.id).take(2).collect::<Vec<_>>(), vec![ids[0], ids[1]]);
        assert_eq!(balance(svc, p, "CARD").await, d("-165"));
        // Позиция чужой операции — ошибка поля.
        let foreign = items(vec![ItemInput { id: Some(Uuid::new_v4()), amount: Some(d("1")), ..Default::default() }]);
        assert_eq!(problem(svc.update(p, t.id, foreign).await.unwrap_err()).1, "items[1].id");
        // Старый формат `lines` заменяет весь чек — как раньше.
        let t = svc.update(p, t.id, receipt(&[("home", "7")])).await.unwrap();
        assert_eq!((t.items.len(), t.total), (1, d("7")));
        assert!(!ids.contains(&t.items[0].id));
    }
}

on_each_repo! {
    /// Техдолг #3: правка проверяет тип категории и у ссылок из прежней версии — доход не превратить в
    /// расход на категории дохода; архивная категория остаётся в старой операции, но не в новой.
    async fn kept_category_is_checked_by_kind(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let salary = TransactionInput { lines: vec![LineDto { category_code: "salary".into(), amount: d("5000"), note: None }], ..input("INCOME", "card") };
        let t = svc.create(p, u, salary).await.unwrap();
        let as_expense = TransactionInput { lines: vec![LineDto { category_code: "salary".into(), amount: d("5000"), note: None }], ..input("EXPENSE", "card") };
        assert_eq!(problem(svc.update(p, t.id, as_expense).await.unwrap_err()), ("KIND_MISMATCH", "items[1].category_code".into()));
        assert_eq!(problem(svc.create(p, u, receipt(&[("old_food", "1")])).await.unwrap_err()), ("NOT_FOUND", "items[1].category_code".into()), "архивная — только в старых операциях");
    }
}

on_each_repo! {
    /// Быстрый ввод и старые клиенты: счёт, категория и сумма — одна позиция «1 шт» без цены (раздел 10).
    async fn minimal_request_is_one_simple_item(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let quick = items(vec![ItemInput { category_code: Some("cafe".into()), amount: Some(d("55")), ..Default::default() }]);
        let t = svc.create(p, u, quick).await.unwrap();
        let x = &t.items[0];
        assert_eq!((x.quantity, x.unit.as_str(), x.price_per_unit, x.discount, x.amount, x.is_simple()), (d("1"), "pcs", None, d("0"), d("55"), true));
        let legacy = svc.create(p, u, receipt(&[("food", "10")])).await.unwrap();
        assert!(legacy.items[0].is_simple());
        assert_eq!((legacy.lines[0].category_code.as_str(), legacy.lines[0].amount), ("FOOD", d("10")), "прежние клиенты API читают lines, как раньше");
        let both = TransactionInput { items: vec![ItemInput::default()], ..receipt(&[("food", "10")]) };
        assert_eq!(problem(svc.create(p, u, both).await.unwrap_err()).1, "items");
    }
}

/// Миграция позиций (docs/group-1.md, раздел 7) на старых данных: каждая проводка на категорию расхода и дохода
/// становится позицией «1 шт» с названием из уточнения, проводке ставится item_id, шапка получает счёт, сумму и
/// признак возврата. Остатки не меняются. Только на PostgreSQL.
#[tokio::test]
async fn receipt_items_migration_on_old_data() {
    use crate::ports::TransactionRepository;
    let Some(db) = TestDb::new("tx_items_mig").await else { return };
    let mut before = sqlx::migrate!("./migrations");
    let all = before.migrations.to_vec();
    // Схема до миграции позиций (20261019130736_receipt_items) — следующие миграции применяются вместе с ней.
    let items = 20261019130736;
    assert!(all.iter().any(|m| m.version == items));
    before.migrations = all.iter().filter(|m| m.version < items).cloned().collect::<Vec<_>>().into();
    before.set_locking(false);
    before.run(&db.pool).await.unwrap();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let (receipt_id, refund_id, salary_id, transfer_id, opening_id) =
        (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let header = |id: Uuid, kind: &str, n: i64| {
        sqlx::query("INSERT INTO fb_transactions (id, project_id, occurred_on, tx_kind, description_lang, created_by, opening_account) VALUES ($1, $2, $3, $4, 'RU', $5, $6)")
            .bind(id)
            .bind(p)
            .bind(day(n as u32))
            .bind(kind.to_string())
            .bind(u)
            .bind((kind == "OPENING").then_some("CARD"))
    };
    let posting = |id: Uuid, pos: i32, kind: &str, code: &str, cur: &str, amount: &str, note: Option<&str>| {
        sqlx::query("INSERT INTO fb_postings (transaction_id, position, ref_kind, ref_code, currency_code, amount, note) VALUES ($1, $2, $3, $4, $5, $6, $7)")
            .bind(id)
            .bind(pos)
            .bind(kind.to_string())
            .bind(code.to_string())
            .bind(cur.to_string())
            .bind(d(amount))
            .bind(note.map(str::to_string))
    };
    let mut tx = db.pool.begin().await.unwrap();
    for (id, kind, n) in [
        (opening_id, "OPENING", 1),
        (receipt_id, "EXPENSE", 2),
        (refund_id, "EXPENSE", 3),
        (salary_id, "INCOME", 4),
        (transfer_id, "TRANSFER", 5),
    ] {
        header(id, kind, n).execute(&mut *tx).await.unwrap();
    }
    for q in [
        posting(opening_id, 1, "ACCOUNT", "CARD", "THB", "1000", None),
        posting(opening_id, 2, "SYSTEM", "OPENING", "THB", "-1000", None),
        posting(receipt_id, 1, "ACCOUNT", "CARD", "THB", "-1250", None),
        posting(receipt_id, 2, "CATEGORY", "FOOD", "THB", "900", Some("овощи, мясо")),
        posting(receipt_id, 3, "CATEGORY", "HOME", "THB", "250", None),
        posting(receipt_id, 4, "CATEGORY", "FOOD", "THB", "100", Some("кофе")),
        posting(refund_id, 1, "ACCOUNT", "CARD", "THB", "300", None),
        posting(refund_id, 2, "CATEGORY", "FOOD", "THB", "-300", None),
        posting(salary_id, 1, "ACCOUNT", "CARD", "THB", "5000", None),
        posting(salary_id, 2, "CATEGORY", "SALARY", "THB", "-5000", None),
        posting(transfer_id, 1, "ACCOUNT", "TINKOFF", "RUB", "-10050", None),
        posting(transfer_id, 2, "CATEGORY", "FEES", "RUB", "50", None),
        posting(transfer_id, 3, "SYSTEM", "EXCHANGE", "RUB", "10000", None),
        posting(transfer_id, 4, "SYSTEM", "EXCHANGE", "THB", "-3912.40", None),
        posting(transfer_id, 5, "ACCOUNT", "CARD", "THB", "3912.40", None),
    ] {
        q.execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    let repo = crate::adapters::postgres::PostgresTransactionRepository::new(db.pool.clone());
    let balances = |r: Vec<(String, Decimal)>| {
        let mut r = r;
        r.sort();
        r
    };
    let balances_before = balances(repo.balances(p, day(31)).await.unwrap());

    let mut after = sqlx::migrate!("./migrations");
    after.set_locking(false);
    after.run(&db.pool).await.unwrap();

    assert_eq!(balances(repo.balances(p, day(31)).await.unwrap()), balances_before, "остатки не меняются");
    let load = |id: Uuid| repo.find(p, id);
    let receipt = load(receipt_id).await.unwrap().unwrap();
    assert_eq!(
        receipt
            .items
            .iter()
            .map(|x| (x.category_code.as_str(), x.name.as_deref(), x.amount, x.quantity, x.price))
            .collect::<Vec<_>>(),
        vec![
            ("FOOD", Some("овощи, мясо"), d("900"), d("1"), None),
            ("HOME", None, d("250"), d("1"), None),
            ("FOOD", Some("кофе"), d("100"), d("1"), None)
        ]
    );
    for t in [
        &receipt,
        &load(refund_id).await.unwrap().unwrap(),
        &load(salary_id).await.unwrap().unwrap(),
        &load(transfer_id).await.unwrap().unwrap(),
        &load(opening_id).await.unwrap().unwrap(),
    ] {
        crate::domain::check_items(t.tx_kind, &t.items, &t.postings).unwrap_or_else(|e| panic!("{:?}: {e}", t.tx_kind));
    }
    let heads: Vec<(Uuid, Option<String>, Option<Decimal>, bool)> =
        sqlx::query_as("SELECT id, account_code, total, refund FROM fb_transactions WHERE project_id = $1")
            .bind(p)
            .fetch_all(&db.pool)
            .await
            .unwrap();
    let head = |id: Uuid| {
        heads
            .iter()
            .find(|h| h.0 == id)
            .map(|h| (h.1.clone().unwrap_or_default(), h.2.unwrap_or_default().normalize(), h.3))
            .unwrap()
    };
    assert_eq!(head(receipt_id), ("CARD".into(), d("1250"), false));
    assert_eq!(head(refund_id), ("CARD".into(), d("300"), true));
    assert_eq!(head(salary_id), ("CARD".into(), d("5000"), false));
    assert_eq!(head(transfer_id), ("TINKOFF".into(), d("10050"), false), "у перевода — счёт списания и списано всего");
    assert_eq!(head(opening_id), ("CARD".into(), d("1000"), false));
    let refund = load(refund_id).await.unwrap().unwrap();
    assert!(refund.refund() && refund.items[0].amount == d("300"));
    // Число позиций = числу проводок на категории расходов и доходов (сверка из раздела 7).
    let (n_items,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM fb_transaction_items").fetch_one(&db.pool).await.unwrap();
    assert_eq!(n_items, 5);
    // Шапка после записи приложением — та же, что после миграции.
    let svc = TransactionService::new(Arc::new(repo), deps());
    let edited = svc
        .update(
            p,
            receipt_id,
            TransactionInput {
                items: vec![ItemInput { id: Some(receipt.items[1].id), amount: Some(d("260")), ..Default::default() }],
                ..input("EXPENSE", "card")
            },
        )
        .await
        .unwrap();
    assert_eq!((edited.total, edited.items[1].id), (d("1260"), receipt.items[1].id));
    let (total,): (Decimal,) = sqlx::query_as("SELECT total FROM fb_transactions WHERE id = $1")
        .bind(receipt_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(total, d("1260"));
    db.close().await;
}

on_each_repo! {
    /// Шаг 5: место подставляет счёт и категорию новых позиций, если их не указали (раздел 5.2); указанное
    /// важнее места; у перевода места нет; лента фильтруется по месту.
    async fn place_fills_account_and_category(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let at_711 = |items: Vec<ItemInput>| TransactionInput { place_code: Some(" seven_eleven ".into()), items, ..input("EXPENSE", "") };
        let t = svc.create(p, u, at_711(vec![ItemInput { amount: Some(d("55")), ..Default::default() }])).await.unwrap();
        assert_eq!((t.place_code.as_deref(), t.account_code.as_str(), t.items[0].category_code.as_str()), (Some("SEVEN_ELEVEN"), "CASH", "FOOD"));
        let own = TransactionInput { account_code: "card".into(), ..at_711(vec![ItemInput { category_code: Some("cafe".into()), amount: Some(d("45")), ..Default::default() }]) };
        let t2 = svc.create(p, u, own).await.unwrap();
        assert_eq!((t2.account_code.as_str(), t2.items[0].category_code.as_str()), ("CARD", "CAFE"), "указанное важнее места");
        let tr = svc.create(p, u, TransactionInput { place_code: Some("SEVEN_ELEVEN".into()), ..transfer("card", "cash", "100", None, None) }).await.unwrap();
        assert_eq!(tr.place_code, None, "у перевода места нет");
        svc.create(p, u, receipt(&[("food", "10")])).await.unwrap();
        let at = ListQuery { place_code: Some("seven_eleven".into()), ..Default::default() };
        assert_eq!(svc.feed(p, at).await.unwrap().items.len(), 2);
        for bad in ["NOWHERE", "OLD"] {
            let e = svc.create(p, u, TransactionInput { place_code: Some(bad.into()), ..receipt(&[("food", "10")]) }).await.unwrap_err();
            assert_eq!(problem(e), (kernel::error::codes::NOT_FOUND, "place_code".to_string()), "{bad}: нет или в архиве");
        }
        // Подставляется только подходящее: доходная категория места не попадает в расход, неактивный счёт — в операцию.
        let wrong = TransactionInput { place_code: Some("PAYDAY".into()), items: vec![ItemInput { amount: Some(d("5")), ..Default::default() }], ..input("EXPENSE", "card") };
        assert_eq!(problem(svc.create(p, u, wrong).await.unwrap_err()).1, "items[1].category_code", "«выберите категорию», а не «не тот тип»");
        let no_acc = TransactionInput { place_code: Some("PAYDAY".into()), ..items(vec![ItemInput { category_code: Some("food".into()), amount: Some(d("5")), ..Default::default() }]) };
        assert_eq!(problem(svc.create(p, u, TransactionInput { account_code: String::new(), ..no_acc }).await.unwrap_err()).1, "account_code");
        // Правка без места убирает его; позиции с `id` категорию от места не получают.
        let mut edit = TransactionInput { items: vec![ItemInput { id: Some(t.items[0].id), ..Default::default() }], ..input("EXPENSE", "cash") };
        edit.place_code = None;
        let edited = svc.update(p, t.id, edit).await.unwrap();
        assert_eq!((edited.place_code, edited.items[0].category_code.as_str()), (None, "FOOD"));
    }
}

fn item(cat: &str, amount: &str, who: Option<&str>) -> ItemInput {
    ItemInput {
        category_code: Some(cat.into()),
        amount: Some(d(amount)),
        for_person_code: who.map(Into::into),
        ..Default::default()
    }
}

on_each_repo! {
    /// Шаг 6 (разделы 5.2, 5.6): «Для кого» шапки получают позиции без своего получателя, `all` — «Для всех» явно;
    /// шапка не хранится — выводится из позиций; новый получатель — только активный человек.
    async fn recipients_on_create(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let plain = svc.create(p, u, receipt(&[("food", "10")])).await.unwrap();
        assert_eq!((plain.items[0].for_person_code.clone(), plain.for_person_code.clone(), plain.recipients_differ), (None, None, false), "по умолчанию — для всех");
        let one = TransactionInput { for_person_code: Some("pinpan".into()), items: vec![item("food", "45", None), item("cafe", "22", None)], ..input("EXPENSE", "card") };
        let t = svc.create(p, u, one).await.unwrap();
        assert!(t.items.iter().all(|x| x.for_person_code.as_deref() == Some("PINPAN")));
        assert_eq!((t.for_person_code.as_deref(), t.recipients_differ), (Some("PINPAN"), false));
        let mixed = TransactionInput {
            for_person_code: Some("PINPAN".into()),
            items: vec![item("food", "88", Some("all")), item("cafe", "45", Some("anna")), item("home", "22", None)],
            ..input("EXPENSE", "card")
        };
        let t = svc.create(p, u, mixed).await.unwrap();
        let who: Vec<Option<&str>> = t.items.iter().map(|x| x.for_person_code.as_deref()).collect();
        assert_eq!(who, vec![None, Some("ANNA"), Some("PINPAN")]);
        assert_eq!((t.for_person_code.clone(), t.recipients_differ), (None, true));
        for (bad, field) in [(TransactionInput { for_person_code: Some("OLD".into()), ..receipt(&[("food", "1")]) }, "for_person_code"), (TransactionInput { items: vec![item("food", "1", Some("NOBODY"))], ..input("EXPENSE", "card") }, "items[1].for_person_code")] {
            assert_eq!(problem(svc.create(p, u, bad).await.unwrap_err()), (kernel::error::codes::NOT_FOUND, field.to_string()));
        }
        for who in ["PINPAN", "GHOST"] {
            let tr = svc.create(p, u, TransactionInput { for_person_code: Some(who.into()), ..transfer("card", "cash", "100", None, None) }).await.unwrap();
            assert_eq!((tr.items.len(), tr.for_person_code), (0, None), "у перевода получателя нет — поле отбрасывается: {who}");
        }
    }
}

on_each_repo! {
    /// Правка (5.6): смена шапки меняет только позиции с прежним выведенным получателем; не названные позиции
    /// остаются как были; новая позиция берёт шапку; получатель, который уже был, остаётся и после архива.
    async fn recipients_on_update(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let t = svc.create(p, u, TransactionInput { items: vec![item("food", "88", None), item("cafe", "45", Some("ANNA")), item("home", "22", None)], ..input("EXPENSE", "card") }).await.unwrap();
        // Выведенная шапка — «Для всех» (разные): PINPAN получают позиции «Для всех», у Анны — её позиция.
        let ids: Vec<Uuid> = t.items.iter().map(|x| x.id).collect();
        let patch = TransactionInput { for_person_code: Some("PINPAN".into()), items: vec![ItemInput { id: Some(ids[0]), ..Default::default() }, item("food", "5", None)], ..input("EXPENSE", "card") };
        let t2 = svc.update(p, t.id, patch).await.unwrap();
        let who: Vec<Option<&str>> = t2.items.iter().map(|x| x.for_person_code.as_deref()).collect();
        assert_eq!(who, vec![Some("PINPAN"), Some("ANNA"), Some("PINPAN"), Some("PINPAN")]);
        // Удаление позиции и смена шапки разом: прежний получатель — по остающимся позициям.
        let two = svc.create(p, u, TransactionInput { items: vec![item("food", "1", Some("PINPAN")), item("cafe", "2", None)], ..input("EXPENSE", "card") }).await.unwrap();
        let drop_b = TransactionInput { remove_item_ids: vec![two.items[1].id], for_person_code: Some("ANNA".into()), ..input("EXPENSE", "card") };
        assert_eq!(svc.update(p, two.id, drop_b).await.unwrap().for_person_code.as_deref(), Some("ANNA"));
        // Без шапки — получатели как были; позиция по `id` с `all` — «Для всех».
        let t3 = svc.update(p, t.id, TransactionInput { items: vec![ItemInput { id: Some(ids[1]), for_person_code: Some("all".into()), ..Default::default() }], ..input("EXPENSE", "card") }).await.unwrap();
        assert_eq!(t3.items.iter().filter(|x| x.for_person_code.is_none()).count(), 1);
    }
}

/// Получатель в архиве остаётся у операции, у которой он уже был; новой операции его не дать.
#[test]
fn archived_recipient_is_kept() {
    let people = vec![PersonInfo {
        code: "OLD".into(),
        title: "old".into(),
        search: "OLD".into(),
        user_id: None,
        archived: true,
    }];
    let accounts = vec![AccountCurrency { code: "CARD".into(), currency: "THB".into(), minor_units: 2 }];
    let items = ItemsDraft {
        for_default: Some(Some("OLD".into())),
        ..ItemsDraft::replace(vec![ItemDraft::simple("FOOD", d("3"))])
    };
    let data = TransactionData {
        occurred_on: day(3),
        description: Description::default(),
        plan: None,
        place_code: None,
        body: Draft::Flow { income: false, account: "CARD".into(), items, refund: false },
    };
    let t = Transaction::create(Uuid::new_v4(), Uuid::new_v4(), data, &accounts).unwrap();
    assert!(crate::service::check_people(&people, &t, None).is_err(), "новый получатель — только активный");
    assert!(crate::service::check_people(&people, &t, Some(&t)).is_ok(), "прежний остаётся");
    assert!(crate::service::check_people(&[], &t, Some(&t)).is_ok(), "и без справочника людей (фича выключена)");
}

on_each_repo! {
    /// Фильтр «Для кого» (раздел 6): операции с позицией этого человека, `all` — с позицией «Для всех»;
    /// итоги ленты — только по его позициям.
    async fn feed_by_recipient(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        svc.create(p, u, TransactionInput { items: vec![item("food", "88", None), item("cafe", "45", Some("ANNA")), item("home", "22", Some("PINPAN"))], ..input("EXPENSE", "card") }).await.unwrap();
        svc.create(p, u, TransactionInput { for_person_code: Some("PINPAN".into()), items: vec![item("food", "10", None)], ..input("EXPENSE", "card") }).await.unwrap();
        svc.create(p, u, receipt(&[("food", "7")])).await.unwrap();
        let by = |who: &str| ListQuery { for_person_code: Some(who.into()), ..Default::default() };
        let pinpan = svc.feed(p, by("pinpan")).await.unwrap();
        assert_eq!((pinpan.items.len(), pinpan.totals.clone()), (2, vec![("THB".to_string(), Decimal::ZERO, d("32"))]));
        let all = svc.feed(p, by("all")).await.unwrap();
        assert_eq!((all.items.len(), all.totals[0].2), (2, d("95")));
        assert_eq!(svc.list(p, by("ANNA"), Pagination { page: 0, size: 10 }).await.unwrap().page.total_elements, 1);
        assert_eq!(svc.feed(p, by(" ")).await.unwrap().items.len(), 3, "пустой параметр — без фильтра");
        // Со счётом: выписки нет (проводки на счёт не делятся по людям), итоги — доля человека.
        let with_acc = svc.feed(p, ListQuery { account_code: Some("CARD".into()), ..by("PINPAN") }).await.unwrap();
        assert!(with_acc.statement.is_none());
        assert_eq!(with_acc.totals[0].2, d("32"));
    }
}

/// Техдолг #9: тексты ошибок фичи — только в словаре `i18n.rs`; в рабочем коде нет строк на русском,
/// иначе участник с английским или тайским интерфейсом увидит ошибку по-русски. `http.rs` не проверяется:
/// в нём только описание API для разработчиков (OpenAPI), оно на русском.
#[test]
fn no_russian_literals_in_code() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["domain.rs", "service.rs", "dto.rs", "ports.rs", "adapters/web.rs", "adapters/postgres.rs"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        for (n, line) in code.lines().enumerate() {
            let line = line.trim_start();
            if line.starts_with("//") || line.starts_with("#[schema") {
                continue;
            }
            // Строковые литералы строки (комментарий после кода отрезан: в литералах фичи нет «//»).
            let code_part = line.split(" //").next().unwrap_or_default();
            let literals: Vec<&str> = code_part.split('"').skip(1).step_by(2).collect();
            let russian = literals.iter().find(|l| {
                l.chars().any(|c| ('а'..='я').contains(&c.to_ascii_lowercase()) || ('А'..='Я').contains(&c) || c == 'ё')
            });
            assert!(
                russian.is_none(),
                "{file}:{}: строка на русском в коде — перенесите в i18n.rs: {:?}",
                n + 1,
                russian
            );
        }
    }
}

on_each_repo! {
    /// Техдолги #25, #26: итоги ленты и выписка считает сервис по всем операциям фильтра; нарастающий остаток —
    /// от остатка на конец, поэтому верен и для обрезанной ленты.
    async fn feed_totals_and_statement(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        svc.set_opening(p, u, "card", d("1000"), day(1)).await.unwrap();
        svc.create(p, u, receipt(&[("food", "300"), ("cafe", "50")])).await.unwrap();
        svc.create(p, u, TransactionInput { refund: true, ..receipt(&[("food", "100")]) }).await.unwrap();
        svc.create(p, u, TransactionInput { lines: vec![LineDto { category_code: "salary".into(), amount: d("5000"), note: None }], ..input("INCOME", "card") }).await.unwrap();
        svc.create(p, u, transfer("card", "cash", "500", None, Some(("fees", "10")))).await.unwrap();
        let month = || ListQuery { from: Some(day(2)), to: Some(day(31)), ..Default::default() };
        let feed = svc.feed(p, month()).await.unwrap();
        assert!(!feed.truncated);
        assert_eq!(feed.items.len(), 4);
        // Расходы: 300 + 50 − 100 (возврат) + 10 (комиссия) = 260; доходы 5000.
        assert_eq!(feed.totals, vec![("THB".into(), d("5000"), d("260"))]);
        let st = svc.feed(p, ListQuery { account_code: Some("CARD".into()), ..month() }).await.unwrap().statement.unwrap();
        assert_eq!((st.start, st.end, st.inflow, st.outflow), (d("1000"), d("5250"), d("5100"), d("-850")));
        let feed = svc.feed(p, ListQuery { account_code: Some("CARD".into()), ..month() }).await.unwrap();
        let newest = feed.items[0].id;
        assert_eq!(st.running.first().map(|r| r.1), Some(d("5250")), "после самой новой операции — остаток на конец");
        assert_eq!(st.running.first().map(|r| r.0), Some(newest));
        assert_eq!(st.running.last().map(|r| r.1), Some(d("650")), "после самой старой: 1000 − 350");
    }
}

// ---------------------------------------------------------------- связи с внешними записями (docs/external-refs.md, раздел 6)

/// SMS банка с id `guid`, пришло 03.10 в 10:51 по Бангкоку.
fn sms(guid: &str) -> ExternalRefInput {
    ExternalRefInput {
        provider: "imessage".into(),
        external_id: guid.into(),
        received_at: Some("2026-10-03T10:51:07+07:00".into()),
    }
}

/// Расход из SMS: сумма на «Еду» — заглушка категории до уточнения.
fn from_sms(guid: &str, amount: &str) -> TransactionInput {
    TransactionInput { external_ref: Some(sms(guid)), ..receipt(&[("food", amount)]) }
}

fn conflict_code(e: AppError) -> &'static str {
    match e {
        AppError::ConflictWith(p) => p.code,
        other => panic!("ожидался конфликт с кодом, а пришёл {other:?}"),
    }
}

async fn count_all(svc: &TransactionService, p: Uuid) -> i64 {
    svc.list(p, ListQuery::default(), Pagination { page: 0, size: 50 }).await.unwrap().page.total_elements
}

on_each_repo! {
    /// Т1–Т4: запись создаёт операцию со связью CREATED; повтор (даже с другой суммой) возвращает её же; тот же id у
    /// другого источника и в другом проекте — новые операции.
    async fn external_ref_creates_once(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let (t, created) = svc.create_once(p, u, from_sms("p:0/A", "250")).await.unwrap();
        assert!(created);
        assert_eq!(t.external_refs.len(), 1);
        let r = &t.external_refs[0];
        assert_eq!((r.provider.as_str(), r.external_id.as_str(), r.link.as_str(), r.transaction_id), ("imessage", "p:0/A", "CREATED", t.id));
        assert_eq!(r.received_at.map(|x| x.to_rfc3339()), Some("2026-10-03T03:51:07+00:00".into()));
        assert_eq!(svc.get(p, t.id).await.unwrap().external_refs, t.external_refs, "связь читается из хранилища такой же");

        let (again, created) = svc.create_once(p, u, from_sms(" p:0/A ", "999")).await.unwrap();
        assert!(!created);
        assert_eq!((again.id, again.total), (t.id, d("250")), "тело повтора не сравнивается — прежняя операция");
        assert_eq!(count_all(svc, p).await, 1);

        let other_source = TransactionInput { external_ref: Some(ExternalRefInput { provider: "telegram".into(), ..sms("p:0/A") }), ..receipt(&[("food", "250")]) };
        let (t3, created) = svc.create_once(p, u, other_source).await.unwrap();
        assert!(created && t3.id != t.id, "тот же id у другого источника — другая запись");
        let (t4, created) = svc.create_once(Uuid::new_v4(), u, from_sms("p:0/A", "250")).await.unwrap();
        assert!(created && t4.id != t.id, "проекты не видят связей друг друга");
        assert_eq!(count_all(svc, p).await, 2);
    }
}

on_each_repo! {
    /// Т5, Т6, Т13: удалённую операцию запись не создаёт снова; «Отменить» возвращает её, и повтор снова отдаёт её же;
    /// поиск связи знает, есть ли операция.
    async fn deleted_transaction_is_not_created_again(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let t = svc.create(p, u, from_sms("g1", "100")).await.unwrap();
        assert!(svc.find_ref(p, "IMESSAGE", " g1 ").await.unwrap().transaction_exists);

        let token = svc.delete_with_undo(p, t.id).await.unwrap();
        let found = svc.find_ref(p, "imessage", "g1").await.unwrap();
        assert_eq!((found.external_ref.transaction_id, found.transaction_exists), (t.id, false));
        assert_eq!(conflict_code(svc.create_once(p, u, from_sms("g1", "100")).await.unwrap_err()), "EXTERNAL_REF_DELETED");
        assert_eq!(count_all(svc, p).await, 0);

        let back = svc.undo_delete(p, token).await.unwrap();
        assert_eq!(back.external_refs.len(), 1, "после «Отменить» связь снова указывает на операцию");
        let (again, created) = svc.create_once(p, u, from_sms("g1", "100")).await.unwrap();
        assert_eq!((again.id, created), (t.id, false));

        svc.delete(p, t.id).await.unwrap();
        assert_eq!(conflict_code(svc.create_once(p, u, from_sms("g1", "1")).await.unwrap_err()), "EXTERNAL_REF_DELETED", "DELETE тоже оставляет связь");
        assert!(matches!(svc.find_ref(p, "imessage", "nope").await.unwrap_err(), AppError::NotFound(_)));
        assert_eq!(problem(svc.find_ref(p, "", "g1").await.unwrap_err()).1, "provider");
        assert_eq!(problem(svc.find_ref(p, "imessage", " ").await.unwrap_err()).1, "external_id");
    }
}

on_each_repo! {
    /// Т7 и проверка полей: ошибка операции или записи — ни операции, ни связи; начальный остаток — без связей.
    async fn invalid_request_leaves_no_ref(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let bad = TransactionInput { external_ref: Some(sms("g7")), ..receipt(&[("nope", "10")]) };
        assert_eq!(problem(svc.create_once(p, u, bad).await.unwrap_err()), ("NOT_FOUND", "items[1].category_code".into()));
        assert!(matches!(svc.find_ref(p, "imessage", "g7").await.unwrap_err(), AppError::NotFound(_)), "связь не записана");
        assert_eq!(count_all(svc, p).await, 0);
        assert!(svc.create_once(p, u, from_sms("g7", "10")).await.unwrap().1, "исправленный запрос создаёт операцию");

        let with = |r: ExternalRefInput| TransactionInput { external_ref: Some(r), ..receipt(&[("food", "1")]) };
        for (r, field) in [
            (ExternalRefInput { provider: "bank sms".into(), ..sms("x") }, "external_ref.provider"),
            (ExternalRefInput { external_id: "x".repeat(201), ..sms("x") }, "external_ref.external_id"),
            (ExternalRefInput { received_at: Some("2026-10-03 10:51".into()), ..sms("x") }, "external_ref.received_at"),
            (ExternalRefInput { received_at: Some("2026-10-03T10:51:07".into()), ..sms("x") }, "external_ref.received_at"),
        ] {
            assert_eq!(problem(svc.create_once(p, u, with(r)).await.unwrap_err()), ("VALIDATION", field.into()));
        }
        let opening = TransactionInput { amount: Some(d("100")), external_ref: Some(sms("g8")), ..input("OPENING", "card") };
        assert_eq!(problem(svc.create_once(p, u, opening).await.unwrap_err()), ("VALIDATION", "external_ref".into()));
        assert_eq!(count_all(svc, p).await, 1);
    }
}

on_each_repo! {
    /// Т8–Т10: прикрепление к ручной операции — MATCHED, повтор без новой связи, к другой операции — TAKEN;
    /// к начальному остатку нельзя, чужая операция — 404; у перевода две связи (списание и зачисление).
    async fn attach_to_existing(svc) {
        let (p, u, anna) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let manual = svc.create(p, u, receipt(&[("cafe", "45")])).await.unwrap();
        let (r, created) = svc.attach(p, anna, manual.id, sms("m1")).await.unwrap();
        assert!(created);
        assert_eq!((r.link.as_str(), r.transaction_id), ("MATCHED", manual.id));
        let (again, created) = svc.attach(p, anna, manual.id, sms(" m1 ")).await.unwrap();
        assert!(!created);
        assert_eq!(again, r);
        assert_eq!(svc.get(p, manual.id).await.unwrap().external_refs.len(), 1, "повтор не добавляет связь");
        assert_eq!(svc.get(p, manual.id).await.unwrap().total, d("45"), "операция не меняется");

        let other = svc.create(p, u, receipt(&[("food", "45")])).await.unwrap();
        assert_eq!(conflict_code(svc.attach(p, u, other.id, sms("m1")).await.unwrap_err()), "EXTERNAL_REF_TAKEN");
        let created_from = svc.create(p, u, from_sms("c1", "5")).await.unwrap();
        assert_eq!(conflict_code(svc.attach(p, u, other.id, sms("c1")).await.unwrap_err()), "EXTERNAL_REF_TAKEN", "запись, создавшая другую операцию");
        svc.delete(p, created_from.id).await.unwrap();
        assert_eq!(conflict_code(svc.attach(p, u, other.id, sms("c1")).await.unwrap_err()), "EXTERNAL_REF_TAKEN", "в том числе удалённую");
        // Ручной операции та же запись больше не создаст — POST с ней вернёт подтверждённую операцию.
        let (same, created) = svc.create_once(p, u, from_sms("m1", "45")).await.unwrap();
        assert_eq!((same.id, created), (manual.id, false));

        svc.set_opening(p, u, "card", d("1000"), day(1)).await.unwrap();
        let opening = svc.list(p, ListQuery { tx_kind: Some("OPENING".into()), ..Default::default() }, Pagination { page: 0, size: 5 }).await.unwrap().content.remove(0);
        assert_eq!(problem(svc.attach(p, u, opening.id, sms("o1")).await.unwrap_err()), ("VALIDATION", String::new()));
        assert!(matches!(svc.attach(Uuid::new_v4(), u, manual.id, sms("x1")).await.unwrap_err(), AppError::NotFound(_)), "чужая операция");
        assert_eq!(problem(svc.attach(p, u, manual.id, ExternalRefInput { provider: String::new(), ..sms("x2") }).await.unwrap_err()).1, "provider");

        let tr = svc.create(p, u, TransactionInput { external_ref: Some(sms("out")), ..transfer("card", "cash", "500", None, None) }).await.unwrap();
        svc.attach(p, u, tr.id, sms("in")).await.unwrap();
        let refs = svc.get(p, tr.id).await.unwrap().external_refs;
        assert_eq!(refs.iter().map(|r| (r.external_id.as_str(), r.link.as_str())).collect::<Vec<_>>(), vec![("out", "CREATED"), ("in", "MATCHED")]);
    }
}

on_each_repo! {
    /// Т11, Т12: правка формой и по `id` связей не трогает; фильтр по источнику — только связанные операции.
    async fn refs_survive_edits_and_filter_feed(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let t = svc.create(p, u, from_sms("e1", "120")).await.unwrap();
        let form = TransactionInput { items: vec![ItemInput { id: Some(t.items[0].id), category_code: Some("cafe".into()), amount: Some(d("120")), ..Default::default() }], replace_items: true, ..input("EXPENSE", "card") };
        assert_eq!(svc.update(p, t.id, form).await.unwrap().external_refs, t.external_refs, "правка формой");
        let patch = TransactionInput { items: vec![ItemInput { id: Some(t.items[0].id), name: Some("Латте".into()), ..Default::default() }], ..input("EXPENSE", "card") };
        assert_eq!(svc.update(p, t.id, patch).await.unwrap().external_refs, t.external_refs, "правка по id");
        let tr = TranslationInput { description_ru: Some("Биг Си".into()), description_en: None, description_th: None };
        assert_eq!(svc.translate(p, t.id, tr).await.unwrap().external_refs.len(), 1);

        let manual = svc.create(p, u, receipt(&[("food", "10")])).await.unwrap();
        let matched = svc.create(p, u, receipt(&[("food", "20")])).await.unwrap();
        svc.attach(p, u, matched.id, sms("e2")).await.unwrap();
        let other = svc.create(p, u, TransactionInput { external_ref: Some(ExternalRefInput { provider: "bank_statement".into(), ..sms("e3") }), ..receipt(&[("food", "30")]) }).await.unwrap();
        let q = |provider: &str| ListQuery { external_provider: Some(provider.into()), ..Default::default() };
        let page = svc.list(p, q(" IMESSAGE "), Pagination { page: 0, size: 50 }).await.unwrap();
        let mut ids: Vec<Uuid> = page.content.iter().map(|x| x.id).collect();
        ids.sort();
        let mut want = vec![t.id, matched.id];
        want.sort();
        assert_eq!((ids, page.page.total_elements), (want, 2));
        assert_eq!(svc.list(p, q("bank_statement"), Pagination { page: 0, size: 50 }).await.unwrap().content.iter().map(|x| x.id).collect::<Vec<_>>(), vec![other.id]);
        let feed = svc.feed(p, q("imessage")).await.unwrap();
        assert_eq!((feed.items.len(), feed.totals[0].2), (2, d("140")), "итоги — по отфильтрованным");
        assert!(feed.items.iter().all(|x| !x.external_refs.is_empty()));
        let all = svc.feed(p, ListQuery::default()).await.unwrap();
        assert!(all.items.iter().find(|x| x.id == manual.id).unwrap().external_refs.is_empty());
        assert_eq!(svc.list(p, q(""), Pagination { page: 0, size: 50 }).await.unwrap().page.total_elements, 4, "пустой параметр — без фильтра");
    }
}

on_each_repo! {
    /// Т14: удаление проекта стирает его связи, чужие остаются.
    async fn purge_removes_project_refs(svc) {
        let (mine, other, u) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let t = svc.create(mine, u, from_sms("same", "10")).await.unwrap();
        svc.attach(mine, u, t.id, sms("second")).await.unwrap();
        svc.create(other, u, from_sms("same", "10")).await.unwrap();
        assert_eq!(svc.purge_project(mine).await.unwrap(), 1);
        for guid in ["same", "second"] {
            assert!(matches!(svc.find_ref(mine, "imessage", guid).await.unwrap_err(), AppError::NotFound(_)), "{guid}");
        }
        assert!(svc.find_ref(other, "imessage", "same").await.unwrap().transaction_exists);
        assert!(svc.create_once(mine, u, from_sms("same", "10")).await.unwrap().1, "после удаления проекта запись снова создаёт операцию");
    }
}

on_each_repo! {
    /// Т15: два одинаковых запроса одновременно — одна операция, оба получают её.
    async fn parallel_requests_create_one(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        for round in 0..5 {
            let guid = format!("race-{round}");
            let (a, b) = tokio::join!(svc.create_once(p, u, from_sms(&guid, "77")), svc.create_once(p, u, from_sms(&guid, "77")));
            let (a, b) = (a.unwrap(), b.unwrap());
            assert_eq!(a.0.id, b.0.id, "раунд {round}");
            assert!(a.1 != b.1, "создал один из двух: {} {}", a.1, b.1);
        }
        assert_eq!(count_all(svc, p).await, 5);
    }
}

on_each_repo! {
    /// Т16, Б2: форма не присылает заметки — позиция с тем же `id` её сохраняет, новая позиция — без заметки.
    async fn form_keeps_item_notes(svc) {
        let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
        let t = svc.create(p, u, items(vec![ItemInput { note: Some("из чека: 2 × 60".into()), ..item("food", "120", None) }])).await.unwrap();
        assert_eq!(t.items[0].note.as_deref(), Some("из чека: 2 × 60"));
        let form = TransactionInput {
            items: vec![ItemInput { id: Some(t.items[0].id), ..item("cafe", "125", None) }, item("home", "5", None)],
            replace_items: true,
            ..input("EXPENSE", "card")
        };
        let t = svc.update(p, t.id, form).await.unwrap();
        assert_eq!(t.items.iter().map(|x| (x.category_code.as_str(), x.amount, x.note.as_deref())).collect::<Vec<_>>(), vec![("CAFE", d("125"), Some("из чека: 2 × 60")), ("HOME", d("5"), None)]);
        let patch = TransactionInput { items: vec![ItemInput { id: Some(t.items[0].id), note: Some(String::new()), ..Default::default() }], ..input("EXPENSE", "card") };
        assert_eq!(svc.update(p, t.id, patch).await.unwrap().items[0].note, None, "по id через API заметку можно стереть");
    }
}

/// HTTP (2.2–2.4): 201 → 200 с тем же id → 409 `EXTERNAL_REF_DELETED`; прикрепление 201/200/409/404; поиск 200/404/422.
#[tokio::test]
async fn external_refs_over_http() {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    let scope =
        kernel::auth::Scope { user_id: Uuid::new_v4(), project_id: Uuid::new_v4(), role: kernel::auth::Role::Editor };
    let app = crate::adapters::http::router(Arc::new(service())).layer(axum::Extension(scope));
    let call = |method: &str, uri: String, body: Option<Value>| {
        let app = app.clone();
        let req = Request::builder().method(method).uri(uri).header("content-type", "application/json");
        let req = req.body(body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty)).unwrap();
        async move {
            let resp = app.oneshot(req).await.unwrap();
            let status = resp.status();
            let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null))
        }
    };
    let expense = |guid: &str, amount: &str| {
        json!({
            "occurred_on": "2026-10-03", "tx_kind": "EXPENSE", "account_code": "CARD", "description_lang": "EN",
            "items": [{"category_code": "FOOD", "amount": amount}],
            "external_ref": {"provider": "imessage", "external_id": guid, "received_at": "2026-10-03T10:51:07+07:00"}
        })
    };
    let (status, first) = call("POST", "/".into(), Some(expense("h1", "250"))).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    assert_eq!(first["external_refs"][0]["link"], "CREATED");
    assert_eq!(first["external_refs"][0]["received_at"], "2026-10-03T03:51:07Z");
    let (status, again) = call("POST", "/".into(), Some(expense("h1", "999"))).await;
    assert_eq!((status, &again["id"], &again["total"]), (StatusCode::OK, &first["id"], &first["total"]));
    let id = first["id"].as_str().unwrap().to_string();

    let (status, found) = call("GET", "/external-refs?provider=imessage&external_id=h1".into(), None).await;
    assert_eq!(
        (status, &found["transaction_id"], &found["transaction_exists"]),
        (StatusCode::OK, &first["id"], &json!(true))
    );
    assert_eq!(
        call("GET", "/external-refs?provider=imessage&external_id=none".into(), None).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, e) = call("GET", "/external-refs?provider=imessage".into(), None).await;
    assert_eq!((status, &e["field"]), (StatusCode::UNPROCESSABLE_ENTITY, &json!("external_id")));

    let (status, list) = call("GET", "/?external_provider=imessage".into(), None).await;
    assert_eq!((status, list["content"].as_array().map(Vec::len)), (StatusCode::OK, Some(1)));

    // Прикрепление: к ручной операции 201, повтор 200, к другой 409, чужая операция 404, неверное поле 422.
    let (_, manual) = call("POST", "/".into(), Some(json!({"occurred_on": "2026-10-03", "tx_kind": "EXPENSE", "account_code": "CARD", "description_lang": "EN", "items": [{"category_code": "CAFE", "amount": "45"}]}))).await;
    let manual_id = manual["id"].as_str().unwrap().to_string();
    assert_eq!(manual["external_refs"], json!([]), "поле всегда есть");
    let body = json!({"provider": "imessage", "external_id": "h2"});
    let (status, r) = call("POST", format!("/{manual_id}/external-refs"), Some(body.clone())).await;
    assert_eq!((status, &r["link"]), (StatusCode::CREATED, &json!("MATCHED")));
    assert_eq!(call("POST", format!("/{manual_id}/external-refs"), Some(body.clone())).await.0, StatusCode::OK);
    let (status, e) = call("POST", format!("/{id}/external-refs"), Some(body)).await;
    assert_eq!((status, &e["error"]), (StatusCode::CONFLICT, &json!("EXTERNAL_REF_TAKEN")));
    assert!(e["message"].as_str().is_some_and(|m| !m.is_empty()));
    assert_eq!(
        call(
            "POST",
            format!("/{}/external-refs", Uuid::new_v4()),
            Some(json!({"provider": "imessage", "external_id": "h3"}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (status, e) =
        call("POST", format!("/{manual_id}/external-refs"), Some(json!({"provider": "Bad Name", "external_id": "h3"})))
            .await;
    assert_eq!(
        (status, &e["error"], &e["field"]),
        (StatusCode::UNPROCESSABLE_ENTITY, &json!("VALIDATION"), &json!("provider"))
    );

    // Удалить → повтор с той же записью 409 с кодом, полем и текстом.
    assert_eq!(call("DELETE", format!("/{id}"), None).await.0, StatusCode::NO_CONTENT);
    let (status, e) = call("POST", "/".into(), Some(expense("h1", "250"))).await;
    assert_eq!(
        (status, &e["error"], &e["field"]),
        (StatusCode::CONFLICT, &json!("EXTERNAL_REF_DELETED"), &json!("external_ref"))
    );
    assert_eq!(e["message"], crate::i18n::dict("err.external_ref_deleted").unwrap().0);
    let (_, found) = call("GET", "/external-refs?provider=imessage&external_id=h1".into(), None).await;
    assert_eq!(found["transaction_exists"], json!(false));
}

/// Контракты 2.1–2.5 описаны в OpenAPI (`/api/docs`).
#[test]
fn external_refs_are_in_openapi() {
    use utoipa::OpenApi;
    let doc = serde_json::to_value(crate::adapters::http::ApiDoc::openapi()).unwrap();
    for path in ["/api/transactions/{id}/external-refs", "/api/transactions/external-refs"] {
        assert!(doc["paths"][path].is_object(), "нет пути {path}");
    }
    let schemas = &doc["components"]["schemas"];
    for name in ["ExternalRefInput", "ExternalRefDto", "ExternalRefFoundDto"] {
        assert!(schemas[name].is_object(), "нет схемы {name}");
    }
    assert!(schemas["TransactionInput"]["properties"]["external_ref"].is_object());
    assert!(schemas["TransactionDto"]["properties"]["external_refs"].is_object());
    assert!(doc["paths"]["/api/transactions"]["post"]["responses"]["200"].is_object(), "повтор — 200");
}
