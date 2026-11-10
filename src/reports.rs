//! OWNED BY A WORKER. See WORKERS.md for the contract.
//! Routes here are mounted at /api/v2/accounts/{account_id} (reports), see main.rs.
use axum::Router;
use crate::common::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
