//! Доменная модель «Транзакции»: двойная запись. Транзакция — шапка + проводки; сумма проводок по
//! каждой валюте равна нулю (ТЗ, раздел 4). У расхода и дохода — позиции чека: количество × цена − скидка
//! = сумма позиции, на каждую позицию одна проводка на категорию (docs/group-1.md, разделы 4.2 и 5.1).
//! Никаких sqlx/axum/serde.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use kernel::error::{Problem, codes};
use kernel::i18n::{Lang, Localized};
use rust_decimal::{Decimal, RoundingStrategy};
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
    /// Позиция чека, которую проводит эта проводка (только CATEGORY у расхода и дохода).
    pub item_id: Option<Uuid>,
}

impl Posting {
    fn new(ref_kind: RefKind, ref_code: &str, currency: &str, amount: Decimal) -> Self {
        Self { ref_kind, ref_code: ref_code.to_string(), currency: currency.to_string(), amount, note: None, item_id: None }
    }

    pub fn is_account(&self, code: &str) -> bool {
        self.ref_kind == RefKind::Account && self.ref_code == code
    }
}

/// Единица количества позиции.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Unit {
    #[default]
    Pcs,
    Kg,
    L,
    G,
}

impl Unit {
    pub const ALL: [Unit; 4] = [Unit::Pcs, Unit::Kg, Unit::L, Unit::G];

    /// `pcs` / `kg` / `l` / `g` — в БД и API.
    pub fn as_str(&self) -> &'static str {
        match self {
            Unit::Pcs => "pcs",
            Unit::Kg => "kg",
            Unit::L => "l",
            Unit::G => "g",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Unit::ALL.into_iter().find(|u| u.as_str().eq_ignore_ascii_case(s.trim()))
    }
}

/// Позиция чека расхода или дохода.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Не меняется при правке.
    pub id: Uuid,
    pub category_code: String,
    /// Название на языке ввода (данные пользователя, без переводов).
    pub name: Option<String>,
    pub quantity: Decimal,
    pub unit: Unit,
    /// Цена за единицу с ценника; `None` — введена только сумма.
    pub price: Option<Decimal>,
    /// Скидка на позицию (с долей скидки на чек).
    pub discount: Decimal,
    /// Сумма позиции > 0: с ценой — `round(количество × цена) − скидка`, без цены — введённая.
    pub amount: Decimal,
    pub note: Option<String>,
}

impl Item {
    /// Одна штука без названия, цены и скидки — в ленте выглядит как обычная операция.
    pub fn is_simple(&self) -> bool {
        self.name.is_none() && self.price.is_none() && self.quantity == Decimal::ONE && self.unit == Unit::Pcs && self.discount.is_zero()
    }

    /// Есть что показать в строке «× цена»: цена, количество не 1, единица не «шт» или скидка.
    pub fn has_details(&self) -> bool {
        self.price.is_some() || self.quantity != Decimal::ONE || self.unit != Unit::Pcs || !self.discount.is_zero()
    }
}

/// Позиция из формы или запроса. Пустые поля — значения по умолчанию (1 шт, без цены и скидки). При правке по `id`
/// через API (`ItemsDraft::replace == false`) пустые поля остаются прежними, а сумма без цены убирает цену; форма
/// присылает строку целиком: пустое название — без названия.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemDraft {
    pub id: Option<Uuid>,
    pub category_code: Option<String>,
    pub name: Option<String>,
    pub quantity: Option<Decimal>,
    pub unit: Option<String>,
    pub price: Option<Decimal>,
    pub discount: Option<Decimal>,
    pub amount: Option<Decimal>,
    pub note: Option<String>,
}

impl ItemDraft {
    /// Позиция «категория и сумма» — как в быстром вводе и старом формате `lines`.
    pub fn simple(category_code: &str, amount: Decimal) -> Self {
        Self { category_code: Some(category_code.to_string()), amount: Some(amount), ..Default::default() }
    }

    fn money_given(&self) -> bool {
        self.quantity.is_some() || self.unit.is_some() || self.price.is_some() || self.discount.is_some() || self.amount.is_some()
    }
}

/// Как изменить позиции операции (docs/group-1.md, 5.6): с `id` — изменить, без `id` — добавить.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemsDraft {
    pub items: Vec<ItemDraft>,
    /// `true` — `items` и есть весь чек: не названные позиции удаляются (форма, старый формат `lines`).
    /// `false` — не названные позиции остаются, удаляются только `remove` (REST и MCP).
    pub replace: bool,
    pub remove: Vec<Uuid>,
    /// Скидка на чек: делится по позициям пропорционально сумме и прибавляется к их скидкам; не хранится.
    pub discount_total: Option<Decimal>,
}

impl ItemsDraft {
    /// Весь чек целиком.
    pub fn replace(items: Vec<ItemDraft>) -> Self {
        Self { items, replace: true, ..Default::default() }
    }
}

/// Комиссия перевода: расход на категорию, списывается со счёта списания.
#[derive(Debug, Clone, PartialEq)]
pub struct Fee {
    pub category_code: String,
    pub amount: Decimal,
}

/// Поля формы — то, что ввёл пользователь. Из них и записанных позиций сервер строит проводки.
#[derive(Debug, Clone, PartialEq)]
pub enum Draft {
    /// Расход или доход: счёт и позиции. `refund` — возврат: те же проводки с обратными знаками.
    Flow { income: bool, account: String, items: ItemsDraft, refund: bool },
    /// Перевод между счетами. `amount` — сколько всего списано со счёта (вместе с комиссией);
    /// `to_amount` — сколько зачислено (задаётся только при разных валютах).
    Transfer { from: String, to: String, amount: Decimal, to_amount: Option<Decimal>, fee: Option<Fee> },
    /// Начальный остаток счёта (может быть отрицательным).
    Opening { account: String, amount: Decimal },
}

