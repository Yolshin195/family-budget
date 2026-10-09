//! Семейная бухгалтерия — веб-интерфейс (Askama + Tailwind) и REST API (архитектура rust-hex-api, см. arch.toml).
//!
//! Маршруты:
//! - `/login`, `/register`, `/logout`, `/lang/{ru|en|th}`, `/api/auth/login` — публичные;
//! - `/<фича>/…` — страницы фич, `/api/<фича>/…` — REST; всё под JWT (cookie или Bearer);
//! - `/docs` — Swagger UI.

#[cfg(feature = "seed")]
mod seed;
mod wiring;

use std::env;
use std::sync::Arc;

use axum::response::Redirect;
use axum::routing::get;
use axum::{Router, middleware};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa_swagger_ui::SwaggerUi;

use kernel::auth::{AuthState, Tokens, require_user};
use kernel::context::AppContext;
use kernel::web::{Nav, attach_page};

#[derive(OpenApi)]
#[openapi(info(title = "Семейная бухгалтерия", version = "0.1.0", description = "JWT: POST /api/auth/login → Authorization: Bearer <token>; активный проект — заголовок X-Project-Id"))]
struct ApiDoc;

/// Стили Tailwind, собранные `npm run css` и вшитые в бинарник (Node для запуска не нужен).
async fn styles() -> impl axum::response::IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8"), (axum::http::header::CACHE_CONTROL, "public, max-age=3600")],
        include_str!("../static/app.css"),
    )
}

/// Подключение к БД с повторами: в контейнере Postgres может стартовать позже приложения.
async fn connect(url: &str) -> anyhow::Result<sqlx::PgPool> {
    let mut attempt = 1;
    loop {
        match PgPoolOptions::new().max_connections(10).connect(url).await {
            Ok(pool) => return Ok(pool),
            Err(e) if attempt < 30 => {
                eprintln!("БД недоступна (попытка {attempt}/30): {e}");
                attempt += 1;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let pool = connect(&env::var("DATABASE_URL")?).await?;
    let tokens = Tokens::hs256(&env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".into()));
    let ctx = AppContext { pool, tokens: tokens.clone() };

    let wired = wiring::features(&ctx);
    let feats = kernel::feature::assemble(&ctx, wired.features).await?;
    println!("Включённые фичи: {:?}", feats.names);

    #[cfg(feature = "seed")]
    if env::var("SEED_DEMO").map(|v| v != "0").unwrap_or(true) {
        seed::run(&wired.seed).await?;
    }

    let mut openapi = ApiDoc::openapi();
    openapi.merge(feats.openapi);
    openapi.components.get_or_insert_with(Default::default).add_security_scheme(
        "bearer_auth",
        SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).bearer_format("JWT").build()),
    );

    let auth = AuthState { tokens, memberships: wired.projects.clone() as Arc<dyn kernel::auth::Memberships> };
    let nav = Nav(Arc::new(feats.nav));
    let private = Router::new()
        .nest("/api", feats.router)
        .merge(feats.pages)
        .route("/", get(|| async { Redirect::to("/budget") }))
        .layer(middleware::from_fn_with_state(nav, attach_page))
        .layer(middleware::from_fn_with_state(auth, require_user));
    let public = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/static/app.css", get(styles))
        .merge(feats.public);

    let app = Router::new()
        .merge(public)
        .merge(private)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    println!("Server starting on http://{addr}  (Swagger: /docs)");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
