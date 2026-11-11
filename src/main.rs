mod auth;
mod automation;
mod bots;
mod cable;
mod common;
mod companies;
mod contacts;
mod conversations;
mod inboxes;
mod reports;
mod seed;
mod settings;
mod storage;
mod web;
mod widget;

use axum::{
    routing::{any, get, post},
    Router,
};
use common::AppState;
use tower_http::services::{ServeDir, ServeFile};

/// Account-scoped API: `/api/v1/accounts/{account_id}/...`. Each module owns its own routes.
fn account_routes() -> Router<AppState> {
    Router::new()
        .merge(conversations::routes())
        .merge(contacts::routes())
        .merge(inboxes::routes())
        .merge(settings::routes())
        .merge(companies::routes())
        .merge(automation::routes())
        .merge(bots::routes())
        .fallback(web::not_implemented)
}

pub fn app(state: AppState) -> Router {
    // React Router SPA: real files under /app/* (assets), everything else under /app falls back to index.html
    let web_dir = state.public_dir.clone();
    let spa = ServeDir::new(&web_dir).fallback(ServeFile::new(web_dir.join("index.html")));
    Router::new()
        .route("/health", get(web::health))
        .route("/auth/sign_in", post(auth::sign_in))
        .route("/auth/validate_token", get(auth::validate_token))
        .route("/auth/sign_out", axum::routing::delete(auth::sign_out))
        .route(
            "/api/v1/profile",
            get(auth::profile_show)
                .put(auth::profile_update)
                .layer(axum::extract::DefaultBodyLimit::max(storage::MAX_BYTES + 1024 * 1024)),
        )
        .route("/api/v1/profile/avatar", axum::routing::delete(auth::profile_avatar_destroy))
        .route("/api/v1/profile/resend_confirmation", post(auth::resend_confirmation))
        .route("/api/v1/profile/reset_access_token", post(auth::reset_access_token))
        .route("/api/v1/profile/set_active_account", axum::routing::put(auth::set_active_account))
        .route("/api/v1/profile/availability", post(auth::availability))
        .route("/api/v1/profile/auto_offline", post(auth::auto_offline))
        .route("/api/v1/profile/work_preferences", post(auth::auto_offline).patch(auth::auto_offline))
        .route("/api/v1/accounts/{account_id}", get(auth::account_show).patch(auth::account_update))
        .route("/api/v1/accounts/{account_id}/onboarding", axum::routing::put(auth::onboarding_update))
        .route("/api/v1/accounts/{account_id}/onboarding/help_center_generation", get(auth::help_center_generation))
        .route("/api/v1/accounts/{account_id}/", get(auth::account_show).patch(auth::account_update))
        .route("/api/v1/accounts/{account_id}/update_active_at", post(auth::update_active_at))
        // ponytail: session list / push subscriptions / changelog not tracked yet; empty responses keep UI happy
        .route("/api/v1/profile/sessions", get(|_u: common::CurrentUser| async { axum::Json(serde_json::json!([])) }))
        .route("/api/v1/notification_subscriptions", post(|_u: common::CurrentUser| async { axum::Json(serde_json::json!({})) }))
        .route("/api/v1/changelog", get(|| async { axum::Json(serde_json::json!({ "posts": [] })) }))
        .nest("/api/v1/accounts/{account_id}", account_routes())
        .nest("/api/v2/accounts/{account_id}", reports::routes().fallback(web::not_implemented))
        .merge(widget::routes())
        .merge(storage::routes())
        .route("/api/{*rest}", any(web::not_implemented))
        .route("/cable", get(cable::ws_handler))
        .route("/", get(|| async { axum::response::Redirect::to("/app/") }))
        .route("/app", get(|| async { axum::response::Redirect::to("/app/") }))
        .nest_service("/app/", spa)
        .route_service("/sdk.js", ServeFile::new(web_dir.join("sdk.js")))
        // Negotiated via Accept-Encoding; skips tiny bodies, images and SSE/ws upgrades by default.
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())).init();
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/pulsar".into());
    let db = sqlx::postgres::PgPoolOptions::new().max_connections(10).connect(&url).await?;

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("seed") {
        return seed::run(&db, &args[2..]).await;
    }

    // Pulsar's queries target this Chatwoot schema version. An older Chatwoot database misses columns and some endpoints 500.
    const SCHEMA: &str = "20261103142055";
    let db_schema: Option<String> = sqlx::query_scalar("SELECT max(version) FROM schema_migrations").fetch_one(&db).await.ok().flatten();
    if db_schema.as_deref().is_none_or(|v| v < SCHEMA) {
        tracing::warn!(
            "database schema {} is older than {SCHEMA} (Chatwoot 4.19). Upgrade Chatwoot first, or apply migrations/0001_schema.sql column additions",
            db_schema.as_deref().unwrap_or("unknown")
        );
    }

    // Released Chatwoot 4.19.1 lacks account_users.accepts_conversations/accepts_calls (added later on main).
    let has_wp: bool = sqlx::query_scalar(
        "SELECT count(*) = 2 FROM information_schema.columns
         WHERE table_schema = current_schema() AND table_name = 'account_users'
           AND column_name IN ('accepts_conversations', 'accepts_calls')",
    )
    .fetch_one(&db)
    .await?;
    let _ = common::HAS_WORK_PREFERENCES.set(has_wp);
    if !has_wp {
        tracing::info!("account_users work preferences not in this schema; using Chatwoot defaults (accept all)");
    }

    // Pulsar-owned table (same as migrations/0003). Created here too so an install that skipped the
    // migration keeps the widget working instead of failing every request. Idempotent; Chatwoot
    // tables are not touched.
    sqlx::query(include_str!("../migrations/0003_pulsar_widget_token_floor.sql"))
        .execute(&db)
        .await
        .map_err(|e| anyhow::anyhow!("creating pulsar_widget_token_floors: {e}"))?;

    let state = AppState {
        db,
        hub: cable::Hub::default(),
        public_dir: std::env::var("WEB_DIR").unwrap_or_else(|_| "web/build/client".into()).into(),
        frontend_url: std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".into()),
    };
    let addr = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:3000".into());
    tracing::info!("pulsar listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app(state)).await?;
    Ok(())
}
