//! Доменная модель «Транзакции»: двойная запись. Транзакция — шапка + проводки; сумма проводок по
//! каждой валюте равна нулю (ТЗ, раздел 4). Никаких sqlx/axum/serde.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use kernel::code::normalize;
use kernel::i18n::{Lang, Localized};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Тип транзакции. Задаётся шаблоном формы, по нему сервер строит проводки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxKind {
    Expense,
    Income,
    Transfer,
    Opening,
}

impl TxKind {
    pub const VALUES: &'static [&'static str] = &["EXPENSE", "INCOME", "TRANSFER", "OPENING"];

    pub fn as_str(&self) -> &'static str {
        match self {
            TxKind::Expense => "EXPENSE",
            TxKind::Income => "INCOME",
            TxKind::Transfer => "TRANSFER",
            TxKind::Opening => "OPENING",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "EXPENSE" => Some(TxKind::Expense),
            "INCOME" => Some(TxKind::Income),
            "TRANSFER" => Some(TxKind::Transfer),
            "OPENING" => Some(TxKind::Opening),
            _ => None,
        }
    }
}

/// Куда относится проводка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Account,
    Category,
    System,
}

impl RefKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RefKind::Account => "ACCOUNT",
            RefKind::Category => "CATEGORY",
            RefKind::System => "SYSTEM",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ACCOUNT" => Some(RefKind::Account),
            "CATEGORY" => Some(RefKind::Category),
            "SYSTEM" => Some(RefKind::System),
            _ => None,
        }
    }
}

/// Системные счета: служебная сторона проводок, вне справочников.
pub mod system {
    /// Сводит две валюты при обмене; его проводки за период в базовой валюте — курсовая разница.
    pub const EXCHANGE: &str = "EXCHANGE";
    /// Источник начальных остатков счетов.
    pub const OPENING: &str = "OPENING";
}

/// Проводка: где и на сколько изменились деньги. «+» — счёт пополнился или деньги ушли на категорию
/// расхода; «−» — счёт уменьшился или деньги пришли из категории дохода.
#[derive(Debug, Clone, PartialEq)]
pub struct Posting {
    pub ref_kind: RefKind,
    pub ref_code: String,
    pub currency: String,
    pub amount: Decimal,
    pub note: Option<String>,
}

impl Posting {
    fn new(ref_kind: RefKind, ref_code: &str, currency: &str, amount: Decimal, note: Option<String>) -> Self {
        Self { ref_kind, ref_code: ref_code.to_string(), currency: currency.to_string(), amount, note }
    }

    pub fn is_account(&self, code: &str) -> bool {
        self.ref_kind == RefKind::Account && self.ref_code == code
    }
}

/// Позиция чека в форме расхода или дохода: категория и сумма > 0.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub category_code: String,
    pub amount: Decimal,
    pub note: Option<String>,
}

/// Комиссия перевода: расход на категорию, списывается со счёта списания.
#[derive(Debug, Clone, PartialEq)]
pub struct Fee {
    pub category_code: String,
    pub amount: Decimal,
}

/// Поля формы — то, что ввёл пользователь. Из них сервер строит проводки.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// Расход или доход: счёт и позиции. `refund` — возврат: те же проводки с обратными знаками.
    Flow { income: bool, account: String, lines: Vec<Line>, refund: bool },
    /// Перевод между счетами. `amount` — сколько всего списано со счёта (вместе с комиссией);
    /// `to_amount` — сколько зачислено (задаётся только при разных валютах).
    Transfer { from: String, to: String, amount: Decimal, to_amount: Option<Decimal>, fee: Option<Fee> },
    /// Начальный остаток счёта (может быть отрицательным).
    Opening { account: String, amount: Decimal },
}

impl Body {
    pub fn kind(&self) -> TxKind {
        match self {
            Body::Flow { income: false, .. } => TxKind::Expense,
            Body::Flow { income: true, .. } => TxKind::Income,
            Body::Transfer { .. } => TxKind::Transfer,
            Body::Opening { .. } => TxKind::Opening,
        }
    }
}

