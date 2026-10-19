//! Модель фичи «MCP для ассистентов»: кто вызывает инструменты, с какими правами, и данные,
//! которыми фича обменивается с другими фичами через порты (без их типов).

use chrono::{DateTime, NaiveDate, Utc};
use kernel::auth::{RequestMeta, Role, scope_allows};
use kernel::i18n::Lang;
use rust_decimal::Decimal;
use uuid::Uuid;

// ---------------------------------------------------------------- права

/// Право, которое нужно инструменту: раздел и запись/чтение — в формате прав API-токенов.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Right {
    pub feature: &'static str,
    pub write: bool,
}

impl Right {
    pub const fn read(feature: &'static str) -> Right {
        Right { feature, write: false }
    }
    pub const fn write(feature: &'static str) -> Right {
        Right { feature, write: true }
    }
}

/// Права доступа ассистента, которые можно выдать в браузере. «Просмотр» есть всегда.
pub const GRANT_SCOPES: [&str; 8] =
    ["read", "transactions:write", "currencies:write", "accounts:write", "categories:write", "rates:write", "limits:write", "planning:write"];
/// Отмечены по умолчанию: просмотр всего и операции.
pub const DEFAULT_GRANT_SCOPES: [&str; 2] = ["read", "transactions:write"];
/// Справочники: их разом отмечает флажок «Справочники».
pub const DIRECTORY_SCOPES: [&str; 4] = ["currencies:write", "accounts:write", "categories:write", "rates:write"];
/// Значение флажка «Справочники» в форме — все права из `DIRECTORY_SCOPES` (форма работает и без JS).
pub const ALL_DIRECTORIES: &str = "directories";

/// Блок страницы «Ассистенты», в котором показано право.
pub fn scope_group(scope: &str) -> &'static str {
    if DIRECTORY_SCOPES.contains(&scope) {
        "directories"
    } else {
        match scope {
            "limits:write" => "limits",
            "planning:write" => "planning",
            _ => "base",
        }
    }
}

/// Оставить только известные права доступа ассистента; «просмотр» добавляется всегда.
pub fn normalize_grant_scopes(input: &[String]) -> Vec<String> {
    let all_directories = input.iter().any(|x| x == ALL_DIRECTORIES);
    let mut out = vec!["read".to_string()];
    for s in GRANT_SCOPES.iter().skip(1) {
        if input.iter().any(|x| x == s) || (all_directories && DIRECTORY_SCOPES.contains(s)) {
            out.push((*s).to_string());
        }
    }
    out
}

/// Как подтверждён вызывающий.
#[derive(Debug, Clone, PartialEq)]
pub enum Via {
    /// Личный API-токен (`fbt_…`): проект и права — у токена.
    Token { token_id: Uuid },
    /// Access-токен провайдера входа (OAuth): проект и права — из доступа ассистента.
    Grant { grant_id: Uuid, client_id: String },
    /// Access-токен провайдера есть, а доступ ассистенту не настроен (или больше не действует).
    Unconfigured { client_id: String, reason: Unconfigured },
}

/// Почему у ассистента нет прав, хотя токен настоящий.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unconfigured {
    /// Доступ ещё не открыт в браузере.
    NoGrant,
    /// Пользователь больше не участник проекта из доступа.
    ProjectGone,
    /// Проект требует 2FA, а доступ открыт без второго фактора.
    Mfa,
}

impl Unconfigured {
    pub fn message(self, setup_url: &str) -> String {
        match self {
            Unconfigured::NoGrant => format!(
                "Access for this assistant is not configured yet. Ask the user to open {setup_url} in a browser, choose a project and permissions, then retry."
            ),
            Unconfigured::ProjectGone => format!(
                "The user is no longer a member of the project this assistant was given access to. Ask the user to choose another project at {setup_url}."
            ),
            Unconfigured::Mfa => format!(
                "The project requires two-factor sign-in. Ask the user to sign in with a second factor and re-save assistant access at {setup_url}."
            ),
        }
    }
}

