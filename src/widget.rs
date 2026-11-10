//! OWNED BY A WORKER. See WORKERS.md for the contract.
//! Public web widget. `routes()` is merged at the ROOT router: use full paths (/widget, /api/v1/widget/...). No CurrentUser.
use axum::Router;
use crate::common::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
