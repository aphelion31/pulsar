use axum::{extract::{Path, State}, routing::get, Json, Router};
use serde_json::{json, Value};
use crate::common::{ApiError, ApiResult, AppState};
use hmac::{Hmac, Mac};
use sha2_compat::Sha256;

pub fn routes() -> Router<AppState> {
    Router::new().route("/public/api/v1/inboxes/{inbox_identifier}", get(show))
        .route("/public/api/v1/inboxes/{inbox_identifier}/contacts", axum::routing::post(contact_create))
        .route("/public/api/v1/inboxes/{inbox_identifier}/contacts/{contact_identifier}", get(contact_show).patch(contact_update))
}

async fn inbox(st: &AppState, identifier: &str) -> Result<(i64, i64, bool, Option<String>), ApiError> {
    Ok(sqlx::query_as("SELECT i.id::bigint, i.account_id::bigint, coalesce(a.hmac_mandatory, false), a.hmac_token FROM channel_api a JOIN inboxes i ON i.channel_id = a.id AND i.channel_type = 'Channel::Api' AND i.account_id = a.account_id WHERE a.identifier = $1")
        .bind(identifier).fetch_one(&st.db).await?)
}

async fn contact(st: &AppState, inbox_id: i64, source: &str) -> Result<(i64, i64), ApiError> {
    Ok(sqlx::query_as("SELECT id, contact_id FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2")
        .bind(inbox_id).bind(source).fetch_one(&st.db).await?)
}

fn verify_hmac(mandatory: bool, token: Option<&str>, identifier: Option<&str>, supplied_id: Option<&str>, hash: Option<&str>) -> Result<bool, ApiError> {
    if !mandatory && hash.is_none_or(str::is_empty) { return Ok(false); }
    let fail = || ApiError(axum::http::StatusCode::UNAUTHORIZED, "HMAC failed: Invalid Identifier Hash Provided".into());
    let id = identifier.filter(|s| !s.is_empty()).ok_or_else(fail)?;
    if supplied_id != Some(id) { return Err(fail()); }
    let bytes = hex::decode(hash.unwrap_or_default()).map_err(|_| fail())?;
    let mut mac = Hmac::<Sha256>::new_from_slice(token.unwrap_or_default().as_bytes()).map_err(|_| fail())?;
    mac.update(id.as_bytes());
    mac.verify_slice(&bytes).map_err(|_| fail())?;
    Ok(true)
}

async fn contact_value(st: &AppState, ci: i64) -> ApiResult {
    let row: (String, Option<String>, i64, Option<String>, Option<String>, Option<String>) = sqlx::query_as("SELECT ci.source_id, ci.pubsub_token, c.id::bigint, c.name, c.email, c.phone_number FROM contact_inboxes ci JOIN contacts c ON c.id = ci.contact_id WHERE ci.id = $1")
        .bind(ci).fetch_one(&st.db).await?;
    Ok(Json(json!({"source_id":row.0,"pubsub_token":row.1,"id":row.2,"name":row.3,"email":row.4,"phone_number":row.5})))
}

async fn contact_create(State(st): State<AppState>, Path(ident): Path<String>, Json(body): Json<Value>) -> ApiResult {
    let (inbox_id, account_id, mandatory, token) = inbox(&st, &ident).await?;
    let id = body["identifier"].as_str();
    verify_hmac(mandatory, token.as_deref(), id, id, body["identifier_hash"].as_str())?;
    let source = body["source_id"].as_str().map(str::to_owned).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if let Ok((ci, _)) = contact(&st, inbox_id, &source).await { return contact_value(&st, ci).await; }
    let mut tx = st.db.begin().await?;
    let existing: Option<i64> = sqlx::query_scalar("SELECT id::bigint FROM contacts WHERE account_id = $1 AND (($2::text IS NOT NULL AND identifier = $2) OR ($3::text IS NOT NULL AND email = $3)) LIMIT 1")
        .bind(account_id as i32).bind(id).bind(body["email"].as_str()).fetch_optional(&mut *tx).await?;
    let contact_id = if let Some(id) = existing { id } else {
        sqlx::query_scalar("INSERT INTO contacts (account_id, name, email, phone_number, identifier, custom_attributes, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,now(),now()) RETURNING id::bigint")
            .bind(account_id as i32).bind(body["name"].as_str().unwrap_or("")).bind(body["email"].as_str()).bind(body["phone_number"].as_str()).bind(id).bind(body.get("custom_attributes").cloned().unwrap_or(json!({}))).fetch_one(&mut *tx).await?
    };
    let ci: i64 = sqlx::query_scalar("INSERT INTO contact_inboxes (inbox_id, contact_id, source_id, pubsub_token, created_at, updated_at) VALUES ($1,$2,$3,$4,now(),now()) RETURNING id")
        .bind(inbox_id).bind(contact_id).bind(source).bind(uuid::Uuid::new_v4().simple().to_string()).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    contact_value(&st, ci).await
}

