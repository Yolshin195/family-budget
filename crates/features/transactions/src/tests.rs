//! Тесты use cases фичи «Транзакции» на in-memory адаптере: без БД и без других фич.

use std::str::FromStr;
use std::sync::Arc;

use chrono::NaiveDate;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::pagination::Pagination;
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::adapters::memory::InMemoryTransactionRepository;
use crate::dto::{LineDto, ListQuery, TransactionInput, TranslationInput};
use crate::domain::{AccountCurrency, Description};
use crate::ports::{AccountInfo, AccountsLookup, BudgetView, CategoriesLookup, CategoryInfo, CategoryLimit, Deps};
use crate::service::TransactionService;

struct FakeAccounts;
struct FakeCategories;

#[async_trait::async_trait]
impl AccountsLookup for FakeAccounts {
    async fn exists(&self, _p: Uuid, code: &str) -> AppResult<bool> {
        Ok(matches!(code, "CARD" | "WISE" | "CASH" | "TINKOFF"))
    }
    async fn currencies(&self, _p: Uuid) -> AppResult<Vec<AccountCurrency>> {
        let a = |code: &str, currency: &str| AccountCurrency { code: code.into(), currency: currency.into(), minor_units: 2 };
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
            AccountInfo { code: "CARD".into(), title: "Карта".into(), currency: "THB".into(), symbol: "฿".into() },
            AccountInfo { code: "WISE".into(), title: "Wise".into(), currency: "USD".into(), symbol: "$".into() },
            AccountInfo { code: "TINKOFF".into(), title: "Тинькофф".into(), currency: "RUB".into(), symbol: "₽".into() },
        ])
    }
}

#[async_trait::async_trait]
impl CategoriesLookup for FakeCategories {
    async fn kind_of(&self, _p: Uuid, code: &str) -> AppResult<Option<String>> {
        Ok(match code {
            "FOOD" | "CAFE" | "HOME" | "FEES" => Some("EXPENSE".into()),
            "SALARY" => Some("INCOME".into()),
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
            names: names.into(),
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
            CategoryLimit { code: "FOOD".into(), limit: Some(d("10000")), forecast: d("9500"), available: Some(d("500")) },
            CategoryLimit { code: "CAFE".into(), limit: None, forecast: d("1000"), available: Some(d("500")) },
        ])
    }
    async fn rates(&self, _p: Uuid, _base: &str, _date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        Ok(vec![("USD".into(), d("33.25"))])
    }
}

fn service() -> TransactionService {
    TransactionService::new(
        Arc::new(InMemoryTransactionRepository::default()),
        Deps { accounts: Arc::new(FakeAccounts), categories: Arc::new(FakeCategories), budget: Arc::new(FakeBudget) },
    )
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
    }
}

fn receipt(lines: &[(&str, &str)]) -> TransactionInput {
    TransactionInput { lines: lines.iter().map(|(c, a)| LineDto { category_code: c.to_string(), amount: d(a), note: None }).collect(), ..input("EXPENSE", "card") }
}

fn transfer(from: &str, to: &str, amount: &str, to_amount: Option<&str>, fee: Option<(&str, &str)>) -> TransactionInput {
    TransactionInput {
        to_account_code: Some(to.into()),
        amount: Some(d(amount)),
        to_amount: to_amount.map(d),
        fee_category_code: fee.map(|f| f.0.into()),
        fee_amount: fee.map(|f| d(f.1)),
        ..input("TRANSFER", from)
    }
}

async fn balance(svc: &TransactionService, p: Uuid, account: &str) -> Decimal {
    svc.balance(p, account, day(31)).await.unwrap()
}

#[tokio::test]
async fn receipt_with_several_lines() {
    let svc = service();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let t = svc.create(p, u, receipt(&[("food", "900"), ("home", "250"), ("cafe", "100")])).await.unwrap();
    assert_eq!(t.total, d("1250"));
    assert_eq!((t.lines.len(), t.postings.len(), t.currency.as_str()), (3, 4, "THB"));
    assert_eq!(t.created_by, u);
    assert_eq!(balance(&svc, p, "CARD").await, d("-1250"));
    let facts = svc.lines(p, day(1), day(31)).await.unwrap();
    assert_eq!(facts.iter().map(|f| f.amount).sum::<Decimal>(), d("1250"));
}