/// Валюта счёта — чтобы построить проводки и проверить точность сумм.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountCurrency {
    pub code: String,
    pub currency: String,
    /// Знаков после запятой у валюты.
    pub minor_units: u32,
}

/// Описание: оригинал + язык + переводы.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Description {
    pub text: Option<String>,
    pub lang: Lang,
    pub ru: Option<String>,
    pub en: Option<String>,
    pub th: Option<String>,
}

impl Description {
    pub fn localized(&self) -> Option<Localized<'_>> {
        self.text.as_deref().map(|original| Localized {
            original,
            lang: self.lang,
            ru: self.ru.as_deref(),
            en: self.en.as_deref(),
            th: self.th.as_deref(),
        })
    }

    fn cleaned(self) -> Self {
        let lang = self.lang;
        // Перевод на язык оригинала не храним — он и есть оригинал.
        let tr = |l: Lang, v: Option<String>| if l == lang { None } else { kernel::i18n::clean(v) };
        Self {
            text: kernel::i18n::clean(self.text),
            lang,
            ru: tr(Lang::Ru, self.ru),
            en: tr(Lang::En, self.en),
            th: tr(Lang::Th, self.th),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub id: Uuid,
    pub project_id: Uuid,
    pub occurred_on: NaiveDate,
    pub tx_kind: TxKind,
    pub description: Description,
    pub created_by: Uuid,
    /// Вхождение плана, которое оплачено этой транзакцией.
    pub plan: Option<(Uuid, NaiveDate)>,
    pub postings: Vec<Posting>,
}

/// Данные для создания и изменения.
#[derive(Debug, Clone)]
pub struct TransactionData {
    pub occurred_on: NaiveDate,
    pub description: Description,
    pub plan: Option<(Uuid, NaiveDate)>,
    pub body: Body,
}

/// Позиций в одной транзакции — не больше (защита от мусорных запросов).
pub const MAX_LINES: usize = 100;
/// Денежные колонки — NUMERIC(18,4).
const MAX_SCALE: u32 = 4;

fn check_amount(field: &str, amount: Decimal, cur: &AccountCurrency) -> Result<(), String> {
    if amount.normalize().scale() > cur.minor_units.min(MAX_SCALE) {
        return Err(format!("{field}: у {} не больше {} знаков после запятой", cur.currency, cur.minor_units.min(MAX_SCALE)));
    }
    Ok(())
}

fn positive(field: &str, amount: Decimal, cur: &AccountCurrency) -> Result<(), String> {
    if amount <= Decimal::ZERO {
        return Err(format!("{field}: сумма должна быть больше нуля"));
    }
    check_amount(field, amount, cur)
}

fn account<'a>(field: &str, code: &str, accounts: &'a [AccountCurrency]) -> Result<(String, &'a AccountCurrency), String> {
    let code = normalize(field, code)?;
    let cur = accounts.iter().find(|a| a.code == code).ok_or_else(|| format!("{field}: счёт '{code}' не найден"))?;
    Ok((code, cur))
}

