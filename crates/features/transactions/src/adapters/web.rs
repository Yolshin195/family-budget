//! Входящий адаптер: веб-страницы фичи «Транзакции».

use std::collections::HashMap;
use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Router};
use axum_extra::extract::Form;
use chrono::{Datelike, Months, NaiveDate};
use kernel::error::{AppError, Problem};
use kernel::i18n::{Lang, month_name, weekdays};
use kernel::web::{Choice, Flash, FormError, PageCtx, PageError, PageResult, money_in, parse_money, render, see_other, see_other_flash};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::{Description, Unit};
use crate::dto::{ItemDto, ItemInput, ListQuery, TransactionDto, TransactionInput, TranslationInput};
use crate::service::TransactionService;

type Svc = State<Arc<TransactionService>>;

pub fn router(service: Arc<TransactionService>) -> Router {
    Router::new()
        .route("/", get(list))
        .route("/new", get(new_form).post(create))
        .route("/reconcile", get(reconcile_form).post(reconcile))
        .route("/{id}/edit", get(edit_form).post(update))
        .route("/{id}/delete", post(delete))
        .route("/undo/{token}", post(undo))
        .route("/{id}/translate", get(translate_form).post(translate))
        .with_state(service)
}

fn ctx(page: PageCtx) -> PageCtx {
    page.for_feature(crate::NAME, crate::i18n::dict)
}

