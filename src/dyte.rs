//! Dyte (Cloudflare RealtimeKit) integration: meeting creation and participant tokens.
//! Mirrors chatwoot/app/controllers/api/v1/accounts/integrations/dyte_controller.rb,
//! chatwoot/app/controllers/api/v1/widget/integrations/dyte_controller.rb,
//! chatwoot/lib/integrations/dyte/processor_service.rb, and chatwoot/lib/dyte.rb.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2_compat::Sha256;
use sqlx::PgPool;
use std::sync::OnceLock;

use crate::{
    common::{require_account, AccountUser, ApiError, AppState, CurrentUser},
    conversations::{insert_message, Conv, CONV_COLS},
};

const DEFAULT_DYTE_API_URL: &str = "https://api.cloudflare.com/client/v4";
const PRESET_NAME: &str = "group-call-host";
const LEGACY_PRESET_NAME: &str = "group_call_host";
const MSG_INVALID_TYPE: &str = "Invalid message type. Action not permitted";
const MSG_MISSING_CREDS: &str = "Cloudflare RealtimeKit credentials are required. Delete the existing Dyte integration and create it again with Cloudflare RealtimeKit credentials.";

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/integrations/dyte/create_a_meeting", post(create_a_meeting))
        .route("/integrations/dyte/add_participant_to_meeting", post(add_participant_to_meeting))
}

pub fn widget_routes() -> Router<AppState> {
    Router::new().route(
        "/api/v1/widget/integrations/dyte/add_participant_to_meeting",
        post(widget_add_participant_to_meeting),
    )
}

