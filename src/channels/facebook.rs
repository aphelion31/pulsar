//! Facebook Messenger channel: OAuth page registration/reauth, /bot webhook, and outbound delivery.
//! Mirrors chatwoot/app/controllers/api/v1/accounts/callbacks_controller.rb
//! and app/services/facebook/*.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2_compat::Sha256;
use sqlx::PgPool;
use std::collections::HashMap;

use crate::{
    common::{ApiError, ApiResult, AppState, CurrentUser, require_account},
    conversations::{insert_message, reload},
    events::Event,
};

pub(crate) fn graph_base() -> String {
    std::env::var("META_GRAPH_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://graph.facebook.com".into())
        .trim_end_matches('/')
        .to_string()
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/bot", get(verify_webhook).post(webhook))
}

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/callbacks/register_facebook_page",
            post(register_facebook_page),
        )
        .route("/callbacks/facebook_pages", post(facebook_pages))
        .route("/callbacks/reauthorize_page", post(reauthorize_page))
}

fn verify_meta_sig(secret: &str, sig_header: &str, body: &[u8]) -> bool {
    if secret.is_empty() {
        return false;
    }
    let Some(sig) = sig_header
        .strip_prefix("sha256=")
        .and_then(|s| hex::decode(s).ok())
    else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&sig).is_ok()
}

async fn exchange_long_lived_token(short_token: &str) -> Result<String, ApiError> {
    let app_id = std::env::var("FB_APP_ID").unwrap_or_default();
    let app_secret = std::env::var("FB_APP_SECRET").unwrap_or_default();
    if app_id.is_empty() || app_secret.is_empty() {
        return Ok(short_token.to_string());
    }
    let resp = reqwest::Client::new()
        .get(format!("{}/oauth/access_token", graph_base()))
        .query(&[
            ("grant_type", "fb_exchange_token"),
            ("client_id", app_id.as_str()),
            ("client_secret", app_secret.as_str()),
            ("fb_exchange_token", short_token),
        ])
        .send()
        .await
        .map_err(|_| ApiError::bad("Facebook API unavailable"))?;
    if !resp.status().is_success() {
        return Ok(short_token.to_string());
    }
    let bytes = resp.bytes().await.unwrap_or_default();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Ok(v["access_token"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| short_token.to_string()))
}

async fn register_facebook_page(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let page_id = body["page_id"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("page_id is required"))?;
    let user_token = body["user_access_token"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("user_access_token is required"))?;
    let page_token = body["page_access_token"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("page_access_token is required"))?;
    let inbox_name = body["inbox_name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Facebook Page");

    let mut tx = st.db.begin().await?;
    let channel_id: i32 = sqlx::query_scalar(
        "INSERT INTO channel_facebook_pages (page_id, user_access_token, page_access_token, account_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, now(), now()) RETURNING id",
    )
    .bind(page_id)
    .bind(user_token)
    .bind(page_token)
    .bind(account_id as i32)
    .fetch_one(&mut *tx)
    .await?;

    let inbox_id: i64 = sqlx::query_scalar(
        "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
         VALUES ($1, $2, $3, 'Channel::FacebookPage', now(), now()) RETURNING id::bigint",
    )
    .bind(channel_id)
    .bind(account_id)
    .bind(inbox_name)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO inbox_members (user_id, inbox_id, created_at, updated_at) VALUES ($1, $2, now(), now())",
    )
    .bind(user.id)
    .bind(inbox_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    st.hub.invalidate_cache(account_id, "inbox");
    Ok(Json(
        crate::inboxes::inbox_json(&st.db, account_id, inbox_id, true).await?,
    ))
}

async fn facebook_pages(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let omniauth_token = body["omniauth_token"]
        .as_str()
        .or_else(|| body["user_access_token"].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("omniauth_token is required"))?;
    let long_token = exchange_long_lived_token(omniauth_token).await?;

    let resp = reqwest::Client::new()
        .get(format!("{}/me/accounts", graph_base()))
        .query(&[("access_token", long_token.as_str())])
        .send()
        .await
        .map_err(|_| ApiError::bad("Facebook API unavailable"))?;
    if !resp.status().is_success() {
        return Err(ApiError::bad("Failed to fetch Facebook pages"));
    }
    let bytes = resp.bytes().await.unwrap_or_default();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(json!({ "data": [] }));
    let pages = v["data"].as_array().cloned().unwrap_or_default();

    let existing: Vec<String> =
        sqlx::query_scalar("SELECT page_id FROM channel_facebook_pages WHERE account_id = $1")
            .bind(account_id as i32)
            .fetch_all(&st.db)
            .await?;

    let out: Vec<Value> = pages
        .into_iter()
        .map(|mut p| {
            let pid = p["id"].as_str().unwrap_or("");
            let exists = existing.iter().any(|e| e == pid);
            if let Some(obj) = p.as_object_mut() {
                obj.insert("exists".into(), json!(exists));
            }
            p
        })
        .collect();

    Ok(Json(
        json!({ "data": { "pages": out, "access_token": long_token } }),
    ))
}

