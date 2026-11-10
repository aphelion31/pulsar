//! OWNED BY A WORKER. Macros, automation rules, webhooks. Mounted at /api/v1/accounts/{account_id}.
use axum::Router;
use crate::common::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
