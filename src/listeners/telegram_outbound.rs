//! Outbound Telegram listener on message.created.
//! Mirrors chatwoot/app/services/telegram/send_on_telegram_service.rb and channel/telegram.rb.

use serde_json::{json, Value};
use sqlx::Row;

use crate::{channels::telegram::api_base, common::AppState, events::Event};

pub async fn handle(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    if let Err(e) = send(&st, &ev).await {
        tracing::warn!(error = %e, "outbound telegram failed");
    }
}

async fn send(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(id) = ev.data["id"].as_i64() else {
        return Ok(());
    };

    let Some(r) = sqlx::query(
        "SELECT m.id::bigint AS id, m.content, m.content_attributes::jsonb AS content_attributes,
                c.additional_attributes, ci.source_id, ct.bot_token
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
         JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::Telegram'
         JOIN channel_telegram ct ON ct.id = i.channel_id AND ct.account_id = i.account_id
         JOIN contact_inboxes ci ON ci.id = c.contact_inbox_id
         WHERE m.id::bigint = $1 AND m.account_id::bigint = $2
           AND m.message_type = 1 AND NOT COALESCE(m.private, false)",
    )
    .bind(id)
    .bind(ev.account_id)
    .fetch_optional(&st.db)
    .await?
    else {
        return Ok(());
    };

    let content: String = r.try_get::<Option<String>, _>("content")?.unwrap_or_default();
    if content.is_empty() {
        return Ok(());
    }

    let bot_token: String = r.try_get("bot_token")?;
    let conv_attrs: Option<Value> = r.try_get("additional_attributes")?;
    let msg_attrs: Option<Value> = r.try_get("content_attributes")?;
    let contact_source_id: String = r.try_get("source_id")?;

    let chat_id: Value = conv_attrs
        .as_ref()
        .and_then(|v| v.get("chat_id"))
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or_else(|| json!(contact_source_id));

    let mut body = json!({
        "chat_id": chat_id,
        "text": content,
    });
    if let Some(reply_to) = msg_attrs
        .as_ref()
        .and_then(|v| v.get("in_reply_to_external_id"))
        .filter(|v| !v.is_null())
    {
        body["reply_to_message_id"] = reply_to.clone();
    }
    if let Some(bc) = conv_attrs
        .as_ref()
        .and_then(|v| v.get("business_connection_id"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        body["business_connection_id"] = json!(bc);
    }

    let url = format!("{}/bot{bot_token}/sendMessage", api_base());
    let resp = reqwest::Client::new()
        .post(&url)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&body)?)
        .send()
        .await?;

    let status = resp.status();
    let bytes = resp.bytes().await?;
    let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

    if !status.is_success() || parsed["ok"].as_bool() == Some(false) {
        let err_msg = format!(
            "{}, {}",
            parsed["error_code"].as_i64().unwrap_or(status.as_u16() as i64),
            parsed["description"].as_str().unwrap_or("telegram error")
        );
        sqlx::query(
            "UPDATE messages SET status = 3, external_error = $1, updated_at = now()
             WHERE id::bigint = $2 AND account_id::bigint = $3",
        )
        .bind(err_msg)
        .bind(id)
        .bind(ev.account_id)
        .execute(&st.db)
        .await?;
        return Ok(());
    }

    if let Some(msg_id) = parsed["result"]["message_id"]
        .as_i64()
        .map(|n| n.to_string())
        .or_else(|| parsed["result"]["message_id"].as_str().map(str::to_string))
    {
        sqlx::query(
            "UPDATE messages SET source_id = $1, updated_at = now()
             WHERE id::bigint = $2 AND account_id::bigint = $3",
        )
        .bind(msg_id)
        .bind(id)
        .bind(ev.account_id)
        .execute(&st.db)
        .await?;
    }

    Ok(())
}