impl Draft {
    pub fn kind(&self) -> TxKind {
        match self {
            Draft::Flow { income: false, .. } => TxKind::Expense,
            Draft::Flow { income: true, .. } => TxKind::Income,
            Draft::Transfer { .. } => TxKind::Transfer,
            Draft::Opening { .. } => TxKind::Opening,
        }
    }
}

/// Поля записанной транзакции (для форм, API и других фич).
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Flow { income: bool, account: String, items: Vec<Item>, refund: bool },
    Transfer { from: String, to: String, amount: Decimal, to_amount: Option<Decimal>, fee: Option<Fee> },
    Opening { account: String, amount: Decimal },
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
    /// Место (справочник `places`) — у расхода и дохода; у перевода и начального остатка всегда пусто.
    pub place_code: Option<String>,
    /// Позиции чека (расход и доход), в порядке чека.
    pub items: Vec<Item>,
    pub postings: Vec<Posting>,
}

/// Данные для создания и изменения.
#[derive(Debug, Clone)]
pub struct TransactionData {
    pub occurred_on: NaiveDate,
    pub description: Description,
    pub plan: Option<(Uuid, NaiveDate)>,
    /// Код места; у перевода и начального остатка не сохраняется.
    pub place_code: Option<String>,
    pub body: Draft,
}

/// Позиций в одной транзакции — не больше (защита от мусорных запросов).
pub const MAX_LINES: usize = 100;
/// Денежные колонки — NUMERIC(18,4): по модулю меньше 10¹⁴ (техдолг #20).
const MAX_SCALE: u32 = 4;
const MAX_MONEY: Decimal = Decimal::from_parts(0x107A_4000, 0x5AF3, 0, false, 0); // 10^14
/// Количество и цена — NUMERIC(18,6): до 6 знаков после запятой, меньше 10¹².
const QTY_SCALE: u32 = 6;
const MAX_QTY: Decimal = Decimal::from_parts(0xD4A5_1000, 0xE8, 0, false, 0); // 10^12

/// Ошибка поля с текстом из словаря фичи на языке запроса.
fn err(field: &str, key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> Problem {
    Problem::validation(field, kernel::i18n::tr(Some(crate::i18n::dict), key, args))
}

fn too_big(field: &str) -> Problem {
    Problem::validation(field, kernel::i18n::tr(None, "err.too_big", &[]))
}

/// Точность и предел денежной суммы в валюте `cur`.
fn check_amount(field: &str, amount: Decimal, cur: &AccountCurrency) -> Result<(), Problem> {
    let n = cur.minor_units.min(MAX_SCALE);
    if amount.normalize().scale() > n {
        return Err(err(field, "err.precision", &[("currency", &cur.currency), ("n", &n)]));
    }
    if amount.abs() >= MAX_MONEY {
        return Err(too_big(field));
    }
    Ok(())
}

fn positive(field: &str, amount: Decimal, cur: &AccountCurrency) -> Result<(), Problem> {
    if amount <= Decimal::ZERO {
        return Err(err(field, "err.positive", &[]));
    }
    check_amount(field, amount, cur)
}

/// Количество и цена: до 6 знаков после запятой и меньше 10¹².
fn check_qty(field: &str, v: Decimal) -> Result<(), Problem> {
    if v.normalize().scale() > QTY_SCALE {
        return Err(err(field, "err.qty_scale", &[("n", &QTY_SCALE)]));
    }
    if v.abs() >= MAX_QTY {
        return Err(too_big(field));
    }
    Ok(())
}

/// Округление суммы до точности валюты, половина — вверх.
fn round_money(v: Decimal, cur: &AccountCurrency) -> Decimal {
    v.round_dp_with_strategy(cur.minor_units.min(MAX_SCALE), RoundingStrategy::MidpointAwayFromZero)
}

fn account<'a>(field: &str, code: &str, accounts: &'a [AccountCurrency]) -> Result<(String, &'a AccountCurrency), Problem> {
    let code = kernel::code::code(field, code)?;
    let cur = accounts.iter().find(|a| a.code == code).ok_or_else(|| Problem::new(codes::NOT_FOUND, field, kernel::i18n::tr(Some(crate::i18n::dict), "err.account_not_found", &[("code", &code)])))?;
    Ok((code, cur))
}

fn sum(field: &str, values: impl Iterator<Item = Decimal>) -> Result<Decimal, Problem> {
    let mut total = Decimal::ZERO;
    for v in values {
        total = total.checked_add(v).filter(|t| t.abs() < MAX_MONEY).ok_or_else(|| too_big(field))?;
    }
    Ok(total)
}