/// Вложенная страница: хлебная крошка на «Операции».
fn nested(page: PageCtx) -> PageCtx {
    let page = ctx(page);
    let title = page.t("title");
    page.crumb(title, crate::MOUNT)
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Подпись дня: «Пт 09.10».
fn day_label(lang: Lang, d: NaiveDate) -> String {
    format!("{} {:02}.{:02}", weekdays(lang)[d.weekday().num_days_from_monday() as usize], d.day(), d.month())
}

/// Сумма со знаком: «+300,00» / «−1 250,00».
fn signed(lang: Lang, v: Decimal) -> String {
    let s = money_in(lang, v.abs());
    if v < Decimal::ZERO { format!("−{s}") } else { format!("+{s}") }
}

/// Курс перевода в направлении, где число больше 1: «1 ฿ = 2,556 ₽» (сколько `from` за 1 `to`).
pub(crate) fn rate_text(lang: Lang, sent: Decimal, from: &str, got: Decimal, to: &str) -> String {
    if sent.is_zero() || got.is_zero() {
        return String::new();
    }
    let (base, quote, r) = if sent >= got { (to, from, sent / got) } else { (from, to, got / sent) };
    let r = r.round_dp(4).normalize();
    let s = if lang == Lang::Ru { r.to_string().replace('.', ",") } else { r.to_string() };
    format!("1 {base} = {s} {quote}")
}

/// Название системного счёта на языке интерфейса.
fn system_title(page: &PageCtx, code: &str) -> String {
    page.t(&format!("sys.{code}"))
}

// ---------------------------------------------------------------- список

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ListParams {
    /// YYYY-MM
    month: String,
    account: String,
    kind: String,
    member: String,
    untranslated: String,
}

/// Категория чека в ленте: сумма по категории и товары в порядке чека (docs/group-1.md, 3.3).
struct GroupView {
    category: String,
    amount: String,
    /// «Круассан · 2 шт × 25 − 3 → 47», «Латте · 45».
    items: Vec<String>,
}

struct TxView {
    dto: TransactionDto,
    /// expense / income / transfer / opening — для цвета точки.
    css: &'static str,
    desc: String,
    missing: bool,
    /// Счёт (у перевода — «откуда → куда») и автор.
    meta: String,
    /// Позиции по категориям — если операция не простая.
    groups: Vec<GroupView>,
    /// Категория простой операции (одна позиция без названия и цены).
    category: String,
    /// Главная сумма со знаком и её валюта.
    amount: String,
    currency: String,
    positive: bool,
    /// Вторая сумма перевода с обменом: «+3 912,40» и валюта.
    second: Option<(String, String)>,
    /// Пояснения: курс, комиссия, возврат.
    notes: Vec<String>,
    /// Остаток счёта после операции (выписка).
    running: Option<String>,
    edit_href: String,
}

struct DayGroup {
    label: String,
    items: Vec<TxView>,
}

/// Плитка сводки: подпись и суммы по валютам (сумма, валюта, отрицательная).
struct Stat {
    label: String,
    values: Vec<(String, String, bool)>,
    income: bool,
}

#[derive(Template)]
#[template(path = "transactions/list.html")]
struct ListPage {
    page: PageCtx,
    month_title: String,
    month: String,
    prev: String,
    next: String,
    params: ListParams,
    accounts: Vec<Choice>,
    kinds: Vec<(&'static str, String)>,
    days: Vec<DayGroup>,
    stats: Vec<Stat>,
    /// Выписка по счёту: (название счёта, валюта).
    statement: Option<(String, String)>,
    /// Включён фильтр (пустой список — «ничего не найдено», а не «операций нет»).
    filtered: bool,
    /// Для «+ Перевод» из выписки.
    account_query: String,
    /// Операций больше, чем показано (итоги — по всем).
    truncated: bool,
}

fn month_start(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").unwrap_or_else(|_| today().with_day(1).unwrap_or(today()))
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(params): Query<ListParams>) -> PageResult {
    let p = page.session.project_id;
    let lang = page.lang;
    let from = month_start(&params.month);
    let to = from.checked_add_months(Months::new(1)).and_then(|d| d.pred_opt()).unwrap_or(from);
    let account = Some(params.account.clone()).filter(|v| !v.is_empty());
    let query = || ListQuery {
        from: Some(from),
        to: Some(to),
        account_code: account.clone(),
        tx_kind: Some(params.kind.clone()).filter(|v| !v.is_empty()),
        created_by: Uuid::parse_str(&params.member).ok(),
        untranslated: (params.untranslated == "1").then(|| lang.code().to_string()),
    };
    // Месяц целиком; итоги и выписка — по всем операциям фильтра, даже если лента обрезана (техдолг #25).
    let feed = svc.feed(p, query()).await?;
    let items = feed.items;
    let cats: HashMap<String, String> = svc.category_titles(p, lang).await?.into_iter().collect();
    let accs: HashMap<String, String> = svc.account_titles(p, lang).await?.into_iter().collect();
    let accounts = svc.account_choices(p, lang).await?;
    let acc_title = |code: &str| accs.get(code).cloned().unwrap_or_else(|| code.to_string());
    let cat_title = |code: &str| cats.get(code).cloned().unwrap_or_else(|| code.to_string());

    // Выписка: остаток на начало месяца и нарастающий остаток — только без прочих фильтров.
    let only_account = params.kind.is_empty() && params.member.is_empty() && params.untranslated != "1";
    let mut running: HashMap<Uuid, Decimal> = HashMap::new();
    let stats: Vec<Stat>;
    let mut statement = None;
    if let (Some(acc), Some(st)) = (&account, feed.statement) {
        let currency = items
            .iter()
            .flat_map(|t| t.postings.iter())
            .find(|x| x.ref_kind == "ACCOUNT" && &x.ref_code == acc)
            .map(|x| x.currency_code.clone())
            .or_else(|| accounts.iter().find(|a| &a.code == acc).map(|a| a.tag.clone()))
            .unwrap_or_default();
        running.extend(st.running);
        let one = |label: &str, v: Decimal, income: bool| Stat {
            label: page_t(&page, label),
            values: vec![(money_in(lang, v), currency.clone(), v < Decimal::ZERO)],
            income,
        };
        stats = vec![
            one("balance_start", st.start, false),
            Stat { label: page_t(&page, "inflow"), values: vec![(signed(lang, st.inflow), currency.clone(), false)], income: true },
            Stat { label: page_t(&page, "outflow"), values: vec![(signed(lang, st.outflow), currency.clone(), false)], income: false },
            one("balance_end", st.end, false),
        ];
        statement = Some((acc_title(acc), currency));
    } else {
        let mut sums = feed.totals;
        if sums.is_empty() {
            sums.push((page.session.base_currency.clone(), Decimal::ZERO, Decimal::ZERO));
        }
        // Базовая валюта первой.
        sums.sort_by_key(|x| x.0 != page.session.base_currency);
        stats = vec![
            Stat { label: page_t(&page, "income_total"), values: sums.iter().map(|(c, i, _)| (signed(lang, *i), c.clone(), false)).collect(), income: true },
            Stat {
                label: page_t(&page, "expense_total"),
                values: sums.iter().map(|(c, _, e)| (signed(lang, -*e), c.clone(), false)).collect(),
                income: false,
            },
            Stat { label: page_t(&page, "net"), values: sums.iter().map(|(c, i, e)| (money_in(lang, i - e), c.clone(), i < e)).collect(), income: false },
        ];
    }

    let mut days: Vec<DayGroup> = Vec::new();
    let mut last: Option<NaiveDate> = None;
    for t in items {
        let author = page.session.member_name(&t.created_by);
        let mut notes = Vec::new();
        let (css, meta, desc_fallback) = match t.tx_kind.as_str() {
            "TRANSFER" => {
                let to = t.to_account_code.clone().unwrap_or_default();
                ("transfer", format!("{} → {} · {author}", acc_title(&t.account_code), acc_title(&to)), page_t(&page, "transfer"))
            }
            "OPENING" => ("opening", format!("{} · {author}", acc_title(&t.account_code)), page_t(&page, "opening")),
            "INCOME" => ("income", format!("{} · {author}", acc_title(&t.account_code)), String::new()),
            _ => ("expense", format!("{} · {author}", acc_title(&t.account_code)), String::new()),
        };
        let simple = t.items.len() == 1 && t.items[0].is_simple();
        let groups = if simple { Vec::new() } else { item_groups(lang, &t.items, &cat_title) };
        if t.refund {
            notes.push(page_t(&page, "refund"));
        }
        if let (Some(to_cur), Some(got)) = (&t.to_currency, t.to_amount) {
            notes.push(format!("{} {}", page_t(&page, "rate"), rate_text(lang, t.sent(), page.cur(&t.currency), got, page.cur(to_cur))));
        }
        if let (Some(fee), Some(cat)) = (t.fee_amount, &t.fee_category_code) {
            notes.push(format!("{} {}\u{a0}{} · {}", page_t(&page, "fee_short"), money_in(lang, fee), page.cur(&t.currency), cat_title(cat)));
        }
        let desc = match t.title(lang) {
            "" if !desc_fallback.is_empty() => desc_fallback,
            "" => t.items.first().map(|x| cat_title(&x.category_code)).unwrap_or_default(),
            s => s.to_string(),
        };
        // В выписке — как операция изменила этот счёт; иначе — сумма операции со знаком.
        let (value, currency, second) = match &account {
            Some(acc) => {
                let cur = t.postings.iter().find(|x| x.ref_kind == "ACCOUNT" && &x.ref_code == acc).map(|x| x.currency_code.clone()).unwrap_or_default();
                (t.amount_on(acc), cur, None)
            }
            None if t.is_transfer() => {
                let to = t.to_account_code.clone().unwrap_or_default();
                let second = t.to_currency.clone().filter(|c| *c != t.currency).map(|c| (signed(lang, t.amount_on(&to)), c));
                (-t.total, t.currency.clone(), second)
            }
            None => (t.amount_on(&t.account_code), t.currency.clone(), None),
        };
        let view = TxView {
            css,
            desc,
            missing: t.missing_tr(lang),
            meta,
            category: if simple { cat_title(&t.items[0].category_code) } else { String::new() },
            groups,
            amount: signed(lang, value),
            currency,
            positive: value > Decimal::ZERO && !t.is_transfer(),
            second,
            notes,
            running: if only_account { running.get(&t.id).map(|b| money_in(lang, *b)) } else { None },
            edit_href: if t.is_opening() {
                format!("/accounts/{}/edit", kernel::web::url_encode(&t.account_code))
            } else {
                format!("/transactions/{}/edit?back={}", t.id, kernel::web::url_encode(&page.back()))
            },
            dto: t,
        };
        if last != Some(view.dto.occurred_on) {
            last = Some(view.dto.occurred_on);
            days.push(DayGroup { label: day_label(lang, view.dto.occurred_on), items: Vec::new() });
        }
        if let Some(g) = days.last_mut() {
            g.items.push(view);
        }
    }
    let fmt = |d: NaiveDate| format!("{}-{:02}", d.year(), d.month());
    let filtered = account.is_some() || !params.kind.is_empty() || !params.member.is_empty() || params.untranslated == "1";
    let kinds = ["EXPENSE", "INCOME", "TRANSFER", "OPENING"].into_iter().map(|k| (k, page_t(&page, &format!("kinds.{k}")))).collect();
    render(&ListPage {
        truncated: feed.truncated,
        filtered,
        month_title: format!("{} {}", month_name(lang, from.month()), from.year()),
        month: fmt(from),
        prev: fmt(from.checked_sub_months(Months::new(1)).unwrap_or(from)),
        next: fmt(from.checked_add_months(Months::new(1)).unwrap_or(from)),
        account_query: account.map(|a| format!("&account={}", kernel::web::url_encode(&a))).unwrap_or_default(),
        params,
        accounts,
        kinds,
        days,
        stats,
        statement,
        page: ctx(page),
    })
}

/// Строка словаря фичи (до того, как `PageCtx` переключён на фичу).
fn page_t(page: &PageCtx, key: &str) -> String {
    kernel::i18n::t(page.lang, Some(crate::i18n::dict), key)
}

/// Число без лишних нулей и без разделителя тысяч: «2», «0,45» — количество и цена.
fn num(lang: Lang, v: Decimal) -> String {
    let s = v.normalize().to_string();
    if lang == Lang::Ru { s.replace('.', ",") } else { s }
}

/// Сумма без «,00»: «47», «12 345», «5,03».
fn money_short(lang: Lang, v: Decimal) -> String {
    let s = money_in(lang, v);
    s.strip_suffix(if lang == Lang::Ru { ",00" } else { ".00" }).map(str::to_string).unwrap_or(s)
}

/// Строка позиции в ленте: «Круассан · 2 шт × 25 − 3 → 47»; без цены — «Латте · 45».
fn item_text(lang: Lang, x: &ItemDto) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(name) = &x.name {
        parts.push(name.clone());
    }
    let unit = kernel::i18n::t(lang, Some(crate::i18n::dict), &format!("unit.{}", x.unit));
    let plain = x.quantity == Decimal::ONE && x.unit == "pcs" && x.discount.is_zero();
    let money = match x.price_per_unit {
        // 1 шт по цене без скидки — цена и есть сумма.
        Some(_) if plain => money_short(lang, x.amount),
        Some(price) => {
            let mut s = format!("{}\u{a0}{unit} × {}", num(lang, x.quantity), money_short(lang, price));
            if !x.discount.is_zero() {
                s.push_str(&format!(" − {}", money_short(lang, x.discount)));
            }
            format!("{s} → {}", money_short(lang, x.amount))
        }
        None if !x.discount.is_zero() => format!("{} − {} → {}", money_short(lang, x.amount + x.discount), money_short(lang, x.discount), money_short(lang, x.amount)),
        None if x.quantity != Decimal::ONE || x.unit != "pcs" => format!("{}\u{a0}{unit} → {}", num(lang, x.quantity), money_short(lang, x.amount)),
        None => money_short(lang, x.amount),
    };
    parts.push(money);
    parts.join(" · ")
}

/// Позиции по категориям в порядке первого появления в чеке; внутри — в порядке чека.
fn item_groups(lang: Lang, items: &[ItemDto], cat_title: &dyn Fn(&str) -> String) -> Vec<GroupView> {
    let mut order: Vec<&str> = Vec::new();
    for x in items {
        if !order.contains(&x.category_code.as_str()) {
            order.push(&x.category_code);
        }
    }
    order
        .into_iter()
        .map(|code| {
            let mine: Vec<&ItemDto> = items.iter().filter(|x| x.category_code == code).collect();
            let total: Decimal = mine.iter().map(|x| x.amount).sum();
            // Одна простая позиция в категории — достаточно строки категории.
            let lines = if mine.len() == 1 && mine[0].is_simple() { Vec::new() } else { mine.iter().map(|x| item_text(lang, x)).collect() };
            GroupView { category: cat_title(code), amount: money_in(lang, total), items: lines }
        })
        .collect()
}

// ---------------------------------------------------------------- форма

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TxForm {
    occurred_on: String,
    tx_kind: String,
    account_code: String,
    refund: String,
    to_account_code: String,
    amount: String,
    to_amount: String,
    fee_category_code: String,
    fee_amount: String,
    description: String,
    description_lang: String,
    description_ru: String,
    description_en: String,
    description_th: String,
    plan_id: String,
    plan_date: String,
    back: String,
    /// Скидка на чек (раздел 5.1): делится по позициям, не хранится — при правке поле пустое.
    discount_total: String,
    // Позиции — параллельные списки: каждая строка формы отправляет все поля, пустые тоже.
    item_id: Vec<String>,
    item_category: Vec<String>,
    item_amount: Vec<String>,
    item_name: Vec<String>,
    item_qty: Vec<String>,
    item_unit: Vec<String>,
    item_price: Vec<String>,
    item_discount: Vec<String>,
}

/// Предзаполнение формы (из календаря планов: «Оплатил»; со счёта: «Перевести»; из быстрого диалога).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Prefill {
    plan_id: String,
    plan_date: String,
    account: String,
    to_account: String,
    category: String,
    amount: String,
    kind: String,
    desc: String,
    desc_lang: String,
    /// Дата операции (из быстрого ввода: «Подробная карточка»).
    date: String,
    back: String,
}

