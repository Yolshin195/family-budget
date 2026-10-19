//! Use cases фичи «MCP для ассистентов»: кто вызывает (вход по токену), доступы ассистентов и
//! инструменты бюджета. Права проверяются здесь — в каждом инструменте, какой бы клиент ни вызвал.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{Days, Duration, NaiveDate, Utc};
use kernel::auth::{API_TOKEN_PREFIX, Credential, RequestMeta, Role, Via as AuthVia};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::throttle::RateLimiter;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::*;
use crate::dto::*;
use crate::ports::{Deps, GrantRepository};

/// Настройки фичи (из окружения, см. `Settings::from_config`).
#[derive(Debug, Clone)]
pub struct Settings {
    /// Публичный адрес приложения без слеша в конце.
    pub base_url: String,
    /// Адрес MCP-ресурса (`<base_url>/mcp`) — с ним сверяется `aud` токенов провайдера.
    pub resource: String,
    /// OAuth-клиент ассистента у провайдера входа (вводится в Claude как Client ID).
    pub oauth_client_id: String,
    /// Scope, который ассистент запрашивает у провайдера.
    pub oauth_scope: String,
    /// Запросов в минуту на ассистента.
    pub rate_limit: u32,
}

impl Settings {
    pub fn from_config(config: &kernel::config::Config) -> Self {
        let base_url = config.base_url.trim_end_matches('/').to_string();
        let var = |name: &str, default: &str| std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_string());
        Settings {
            resource: format!("{base_url}/mcp"),
            base_url,
            oauth_client_id: var("MCP_OAUTH_CLIENT_ID", "claude"),
            oauth_scope: var("MCP_OAUTH_SCOPE", "openid"),
            rate_limit: config.rate_limit,
        }
    }

    pub fn setup_url(&self) -> String {
        format!("{}/assistants", self.base_url)
    }

    pub fn resource_metadata_url(&self) -> String {
        format!("{}/.well-known/oauth-protected-resource/mcp", self.base_url)
    }
}

/// Итог проверки токена на `/mcp`.
#[derive(Debug)]
pub enum Auth {
    Ok(Box<Caller>),
    /// Токена нет — 401 с адресом метаданных (начать OAuth).
    Missing,
    /// Токен не прошёл проверку — 401 `invalid_token`.
    Invalid,
    Throttled,
}

/// Результат инструмента: короткий итог для модели + данные.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub summary: String,
    pub data: Value,
}

/// Ошибка инструмента — текст для модели (не обрывает соединение).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolError(pub String);

pub type ToolResult = Result<ToolOutput, ToolError>;

impl From<AppError> for ToolError {
    fn from(e: AppError) -> Self {
        match e {
            AppError::Validation(m) | AppError::Conflict(m) | AppError::NotFound(m) | AppError::Forbidden(m) => ToolError(m),
            AppError::TooManyRequests(m) => ToolError(m),
            other => {
                eprintln!("mcp: {other:?}");
                ToolError("internal error, try again later".into())
            }
        }
    }
}

impl From<String> for ToolError {
    fn from(m: String) -> Self {
        ToolError(m)
    }
}

/// Какое право нужно инструменту. `None` — инструмент доступен всегда (`get_context`).
pub fn tool_right(name: &str) -> Option<Right> {
    Some(match name {
        "list_transactions" | "get_balances" => Right::read("transactions"),
        "get_budget" => Right::read("budget"),
        "list_upcoming_payments" => Right::read("planning"),
        "add_expense" | "add_income" | "add_transfer" | "update_transaction" | "pay_planned" => Right::write("transactions"),
        "skip_planned" | "add_plan" | "update_plan" => Right::write("planning"),
        "list_plans" => Right::read("planning"),
        "add_currency" | "update_currency" => Right::write("currencies"),
        "add_account" | "update_account" => Right::write("accounts"),
        "add_category" | "update_category" => Right::write("categories"),
        "list_rates" => Right::read("rates"),
        "set_rate" => Right::write("rates"),
        "list_limits" => Right::read("limits"),
        "set_limit" => Right::write("limits"),
        _ => return None,
    })
}

const MAX_PAGE: i64 = 200;
/// Сколько операций просматривать при фильтре по категории или тексту.
const SCAN_LIMIT: i64 = 1000;

pub struct McpService {
    repo: Arc<dyn GrantRepository>,
    deps: Deps,
    settings: Settings,
    limiter: RateLimiter,
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

fn lang_of(field: &str, value: Option<&str>, default: Lang) -> Result<Lang, String> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(default),
        Some(v) => Lang::parse(&v.to_ascii_lowercase()).ok_or_else(|| format!("{field}: expected ru, en or th, got '{v}'")),
    }
}

fn parse_id(field: &str, value: &str) -> Result<Uuid, ToolError> {
    Uuid::parse_str(value.trim()).map_err(|_| ToolError(format!("{field}: not an id: '{value}'")))
}

fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn deny(right: Right) -> ToolError {
    ToolError(format!(
        "permission denied: this assistant has no '{}:{}' right in this project. The user can change assistant permissions in the browser (profile → assistants); it cannot be changed from here.",
        right.feature,
        if right.write { "write" } else { "read" }
    ))
}

impl McpService {
    pub fn new(repo: Arc<dyn GrantRepository>, deps: Deps, settings: Settings) -> Self {
        Self { repo, deps, settings, limiter: RateLimiter::new() }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub async fn issuer(&self) -> Option<String> {
        self.deps.accounts.issuer().await
    }

    // ------------------------------------------------------------ вход на /mcp

    /// Проверить `Authorization: Bearer`: личный API-токен или access-токен провайдера входа.
    pub async fn authenticate(&self, bearer: Option<&str>, meta: RequestMeta) -> AppResult<Auth> {
        let Some(raw) = bearer.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(Auth::Missing) };
        let caller = if raw.starts_with(API_TOKEN_PREFIX) { self.token_caller(raw, meta).await? } else { self.grant_caller(raw, meta).await? };
        let Some(caller) = caller else { return Ok(Auth::Invalid) };
        if !self.limiter.allow(&caller.throttle_key(), self.settings.rate_limit) {
            return Ok(Auth::Throttled);
        }
        Ok(Auth::Ok(Box::new(caller)))
    }

    async fn token_caller(&self, raw: &str, meta: RequestMeta) -> AppResult<Option<Caller>> {
        let Some(identity) = self.deps.authenticator.authenticate(&Credential::ApiToken(raw.to_string())).await? else { return Ok(None) };
        let AuthVia::Token { token_id, project_id, scopes } = identity.via else { return Ok(None) };
        // Токен выпущен на один проект: если пользователь из него вышел — токен не действует.
        let Some(info) = self.deps.memberships.session(identity.user_id, Some(project_id)).await?.filter(|i| i.project_id == project_id) else {
            return Ok(None);
        };
        Ok(Some(Caller {
            user_id: identity.user_id,
            project_id,
            project_name: info.project_name,
            role: info.role,
            scopes,
            base_currency: info.base_currency,
            lang: info.user_lang,
            via: Via::Token { token_id },
            meta,
        }))
    }

    async fn grant_caller(&self, raw: &str, meta: RequestMeta) -> AppResult<Option<Caller>> {
        let Some((user_id, client_id)) = self.deps.accounts.verify_access_token(raw, &self.settings.resource).await? else { return Ok(None) };
        let unconfigured = |info: Option<kernel::auth::SessionInfo>, reason: Unconfigured| Caller {
            user_id,
            project_id: info.as_ref().map(|i| i.project_id).unwrap_or_default(),
            project_name: info.as_ref().map(|i| i.project_name.clone()).unwrap_or_default(),
            role: Role::Viewer,
            scopes: Vec::new(),
            base_currency: info.as_ref().map(|i| i.base_currency.clone()).unwrap_or_default(),
            lang: info.as_ref().map(|i| i.user_lang).unwrap_or_default(),
            via: Via::Unconfigured { client_id: client_id.clone(), reason },
            meta: meta.clone(),
        };
        let Some(grant) = self.repo.grant(user_id, &client_id).await? else {
            let info = self.deps.memberships.session(user_id, None).await?;
            return Ok(Some(unconfigured(info, Unconfigured::NoGrant)));
        };
        let info = self.deps.memberships.session(user_id, Some(grant.project_id)).await?;
        let Some(info) = info.filter(|i| i.project_id == grant.project_id) else {
            let info = self.deps.memberships.session(user_id, None).await?;
            return Ok(Some(unconfigured(info, Unconfigured::ProjectGone)));
        };
        if info.require_2fa && !grant.mfa {
            return Ok(Some(unconfigured(Some(info), Unconfigured::Mfa)));
        }
        let now = Utc::now();
        if grant.last_used_at.is_none_or(|t| now - t > Duration::minutes(1)) {
            self.repo.touch(grant.id, now).await?;
        }
        Ok(Some(Caller {
            user_id,
            project_id: grant.project_id,
            project_name: info.project_name,
            role: info.role,
            scopes: grant.scopes,
            base_currency: info.base_currency,
            lang: info.user_lang,
            via: Via::Grant { grant_id: grant.id, client_id },
            meta,
        }))
    }