/// Нормализует поля формы и строит проводки (ТЗ, раздел 4, таблица типов).
fn build(body: Body, accounts: &[AccountCurrency]) -> Result<(Body, Vec<Posting>), String> {
    match body {
        Body::Flow { income, account: acc, lines, refund } => {
            let (acc, cur) = account("account_code", &acc, accounts)?;
            if lines.is_empty() {
                return Err("lines: нужна хотя бы одна позиция".into());
            }
            if lines.len() > MAX_LINES {
                return Err(format!("lines: не больше {MAX_LINES} позиций"));
            }
            let mut clean = Vec::with_capacity(lines.len());
            for (i, l) in lines.into_iter().enumerate() {
                positive(&format!("lines[{}].amount", i + 1), l.amount, cur)?;
                clean.push(Line {
                    category_code: normalize(&format!("lines[{}].category_code", i + 1), &l.category_code)?,
                    amount: l.amount,
                    note: kernel::i18n::clean(l.note),
                });
            }
            // Расход: счёт −, категории +. Доход — наоборот. Возврат меняет знаки.
            let sign = if income != refund { -Decimal::ONE } else { Decimal::ONE };
            let total: Decimal = clean.iter().map(|l| l.amount).sum();
            let mut postings = vec![Posting::new(RefKind::Account, &acc, &cur.currency, -sign * total, None)];
            postings.extend(clean.iter().map(|l| Posting::new(RefKind::Category, &l.category_code, &cur.currency, sign * l.amount, l.note.clone())));
            Ok((Body::Flow { income, account: acc, lines: clean, refund }, postings))
        }
        Body::Transfer { from, to, amount, to_amount, fee } => {
            let (from, fc) = account("account_code", &from, accounts)?;
            let (to, tc) = account("to_account_code", &to, accounts)?;
            if from == to {
                return Err("to_account_code: счёт зачисления должен отличаться от счёта списания".into());
            }
            positive("amount", amount, fc)?;
            let fee = match fee {
                None => None,
                Some(f) => {
                    positive("fee_amount", f.amount, fc)?;
                    if f.amount >= amount {
                        return Err("fee_amount: комиссия должна быть меньше суммы списания".into());
                    }
                    Some(Fee { category_code: normalize("fee_category_code", &f.category_code)?, amount: f.amount })
                }
            };
            let sent = amount - fee.as_ref().map(|f| f.amount).unwrap_or_default();
            let mut postings = vec![Posting::new(RefKind::Account, &from, &fc.currency, -amount, None)];
            if let Some(f) = &fee {
                postings.push(Posting::new(RefKind::Category, &f.category_code, &fc.currency, f.amount, None));
            }
            let to_amount = if fc.currency == tc.currency {
                if to_amount.is_some_and(|t| t != sent) {
                    return Err("to_amount: при одной валюте зачисляется списанное за вычетом комиссии".into());
                }
                postings.push(Posting::new(RefKind::Account, &to, &tc.currency, sent, None));
                None
            } else {
                let got = to_amount.ok_or("to_amount: при разных валютах укажите, сколько зачислено")?;
                positive("to_amount", got, tc)?;
                postings.push(Posting::new(RefKind::System, system::EXCHANGE, &fc.currency, sent, None));
                postings.push(Posting::new(RefKind::System, system::EXCHANGE, &tc.currency, -got, None));
                postings.push(Posting::new(RefKind::Account, &to, &tc.currency, got, None));
                Some(got)
            };
            Ok((Body::Transfer { from, to, amount, to_amount, fee }, postings))
        }
        Body::Opening { account: acc, amount } => {
            let (acc, cur) = account("account_code", &acc, accounts)?;
            if amount.is_zero() {
                return Err("amount: начальный остаток не может быть нулевым".into());
            }
            check_amount("amount", amount, cur)?;
            let postings = vec![
                Posting::new(RefKind::Account, &acc, &cur.currency, amount, None),
                Posting::new(RefKind::System, system::OPENING, &cur.currency, -amount, None),
            ];
            Ok((Body::Opening { account: acc, amount }, postings))
        }
    }
}

/// Главное правило двойной записи: не меньше двух проводок, без нулевых сумм, сумма по каждой
/// валюте равна нулю.
pub fn check_balanced(postings: &[Posting]) -> Result<(), String> {
    if postings.len() < 2 {
        return Err("postings: нужно не меньше двух проводок".into());
    }
    if postings.iter().any(|p| p.amount.is_zero()) {
        return Err("postings: сумма проводки не может быть нулевой".into());
    }
    let mut sums: BTreeMap<&str, Decimal> = BTreeMap::new();
    for p in postings {
        *sums.entry(p.currency.as_str()).or_default() += p.amount;
    }
    match sums.into_iter().find(|(_, s)| !s.is_zero()) {
        Some((cur, s)) => Err(format!("postings: сумма проводок в {cur} равна {s}, а должна быть 0")),
        None => Ok(()),
    }
}