/// Строка позиции в форме.
#[derive(Debug, Default, Clone)]
struct ItemForm {
    /// Номер строки с 1 (`items[n]` в ошибках).
    n: usize,
    /// Пустая строка в запасе.
    spare: bool,
    /// Поля с ошибкой и тексты ошибок строки.
    invalid: Vec<&'static str>,
    errors: Vec<String>,
    id: String,
    category: String,
    amount: String,
    name: String,
    qty: String,
    unit: String,
    price: String,
    discount: String,
}

impl ItemForm {
    fn is_empty(&self) -> bool {
        [&self.category, &self.amount, &self.name, &self.price].iter().all(|v| v.trim().is_empty())
    }

    /// «× цена» раскрыто: есть цена, количество не 1, единица не «шт» или скидка (раздел 3.2).
    fn open(&self) -> bool {
        let set = |v: &str, default: &str| !v.trim().is_empty() && v.trim() != default;
        !self.price.trim().is_empty() || set(&self.qty, "1") || set(&self.unit, "pcs") || set(&self.discount, "0") || self.bad("quantity") || self.bad("unit")
    }

    fn bad(&self, field: &str) -> bool {
        self.invalid.contains(&field)
    }
}

/// Поля позиции, к которым бывает ошибка, в порядке строки формы.
const ITEM_FIELDS: [&str; 7] = ["id", "category_code", "amount", "quantity", "unit", "price", "discount"];