async fn contact_show(State(st): State<AppState>, Path((ident, source)): Path<(String, String)>, query: axum::extract::Query<std::collections::HashMap<String, String>>) -> ApiResult {
    let (inbox_id, account_id, mandatory, token) = inbox(&st, &ident).await?;
    let (ci, cid) = contact(&st, inbox_id, &source).await?;
    let identifier: Option<String> = sqlx::query_scalar("SELECT identifier FROM contacts WHERE id = $1 AND account_id = $2").bind(cid as i32).bind(account_id as i32).fetch_one(&st.db).await?;
    if verify_hmac(mandatory, token.as_deref(), identifier.as_deref(), query.get("identifier").map(String::as_str), query.get("identifier_hash").map(String::as_str))? {
        sqlx::query("UPDATE contact_inboxes SET hmac_verified = true WHERE id = $1 AND inbox_id = $2").bind(ci).bind(inbox_id).execute(&st.db).await?;
    }
    contact_value(&st, ci).await
}

async fn contact_update(State(st): State<AppState>, Path((ident, source)): Path<(String, String)>, Json(body): Json<Value>) -> ApiResult {
    let (inbox_id, account_id, mandatory, token) = inbox(&st, &ident).await?;
    let (ci, cid) = contact(&st, inbox_id, &source).await?;
    let identifier: Option<String> = sqlx::query_scalar("SELECT identifier FROM contacts WHERE id = $1 AND account_id = $2").bind(cid as i32).bind(account_id as i32).fetch_one(&st.db).await?;
    let verified = verify_hmac(mandatory, token.as_deref(), identifier.as_deref(), body["identifier"].as_str(), body["identifier_hash"].as_str())?;
    sqlx::query("UPDATE contacts SET name = coalesce($1,name), email = coalesce($2,email), phone_number = coalesce($3,phone_number), custom_attributes = coalesce(custom_attributes,'{}'::jsonb) || $4::jsonb, updated_at = now() WHERE id = $5 AND account_id = $6")
        .bind(body["name"].as_str()).bind(body["email"].as_str()).bind(body["phone_number"].as_str()).bind(body.get("custom_attributes").cloned().unwrap_or(json!({}))).bind(cid as i32).bind(account_id as i32).execute(&st.db).await?;
    if verified { sqlx::query("UPDATE contact_inboxes SET hmac_verified = true WHERE id = $1 AND inbox_id = $2").bind(ci).bind(inbox_id).execute(&st.db).await?; }
    contact_value(&st, ci).await
}

async fn show(State(st): State<AppState>, Path(identifier): Path<String>) -> ApiResult {
    let row: (String, Option<bool>, String, Option<String>, Option<bool>, Option<bool>, Option<bool>) = sqlx::query_as(
        "SELECT a.identifier, a.hmac_mandatory, i.name, i.timezone, i.working_hours_enabled, i.csat_survey_enabled, i.greeting_enabled FROM channel_api a JOIN inboxes i ON i.channel_id = a.id AND i.channel_type = 'Channel::Api' AND i.account_id = a.account_id WHERE a.identifier = $1"
    ).bind(&identifier).fetch_one(&st.db).await?;
    Ok(Json(json!({"identifier":row.0,"identity_validation_enabled":row.1,"name":row.2,"timezone":row.3,"working_hours":[],"working_hours_enabled":row.4,"csat_survey_enabled":row.5,"greeting_enabled":row.6})))
}