/// Количество, единица, цена, скидка и сумма позиции по правилам раздела 5.1. `old` — записанная позиция при правке
/// по `id` через API: пустые поля берутся из неё, а сумма без цены убирает цену («передана только сумма — цена
/// пустая»). Без `old` пустые поля — значения по умолчанию: 1 шт, без цены и скидки.
fn money(i: usize, d: &ItemDraft, old: Option<&Item>, cur: &AccountCurrency) -> Result<(Decimal, Unit, Option<Decimal>, Decimal, Decimal), Problem> {
    let f = |name: &str| format!("items[{i}].{name}");
    let quantity = d.quantity.or(old.map(|o| o.quantity)).unwrap_or(Decimal::ONE);
    if quantity <= Decimal::ZERO {
        return Err(err(&f("quantity"), "err.quantity_positive", &[]));
    }
    check_qty(&f("quantity"), quantity)?;
    let unit = match d.unit.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        None => old.map(|o| o.unit).unwrap_or_default(),
        Some(u) => Unit::parse(u).ok_or_else(|| err(&f("unit"), "err.unit_invalid", &[]))?,
    };
    let discount = d.discount.or(old.map(|o| o.discount)).unwrap_or_default();
    if discount < Decimal::ZERO {
        return Err(err(&f("discount"), "err.discount_negative", &[]));
    }
    check_amount(&f("discount"), discount, cur)?;
    let price = match (d.price, d.amount) {
        (Some(p), _) => Some(p),
        (None, Some(_)) => None,
        (None, None) => old.and_then(|o| o.price),
    };
    // Без цены сумма — введённая; при правке без новой суммы — прежняя.
    let given = d.amount.or(if price.is_none() { old.map(|o| o.amount) } else { None });
    let amount = match (price, given) {
        (Some(price), given) => {
            if price < Decimal::ZERO {
                return Err(err(&f("price"), "err.price_negative", &[]));
            }
            check_qty(&f("price"), price)?;
            let gross = round_money(quantity.checked_mul(price).ok_or_else(|| too_big(&f("price")))?, cur);
            let computed = gross - discount;
            if computed <= Decimal::ZERO {
                return Err(err(&f(if discount.is_zero() { "price" } else { "discount" }), "err.discount_too_big", &[]));
            }
            check_amount(&f("amount"), computed, cur)?;
            match given {
                None => computed,
                Some(a) => {
                    positive(&f("amount"), a, cur)?;
                    // Допуск — единица младшего разряда валюты: в пределах хранится присланная сумма.
                    if (a - computed).abs() > Decimal::new(1, cur.minor_units.min(MAX_SCALE)) {
                        let message = kernel::i18n::tr(Some(crate::i18n::dict), "err.amount_mismatch", &[("amount", &a.normalize()), ("computed", &computed.normalize())]);
                        return Err(Problem::new(codes::AMOUNT_MISMATCH, &f("amount"), message));
                    }
                    a
                }
            }
        }
        (None, Some(a)) => {
            positive(&f("amount"), a, cur)?;
            a
        }
        (None, None) => return Err(err(&f("amount"), "err.amount_or_price", &[])),
    };
    Ok((quantity, unit, price, discount, amount))
}

/// Позиция из черновика; `i` — её номер в чеке после правки, `old` — записанная позиция с тем же `id`.
/// `patch` — правка по `id` через API: пустые поля остаются прежними. Иначе черновик — вся строка формы:
/// пустое название — без названия, пустая категория — ошибка.
fn item(i: usize, d: ItemDraft, old: Option<&Item>, patch: bool, cur: &AccountCurrency) -> Result<Item, Problem> {
    let field = format!("items[{i}].category_code");
    let base = if patch { old } else { None };
    // Категория места по умолчанию появится со справочником мест (шаг 5); пока — обязательна.
    let category = match d.category_code.as_deref().map(str::trim) {
        Some(c) if !c.is_empty() => c.to_string(),
        Some(_) => return Err(err(&field, "err.category_required", &[])),
        None => base.map(|o| o.category_code.clone()).ok_or_else(|| err(&field, "err.category_required", &[]))?,
    };
    let category_code = kernel::code::code(&field, &category)?;
    let (quantity, unit, price, discount, amount) = match base {
        // Ничего из денег не прислали — прежние значения как есть, без пересчёта.
        Some(o) if !d.money_given() => (o.quantity, o.unit, o.price, o.discount, o.amount),
        _ => money(i, &d, base, cur)?,
    };
    let keep = |v: Option<String>, prev: Option<&Option<String>>| match v {
        Some(v) => kernel::i18n::clean(Some(v)),
        None => prev.cloned().flatten(),
    };
    Ok(Item {
        id: d.id.or(old.map(|o| o.id)).unwrap_or_else(Uuid::new_v4),
        category_code,
        name: keep(d.name, base.map(|o| &o.name)),
        quantity,
        unit,
        price,
        discount,
        amount,
        note: keep(d.note, base.map(|o| &o.note)),
    })
}

/// Скидка на чек: доли пропорционально суммам позиций (вниз до точности валюты), остаток округления —
/// самой крупной позиции. Доля прибавляется к скидке позиции и вычитается из её суммы.
fn split_discount(items: &mut [Item], total_discount: Decimal, cur: &AccountCurrency) -> Result<(), Problem> {
    let field = "discount_total";
    if total_discount < Decimal::ZERO {
        return Err(err(field, "err.discount_negative", &[]));
    }
    check_amount(field, total_discount, cur)?;
    let total = sum(field, items.iter().map(|x| x.amount))?;
    if total_discount >= total {
        return Err(err(field, "err.receipt_discount_too_big", &[]));
    }
    let n = cur.minor_units.min(MAX_SCALE);
    let mut shares = Vec::with_capacity(items.len());
    for x in items.iter() {
        let share = total_discount.checked_mul(x.amount).ok_or_else(|| too_big(field))? / total;
        shares.push(share.round_dp_with_strategy(n, RoundingStrategy::ToZero));
    }
    let rest = total_discount - shares.iter().copied().sum::<Decimal>();
    let largest = (0..items.len()).max_by(|a, b| items[*a].amount.cmp(&items[*b].amount).then(b.cmp(a))).unwrap_or_default();
    shares[largest] += rest;
    for (i, (x, share)) in items.iter_mut().zip(shares).enumerate() {
        x.discount += share;
        x.amount -= share;
        if x.amount <= Decimal::ZERO {
            return Err(err(&format!("items[{}].amount", i + 1), "err.discount_too_big", &[]));
        }
    }
    Ok(())
}

