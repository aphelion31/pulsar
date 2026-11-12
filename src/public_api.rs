use axum::{extract::{Path, State}, routing::get, Json, Router};
use serde_json::json;
use crate::common::{ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new().route("/public/api/v1/inboxes/{inbox_identifier}", get(show))
}

async fn show(State(st): State<AppState>, Path(identifier): Path<String>) -> ApiResult {
    let row: (String, Option<bool>, String, Option<String>, Option<bool>, Option<bool>, Option<bool>) = sqlx::query_as(
        "SELECT a.identifier, a.hmac_mandatory, i.name, i.timezone, i.working_hours_enabled, i.csat_survey_enabled, i.greeting_enabled FROM channel_api a JOIN inboxes i ON i.channel_id = a.id AND i.channel_type = 'Channel::Api' AND i.account_id = a.account_id WHERE a.identifier = $1"
    ).bind(&identifier).fetch_one(&st.db).await?;
    Ok(Json(json!({"identifier":row.0,"identity_validation_enabled":row.1,"name":row.2,"timezone":row.3,"working_hours":[],"working_hours_enabled":row.4,"csat_survey_enabled":row.5,"greeting_enabled":row.6})))
}
