//! OWNED BY A WORKER. See WORKERS.md for the contract.
//! File storage + attachments + avatars. `routes()` is merged at the ROOT router (full paths, e.g. /rails/active_storage/...).
use axum::Router;
use crate::common::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