    // ------------------------------------------------------------ доступы ассистентов (браузер)

    pub async fn grants(&self, user_id: Uuid) -> AppResult<Vec<Grant>> {
        self.repo.grants_of(user_id).await
    }

    pub async fn is_guest(&self, user_id: Uuid) -> AppResult<bool> {
        self.deps.accounts.is_guest(user_id).await
    }

    /// Требует ли проект второй фактор (для проекта, в котором пользователь состоит).
    pub async fn require_2fa(&self, user_id: Uuid, project_id: Uuid) -> AppResult<bool> {
        Ok(self.deps.memberships.session(user_id, Some(project_id)).await?.filter(|i| i.project_id == project_id).is_some_and(|i| i.require_2fa))
    }

    /// Открыть или изменить доступ ассистента. Вызывается только со страницы в браузере (сессия и
    /// CSRF): `role` и `require_2fa` — пользователя в выбранном проекте, `session_mfa` — был ли
    /// второй фактор при входе.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_grant(
        &self,
        user_id: Uuid,
        client_id: &str,
        project_id: Uuid,
        role: Role,
        require_2fa: bool,
        session_mfa: bool,
        scopes: &[String],
        meta: &RequestMeta,
    ) -> AppResult<Grant> {
        if self.deps.accounts.is_guest(user_id).await? {
            return Err(AppError::Forbidden("демо-гость: только просмотр".into()));
        }
        if require_2fa && !session_mfa {
            return Err(AppError::Forbidden("проект требует 2FA: войдите со вторым фактором и сохраните доступ снова".into()));
        }
        let mut scopes = normalize_grant_scopes(scopes);
        if role < Role::Editor {
            // Права не выше роли: VIEWER — только просмотр.
            scopes.retain(|s| s == "read");
        }
        let now = Utc::now();
        let existing = self.repo.grant(user_id, client_id).await?;
        let grant = Grant {
            id: existing.as_ref().map(|g| g.id).unwrap_or_else(Uuid::new_v4),
            user_id,
            client_id: client_id.to_string(),
            project_id,
            scopes: scopes.clone(),
            mfa: session_mfa,
            created_at: existing.as_ref().map(|g| g.created_at).unwrap_or(now),
            updated_at: now,
            last_used_at: existing.as_ref().and_then(|g| g.last_used_at),
        };
        self.repo.save(&grant).await?;
        self.deps
            .accounts
            .record(user_id, Some(project_id), "mcp_access_granted", meta, &[("client", client_id.to_string()), ("scopes", scopes.join(" "))])
            .await;
        Ok(grant)
    }

    pub async fn revoke_grant(&self, user_id: Uuid, client_id: &str, meta: &RequestMeta) -> AppResult<()> {
        let grant = self.repo.grant(user_id, client_id).await?;
        if self.repo.delete(user_id, client_id).await? {
            self.deps.accounts.record(user_id, grant.map(|g| g.project_id), "mcp_access_revoked", meta, &[("client", client_id.to_string())]).await;
        }
        Ok(())
    }

    pub async fn purge_project(&self, project_id: Uuid) -> AppResult<u64> {
        self.repo.delete_project(project_id).await
    }

    // ------------------------------------------------------------ инструменты

    fn check(&self, caller: &Caller, right: Right) -> Result<(), ToolError> {
        if let Via::Unconfigured { reason, .. } = &caller.via {
            return Err(ToolError(reason.message(&self.settings.setup_url())));
        }
        if caller.allows(right) { Ok(()) } else { Err(deny(right)) }
    }

    /// Какие инструменты показать вызывающему (`tools/list`).
    pub fn visible(&self, caller: &Caller, tool: &str) -> bool {
        match tool_right(tool) {
            None => true,
            Some(r) => !matches!(caller.via, Via::Unconfigured { .. }) && caller.allows(r),
        }
    }

    /// Событие журнала `mcp_write`: инструмент, клиент и что изменено (вид записи → код или id).
    async fn audit_write(&self, caller: &Caller, tool: &str, record: &[(&str, String)]) {
        let mut details = vec![("tool", tool.to_string()), ("client", caller.client().to_string())];
        details.extend(record.iter().cloned());
        self.deps.accounts.record(caller.user_id, Some(caller.project_id), "mcp_write", &caller.meta, &details).await;
    }

    async fn quick(&self, caller: &Caller, date: NaiveDate) -> Result<QuickInfo, ToolError> {
        Ok(self.deps.ledger.quick(caller.project_id, caller.user_id, &caller.base_currency, caller.lang, date).await?)
    }

    fn permissions(&self, caller: &Caller) -> Value {
        let can = |r: Right| caller.allows(r);
        json!({
            "read": can(Right::read("transactions")),
            "write_transactions": can(Right::write("transactions")),
            "write_planning": can(Right::write("planning")),
            "write_currencies": can(Right::write("currencies")),
            "write_accounts": can(Right::write("accounts")),
            "write_categories": can(Right::write("categories")),
            "write_rates": can(Right::write("rates")),
            "write_limits": can(Right::write("limits")),
            "delete": false,
            "how_to_change": format!("Only the user can change permissions, in a browser: {}", self.settings.setup_url()),
        })
    }

    /// Проект, валюта, язык, счета с остатками, дерево категорий, частые категории, права.
    pub async fn get_context(&self, caller: &Caller) -> ToolResult {
        if let Via::Unconfigured { reason, .. } = &caller.via {
            return Ok(ToolOutput { summary: reason.message(&self.settings.setup_url()), data: json!({"configured": false, "setup_url": self.settings.setup_url()}) });
        }
        let today = today();
        let mut base = json!({
            "project": caller.project_name,
            "role": caller.role.as_str(),
            "base_currency": caller.base_currency,
            "user_language": caller.lang.code(),
            "today": today,
            "permissions": self.permissions(caller),
        });
        // Валюты — для просмотра и для ссылок из счетов и курсов.
        if [Right::read("currencies"), Right::write("accounts"), Right::write("rates")].into_iter().any(|r| caller.allows(r)) {
            let list = self.deps.directories.currencies(caller.project_id).await?;
            base["currencies"] = list.iter().filter(|c| !c.archived).map(|c| json!({"code": c.code, "name": c.name.show(caller.lang), "symbol": c.symbol})).collect();
        }
        if !caller.allows(Right::read("transactions")) {
            return Ok(ToolOutput { summary: "No read access to transactions in this project.".into(), data: base });
        }
        let q = self.quick(caller, today).await?;
        let balances = self.deps.ledger.balances(caller.project_id, today).await?;
        let balance = |code: &str| balances.iter().find(|b| b.0 == code).map(|b| b.1).unwrap_or_default();
        let cat = |c: &CategoryInfo| {
            json!({"code": c.code, "title": c.title, "names": c.search, "parent": c.parent_code, "depth": c.depth, "month_limit": c.limit, "month_available": c.available})
        };
        let mut data = base;
        data["default_account"] = json!(q.default_account);
        data["accounts"] = q.accounts.iter().map(|a| json!({"code": a.code, "title": a.title, "currency": a.currency, "symbol": a.symbol, "balance": balance(&a.code)})).collect();
        data["expense_categories"] = q.categories.iter().filter(|c| c.kind == "EXPENSE").map(cat).collect();
        data["income_categories"] = q.categories.iter().filter(|c| c.kind == "INCOME").map(cat).collect();
        data["frequent_expense_categories"] = json!(q.frequent_expense);
        data["frequent_income_categories"] = json!(q.frequent_income);
        Ok(ToolOutput {
            summary: format!(
                "Project '{}', base currency {}, {} accounts, {} categories. Amounts of transactions are in the account currency.",
                caller.project_name,
                caller.base_currency,
                q.accounts.len(),
                q.categories.len()
            ),
            data,
        })
    }

    pub async fn list_transactions(&self, caller: &Caller, a: ListTransactionsArgs) -> ToolResult {
        self.check(caller, Right::read("transactions"))?;
        let today = today();
        let to = parse_date("to", a.to.as_deref(), today)?;
        let from = parse_date("from", a.from.as_deref(), to - Days::new(30))?;
        let limit = i64::from(a.limit.unwrap_or(50)).clamp(1, MAX_PAGE);
        let offset = i64::from(a.offset.unwrap_or(0));
        let need_quick = a.account.is_some() || a.category.is_some();
        let q = if need_quick { self.quick(caller, today).await? } else { QuickInfo::default() };
        let account_code = match a.account.as_deref() {
            Some(acc) => Some(resolve("account", acc, &q.accounts, |x| &x.code, |x| x.title.clone())?.code.clone()),
            None => None,
        };
        let kind = match clean(a.kind).map(|k| k.to_ascii_uppercase()) {
            None => None,
            Some(k) if ["EXPENSE", "INCOME", "TRANSFER"].contains(&k.as_str()) => Some(k),
            Some(k) => return Err(ToolError(format!("kind: expected expense, income or transfer, got '{k}'"))),
        };
        let filter = TxFilter { from: Some(from), to: Some(to), account_code, kind };
        let category_set: Option<HashSet<String>> = match a.category.as_deref() {
            Some(c) => {
                let root = resolve("category", c, &q.categories, |x| &x.code, |x| format!("{}\u{1f}{}", x.title, x.search))?.code.clone();
                Some(subtree(&q.categories, &root))
            }
            None => None,
        };
        let text = clean(a.text).map(|t| t.to_lowercase());
        let (items, total) = if category_set.is_none() && text.is_none() {
            self.deps.ledger.list(caller.project_id, filter, limit, offset).await?
        } else {
            let mut found = Vec::new();
            let mut scanned = 0;
            while scanned < SCAN_LIMIT {
                let (page, all) = self.deps.ledger.list(caller.project_id, filter.clone(), MAX_PAGE, scanned).await?;
                let n = page.len() as i64;
                found.extend(page.into_iter().filter(|t| {
                    category_set.as_ref().is_none_or(|set| t.lines.iter().any(|l| set.contains(&l.category_code)) || t.fee_category_code.as_ref().is_some_and(|f| set.contains(f)))
                        && text.as_ref().is_none_or(|needle| {
                            [t.description.as_deref(), t.description_ru.as_deref(), t.description_en.as_deref(), t.description_th.as_deref()]
                                .into_iter()
                                .flatten()
                                .chain(t.lines.iter().filter_map(|l| l.note.as_deref()))
                                .any(|s| s.to_lowercase().contains(needle.as_str()))
                        })
                }));
                scanned += n;
                if n < MAX_PAGE || scanned >= all {
                    break;
                }
            }
            let total = found.len() as i64;
            (found.into_iter().skip(offset as usize).take(limit as usize).collect(), total)
        };
        let shown = items.len();
        let data: Vec<Value> = items.iter().map(|t| tx_json(t, caller.lang)).collect();
        Ok(ToolOutput {
            summary: format!("{shown} of {total} transactions from {from} to {to} (newest first, offset {offset})."),
            data: json!({"from": from, "to": to, "total": total, "offset": offset, "transactions": data}),
        })
    }

    pub async fn get_balances(&self, caller: &Caller, a: DateArgs) -> ToolResult {
        self.check(caller, Right::read("transactions"))?;
        let today = today();
        let date = parse_date("date", a.date.as_deref(), today)?;
        let q = self.quick(caller, today).await?;
        let balances = self.deps.ledger.balances(caller.project_id, date).await?;
        let rows: Vec<Value> = q
            .accounts
            .iter()
            .map(|a| json!({"code": a.code, "title": a.title, "currency": a.currency, "balance": balances.iter().find(|b| b.0 == a.code).map(|b| b.1).unwrap_or_default()}))
            .collect();
        Ok(ToolOutput { summary: format!("Balances of {} accounts at the end of {date}.", rows.len()), data: json!({"date": date, "accounts": rows}) })
    }

    pub async fn get_budget(&self, caller: &Caller, a: BudgetArgs) -> ToolResult {
        self.check(caller, Right::read("budget"))?;
        let view = clean(a.view).map(|v| v.to_ascii_lowercase()).unwrap_or_else(|| "month".into());
        if !["week", "month", "year"].contains(&view.as_str()) {
            return Err(ToolError(format!("view: expected week, month or year, got '{view}'")));
        }
        let date = parse_date("date", a.date.as_deref(), today())?;
        let r = self.deps.budget.report(caller.project_id, &caller.base_currency, &view, date, caller.lang).await?;
        let lines: Vec<Value> = r
            .lines
            .iter()
            .filter(|l| !(l.fact.is_zero() && l.planned.is_zero() && l.limit.is_none()))
            .map(|l| {
                json!({"code": l.code, "title": l.title, "kind": l.kind, "parent": l.parent_code, "depth": l.depth, "limit": l.limit, "fact": l.fact, "planned": l.planned, "forecast": l.forecast, "available": l.available, "level": l.level})
            })
            .collect();
        Ok(ToolOutput {
            summary: format!(
                "Budget {} – {} in {}: expenses {} (+{} planned), income {} (+{} planned). Category sums include subcategories; level warn = ≥80% of limit, over = above limit.",
                r.from, r.to, r.base_currency, r.expense_fact, r.expense_planned, r.income_fact, r.income_planned
            ),
            data: json!({
                "from": r.from, "to": r.to, "base_currency": r.base_currency,
                "expense_fact": r.expense_fact, "expense_planned": r.expense_planned,
                "income_fact": r.income_fact, "income_planned": r.income_planned,
                "categories": lines, "missing_rates": r.missing_rates,
            }),
        })
    }

    pub async fn list_upcoming_payments(&self, caller: &Caller, a: UpcomingArgs) -> ToolResult {
        self.check(caller, Right::read("planning"))?;
        let today = today();
        let from = parse_date("from", a.from.as_deref(), today)?;
        let to = parse_date("to", a.to.as_deref(), from + Days::new(30))?;
        let list = self.deps.plans.occurrences(caller.project_id, from, to, today, caller.lang).await?;
        let rows: Vec<Value> = list
            .iter()
            .map(|o| {
                json!({"plan_id": o.plan_id, "name": o.name, "date": o.date, "status": o.status, "kind": o.kind, "amount": o.amount, "account": o.account_code, "category": o.category_code, "to_account": o.to_account_code, "transaction_id": o.transaction_id})
            })
            .collect();
        let open = list.iter().filter(|o| o.status == "EXPECTED" || o.status == "OVERDUE").count();
        Ok(ToolOutput { summary: format!("{} scheduled payments from {from} to {to}, {open} not settled yet.", rows.len()), data: json!({"from": from, "to": to, "payments": rows}) })
    }

    /// Строки операции из параметров: позиции или сумма+категория.
    fn lines(&self, q: &QuickInfo, kind: &str, amount: Option<&str>, category: Option<&str>, items: Option<Vec<ItemArgs>>) -> Result<Vec<TxLine>, ToolError> {
        let find = |c: &str| category_code(q, kind, "category", c);
        match (items.filter(|i| !i.is_empty()), amount, category) {
            (Some(items), None, None) => items
                .into_iter()
                .enumerate()
                .map(|(i, it)| Ok(TxLine { category_code: find(&it.category)?, amount: parse_amount(&format!("items[{i}].amount"), &it.amount)?, note: clean(it.note) }))
                .collect(),
            (None, Some(amount), Some(category)) => Ok(vec![TxLine { category_code: find(category)?, amount: parse_amount("amount", amount)?, note: None }]),
            (None, _, _) => Err(ToolError("pass amount and category, or items".into())),
            (Some(_), _, _) => Err(ToolError("pass either items or amount+category, not both".into())),
        }
    }

    fn account(&self, q: &QuickInfo, field: &str, value: Option<&str>) -> Result<String, ToolError> {
        match value {
            Some(v) => Ok(resolve(field, v, &q.accounts, |x| &x.code, |x| x.title.clone())?.code.clone()),
            None => q.default_account.clone().ok_or_else(|| ToolError(format!("{field}: no default account; pass an account code"))),
        }
    }

    fn describe(&self, caller: &Caller, draft: &mut TxDraft, description: Option<String>, lang: Option<String>, tr: Option<Translations>) -> Result<(), ToolError> {
        let tr = tr.unwrap_or_default();
        draft.description = clean(description);
        draft.description_lang = lang_of("description_lang", lang.as_deref(), caller.lang)?;
        draft.description_ru = clean(tr.ru);
        draft.description_en = clean(tr.en);
        draft.description_th = clean(tr.th);
        Ok(())
    }

    async fn add_flow(&self, caller: &Caller, kind: &str, a: AddFlowArgs, tool: &str) -> ToolResult {
        self.check(caller, Right::write("transactions"))?;
        let today = today();
        let date = parse_date("date", a.date.as_deref(), today)?;
        let q = self.quick(caller, date).await?;
        let lines = self.lines(&q, kind, a.amount.as_deref(), a.category.as_deref(), a.items)?;
        let by_default = a.account.is_none();
        let account_code = self.account(&q, "account", a.account.as_deref())?;
        let mut draft = blank_draft(kind, date, account_code, caller.lang);
        draft.refund = kind == "EXPENSE" && a.refund.unwrap_or(false);
        draft.lines = lines;
        self.describe(caller, &mut draft, a.description, a.description_lang, a.translations)?;
        let tx = self.deps.ledger.create(caller.project_id, caller.user_id, draft).await?;
        self.audit_write(caller, tool, &[("transaction", tx.id.to_string())]).await;
        let mut data = json!({"transaction": tx_json(&tx, caller.lang)});
        let mut summary = format!("Saved {} {} {} on {} from account {}.", kind.to_lowercase(), tx.total.normalize(), tx.currency, tx.occurred_on, tx.account_code);
        if by_default && tx.currency != caller.base_currency {
            // Счёт по умолчанию — последний использованный; если он в другой валюте, легко ошибиться.
            summary.push_str(&format!(
                " Note: the default account {} is in {}, not the base currency {} — if the user meant another currency, move it with update_transaction (account).",
                tx.account_code, tx.currency, caller.base_currency
            ));
        }
        if kind == "EXPENSE" {
            // Остаток лимитов после записи — как в диалоге быстрого расхода.
            let after = self.quick(caller, date).await?;
            let left: Vec<Value> = tx
                .lines
                .iter()
                .filter_map(|l| after.categories.iter().find(|c| c.code == l.category_code))
                .filter(|c| c.available.is_some())
                .map(|c| json!({"category": c.code, "title": c.title, "month_limit": c.limit, "month_available": c.available}))
                .collect();
            let first = tx.lines.iter().filter_map(|l| after.categories.iter().find(|c| c.code == l.category_code)).find_map(|c| c.available.map(|a| (c, a)));
            if let Some((c, available)) = first {
                summary.push_str(&if available < rust_decimal::Decimal::ZERO {
                    format!(" Monthly limit of '{}' is exceeded by {} {} (with parent limits).", c.title, (-available).normalize(), caller.base_currency)
                } else {
                    format!(" Left this month in '{}': {} {} (with parent limits).", c.title, available.normalize(), caller.base_currency)
                });
            }
            data["limits_after"] = json!(left);
        }
        Ok(ToolOutput { summary, data })
    }

    pub async fn add_expense(&self, caller: &Caller, a: AddFlowArgs) -> ToolResult {
        self.add_flow(caller, "EXPENSE", a, "add_expense").await
    }

    pub async fn add_income(&self, caller: &Caller, a: AddFlowArgs) -> ToolResult {
        if a.refund.unwrap_or(false) {
            return Err(ToolError("refund applies to expenses only".into()));
        }
        self.add_flow(caller, "INCOME", a, "add_income").await
    }

    pub async fn add_transfer(&self, caller: &Caller, a: AddTransferArgs) -> ToolResult {
        self.check(caller, Right::write("transactions"))?;
        let today = today();
        let date = parse_date("date", a.date.as_deref(), today)?;
        let q = self.quick(caller, date).await?;
        let from = self.account(&q, "from_account", Some(&a.from_account))?;
        let to = self.account(&q, "to_account", Some(&a.to_account))?;
        let mut draft = blank_draft("TRANSFER", date, from, caller.lang);
        draft.to_account_code = Some(to);
        draft.amount = Some(parse_amount("amount", &a.amount)?);
        draft.to_amount = a.to_amount.as_deref().map(|v| parse_amount("to_amount", v)).transpose()?;
        draft.fee_amount = a.fee_amount.as_deref().map(|v| parse_amount("fee_amount", v)).transpose()?;
        draft.fee_category_code = a.fee_category.as_deref().map(|c| category_code(&q, "EXPENSE", "fee_category", c)).transpose()?;
        self.describe(caller, &mut draft, a.description, a.description_lang, a.translations)?;
        let tx = self.deps.ledger.create(caller.project_id, caller.user_id, draft).await?;
        self.audit_write(caller, "add_transfer", &[("transaction", tx.id.to_string())]).await;
        let received = match (&tx.to_amount, &tx.to_currency) {
            (Some(x), Some(c)) => format!(", received {x} {c}"),
            _ => String::new(),
        };
        Ok(ToolOutput {
            summary: format!(
                "Saved transfer {} {} from {} to {}{received} on {}.",
                tx.total,
                tx.currency,
                tx.account_code,
                tx.to_account_code.clone().unwrap_or_default(),
                tx.occurred_on
            ),
            data: json!({"transaction": tx_json(&tx, caller.lang)}),
        })
    }

    pub async fn update_transaction(&self, caller: &Caller, a: UpdateTransactionArgs) -> ToolResult {
        self.check(caller, Right::write("transactions"))?;
        let id = Uuid::parse_str(a.id.trim()).map_err(|_| ToolError(format!("id: not a transaction id: '{}'", a.id)))?;
        let old = self.deps.ledger.get(caller.project_id, id).await?;
        if old.kind == "OPENING" {
            return Err(ToolError("opening balances are changed in the account settings, not here".into()));
        }
        let mut draft = old.draft();
        if let Some(d) = a.date.as_deref() {
            draft.occurred_on = parse_date("date", Some(d), old.occurred_on)?;
        }
        let q = self.quick(caller, draft.occurred_on).await?;
        if let Some(acc) = a.account.as_deref() {
            draft.account_code = self.account(&q, "account", Some(acc))?;
        }
        if old.kind == "TRANSFER" {
            if a.items.is_some() || a.category.is_some() {
                return Err(ToolError("transfers have no categories; change amount, accounts or to_amount".into()));
            }
            if let Some(v) = a.amount.as_deref() {
                draft.amount = Some(parse_amount("amount", v)?);
            }
            if let Some(acc) = a.to_account.as_deref() {
                draft.to_account_code = Some(self.account(&q, "to_account", Some(acc))?);
            }
            if let Some(v) = a.to_amount.as_deref() {
                draft.to_amount = Some(parse_amount("to_amount", v)?);
            }
        } else {
            if a.to_account.is_some() || a.to_amount.is_some() {
                return Err(ToolError("to_account and to_amount apply to transfers only".into()));
            }
            match (a.items, a.amount.as_deref(), a.category.as_deref()) {
                (Some(items), None, None) => draft.lines = self.lines(&q, &old.kind, None, None, Some(items))?,
                (Some(_), _, _) => return Err(ToolError("pass either items or amount/category, not both".into())),
                (None, None, None) => {}
                (None, amount, category) => {
                    if draft.lines.len() != 1 {
                        return Err(ToolError("this transaction has several lines; pass items to replace them".into()));
                    }
                    let line = &mut draft.lines[0];
                    if let Some(v) = amount {
                        line.amount = parse_amount("amount", v)?;
                    }
                    if let Some(c) = category {
                        line.category_code = category_code(&q, &old.kind, "category", c)?;
                    }
                }
            }
        }
        if a.description.is_some() || a.translations.is_some() || a.description_lang.is_some() {
            let description = a.description.or(old.description.clone());
            let lang = a.description_lang.or_else(|| Some(old.description_lang.code().to_string()));
            self.describe(caller, &mut draft, description, lang, a.translations)?;
        }
        let tx = self.deps.ledger.update(caller.project_id, id, draft).await?;
        self.audit_write(caller, "update_transaction", &[("transaction", tx.id.to_string())]).await;
        Ok(ToolOutput { summary: format!("Updated transaction {} ({} {} on {}).", tx.id, tx.total, tx.currency, tx.occurred_on), data: json!({"transaction": tx_json(&tx, caller.lang)}) })
    }

    pub async fn pay_planned(&self, caller: &Caller, a: PayPlannedArgs) -> ToolResult {
        self.check(caller, Right::write("transactions"))?;
        let plan_id = Uuid::parse_str(a.plan_id.trim()).map_err(|_| ToolError(format!("plan_id: not a plan id: '{}'", a.plan_id)))?;
        let today = today();
        let date = parse_date("date", Some(&a.date), today)?;
        let occ = self
            .deps
            .plans
            .occurrences(caller.project_id, date, date, today, caller.lang)
            .await?
            .into_iter()
            .find(|o| o.plan_id == plan_id)
            .ok_or_else(|| ToolError(format!("no scheduled payment of plan {plan_id} on {date}")))?;
        if occ.status == "PAID" {
            return Err(ToolError(format!("already paid: transaction {}", occ.transaction_id.map(|t| t.to_string()).unwrap_or_default())));
        }
        let paid_on = parse_date("paid_on", a.paid_on.as_deref(), today)?;
        let q = self.quick(caller, paid_on).await?;
        let account = match a.account.as_deref() {
            Some(acc) => self.account(&q, "account", Some(acc))?,
            None => occ.account_code.clone(),
        };
        let amount = match a.amount.as_deref() {
            Some(v) => parse_amount("amount", v)?,
            None => occ.amount,
        };
        let mut draft = blank_draft(&occ.kind, paid_on, account, caller.lang);
        if occ.kind == "TRANSFER" {
            draft.to_account_code = occ.to_account_code.clone();
            draft.amount = Some(amount);
        } else {
            let category = occ.category_code.clone().ok_or_else(|| ToolError("the plan has no category".into()))?;
            draft.lines = vec![TxLine { category_code: category, amount, note: None }];
        }
        draft.description = Some(occ.name.clone());
        draft.plan_id = Some(plan_id);
        draft.plan_date = Some(date);
        let tx = self.deps.ledger.create(caller.project_id, caller.user_id, draft).await?;
        self.audit_write(caller, "pay_planned", &[("transaction", tx.id.to_string())]).await;
        Ok(ToolOutput {
            summary: format!("Paid '{}' scheduled for {date}: {} {} on {}.", occ.name, tx.total, tx.currency, tx.occurred_on),
            data: json!({"transaction": tx_json(&tx, caller.lang)}),
        })
    }

    pub async fn skip_planned(&self, caller: &Caller, a: SkipPlannedArgs) -> ToolResult {
        self.check(caller, Right::write("planning"))?;
        let plan_id = Uuid::parse_str(a.plan_id.trim()).map_err(|_| ToolError(format!("plan_id: not a plan id: '{}'", a.plan_id)))?;
        let date = parse_date("date", Some(&a.date), today())?;
        self.deps.plans.skip(caller.project_id, caller.user_id, plan_id, date).await?;
        self.audit_write(caller, "skip_planned", &[("plan", plan_id.to_string()), ("date", date.to_string())]).await;
        Ok(ToolOutput { summary: format!("Skipped the payment of plan {plan_id} scheduled for {date}."), data: json!({"plan_id": plan_id, "date": date, "status": "SKIPPED"}) })
    }

    // ------------------------------------------------------------ справочники: валюты, счета, категории

    /// Название новой записи: текст, язык (по умолчанию — язык пользователя), переводы.
    fn new_name(&self, caller: &Caller, text: &str, lang: Option<&str>, tr: Option<NameTranslations>) -> Result<Name, ToolError> {
        let tr = tr.unwrap_or_default();
        Ok(Name::new("name", text, lang_of("name_lang", lang, caller.lang)?, tr.ru, tr.en, tr.th)?)
    }

    pub async fn add_currency(&self, caller: &Caller, a: AddCurrencyArgs) -> ToolResult {
        self.check(caller, Right::write("currencies"))?;
        let code = parse_code("code", &a.code)?;
        let list = self.deps.directories.currencies(caller.project_id).await?;
        if let Some(x) = list.iter().find(|x| x.code == code) {
            return Err(taken("currency", &x.code, x.name.show(caller.lang), x.archived, "update_currency"));
        }
        let currency = Currency {
            code,
            name: self.new_name(caller, &a.name, a.name_lang.as_deref(), a.translations)?,
            symbol: required("symbol", &a.symbol)?,
            minor_units: minor_units(a.minor_units.unwrap_or(2))?,
            archived: false,
        };
        let c = self.deps.directories.create_currency(caller.project_id, &currency).await?;
        self.audit_write(caller, "add_currency", &[("currency", c.code.clone())]).await;
        Ok(ToolOutput {
            summary: format!(
                "Added currency {} '{}' ({}). To convert it in budgets, set its rate with set_rate (units of {} for 1 {}).",
                c.code,
                c.name.show(caller.lang),
                c.symbol,
                caller.base_currency,
                c.code
            ),
            data: json!({"currency": currency_json(&c)}),
        })
    }

    pub async fn update_currency(&self, caller: &Caller, a: UpdateCurrencyArgs) -> ToolResult {
        self.check(caller, Right::write("currencies"))?;
        if a.name.is_none() && a.name_lang.is_none() && a.translations.is_none() && a.symbol.is_none() && a.minor_units.is_none() {
            return Err(nothing_to_change());
        }
        let list = self.deps.directories.currencies(caller.project_id).await?;
        let old = find_record("code", &a.code, &list)?;
        let mut c = old.clone();
        c.name = renamed(caller, &old.name, a.name.as_deref(), a.name_lang.as_deref(), a.translations)?;
        if let Some(v) = a.symbol.as_deref() {
            c.symbol = required("symbol", v)?;
        }
        if let Some(m) = a.minor_units {
            c.minor_units = minor_units(m)?;
        }
        let c = self.deps.directories.update_currency(caller.project_id, &c).await?;
        self.audit_write(caller, "update_currency", &[("currency", c.code.clone())]).await;
        Ok(ToolOutput { summary: format!("Updated currency {} '{}' ({}).", c.code, c.name.show(caller.lang), c.symbol), data: json!({"currency": currency_json(&c)}) })
    }

    /// Начальный остаток из параметров (со знаком) и его дата. Это операция в журнале — нужно ещё право на операции.
    fn opening(&self, caller: &Caller, amount: Option<&str>, date: Option<&str>) -> Result<Option<(Decimal, NaiveDate)>, ToolError> {
        let Some(amount) = amount else {
            return match clean(date.map(String::from)) {
                Some(_) => Err(ToolError("opening_date: pass opening_balance with it".into())),
                None => Ok(None),
            };
        };
        self.check(caller, Right::write("transactions")).map_err(|e| ToolError(format!("opening_balance: {}", e.0)))?;
        Ok(Some((parse_signed_amount("opening_balance", amount)?, parse_date("opening_date", date, today())?)))
    }

    /// Задать начальный остаток уже сохранённого счёта; ошибка — предупреждением в итоге (счёт уже изменён).
    async fn apply_opening(&self, caller: &Caller, account: &Account, opening: Option<(Decimal, NaiveDate)>, record: &mut Vec<(&str, String)>, summary: &mut String) {
        let Some((amount, date)) = opening else { return };
        match self.deps.directories.set_opening(caller.project_id, caller.user_id, &account.code, amount, date).await {
            Ok(()) if amount.is_zero() => {
                record.push(("opening_balance", "0".into()));
                summary.push_str(" Opening balance removed.");
            }
            Ok(()) => {
                record.push(("opening_balance", amount.to_string()));
                summary.push_str(&format!(" Opening balance {} {} on {date}.", amount.normalize(), account.currency));
            }
            Err(e) => {
                let ToolError(m) = ToolError::from(e);
                summary.push_str(&format!(" WARNING: the opening balance was NOT set: {m}. Fix it and retry with update_account (opening_balance)."));
            }
        }
    }

    pub async fn add_account(&self, caller: &Caller, a: AddAccountArgs) -> ToolResult {
        self.check(caller, Right::write("accounts"))?;
        let code = parse_code("code", &a.code)?;
        let opening = self.opening(caller, a.opening_balance.as_deref(), a.opening_date.as_deref())?.filter(|(amount, _)| !amount.is_zero());
        let kind = parse_choice("kind", &a.kind, &ACCOUNT_KINDS)?;
        let name = self.new_name(caller, &a.name, a.name_lang.as_deref(), a.translations)?;
        let dirs = &self.deps.directories;
        let currencies = dirs.currencies(caller.project_id).await?;
        let currency = find_ref("currency", &a.currency, &currencies)?.code.clone();
        let accounts = dirs.accounts(caller.project_id).await?;
        if let Some(x) = accounts.iter().find(|x| x.code == code) {
            return Err(taken("account", &x.code, x.name.show(caller.lang), x.archived, "update_account"));
        }
        let acc = dirs.create_account(caller.project_id, &Account { code, name, kind, currency, holder_user_id: None, archived: false }).await?;
        let mut record = vec![("account", acc.code.clone())];
        let mut summary = format!("Added account {} '{}' ({}, {}).", acc.code, acc.name.show(caller.lang), acc.kind.to_lowercase(), acc.currency);
        self.apply_opening(caller, &acc, opening, &mut record, &mut summary).await;
        self.audit_write(caller, "add_account", &record).await;
        Ok(ToolOutput { summary, data: json!({"account": account_json(&acc)}) })
    }

    pub async fn update_account(&self, caller: &Caller, a: UpdateAccountArgs) -> ToolResult {
        self.check(caller, Right::write("accounts"))?;
        let opening = self.opening(caller, a.opening_balance.as_deref(), a.opening_date.as_deref())?;
        let fields = a.name.is_some() || a.name_lang.is_some() || a.translations.is_some() || a.kind.is_some() || a.currency.is_some();
        if !fields && opening.is_none() {
            return Err(nothing_to_change());
        }
        let dirs = &self.deps.directories;
        let accounts = dirs.accounts(caller.project_id).await?;
        let old = find_record("code", &a.code, &accounts)?;
        let mut acc = old.clone();
        if fields {
            acc.name = renamed(caller, &old.name, a.name.as_deref(), a.name_lang.as_deref(), a.translations)?;
            if let Some(k) = a.kind.as_deref() {
                acc.kind = parse_choice("kind", k, &ACCOUNT_KINDS)?;
            }
            if let Some(c) = a.currency.as_deref() {
                let currencies = dirs.currencies(caller.project_id).await?;
                acc.currency = find_ref("currency", c, &currencies)?.code.clone();
            }
            acc = dirs.update_account(caller.project_id, &acc).await?;
        }
        let mut record = vec![("account", acc.code.clone())];
        let mut summary = format!("Updated account {} '{}' ({}, {}).", acc.code, acc.name.show(caller.lang), acc.kind.to_lowercase(), acc.currency);
        match (fields, opening) {
            // Меняется только начальный остаток — его ошибка и есть ошибка инструмента.
            (false, Some((amount, date))) => {
                dirs.set_opening(caller.project_id, caller.user_id, &acc.code, amount, date).await?;
                summary = if amount.is_zero() {
                    format!("Removed the opening balance of account {}.", acc.code)
                } else {
                    format!("Opening balance of account {}: {} {} on {date}.", acc.code, amount.normalize(), acc.currency)
                };
                record.push(("opening_balance", amount.to_string()));
            }
            _ => self.apply_opening(caller, &acc, opening, &mut record, &mut summary).await,
        }
        self.audit_write(caller, "update_account", &record).await;
        Ok(ToolOutput { summary, data: json!({"account": account_json(&acc)}) })
    }

    /// Код активной категории вида `kind` по коду или названию на любом языке.
    async fn category_of_kind(&self, caller: &Caller, kind: &str, field: &str, value: &str) -> Result<String, ToolError> {
        let all = self.deps.directories.categories(caller.project_id).await?;
        let pool: Vec<&Category> = all.iter().filter(|c| c.kind == kind).collect();
        Ok(find_ref(field, value, &pool)?.code.clone())
    }

    pub async fn add_category(&self, caller: &Caller, a: AddCategoryArgs) -> ToolResult {
        self.check(caller, Right::write("categories"))?;
        let code = parse_code("code", &a.code)?;
        let kind = a.kind.as_deref().map(|k| parse_choice("kind", k, &CATEGORY_KINDS)).transpose()?;
        let name = self.new_name(caller, &a.name, a.name_lang.as_deref(), a.translations)?;
        let dirs = &self.deps.directories;
        let all = dirs.categories(caller.project_id).await?;
        if let Some(x) = all.iter().find(|x| x.code == code) {
            return Err(taken("category", &x.code, x.name.show(caller.lang), x.archived, "update_category"));
        }
        let parent = match clean(a.parent) {
            Some(p) => Some(parent_of(&all, &p, kind.as_deref(), None)?),
            None => None,
        };
        let kind = match (kind, parent) {
            (Some(k), _) => k,
            (None, Some(p)) => p.kind.clone(),
            (None, None) => return Err(ToolError("kind: expense or income is required for a top-level category".into())),
        };
        let parent_code = parent.map(|p| p.code.clone());
        let sort_order = a.sort_order.unwrap_or_else(|| next_sort_order(&all, parent_code.as_deref(), &kind));
        let c = dirs.create_category(caller.project_id, &Category { code, name, kind, parent_code, sort_order, archived: false }).await?;
        self.audit_write(caller, "add_category", &[("category", c.code.clone())]).await;
        let place = match parent {
            Some(p) => format!("under '{}' ({})", p.name.show(caller.lang), p.code),
            None => "at the top level".into(),
        };
        Ok(ToolOutput {
            summary: format!("Added {} category {} '{}' {place}.", c.kind.to_lowercase(), c.code, c.name.show(caller.lang)),
            data: json!({"category": category_json(&c)}),
        })
    }

    pub async fn update_category(&self, caller: &Caller, a: UpdateCategoryArgs) -> ToolResult {
        self.check(caller, Right::write("categories"))?;
        if a.name.is_none() && a.name_lang.is_none() && a.translations.is_none() && a.parent.is_none() && a.sort_order.is_none() {
            return Err(nothing_to_change());
        }
        let all = self.deps.directories.categories(caller.project_id).await?;
        let old = find_record("code", &a.code, &all)?;
        let mut c = old.clone();
        c.name = renamed(caller, &old.name, a.name.as_deref(), a.name_lang.as_deref(), a.translations)?;
        if let Some(p) = a.parent.as_deref() {
            c.parent_code = match p.trim() {
                "" => None,
                p => Some(parent_of(&all, p, Some(&old.kind), Some(&old.code))?.code.clone()),
            };
            if c.parent_code != old.parent_code {
                // Перенесённая категория встаёт последней среди новых соседей.
                c.sort_order = next_sort_order(&all, c.parent_code.as_deref(), &c.kind);
            }
        }
        if let Some(o) = a.sort_order {
            c.sort_order = o;
        }
        let c = self.deps.directories.update_category(caller.project_id, &c).await?;
        self.audit_write(caller, "update_category", &[("category", c.code.clone())]).await;
        let mut summary = format!("Updated category {} '{}'.", c.code, c.name.show(caller.lang));
        if c.parent_code != old.parent_code {
            summary.push_str(&match &c.parent_code {
                Some(p) => format!(" Moved under {p}."),
                None => " Moved to the top level.".into(),
            });
        }
        Ok(ToolOutput { summary, data: json!({"category": category_json(&c)}) })
    }

    // ------------------------------------------------------------ курсы валют

    pub async fn set_rate(&self, caller: &Caller, a: SetRateArgs) -> ToolResult {
        self.check(caller, Right::write("rates"))?;
        let base = &caller.base_currency;
        let (entered, inverted) = match (clean(a.rate), clean(a.inverse_rate)) {
            (Some(r), None) => (parse_rate("rate", &r)?, false),
            (None, Some(r)) => (parse_rate("inverse_rate", &r)?, true),
            (Some(_), Some(_)) => {
                return Err(ToolError(format!(
                    "Pass exactly one of rate (units of {base} for 1 unit of the currency) and inverse_rate (units of the currency for 1 {base}), not both."
                )));
            }
            (None, None) => {
                return Err(ToolError(format!(
                    "Pass rate (units of {base} for 1 unit of the currency) or inverse_rate (units of the currency for 1 {base})."
                )));
            }
        };
        let date = parse_date("date", a.date.as_deref(), today())?;
        let currencies = self.deps.directories.currencies(caller.project_id).await?;
        let currency = find_ref("currency", &a.currency, &currencies)?.code.clone();
        if &currency == base {
            return Err(ToolError(format!(
                "currency: {base} is the base currency of the project — its rate is always 1. Pass the other currency, with rate = units of {base} for 1 unit of it."
            )));
        }
        // Курсы этой валюты до даты включительно, новые сначала: курс на эту дату заменяется.
        let history: Vec<Rate> = self.deps.rates.list(caller.project_id, None, Some(date)).await?.into_iter().filter(|r| r.currency == currency).collect();
        let old = history.iter().find(|r| r.date == date);
        let previous = history.iter().filter(|r| r.date < date).max_by_key(|r| r.date);
        let draft = Rate::entered(old.map(|r| r.id).unwrap_or_default(), date, currency, entered, inverted);
        let saved = match old {
            Some(_) => self.deps.rates.update(caller.project_id, &draft).await?,
            None => self.deps.rates.create(caller.project_id, &draft).await?,
        };
        self.audit_write(caller, "set_rate", &[("rate", saved.id.to_string()), ("currency", saved.currency.clone()), ("date", saved.date.to_string())]).await;
        let mut summary = format!("Rate on {}: {} (the base currency is {base}).", saved.date, rate_text(&saved, base));
        if let Some(o) = old {
            summary.push_str(&format!(" Replaced the previous rate {} for that date.", rate_main(o, base)));
        }
        if let Some(p) = previous.filter(|p| saved.rate > p.rate * Decimal::from(5) || saved.rate * Decimal::from(5) < p.rate) {
            summary.push_str(&format!(
                " Check the direction: the rate on {} was {} — more than 5 times different. If the user meant the other direction, set it again with {}.",
                p.date,
                rate_text(p, base),
                if saved.inverted { "rate instead of inverse_rate" } else { "inverse_rate instead of rate" }
            ));
        }
        Ok(ToolOutput { summary, data: json!({"base_currency": base, "rate": rate_json(&saved, base), "replaced": old.is_some()}) })
    }

    pub async fn list_rates(&self, caller: &Caller, a: ListRatesArgs) -> ToolResult {
        self.check(caller, Right::read("rates"))?;
        let today = today();
        let from = clean(a.from).map(|v| parse_date("from", Some(&v), today)).transpose()?;
        let to = clean(a.to).map(|v| parse_date("to", Some(&v), today)).transpose()?;
        let currency = match clean(a.currency) {
            Some(c) => {
                let list = self.deps.directories.currencies(caller.project_id).await?;
                Some(find_record("currency", &c, &list)?.code.clone())
            }
            None => None,
        };
        let mut rates = self.deps.rates.list(caller.project_id, from, to).await?;
        rates.retain(|r| currency.as_ref().is_none_or(|c| &r.currency == c));
        rates.sort_by(|x, y| y.date.cmp(&x.date).then(x.currency.cmp(&y.currency)));
        let base = &caller.base_currency;
        let mut latest: Vec<&Rate> = Vec::new();
        for r in &rates {
            if !latest.iter().any(|l| l.currency == r.currency) {
                latest.push(r);
            }
        }
        let latest_text = latest.iter().take(10).map(|r| format!("{} ({})", rate_text(r, base), r.date)).collect::<Vec<_>>().join(", ");
        Ok(ToolOutput {
            summary: format!(
                "{} rates (newest first, up to {MAX_PAGE} shown); rate = units of {base} for 1 unit of the currency, inverse_rate = units of the currency for 1 {base}.{}",
                rates.len(),
                if latest_text.is_empty() { String::new() } else { format!(" Latest: {latest_text}.") }
            ),
            data: json!({
                "base_currency": base,
                "total": rates.len(),
                "latest": latest.iter().map(|r| rate_json(r, base)).collect::<Vec<_>>(),
                "rates": rates.iter().take(MAX_PAGE as usize).map(|r| rate_json(r, base)).collect::<Vec<_>>(),
            }),
        })
    }

    // ------------------------------------------------------------ лимиты

    pub async fn set_limit(&self, caller: &Caller, a: SetLimitArgs) -> ToolResult {
        self.check(caller, Right::write("limits"))?;
        let amount = parse_amount("amount", &a.amount)?;
        let period = clean(a.period).map(|p| parse_choice("period", &p, &LIMIT_PERIODS)).transpose()?.unwrap_or_else(|| "MONTH".into());
        let today = today();
        let valid_from = match clean(a.valid_from) {
            Some(v) => parse_date("valid_from", Some(&v), today)?,
            None => period_start(&period, today),
        };
        let categories = self.deps.directories.categories(caller.project_id).await?;
        let cat = find_ref("category", &a.category, &categories)?;
        let rules = self.deps.limits.list(caller.project_id).await?;
        // У категории одно правило на дату начала: правило с тем же valid_from заменяется (и период тоже).
        let old = rules.iter().find(|l| l.category_code == cat.code && l.valid_from == valid_from);
        let draft = Limit { id: old.map(|l| l.id).unwrap_or_default(), category_code: cat.code.clone(), period, valid_from, amount };
        let saved = match old {
            Some(_) => self.deps.limits.update(caller.project_id, &draft).await?,
            None => self.deps.limits.create(caller.project_id, &draft).await?,
        };
        self.audit_write(caller, "set_limit", &[("limit", saved.id.to_string()), ("category", saved.category_code.clone())]).await;
        let base = &caller.base_currency;
        let mut summary = format!(
            "Limit for '{}' ({}, with subcategories): {} {base} per {} from {}.",
            cat.name.show(caller.lang),
            cat.code,
            saved.amount.normalize(),
            saved.period.to_lowercase(),
            saved.valid_from
        );
        if let Some(o) = old {
            summary.push_str(&format!(" Replaced {} per {} from the same date.", o.amount.normalize(), o.period.to_lowercase()));
        }
        if let Some(later) = rules.iter().filter(|l| l.category_code == cat.code && l.valid_from > saved.valid_from).min_by_key(|l| l.valid_from) {
            summary.push_str(&format!(" Note: a later rule from {} ({} per {}) takes over from that date.", later.valid_from, later.amount.normalize(), later.period.to_lowercase()));
        }
        Ok(ToolOutput { summary, data: json!({"base_currency": base, "limit": limit_json(&saved, cat.name.show(caller.lang), None), "replaced": old.is_some()}) })
    }

    pub async fn list_limits(&self, caller: &Caller) -> ToolResult {
        self.check(caller, Right::read("limits"))?;
        let today = today();
        let mut rules = self.deps.limits.list(caller.project_id).await?;
        rules.sort_by(|x, y| x.category_code.cmp(&y.category_code).then(y.valid_from.cmp(&x.valid_from)));
        let categories = self.deps.directories.categories(caller.project_id).await?;
        let title = |code: &str| categories.iter().find(|c| c.code == code).map(|c| c.name.show(caller.lang).to_string()).unwrap_or_default();
        // Действует последнее правило категории с valid_from ≤ сегодня (правила отсортированы: новые сначала).
        let mut seen: HashSet<&str> = HashSet::new();
        let rows: Vec<Value> = rules
            .iter()
            .map(|l| {
                let current = l.valid_from <= today && seen.insert(l.category_code.as_str());
                limit_json(l, &title(&l.category_code), Some(current))
            })
            .collect();
        Ok(ToolOutput {
            summary: format!(
                "{} limit rules, {} in force today; amounts in {}. For a category the rule in force is the latest with valid_from ≤ today, and it covers subcategories.",
                rules.len(),
                seen.len(),
                caller.base_currency
            ),
            data: json!({"base_currency": caller.base_currency, "today": today, "limits": rows}),
        })
    }

    // ------------------------------------------------------------ плановые операции

    pub async fn list_plans(&self, caller: &Caller) -> ToolResult {
        self.check(caller, Right::read("planning"))?;
        let plans = self.deps.plans.list(caller.project_id).await?;
        let active = plans.iter().filter(|p| p.draft.active).count();
        Ok(ToolOutput {
            summary: format!("{} plans, {active} active (paused ones expect no payments); amounts in the account currency.", plans.len()),
            data: json!({"plans": plans.iter().map(|p| plan_json(p, caller.lang)).collect::<Vec<_>>()}),
        })
    }

    pub async fn add_plan(&self, caller: &Caller, a: AddPlanArgs) -> ToolResult {
        self.check(caller, Right::write("planning"))?;
        let kind = parse_choice("kind", &a.kind, &PLAN_KINDS)?;
        let amount = parse_amount("amount", &a.amount)?;
        let recurrence = parse_choice("recurrence", &a.recurrence, &RECURRENCES)?;
        let today = today();
        let start_on = parse_date("start_on", a.start_on.as_deref(), today)?;
        let end_on = clean(a.end_on).map(|v| parse_date("end_on", Some(&v), today)).transpose()?;
        let name = self.new_name(caller, &a.name, a.name_lang.as_deref(), a.translations)?;
        let accounts = self.deps.directories.accounts(caller.project_id).await?;
        let account = find_ref("account", &a.account, &accounts)?;
        let (category_code, to_account_code) = if kind == "TRANSFER" {
            if a.category.is_some() {
                return Err(ToolError("category: transfers have no category; pass to_account".into()));
            }
            let to = a.to_account.as_deref().ok_or_else(|| ToolError("to_account: required for a transfer plan".into()))?;
            (None, Some(find_ref("to_account", to, &accounts)?.code.clone()))
        } else {
            if a.to_account.is_some() {
                return Err(ToolError("to_account: only transfer plans have a destination account".into()));
            }
            let c = a.category.as_deref().ok_or_else(|| ToolError(format!("category: required for an {} plan", kind.to_lowercase())))?;
            (Some(self.category_of_kind(caller, &kind, "category", c).await?), None)
        };
        let draft = PlanDraft {
            name,
            kind,
            account_code: account.code.clone(),
            category_code,
            to_account_code,
            amount,
            recurrence,
            interval: a.interval.unwrap_or(1),
            start_on,
            end_on,
            active: true,
        };
        let plan = self.deps.plans.create(caller.project_id, caller.user_id, &draft).await?;
        self.audit_write(caller, "add_plan", &[("plan", plan.id.to_string())]).await;
        let d = &plan.draft;
        Ok(ToolOutput {
            summary: format!(
                "Added plan '{}' (id {}): {} {} {}, {}, from {}{}.",
                d.name.show(caller.lang),
                plan.id,
                d.kind.to_lowercase(),
                d.amount.normalize(),
                account.currency,
                every(&d.recurrence, d.interval),
                d.start_on,
                d.end_on.map(|e| format!(" to {e}")).unwrap_or_default()
            ),
            data: json!({"plan": plan_json(&plan, caller.lang)}),
        })
    }

    pub async fn update_plan(&self, caller: &Caller, a: UpdatePlanArgs) -> ToolResult {
        self.check(caller, Right::write("planning"))?;
        let id = parse_id("id", &a.id)?;
        let changes = a.name.is_some()
            || a.name_lang.is_some()
            || a.translations.is_some()
            || a.account.is_some()
            || a.category.is_some()
            || a.to_account.is_some()
            || a.amount.is_some()
            || a.recurrence.is_some()
            || a.interval.is_some()
            || a.start_on.is_some()
            || a.end_on.is_some()
            || a.active.is_some();
        if !changes {
            return Err(nothing_to_change());
        }
        let old = self.deps.plans.list(caller.project_id).await?.into_iter().find(|p| p.id == id).ok_or_else(|| ToolError(format!("id: plan {id} not found; call list_plans")))?;
        let today = today();
        let mut d = old.draft.clone();
        d.name = renamed(caller, &old.draft.name, a.name.as_deref(), a.name_lang.as_deref(), a.translations)?;
        if let Some(v) = a.amount.as_deref() {
            d.amount = parse_amount("amount", v)?;
        }
        if let Some(r) = a.recurrence.as_deref() {
            d.recurrence = parse_choice("recurrence", r, &RECURRENCES)?;
        }
        if let Some(i) = a.interval {
            d.interval = i;
        }
        if let Some(v) = a.start_on.as_deref() {
            d.start_on = parse_date("start_on", Some(v), old.draft.start_on)?;
        }
        if let Some(v) = a.end_on.as_deref() {
            d.end_on = if v.trim().is_empty() { None } else { Some(parse_date("end_on", Some(v), today)?) };
        }
        if let Some(active) = a.active {
            d.active = active;
        }
        if a.account.is_some() || a.to_account.is_some() {
            let accounts = self.deps.directories.accounts(caller.project_id).await?;
            if let Some(acc) = a.account.as_deref() {
                d.account_code = find_ref("account", acc, &accounts)?.code.clone();
            }
            if let Some(to) = a.to_account.as_deref() {
                if d.kind != "TRANSFER" {
                    return Err(ToolError("to_account: only transfer plans have a destination account".into()));
                }
                d.to_account_code = Some(find_ref("to_account", to, &accounts)?.code.clone());
            }
        }
        if let Some(c) = a.category.as_deref() {
            if d.kind == "TRANSFER" {
                return Err(ToolError("category: transfer plans have no category".into()));
            }
            d.category_code = Some(self.category_of_kind(caller, &d.kind, "category", c).await?);
        }
        let plan = self.deps.plans.update(caller.project_id, id, &d).await?;
        self.audit_write(caller, "update_plan", &[("plan", plan.id.to_string())]).await;
        let mut state = match (old.draft.active, plan.draft.active) {
            (true, false) => " It is paused now: no more expected payments.".to_string(),
            (false, true) => " It is active again.".to_string(),
            _ => String::new(),
        };
        if plan.id != id {
            // Сменили правило дат при прошедших вхождениях: прежний план закрыт и хранит историю.
            state.push_str(&format!(" The date rule changed after past occurrences: plan {id} keeps their history and is closed; the new rule continues as plan {}.", plan.id));
        }
        Ok(ToolOutput { summary: format!("Updated plan '{}' (id {}).{state}", plan.draft.name.show(caller.lang), plan.id), data: json!({"plan": plan_json(&plan, caller.lang)}) })
    }
}