/// Позиции после правки: записанные `existing` + черновик (раздел 5.6). Чек после правки: при правке по `id`
/// (REST, MCP) — записанные позиции без удалённых на своих местах, новые в конце; форма присылает весь чек — в её
/// порядке. Номер позиции в ошибках — номер в этом чеке (в ошибках `id` — номер в запросе).
fn resolve_items(d: ItemsDraft, existing: &[Item], cur: &AccountCurrency) -> Result<Vec<Item>, Problem> {
    for id in &d.remove {
        if !existing.iter().any(|x| x.id == *id) {
            return Err(err("remove_item_ids", "err.item_unknown", &[("id", id)]));
        }
    }
    let patch = !d.replace;
    let mut out: Vec<Item> = if patch { existing.iter().filter(|x| !d.remove.contains(&x.id)).cloned().collect() } else { Vec::new() };
    let mut seen: Vec<Uuid> = Vec::new();
    for (n, draft) in d.items.into_iter().enumerate() {
        let old = match draft.id {
            None => None,
            Some(id) => {
                let field = format!("items[{}].id", n + 1);
                if seen.contains(&id) || d.remove.contains(&id) {
                    return Err(err(&field, "err.item_twice", &[("id", &id)]));
                }
                seen.push(id);
                Some(existing.iter().find(|x| x.id == id).ok_or_else(|| err(&field, "err.item_unknown", &[("id", &id)]))?)
            }
        };
        let slot = if patch { old.and_then(|o| out.iter().position(|x| x.id == o.id)) } else { None };
        let new = item(slot.map_or(out.len(), |s| s) + 1, draft, old, patch, cur)?;
        match slot {
            Some(s) => out[s] = new,
            None => out.push(new),
        }
    }
    if out.is_empty() {
        return Err(err("items", "err.items_empty", &[]));
    }
    if out.len() > MAX_LINES {
        return Err(err("items", "err.items_max", &[("n", &MAX_LINES)]));
    }
    // Оставшиеся как были позиции — тоже в валюте счёта: счёт могли сменить на валюту с другой точностью.
    for (i, x) in out.iter().enumerate() {
        positive(&format!("items[{}].amount", i + 1), x.amount, cur)?;
        check_amount(&format!("items[{}].discount", i + 1), x.discount, cur)?;
    }
    if let Some(dt) = d.discount_total.filter(|v| !v.is_zero()) {
        split_discount(&mut out, dt, cur)?;
    }
    Ok(out)
}

/// Нормализует поля формы и строит позиции и проводки (ТЗ, раздел 4, таблица типов).
fn build(draft: Draft, existing: &[Item], accounts: &[AccountCurrency]) -> Result<(Vec<Item>, Vec<Posting>), Problem> {
    match draft {
        Draft::Flow { income, account: acc, items, refund } => {
            let (acc, cur) = account("account_code", &acc, accounts)?;
            let items = resolve_items(items, existing, cur)?;
            // Расход: счёт −, категории +. Доход — наоборот. Возврат меняет знаки.
            let sign = if income != refund { -Decimal::ONE } else { Decimal::ONE };
            let total = sum("items", items.iter().map(|x| x.amount))?;
            let mut postings = vec![Posting::new(RefKind::Account, &acc, &cur.currency, -sign * total)];
            postings.extend(items.iter().map(|x| Posting {
                note: x.name.clone(),
                item_id: Some(x.id),
                ..Posting::new(RefKind::Category, &x.category_code, &cur.currency, sign * x.amount)
            }));
            Ok((items, postings))
        }
        Draft::Transfer { from, to, amount, to_amount, fee } => {
            let (from, fc) = account("account_code", &from, accounts)?;
            let (to, tc) = account("to_account_code", &to, accounts)?;
            if from == to {
                return Err(err("to_account_code", "err.same_account", &[]));
            }
            positive("amount", amount, fc)?;
            let fee = match fee {
                None => None,
                Some(f) => {
                    positive("fee_amount", f.amount, fc)?;
                    if f.amount >= amount {
                        return Err(err("fee_amount", "err.fee_too_big", &[]));
                    }
                    Some(Fee { category_code: kernel::code::code("fee_category_code", &f.category_code)?, amount: f.amount })
                }
            };
            let sent = amount - fee.as_ref().map(|f| f.amount).unwrap_or_default();
            let mut postings = vec![Posting::new(RefKind::Account, &from, &fc.currency, -amount)];
            if let Some(f) = &fee {
                postings.push(Posting::new(RefKind::Category, &f.category_code, &fc.currency, f.amount));
            }
            if fc.currency == tc.currency {
                if to_amount.is_some_and(|t| t != sent) {
                    return Err(err("to_amount", "err.to_amount_same", &[]));
                }
                postings.push(Posting::new(RefKind::Account, &to, &tc.currency, sent));
            } else {
                let got = to_amount.ok_or_else(|| err("to_amount", "err.to_amount_required", &[]))?;
                positive("to_amount", got, tc)?;
                postings.push(Posting::new(RefKind::System, system::EXCHANGE, &fc.currency, sent));
                postings.push(Posting::new(RefKind::System, system::EXCHANGE, &tc.currency, -got));
                postings.push(Posting::new(RefKind::Account, &to, &tc.currency, got));
            }
            Ok((Vec::new(), postings))
        }
        Draft::Opening { account: acc, amount } => {
            let (acc, cur) = account("account_code", &acc, accounts)?;
            if amount.is_zero() {
                return Err(err("amount", "err.opening_zero", &[]));
            }
            check_amount("amount", amount, cur)?;
            let postings = vec![Posting::new(RefKind::Account, &acc, &cur.currency, amount), Posting::new(RefKind::System, system::OPENING, &cur.currency, -amount)];
            Ok((Vec::new(), postings))
        }
    }
}

/// Главное правило двойной записи: не меньше двух проводок, без нулевых сумм, сумма по каждой
/// валюте равна нулю.
pub fn check_balanced(postings: &[Posting]) -> Result<(), String> {
    if postings.len() < 2 {
        return Err("postings: at least two postings are required".into());
    }
    if postings.iter().any(|p| p.amount.is_zero()) {
        return Err("postings: a posting amount cannot be zero".into());
    }
    let mut sums: BTreeMap<&str, Decimal> = BTreeMap::new();
    for p in postings {
        *sums.entry(p.currency.as_str()).or_default() += p.amount;
    }
    match sums.into_iter().find(|(_, s)| !s.is_zero()) {
        Some((cur, s)) => Err(format!("postings: the sum in {cur} is {s}, must be 0")),
        None => Ok(()),
    }
}