async fn reauthorize_page(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let inbox_id = body["inbox_id"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("inbox_id is required"))?;
    let omniauth_token = body["omniauth_token"]
        .as_str()
        .or_else(|| body["user_access_token"].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("omniauth_token is required"))?;

    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT f.id::bigint, f.page_id
         FROM inboxes i
         JOIN channel_facebook_pages f ON f.id = i.channel_id
         WHERE i.id::bigint = $1 AND i.account_id::bigint = $2 AND i.channel_type = 'Channel::FacebookPage'",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?;
    let Some((channel_id, fb_page_id)) = row else {
        return Ok(StatusCode::UNPROCESSABLE_ENTITY.into_response());
    };

    let long_token = exchange_long_lived_token(omniauth_token).await?;
    let resp = reqwest::Client::new()
        .get(format!("{}/me/accounts", graph_base()))
        .query(&[("access_token", long_token.as_str())])
        .send()
        .await
        .map_err(|_| ApiError::bad("Facebook API unavailable"))?;
    let bytes = resp.bytes().await.unwrap_or_default();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(json!({ "data": [] }));
    let found = v["data"].as_array().and_then(|arr| {
        arr.iter()
            .find(|p| p["id"].as_str() == Some(fb_page_id.as_str()))
    });
    let Some(page_detail) = found else {
        return Ok(StatusCode::UNPROCESSABLE_ENTITY.into_response());
    };
    let page_token = page_detail["access_token"].as_str().unwrap_or("");
    sqlx::query(
        "UPDATE channel_facebook_pages
         SET user_access_token = $2, page_access_token = $3, updated_at = now()
         WHERE id = $1",
    )
    .bind(channel_id as i32)
    .bind(&long_token)
    .bind(page_token)
    .execute(&st.db)
    .await?;

    Ok(Json(crate::inboxes::inbox_json(&st.db, account_id, inbox_id, true).await?).into_response())
}

async fn verify_webhook(Query(q): Query<HashMap<String, String>>) -> Response {
    let mode = q.get("hub.mode").map(String::as_str);
    let token = q.get("hub.verify_token").map(String::as_str);
    let challenge = q.get("hub.challenge").cloned().unwrap_or_default();
    let expected = std::env::var("FB_VERIFY_TOKEN").unwrap_or_default();
    if mode == Some("subscribe") && !expected.is_empty() && token == Some(expected.as_str()) {
        (StatusCode::OK, challenge).into_response()
    } else {
        StatusCode::FORBIDDEN.into_response()
    }
}

async fn webhook(State(st): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let secret = std::env::var("FB_APP_SECRET").unwrap_or_default();
    let sig = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !verify_meta_sig(&secret, sig, &body) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if let Err(e) = process_facebook_entries(&st, &payload).await {
        tracing::warn!(error = ?e, "facebook webhook processing failed");
    }
    (StatusCode::OK, "EVENT_RECEIVED").into_response()
}

pub(crate) async fn find_or_create_social_contact(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    source_id: &str,
    default_name: &str,
) -> Result<(i64, i64), ApiError> {
    if let Some((cid, ci_id)) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT contact_id::bigint, id::bigint FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2 LIMIT 1",
    )
    .bind(inbox_id)
    .bind(source_id)
    .fetch_optional(db)
    .await?
    {
        return Ok((cid, ci_id));
    }

    let contact_id: i64 = sqlx::query_scalar(
        "INSERT INTO contacts (account_id, name, contact_type, created_at, updated_at)
         VALUES ($1, $2, 0, now(), now()) RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(default_name)
    .fetch_one(db)
    .await?;

    let ci_id: i64 = sqlx::query_scalar(
        "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at)
         VALUES ($1, $2, $3, now(), now()) RETURNING id::bigint",
    )
    .bind(contact_id)
    .bind(inbox_id)
    .bind(source_id)
    .fetch_one(db)
    .await?;

    Ok((contact_id, ci_id))
}

pub(crate) async fn find_or_create_social_conversation(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    contact_id: i64,
    contact_inbox_id: i64,
    lock_single: bool,
) -> Result<crate::conversations::Conv, ApiError> {
    let existing: Option<i64> = if lock_single {
        sqlx::query_scalar(
            "SELECT id::bigint FROM conversations WHERE account_id = $1 AND inbox_id = $2 AND contact_inbox_id = $3 ORDER BY id DESC LIMIT 1",
        )
        .bind(account_id as i32)
        .bind(inbox_id as i32)
        .bind(contact_inbox_id)
        .fetch_optional(db)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT id::bigint FROM conversations WHERE account_id = $1 AND inbox_id = $2 AND contact_inbox_id = $3 AND status <> 1 ORDER BY id DESC LIMIT 1",
        )
        .bind(account_id as i32)
        .bind(inbox_id as i32)
        .bind(contact_inbox_id)
        .fetch_optional(db)
        .await?
    };

    if let Some(conv_id) = existing {
        return reload(db, conv_id).await;
    }

    let conv_id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations
         (account_id, inbox_id, contact_id, contact_inbox_id, status, display_id, uuid, last_activity_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 0,
                 COALESCE((SELECT MAX(display_id) FROM conversations WHERE account_id = $1), 0) + 1,
                 gen_random_uuid(), now(), now(), now())
         RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(inbox_id as i32)
    .bind(contact_id)
    .bind(contact_inbox_id)
    .fetch_one(db)
    .await?;

    reload(db, conv_id).await
}