fn nothing_to_change() -> ToolError {
    ToolError("nothing to change: pass at least one field to update".into())
}

fn required(field: &str, value: &str) -> Result<String, ToolError> {
    clean(Some(value.to_string())).ok_or_else(|| ToolError(format!("{field}: empty")))
}

fn minor_units(value: u32) -> Result<i32, ToolError> {
    if value <= 8 { Ok(value as i32) } else { Err(ToolError(format!("minor_units: expected 0–8, got {value}"))) }
}

/// Код уже занят: подсказка, как быть.
fn taken(what: &str, code: &str, title: &str, archived: bool, update_tool: &str) -> ToolError {
    if archived {
        ToolError(format!("code: {what} {code} ('{title}') already exists and is archived; the user can restore it in the browser, or choose another code"))
    } else {
        ToolError(format!("code: {what} {code} ('{title}') already exists; change it with {update_tool} or choose another code"))
    }
}

/// Частичное изменение названия: текст, язык, переводы — только переданные. Язык нового текста по
/// умолчанию — язык пользователя (как у новых записей), без нового текста — прежний.
fn renamed(caller: &Caller, old: &Name, text: Option<&str>, lang: Option<&str>, tr: Option<NameTranslations>) -> Result<Name, ToolError> {
    let default = if text.is_some() { caller.lang } else { old.lang };
    let lang = lang_of("name_lang", lang, default)?;
    let tr = tr.unwrap_or_default();
    Ok(old.changed("name", text, Some(lang), tr.ru, tr.en, tr.th)?)
}