/// Инвариант «позиция ↔ проводка» (docs/group-1.md, 4.2): у расхода и дохода каждая проводка CATEGORY
/// ссылается на свою позицию, одна проводка на позицию, её сумма — ±amount; у остальных позиций нет.
pub fn check_items(kind: TxKind, items: &[Item], postings: &[Posting]) -> Result<(), String> {
    let cats: Vec<&Posting> = postings.iter().filter(|p| p.ref_kind == RefKind::Category).collect();
    match kind {
        TxKind::Expense | TxKind::Income => {
            if cats.len() != items.len() {
                return Err(format!("items: {} items, {} category postings", items.len(), cats.len()));
            }
            for x in items {
                let mine: Vec<&&Posting> = cats.iter().filter(|p| p.item_id == Some(x.id)).collect();
                if mine.len() != 1 || mine[0].amount.abs() != x.amount || mine[0].ref_code != x.category_code {
                    return Err(format!("items: item {} has no matching posting", x.id));
                }
            }
            Ok(())
        }
        TxKind::Transfer | TxKind::Opening if items.is_empty() && postings.iter().all(|p| p.item_id.is_none()) => Ok(()),
        _ => Err("items: only expenses and income have items".into()),
    }
}

impl Transaction {
    pub fn create(project_id: Uuid, created_by: Uuid, data: TransactionData, accounts: &[AccountCurrency]) -> Result<Self, Problem> {
        let mut t = Self {
            id: Uuid::new_v4(),
            project_id,
            occurred_on: data.occurred_on,
            tx_kind: data.body.kind(),
            description: Description::default(),
            created_by,
            plan: None,
            place_code: None,
            items: Vec::new(),
            postings: Vec::new(),
        };
        t.change(data, accounts)?;
        Ok(t)
    }

    /// Изменение: набор проводок пересобирается целиком, тип можно сменить. Позиции — по `id`
    /// (раздел 5.6): записанные позиции сохраняют `id`.
    pub fn change(&mut self, data: TransactionData, accounts: &[AccountCurrency]) -> Result<(), Problem> {
        let kind = data.body.kind();
        if kind == TxKind::Opening && data.plan.is_some() {
            return Err(err("plan_id", "err.opening_plan", &[]));
        }
        let place_code = match kind {
            TxKind::Expense | TxKind::Income => data.place_code.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(|v| kernel::code::code("place_code", v)).transpose()?,
            TxKind::Transfer | TxKind::Opening => None,
        };
        let (items, postings) = build(data.body, &self.items, accounts)?;
        check_balanced(&postings).and_then(|_| check_items(kind, &items, &postings)).map_err(|m| Problem::validation("postings", m))?;
        self.occurred_on = data.occurred_on;
        self.tx_kind = kind;
        self.description = data.description.cleaned();
        self.plan = data.plan;
        self.place_code = place_code;
        self.items = items;
        self.postings = postings;
        Ok(())
    }