/// Тот, кто вызывает инструменты: пользователь, проект, роль и права.
#[derive(Debug, Clone)]
pub struct Caller {
    pub user_id: Uuid,
    pub project_id: Uuid,
    pub project_name: String,
    pub role: Role,
    pub scopes: Vec<String>,
    pub base_currency: String,
    pub lang: Lang,
    pub via: Via,
    /// IP и клиент — для журнала безопасности.
    pub meta: RequestMeta,
}

impl Caller {
    /// Разрешено ли действие: право доступа (как у API-токена) и роль в проекте (запись — от EDITOR).
    pub fn allows(&self, right: Right) -> bool {
        scope_allows(&self.scopes, right.feature, right.write) && (!right.write || self.role >= Role::Editor)
    }

    /// Ключ лимита запросов.
    pub fn throttle_key(&self) -> String {
        match &self.via {
            Via::Token { token_id } => format!("mcp:t:{token_id}"),
            Via::Grant { grant_id, .. } => format!("mcp:g:{grant_id}"),
            Via::Unconfigured { client_id, .. } => format!("mcp:u:{}:{client_id}", self.user_id),
        }
    }

    pub fn client(&self) -> &str {
        match &self.via {
            Via::Token { .. } => "api-token",
            Via::Grant { client_id, .. } | Via::Unconfigured { client_id, .. } => client_id,
        }
    }
}

/// Доступ ассистента (OAuth-клиента) к проекту, открытый пользователем в браузере.
#[derive(Debug, Clone, PartialEq)]
pub struct Grant {
    pub id: Uuid,
    pub user_id: Uuid,
    pub client_id: String,
    pub project_id: Uuid,
    pub scopes: Vec<String>,
    pub mfa: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------- данные других фич

/// Счёт для подсказок ассистенту.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountInfo {
    pub code: String,
    pub title: String,
    pub currency: String,
    pub symbol: String,
    /// Есть курс валюты счёта к базовой на дату: без него сумма не войдёт в бюджет (этап 0, Ц3).
    pub has_rate: bool,
}

/// Категория дерева: названия на всех языках (`search`) и остаток лимита за месяц.
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryInfo {
    pub code: String,
    pub parent_code: Option<String>,
    /// EXPENSE / INCOME
    pub kind: String,
    pub title: String,
    pub depth: usize,
    /// Оригинал и переводы названия через пробел.
    pub search: String,
    pub limit: Option<Decimal>,
    pub available: Option<Decimal>,
}

/// Всё, чтобы быстро записать операцию: счета, категории, частые, счёт по умолчанию.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuickInfo {
    pub default_account: Option<String>,
    pub accounts: Vec<AccountInfo>,
    pub categories: Vec<CategoryInfo>,
    pub frequent_expense: Vec<String>,
    pub frequent_income: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TxLine {
    pub category_code: String,
    pub amount: Decimal,
    pub note: Option<String>,
}

/// Операция для создания или изменения (как форма в интерфейсе).
#[derive(Debug, Clone, PartialEq)]
pub struct TxDraft {
    pub occurred_on: NaiveDate,
    /// EXPENSE / INCOME / TRANSFER
    pub kind: String,
    pub account_code: String,
    pub refund: bool,
    pub lines: Vec<TxLine>,
    pub to_account_code: Option<String>,
    pub amount: Option<Decimal>,
    pub to_amount: Option<Decimal>,
    pub fee_category_code: Option<String>,
    pub fee_amount: Option<Decimal>,
    pub description: Option<String>,
    pub description_lang: Lang,
    pub description_ru: Option<String>,
    pub description_en: Option<String>,
    pub description_th: Option<String>,
    pub plan_id: Option<Uuid>,
    pub plan_date: Option<NaiveDate>,
}