/// Родитель категории вида `kind` (если известен) по коду или названию, кроме самой категории `except`.
/// Нашёлся только родитель другого вида — так и сказать, а не «не найдено».
fn parent_of<'a>(all: &'a [Category], query: &str, kind: Option<&str>, except: Option<&str>) -> Result<&'a Category, ToolError> {
    let candidates: Vec<&Category> = all.iter().filter(|c| except != Some(c.code.as_str())).collect();
    let same: Vec<&Category> = candidates.iter().copied().filter(|c| kind.is_none_or(|k| c.kind == k)).collect();
    match find_ref("parent", query, &same) {
        Ok(p) => Ok(*p),
        Err(e) => match (kind, find_ref("parent", query, &candidates)) {
            (Some(k), Ok(other)) => Err(ToolError(format!(
                "parent: {} is an {} category; an {} category can only be under an {} one",
                other.code,
                other.kind.to_lowercase(),
                k.to_lowercase(),
                k.to_lowercase()
            ))),
            _ => Err(ToolError(e)),
        },
    }
}

/// Порядок новой категории: после последнего соседа (тот же родитель и вид).
fn next_sort_order(all: &[Category], parent: Option<&str>, kind: &str) -> i32 {
    all.iter().filter(|c| c.parent_code.as_deref() == parent && c.kind == kind).map(|c| c.sort_order).max().map_or(10, |m| m.saturating_add(10))
}

