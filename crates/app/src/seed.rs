//! Демо-данные. Всё создаётся через сервисы фич — те же проверки, что у пользователя в интерфейсе.
//! Даты — относительно текущего месяца.
//!
//! - `SEED_DEMO=1` (разработка): пользователи Алексей и Anna с личными проектами + семейный проект
//!   «Семья». Войти под ними можно через «Вход для разработки» (без Rauthy, только `APP_ENV=dev`).
//! - `GUEST_DEMO=1`: гость (`demo:guest`) — VIEWER в демо-семье; «Посмотреть демо» на лендинге.
//!   Демо-семья пересоздаётся раз в сутки ([`reset_family`]) — гости не видят чужих правок.
//!
//! Пользователи демо входят только по `sub` вида `demo:*` — Rauthy таких не выдаёт.

use std::str::FromStr;
use std::sync::Arc;

use chrono::{Datelike, Months, NaiveDate, NaiveTime, TimeZone};
use kernel::auth::{Memberships, RequestMeta, Role};
use kernel::error::AppResult;
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::wiring::wire_projects::{ensure_currency, localized};

pub struct Services {
    pub projects: Arc<projects::Service>,
    pub currencies: Arc<currencies::Service>,
    pub rates: Arc<rates::Service>,
    pub accounts: Arc<accounts::Service>,
    pub places: Arc<places::Service>,
    pub people: Arc<people::Service>,
    pub transactions: Arc<transactions::Service>,
    pub limits: Arc<limits::Service>,
    pub planning: Arc<planning::Service>,
}

const ALEKSEY: (&str, &str, &str) = ("demo:aleksey", "aleksey@demo.local", "Алексей");
const ANNA: (&str, &str, &str) = ("demo:anna", "anna@demo.local", "Anna");
const GUEST: (&str, &str, &str) = (projects::GUEST_SUB, "guest@demo.local", "Гость · Guest");
const FAMILY: &str = "Семья";

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap_or_default()
}

/// День месяца с отступом `back` месяцев назад; 31-е в коротком месяце → последний день.
fn day(back: u32, d: u32) -> NaiveDate {
    let today = chrono::Local::now().date_naive();
    let first = today.with_day(1).unwrap_or(today).checked_sub_months(Months::new(back)).unwrap_or(today);
    first.checked_add_days(chrono::Days::new((d - 1) as u64)).filter(|x| x.month() == first.month()).unwrap_or(first)
}

/// Время демо-операции: у трат, где оно очевидно, — своё (завтрак утром, ужин вечером), у остальных — с 08:00 до
/// 21:59 по ключу (описанию), одно и то же при каждом пересоздании демо. Ещё не наступившее время — `None`: операция
/// за сегодня получит «сейчас».
fn demo_time(date: NaiveDate, key: &str) -> Option<String> {
    const KNOWN: [(&str, u32); 5] = [
        ("Завтрак", 8 * 60 + 40),
        ("Grab", 8 * 60 + 55),
        ("ร้านกาแฟ", 10 * 60 + 20),
        ("Ужин", 19 * 60 + 45),
        ("Кино", 20 * 60 + 15),
    ];
    let h = key.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(u32::from(b)));
    let known = KNOWN.iter().find(|k| key.starts_with(k.0)).map(|k| k.1);
    let minutes = known.unwrap_or(8 * 60 + h % (14 * 60));
    let at = chrono::Local.from_local_datetime(&date.and_time(NaiveTime::from_hms_opt(minutes / 60, minutes % 60, 0)?));
    at.earliest().filter(|t| *t < chrono::Local::now()).map(|t| t.to_rfc3339())
}

