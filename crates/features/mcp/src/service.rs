//! Use cases фичи «MCP для ассистентов»: кто вызывает (вход по токену), доступы ассистентов и
//! инструменты бюджета. Права проверяются здесь — в каждом инструменте, какой бы клиент ни вызвал.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{Days, Duration, NaiveDate, Utc};
use kernel::auth::{API_TOKEN_PREFIX, Credential, RequestMeta, Role, Via as AuthVia};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::throttle::RateLimiter;
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
        "skip_planned" => Right::write("planning"),
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

fn lang_of(value: Option<&str>, default: Lang) -> Result<Lang, String> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(default),
        Some(v) => Lang::parse(&v.to_ascii_lowercase()).ok_or_else(|| format!("description_lang: expected ru, en or th, got '{v}'")),
    }
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

    async fn audit_write(&self, caller: &Caller, tool: &str, tx: Option<Uuid>) {
        let mut details = vec![("tool", tool.to_string()), ("client", caller.client().to_string())];
        if let Some(id) = tx {
            details.push(("transaction", id.to_string()));
        }
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
        let base = json!({
            "project": caller.project_name,
            "role": caller.role.as_str(),
            "base_currency": caller.base_currency,
            "user_language": caller.lang.code(),
            "today": today,
            "permissions": self.permissions(caller),
        });
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
        draft.description_lang = lang_of(lang.as_deref(), caller.lang)?;
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
        self.audit_write(caller, tool, Some(tx.id)).await;
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
        self.audit_write(caller, "add_transfer", Some(tx.id)).await;
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
        self.audit_write(caller, "update_transaction", Some(tx.id)).await;
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
        self.audit_write(caller, "pay_planned", Some(tx.id)).await;
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
        self.audit_write(caller, "skip_planned", None).await;
        Ok(ToolOutput { summary: format!("Skipped the payment of plan {plan_id} scheduled for {date}."), data: json!({"plan_id": plan_id, "date": date, "status": "SKIPPED"}) })
    }
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