/// Пустых строк позиций в запасе — для ввода чека без JS (с JS они спрятаны до «Добавить позицию»).
const SPARE_ITEMS: usize = 3;

/// Проводка для блока «Проводки» в карточке транзакции.
struct PostingView {
    title: String,
    system: bool,
    amount: String,
    currency: String,
}

#[derive(Template)]
#[template(path = "transactions/form.html")]
struct FormPage {
    page: PageCtx,
    id: Option<Uuid>,
    f: TxForm,
    /// Заполненные позиции, затем пустые в запасе.
    items: Vec<ItemForm>,
    units: Vec<(&'static str, String)>,
    accounts: Vec<Choice>,
    categories: Vec<Choice>,
    /// Курс 1 единицы валюты к базовой на дату формы (для курса справочника рядом с курсом перевода).
    rates: HashMap<String, String>,
    /// Знаков после запятой у валюты счёта — для суммы «количество × цена» в браузере.
    minor: HashMap<String, u32>,
    postings: Vec<PostingView>,
    error: Option<String>,
}

/// Поля формы, у которых свой текст ошибки рядом с полем.
const FIELDS: [&str; 11] = [
    "occurred_on", "account_code", "tx_kind", "description", "to_account_code", "amount", "to_amount", "fee_category_code", "fee_amount", "discount_total", "items",
];

impl FormPage {
    fn rate_of(&self, currency: &str) -> String {
        self.rates.get(currency).cloned().unwrap_or_default()
    }

    fn minor_of(&self, account: &str) -> u32 {
        self.minor.get(account).copied().unwrap_or(2)
    }

    /// Ошибка, не привязанная ни к одному полю формы, — общим блоком над формой.
    fn general_error(&self) -> Option<String> {
        let e = FormError::parse(self.error.as_deref()?);
        let known = FIELDS.contains(&e.field.as_str()) || e.field.starts_with("items[");
        if known { None } else { self.error.clone() }
    }

    /// Строка «Ещё» в свёрнутом виде: что внутри (раздел 3.2).
    fn more_summary(&self) -> String {
        self.page.t("discount_total").to_lowercase()
    }

