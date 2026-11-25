//! Outbound Bandwidth SMS listener on message.created.
//! Mirrors chatwoot/app/services/sms/send_on_sms_service.rb and channel/sms.rb#send_message.

use serde_json::Value;
use sqlx::Row;

use crate::{channels::sms::send_text, common::AppState, events::Event};

pub async fn handle(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    if let Err(e) = send(&st, &ev).await {
        tracing::warn!(error = %e, "outbound sms failed");
    }
}

async fn mark_failed(
    st: &AppState,
    msg_id: i64,
    account_id: i64,
    err_msg: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE messages
         SET status = 3,
             content_attributes = COALESCE(content_attributes, '{}'::json)::jsonb || jsonb_build_object('external_error', $1::text),
             updated_at = now()
         WHERE id::bigint = $2 AND account_id::bigint = $3",
    )
    .bind(err_msg)
    .bind(msg_id)
    .bind(account_id)
    .execute(&st.db)
    .await?;
    Ok(())
}

async fn send(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(id) = ev.data["id"].as_i64() else {
        return Ok(());
    };

    // Same filters as Base::SendOnChannelService: outgoing, public, not from a channel echo.
    let Some(r) = sqlx::query(
        "SELECT m.content, ci.source_id, cs.phone_number, cs.provider_config
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
         JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::Sms'
         JOIN channel_sms cs ON cs.id = i.channel_id AND cs.account_id = i.account_id
         JOIN contact_inboxes ci ON ci.id = c.contact_inbox_id
         WHERE m.id::bigint = $1 AND m.account_id::bigint = $2
           AND m.message_type = 1 AND NOT COALESCE(m.private, false)
           AND (m.source_id IS NULL OR m.source_id = '')",
    )
    .bind(id)
    .bind(ev.account_id)
    .fetch_optional(&st.db)
    .await?
    else {
        return Ok(());
    };

    let content: String = r
        .try_get::<Option<String>, _>("content")?
        .unwrap_or_default();
    if content.is_empty() {
        return Ok(());
    }
    let to: String = r.try_get("source_id")?;
    let from: String = r.try_get("phone_number")?;
    let cfg: Value = r.try_get("provider_config")?;
    let Some(cfg) = cfg.as_object().cloned() else {
        mark_failed(st, id, ev.account_id, "Bandwidth configuration is missing").await?;
        return Ok(());
    };

    match send_text(&cfg, &from, &to, &content).await {
        Ok(provider_id) if !provider_id.is_empty() => {
            sqlx::query(
                "UPDATE messages SET source_id = $1, updated_at = now()
                 WHERE id::bigint = $2 AND account_id::bigint = $3",
            )
            .bind(&provider_id)
            .bind(id)
            .bind(ev.account_id)
            .execute(&st.db)
            .await?;
        }
        Ok(_) => {}
        Err(err) => mark_failed(st, id, ev.account_id, &err).await?,
    }
    Ok(())
}