/// Сохранённая операция.
#[derive(Debug, Clone, PartialEq)]
pub struct Tx {
    pub id: Uuid,
    pub occurred_on: NaiveDate,
    pub kind: String,
    pub account_code: String,
    pub to_account_code: Option<String>,
    pub refund: bool,
    pub lines: Vec<TxLine>,
    pub total: Decimal,
    pub currency: String,
    pub to_amount: Option<Decimal>,
    pub to_currency: Option<String>,
    pub fee_category_code: Option<String>,
    pub fee_amount: Option<Decimal>,
    pub description: Option<String>,
    pub description_lang: Lang,
    pub description_ru: Option<String>,
    pub description_en: Option<String>,
    pub description_th: Option<String>,
    pub created_by: Uuid,
    pub plan_id: Option<Uuid>,
    pub plan_date: Option<NaiveDate>,
}

impl Tx {
    /// Операция как черновик — основа для частичного изменения.
    pub fn draft(&self) -> TxDraft {
        TxDraft {
            occurred_on: self.occurred_on,
            kind: self.kind.clone(),
            account_code: self.account_code.clone(),
            refund: self.refund,
            lines: self.lines.clone(),
            to_account_code: self.to_account_code.clone(),
            amount: (self.kind == "TRANSFER").then_some(self.total),
            to_amount: self.to_amount,
            fee_category_code: self.fee_category_code.clone(),
            fee_amount: self.fee_amount,
            description: self.description.clone(),
            description_lang: self.description_lang,
            description_ru: self.description_ru.clone(),
            description_en: self.description_en.clone(),
            description_th: self.description_th.clone(),
            plan_id: self.plan_id,
            plan_date: self.plan_date,
        }
    }

    /// Описание на языке `lang` (оригинал или перевод).
    pub fn description_in(&self, lang: Lang) -> Option<String> {
        let original = self.description.as_deref()?;
        let t = match lang {
            Lang::Ru => self.description_ru.as_deref(),
            Lang::En => self.description_en.as_deref(),
            Lang::Th => self.description_th.as_deref(),
        };
        Some(if lang == self.description_lang { original } else { t.filter(|s| !s.trim().is_empty()).unwrap_or(original) }.to_string())
    }
}

/// Фильтр списка операций.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TxFilter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub account_code: Option<String>,
    pub kind: Option<String>,
}

/// Строка бюджета по категории (суммы по поддереву, в базовой валюте).
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetLine {
    pub code: String,
    pub parent_code: Option<String>,
    pub title: String,
    pub kind: String,
    pub depth: usize,
    pub limit: Option<Decimal>,
    pub fact: Decimal,
    pub planned: Decimal,
    pub forecast: Decimal,
    pub available: Option<Decimal>,
    /// none / ok / warn (≥ 80 %) / over (> 100 %)
    pub level: String,
    /// Суммы поддерева без курса к базовой валюте (валюта, сумма): в факт не вошли.
    pub no_rate: Vec<(String, Decimal)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetReport {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub base_currency: String,
    pub expense_fact: Decimal,
    pub expense_planned: Decimal,
    pub income_fact: Decimal,
    pub income_planned: Decimal,
    pub lines: Vec<BudgetLine>,
    pub missing_rates: Vec<String>,
}

/// Вхождение плана (платёж по расписанию).
#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub plan_id: Uuid,
    pub name: String,
    pub date: NaiveDate,
    /// EXPECTED / OVERDUE / PAID / SKIPPED
    pub status: String,
    pub kind: String,
    pub amount: Decimal,
    pub account_code: String,
    pub category_code: Option<String>,
    pub to_account_code: Option<String>,
    pub transaction_id: Option<Uuid>,
}

/// Название записи: текст на языке оригинала и переводы (справочники, планы).
#[derive(Debug, Clone, PartialEq)]
pub struct Name {
    pub text: String,
    pub lang: Lang,
    pub ru: Option<String>,
    pub en: Option<String>,
    pub th: Option<String>,
}

