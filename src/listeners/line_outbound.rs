//! Outbound LINE listener on message.created.
//! Mirrors chatwoot/app/services/line/send_on_line_service.rb and channel/line.rb.

use serde_json::{json, Value};
use sqlx::Row;

use crate::{channels::line::api_base, common::AppState, events::Event};

pub async fn handle(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    if let Err(e) = send(&st, &ev).await {
        tracing::warn!(error = %e, "outbound line failed");
    }
}

async fn send(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(id) = ev.data["id"].as_i64() else {
        return Ok(());
    };

    let Some(r) = sqlx::query(
        "SELECT m.id::bigint AS id, m.content, ci.source_id, cl.line_channel_token
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
         JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::Line'
         JOIN channel_line cl ON cl.id = i.channel_id AND cl.account_id = i.account_id
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

    let user_id: String = r.try_get("source_id")?;
    let channel_token: String = r.try_get("line_channel_token")?;

    let body = json!({
        "to": user_id,
        "messages": [{
            "type": "text",
            "text": content,
        }],
    });

    let url = format!("{}/v2/bot/message/push", api_base());
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Authorization", format!("Bearer {channel_token}"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&body)?)
        .send()
        .await?;

    let status = resp.status();
    let bytes = resp.bytes().await?;
    let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

    if status.is_success() {
        sqlx::query(
            "UPDATE messages SET status = 1, updated_at = now()
             WHERE id::bigint = $1 AND account_id::bigint = $2",
        )
        .bind(id)
        .bind(ev.account_id)
        .execute(&st.db)
        .await?;
    } else {
        let err_msg = parsed["message"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("line error ({})", status.as_u16()));
        sqlx::query(
            "UPDATE messages SET status = 3, external_error = $1, updated_at = now()
             WHERE id::bigint = $2 AND account_id::bigint = $3",
        )
        .bind(err_msg)
        .bind(id)
        .bind(ev.account_id)
        .execute(&st.db)
        .await?;
    }

    Ok(())
}
