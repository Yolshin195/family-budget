//! Входящий адаптер: HTTP фичи — `/mcp` (Streamable HTTP, свой вход по токену) и метаданные
//! защищённого ресурса (RFC 9728) для OAuth-клиентов вроде Claude.
//!
//! Роутер монтируется как публичный: обычный вход по cookie здесь не действует, а токен проверяет
//! свой слой — личный API-токен или access-токен провайдера входа, выпущенный для этого ресурса.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use kernel::auth::RequestMeta;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::tower::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::json;

use crate::adapters::server::BudgetServer;
use crate::service::{Auth, McpService};

type Svc = State<Arc<McpService>>;

/// `/mcp` и `/.well-known/oauth-protected-resource[/mcp]`.
pub fn router(svc: Arc<McpService>, dev: bool) -> Router {
    let settings = svc.settings().clone();
    let authority = settings.base_url.split("://").nth(1).unwrap_or("localhost").split('/').next().unwrap_or("localhost").to_string();
    let host = authority.split(':').next().unwrap_or("localhost").to_string();
    let origin = kernel::config::origin_of(&settings.base_url);
    // rmcp: запись без порта скоро будет значить только порт по умолчанию — указываем его явно.
    let origin = if origin.rsplit(':').next().is_some_and(|p| p.chars().all(|c| c.is_ascii_digit())) {
        origin
    } else if origin.starts_with("https://") {
        format!("{origin}:443")
    } else {
        format!("{origin}:80")
    };
    let mut hosts = vec![host, authority];
    let mut origins = vec![origin];
    if dev {
        // Разработка: MCP Inspector и Claude Code с этой машины.
        hosts.extend(["localhost", "127.0.0.1"].map(String::from));
        origins.extend(["http://localhost:*", "http://127.0.0.1:*"].map(String::from));
    }
    // Без серверных сессий (в MCP 2026-07-28 их нет) и с ответами JSON: уведомления сервера не нужны,
    // а глобальный таймаут запроса не рвёт долгие SSE-потоки.
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .with_allowed_hosts(hosts)
        .with_allowed_origins(origins);
    let server_svc = svc.clone();
    let mcp = StreamableHttpService::new(move || Ok(BudgetServer::new(server_svc.clone())), Arc::new(NeverSessionManager::default()), config);
    Router::new()
        .route("/.well-known/oauth-protected-resource", get(resource_metadata))
        .route("/.well-known/oauth-protected-resource/mcp", get(resource_metadata))
        .with_state(svc.clone())
        .merge(Router::new().nest_service("/mcp", mcp).layer(middleware::from_fn_with_state(svc, authenticate)))
}

/// RFC 9728: где получить токен для этого ресурса. Без настроенного провайдера входа — 404
/// (OAuth недоступен, остаются личные API-токены).
async fn resource_metadata(State(svc): Svc) -> Response {
    let Some(issuer) = svc.issuer().await else { return StatusCode::NOT_FOUND.into_response() };
    let s = svc.settings();
    Json(json!({
        "resource": s.resource,
        "authorization_servers": [issuer],
        "scopes_supported": s.oauth_scope.split_whitespace().collect::<Vec<_>>(),
        "bearer_methods_supported": ["header"],
        "resource_name": "Family budget",
        "resource_documentation": s.setup_url(),
    }))
    .into_response()
}

fn unauthorized(svc: &McpService, invalid: bool) -> Response {
    let s = svc.settings();
    let mut challenge = format!("Bearer resource_metadata=\"{}\", scope=\"{}\"", s.resource_metadata_url(), s.oauth_scope);
    if invalid {
        challenge.push_str(", error=\"invalid_token\"");
    }
    let mut resp = (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized", "message": "требуется токен: Authorization: Bearer <токен>"}))).into_response();
    if let Ok(v) = HeaderValue::from_str(&challenge) {
        resp.headers_mut().insert(WWW_AUTHENTICATE, v);
    }
    resp
}

/// Вход на `/mcp`: токен → вызывающий (пользователь, проект, права) в расширения запроса.
async fn authenticate(State(svc): Svc, mut req: Request<Body>, next: Next) -> Response {
    let bearer = req.headers().get(AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(str::to_string);
    let meta = req.extensions().get::<RequestMeta>().cloned().unwrap_or_default();
    match svc.authenticate(bearer.as_deref(), meta).await {
        Ok(Auth::Ok(caller)) => {
            req.extensions_mut().insert(*caller);
            next.run(req).await
        }
        Ok(Auth::Missing) => unauthorized(&svc, false),
        Ok(Auth::Invalid) => unauthorized(&svc, true),
        Ok(Auth::Throttled) => (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error": "too_many_requests", "message": "слишком много запросов, повторите через минуту"}))).into_response(),
        Err(e) => e.into_response(),
    }
}