async fn process_facebook_entries(st: &AppState, payload: &Value) -> Result<(), ApiError> {
    let Some(entries) = payload["entry"].as_array() else {
        return Ok(());
    };
    for entry in entries {
        let Some(page_id) = entry["id"].as_str() else {
            continue;
        };
        let channel: Option<(i64, i64, bool)> = sqlx::query_as(
            "SELECT i.id::bigint, i.account_id::bigint, i.lock_to_single_conversation
             FROM channel_facebook_pages f
             JOIN inboxes i ON i.channel_id = f.id AND i.account_id = f.account_id AND i.channel_type = 'Channel::FacebookPage'
             WHERE f.page_id = $1 LIMIT 1",
        )
        .bind(page_id)
        .fetch_optional(&st.db)
        .await?;
        let Some((inbox_id, account_id, lock_single)) = channel else {
            continue;
        };

        let Some(messaging) = entry["messaging"].as_array() else {
            continue;
        };
        for ev in messaging {
            let Some(msg) = ev.get("message").filter(|m| m.is_object()) else {
                continue;
            };
            let is_echo = msg["is_echo"].as_bool().unwrap_or(false);
            let Some(mid) = msg["mid"].as_str().filter(|s| !s.is_empty()) else {
                continue;
            };
            let dup: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM messages WHERE inbox_id = $1 AND source_id = $2)",
            )
            .bind(inbox_id as i32)
            .bind(mid)
            .fetch_one(&st.db)
            .await?;
            if dup {
                continue;
            }
            let sender_id = if is_echo {
                ev["recipient"]["id"].as_str()
            } else {
                ev["sender"]["id"].as_str()
            };
            let Some(psid) = sender_id.filter(|s| !s.is_empty()) else {
                continue;
            };
            let content = msg["text"].as_str().unwrap_or("");
            let (contact_id, ci_id) = find_or_create_social_contact(
                &st.db,
                account_id,
                inbox_id,
                psid,
                &format!("Facebook User {psid}"),
            )
            .await?;
            let conv = find_or_create_social_conversation(
                &st.db,
                account_id,
                inbox_id,
                contact_id,
                ci_id,
                lock_single,
            )
            .await?;
            let msg_type = if is_echo { 1 } else { 0 };
            let sender = (!is_echo).then_some(("Contact", contact_id));
            insert_message(
                st,
                &conv,
                sender,
                content,
                msg_type,
                false,
                0,
                json!({}),
                Some(mid),
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn outbound(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    if let Err(e) = send_outbound(&st, &ev).await {
        tracing::warn!(error = %e, "Facebook outbound failed");
    }
}

async fn send_outbound(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(msg_id) = ev.data["id"].as_i64() else {
        return Ok(());
    };
    let row: Option<(Option<String>, String, String, String)> = sqlx::query_as(
        "SELECT m.content, ci.source_id, f.page_id, f.page_access_token
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
         JOIN contact_inboxes ci ON ci.id = c.contact_inbox_id
         JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::FacebookPage'
         JOIN channel_facebook_pages f ON f.id = i.channel_id AND f.account_id = i.account_id
         WHERE m.id = $1 AND m.account_id = $2 AND m.message_type = 1 AND NOT COALESCE(m.private, false)",
    )
    .bind(msg_id)
    .bind(ev.account_id)
    .fetch_optional(&st.db)
    .await?;

    let Some((content, recipient_id, page_id, token)) = row else {
        return Ok(());
    };
    let body_bytes = serde_json::to_vec(&json!({
        "recipient": { "id": recipient_id },
        "messaging_type": "RESPONSE",
        "message": { "text": content.unwrap_or_default() },
    }))?;
    let resp = reqwest::Client::new()
        .post(format!("{}/v19.0/{page_id}/messages", graph_base()))
        .query(&[("access_token", token.as_str())])
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body_bytes)
        .send()
        .await?;
    let status = resp.status();
    let bytes = resp.bytes().await.unwrap_or_default();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if !status.is_success() {
        sqlx::query(
            "UPDATE messages SET status = 3, external_error = $2, updated_at = now() WHERE id = $1",
        )
        .bind(msg_id)
        .bind(
            v["error"]["message"]
                .as_str()
                .unwrap_or("Facebook send failed"),
        )
        .execute(&st.db)
        .await?;
        return Ok(());
    }
    if let Some(mid) = v["message_id"].as_str() {
        sqlx::query(
            "UPDATE messages SET source_id = $2, status = 1, updated_at = now() WHERE id = $1",
        )
        .bind(msg_id)
        .bind(mid)
        .execute(&st.db)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_signature_verification() {
        let secret = "test-secret";
        let body = br#"{"object":"page"}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_meta_sig(secret, &sig, body));
        assert!(!verify_meta_sig(secret, "sha256=deadbeef", body));
    }
}
