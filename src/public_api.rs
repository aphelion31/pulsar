use axum::{extract::{Path, State}, routing::get, Json, Router};
use serde_json::{json, Value};
use crate::common::{ApiError, ApiResult, AppState};
use hmac::{Hmac, Mac};
use sha2_compat::Sha256;
use crate::conversations::{Conv, CONV_COLS, MSG_COLS, Msg, emit, emit_conv, msg_json, contact_json};

pub fn routes() -> Router<AppState> {
    Router::new().route("/public/api/v1/inboxes/{inbox_identifier}", get(show))
        .route("/public/api/v1/inboxes/{inbox_identifier}/contacts", axum::routing::post(contact_create))
        .route("/public/api/v1/inboxes/{inbox_identifier}/contacts/{contact_identifier}", get(contact_show).patch(contact_update))
        .route("/public/api/v1/inboxes/{inbox_identifier}/contacts/{contact_identifier}/conversations", get(conv_index).post(conv_create))
        .route("/public/api/v1/inboxes/{inbox_identifier}/contacts/{contact_identifier}/conversations/{id}", get(conv_show))
        .route("/public/api/v1/inboxes/{inbox_identifier}/contacts/{contact_identifier}/conversations/{id}/toggle_status", axum::routing::post(toggle_status))
        .route("/public/api/v1/inboxes/{inbox_identifier}/contacts/{contact_identifier}/conversations/{id}/toggle_typing", axum::routing::post(toggle_typing))
        .route("/public/api/v1/inboxes/{inbox_identifier}/contacts/{contact_identifier}/conversations/{id}/update_last_seen", axum::routing::post(update_last_seen))
}

async fn scoped_conv(st: &AppState, ident: &str, source: &str, display_id: i64) -> Result<Conv, ApiError> {
    let (inbox_id, account_id, _, _) = inbox(st, ident).await?;
    let (ci, _) = contact(st, inbox_id, source).await?;
    Ok(sqlx::query_as(&format!("SELECT {CONV_COLS} FROM conversations c WHERE c.inbox_id = $1 AND c.account_id = $2 AND c.contact_inbox_id = $3 AND c.display_id = $4"))
        .bind(inbox_id as i32).bind(account_id as i32).bind(ci).bind(display_id as i32).fetch_one(&st.db).await?)
}

async fn public_message(st: &AppState, c: &Conv, m: &Msg) -> Value {
    let v = msg_json(&st.db, m, c).await;
    let mut out = json!({"id":v["id"],"content":v["content"],"message_type":v["message_type"],"content_type":v["content_type"],"content_attributes":v["content_attributes"],"created_at":v["created_at"],"conversation_id":v["conversation_id"]});
    for key in ["sender", "attachments"] { if let Some(value) = v.get(key) { out[key] = value.clone(); } }
    out
}

async fn public_conv(st: &AppState, c: &Conv) -> Result<Value, ApiError> {
    let messages: Vec<Msg> = sqlx::query_as(&format!("SELECT {MSG_COLS} FROM messages m WHERE m.conversation_id = $1 AND m.account_id = $2 AND m.private = false AND m.message_type <> 2 ORDER BY m.id"))
        .bind(c.id as i32).bind(c.account_id as i32).fetch_all(&st.db).await?;
    let mut values = Vec::new();
    for m in &messages { values.push(public_message(st, c, m).await); }
    let status = ["open", "resolved", "pending", "snoozed"].get(c.status as usize).unwrap_or(&"open");
    Ok(json!({"id":c.display_id,"uuid":c.uuid,"inbox_id":c.inbox_id,"contact_last_seen_at":c.contact_last_seen_at.map(|v| v.and_utc().timestamp()).unwrap_or(0),"agent_last_seen_at":c.agent_last_seen_at.map(|v| v.and_utc().timestamp()).unwrap_or(0),"status":status,"messages":values,"contact":contact_json(&st.db,c.contact_id).await}))
}

