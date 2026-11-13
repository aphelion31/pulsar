//! Telegram channel: inbox setup (getMe + setWebhook) and inbound webhook (/webhooks/telegram/{bot_token}).
//! Mirrors chatwoot/app/models/channel/telegram.rb, app/controllers/webhooks/telegram_controller.rb,
//! and app/services/telegram/incoming_message_service.rb.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::{
    common::{ApiError, AppState},
    conversations::{insert_message, reload},
};

pub(crate) fn api_base() -> String {
    std::env::var("TELEGRAM_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://api.telegram.org".to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/webhooks/telegram/{bot_token}", post(webhook))
}

/// Validate bot_token via getMe, insert channel_telegram, and register webhook via setWebhook.
pub(crate) async fn create_channel(
    tx: &mut Transaction<'_, Postgres>,
    account_id: i64,
    ch: &Value,
    frontend_url: &str,
) -> Result<(i64, &'static str), ApiError> {
    let bot_token = ch["bot_token"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Bot token can't be blank"))?;

    let base = api_base();
    let client = reqwest::Client::new();

    // Validate token with Telegram Bot API getMe.
    let me_url = format!("{base}/bot{bot_token}/getMe");
    let me_resp = client
        .get(&me_url)
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("Invalid bot token: {e}")))?;
    if !me_resp.status().is_success() {
        return Err(ApiError::bad("Invalid bot token"));
    }
    let me_bytes = me_resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad(format!("Invalid bot token: {e}")))?;
    let me_json: Value = serde_json::from_slice(&me_bytes)
        .map_err(|_| ApiError::bad("Invalid bot token"))?;
    if me_json["ok"].as_bool() != Some(true) {
        return Err(ApiError::bad("Invalid bot token"));
    }
    let bot_name = me_json["result"]["username"]
        .as_str()
        .map(str::to_string);

    let channel_id: i64 = sqlx::query_scalar(
        "INSERT INTO channel_telegram (account_id, bot_token, bot_name, created_at, updated_at)
         VALUES ($1, $2, $3, now(), now()) RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(bot_token)
    .bind(bot_name)
    .fetch_one(&mut **tx)
    .await?;

    // Register webhook (deleteWebhook best-effort, then setWebhook).
    let _ = client.post(format!("{base}/bot{bot_token}/deleteWebhook")).send().await;
    let webhook_url = format!("{}/webhooks/telegram/{bot_token}", frontend_url.trim_end_matches('/'));
    let hook_body = serde_json::to_vec(&json!({ "url": webhook_url }))
        .map_err(|e| ApiError::bad(format!("Webhook JSON error: {e}")))?;
    let hook_resp = client
        .post(format!("{base}/bot{bot_token}/setWebhook"))
        .header("content-type", "application/json")
        .body(hook_body)
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("Error setting up the webhook: {e}")))?;
    if !hook_resp.status().is_success() {
        return Err(ApiError::bad("Error setting up the webhook"));
    }

    Ok((channel_id, "Channel::Telegram"))
}

