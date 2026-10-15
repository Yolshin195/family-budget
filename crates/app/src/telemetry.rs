//! Журнал сервера (`tracing`) и отправка ошибок в Sentry (или совместимый GlitchTip).
//!
//! Без `SENTRY_DSN` — только журнал (`docker compose logs app`). С ним в Sentry уходят события уровня ERROR
//! (внутренние ошибки — `AppError::public_message`, сбои фоновых задач и запуска) и паники; события INFO/WARN —
//! «хлебными крошками» к ним. О запросе — только метод и шаблон маршрута (`GET /api/transactions/{id}`):
//! без адреса с параметрами, заголовков, cookie, тела, IP и пользователя — данные бюджета остаются у нас.
use std::env;
use std::io::IsTerminal;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use sentry::integrations::tracing::EventFilter;
use tracing::{Level, Metadata};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use kernel::config::{Config, secret_var};

/// Включить журнал и, если задан `SENTRY_DSN`, Sentry. Вызывать до запуска tokio: потоки рантайма берут
/// клиент Sentry из главного хаба. Гард держать до конца `main` — при выходе он отправляет накопленное.
pub fn init(config: &Config) -> Option<sentry::ClientInitGuard> {
    let dsn = secret_var("SENTRY_DSN").map(|v| v.parse::<sentry::types::Dsn>());
    let guard = match &dsn {
        Some(Ok(dsn)) => {
            // Отправка — через rustls с ring, как у sqlx и openidconnect (без OpenSSL и aws-lc).
            let _ = rustls::crypto::ring::default_provider().install_default();
            Some(sentry::init((dsn.clone(), options(config))))
        }
        _ => None,
    };
    tracing_subscriber::registry()
        // RUST_LOG переопределяет уровень, например `RUST_LOG=info,sqlx=warn`.
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_ansi(std::io::stdout().is_terminal()))
        .with(guard.is_some().then(|| sentry::integrations::tracing::layer().event_filter(event_filter)))
        .init();
    match (dsn, &guard) {
        (Some(Err(e)), _) => tracing::error!("SENTRY_DSN: {e} — ошибки в Sentry не отправляются"),
        (_, Some(g)) => {
            let o = g.options();
            let (env, release) = (o.environment.as_deref().unwrap_or_default(), o.release.as_deref().unwrap_or_default());
            tracing::info!("Sentry: ошибки отправляются (окружение {env}, версия {release})");
        }
        _ => {}
    }
    guard
}

fn options(config: &Config) -> sentry::ClientOptions {
    let var = |name: &str| env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let environment = var("SENTRY_ENVIRONMENT").unwrap_or_else(|| (if config.is_prod() { "production" } else { "development" }).into());
    // Версия: family_budget@0.1.0+<коммит> — Sentry покажет, с какой сборки ошибка началась или вернулась.
    let release = var("SENTRY_RELEASE").unwrap_or_else(|| match var("GIT_SHA") {
        Some(sha) => format!("family_budget@{}+{}", env!("CARGO_PKG_VERSION"), sha.chars().take(12).collect::<String>()),
        None => format!("family_budget@{}", env!("CARGO_PKG_VERSION")),
    });
    // Без IP, cookie и заголовков авторизации.
    sentry::ClientOptions::new().environment(environment).release(release).send_default_pii(false)
}

/// ERROR → событие (issue в Sentry), INFO/WARN → «хлебные крошки». Исключение — `tower_http`: его ERROR
/// «response failed» на каждый ответ 5xx дублировал бы саму ошибку, уже записанную обработчиком.
fn event_filter(md: &Metadata<'_>) -> EventFilter {
    match *md.level() {
        Level::ERROR if md.target().starts_with("tower_http") => EventFilter::Breadcrumb,
        Level::ERROR => EventFilter::Event,
        Level::WARN | Level::INFO => EventFilter::Breadcrumb,
        _ => EventFilter::Ignore,
    }
}

/// Middleware: имя маршрута (`GET /api/transactions/{id}`) — в события Sentry этого запроса. Подключать через
/// `Router::layer` под [`sentry::integrations::tower::NewSentryLayer`] (свой хаб на каждый запрос), иначе имя
/// одного запроса достанется событиям другого.
pub async fn route_name(req: Request, next: Next) -> Response {
    if let Some(path) = req.extensions().get::<MatchedPath>() {
        let name = format!("{} {}", req.method(), path.as_str());
        sentry::configure_scope(|scope| scope.set_transaction(Some(&name)));
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use axum::body::Body;
    use axum::routing::get;
    use axum::{Router, middleware};
    use kernel::error::{AppError, AppResult};
    use sentry::integrations::tower::NewSentryLayer;
    use tower::ServiceExt;
    use tower_http::trace::TraceLayer;

    async fn broken() -> AppResult<&'static str> {
        Err(AppError::Internal(anyhow::anyhow!("connection reset").context("курс валюты не сохранён")))
    }

    /// Ответ 500: в Sentry ровно одно событие (ERROR от `tower_http` — лишь «крошка») — с цепочкой ошибки и
    /// шаблоном маршрута, без адреса, параметров, заголовков и cookie. Ошибка клиента (404) событий не создаёт.
    #[test]
    fn internal_error_reaches_sentry_once_without_request_data() {
        let app = Router::new()
            .route("/api/rates/{id}", get(broken))
            .route("/api/ok", get(|| async { AppResult::<&str>::Err(AppError::NotFound("нет".into())) }))
            .layer(middleware::from_fn(route_name))
            .layer(TraceLayer::new_for_http())
            .layer(NewSentryLayer::<Request>::new_from_top());
        let subscriber = tracing_subscriber::registry().with(sentry::integrations::tracing::layer().event_filter(event_filter));

        let events = sentry::test::with_captured_events_options(
            || {
                let _log = tracing::subscriber::set_default(subscriber);
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                rt.block_on(async {
                    for uri in ["/api/rates/7f0c2b9e?q=кафе", "/api/ok"] {
                        let req = axum::http::Request::get(uri).header("cookie", "fb_session=secret").body(Body::empty()).unwrap();
                        tokio::time::timeout(Duration::from_secs(5), app.clone().oneshot(req)).await.unwrap().unwrap();
                    }
                });
            },
            sentry::ClientOptions::new().send_default_pii(false),
        );

        assert_eq!(events.len(), 1, "одно событие на ответ 500: {events:#?}");
        let event = &events[0];
        assert_eq!(event.transaction.as_deref(), Some("GET /api/rates/{id}"));
        let values: Vec<_> = event.exception.values.iter().filter_map(|e| e.value.as_deref()).collect();
        assert!(values.contains(&"курс валюты не сохранён") && values.contains(&"connection reset"), "{values:?}");
        assert!(event.request.is_none() && event.user.is_none(), "{event:#?}");
        let json = serde_json::to_string(event).unwrap();
        assert!(!json.contains("7f0c2b9e") && !json.contains("кафе") && !json.contains("secret"), "{json}");
    }
}