/// «every month», «every 2 weeks», «once».
fn every(recurrence: &str, interval: u32) -> String {
    let unit = match recurrence {
        "DAILY" => "day",
        "WEEKLY" => "week",
        "MONTHLY" => "month",
        "YEARLY" => "year",
        _ => return "once".into(),
    };
    if interval <= 1 { format!("every {unit}") } else { format!("every {interval} {unit}s") }
}

fn name_json(n: &Name) -> Value {
    json!({"text": n.text, "lang": n.lang.code(), "ru": n.ru, "en": n.en, "th": n.th})
}

fn currency_json(c: &Currency) -> Value {
    json!({"code": c.code, "name": name_json(&c.name), "symbol": c.symbol, "minor_units": c.minor_units, "archived": c.archived})
}

fn account_json(a: &Account) -> Value {
    json!({"code": a.code, "name": name_json(&a.name), "kind": a.kind, "currency": a.currency, "archived": a.archived})
}

fn category_json(c: &Category) -> Value {
    json!({"code": c.code, "name": name_json(&c.name), "kind": c.kind, "parent": c.parent_code, "sort_order": c.sort_order, "archived": c.archived})
}

fn rate_json(r: &Rate, base: &str) -> Value {
    let inverse = r.inverse_rate();
    json!({
        "id": r.id, "date": r.date, "currency": r.currency,
        "rate": r.rate.normalize(), "inverse_rate": inverse.normalize(),
        "entered": r.entered.normalize(), "inverted": r.inverted,
        "meaning": format!("1 {} = {} {base}", r.currency, show_rate(r.rate)),
        "inverse_meaning": format!("1 {base} = {} {}", show_rate(inverse), r.currency),
    })
}