    /// Поля формы, восстановленные из позиций и проводок (для редактирования и API).
    pub fn body(&self) -> Body {
        let accounts: Vec<&Posting> = self.postings.iter().filter(|p| p.ref_kind == RefKind::Account).collect();
        let cats = self.postings.iter().filter(|p| p.ref_kind == RefKind::Category);
        let first = accounts.first().map(|p| (p.ref_code.clone(), p.amount)).unwrap_or_default();
        match self.tx_kind {
            TxKind::Expense | TxKind::Income => {
                let income = self.tx_kind == TxKind::Income;
                Body::Flow { income, account: first.0, items: self.items.clone(), refund: self.refund() }
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

    /// Возврат: расход пополнил счёт или доход его уменьшил.
    pub fn refund(&self) -> bool {
        let income = self.tx_kind == TxKind::Income;
        let first = self.postings.iter().find(|p| p.ref_kind == RefKind::Account).map(|p| p.amount).unwrap_or_default();
        matches!(self.tx_kind, TxKind::Expense | TxKind::Income) && (first > Decimal::ZERO) != income
    }

    /// Сумма транзакции: у расхода и дохода — сумма позиций, у перевода — сколько списано,
    /// у начального остатка — сам остаток.
    pub fn total(&self) -> Decimal {
        match self.tx_kind {
            TxKind::Expense | TxKind::Income => self.items.iter().map(|x| x.amount).sum(),
            TxKind::Transfer => -self.postings.iter().filter(|p| p.ref_kind == RefKind::Account && p.amount < Decimal::ZERO).map(|p| p.amount).next().unwrap_or_default(),
            TxKind::Opening => self.postings.iter().find(|p| p.ref_kind == RefKind::Account).map(|p| p.amount).unwrap_or_default(),
        }
    }

    /// Счёт операции: у перевода — счёт списания.
    pub fn account_code(&self) -> &str {
        let mut accounts = self.postings.iter().filter(|p| p.ref_kind == RefKind::Account);
        let main = if self.tx_kind == TxKind::Transfer { accounts.find(|p| p.amount < Decimal::ZERO) } else { accounts.next() };
        main.map_or("", |p| p.ref_code.as_str())
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
            AccountCurrency { code: "VCB".into(), currency: "VND".into(), minor_units: 0 },
        ]
    }

    fn data(body: Draft) -> TransactionData {
        TransactionData {
            occurred_on: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            description: Description { text: Some(" Чек ".into()), lang: Lang::Ru, ru: Some("Чек".into()), en: Some("Receipt".into()), th: Some(" ".into()) },
            plan: None,
            place_code: None,
            body,
        }
    }

    fn flow_items(income: bool, refund: bool, account: &str, items: Vec<ItemDraft>) -> Draft {
        Draft::Flow { income, account: account.into(), items: ItemsDraft::replace(items), refund }
    }

    fn flow(income: bool, refund: bool, account: &str, lines: Vec<(&str, &str)>) -> Draft {
        flow_items(income, refund, account, lines.into_iter().map(|(c, a)| ItemDraft::simple(c, dec(a))).collect())
    }

    fn receipt(lines: Vec<(&str, &str)>) -> Draft {
        flow(false, false, "card", lines)
    }

    fn create(body: Draft) -> Result<Transaction, Problem> {
        Transaction::create(Uuid::nil(), Uuid::nil(), data(body), &accounts())
    }

    fn amounts(t: &Transaction) -> Vec<(&str, &str, Decimal)> {
        t.postings.iter().map(|p| (p.ref_code.as_str(), p.currency.as_str(), p.amount)).collect()
    }

    /// «× цена»: количество, единица, цена, скидка.
    fn priced(category: &str, name: &str, qty: &str, unit: &str, price: &str, discount: &str) -> ItemDraft {
        ItemDraft {
            category_code: Some(category.into()),
            name: Some(name.into()),
            quantity: Some(dec(qty)),
            unit: Some(unit.into()),
            price: Some(dec(price)),
            discount: Some(dec(discount)),
            ..Default::default()
        }
    }

    #[test]
    fn receipt_is_balanced_and_description_is_cleaned() {
        let t = create(receipt(vec![("food", "900"), ("home", "250"), ("cafe", "100")])).unwrap();
        assert_eq!(t.total(), dec("1250"));
        assert_eq!(amounts(&t), vec![("CARD", "THB", dec("-1250")), ("FOOD", "THB", dec("900")), ("HOME", "THB", dec("250")), ("CAFE", "THB", dec("100"))]);
        assert_eq!(t.description.text.as_deref(), Some("Чек"));
        assert_eq!(t.description.ru, None, "перевод на язык оригинала не хранится");
        assert_eq!(t.description.th, None);
        assert!(t.items.iter().all(Item::is_simple), "категория и сумма — простая позиция: 1 шт, без цены");
        assert!(check_items(t.tx_kind, &t.items, &t.postings).is_ok());
    }

    #[test]
    fn refund_and_income_flip_signs() {
        let refund = create(flow(false, true, "card", vec![("food", "300")])).unwrap();
        assert_eq!(amounts(&refund), vec![("CARD", "THB", dec("300")), ("FOOD", "THB", dec("-300"))]);
        assert!(matches!(refund.body(), Body::Flow { refund: true, income: false, .. }));
        assert!(refund.refund());
        let salary = create(flow(true, false, "card", vec![("salary", "100000")])).unwrap();
        assert_eq!(amounts(&salary), vec![("CARD", "THB", dec("100000")), ("SALARY", "THB", dec("-100000"))]);
        assert_eq!((salary.tx_kind, salary.total(), salary.refund()), (TxKind::Income, dec("100000"), false));
    }

    #[test]
    fn invalid_lines() {
        assert_eq!(create(receipt(vec![])).unwrap_err().field, "items");
        assert_eq!(create(receipt(vec![("food", "0")])).unwrap_err().field, "items[1].amount");
        assert!(create(receipt(vec![("food", "-5")])).is_err());
        assert!(create(receipt(vec![("food", "1.001")])).is_err(), "у THB два знака после запятой");
        assert_eq!(create(flow(false, false, "nope", vec![("food", "1")])).unwrap_err().code, codes::NOT_FOUND);
        assert!(create(receipt(vec![("food", "100000000000000")])).is_err(), "10¹⁴ не помещается в NUMERIC(18,4)");
        let no_category = flow_items(false, false, "card", vec![ItemDraft { amount: Some(dec("5")), ..Default::default() }]);
        assert_eq!(create(no_category).unwrap_err().field, "items[1].category_code");
        let nothing = flow_items(false, false, "card", vec![ItemDraft { category_code: Some("food".into()), ..Default::default() }]);
        assert_eq!(create(nothing).unwrap_err().field, "items[1].amount", "нет ни суммы, ни цены");
    }

    /// Раздел 5.1: сумма = round(количество × цена, точность валюты, половина вверх) − скидка.
    #[test]
    fn amount_from_quantity_and_price() {
        let t = create(flow_items(
            false,
            false,
            "card",
            vec![
                priced("snacks", "Круассан", "2", "pcs", "25", "3"),
                priced("food", "Бананы", "0.45", "kg", "60", "0"),
                // 0,335 кг × 15 = 5,025 → половина вверх, 5,03.
                priced("food", "Сыр", "0.335", "kg", "15", "0"),
            ],
        ))
        .unwrap();
        assert_eq!(t.items.iter().map(|x| x.amount).collect::<Vec<_>>(), vec![dec("47"), dec("27"), dec("5.03")]);
        assert_eq!(t.items[1].unit, Unit::Kg);
        assert_eq!(t.total(), dec("79.03"));
        assert!(t.items.iter().all(|x| !x.is_simple() && x.has_details()));
        // Донг — без копеек: 3 × 33 333,5 = 100 000,5 → 100 001.
        let vnd = create(flow_items(false, false, "vcb", vec![priced("food", "Phở", "3", "pcs", "33333.5", "0")])).unwrap();
        assert_eq!(vnd.total(), dec("100001"));
    }

    /// Раздел 5.1: присланная сумма и цена расходятся больше чем на единицу младшего разряда —
    /// `AMOUNT_MISMATCH`; в пределах допуска хранится присланная сумма.
    #[test]
    fn amount_must_match_price() {
        let with = |amount: &str| ItemDraft { amount: Some(dec(amount)), ..priced("snacks", "Круассан", "2", "pcs", "25", "3") };
        let e = create(flow_items(false, false, "card", vec![with("50")])).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), (codes::AMOUNT_MISMATCH, "items[1].amount"));
        assert!(e.message.contains("47"), "{e}");
        let t = create(flow_items(false, false, "card", vec![with("47.01")])).unwrap();
        assert_eq!(t.total(), dec("47.01"), "в пределах допуска — присланная сумма");
        let eaten = create(flow_items(false, false, "card", vec![priced("snacks", "Круассан", "1", "pcs", "25", "25")])).unwrap_err();
        assert_eq!(eaten.field, "items[1].discount", "скидка съела позицию целиком");
        let e = create(flow_items(false, false, "card", vec![priced("food", "x", "0", "pcs", "1", "0")])).unwrap_err();
        assert_eq!(e.field, "items[1].quantity");
        let e = create(flow_items(false, false, "card", vec![priced("food", "x", "1", "box", "1", "0")])).unwrap_err();
        assert_eq!(e.field, "items[1].unit");
        let e = create(flow_items(false, false, "card", vec![priced("food", "x", "0.0000001", "kg", "1", "0")])).unwrap_err();
        assert_eq!(e.field, "items[1].quantity", "не больше 6 знаков");
        let e = create(flow_items(false, false, "card", vec![priced("food", "x", "999999999999", "pcs", "999999999999", "0")])).unwrap_err();
        assert_eq!(e.field, "items[1].amount", "переполнение — ошибка поля, а не паника");
    }