    /// «Ещё» раскрыто: в нём есть введённое значение или ошибка.
    fn more_open(&self) -> bool {
        !self.f.discount_total.trim().is_empty() || kernel::web::field_error(&self.error, "discount_total").is_some()
    }
}

fn opt(v: &str) -> Option<String> {
    Some(v.trim().to_string()).filter(|v| !v.is_empty())
}

fn back_or_list(back: &str) -> String {
    // Только относительные пути этого сайта — без открытых редиректов.
    if back.starts_with('/') && !back.starts_with("//") { back.to_string() } else { "/transactions".to_string() }
}

/// Ошибка поля из словаря фичи на языке запроса.
fn invalid(field: &str, key: &str) -> AppError {
    Problem::validation(field, kernel::i18n::tr(Some(crate::i18n::dict), key, &[])).into()
}

impl TxForm {
    /// Заполненные строки позиций в порядке формы: пустые строки (запас) пропускаются, и номер позиции в
    /// ошибке — номер среди заполненных.
    fn items(&self) -> Vec<ItemForm> {
        let n = [self.item_category.len(), self.item_amount.len(), self.item_name.len(), self.item_price.len()].into_iter().max().unwrap_or(0);
        let at = |v: &Vec<String>, i: usize| v.get(i).cloned().unwrap_or_default();
        (0..n)
            .map(|i| ItemForm {
                id: at(&self.item_id, i),
                category: at(&self.item_category, i),
                amount: at(&self.item_amount, i),
                name: at(&self.item_name, i),
                qty: at(&self.item_qty, i),
                unit: at(&self.item_unit, i),
                price: at(&self.item_price, i),
                discount: at(&self.item_discount, i),
                ..Default::default()
            })
            .filter(|x| !x.is_empty())
            .collect()
    }

    fn kind(&self) -> &str {
        match self.tx_kind.as_str() {
            "INCOME" => "INCOME",
            "TRANSFER" => "TRANSFER",
            _ => "EXPENSE",
        }
    }

    fn to_input(&self) -> Result<TransactionInput, AppError> {
        let v = |m: String| AppError::Validation(m);
        let money = |field: &str, s: &str| opt(s).map(|s| parse_money(field, &s)).transpose().map_err(v);
        let transfer = self.kind() == "TRANSFER";
        let mut items = Vec::new();
        if !transfer {
            for (n, x) in self.items().into_iter().enumerate() {
                let f = |name: &str| format!("items[{}].{name}", n + 1);
                let price = money(&f("price"), &x.price)?;
                items.push(ItemInput {
                    id: opt(&x.id).map(|s| Uuid::parse_str(&s)).transpose().map_err(|_| invalid(&f("id"), "err.bad_id"))?,
                    category_code: opt(&x.category),
                    name: opt(&x.name),
                    quantity: money(&f("quantity"), &x.qty)?,
                    unit: opt(&x.unit),
                    // С ценой сумму считает сервер (раздел 3.2): поле суммы в форме только для чтения.
                    amount: if price.is_some() { None } else { money(&f("amount"), &x.amount)? },
                    price_per_unit: price,
                    discount: money(&f("discount"), &x.discount)?,
                    note: None,
                });
            }
        }
        Ok(TransactionInput {
            occurred_on: self.occurred_on.parse().map_err(|_| invalid("occurred_on", "err.bad_date"))?,
            tx_kind: self.kind().to_string(),
            account_code: self.account_code.clone(),
            refund: !transfer && self.refund == "1",
            items,
            replace_items: true,
            discount_total: if transfer { None } else { money("discount_total", &self.discount_total)? },
            to_account_code: if transfer { opt(&self.to_account_code) } else { None },
            amount: if transfer { money("amount", &self.amount)? } else { None },
            to_amount: if transfer { money("to_amount", &self.to_amount)? } else { None },
            fee_category_code: if transfer { opt(&self.fee_category_code) } else { None },
            fee_amount: if transfer { money("fee_amount", &self.fee_amount)? } else { None },
            description: opt(&self.description),
            description_lang: self.description_lang.clone(),
            description_ru: opt(&self.description_ru),
            description_en: opt(&self.description_en),
            description_th: opt(&self.description_th),
            plan_id: opt(&self.plan_id).map(|s| Uuid::parse_str(&s)).transpose().map_err(|_| invalid("plan_id", "err.bad_id"))?,
            plan_date: opt(&self.plan_date).map(|s| s.parse()).transpose().map_err(|_| invalid("plan_date", "err.bad_date"))?,
            ..Default::default()
        })
    }