/// Демо-пользователи и их личные проекты (`SEED_DEMO=1`), демо-семья с гостем (`GUEST_DEMO=1`).
pub async fn run(s: &Services, personal: bool, guest: bool) -> anyhow::Result<()> {
    if !personal && !guest {
        return Ok(());
    }
    let p = &s.projects;
    let me = p.ensure_user(ALEKSEY.0, ALEKSEY.1, ALEKSEY.2, Lang::Ru).await.map_err(err)?;
    let anna = p.ensure_user(ANNA.0, ANNA.1, ANNA.2, Lang::Th).await.map_err(err)?;
    // Личные проекты — только новым демо-пользователям (у перенесённых из старой базы они уже есть).
    let fresh = p.projects_of(me).await.map_err(err)?.is_empty();
    if personal && fresh {
        println!("Демо-данные: пользователи {} и {} с личными проектами…", ALEKSEY.1, ANNA.1);
        p.onboard(me, ALEKSEY.2, "THB", Lang::Ru).await.map_err(err)?;
        p.onboard(anna, ANNA.2, "THB", Lang::Th).await.map_err(err)?;
        fill_personal(s, me).await.map_err(err)?;
    }
    let family = p
        .projects_of(me)
        .await
        .map_err(err)?
        .into_iter()
        .find(|(_, k, n)| *k == projects::ProjectKind::Family && n == FAMILY);
    match family {
        None => {
            println!("Демо-данные: семейный проект «{FAMILY}»…");
            create_family(s, me, anna, guest).await.map_err(err)?;
        }
        // База до обновления: семья уже есть — гостя добавить к ней.
        Some((id, _, _)) if guest && !p.has_guest(id).await.map_err(err)? => {
            p.ensure_user(GUEST.0, GUEST.1, GUEST.2, Lang::En).await.map_err(err)?;
            p.add_guest(me, id).await.map_err(err)?;
        }
        Some(_) => {}
    }
    Ok(())
}

/// Сброс демо для гостей (раз в сутки): семья удаляется со всеми данными и создаётся заново.
pub async fn reset_family(s: &Services) -> anyhow::Result<()> {
    let p = &s.projects;
    let (Some(me), Some(anna)) =
        (p.user_by_sub(ALEKSEY.0).await.map_err(err)?, p.user_by_sub(ANNA.0).await.map_err(err)?)
    else {
        return Ok(());
    };
    for (id, kind, name) in p.projects_of(me).await.map_err(err)? {
        if kind == projects::ProjectKind::Family && name == FAMILY {
            p.purge_project(id).await.map_err(err)?;
        }
    }
    create_family(s, me, anna, true).await.map_err(err)?;
    println!("Демо для гостей: проект «{FAMILY}» пересоздан");
    Ok(())
}

async fn create_family(s: &Services, me: Uuid, anna: Uuid, guest: bool) -> AppResult<()> {
    let meta = RequestMeta::default();
    let p = &s.projects;
    let family = p.create_project(me, FAMILY, projects::ProjectKind::Family, "THB", &meta).await?;
    // Анна входит в семью так же, как настоящие участники, — по приглашению.
    let invite = p.create_invite(me, family, Role::Editor, &meta).await?;
    p.accept_invite(anna, &p.invite_code(invite), &meta).await?;
    if guest {
        let g = p.ensure_user(GUEST.0, GUEST.1, GUEST.2, Lang::En).await?;
        // Гость — только наблюдатель; личного проекта у него нет.
        let _ = g;
        p.add_guest(me, family).await?;
    }
    fill_family(s, family, me, anna).await
}

fn err(e: kernel::error::AppError) -> anyhow::Error {
    anyhow::anyhow!("seed: {e}")
}

/// Счёт и его начальный остаток (транзакция `OPENING` на начало демо-периода).
/// Стартовый счёт проекта (`CASH`, см. `wire_projects::DEFAULT_ACCOUNT`) уже есть — он обновляется.
#[allow(clippy::too_many_arguments)]
async fn account(
    s: &Services,
    p: Uuid,
    by: Uuid,
    code: &str,
    kind: &str,
    cur: &str,
    opening: &str,
    holder: Option<Uuid>,
    lang: Lang,
    names: (&str, &str, &str),
) -> AppResult<()> {
    let (name, name_lang, name_ru, name_en, name_th) = localized(lang, names.0, names.1, names.2);
    if s.accounts.exists(p, code).await? {
        s.accounts
            .update(
                p,
                code,
                accounts::dto::UpdateAccountDto {
                    name,
                    archived: false,
                    name_lang,
                    name_ru,
                    name_en,
                    name_th,
                    account_kind: kind.into(),
                    currency_code: cur.into(),
                    holder_user_id: holder,
                },
            )
            .await?;
    } else {
        s.accounts
            .create(
                p,
                accounts::dto::CreateAccountDto {
                    code: code.into(),
                    name,
                    name_lang,
                    name_ru,
                    name_en,
                    name_th,
                    account_kind: kind.into(),
                    currency_code: cur.into(),
                    holder_user_id: holder,
                },
            )
            .await?;
    }
    s.accounts.set_opening(p, by, code, dec(opening), day(3, 1)).await
}

