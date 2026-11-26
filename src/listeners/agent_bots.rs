//! Agent bot listener: delivers webhook callbacks to `agent_bots.outgoing_url` linked via `agent_bot_inboxes`.
use sqlx::Row;

use crate::{common::AppState, events::Event, listeners::webhooks};

pub async fn handle(st: AppState, ev: Event) {
    if let Err(error) = process(&st, &ev).await {
        tracing::warn!(%error, event = %ev.name, "agent_bots listener failed");
    }
}

async fn process(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    if !matches!(
        ev.name.as_str(),
        "message.created"
            | "message.updated"
            | "conversation.created"
            | "conversation.status_changed"
            | "webwidget.triggered"
    ) {
        return Ok(());
    }
    if ev.name.starts_with("message.") {
        if ev.data["private"].as_bool().unwrap_or(false) {
            return Ok(());
        }
        let mt = ev.data["message_type"].as_i64();
        let stype = ev.data["sender_type"].as_str();
        if mt == Some(2) || (mt == Some(1) && stype == Some("User")) {
            return Ok(());
        }
    }
    let Some((inbox_opt, payload)) = webhooks::build_event_payload(&st.db, ev).await? else {
        return Ok(());
    };
    let Some(inbox_id) = inbox_opt else {
        return Ok(());
    };
    let rows = sqlx::query(
        "SELECT ab.outgoing_url, ab.secret
         FROM agent_bots ab
         JOIN agent_bot_inboxes abi ON abi.agent_bot_id = ab.id
         WHERE abi.inbox_id = $1 AND abi.account_id = $2
           AND COALESCE(abi.status, 0) = 0 AND COALESCE(ab.bot_type, 0) = 0
           AND COALESCE(ab.outgoing_url, '') <> ''",
    )
    .bind(inbox_id as i32)
    .bind(ev.account_id as i32)
    .fetch_all(&st.db)
    .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let body = serde_json::to_string(&payload)?;
    for row in rows {
        let url: String = row.try_get("outgoing_url")?;
        let secret: Option<String> = row.try_get("secret")?;
        let dec = secret.map(|s| crate::ar_encryption::decrypt(&s));
        if let Err(error) = webhooks::deliver(&url, dec.as_deref(), &body).await {
            tracing::warn!(%error, url = %url, "agent_bot webhook delivery failed");
        }
    }
    Ok(())
}