impl Transaction {
    pub fn create(project_id: Uuid, created_by: Uuid, data: TransactionData, accounts: &[AccountCurrency]) -> Result<Self, String> {
        let mut t = Self {
            id: Uuid::new_v4(),
            project_id,
            occurred_on: data.occurred_on,
            tx_kind: data.body.kind(),
            description: Description::default(),
            created_by,
            plan: None,
            postings: Vec::new(),
        };
        t.change(data, accounts)?;
        Ok(t)
    }

    /// Полная замена данных: набор проводок пересобирается целиком, тип можно сменить.
    pub fn change(&mut self, data: TransactionData, accounts: &[AccountCurrency]) -> Result<(), String> {
        let kind = data.body.kind();
        if kind == TxKind::Opening && data.plan.is_some() {
            return Err("plan_id: начальный остаток не оплачивает план".into());
        }
        let (_, postings) = build(data.body, accounts)?;
        check_balanced(&postings)?;
        self.occurred_on = data.occurred_on;
        self.tx_kind = kind;
        self.description = data.description.cleaned();
        self.plan = data.plan;
        self.postings = postings;
        Ok(())
    }

    /// Поля формы, восстановленные из проводок (для редактирования и API).
    pub fn body(&self) -> Body {
        let accounts: Vec<&Posting> = self.postings.iter().filter(|p| p.ref_kind == RefKind::Account).collect();
        let cats = self.postings.iter().filter(|p| p.ref_kind == RefKind::Category);
        let first = accounts.first().map(|p| (p.ref_code.clone(), p.amount)).unwrap_or_default();
        match self.tx_kind {
            TxKind::Expense | TxKind::Income => {
                let income = self.tx_kind == TxKind::Income;
                // Обычный расход уменьшает счёт, обычный доход — пополняет.
                let refund = (first.1 > Decimal::ZERO) != income;
                let lines = cats.map(|p| Line { category_code: p.ref_code.clone(), amount: p.amount.abs(), note: p.note.clone() }).collect();
                Body::Flow { income, account: first.0, lines, refund }
            }
            TxKind::Transfer => {
                let from = accounts.iter().find(|p| p.amount < Decimal::ZERO);
                let to = accounts.iter().find(|p| p.amount > Decimal::ZERO);
                let exchange = self.postings.iter().any(|p| p.ref_kind == RefKind::System);
                Body::Transfer {
                    from: from.map(|p| p.ref_code.clone()).unwrap_or_default(),
                    to: to.map(|p| p.ref_code.clone()).unwrap_or_default(),
                    amount: from.map(|p| -p.amount).unwrap_or_default(),
                    to_amount: if exchange { to.map(|p| p.amount) } else { None },
                    fee: cats.map(|p| Fee { category_code: p.ref_code.clone(), amount: p.amount }).next(),
                }
            }
            TxKind::Opening => Body::Opening { account: first.0, amount: first.1 },
        }
    }

    /// Сумма транзакции: у расхода и дохода — сумма позиций, у перевода — сколько списано,
    /// у начального остатка — сам остаток.
    pub fn total(&self) -> Decimal {
        match self.body() {
            Body::Flow { lines, .. } => lines.iter().map(|l| l.amount).sum(),
            Body::Transfer { amount, .. } | Body::Opening { amount, .. } => amount,
        }
    }

    /// Валюта суммы `total`: валюта счёта операции, у перевода — счёта списания.
    pub fn currency(&self) -> &str {
        let mut accounts = self.postings.iter().filter(|p| p.ref_kind == RefKind::Account);
        let main = if self.tx_kind == TxKind::Transfer { accounts.find(|p| p.amount < Decimal::ZERO) } else { accounts.next() };
        main.map_or("", |p| p.currency.as_str())
    }

    /// Как транзакция изменила остаток счёта.
    pub fn amount_on(&self, account: &str) -> Decimal {
        self.postings.iter().filter(|p| p.is_account(account)).map(|p| p.amount).sum()
    }

    /// Счёт начального остатка (только у `OPENING`) — по нему БД держит «один остаток на счёт».
    pub fn opening_account(&self) -> Option<&str> {
        if self.tx_kind != TxKind::Opening {
            return None;
        }
        self.accounts().next()
    }

