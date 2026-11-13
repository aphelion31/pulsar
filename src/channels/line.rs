//! LINE channel: inbox setup and inbound webhook (/webhooks/line/{line_channel_id}).
//! Mirrors chatwoot/app/models/channel/line.rb, app/controllers/webhooks/line_controller.rb,
//! app/jobs/webhooks/line_events_job.rb, and app/services/line/incoming_message_service.rb.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::{
    common::{ApiError, AppState},
    conversations::{insert_message, reload},
};

pub(crate) fn api_base() -> String {
    std::env::var("LINE_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://api.line.me".to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/webhooks/line/{line_channel_id}", post(webhook))
}

/// Validate LINE channel fields and insert into channel_line.
pub(crate) async fn create_channel(
    tx: &mut Transaction<'_, Postgres>,
    account_id: i64,
    ch: &Value,
) -> Result<(i64, &'static str), ApiError> {
    let line_channel_id = ch["line_channel_id"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Line channel id can't be blank"))?;
    let line_channel_secret = ch["line_channel_secret"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Line channel secret can't be blank"))?;
    let line_channel_token = ch["line_channel_token"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Line channel token can't be blank"))?;

    let dup: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM channel_line WHERE line_channel_id = $1 LIMIT 1",
    )
    .bind(line_channel_id)
    .fetch_optional(&mut **tx)
    .await?;
    if dup.is_some() {
        return Err(ApiError::bad("Line channel has already been taken"));
    }

    let channel_id: i64 = sqlx::query_scalar(
        "INSERT INTO channel_line (account_id, line_channel_id, line_channel_secret, line_channel_token, created_at, updated_at)
         VALUES ($1, $2, $3, $4, now(), now()) RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(line_channel_id)
    .bind(line_channel_secret)
    .bind(line_channel_token)
    .fetch_one(&mut **tx)
    .await?;

    Ok((channel_id, "Channel::Line"))
}

fn verify_signature(secret: &str, raw_body: &[u8], signature: Option<&str>) -> bool {
    let Some(sig) = signature.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let Ok(sig_bytes) = STANDARD.decode(sig) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<sha2_compat::Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(raw_body);
    mac.verify_slice(&sig_bytes).is_ok()
}

/// POST /webhooks/line/{line_channel_id}
async fn webhook(
    State(st): State<AppState>,
    Path(line_channel_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let inbox_row = sqlx::query(
        "SELECT i.id::bigint AS inbox_id, i.account_id::bigint AS account_id,
                i.lock_to_single_conversation, cl.line_channel_secret, cl.line_channel_token
         FROM channel_line cl
         JOIN inboxes i ON i.channel_id = cl.id AND i.channel_type = 'Channel::Line'
         WHERE cl.line_channel_id = $1
         LIMIT 1",
    )
    .bind(&line_channel_id)
    .fetch_optional(&st.db)
    .await?;

    let Some(inbox_row) = inbox_row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Inbox not found" }))).into_response());
    };

    let secret: String = inbox_row.try_get("line_channel_secret")?;
    let sig = headers.get("x-line-signature").and_then(|v| v.to_str().ok());
    if !verify_signature(&secret, &body, sig) {
        return Ok((StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid signature" }))).into_response());
    }

    let payload: Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::bad("Invalid JSON payload"))?;

    let inbox_id: i64 = inbox_row.try_get("inbox_id")?;
    let account_id: i64 = inbox_row.try_get("account_id")?;
    let lock_to_single: bool = inbox_row.try_get("lock_to_single_conversation")?;
    let token: String = inbox_row.try_get("line_channel_token")?;

    if let Err(e) = process_events(&st, account_id, inbox_id, lock_to_single, &token, &payload).await {
        tracing::warn!(inbox_id, error = ?e, "line inbound processing failed");
    }

    Ok(StatusCode::OK.into_response())
}

async fn process_events(
    st: &AppState,
    account_id: i64,
    inbox_id: i64,
    lock_to_single: bool,
    channel_token: &str,
    payload: &Value,
) -> Result<(), ApiError> {
    let Some(events) = payload.get("events").and_then(Value::as_array) else {
        return Ok(());
    };

    let client = reqwest::Client::new();
    let base = api_base();

    for event in events {
        if event["type"].as_str() != Some("message") {
            continue;
        }
        if event["message"]["type"].as_str() != Some("text") {
            continue;
        }
        let Some(user_id) = event["source"]["userId"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(msg_id) = event["message"]["id"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(text) = event["message"]["text"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };

        // Deduplicate by message source_id within this inbox.
        let dup: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM messages WHERE inbox_id = $1 AND source_id = $2 LIMIT 1",
        )
        .bind(inbox_id as i32)
        .bind(msg_id)
        .fetch_optional(&st.db)
        .await?;
        if dup.is_some() {
            continue;
        }

        let display_name = fetch_profile_name(&client, &base, channel_token, user_id).await;

        let (contact_id, contact_inbox_id) = find_or_create_contact(
            &st.db,
            account_id,
            inbox_id,
            user_id,
            &display_name,
        )
        .await?;

        let conv = find_or_create_conversation(
            &st.db,
            account_id,
            inbox_id,
            contact_id,
            contact_inbox_id,
            lock_to_single,
        )
        .await?;

        insert_message(
            st,
            &conv,
            Some(("Contact", contact_id)),
            text,
            0, // incoming
            false,
            0, // text
            json!({ "__pulsar_source_id": msg_id }),
            None,
        )
        .await?;
    }

    Ok(())
}

async fn fetch_profile_name(
    client: &reqwest::Client,
    base: &str,
    channel_token: &str,
    user_id: &str,
) -> String {
    let url = format!("{base}/v2/bot/profile/{user_id}");
    let Ok(resp) = client
        .get(&url)
        .header("Authorization", format!("Bearer {channel_token}"))
        .send()
        .await
    else {
        return user_id.to_string();
    };
    if !resp.status().is_success() {
        return user_id.to_string();
    }
    let Ok(bytes) = resp.bytes().await else {
        return user_id.to_string();
    };
    let Ok(json) = serde_json::from_slice::<Value>(&bytes) else {
        return user_id.to_string();
    };
    json["displayName"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(user_id)
        .to_string()
}

async fn find_or_create_contact(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    user_id: &str,
    name: &str,
) -> Result<(i64, i64), ApiError> {
    if let Some(row) = sqlx::query(
        "SELECT id::bigint AS ci_id, contact_id::bigint AS contact_id
         FROM contact_inboxes
         WHERE inbox_id = $1 AND source_id = $2
         LIMIT 1",
    )
    .bind(inbox_id as i32)
    .bind(user_id)
    .fetch_optional(db)
    .await?
    {
        return Ok((row.try_get("contact_id")?, row.try_get("ci_id")?));
    }

    let additional_attrs = json!({
        "social_line_user_id": user_id,
    });

    let contact_id: i64 = sqlx::query_scalar(
        "INSERT INTO contacts (name, account_id, additional_attributes, created_at, updated_at, last_activity_at)
         VALUES ($1, $2, $3, now(), now(), now()) RETURNING id::bigint",
    )
    .bind(name)
    .bind(account_id as i32)
    .bind(additional_attrs)
    .fetch_one(db)
    .await?;

    sqlx::query(
        "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at, pubsub_token)
         VALUES ($1, $2, $3, now(), now(), $4)
         ON CONFLICT (inbox_id, source_id) DO NOTHING",
    )
    .bind(contact_id)
    .bind(inbox_id as i32)
    .bind(user_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(db)
    .await?;

    let row = sqlx::query(
        "SELECT id::bigint AS ci_id, contact_id::bigint AS contact_id
         FROM contact_inboxes
         WHERE inbox_id = $1 AND source_id = $2
         LIMIT 1",
    )
    .bind(inbox_id as i32)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    Ok((row.try_get("contact_id")?, row.try_get("ci_id")?))
}

async fn find_or_create_conversation(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    contact_id: i64,
    contact_inbox_id: i64,
    lock_to_single: bool,
) -> Result<crate::conversations::Conv, ApiError> {
    let existing: Option<i64> = if lock_to_single {
        sqlx::query_scalar(
            "SELECT id::bigint FROM conversations
             WHERE contact_inbox_id = $1 AND account_id = $2
             ORDER BY id DESC LIMIT 1",
        )
        .bind(contact_inbox_id)
        .bind(account_id as i32)
        .fetch_optional(db)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT id::bigint FROM conversations
             WHERE contact_inbox_id = $1 AND account_id = $2 AND status <> 1
             ORDER BY id DESC LIMIT 1",
        )
        .bind(contact_inbox_id)
        .bind(account_id as i32)
        .fetch_optional(db)
        .await?
    };

    let conv_id = match existing {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO conversations (account_id, inbox_id, status, contact_id, contact_inbox_id,
                                            additional_attributes, custom_attributes, last_activity_at, created_at, updated_at)
                 VALUES ($1, $2, 0, $3, $4, '{}', '{}', now(), now(), now()) RETURNING id::bigint",
            )
            .bind(account_id as i32)
            .bind(inbox_id as i32)
            .bind(contact_id)
            .bind(contact_inbox_id)
            .fetch_one(db)
            .await?
        }
    };

    reload(db, conv_id).await
}