impl Name {
    pub fn new(field: &str, text: &str, lang: Lang, ru: Option<String>, en: Option<String>, th: Option<String>) -> Result<Name, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err(format!("{field}: empty"));
        }
        Ok(Name { text: text.to_string(), lang, ru: kernel::i18n::clean(ru), en: kernel::i18n::clean(en), th: kernel::i18n::clean(th) })
    }

    /// Частичное изменение. Новый текст — переводы только переданные (старые описывали прежний
    /// текст); без нового текста переданные переводы заменяют старые по языкам (пустой — убрать).
    pub fn changed(&self, field: &str, text: Option<&str>, lang: Option<Lang>, ru: Option<String>, en: Option<String>, th: Option<String>) -> Result<Name, String> {
        let lang = lang.unwrap_or(self.lang);
        match text {
            Some(t) => Name::new(field, t, lang, ru, en, th),
            None => {
                let keep = |new: Option<String>, old: &Option<String>| match new {
                    Some(v) => kernel::i18n::clean(Some(v)),
                    None => old.clone(),
                };
                Ok(Name { text: self.text.clone(), lang, ru: keep(ru, &self.ru), en: keep(en, &self.en), th: keep(th, &self.th) })
            }
        }
    }

    /// Название на языке `lang` (перевод или оригинал).
    pub fn show(&self, lang: Lang) -> &str {
        kernel::i18n::pick(lang, &self.text, self.lang, self.ru.as_deref(), self.en.as_deref(), self.th.as_deref())
    }

    /// Оригинал и переводы через `\u{1f}` — для поиска по названию на любом языке (`resolve`).
    pub fn search(&self) -> String {
        [Some(self.text.as_str()), self.ru.as_deref(), self.en.as_deref(), self.th.as_deref()].into_iter().flatten().collect::<Vec<_>>().join("\u{1f}")
    }
}

/// Валюта (фича `currencies`).
#[derive(Debug, Clone, PartialEq)]
pub struct Currency {
    pub code: String,
    pub name: Name,
    pub symbol: String,
    pub minor_units: i32,
    pub archived: bool,
}

/// Счёт (фича `accounts`).
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub code: String,
    pub name: Name,
    /// CASH / CARD / BANK / SAVINGS
    pub kind: String,
    pub currency: String,
    pub holder_user_id: Option<Uuid>,
    pub archived: bool,
}

/// Категория (фича `categories`).
#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    pub code: String,
    pub name: Name,
    /// EXPENSE / INCOME
    pub kind: String,
    pub parent_code: Option<String>,
    pub sort_order: i32,
    pub archived: bool,
}

/// Курс валюты на дату: сколько единиц базовой валюты за 1 единицу `currency` (фича `rates`).
#[derive(Debug, Clone, PartialEq)]
pub struct Rate {
    /// При создании не используется: id выбирает фича.
    pub id: Uuid,
    pub date: NaiveDate,
    pub currency: String,
    /// Базовой за 1 единицу валюты; считается из `entered` и `inverted`.
    pub rate: Decimal,
    /// Число, как его передали.
    pub entered: Decimal,
    /// `false` — 1 валюта = `entered` базовой; `true` — 1 базовая = `entered` валюты.
    pub inverted: bool,
}

impl Rate {
    /// Курс из введённого числа и направления (как в фиче `rates`: 12 знаков у пересчитанного).
    pub fn entered(id: Uuid, date: NaiveDate, currency: String, entered: Decimal, inverted: bool) -> Self {
        let rate = if inverted { invert(entered) } else { entered };
        Self { id, date, currency, rate, entered, inverted }
    }

    /// Сколько единиц валюты за 1 единицу базовой.
    pub fn inverse_rate(&self) -> Decimal {
        if self.inverted { self.entered } else { invert(self.rate) }
    }
}

fn invert(x: Decimal) -> Decimal {
    if x.is_zero() { Decimal::ZERO } else { (Decimal::ONE / x).round_dp(12).normalize() }
}

/// Курс: число больше нуля, не больше 6 знаков после запятой (как хранит фича `rates`).
pub fn parse_rate(field: &str, value: &str) -> Result<Decimal, String> {
    let cleaned: String = value.chars().filter(|c| !c.is_whitespace()).collect::<String>().replace(',', ".");
    let d: Decimal = cleaned.parse().map_err(|_| format!("{field}: '{value}' is not a number"))?;
    let d = d.normalize();
    if d <= Decimal::ZERO {
        return Err(format!("{field}: the rate must be greater than zero"));
    }
    if d.scale() > 6 {
        return Err(format!("{field}: at most 6 digits after the decimal point"));
    }
    Ok(d)
}