    /// Счета, которых касается транзакция.
    pub fn accounts(&self) -> impl Iterator<Item = &str> {
        self.postings.iter().filter(|p| p.ref_kind == RefKind::Account).map(|p| p.ref_code.as_str())
    }

    /// Изменить только переводы описания.
    pub fn translate(&mut self, ru: Option<String>, en: Option<String>, th: Option<String>) {
        let d = Description { text: self.description.text.clone(), lang: self.description.lang, ru, en, th };
        self.description = d.cleaned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn accounts() -> Vec<AccountCurrency> {
        vec![
            AccountCurrency { code: "CARD".into(), currency: "THB".into(), minor_units: 2 },
            AccountCurrency { code: "CASH".into(), currency: "THB".into(), minor_units: 2 },
            AccountCurrency { code: "TINKOFF".into(), currency: "RUB".into(), minor_units: 2 },
        ]
    }

    fn data(body: Body) -> TransactionData {
        TransactionData {
            occurred_on: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            description: Description { text: Some(" Чек ".into()), lang: Lang::Ru, ru: Some("Чек".into()), en: Some("Receipt".into()), th: Some(" ".into()) },
            plan: None,
            body,
        }
    }

    fn flow(income: bool, refund: bool, account: &str, lines: Vec<(&str, &str)>) -> Body {
        Body::Flow {
            income,
            account: account.into(),
            lines: lines.into_iter().map(|(c, a)| Line { category_code: c.into(), amount: dec(a), note: None }).collect(),
            refund,
        }
    }

    fn receipt(lines: Vec<(&str, &str)>) -> Body {
        flow(false, false, "card", lines)
    }

    fn create(body: Body) -> Result<Transaction, String> {
        Transaction::create(Uuid::nil(), Uuid::nil(), data(body), &accounts())
    }

    fn amounts(t: &Transaction) -> Vec<(&str, &str, Decimal)> {
        t.postings.iter().map(|p| (p.ref_code.as_str(), p.currency.as_str(), p.amount)).collect()
    }

    #[test]
    fn receipt_is_balanced_and_description_is_cleaned() {
        let t = create(receipt(vec![("food", "900"), ("home", "250"), ("cafe", "100")])).unwrap();
        assert_eq!(t.total(), dec("1250"));
        assert_eq!(amounts(&t), vec![("CARD", "THB", dec("-1250")), ("FOOD", "THB", dec("900")), ("HOME", "THB", dec("250")), ("CAFE", "THB", dec("100"))]);
        assert_eq!(t.description.text.as_deref(), Some("Чек"));
        assert_eq!(t.description.ru, None, "перевод на язык оригинала не хранится");
        assert_eq!(t.description.th, None);
        assert_eq!(t.body(), receipt(vec![("FOOD", "900"), ("HOME", "250"), ("CAFE", "100")]).normalized());
    }

    #[test]
    fn refund_and_income_flip_signs() {
        let refund = create(flow(false, true, "card", vec![("food", "300")])).unwrap();
        assert_eq!(amounts(&refund), vec![("CARD", "THB", dec("300")), ("FOOD", "THB", dec("-300"))]);
        assert!(matches!(refund.body(), Body::Flow { refund: true, income: false, .. }));
        let salary = create(flow(true, false, "card", vec![("salary", "100000")])).unwrap();
        assert_eq!(amounts(&salary), vec![("CARD", "THB", dec("100000")), ("SALARY", "THB", dec("-100000"))]);
        assert_eq!((salary.tx_kind, salary.total()), (TxKind::Income, dec("100000")));
    }

    #[test]
    fn invalid_lines() {
        assert!(create(receipt(vec![])).is_err());
        assert!(create(receipt(vec![("food", "0")])).is_err());
        assert!(create(receipt(vec![("food", "-5")])).is_err());
        assert!(create(receipt(vec![("food", "1.001")])).is_err(), "у THB два знака после запятой");
        assert!(create(flow(false, false, "nope", vec![("food", "1")])).is_err());
    }

    /// ТЗ, раздел 4: обмен 10 000 ₽ → 3 912,40 ฿ с комиссией 50 ₽ — одна сбалансированная транзакция.
    #[test]
    fn exchange_with_fee() {
        let body = Body::Transfer {
            from: "tinkoff".into(),
            to: "card".into(),
            amount: dec("10050"),
            to_amount: Some(dec("3912.40")),
            fee: Some(Fee { category_code: "fees".into(), amount: dec("50") }),
        };
        let t = create(body).unwrap();
        assert_eq!(
            amounts(&t),
            vec![
                ("TINKOFF", "RUB", dec("-10050")),
                ("FEES", "RUB", dec("50")),
                ("EXCHANGE", "RUB", dec("10000")),
                ("EXCHANGE", "THB", dec("-3912.40")),
                ("CARD", "THB", dec("3912.40")),
            ]
        );
        assert_eq!((t.amount_on("TINKOFF"), t.amount_on("CARD"), t.total()), (dec("-10050"), dec("3912.40"), dec("10050")));
        assert!(matches!(t.body(), Body::Transfer { to_amount: Some(_), fee: Some(_), .. }));
    }

    #[test]
    fn transfer_in_one_currency() {
        let body = Body::Transfer { from: "card".into(), to: "cash".into(), amount: dec("5000"), to_amount: None, fee: None };
        let t = create(body.clone()).unwrap();
        assert_eq!(amounts(&t), vec![("CARD", "THB", dec("-5000")), ("CASH", "THB", dec("5000"))]);
        assert_eq!(t.body(), body.normalized());
        let same = Body::Transfer { from: "card".into(), to: "card".into(), amount: dec("1"), to_amount: None, fee: None };
        assert!(create(same).is_err(), "перевод на тот же счёт");
        let wrong = Body::Transfer { from: "card".into(), to: "cash".into(), amount: dec("10"), to_amount: Some(dec("9")), fee: None };
        assert!(create(wrong).is_err(), "в одной валюте зачисляется столько же");
        let no_to = Body::Transfer { from: "tinkoff".into(), to: "card".into(), amount: dec("10"), to_amount: None, fee: None };
        assert!(create(no_to).is_err(), "при обмене нужна сумма зачисления");
        let big_fee = Body::Transfer { from: "card".into(), to: "cash".into(), amount: dec("10"), to_amount: None, fee: Some(Fee { category_code: "fees".into(), amount: dec("10") }) };
        assert!(create(big_fee).is_err(), "комиссия меньше суммы");
    }

    #[test]
    fn opening_balance() {
        let t = create(Body::Opening { account: "card".into(), amount: dec("-150") }).unwrap();
        assert_eq!(amounts(&t), vec![("CARD", "THB", dec("-150")), ("OPENING", "THB", dec("150"))]);
        assert!(create(Body::Opening { account: "card".into(), amount: Decimal::ZERO }).is_err());
        let mut planned = data(Body::Opening { account: "card".into(), amount: dec("1") });
        planned.plan = Some((Uuid::nil(), planned.occurred_on));
        assert!(Transaction::create(Uuid::nil(), Uuid::nil(), planned, &accounts()).is_err());
    }

    #[test]
    fn balance_rule() {
        let p = |cur: &str, a: &str| Posting::new(RefKind::Account, "X", cur, dec(a), None);
        assert!(check_balanced(&[p("THB", "10"), p("THB", "-10")]).is_ok());
        assert!(check_balanced(&[p("THB", "10"), p("RUB", "-10")]).is_err(), "разные валюты не сводят друг друга");
        assert!(check_balanced(&[p("THB", "10")]).is_err());
        assert!(check_balanced(&[p("THB", "0"), p("THB", "0")]).is_err());
    }

    impl Body {
        /// Как поля выглядят после нормализации кодов (для сравнения в тестах).
        fn normalized(self) -> Body {
            build(self, &accounts()).unwrap().0
        }
    }
}