    fn from_dto(t: &TransactionDto) -> Self {
        let num = |d: Option<Decimal>| d.map(|d| d.normalize().to_string()).unwrap_or_default();
        let col = |f: &dyn Fn(&ItemDto) -> String| t.items.iter().map(f).collect::<Vec<String>>();
        Self {
            occurred_on: t.occurred_on.to_string(),
            tx_kind: t.tx_kind.clone(),
            account_code: t.account_code.clone(),
            refund: if t.refund { "1".into() } else { String::new() },
            to_account_code: t.to_account_code.clone().unwrap_or_default(),
            amount: if t.is_transfer() { num(Some(t.total)) } else { String::new() },
            to_amount: num(t.to_amount),
            fee_category_code: t.fee_category_code.clone().unwrap_or_default(),
            fee_amount: num(t.fee_amount),
            description: t.description.clone().unwrap_or_default(),
            description_lang: t.description_lang.clone(),
            description_ru: t.description_ru.clone().unwrap_or_default(),
            description_en: t.description_en.clone().unwrap_or_default(),
            description_th: t.description_th.clone().unwrap_or_default(),
            plan_id: t.plan_id.map(|u| u.to_string()).unwrap_or_default(),
            plan_date: t.plan_date.map(|d| d.to_string()).unwrap_or_default(),
            back: String::new(),
            discount_total: String::new(),
            item_id: col(&|x| x.id.to_string()),
            item_category: col(&|x| x.category_code.clone()),
            item_amount: col(&|x| x.amount.normalize().to_string()),
            item_name: col(&|x| x.name.clone().unwrap_or_default()),
            item_qty: col(&|x| x.quantity.normalize().to_string()),
            item_unit: col(&|x| x.unit.clone()),
            item_price: col(&|x| num(x.price_per_unit)),
            item_discount: col(&|x| x.discount.normalize().to_string()),
        }
    }
}

async fn form_page(svc: &TransactionService, page: PageCtx, existing: Option<&TransactionDto>, f: TxForm, error: Option<String>) -> PageResult {
    let p = page.session.project_id;
    let lang = page.lang;
    let accounts = svc.account_choices(p, lang).await?;
    let categories = svc.category_choices(p, lang).await?;
    let date = f.occurred_on.parse::<NaiveDate>().unwrap_or_else(|_| today());
    let base = page.session.base_currency.clone();
    let mut rates: HashMap<String, String> = svc.rates(p, &base, date).await?.into_iter().map(|(c, r)| (c, r.normalize().to_string())).collect();
    rates.insert(base, "1".into());
    let postings = match existing {
        None => Vec::new(),
        Some(t) => {
            let accs: HashMap<String, String> = svc.account_titles(p, lang).await?.into_iter().collect();
            let cats: HashMap<String, String> = svc.category_titles(p, lang).await?.into_iter().collect();
            let page = ctx(page.clone());
            t.postings
                .iter()
                .map(|x| PostingView {
                    title: match x.ref_kind.as_str() {
                        "ACCOUNT" => accs.get(&x.ref_code).cloned().unwrap_or_else(|| x.ref_code.clone()),
                        "CATEGORY" => cats.get(&x.ref_code).cloned().unwrap_or_else(|| x.ref_code.clone()),
                        _ => system_title(&page, &x.ref_code),
                    },
                    system: x.ref_kind == "SYSTEM",
                    amount: signed(lang, x.amount),
                    currency: x.currency_code.clone(),
                })
                .collect()
        }
    };
    let mut items = f.items();
    items.extend(std::iter::repeat_n(ItemForm { spare: true, ..Default::default() }, SPARE_ITEMS));
    for (i, x) in items.iter_mut().enumerate() {
        x.n = i + 1;
        for name in ITEM_FIELDS {
            if let Some(e) = kernel::web::field_error(&error, &format!("items[{}].{name}", x.n)) {
                x.invalid.push(name);
                x.errors.push(e);
            }
        }
    }
    let units = Unit::ALL.into_iter().map(|u| (u.as_str(), page_t(&page, &format!("unit.{}", u.as_str())))).collect();
    let minor = svc.minor_units(p).await?.into_iter().collect();
    render(&FormPage { page: nested(page), id: existing.map(|t| t.id), f, items, units, accounts, categories, rates, minor, postings, error })
}

async fn new_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(q): Query<Prefill>) -> PageResult {
    let date = q.date.parse::<NaiveDate>().unwrap_or_else(|_| today());
    let kind = match q.kind.as_str() {
        "INCOME" | "TRANSFER" => q.kind.clone(),
        _ => "EXPENSE".into(),
    };
    let transfer = kind == "TRANSFER";
    let f = TxForm {
        occurred_on: date.to_string(),
        account_code: q.account,
        to_account_code: q.to_account,
        amount: if transfer { q.amount.clone() } else { String::new() },
        tx_kind: kind,
        description: q.desc,
        description_lang: Lang::parse(&q.desc_lang).unwrap_or(page.lang).upper().into(),
        plan_id: q.plan_id,
        plan_date: q.plan_date,
        back: q.back,
        item_category: vec![q.category],
        item_amount: vec![if transfer { String::new() } else { q.amount }],
        ..Default::default()
    };
    form_page(&svc, page, None, f, None).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<TxForm>) -> PageResult {
    let s = page.session.scope();
    let result = async { svc.create(s.project_id, s.user_id, f.to_input()?).await }.await;
    match result {
        Ok(_) => Ok(see_other_flash(&page.config, &back_or_list(&f.back), Flash::ok(page.t("toast.saved")))),
        Err(e @ (AppError::Validation(_) | AppError::Invalid(_) | AppError::Conflict(_))) => form_page(&svc, page, None, f, Some(e.to_string())).await,
        Err(e) => Err(PageError(e)),
    }
}

async fn edit_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let t = svc.get(page.session.project_id, id).await?;
    // Начальный остаток ведётся из формы счёта.
    if t.is_opening() {
        return Ok(see_other(&format!("/accounts/{}/edit", kernel::web::url_encode(&t.account_code))));
    }
    let f = TxForm::from_dto(&t);
    form_page(&svc, page, Some(&t), f, None).await
}