async fn conv_index(State(st): State<AppState>, Path((ident, source)): Path<(String, String)>) -> ApiResult {
    let (inbox_id, account_id, _, _) = inbox(&st, &ident).await?;
    let (ci, _) = contact(&st, inbox_id, &source).await?;
    let rows: Vec<Conv> = sqlx::query_as(&format!("SELECT {CONV_COLS} FROM conversations c WHERE c.inbox_id = $1 AND c.account_id = $2 AND c.contact_inbox_id = $3 ORDER BY c.id DESC"))
        .bind(inbox_id as i32).bind(account_id as i32).bind(ci).fetch_all(&st.db).await?;
    let mut out = Vec::new(); for c in &rows { out.push(public_conv(&st, c).await?); } Ok(Json(json!(out)))
}

async fn conv_create(State(st): State<AppState>, Path((ident, source)): Path<(String, String)>, Json(body): Json<Value>) -> ApiResult {
    let (inbox_id, account_id, _, _) = inbox(&st, &ident).await?;
    let (ci, cid) = contact(&st, inbox_id, &source).await?;
    let id: i64 = sqlx::query_scalar("INSERT INTO conversations (account_id,inbox_id,status,contact_id,contact_inbox_id,additional_attributes,custom_attributes,last_activity_at,created_at,updated_at) VALUES ($1,$2,0,$3,$4,'{}'::jsonb,$5,now(),now(),now()) RETURNING id::bigint")
        .bind(account_id as i32).bind(inbox_id as i32).bind(cid).bind(ci).bind(body.get("custom_attributes").cloned().unwrap_or(json!({}))).fetch_one(&st.db).await?;
    let c = emit_conv(&st, id, "conversation.created").await?;
    Ok(Json(public_conv(&st, &c).await?))
}

async fn conv_show(State(st): State<AppState>, Path((ident, source, id)): Path<(String, String, i64)>) -> ApiResult {
    let c = scoped_conv(&st, &ident, &source, id).await?; Ok(Json(public_conv(&st, &c).await?))
}
async fn toggle_status(State(st): State<AppState>, Path((ident, source, id)): Path<(String, String, i64)>) -> ApiResult {
    let mut c = scoped_conv(&st, &ident, &source, id).await?;
    if c.status != 1 {
        sqlx::query("UPDATE conversations SET status = 1, updated_at = now() WHERE id = $1 AND account_id = $2 AND inbox_id = $3 AND contact_inbox_id = $4").bind(c.id as i32).bind(c.account_id as i32).bind(c.inbox_id as i32).bind(c.contact_inbox_id).execute(&st.db).await?;
        c = emit_conv(&st, c.id, "conversation.status_changed").await?;
    }
    Ok(Json(public_conv(&st, &c).await?))
}
async fn toggle_typing(State(st): State<AppState>, Path((ident, source, id)): Path<(String, String, i64)>, Json(body): Json<Value>) -> Result<axum::http::StatusCode, ApiError> {
    let c = scoped_conv(&st, &ident, &source, id).await?;
    if let Some(event) = match body["typing_status"].as_str() { Some("on") => Some("conversation.typing_on"), Some("off") => Some("conversation.typing_off"), _ => None } {
        emit(&st, &c, event, json!({"conversation":public_conv(&st,&c).await?,"user":contact_json(&st.db,c.contact_id).await})).await;
    }
    Ok(axum::http::StatusCode::OK)
}
async fn update_last_seen(State(st): State<AppState>, Path((ident, source, id)): Path<(String, String, i64)>) -> Result<axum::http::StatusCode, ApiError> {
    let c = scoped_conv(&st, &ident, &source, id).await?;
    sqlx::query("UPDATE conversations SET contact_last_seen_at = now(), updated_at = now() WHERE id = $1 AND account_id = $2 AND inbox_id = $3 AND contact_inbox_id = $4").bind(c.id as i32).bind(c.account_id as i32).bind(c.inbox_id as i32).bind(c.contact_inbox_id).execute(&st.db).await?;
    Ok(axum::http::StatusCode::OK)
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