/// POST /webhooks/telegram/{bot_token}
async fn webhook(
    State(st): State<AppState>,
    Path(bot_token): Path<String>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let inbox_row = sqlx::query(
        "SELECT i.id::bigint AS inbox_id, i.account_id::bigint AS account_id, i.lock_to_single_conversation
         FROM channel_telegram ct
         JOIN inboxes i ON i.channel_id = ct.id AND i.channel_type = 'Channel::Telegram'
         WHERE ct.bot_token = $1
         LIMIT 1",
    )
    .bind(&bot_token)
    .fetch_optional(&st.db)
    .await?;

    let Some(inbox_row) = inbox_row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Inbox not found" }))).into_response());
    };

    let inbox_id: i64 = inbox_row.try_get("inbox_id")?;
    let account_id: i64 = inbox_row.try_get("account_id")?;
    let lock_to_single: bool = inbox_row.try_get("lock_to_single_conversation")?;

    let callback_query_id = payload
        .get("callback_query")
        .and_then(|cq| cq.get("id"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if let Err(e) = process_update(&st, account_id, inbox_id, lock_to_single, &payload).await {
        tracing::warn!(inbox_id, error = ?e, "telegram inbound processing failed");
    }

    if let Some(cb_id) = callback_query_id {
        return Ok((
            StatusCode::OK,
            Json(json!({ "method": "answerCallbackQuery", "callback_query_id": cb_id })),
        )
            .into_response());
    }
    Ok(StatusCode::OK.into_response())
}

async fn process_update(
    st: &AppState,
    account_id: i64,
    inbox_id: i64,
    lock_to_single: bool,
    payload: &Value,
) -> Result<(), ApiError> {
    let msg = payload
        .get("message")
        .or_else(|| payload.get("business_message"));
    let callback = payload.get("callback_query");

    // Chatwoot only processes private chats (not groups).
    let (base_obj, text, msg_id_str, is_callback) = if let Some(cq) = callback {
        let chat_type = cq["message"]["chat"]["type"].as_str();
        if chat_type != Some("private") {
            return Ok(());
        }
        let data = cq["data"].as_str().unwrap_or("").to_string();
        let id_str = cq["id"].as_str().unwrap_or("").to_string();
        (cq, data, id_str, true)
    } else if let Some(m) = msg {
        let chat_type = m["chat"]["type"].as_str();
        if chat_type != Some("private") {
            return Ok(());
        }
        let text = m["text"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| m["caption"].as_str().filter(|s| !s.is_empty()))
            .unwrap_or("")
            .to_string();
        let id_str = m["message_id"]
            .as_i64()
            .map(|n| n.to_string())
            .or_else(|| m["message_id"].as_str().map(str::to_string))
            .unwrap_or_default();
        (m, text, id_str, false)
    } else {
        return Ok(());
    };

    if msg_id_str.is_empty() || text.is_empty() {
        return Ok(());
    }

    // Deduplicate by message source_id within this inbox.
    let dup: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM messages WHERE inbox_id = $1 AND source_id = $2 LIMIT 1",
    )
    .bind(inbox_id as i32)
    .bind(&msg_id_str)
    .fetch_optional(&st.db)
    .await?;
    if dup.is_some() {
        return Ok(());
    }

    let chat = if is_callback {
        &base_obj["message"]["chat"]
    } else {
        &base_obj["chat"]
    };
    let from = &base_obj["from"];
    let chat_id = chat["id"].as_i64().unwrap_or(0);
    let business_conn_id = base_obj["business_connection_id"].as_str().map(str::to_string);
    let from_id = from["id"].as_i64().unwrap_or(chat_id);
    if from_id == 0 {
        return Ok(());
    }
    let source_id = from_id.to_string();

    let first_name = from["first_name"].as_str().unwrap_or("").trim();
    let last_name = from["last_name"].as_str().unwrap_or("").trim();
    let username = from["username"].as_str().map(str::to_string);
    let language_code = from["language_code"].as_str().map(str::to_string);
    let full_name = format!("{first_name} {last_name}").trim().to_string();
    let contact_name = if !full_name.is_empty() {
        full_name
    } else if let Some(u) = username.as_deref().filter(|s| !s.is_empty()) {
        u.to_string()
    } else {
        source_id.clone()
    };

    let (contact_id, contact_inbox_id) = find_or_create_contact(
        &st.db,
        account_id,
        inbox_id,
        &source_id,
        &contact_name,
        username.as_deref(),
        language_code.as_deref(),
        from_id,
    )
    .await?;

    let conv = find_or_create_conversation(
        &st.db,
        account_id,
        inbox_id,
        contact_id,
        contact_inbox_id,
        lock_to_single,
        chat_id,
        business_conn_id.as_deref(),
    )
    .await?;

    let mut content_attrs = json!({ "__pulsar_source_id": msg_id_str });
    if let Some(reply_to) = base_obj["reply_to_message"]["message_id"].as_i64() {
        content_attrs["in_reply_to_external_id"] = json!(reply_to);
    }

    insert_message(
        st,
        &conv,
        Some(("Contact", contact_id)),
        &text,
        0, // incoming
        false,
        0, // text
        content_attrs,
        None,
    )
    .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn find_or_create_contact(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    source_id: &str,
    name: &str,
    username: Option<&str>,
    language_code: Option<&str>,
    from_id: i64,
) -> Result<(i64, i64), ApiError> {
    if let Some(row) = sqlx::query(
        "SELECT id::bigint AS ci_id, contact_id::bigint AS contact_id
         FROM contact_inboxes
         WHERE inbox_id = $1 AND source_id = $2
         LIMIT 1",
    )
    .bind(inbox_id as i32)
    .bind(source_id)
    .fetch_optional(db)
    .await?
    {
        return Ok((row.try_get("contact_id")?, row.try_get("ci_id")?));
    }

    let additional_attrs = json!({
        "username": username,
        "language_code": language_code,
        "social_telegram_user_id": from_id,
        "social_telegram_user_name": username,
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
    .bind(source_id)
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
    .bind(source_id)
    .fetch_one(db)
    .await?;

    Ok((row.try_get("contact_id")?, row.try_get("ci_id")?))
}

#[allow(clippy::too_many_arguments)]
async fn find_or_create_conversation(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    contact_id: i64,
    contact_inbox_id: i64,
    lock_to_single: bool,
    chat_id: i64,
    business_conn_id: Option<&str>,
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
            let mut attrs = json!({ "chat_id": chat_id });
            if let Some(bc) = business_conn_id.filter(|s| !s.is_empty()) {
                attrs["business_connection_id"] = json!(bc);
            }
            sqlx::query_scalar(
                "INSERT INTO conversations (account_id, inbox_id, status, contact_id, contact_inbox_id,
                                            additional_attributes, custom_attributes, last_activity_at, created_at, updated_at)
                 VALUES ($1, $2, 0, $3, $4, $5, '{}', now(), now(), now()) RETURNING id::bigint",
            )
            .bind(account_id as i32)
            .bind(inbox_id as i32)
            .bind(contact_id)
            .bind(contact_inbox_id)
            .bind(attrs)
            .fetch_one(db)
            .await?
        }
    };

    reload(db, conv_id).await
}
