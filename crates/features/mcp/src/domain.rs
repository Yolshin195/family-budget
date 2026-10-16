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
pub const GRANT_SCOPES: [&str; 3] = ["read", "transactions:write", "planning:write"];
/// Отмечены по умолчанию: просмотр всего и операции.
pub const DEFAULT_GRANT_SCOPES: [&str; 2] = ["read", "transactions:write"];

/// Оставить только известные права доступа ассистента; «просмотр» добавляется всегда.
pub fn normalize_grant_scopes(input: &[String]) -> Vec<String> {
    let mut out = vec!["read".to_string()];
    for s in GRANT_SCOPES.iter().skip(1) {
        if input.iter().any(|x| x == s) {
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

// ---------------------------------------------------------------- разбор параметров

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