/// Правило лимита категории (фича `limits`): сумма в базовой валюте за период, действует с `valid_from`.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    /// При создании не используется: id выбирает фича.
    pub id: Uuid,
    pub category_code: String,
    /// WEEK / MONTH / YEAR
    pub period: String,
    pub valid_from: NaiveDate,
    /// 0 — тратить нельзя совсем; `None` — «лимит снят» с `valid_from`.
    pub amount: Option<Decimal>,
}

/// Плановая операция для создания или изменения (фича `planning`).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanDraft {
    pub name: Name,
    /// EXPENSE / INCOME / TRANSFER
    pub kind: String,
    pub account_code: String,
    pub category_code: Option<String>,
    pub to_account_code: Option<String>,
    /// В валюте счёта.
    pub amount: Decimal,
    /// ONCE / DAILY / WEEKLY / MONTHLY / YEARLY
    pub recurrence: String,
    pub interval: u32,
    pub start_on: NaiveDate,
    pub end_on: Option<NaiveDate>,
    pub active: bool,
}

/// Сохранённая плановая операция.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub id: Uuid,
    pub draft: PlanDraft,
}

pub const ACCOUNT_KINDS: [&str; 4] = ["CASH", "CARD", "BANK", "SAVINGS"];
pub const CATEGORY_KINDS: [&str; 2] = ["EXPENSE", "INCOME"];
pub const PLAN_KINDS: [&str; 3] = ["EXPENSE", "INCOME", "TRANSFER"];
pub const LIMIT_PERIODS: [&str; 3] = ["WEEK", "MONTH", "YEAR"];
pub const RECURRENCES: [&str; 5] = ["ONCE", "DAILY", "WEEKLY", "MONTHLY", "YEARLY"];

/// Запись справочника, на которую ссылаются по коду или названию.
pub trait Record {
    fn code(&self) -> &str;
    /// Названия на всех языках через `\u{1f}`.
    fn names(&self) -> String;
    fn archived(&self) -> bool;
}

impl Record for Currency {
    fn code(&self) -> &str {
        &self.code
    }
    fn names(&self) -> String {
        format!("{}\u{1f}{}", self.name.search(), self.symbol)
    }
    fn archived(&self) -> bool {
        self.archived
    }
}

impl Record for Account {
    fn code(&self) -> &str {
        &self.code
    }
    fn names(&self) -> String {
        self.name.search()
    }
    fn archived(&self) -> bool {
        self.archived
    }
}

impl Record for Category {
    fn code(&self) -> &str {
        &self.code
    }
    fn names(&self) -> String {
        self.name.search()
    }
    fn archived(&self) -> bool {
        self.archived
    }
}

impl<T: Record> Record for &T {
    fn code(&self) -> &str {
        (**self).code()
    }
    fn names(&self) -> String {
        (**self).names()
    }
    fn archived(&self) -> bool {
        (**self).archived()
    }
}

/// Ссылка на активную запись (валюта счёта, родитель категории, счёт плана): код или название на любом языке.
pub fn find_ref<'a, T: Record>(field: &str, query: &str, items: &'a [T]) -> Result<&'a T, String> {
    let active: Vec<&T> = items.iter().filter(|x| !x.archived()).collect();
    match resolve(field, query, &active, |x| x.code(), |x| x.names()) {
        Ok(x) => Ok(*x),
        Err(e) => match items.iter().find(|x| x.archived() && x.code().eq_ignore_ascii_case(query.trim())) {
            Some(x) => Err(format!("{field}: '{}' is archived; the user can restore it in the browser", x.code())),
            None => Err(e),
        },
    }
}