async fn update(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<TxForm>) -> PageResult {
    let p = page.session.project_id;
    let result = async { svc.update(p, id, f.to_input()?).await }.await;
    match result {
        Ok(_) => Ok(see_other_flash(&page.config, &back_or_list(&f.back), Flash::ok(page.t("toast.saved")))),
        Err(e @ (AppError::Validation(_) | AppError::Invalid(_) | AppError::Conflict(_))) => {
            let t = svc.get(p, id).await?;
            form_page(&svc, page, Some(&t), f, Some(e.to_string())).await
        }
        Err(e) => Err(PageError(e)),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BackForm {
    back: String,
}

/// Удаление без «Вы уверены?»: тост с «Отменить» на 5 секунд. Удаление оплаты плана возвращает
/// вхождение в «ожидается», «Отменить» — снова в «оплачено».
async fn delete(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<BackForm>) -> PageResult {
    let token = svc.delete_with_undo(page.session.project_id, id).await?;
    let back = back_or_list(&f.back);
    let undo = format!("/transactions/undo/{token}?back={}", kernel::web::url_encode(&back));
    Ok(see_other_flash(&page.config, &back, Flash::with_undo(page.t("toast.deleted"), undo)))
}

async fn undo(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(token): Path<Uuid>, Query(q): Query<BackForm>) -> PageResult {
    let flash = match svc.undo_delete(page.session.project_id, token).await {
        Ok(_) => Flash::ok(page.t("toast.restored")),
        Err(AppError::NotFound(_)) => Flash::error(page.t("toast.undo_expired")),
        Err(AppError::Conflict(m)) => Flash::error(m),
        Err(e) => return Err(PageError(e)),
    };
    Ok(see_other_flash(&page.config, &back_or_list(&q.back), flash))
}

// ---------------------------------------------------------------- сверка остатка

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ReconcileForm {
    account: String,
    occurred_on: String,
    actual: String,
    category_code: String,
}

#[derive(Template)]
#[template(path = "transactions/reconcile.html")]
struct ReconcilePage {
    page: PageCtx,
    f: ReconcileForm,
    account_title: String,
    currency: String,
    /// Остаток по проводкам на дату формы.
    book: String,
    book_raw: String,
    categories: Vec<Choice>,
    error: Option<String>,
}

async fn reconcile_page(svc: &TransactionService, page: PageCtx, f: ReconcileForm, error: Option<String>) -> PageResult {
    let p = page.session.project_id;
    let lang = page.lang;
    let accounts = svc.account_choices(p, lang).await?;
    let acc = accounts
        .iter()
        .find(|a| a.code == f.account)
        .ok_or_else(|| AppError::NotFound(kernel::i18n::tr(Some(crate::i18n::dict), "err.account_not_found", &[("code", &f.account)])))?;
    let date = f.occurred_on.parse::<NaiveDate>().unwrap_or_else(|_| today());
    let book = svc.balance(p, &acc.code, date).await?;
    let categories = svc.category_choices(p, lang).await?;
    let page = ctx(page);
    let title = page.t("nav_accounts");
    render(&ReconcilePage {
        account_title: acc.label.clone(),
        currency: acc.tag.clone(),
        book: money_in(lang, book),
        book_raw: book.normalize().to_string(),
        categories,
        f,
        error,
        page: page.crumb(title, "/accounts"),
    })
}

async fn reconcile_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Query(mut f): Query<ReconcileForm>) -> PageResult {
    if f.occurred_on.is_empty() {
        f.occurred_on = today().to_string();
    }
    reconcile_page(&svc, page, f, None).await
}

async fn reconcile(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<ReconcileForm>) -> axum::response::Response {
    let s = page.session.scope();
    let lang = page.lang;
    let result = async {
        let date: NaiveDate = f.occurred_on.parse().map_err(|_| invalid("occurred_on", "err.bad_date"))?;
        let actual = parse_money("actual", &f.actual).map_err(AppError::Validation)?;
        let text = crate::i18n::dict("reconcile_desc").map(|t| lang.pick(t).to_string());
        let description = Description { text, lang, ..Default::default() };
        svc.reconcile(s.project_id, s.user_id, &f.account, actual, date, &f.category_code, description).await
    }
    .await;
    let back = format!("/transactions?account={}&month={}", kernel::web::url_encode(&f.account), f.occurred_on.get(..7).unwrap_or_default());
    match result {
        Ok(Some(_)) => see_other_flash(&page.config, &back, Flash::ok(page_t(&page, "reconcile_done"))),
        Ok(None) => see_other_flash(&page.config, &back, Flash::ok(page_t(&page, "reconcile_same"))),
        Err(e @ (AppError::Validation(_) | AppError::Invalid(_))) => reconcile_page(&svc, page, f, Some(e.to_string())).await.into_response(),
        Err(e) => PageError(e).into_response(),
    }
}

// ---------------------------------------------------------------- перевод описания

#[derive(Template)]
#[template(path = "transactions/translate.html")]
struct TranslatePage {
    page: PageCtx,
    t: TransactionDto,
    /// Поля переводов: (RU/EN/TH, имя поля, значение) — кроме языка оригинала.
    fields: Vec<(&'static str, &'static str, String)>,
    error: Option<String>,
}

fn translate_page(page: PageCtx, t: TransactionDto, error: Option<String>) -> PageResult {
    let fields = [("RU", "description_ru", &t.description_ru), ("EN", "description_en", &t.description_en), ("TH", "description_th", &t.description_th)]
        .into_iter()
        .filter(|(code, _, _)| *code != t.description_lang)
        .map(|(code, name, v)| (code, name, v.clone().unwrap_or_default()))
        .collect();
    render(&TranslatePage { page: nested(page), t, fields, error })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TranslateForm {
    description_ru: String,
    description_en: String,
    description_th: String,
}

async fn translate_form(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let t = svc.get(page.session.project_id, id).await?;
    translate_page(page, t, None)
}

async fn translate(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>, Form(f): Form<TranslateForm>) -> PageResult {
    let input = TranslationInput { description_ru: opt(&f.description_ru), description_en: opt(&f.description_en), description_th: opt(&f.description_th) };
    match svc.translate(page.session.project_id, id, input).await {
        Ok(_) => Ok(see_other_flash(&page.config, "/transactions", Flash::ok(page.t("toast.saved")))),
        Err(e @ (AppError::Validation(_) | AppError::Invalid(_))) => {
            let t = svc.get(page.session.project_id, id).await?;
            translate_page(page, t, Some(e.to_string()))
        }
        Err(e) => Err(PageError(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn form(rows: &[[&str; 8]]) -> TxForm {
        let col = |i: usize| rows.iter().map(|r| r[i].to_string()).collect::<Vec<_>>();
        TxForm {
            occurred_on: "2026-10-18".into(),
            tx_kind: "EXPENSE".into(),
            account_code: "CARD".into(),
            description_lang: "RU".into(),
            item_id: col(0),
            item_category: col(1),
            item_amount: col(2),
            item_name: col(3),
            item_qty: col(4),
            item_unit: col(5),
            item_price: col(6),
            item_discount: col(7),
            ..Default::default()
        }
    }

    /// Форма без JS: пустые строки в запасе пропускаются, с ценой сумму считает сервер, чек — целиком.
    #[test]
    fn form_items_to_input() {
        let id = Uuid::new_v4().to_string();
        let f = form(&[
            [id.as_str(), "FOOD", "14", "Вода", "1", "pcs", "", "0"],
            ["", "", "", "", "", "pcs", "", ""],
            ["", "SNACKS", "50", "Круассан", "2", "pcs", "25", "3"],
            ["", "", "", "", "1", "pcs", "", "0"],
        ]);
        let input = f.to_input().unwrap();
        assert!(input.replace_items);
        assert_eq!(input.items.len(), 2);
        assert_eq!((input.items[0].id.map(|u| u.to_string()), input.items[0].amount), (Some(id), Some(Decimal::from(14))));
        let x = &input.items[1];
        assert_eq!((x.amount, x.price_per_unit, x.quantity, x.discount, x.name.as_deref()), (None, Some(Decimal::from(25)), Some(Decimal::from(2)), Some(Decimal::from(3)), Some("Круассан")));
        let bad = form(&[["nope", "FOOD", "1", "", "", "pcs", "", ""]]).to_input().unwrap_err();
        assert!(matches!(bad, AppError::Invalid(p) if p.field == "items[1].id"));
        let bad = form(&[["", "FOOD", "abc", "", "", "pcs", "", ""]]).to_input().unwrap_err();
        assert!(bad.to_string().starts_with("items[1].amount: "));
    }

    /// Строка позиции в ленте (раздел 3.3).
    #[test]
    fn item_line_in_feed() {
        let d = |s: &str| Decimal::from_str(s).unwrap();
        let item = |name: Option<&str>, qty: &str, unit: &str, price: Option<&str>, discount: &str, amount: &str| ItemDto {
            id: Uuid::nil(),
            category_code: "FOOD".into(),
            name: name.map(Into::into),
            quantity: d(qty),
            unit: unit.into(),
            price_per_unit: price.map(d),
            discount: d(discount),
            amount: d(amount),
            note: None,
        };
        let ru = Lang::Ru;
        assert_eq!(item_text(ru, &item(Some("Круассан"), "2", "pcs", Some("25"), "3", "47")), "Круассан · 2\u{a0}шт × 25 − 3 → 47");
        assert_eq!(item_text(ru, &item(Some("Латте"), "1", "pcs", None, "0", "45")), "Латте · 45");
        assert_eq!(item_text(ru, &item(Some("Бананы"), "0.45", "kg", Some("60"), "0", "27")), "Бананы · 0,45\u{a0}кг × 60 → 27");
        assert_eq!(item_text(Lang::En, &item(None, "1", "pcs", None, "4.5", "40.5")), "45 − 4.50 → 40.50");
        assert_eq!(item_text(Lang::Th, &item(Some("น้ำ"), "3", "l", None, "0", "30")), "น้ำ · 3\u{a0}ล. → 30");
        assert_eq!(item_text(ru, &item(Some("Вода"), "1", "pcs", Some("14"), "0", "14")), "Вода · 14", "1 шт без скидки — только сумма");
    }

    #[test]
    fn rate_is_shown_above_one() {
        let d = |s: &str| Decimal::from_str(s).unwrap();
        assert_eq!(rate_text(Lang::Ru, d("10000"), "RUB", d("3912.40"), "THB"), "1 THB = 2,556 RUB");
        assert_eq!(rate_text(Lang::En, d("100"), "USD", d("3325"), "THB"), "1 USD = 33.25 THB");
    }
}
