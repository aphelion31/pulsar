//! OWNED BY A WORKER. See WORKERS.md for the contract.
use axum::Router;
use crate::common::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
