//! Семейная бухгалтерия — веб-интерфейс (Askama + Tailwind + htmx) и REST API (архитектура rust-hex-api, см. arch.toml).
//!
//! Маршруты:
//! - публичные: `/` (лендинг), `/privacy`, `/terms`, `/data-deletion`, `/sitemap.xml`, `/robots.txt`,
//!   `/auth/login|register|callback|logout` (вход через Rauthy), `/demo`, `/lang/{ru|en|th}`, `/static/…`;
//! - аккаунт (вход есть, проекта может не быть): `/onboarding`;
//! - приложение: `/<фича>/…` — страницы, `/api/<фича>/…` — REST; cookie-сессия или API-токен;
//! - `/docs` — Swagger UI (только в разработке).

mod assets;
#[cfg(feature = "seed")]
mod seed;
mod wiring;

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use axum::routing::get;
use axum::{Router, middleware};
use sqlx::postgres::PgPoolOptions;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa_swagger_ui::SwaggerUi;

use kernel::auth::{AuthState, MAX_BODY, attach_meta, maybe_user, require_identity, require_user};
use kernel::config::Config;
use kernel::context::AppContext;
use kernel::throttle::RateLimiter;
use kernel::web::{Nav, WebState, attach_page, error_pages, no_index, not_found, security_headers, static_file};

#[derive(OpenApi)]
#[openapi(info(
    title = "Семейная бухгалтерия",
    version = "0.2.0",
    description = "Авторизация: личный API-токен из «Профиль → Безопасность» в заголовке `Authorization: Bearer fbt_…`. \
                   Токен действует в одном проекте и с выбранными правами (read, write, <фича>:read|write)."
))]
struct ApiDoc;

/// Подключение к БД с повторами: в контейнере Postgres может стартовать позже приложения.
async fn connect(url: &str) -> anyhow::Result<sqlx::PgPool> {
    let mut attempt = 1;
    loop {
        match PgPoolOptions::new().max_connections(10).connect(url).await {
            Ok(pool) => return Ok(pool),
            Err(e) if attempt < 30 => {
                eprintln!("БД недоступна (попытка {attempt}/30): {e}");
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn flag(name: &str, default: bool) -> bool {
    env::var(name).map(|v| !matches!(v.trim(), "0" | "false" | "no" | "off" | "")).unwrap_or(default)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let config = Arc::new(Config::from_env());
    if let Err(e) = config.validate() {
        anyhow::bail!("настройки продакшена: {e}");
    }
    let seed_demo = flag("SEED_DEMO", !config.is_prod());
    if config.is_prod() && seed_demo {
        anyhow::bail!("SEED_DEMO=1 запрещён в продакшене: демо для гостей включается GUEST_DEMO=1");
    }
    if config.oidc.is_none() {
        eprintln!("Внимание: Rauthy не настроен (OIDC_ISSUER) — вход только «для разработки» (DEV_LOGIN).");
    }

    assets::register();
    let database_url = kernel::config::secret_var("DATABASE_URL").ok_or_else(|| anyhow::anyhow!("DATABASE_URL не задан"))?;
    let pool = connect(&database_url).await?;
    let ctx = AppContext { pool, config: config.clone() };

    let wired = wiring::features(&ctx);
    let feats = kernel::feature::assemble(&ctx, wired.features).await?;
    println!("Включённые фичи: {:?}", feats.names);

    #[cfg(feature = "seed")]
    {
        let seed = Arc::new(wired.seed);
        seed::run(&seed, seed_demo, config.guest_demo).await?;
        if config.guest_demo {
            // Демо для гостей сбрасывается раз в сутки.
            let s = seed.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(24 * 3600));
                tick.tick().await;
                loop {
                    tick.tick().await;
                    if let Err(e) = seed::reset_family(&s).await {
                        eprintln!("сброс демо: {e}");
                    }
                }
            });
        }
    }

    // Чистка: истёкшие сессии и журнал безопасности старше года — раз в 6 часов.
    let projects = wired.projects.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(6 * 3600));
        loop {
            tick.tick().await;
            match projects.cleanup().await {
                Ok((s, a)) if s + a > 0 => println!("чистка: сессий {s}, записей журнала {a}"),
                Ok(_) => {}
                Err(e) => eprintln!("чистка: {e}"),
            }
        }
    });

    // Воронка лендинга (визиты → регистрации → первый расход) — в журнал сервера раз в сутки, без ПД.
    #[cfg(feature = "site")]
    {
        let site = wired.site.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(24 * 3600));
            tick.tick().await;
            loop {
                tick.tick().await;
                let day = chrono::Utc::now().date_naive().pred_opt().unwrap_or_default();
                if let Ok(f) = site.funnel(day).await {
                    println!(
                        "воронка {day}: визиты {} → регистрации {} ({:.1} %) → первый расход {} ({:.1} %)",
                        f.visits,
                        f.signups,
                        site::Funnel::rate(f.signups, f.visits as i64),
                        f.first_expenses,
                        site::Funnel::rate(f.first_expenses, f.signups)
                    );
                }
            }
        });
    }

    let mut openapi = ApiDoc::openapi();
    openapi.merge(feats.openapi);
    openapi.components.get_or_insert_with(Default::default).add_security_scheme(
        "bearer_auth",
        SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).bearer_format("fbt_…").build()),
    );

    let auth = AuthState {
        config: config.clone(),
        authenticator: wired.projects.clone() as Arc<dyn kernel::auth::Authenticator>,
        memberships: wired.projects.clone() as Arc<dyn kernel::auth::Memberships>,
        limiter: Arc::new(RateLimiter::new()),
    };
    let web = WebState { nav: Nav(Arc::new(feats.nav)), config: config.clone() };

    // Приложение: сессия или API-токен → проект → роль → контекст страницы. Закрыто от индексации.
    let private = Router::new()
        .nest("/api", feats.router)
        .merge(feats.pages)
        .layer(middleware::from_fn_with_state(web, attach_page))
        .layer(middleware::from_fn_with_state(auth.clone(), require_user))
        .layer(middleware::from_fn(no_index));
    // Онбординг: вход есть, проекта ещё нет.
    let account = Router::new()
        .merge(feats.account)
        .layer(middleware::from_fn_with_state(auth.clone(), require_identity))
        .layer(middleware::from_fn(no_index));
    let public = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/static/{file}", get(static_file))
        .merge(feats.public)
        .layer(middleware::from_fn_with_state(auth, maybe_user));

    let mut app = Router::new().merge(public).merge(account).merge(private);
    if config.swagger {
        app = app.merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi));
    }

    // CORS — только свой домен (раньше был разрешён любой).
    let cors = CorsLayer::new()
        .allow_origin(HeaderValue::from_str(&config.origin())?)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, HeaderName::from_static("x-csrf-token"), HeaderName::from_static("x-project-id")])
        .allow_credentials(true);

    let app = app
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(config.clone(), error_pages))
        .layer(middleware::from_fn_with_state(config.clone(), security_headers))
        .layer(middleware::from_fn_with_state(config.clone(), attach_meta))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(RequestBodyLimitLayer::new(MAX_BODY))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(15)))
        .layer(cors)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    println!("Server starting on http://{addr}  ({}; {})", if config.is_prod() { "production" } else { "development" }, config.base_url);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}