/// Курс в направлении, в котором его ввели: «1 THB = 2.5 RUB».
fn rate_main(r: &Rate, base: &str) -> String {
    if r.inverted { format!("1 {base} = {} {}", r.entered.normalize(), r.currency) } else { format!("1 {} = {} {base}", r.currency, r.entered.normalize()) }
}

/// Оба направления: «1 THB = 2.5 RUB (1 RUB = 0.4 THB)».
fn rate_text(r: &Rate, base: &str) -> String {
    let other = if r.inverted { format!("1 {} = {} {base}", r.currency, show_rate(r.rate)) } else { format!("1 {base} = {} {}", show_rate(r.inverse_rate()), r.currency) };
    format!("{} ({other})", rate_main(r, base))
}

/// Пересчитанный курс для текста: 6 знаков после запятой, без лишних нулей.
fn show_rate(v: Decimal) -> Decimal {
    v.round_dp(6).normalize()
}

fn limit_json(l: &Limit, title: &str, current: Option<bool>) -> Value {
    let mut v = json!({"id": l.id, "category": l.category_code, "title": title, "period": l.period, "valid_from": l.valid_from, "amount": l.amount});
    if let Some(c) = current {
        v["current"] = json!(c);
    }
    v
}

fn plan_json(p: &Plan, lang: Lang) -> Value {
    let d = &p.draft;
    json!({
        "id": p.id,
        "name": d.name.show(lang),
        "kind": d.kind,
        "account": d.account_code,
        "category": d.category_code,
        "to_account": d.to_account_code,
        "amount": d.amount,
        "recurrence": d.recurrence,
        "interval": d.interval,
        "start_on": d.start_on,
        "end_on": d.end_on,
        "active": d.active,
    })
}