struct Tx<'a> {
    date: NaiveDate,
    account: &'a str,
    kind: &'a str,
    desc: Option<(&'a str, Lang)>,
    /// (ru, en, th) — переводы описания, которые есть сразу
    tr: (Option<&'a str>, Option<&'a str>, Option<&'a str>),
    lines: Vec<(&'a str, &'a str, Option<&'a str>)>,
    plan: Option<(Uuid, NaiveDate)>,
    /// Место (справочник мест, шаг 5)
    place: Option<&'a str>,
    by: Uuid,
}

async fn tx(s: &Services, p: Uuid, t: Tx<'_>) -> AppResult<()> {
    tx_for(s, p, t, &[]).await
}

/// Операция с получателями позиций (шаг 6): `who[i]` — для кого `lines[i]`, пусто — для всех.
async fn tx_for(s: &Services, p: Uuid, t: Tx<'_>, who: &[Option<&str>]) -> AppResult<()> {
    if t.date > chrono::Local::now().date_naive() {
        return Ok(());
    }
    let key = format!("{}{:?}", t.desc.map(|d| d.0).unwrap_or_default(), t.lines.first());
    let input = transactions::dto::TransactionInput {
        occurred_on: t.date,
        occurred_at: demo_time(t.date, &key),
        account_code: t.account.into(),
        tx_kind: t.kind.into(),
        refund: false,
        to_account_code: None,
        amount: None,
        to_amount: None,
        fee_category_code: None,
        fee_amount: None,
        description: t.desc.map(|d| d.0.to_string()),
        description_lang: t.desc.map(|d| d.1).unwrap_or(Lang::Ru).upper().into(),
        description_ru: t.tr.0.map(Into::into),
        description_en: t.tr.1.map(Into::into),
        description_th: t.tr.2.map(Into::into),
        plan_id: t.plan.map(|x| x.0),
        plan_date: t.plan.map(|x| x.1),
        place_code: t.place.map(Into::into),
        items: t
            .lines
            .into_iter()
            .enumerate()
            .map(|(i, (c, a, n))| transactions::dto::ItemInput {
                category_code: Some(c.into()),
                amount: Some(dec(a)),
                name: n.map(Into::into),
                for_person_code: who.get(i).copied().flatten().map(Into::into),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    s.transactions.create(p, t.by, input).await.map(|_| ())
}

/// Чек с ценами (группа 1, шаг 2): позиции (категория, название, количество, единица, цена, скидка).
/// Названия позиций — данные пользователя, без переводов.
#[allow(clippy::too_many_arguments)]
async fn receipt(
    s: &Services,
    p: Uuid,
    date: NaiveDate,
    place: &str,
    desc: (&str, Lang),
    tr: (Option<&str>, Option<&str>, Option<&str>),
    items: &[(&str, &str, &str, &str, &str, &str)],
    by: Uuid,
) -> AppResult<()> {
    if date > chrono::Local::now().date_naive() {
        return Ok(());
    }
    let input = transactions::dto::TransactionInput {
        occurred_on: date,
        occurred_at: demo_time(date, desc.0),
        account_code: "CARD_KBANK".into(),
        tx_kind: "EXPENSE".into(),
        place_code: Some(place.into()),
        description: Some(desc.0.into()),
        description_lang: desc.1.upper().into(),
        description_ru: tr.0.map(Into::into),
        description_en: tr.1.map(Into::into),
        description_th: tr.2.map(Into::into),
        items: items
            .iter()
            .map(|(c, n, q, u, price, discount)| transactions::dto::ItemInput {
                category_code: Some((*c).into()),
                name: Some((*n).into()),
                quantity: Some(dec(q)),
                unit: Some((*u).into()),
                price_per_unit: Some(dec(price)),
                discount: Some(dec(discount)),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    s.transactions.create(p, by, input).await.map(|_| ())
}

/// Перевод между счетами: `to_amount` — при обмене валют, `fee` — комиссия (категория, сумма).
struct Transfer<'a> {
    date: NaiveDate,
    from: &'a str,
    to: &'a str,
    amount: &'a str,
    to_amount: Option<&'a str>,
    fee: Option<(&'a str, &'a str)>,
    desc: (&'a str, Lang),
    plan: Option<(Uuid, NaiveDate)>,
    by: Uuid,
}

async fn transfer(s: &Services, p: Uuid, t: Transfer<'_>) -> AppResult<()> {
    if t.date > chrono::Local::now().date_naive() {
        return Ok(());
    }
    let input = transactions::dto::TransactionInput {
        occurred_on: t.date,
        occurred_at: demo_time(t.date, t.desc.0),
        tx_kind: "TRANSFER".into(),
        account_code: t.from.into(),
        refund: false,
        lines: Vec::new(),
        to_account_code: Some(t.to.into()),
        amount: Some(dec(t.amount)),
        to_amount: t.to_amount.map(dec),
        fee_category_code: t.fee.map(|f| f.0.into()),
        fee_amount: t.fee.map(|f| dec(f.1)),
        description: Some(t.desc.0.into()),
        description_lang: t.desc.1.upper().into(),
        description_ru: None,
        description_en: None,
        description_th: None,
        plan_id: t.plan.map(|x| x.0),
        plan_date: t.plan.map(|x| x.1),
        ..Default::default()
    };
    s.transactions.create(p, t.by, input).await.map(|_| ())
}

#[allow(clippy::too_many_arguments)]
async fn plan(
    s: &Services,
    p: Uuid,
    by: Uuid,
    lang: Lang,
    names: (&str, &str, &str),
    kind: &str,
    acc: &str,
    cat: &str,
    amount: &str,
    rec: &str,
    start: NaiveDate,
) -> AppResult<Uuid> {
    let (name, name_lang, name_ru, name_en, name_th) = localized(lang, names.0, names.1, names.2);
    let input = planning::dto::PlanInput {
        name,
        name_lang,
        name_ru,
        name_en,
        name_th,
        tx_kind: kind.into(),
        account_code: acc.into(),
        category_code: (kind != "TRANSFER").then(|| cat.into()),
        to_account_code: (kind == "TRANSFER").then(|| cat.into()),
        amount: dec(amount),
        recurrence: rec.into(),
        interval: 1,
        start_on: start,
        end_on: None,
        active: true,
    };
    Ok(s.planning.create(p, by, input).await?.id)
}

async fn limit(s: &Services, p: Uuid, cat: &str, period: &str, amount: &str, from: NaiveDate) -> AppResult<()> {
    s.limits
        .create(
            p,
            limits::dto::LimitInput {
                valid_from: from,
                category_code: cat.into(),
                limit_period: period.into(),
                amount: Some(dec(amount)),
                removed: false,
            },
        )
        .await
        .map(|_| ())
}

async fn fill_family(s: &Services, p: Uuid, me: Uuid, anna: Uuid) -> AppResult<()> {
    let ru = Lang::Ru;
    for c in ["USD", "RUB"] {
        ensure_currency(&s.currencies, p, c, ru).await?;
    }
    // Доллар — «1 USD = x THB», рубль — как котируют банки: «1 THB = x RUB».
    for (cur, rate, inverted, back) in
        [("USD", "33.80", false, 2), ("USD", "33.25", false, 0), ("RUB", "2.56", true, 2), ("RUB", "2.70", true, 0)]
    {
        s.rates
            .create(
                p,
                rates::dto::RateInput { rate_date: day(back, 1), currency_code: cur.into(), rate: dec(rate), inverted },
            )
            .await?;
    }
    account(s, p, me, "CASH", "CASH", "THB", "4500", None, ru, ("Наличные", "Cash", "เงินสด")).await?;
    account(s, p, me, "CARD_KBANK", "CARD", "THB", "62000", Some(me), ru, ("Карта KBank", "KBank card", "บัตรกสิกร"))
        .await?;
    // Счёт заведён Анной на тайском — переводы на ru/en есть, видно в справочнике.
    account(
        s,
        p,
        anna,
        "CARD_SCB",
        "CARD",
        "THB",
        "18000",
        Some(anna),
        Lang::Th,
        ("Карта SCB", "SCB card", "บัตรไทยพาณิชย์"),
    )
    .await?;
    account(
        s,
        p,
        me,
        "WISE_USD",
        "BANK",
        "USD",
        "850",
        Some(me),
        Lang::En,
        ("Wise доллары", "Wise USD", "Wise ดอลลาร์"),
    )
    .await?;
    // Рубли из России: каждый месяц меняются на баты через приложение (перевод с обменом и комиссией).
    account(
        s,
        p,
        me,
        "TINKOFF_RUB",
        "CARD",
        "RUB",
        "120000",
        Some(me),
        ru,
        ("Тинькофф рубли", "Tinkoff RUB", "Tinkoff รูเบิล"),
    )
    .await?;
    // Места (шаг 5): названия — как на вывеске, не переводятся; варианты написания — для поиска на любом языке.
    for (code, name, aliases, kind, category, account, location) in [
        (
            "SEVEN_ELEVEN",
            "7-Eleven",
            &["7-11", "เซเว่น อีเลฟเว่น", "Севен"][..],
            "SHOP",
            "FOOD_GROCERIES",
            "CARD_KBANK",
            None,
        ),
        (
            "CAFE_AMAZON",
            "Café Amazon",
            &["Кафе Амазон", "คาเฟ่ อเมซอน"][..],
            "CAFE",
            "FOOD_CAFE",
            "CASH",
            Some("On Nut"),
        ),
        ("TOPS", "Tops", &["ท็อปส์", "Топс"][..], "SHOP", "FOOD_GROCERIES", "CARD_SCB", None),
        ("BIG_C", "Big C", &["บิ๊กซี", "Биг Си"][..], "SHOP", "FOOD_GROCERIES", "CARD_SCB", None),
        (
            "ON_NUT_MARKET",
            "ตลาดอ่อนนุช",
            &["On Nut market", "Рынок Он Нут"][..],
            "MARKET",
            "FOOD_GROCERIES",
            "CASH",
            None,
        ),
        ("GRAB", "Grab", &["แกร็บ", "Грэб"][..], "SERVICE", "TRANSPORT_TAXI", "CASH", None),
    ] {
        let data = places::dto::PlaceFields {
            name: name.into(),
            parent_code: None,
            aliases: aliases.iter().map(|a| (*a).into()).collect(),
            default_category_code: Some(category.into()),
            default_account_code: Some(account.into()),
            place_kind: kind.into(),
            location: location.map(Into::into),
        };
        s.places.create(p, places::dto::CreatePlaceDto { code: code.into(), data }).await?;
    }
    // Дерево мест: конкретные магазины сети 7-Eleven берут у неё категорию и счёт.
    for (code, location) in [("SEVEN_ELEVEN_SUKHUMVIT_24", "Sukhumvit 24"), ("SEVEN_ELEVEN_ON_NUT", "On Nut")] {
        let data = places::dto::PlaceFields {
            name: "7-Eleven".into(),
            parent_code: Some("SEVEN_ELEVEN".into()),
            place_kind: "SHOP".into(),
            location: Some(location.into()),
            ..Default::default()
        };
        s.places.create(p, places::dto::CreatePlaceDto { code: code.into(), data }).await?;
    }
    // Люди (шаг 6): Алексей и Анна — участники (записи создались сами), дочь Пинпан — без аккаунта.
    let pinpan = people::dto::PersonFields {
        name: "Pinpan".into(), aliases: vec!["Пинпан".into(), "ปิ่นปั่น".into()]
    };
    s.people.create(p, people::dto::CreatePersonDto { code: "PINPAN".into(), data: pinpan }).await?;
    let anna_code = s.people.of_member(p, anna).await?.map(|x| x.code);

    let start = day(3, 1);
    for (cat, period, amount) in [
        ("HOME", "MONTH", "21000"),
        ("FOOD", "MONTH", "18000"),
        ("FOOD_CAFE", "MONTH", "6000"),
        ("TRANSPORT", "MONTH", "4000"),
        ("FUN", "MONTH", "2500"),
        ("HOUSEHOLD", "WEEK", "700"),
        ("HEALTH", "YEAR", "24000"),
    ] {
        limit(s, p, cat, period, amount, start).await?;
    }

    let rent = plan(
        s,
        p,
        me,
        ru,
        ("Аренда квартиры", "Apartment rent", "ค่าเช่าคอนโด"),
        "EXPENSE",
        "CARD_KBANK",
        "HOME_RENT",
        "15000",
        "MONTHLY",
        day(9, 5),
    )
    .await?;
    let power = plan(
        s,
        p,
        me,
        ru,
        ("Электричество", "Electricity", "ค่าไฟ"),
        "EXPENSE",
        "CARD_KBANK",
        "HOME_ELECTRICITY",
        "1800",
        "MONTHLY",
        day(9, 20),
    )
    .await?;
    let water = plan(
        s,
        p,
        anna,
        Lang::Th,
        ("Вода", "Water", "ค่าน้ำ"),
        "EXPENSE",
        "CARD_SCB",
        "HOME_WATER",
        "250",
        "MONTHLY",
        day(9, 20),
    )
    .await?;
    // Интернет заведён на тайском без перевода на русский — пример «нет перевода».
    let net = s
        .planning
        .create(
            p,
            anna,
            planning::dto::PlanInput {
                name: "อินเทอร์เน็ต AIS Fibre".into(),
                name_lang: "TH".into(),
                name_ru: None,
                name_en: Some("AIS Fibre internet".into()),
                name_th: None,
                tx_kind: "EXPENSE".into(),
                account_code: "CARD_SCB".into(),
                category_code: Some("HOME_INTERNET".into()),
                to_account_code: None,
                amount: dec("649"),
                recurrence: "MONTHLY".into(),
                interval: 1,
                start_on: day(9, 12),
                end_on: None,
                active: true,
            },
        )
        .await?
        .id;
    let netflix = plan(
        s,
        p,
        me,
        Lang::En,
        ("Netflix", "Netflix", "Netflix"),
        "EXPENSE",
        "CARD_KBANK",
        "FUN_SUBSCRIPTIONS",
        "419",
        "MONTHLY",
        day(9, 15),
    )
    .await?;
    let salary = plan(
        s,
        p,
        me,
        ru,
        ("Зарплата Алексея", "Aleksey's salary", "เงินเดือนอเล็กเซย์"),
        "INCOME",
        "CARD_KBANK",
        "SALARY",
        "85000",
        "MONTHLY",
        day(9, 1),
    )
    .await?;
    let freelance = plan(
        s,
        p,
        anna,
        Lang::Th,
        ("Подработка Анны", "Anna's side gig", "งานเสริมของแอนนา"),
        "INCOME",
        "CARD_SCB",
        "FREELANCE",
        "12000",
        "MONTHLY",
        day(9, 25),
    )
    .await?;
    plan(
        s,
        p,
        me,
        ru,
        ("Страховка авто", "Car insurance", "ประกันรถ"),
        "EXPENSE",
        "CARD_KBANK",
        "TRANSPORT",
        "7200",
        "ONCE",
        day(0, 27),
    )
    .await?;
    // Плановый перевод: рубли → баты 2-го числа (сумму зачисления вводят при оплате).
    let exchange = plan(
        s,
        p,
        me,
        ru,
        ("Обмен рублей на баты", "RUB to THB exchange", "แลกรูเบิลเป็นบาท"),
        "TRANSFER",
        "TINKOFF_RUB",
        "CARD_KBANK",
        "20050",
        "MONTHLY",
        day(9, 2),
    )
    .await?;

    // Прошлый месяц: все планы оплачены (кроме подработки — пропуск), плюс повседневные траты.
    for back in [1u32, 0] {
        let pay = |plan: Uuid, d: u32| (plan, day(back, d));
        tx(
            s,
            p,
            Tx {
                date: day(back, 1),
                account: "CARD_KBANK",
                kind: "INCOME",
                desc: Some(("Зарплата", ru)),
                tr: (None, Some("Salary"), Some("เงินเดือน")),
                lines: vec![("SALARY", "85000", None)],
                plan: Some(pay(salary, 1)),
                place: None,
                by: me,
            },
        )
        .await?;
        tx(
            s,
            p,
            Tx {
                date: day(back, 6),
                account: "CARD_KBANK",
                kind: "EXPENSE",
                desc: Some(("Аренда квартиры", ru)),
                tr: (None, Some("Apartment rent"), Some("ค่าเช่าคอนโด")),
                lines: vec![("HOME_RENT", "15000", None)],
                plan: Some(pay(rent, 5)),
                place: None,
                by: me,
            },
        )
        .await?;
        tx(
            s,
            p,
            Tx {
                date: day(back, 3),
                account: "CARD_SCB",
                kind: "EXPENSE",
                desc: Some(("ท็อปส์ ซูเปอร์มาร์เก็ต", Lang::Th)),
                tr: (Some("Tops, супермаркет"), None, None),
                lines: vec![
                    ("FOOD_GROCERIES", "1640", Some("овощи, мясо")),
                    ("HOUSEHOLD", "385", Some("средства для стирки")),
                    ("FOOD_CAFE", "120", Some("кофе")),
                ],
                plan: None,
                place: Some("TOPS"),
                by: anna,
            },
        )
        .await?;
        tx(
            s,
            p,
            Tx {
                date: day(back, 4),
                account: "CASH",
                kind: "EXPENSE",
                desc: Some(("Grab до офиса", ru)),
                tr: (None, None, None),
                lines: vec![("TRANSPORT_TAXI", "210", None)],
                plan: None,
                place: Some("GRAB"),
                by: me,
            },
        )
        .await?;
        // Чек с ценами: количество × цена − скидка; бананы — на вес.
        // Названия — как на чеке (по-английски), их не переводят.
        receipt(
            s,
            p,
            day(back, 12),
            "SEVEN_ELEVEN_SUKHUMVIT_24",
            ("Завтрак в 7-Eleven", ru),
            (None, Some("Breakfast at 7-Eleven"), Some("อาหารเช้าที่เซเว่น")),
            &[
                ("FOOD_GROCERIES", "Water 1.5 L", "1", "pcs", "14", "0"),
                ("FOOD_CAFE", "Latte", "1", "pcs", "45", "0"),
                ("FOOD_CAFE", "Croissant", "2", "pcs", "25", "3"),
                ("FOOD_GROCERIES", "Bananas", "0.45", "kg", "60", "0"),
            ],
            me,
        )
        .await?;
        // Обмен через приложение: списано 20 050 ₽ (из них 50 ₽ комиссия), зачислено по курсу хуже справочного.
        let got = if back == 1 { "7640" } else { "7280" };
        transfer(
            s,
            p,
            Transfer {
                date: day(back, 2),
                from: "TINKOFF_RUB",
                to: "CARD_KBANK",
                amount: "20050",
                to_amount: Some(got),
                fee: Some(("FEES", "50")),
                desc: ("Обмен через приложение", ru),
                plan: Some(pay(exchange, 2)),
                by: me,
            },
        )
        .await?;
        transfer(
            s,
            p,
            Transfer {
                date: day(back, 10),
                from: "CARD_KBANK",
                to: "CASH",
                amount: "5000",
                to_amount: None,
                fee: None,
                desc: ("Снял наличные", ru),
                plan: None,
                by: me,
            },
        )
        .await?;
        tx(
            s,
            p,
            Tx {
                date: day(back, 7),
                account: "CARD_KBANK",
                kind: "EXPENSE",
                desc: Some(("Ужин с друзьями", ru)),
                tr: (None, Some("Dinner with friends"), None),
                lines: vec![("FOOD_CAFE", "2450", None)],
                plan: None,
                place: None,
                by: me,
            },
        )
        .await?;
        tx_for(
            s,
            p,
            Tx {
                date: day(back, 8),
                account: "CARD_SCB",
                kind: "EXPENSE",
                desc: Some(("Big C", Lang::En)),
                tr: (None, None, Some("บิ๊กซี")),
                lines: vec![
                    ("FOOD_GROCERIES", "2180", None),
                    ("KIDS", "690", Some("краски")),
                    ("HOUSEHOLD", "240", None),
                ],
                plan: None,
                place: Some("BIG_C"),
                by: anna,
            },
            &[None, Some("PINPAN"), None],
        )
        .await?;
        tx_for(
            s,
            p,
            Tx {
                date: day(back, 14),
                account: "CARD_KBANK",
                kind: "EXPENSE",
                desc: Some(("Кружок рисования", ru)),
                tr: (None, Some("Art class"), Some("คลาสวาดรูป")),
                lines: vec![("KIDS", "1200", None)],
                plan: None,
                place: None,
                by: anna,
            },
            &[Some("PINPAN")],
        )
        .await?;
        tx(
            s,
            p,
            Tx {
                date: day(back, 9),
                account: "WISE_USD",
                kind: "EXPENSE",
                desc: Some(("Spotify Family", Lang::En)),
                tr: (None, None, None),
                lines: vec![("FUN_SUBSCRIPTIONS", "19.99", None)],
                plan: None,
                place: None,
                by: me,
            },
        )
        .await?;
        tx(
            s,
            p,
            Tx {
                date: day(back, 9),
                account: "CASH",
                kind: "EXPENSE",
                desc: Some(("ร้านกาแฟ", Lang::Th)),
                tr: (None, None, None),
                lines: vec![("FOOD_CAFE", "1850", Some("кофе за неделю"))],
                plan: None,
                place: Some("CAFE_AMAZON"),
                by: anna,
            },
        )
        .await?;
        if back == 1 {
            tx(
                s,
                p,
                Tx {
                    date: day(1, 12),
                    account: "CARD_SCB",
                    kind: "EXPENSE",
                    desc: None,
                    tr: (None, None, None),
                    lines: vec![("HOME_INTERNET", "649", None)],
                    plan: Some(pay(net, 12)),
                    place: None,
                    by: anna,
                },
            )
            .await?;
            tx(
                s,
                p,
                Tx {
                    date: day(1, 15),
                    account: "CARD_KBANK",
                    kind: "EXPENSE",
                    desc: Some(("Netflix", Lang::En)),
                    tr: (None, None, None),
                    lines: vec![("FUN_SUBSCRIPTIONS", "419", None)],
                    plan: Some(pay(netflix, 15)),
                    place: None,
                    by: me,
                },
            )
            .await?;
            tx(
                s,
                p,
                Tx {
                    date: day(1, 21),
                    account: "CARD_KBANK",
                    kind: "EXPENSE",
                    desc: Some(("Электричество", ru)),
                    tr: (None, None, None),
                    lines: vec![("HOME_ELECTRICITY", "1960", Some("кондиционер"))],
                    plan: Some(pay(power, 20)),
                    place: None,
                    by: me,
                },
            )
            .await?;
            tx(
                s,
                p,
                Tx {
                    date: day(1, 20),
                    account: "CARD_SCB",
                    kind: "EXPENSE",
                    desc: None,
                    tr: (None, None, None),
                    lines: vec![("HOME_WATER", "230", None)],
                    plan: Some(pay(water, 20)),
                    place: None,
                    by: anna,
                },
            )
            .await?;
            tx_for(
                s,
                p,
                Tx {
                    date: day(1, 18),
                    account: "CARD_KBANK",
                    kind: "EXPENSE",
                    desc: Some(("Аптека", ru)),
                    tr: (None, Some("Pharmacy"), Some("ร้านขายยา")),
                    lines: vec![("HEALTH", "780", None)],
                    plan: None,
                    place: None,
                    by: me,
                },
                &[anna_code.as_deref()],
            )
            .await?;
            tx(
                s,
                p,
                Tx {
                    date: day(1, 23),
                    account: "CARD_KBANK",
                    kind: "EXPENSE",
                    desc: Some(("Кино", ru)),
                    tr: (None, None, None),
                    lines: vec![("FUN", "640", None), ("FOOD_CAFE", "260", Some("попкорн"))],
                    plan: None,
                    place: None,
                    by: me,
                },
            )
            .await?;
            s.planning.skip(p, anna, freelance, day(1, 25)).await?;
        }
    }
    sms_purchase(s, p, me).await
}

/// Покупка по карте из SMS банка (docs/external-refs.md): связь с SMS и остаток банка после неё. Остаток такой же,
/// как по операциям, — карточка счёта показывает «Сходится».
async fn sms_purchase(s: &Services, p: Uuid, by: Uuid) -> AppResult<()> {
    let at = chrono::Utc::now() - chrono::Duration::minutes(5);
    let amount = dec("95");
    let balance = s.transactions.balance_at(p, "CARD_KBANK", at).await? - amount;
    let input = transactions::dto::TransactionInput {
        occurred_on: at.with_timezone(&chrono::Local).date_naive(),
        occurred_at: Some(at.to_rfc3339()),
        tx_kind: "EXPENSE".into(),
        account_code: "CARD_KBANK".into(),
        description: Some("Кофе".into()),
        description_lang: Lang::Ru.upper().into(),
        description_en: Some("Coffee".into()),
        description_th: Some("กาแฟ".into()),
        items: vec![transactions::dto::ItemInput {
            category_code: Some("FOOD_CAFE".into()),
            amount: Some(amount),
            ..Default::default()
        }],
        external_ref: Some(transactions::dto::ExternalRefInput {
            provider: "imessage".into(),
            external_id: "demo:kbank-coffee".into(),
            received_at: Some(at.to_rfc3339()),
            occurred_at: Some(at.to_rfc3339()),
            balance: Some(balance),
            account_code: None,
        }),
        ..Default::default()
    };
    s.transactions.create(p, by, input).await.map(|_| ())
}

async fn fill_personal(s: &Services, me: Uuid) -> AppResult<()> {
    let session = Memberships::session(s.projects.as_ref(), me, None)
        .await?
        .ok_or_else(|| kernel::error::AppError::NotFound("personal".into()))?;
    let p = session.project_id;
    account(s, p, me, "CASH", "CASH", "THB", "2000", Some(me), Lang::Ru, ("Мои наличные", "My cash", "เงินสดของฉัน"))
        .await?;
    tx(
        s,
        p,
        Tx {
            date: day(0, 2),
            account: "CASH",
            kind: "EXPENSE",
            desc: Some(("Книги по Rust", Lang::Ru)),
            tr: (None, Some("Rust books"), None),
            lines: vec![("FUN", "890", None)],
            plan: None,
            place: None,
            by: me,
        },
    )
    .await?;
    Ok(())
}