    /// Раздел 5.1: скидка на чек делится пропорционально суммам, остаток округления — самой крупной.
    #[test]
    fn receipt_discount_is_split() {
        let mut body = receipt(vec![("food", "100"), ("cafe", "50"), ("home", "50")]);
        if let Draft::Flow { items, .. } = &mut body {
            items.discount_total = Some(dec("10.01"));
        }
        let t = create(body).unwrap();
        // 10,01 × 100/200 = 5,005 → 5,00; 2,5025 → 2,50 дважды; остаток 0,01 — самой крупной.
        assert_eq!(t.items.iter().map(|x| (x.discount, x.amount)).collect::<Vec<_>>(), vec![(dec("5.01"), dec("94.99")), (dec("2.50"), dec("47.50")), (dec("2.50"), dec("47.50"))]);
        assert_eq!(t.total(), dec("189.99"));
        let mut too_much = receipt(vec![("food", "100")]);
        if let Draft::Flow { items, .. } = &mut too_much {
            items.discount_total = Some(dec("100"));
        }
        assert_eq!(create(too_much).unwrap_err().field, "discount_total");
    }

    /// Раздел 5.6: позиции правятся по `id` — с `id` изменить, без `id` добавить, `remove` удалить;
    /// `id` записанных позиций не меняются.
    #[test]
    fn items_are_edited_by_id() {
        let mut t = create(receipt(vec![("food", "100"), ("cafe", "50")])).unwrap();
        let (food, cafe) = (t.items[0].id, t.items[1].id);
        let edit = |items: ItemsDraft| data(Draft::Flow { income: false, account: "card".into(), items, refund: false });
        // Только сумма кофе: категория и название остаются, новая позиция добавляется в конец.
        let draft = ItemsDraft { items: vec![ItemDraft { id: Some(cafe), amount: Some(dec("60")), ..Default::default() }, ItemDraft::simple("home", dec("7"))], ..Default::default() };
        t.change(edit(draft), &accounts()).unwrap();
        assert_eq!(t.items.iter().map(|x| (x.category_code.as_str(), x.amount)).collect::<Vec<_>>(), vec![("FOOD", dec("100")), ("CAFE", dec("60")), ("HOME", dec("7"))]);
        assert_eq!((t.items[0].id, t.items[1].id), (food, cafe));
        // Только название — денежные поля прежние.
        let draft = ItemsDraft { items: vec![ItemDraft { id: Some(food), name: Some("Овощи".into()), ..Default::default() }], remove: vec![cafe], ..Default::default() };
        t.change(edit(draft), &accounts()).unwrap();
        assert_eq!(t.items.iter().map(|x| (x.name.as_deref(), x.amount)).collect::<Vec<_>>(), vec![(Some("Овощи"), dec("100")), (None, dec("7"))]);
        assert!(check_items(t.tx_kind, &t.items, &t.postings).is_ok());
        // Чужой id и дважды один id — ошибки.
        let foreign = ItemsDraft { items: vec![ItemDraft { id: Some(Uuid::new_v4()), ..Default::default() }], ..Default::default() };
        assert_eq!(t.clone().change(edit(foreign), &accounts()).unwrap_err().field, "items[1].id");
        let twice = ItemsDraft { items: vec![ItemDraft { id: Some(food), ..Default::default() }, ItemDraft { id: Some(food), ..Default::default() }], ..Default::default() };
        assert_eq!(t.clone().change(edit(twice), &accounts()).unwrap_err().field, "items[2].id");
        // Форма — весь чек: не названная позиция удаляется, пустое название — без названия.
        let form = ItemsDraft::replace(vec![ItemDraft { id: Some(food), name: Some(String::new()), ..ItemDraft::simple("food", dec("100")) }]);
        t.change(edit(form), &accounts()).unwrap();
        assert_eq!((t.items.len(), t.items[0].id, t.total(), t.items[0].name.as_deref()), (1, food, dec("100"), None));
        let no_category = ItemsDraft::replace(vec![ItemDraft { id: Some(food), amount: Some(dec("1")), ..Default::default() }]);
        assert_eq!(t.clone().change(edit(no_category), &accounts()).unwrap_err().field, "items[1].category_code", "в форме категория обязательна");
        let none_left = ItemsDraft { remove: vec![food], ..Default::default() };
        assert_eq!(t.clone().change(edit(none_left), &accounts()).unwrap_err().field, "items");
    }