fn dyte_base_url() -> String {
    std::env::var("DYTE_API_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_DYTE_API_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

struct DyteCreds {
    account_id: String,
    app_id: String,
    api_token: String,
}

async fn load_dyte_creds(db: &PgPool, account_id: i64) -> Result<Option<DyteCreds>, ApiError> {
    let settings: Option<Value> = sqlx::query_scalar(
        "SELECT settings FROM integrations_hooks WHERE account_id = $1::bigint AND app_id = 'dyte' ORDER BY id LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(db)
    .await?;

    let settings = settings.ok_or_else(ApiError::not_found)?;
    let s = |k: &str| {
        settings
            .get(k)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    match (s("account_id"), s("app_id"), s("api_token")) {
        (Some(acct), Some(app), Some(tok)) => Ok(Some(DyteCreds {
            account_id: acct,
            app_id: app,
            api_token: tok,
        })),
        _ => Ok(None),
    }
}

async fn authorize_conv(db: &PgPool, au: &AccountUser, c: &Conv) -> Result<(), ApiError> {
    if !au.is_admin() {
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inbox_members WHERE inbox_id = $1 AND user_id = $2)",
        )
        .bind(c.inbox_id)
        .bind(au.user_id)
        .fetch_one(db)
        .await?;
        if !member {
            return Err(ApiError::not_found());
        }
    }
    if au.custom_perms.is_some() && !au.is_admin() {
        let ok: bool = sqlx::query_scalar(&format!(
            "SELECT {} FROM conversations c WHERE c.id = $1",
            au.custom_scope_sql("$2")
        ))
        .bind(c.id)
        .bind(au.user_id)
        .fetch_one(db)
        .await?;
        if !ok {
            return Err(ApiError::not_found());
        }
    }
    Ok(())
}

fn json_id(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn render_dyte_response(v: Value) -> Response {
    let has_error = v.get("error").is_some_and(|e| !e.is_null());
    let status = if has_error { StatusCode::UNPROCESSABLE_ENTITY } else { StatusCode::OK };
    (status, Json(v)).into_response()
}

// ---------- Dyte HTTP client (mirrors lib/dyte.rb) ----------

async fn dyte_request(
    client: &reqwest::Client,
    creds: &DyteCreds,
    method: reqwest::Method,
    path: &str,
    payload: Option<Value>,
) -> Value {
    let url = format!(
        "{}/accounts/{}/realtime/kit/{}/{}",
        dyte_base_url(),
        creds.account_id,
        creds.app_id,
        path.trim_start_matches('/')
    );
    let mut req = client
        .request(method, &url)
        .header("Authorization", format!("Bearer {}", creds.api_token))
        .header("Content-Type", "application/json");
    if let Some(body) = payload {
        req = req.body(body.to_string());
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return json!({ "error": e.to_string(), "error_code": 502 }),
    };
    let code = resp.status().as_u16();
    let ok = resp.status().is_success();
    let parsed: Value = resp
        .text()
        .await
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    if !ok {
        return json!({ "error": parsed, "error_code": code });
    }
    match parsed.get("data") {
        Some(d) if d.is_object() || d.is_array() => d.clone(),
        _ => json!({ "error": "unexpected_response", "error_code": code }),
    }
}

fn preset_not_found(resp: &Value) -> bool {
    let Some(err) = resp.get("error") else { return false };
    let msg = err
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .or_else(|| err.get("message").and_then(|m| m.as_str()))
        .or_else(|| err.as_str())
        .unwrap_or("");
    msg.contains("No preset found")
}

async fn dyte_create_meeting(client: &reqwest::Client, creds: &DyteCreds, title: &str) -> Value {
    dyte_request(client, creds, reqwest::Method::POST, "meetings", Some(json!({ "title": title }))).await
}

async fn dyte_add_participant(
    client: &reqwest::Client,
    creds: &DyteCreds,
    meeting_id: &str,
    client_id: &str,
    name: &str,
    avatar_url: &str,
) -> Value {
    let path = format!("meetings/{meeting_id}/participants");
    let mut payload = json!({
        "custom_participant_id": client_id,
        "name": name,
        "picture": avatar_url,
        "preset_name": PRESET_NAME
    });
    let resp = dyte_request(client, creds, reqwest::Method::POST, &path, Some(payload.clone())).await;
    if !preset_not_found(&resp) {
        return resp;
    }
    payload["preset_name"] = json!(LEGACY_PRESET_NAME);
    dyte_request(client, creds, reqwest::Method::POST, &path, Some(payload)).await
}

async fn dyte_refresh_token(
    client: &reqwest::Client,
    creds: &DyteCreds,
    meeting_id: &str,
    participant_id: &str,
) -> Value {
    let path = format!("meetings/{meeting_id}/participants/{participant_id}/token");
    dyte_request(client, creds, reqwest::Method::POST, &path, None).await
}

async fn dyte_fetch_participants(
    client: &reqwest::Client,
    creds: &DyteCreds,
    meeting_id: &str,
) -> Value {
    let path = format!("meetings/{meeting_id}/participants");
    dyte_request(client, creds, reqwest::Method::GET, &path, None).await
}

// ---------- ProcessorService logic ----------

async fn store_participant_id(
    db: &PgPool,
    message_id: i64,
    attrs: &Value,
    client_id: &str,
    participant_id: &str,
) {
    let mut updated = if attrs.is_object() { attrs.clone() } else { json!({}) };
    let data = updated.as_object_mut().unwrap().entry("data").or_insert_with(|| json!({}));
    if !data.is_object() {
        *data = json!({});
    }
    let participants = data.as_object_mut().unwrap().entry("participants").or_insert_with(|| json!({}));
    if !participants.is_object() {
        *participants = json!({});
    }
    participants.as_object_mut().unwrap().insert(client_id.to_string(), json!(participant_id));
    if let Err(e) = sqlx::query(
        "UPDATE messages SET content_attributes = $2::json, updated_at = now() WHERE id = $1::bigint",
    )
    .bind(message_id)
    .bind(updated)
    .execute(db)
    .await
    {
        tracing::warn!("[dyte] Failed to store RealtimeKit participant ID for message {message_id}: {e}");
    }
}

async fn add_participant_flow(
    db: &PgPool,
    creds: &DyteCreds,
    message_id: i64,
    attrs: &Value,
    client_id: &str,
    name: &str,
    avatar_url: &str,
) -> Response {
    let meeting_id = attrs
        .get("data")
        .and_then(|d| d.get("meeting_id"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    if meeting_id.is_empty() {
        return render_dyte_response(json!({ "error": "Missing meeting_id" }));
    }

    let client = reqwest::Client::new();
    if let Some(pid) = attrs
        .get("data")
        .and_then(|d| d.get("participants"))
        .and_then(|p| p.get(client_id))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        let resp = dyte_refresh_token(&client, creds, meeting_id, pid).await;
        if resp.get("error").is_none() {
            return render_dyte_response(resp);
        }
    }

    let resp = dyte_add_participant(&client, creds, meeting_id, client_id, name, avatar_url).await;
    if resp.get("error").is_none() {
        if let Some(pid) = resp.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            store_participant_id(db, message_id, attrs, client_id, pid).await;
        }
        return render_dyte_response(resp);
    }

    let participants = dyte_fetch_participants(&client, creds, meeting_id).await;
    if let Some(arr) = participants.as_array() {
        if let Some(pid) = arr.iter().find_map(|p| {
            let cid = p.get("custom_participant_id").and_then(|v| {
                v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string()))
            })?;
            if cid == client_id {
                p.get("id").and_then(|v| v.as_str()).map(str::to_string)
            } else {
                None
            }
        }) {
            let token_resp = dyte_refresh_token(&client, creds, meeting_id, &pid).await;
            if token_resp.get("error").is_none() {
                store_participant_id(db, message_id, attrs, client_id, &pid).await;
            }
            return render_dyte_response(token_resp);
        }
    }

    render_dyte_response(resp)
}

// ---------- Account handlers ----------

async fn create_a_meeting(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    let au = require_account(&st.db, &user, account_id).await?;
    let conv_display_id = json_id(&body["conversation_id"]).ok_or_else(ApiError::not_found)?;
    let c: Conv = sqlx::query_as(&format!(
        "SELECT {CONV_COLS} FROM conversations c WHERE c.account_id = $1 AND c.display_id = $2"
    ))
    .bind(account_id)
    .bind(conv_display_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    authorize_conv(&st.db, &au, &c).await?;

    let Some(creds) = load_dyte_creds(&st.db, account_id).await? else {
        return Ok(render_dyte_response(json!({ "error": MSG_MISSING_CREDS })));
    };

    let available_name: String = sqlx::query_scalar(
        "SELECT COALESCE(NULLIF(display_name, ''), name) FROM users WHERE id = $1::bigint",
    )
    .bind(user.id)
    .fetch_optional(&st.db)
    .await?
    .unwrap_or_else(|| user.name.clone());
    let title = format!("{available_name} has started a meeting");

    let client = reqwest::Client::new();
    let meeting = dyte_create_meeting(&client, &creds, &title).await;
    if meeting.get("error").is_some() {
        return Ok(render_dyte_response(meeting));
    }

    let meeting_id = meeting.get("id").cloned().unwrap_or(Value::Null);
    let attrs = json!({
        "type": "dyte",
        "data": { "meeting_id": meeting_id }
    });
    let msg = insert_message(
        &st,
        &c,
        Some(("User", user.id)),
        &title,
        1,
        false,
        10, // content_type: integrations
        attrs,
        None,
    )
    .await?;

    Ok((StatusCode::OK, Json(msg)).into_response())
}

async fn add_participant_to_meeting(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    let au = require_account(&st.db, &user, account_id).await?;
    let message_id = json_id(&body["message_id"]).ok_or_else(ApiError::not_found)?;

    let (conv_id, content_type, attrs): (i64, i32, Option<Value>) = sqlx::query_as(
        "SELECT conversation_id::bigint, content_type, content_attributes::jsonb
         FROM messages WHERE id = $1::bigint AND account_id = $2::bigint",
    )
    .bind(message_id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let c: Conv = sqlx::query_as(&format!(
        "SELECT {CONV_COLS} FROM conversations c WHERE c.account_id = $1 AND c.id = $2"
    ))
    .bind(account_id)
    .bind(conv_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    authorize_conv(&st.db, &au, &c).await?;

    if content_type != 10 {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": MSG_INVALID_TYPE }))).into_response());
    }
    let Some(creds) = load_dyte_creds(&st.db, account_id).await? else {
        return Ok(render_dyte_response(json!({ "error": MSG_MISSING_CREDS })));
    };

    let client_id = format!("User:{}", user.id);
    let avatar_url = format!("{}/integrations/slack/user.png", st.frontend_url.trim_end_matches('/'));
    let name = if user.name.trim().is_empty() { "Agent" } else { user.name.as_str() };
    let attrs = attrs.unwrap_or_else(|| json!({}));

    Ok(add_participant_flow(&st.db, &creds, message_id, &attrs, &client_id, name, &avatar_url).await)
}

// ---------- Widget handler ----------

#[derive(Deserialize, Default)]
struct WidgetQuery {
    website_token: Option<String>,
}

#[derive(Deserialize)]
struct WidgetClaims {
    source_id: String,
    inbox_id: i64,
    exp: i64,
}

fn widget_secret() -> &'static [u8] {
    static S: OnceLock<Vec<u8>> = OnceLock::new();
    S.get_or_init(|| match std::env::var("SECRET_KEY_BASE") {
        Ok(v) if !v.is_empty() => v.into_bytes(),
        _ => b"pulsar-dev-secret-change-me".to_vec(),
    })
}

fn decode_widget_token(token: &str, inbox_id: i64) -> Option<WidgetClaims> {
    let mut it = token.split('.');
    let (h, p, s) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let input = &token[..h.len() + 1 + p.len()];
    let mut mac = Hmac::<Sha256>::new_from_slice(widget_secret()).ok()?;
    mac.update(input.as_bytes());
    let got = URL_SAFE_NO_PAD.decode(s).ok()?;
    mac.verify_slice(&got).ok()?;
    let claims: WidgetClaims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(p).ok()?).ok()?;
    if claims.exp < chrono::Utc::now().timestamp() || claims.inbox_id != inbox_id {
        return None;
    }
    Some(claims)
}

async fn widget_add_participant_to_meeting(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<WidgetQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    let website_token = q
        .website_token
        .as_deref()
        .or_else(|| body.get("website_token").and_then(|v| v.as_str()))
        .ok_or_else(ApiError::not_found)?;

    let (inbox_id, account_id, account_status): (i64, i64, Option<i32>) = sqlx::query_as(
        "SELECT i.id::bigint, i.account_id::bigint, a.status
         FROM channel_web_widgets w
         JOIN inboxes i ON i.channel_id = w.id AND i.channel_type = 'Channel::WebWidget'
         JOIN accounts a ON a.id = i.account_id
         WHERE w.website_token = $1",
    )
    .bind(website_token)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    if account_status == Some(1) {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "Account is suspended".into()));
    }

    let auth_tok = headers
        .get("x-auth-token")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::unauthorized)?;
    let claims = decode_widget_token(auth_tok, inbox_id).ok_or_else(ApiError::unauthorized)?;

    let (contact_id, contact_name): (i64, Option<String>) = sqlx::query_as(
        "SELECT c.id::bigint, c.name
         FROM contact_inboxes ci
         JOIN contacts c ON c.id = ci.contact_id
         WHERE ci.inbox_id = $1::bigint AND ci.source_id = $2 AND c.account_id = $3::bigint",
    )
    .bind(inbox_id)
    .bind(&claims.source_id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::unauthorized)?;

    let message_id = json_id(&body["message_id"]).ok_or_else(ApiError::not_found)?;
    let (content_type, attrs): (i32, Option<Value>) = sqlx::query_as(
        "SELECT m.content_type, m.content_attributes::jsonb
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE m.id = $1::bigint AND c.inbox_id = $2::bigint AND c.contact_id = $3::bigint",
    )
    .bind(message_id)
    .bind(inbox_id)
    .bind(contact_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    if content_type != 10 {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": MSG_INVALID_TYPE }))).into_response());
    }
    let Some(creds) = load_dyte_creds(&st.db, account_id).await? else {
        return Ok(render_dyte_response(json!({ "error": MSG_MISSING_CREDS })));
    };

    let client_id = format!("Contact:{contact_id}");
    let name = contact_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Visitor");
    let avatar_url = format!("{}/integrations/slack/user.png", st.frontend_url.trim_end_matches('/'));
    let attrs = attrs.unwrap_or_else(|| json!({}));

    Ok(add_participant_flow(&st.db, &creds, message_id, &attrs, &client_id, name, &avatar_url).await)
}