fn blank_draft(kind: &str, date: NaiveDate, account_code: String, lang: Lang) -> TxDraft {
    TxDraft {
        occurred_on: date,
        kind: kind.to_string(),
        account_code,
        refund: false,
        lines: Vec::new(),
        to_account_code: None,
        amount: None,
        to_amount: None,
        fee_category_code: None,
        fee_amount: None,
        description: None,
        description_lang: lang,
        description_ru: None,
        description_en: None,
        description_th: None,
        plan_id: None,
        plan_date: None,
    }
}

/// Код категории вида `kind` (EXPENSE / INCOME) по коду или названию на любом языке.
fn category_code(q: &QuickInfo, kind: &str, field: &str, value: &str) -> Result<String, ToolError> {
    let cats: Vec<&CategoryInfo> = q.categories.iter().filter(|c| c.kind == kind).collect();
    Ok(resolve(field, value, &cats, |x| &x.code, |x| format!("{}\u{1f}{}", x.title, x.search))?.code.clone())
}

/// Коды категории и всех её потомков.
fn subtree(categories: &[CategoryInfo], root: &str) -> HashSet<String> {
    let mut set: HashSet<String> = HashSet::from([root.to_string()]);
    loop {
        let before = set.len();
        for c in categories {
            if c.parent_code.as_ref().is_some_and(|p| set.contains(p)) {
                set.insert(c.code.clone());
            }
        }
        if set.len() == before {
            return set;
        }
    }
}

fn tx_json(t: &Tx, lang: Lang) -> Value {
    let mut v = json!({
        "id": t.id,
        "date": t.occurred_on,
        "kind": t.kind,
        "account": t.account_code,
        "total": t.total,
        "currency": t.currency,
        "description": t.description_in(lang),
    });
    if t.kind == "TRANSFER" {
        v["to_account"] = json!(t.to_account_code);
        v["to_amount"] = json!(t.to_amount);
        v["to_currency"] = json!(t.to_currency);
        if t.fee_amount.is_some() {
            v["fee"] = json!({"amount": t.fee_amount, "category": t.fee_category_code});
        }
    } else {
        v["lines"] = t.lines.iter().map(|l| json!({"category": l.category_code, "amount": l.amount, "note": l.note})).collect();
        if t.refund {
            v["refund"] = json!(true);
        }
    }
    if let Some(p) = t.plan_id {
        v["plan"] = json!({"id": p, "date": t.plan_date});
    }
    v
}