    /// Правка по id через API: пустые поля остаются прежними; новое количество пересчитывает сумму по прежней цене,
    /// сумма без цены убирает цену; номер позиции в ошибке — номер в чеке после правки.
    #[test]
    fn item_patch_keeps_omitted_fields() {
        let mut t = create(flow_items(false, false, "card", vec![ItemDraft::simple("food", dec("10")), priced("snacks", "Круассан", "2", "pcs", "25", "3")])).unwrap();
        let id = t.items[1].id;
        let patch = |x: ItemDraft| data(Draft::Flow { income: false, account: "card".into(), items: ItemsDraft { items: vec![ItemDraft { id: Some(id), ..x }], ..Default::default() }, refund: false });
        t.change(patch(ItemDraft { quantity: Some(dec("3")), ..Default::default() }), &accounts()).unwrap();
        assert_eq!((t.items[1].amount, t.items[1].price, t.items[1].discount, t.items[1].name.as_deref()), (dec("72"), Some(dec("25")), dec("3"), Some("Круассан")));
        t.change(patch(ItemDraft { amount: Some(dec("60")), ..Default::default() }), &accounts()).unwrap();
        assert_eq!((t.items[1].amount, t.items[1].price, t.items[1].quantity), (dec("60"), None, dec("3")), "сумма без цены — цена пустая");
        let e = t.clone().change(patch(ItemDraft { amount: Some(dec("0")), ..Default::default() }), &accounts()).unwrap_err();
        assert_eq!(e.field, "items[2].amount", "номер — в чеке, а не в запросе");
        t.change(patch(ItemDraft { name: Some(String::new()), ..Default::default() }), &accounts()).unwrap();
        assert_eq!(t.items[1].name, None, "пустая строка стирает название");
    }

    /// Счёт сменили на валюту без копеек — оставшиеся позиции проверяются по её точности.
    #[test]
    fn kept_items_follow_account_currency() {
        let mut t = create(receipt(vec![("food", "45.50")])).unwrap();
        let to_vnd = data(Draft::Flow { income: false, account: "vcb".into(), items: ItemsDraft::default(), refund: false });
        assert_eq!(t.change(to_vnd, &accounts()).unwrap_err().field, "items[1].amount");
    }

    /// Техдолг #9: ошибки домена — на языке запроса.
    #[tokio::test]
    async fn errors_are_translated() {
        let e = kernel::i18n::scope(Lang::Th, async { create(receipt(vec![("food", "0")])).unwrap_err() }).await;
        assert_eq!(e.message, "จำนวนเงินต้องมากกว่าศูนย์");
        let e = kernel::i18n::scope(Lang::En, async { create(receipt(vec![("food", "0")])).unwrap_err() }).await;
        assert_eq!(e.message, "the amount must be greater than zero");
    }

    /// ТЗ, раздел 4: обмен 10 000 ₽ → 3 912,40 ฿ с комиссией 50 ₽ — одна сбалансированная транзакция.
    #[test]
    fn exchange_with_fee() {
        let body = Draft::Transfer {
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
        assert!(t.items.is_empty() && t.postings.iter().all(|p| p.item_id.is_none()), "комиссия — проводка без позиции");
        assert_eq!(t.account_code(), "TINKOFF");
    }

    #[test]
    fn transfer_in_one_currency() {
        let body = Draft::Transfer { from: "card".into(), to: "cash".into(), amount: dec("5000"), to_amount: None, fee: None };
        let t = create(body).unwrap();
        assert_eq!(amounts(&t), vec![("CARD", "THB", dec("-5000")), ("CASH", "THB", dec("5000"))]);
        assert_eq!(t.body(), Body::Transfer { from: "CARD".into(), to: "CASH".into(), amount: dec("5000"), to_amount: None, fee: None });
        let same = Draft::Transfer { from: "card".into(), to: "card".into(), amount: dec("1"), to_amount: None, fee: None };
        assert!(create(same).is_err(), "перевод на тот же счёт");
        let wrong = Draft::Transfer { from: "card".into(), to: "cash".into(), amount: dec("10"), to_amount: Some(dec("9")), fee: None };
        assert!(create(wrong).is_err(), "в одной валюте зачисляется столько же");
        let no_to = Draft::Transfer { from: "tinkoff".into(), to: "card".into(), amount: dec("10"), to_amount: None, fee: None };
        assert!(create(no_to).is_err(), "при обмене нужна сумма зачисления");
        let big_fee = Draft::Transfer { from: "card".into(), to: "cash".into(), amount: dec("10"), to_amount: None, fee: Some(Fee { category_code: "fees".into(), amount: dec("10") }) };
        assert!(create(big_fee).is_err(), "комиссия меньше суммы");
    }

    #[test]
    fn opening_balance() {
        let t = create(Draft::Opening { account: "card".into(), amount: dec("-150") }).unwrap();
        assert_eq!(amounts(&t), vec![("CARD", "THB", dec("-150")), ("OPENING", "THB", dec("150"))]);
        assert_eq!(t.total(), dec("-150"));
        assert!(create(Draft::Opening { account: "card".into(), amount: Decimal::ZERO }).is_err());
        let mut planned = data(Draft::Opening { account: "card".into(), amount: dec("1") });
        planned.plan = Some((Uuid::nil(), planned.occurred_on));
        assert!(Transaction::create(Uuid::nil(), Uuid::nil(), planned, &accounts()).is_err());
    }

    #[test]
    fn balance_rule() {
        let p = |cur: &str, a: &str| Posting::new(RefKind::Account, "X", cur, dec(a));
        assert!(check_balanced(&[p("THB", "10"), p("THB", "-10")]).is_ok());
        assert!(check_balanced(&[p("THB", "10"), p("RUB", "-10")]).is_err(), "разные валюты не сводят друг друга");
        assert!(check_balanced(&[p("THB", "10")]).is_err());
        assert!(check_balanced(&[p("THB", "0"), p("THB", "0")]).is_err());
    }

    #[test]
    fn item_invariant() {
        let t = create(receipt(vec![("food", "100"), ("cafe", "50")])).unwrap();
        let mut broken = t.postings.clone();
        broken[2].item_id = broken[1].item_id;
        assert!(check_items(TxKind::Expense, &t.items, &broken).is_err(), "две проводки на одну позицию");
        let mut wrong_sum = t.items.clone();
        wrong_sum[0].amount = dec("99");
        assert!(check_items(TxKind::Expense, &wrong_sum, &t.postings).is_err());
        assert!(check_items(TxKind::Transfer, &t.items, &[]).is_err(), "у перевода позиций нет");
    }

    #[test]
    fn money_limits() {
        assert_eq!(MAX_MONEY, dec("100000000000000"));
        assert_eq!(MAX_QTY, dec("1000000000000"));
    }
}
