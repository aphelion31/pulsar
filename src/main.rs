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
use tower_http::services::ServeDir;

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
    let public = state.public_dir.clone();
    Router::new()
        .route("/health", get(web::health))
        .route("/auth/sign_in", post(auth::sign_in))
        .route("/auth/validate_token", get(auth::validate_token))
        .route("/auth/sign_out", axum::routing::delete(auth::sign_out))
        .route("/api/v1/profile", get(auth::profile_show).put(auth::profile_update))
        .route("/api/v1/profile/set_active_account", axum::routing::put(auth::set_active_account))
        .route("/api/v1/profile/availability", post(auth::availability))
        .route("/api/v1/profile/auto_offline", post(auth::auto_offline))
        .route("/api/v1/profile/work_preferences", post(auth::auto_offline).patch(auth::auto_offline))
        .route("/api/v1/accounts/{account_id}", get(auth::account_show).patch(auth::account_update))
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
        .route("/", get(web::dashboard))
        .route("/app", get(web::dashboard))
        .route("/app/{*rest}", get(web::dashboard))
        .fallback_service(ServeDir::new(public))
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

    let state = AppState {
        db,
        hub: cable::Hub::default(),
        public_dir: std::env::var("PUBLIC_DIR").unwrap_or_else(|_| "../chatwoot/public".into()).into(),
        frontend_url: std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".into()),
    };
    let addr = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:3000".into());
    tracing::info!("pulsar listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app(state)).await?;
    Ok(())
}