#[tokio::test]
async fn category_kind_must_match() {
    let svc = service();
    let err = svc.create(Uuid::new_v4(), Uuid::new_v4(), receipt(&[("food", "10"), ("salary", "5")])).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(m) if m.contains("lines[2]")));
    let err = svc.create(Uuid::new_v4(), Uuid::new_v4(), transfer("card", "cash", "10", None, Some(("salary", "1")))).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(m) if m.contains("fee_category_code")), "комиссия — расход");
    let err = svc.create(Uuid::new_v4(), Uuid::new_v4(), receipt(&[("nope", "10")])).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
async fn unknown_account_is_validation_error() {
    let svc = service();
    let mut input = receipt(&[("food", "10")]);
    input.account_code = "NOPE".into();
    assert!(matches!(svc.create(Uuid::new_v4(), Uuid::new_v4(), input).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn plan_occurrence_is_paid_once_and_freed_on_delete() {
    let svc = service();
    let (p, u, plan) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
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

#[tokio::test]
async fn translation_filter() {
    let svc = service();
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

#[tokio::test]
async fn projects_are_isolated() {
    let svc = service();
    let t = svc.create(Uuid::new_v4(), Uuid::new_v4(), receipt(&[("food", "10")])).await.unwrap();
    assert!(matches!(svc.get(Uuid::new_v4(), t.id).await.unwrap_err(), AppError::NotFound(_)));
}

/// Быстрый расход: последний счёт пользователя, частые категории за 30 дней (дополняются корнями
/// дерева), остатки лимитов и курсы — из транзакций, без новых таблиц.
#[tokio::test]
async fn quick_defaults_follow_user_habits() {
    let svc = service();
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
    assert_eq!(q.accounts.iter().find(|a| a.code == "WISE").unwrap().rate_to_base, d("33.25"));
    // через 30 дней привычки «остывают»
    let later = svc.quick_defaults(p, me, "THB", Lang::Ru, day(3) + chrono::Days::new(40)).await.unwrap();
    assert_eq!(later.frequent_expense, vec!["FOOD", "HOME"]);
}

/// ТЗ, раздел 13: чужой проект → 404; удаление отменяется с тем же id; проект очищается целиком.
#[tokio::test]
async fn foreign_project_undo_and_purge() {
    let svc = service();
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
    assert_eq!((back.id, back.total, back.lines.len()), (t.id, d("1000"), 2));

    svc.create(other, me, receipt(&[("FOOD", "1")])).await.unwrap();
    assert_eq!(svc.purge_project(mine).await.unwrap(), 1);
    assert!(svc.lines(mine, day(1), day(31)).await.unwrap().is_empty());
    assert_eq!(svc.lines(other, day(1), day(31)).await.unwrap().len(), 1);
}

/// ТЗ, приёмка: обмен 10 000 ₽ → 3 912,40 ฿ с комиссией 50 ₽ — одна транзакция: остатки обоих
/// счетов верны, в факты бюджета попадает только комиссия; удаление и «Отменить» возвращают остатки.
#[tokio::test]
async fn exchange_is_one_transaction() {
    let svc = service();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    svc.set_opening(p, u, "tinkoff", d("20000"), day(1)).await.unwrap();
    svc.set_opening(p, u, "card", d("1000"), day(1)).await.unwrap();
    let t = svc.create(p, u, transfer("tinkoff", "card", "10050", Some("3912.40"), Some(("fees", "50")))).await.unwrap();
    assert_eq!((t.tx_kind.as_str(), t.total, t.sent(), t.to_amount), ("TRANSFER", d("10050"), d("10000"), Some(d("3912.40"))));
    assert_eq!((t.currency.as_str(), t.to_currency.as_deref()), ("RUB", Some("THB")));
    assert_eq!(t.postings.len(), 5);
    assert_eq!((balance(&svc, p, "TINKOFF").await, balance(&svc, p, "CARD").await), (d("9950"), d("4912.40")));

    let facts = svc.lines(p, day(1), day(31)).await.unwrap();
    assert_eq!(facts.len(), 1, "доходы и расходы не меняются — только комиссия");
    assert_eq!((facts[0].kind.as_str(), facts[0].category_code.as_str(), facts[0].amount, facts[0].currency.as_str()), ("EXPENSE", "FEES", d("50"), "RUB"));
    let fx = svc.exchange(p, day(1), day(31)).await.unwrap();
    assert_eq!(fx.iter().map(|x| (x.currency.as_str(), x.amount)).collect::<Vec<_>>(), vec![("RUB", d("10000")), ("THB", d("-3912.40"))]);

    let token = svc.delete_with_undo(p, t.id).await.unwrap();
    assert_eq!((balance(&svc, p, "TINKOFF").await, balance(&svc, p, "CARD").await), (d("20000"), d("1000")));
    svc.undo_delete(p, token).await.unwrap();
    assert_eq!((balance(&svc, p, "TINKOFF").await, balance(&svc, p, "CARD").await), (d("9950"), d("4912.40")));
}

#[tokio::test]
async fn transfer_rules_and_type_change() {
    let svc = service();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let cash = svc.create(p, u, transfer("card", "cash", "5000", None, None)).await.unwrap();
    assert_eq!((balance(&svc, p, "CARD").await, balance(&svc, p, "CASH").await), (d("-5000"), d("5000")));
    assert!(svc.create(p, u, transfer("card", "card", "1", None, None)).await.is_err(), "тот же счёт");
    assert!(svc.create(p, u, transfer("tinkoff", "card", "100", None, None)).await.is_err(), "при обмене нужна сумма зачисления");
    assert!(svc.create(p, u, transfer("card", "nope", "100", None, None)).await.is_err());
    // Тип можно сменить: перевод → расход, проводки пересобираются.
    let t = svc.update(p, cash.id, receipt(&[("food", "300")])).await.unwrap();
    assert_eq!((t.tx_kind.as_str(), t.to_account_code), ("EXPENSE", None));
    assert_eq!((balance(&svc, p, "CARD").await, balance(&svc, p, "CASH").await), (d("-300"), d("0")));
}

/// Возврат — расход с обратными знаками: пополняет счёт и уменьшает факт категории.
#[tokio::test]
async fn refund_reduces_fact() {
    let svc = service();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    svc.create(p, u, receipt(&[("food", "1000")])).await.unwrap();
    let r = svc.create(p, u, TransactionInput { refund: true, ..receipt(&[("food", "300")]) }).await.unwrap();
    assert!(r.refund);
    assert_eq!(balance(&svc, p, "CARD").await, d("-700"));
    let facts = svc.lines(p, day(1), day(31)).await.unwrap();
    assert_eq!(facts.iter().map(|f| f.amount).sum::<Decimal>(), d("700"));
    let salary = svc.create(p, u, TransactionInput { lines: vec![LineDto { category_code: "salary".into(), amount: d("5000"), note: None }], ..input("INCOME", "card") }).await.unwrap();
    assert_eq!(salary.total, d("5000"));
    let facts = svc.lines(p, day(1), day(31)).await.unwrap();
    assert_eq!(facts.iter().filter(|f| f.kind == "INCOME").map(|f| f.amount).sum::<Decimal>(), d("5000"), "доход в фактах — с плюсом");
}

/// Начальный остаток ведётся из формы счёта: один на счёт, нулевой — удаляется.
#[tokio::test]
async fn opening_balance_from_account_form() {
    let svc = service();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    svc.set_opening(p, u, "card", d("12000"), day(1)).await.unwrap();
    assert_eq!(svc.opening(p, "card").await.unwrap(), Some((d("12000"), day(1))));
    svc.set_opening(p, u, "CARD", d("-150"), day(2)).await.unwrap();
    assert_eq!(svc.opening(p, "CARD").await.unwrap(), Some((d("-150"), day(2))));
    assert_eq!(balance(&svc, p, "CARD").await, d("-150"));
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

/// «Сверить остаток»: разница вносится расходом или доходом на выбранную категорию.
#[tokio::test]
async fn reconcile_balance() {
    let svc = service();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    let desc = || Description { text: Some("Сверка".into()), lang: Lang::Ru, ..Default::default() };
    svc.set_opening(p, u, "card", d("1000"), day(1)).await.unwrap();
    assert!(svc.reconcile(p, u, "card", d("1000"), day(5), "food", desc()).await.unwrap().is_none(), "совпало — вносить нечего");
    let t = svc.reconcile(p, u, "card", d("940"), day(5), "food", desc()).await.unwrap().unwrap();
    assert_eq!((t.tx_kind.as_str(), t.total), ("EXPENSE", d("60")));
    let err = svc.reconcile(p, u, "card", d("1000"), day(5), "food", desc()).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(m) if m.contains("дохода")), "остаток больше — нужна категория дохода");
    svc.reconcile(p, u, "card", d("1000"), day(5), "salary", desc()).await.unwrap().unwrap();
    assert_eq!(balance(&svc, p, "CARD").await, d("1000"));
}

/// Фильтр по счёту — выписка: переводы видны на обоих счетах; фильтр по типу.
#[tokio::test]
async fn statement_and_kind_filters() {
    let svc = service();
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

/// Ссылка на счёт, ушедший в архив, остаётся валидной при правке транзакции.
#[tokio::test]
async fn archived_refs_stay_valid_on_edit() {
    let svc = service();
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    assert!(svc.create(p, u, receipt(&[("food", "1")]).with_account("old")).await.is_err(), "на архивный счёт не сослаться заново");
    let t = svc.create(p, u, receipt(&[("food", "1")])).await.unwrap();
    // правка описания и суммы без смены ссылок проходит
    let mut again = receipt(&[("food", "2")]);
    again.description = Some("Big C, 2".into());
    assert_eq!(svc.update(p, t.id, again).await.unwrap().total, d("2"));
}

impl TransactionInput {
    fn with_account(self, account: &str) -> Self {
        Self { account_code: account.into(), ..self }
    }
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing = kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
    // Ключи, которые код собирает не через `.t("…")`.
    let dynamic = [
        "balance_start", "balance_end", "inflow", "outflow", "income_total", "expense_total", "net", "transfer", "opening", "refund", "rate",
        "fee_short", "kinds.EXPENSE", "kinds.INCOME", "kinds.TRANSFER", "kinds.OPENING", "sys.EXCHANGE", "sys.OPENING", "reconcile_done",
        "reconcile_same", "reconcile_desc", "nav_accounts",
    ];
    let absent: Vec<&str> = dynamic.into_iter().filter(|k| crate::i18n::dict(k).is_none()).collect();
    assert!(absent.is_empty(), "нет перевода для ключей: {absent:?}");
}

/// Ц7: несуществующий план и дата вне правила не принимаются.
#[tokio::test]
#[ignore = "Ц7: исправление ждёт утверждения разбора (docs/stage-0.md)"]
async fn unknown_plan_occurrence_is_rejected() {
    let svc = service();
    let mut input = receipt(&[("home", "1539.87")]);
    input.plan_id = Some(Uuid::new_v4());
    input.plan_date = Some(day(17));
    assert!(svc.create(Uuid::new_v4(), Uuid::new_v4(), input).await.is_err(), "оплачено вхождение несуществующего плана");
}

/// Ц10: двойной клик «Сохранить» в форме счёта — два начальных остатка. Нужен `TEST_DATABASE_URL`.
#[tokio::test]
#[ignore = "Ц10: исправление ждёт утверждения разбора (docs/stage-0.md)"]
async fn second_opening_is_rejected_by_db() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL не задан — тест пропущен");
        return;
    };
    let schema = format!("tx_c10_{}", Uuid::new_v4().simple());
    let admin = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::raw_sql(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
    let opts: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    let pool = sqlx::PgPool::connect_with(opts.options([("search_path", schema.as_str())])).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    use crate::domain::{Body, Transaction, TransactionData};
    use crate::ports::TransactionRepository;
    let repo = crate::adapters::postgres::PostgresTransactionRepository::new(pool.clone());
    let accounts = [AccountCurrency { code: "CARD".into(), currency: "THB".into(), minor_units: 2 }];
    let (p, u) = (Uuid::new_v4(), Uuid::new_v4());
    // Оба запроса прошли проверку «остатка ещё нет» до того, как другой записал свой.
    let opening = || {
        let data = TransactionData { occurred_on: day(15), description: Description::default(), plan: None, body: Body::Opening { account: "card".into(), amount: d("1961.31") } };
        Transaction::create(p, u, data, &accounts).unwrap()
    };
    repo.insert(&opening()).await.unwrap();
    let second = repo.insert(&opening()).await;
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM fb_transactions WHERE tx_kind = 'OPENING'").fetch_one(&pool).await.unwrap();
    pool.close().await;
    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&admin).await.unwrap();
    assert!(matches!(second, Err(AppError::Conflict(_))), "второй начальный остаток принят: {second:?}");
    assert_eq!(n, 1, "два начальных остатка у одного счёта — остаток удвоен");
}