/// Изменяемая запись: точный код (в том числе архивной) или название активной.
pub fn find_record<'a, T: Record>(field: &str, query: &str, items: &'a [T]) -> Result<&'a T, String> {
    match items.iter().find(|x| x.code().eq_ignore_ascii_case(query.trim())) {
        Some(x) => Ok(x),
        None => find_ref(field, query, items),
    }
}

// ---------------------------------------------------------------- разбор параметров

/// Код новой записи справочника: UPPER_SNAKE (буквы, цифры, `_`, первая — буква), до 40 знаков.
pub fn parse_code(field: &str, value: &str) -> Result<String, String> {
    let code = value.trim().to_uppercase();
    let ok = code.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && code.len() <= 40
        && code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if ok { Ok(code) } else { Err(format!("{field}: '{}' — use UPPER_SNAKE: Latin letters, digits and _, starting with a letter (e.g. KBANK, CARD_KBANK)", value.trim())) }
}

/// Значение из списка без учёта регистра: `cash` → `CASH`.
pub fn parse_choice(field: &str, value: &str, allowed: &[&str]) -> Result<String, String> {
    let v = value.trim().to_uppercase();
    if allowed.contains(&v.as_str()) {
        Ok(v)
    } else {
        Err(format!("{field}: expected one of {}, got '{}'", allowed.iter().map(|a| a.to_lowercase()).collect::<Vec<_>>().join(" | "), value.trim()))
    }
}

/// Сумма со знаком (начальный остаток может быть отрицательным — долг по карте).
pub fn parse_signed_amount(field: &str, value: &str) -> Result<Decimal, String> {
    kernel::web::parse_money(field, value)
}

/// Начало периода лимита (WEEK — с понедельника, MONTH — 1-е число, YEAR — 1 января), содержащего `date`.
pub fn period_start(period: &str, date: NaiveDate) -> NaiveDate {
    use chrono::Datelike;
    match period {
        "WEEK" => date - chrono::Days::new(u64::from(date.weekday().num_days_from_monday())),
        "YEAR" => NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap_or(date),
        _ => date.with_day(1).unwrap_or(date),
    }
}

/// Сумма из текста: «120.50», «120,50», «1 200», «120+45» (как в поле суммы интерфейса); > 0.
pub fn parse_amount(field: &str, value: &str) -> Result<Decimal, String> {
    let d = kernel::web::parse_money(field, value)?;
    if d <= Decimal::ZERO {
        return Err(format!("{field}: amount must be greater than zero"));
    }
    Ok(d)
}

/// Дата `YYYY-MM-DD`; пусто — `today`.
pub fn parse_date(field: &str, value: Option<&str>, today: NaiveDate) -> Result<NaiveDate, String> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(today),
        Some("today") => Ok(today),
        Some("yesterday") => Ok(today.pred_opt().unwrap_or(today)),
        Some(v) => NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| format!("{field}: expected date YYYY-MM-DD, got '{v}'")),
    }
}

/// Найти код по коду или названию на любом языке. Ошибка — с вариантами, чтобы ассистент уточнил.
pub fn resolve<'a, T>(field: &str, query: &str, items: &'a [T], code: impl Fn(&T) -> &str, names: impl Fn(&T) -> String) -> Result<&'a T, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err(format!("{field}: empty"));
    }
    if let Some(x) = items.iter().find(|x| code(x).eq_ignore_ascii_case(q)) {
        return Ok(x);
    }
    let low = q.to_lowercase();
    let exact: Vec<&T> = items.iter().filter(|x| names(x).to_lowercase().split('\u{1f}').any(|n| n.trim() == low)).collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    let partial: Vec<&T> = items.iter().filter(|x| names(x).to_lowercase().contains(&low)).collect();
    match partial.len() {
        1 => Ok(partial[0]),
        0 => Err(format!("{field}: '{q}' not found; call get_context for valid codes")),
        _ => Err(format!(
            "{field}: '{q}' is ambiguous: {}; pass the exact code",
            partial.iter().take(8).map(|x| code(x)).collect::<Vec<_>>().join(", ")
        )),
    }
}
