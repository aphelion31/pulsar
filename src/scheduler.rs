//! Periodic jobs (config/schedule.yml): trigger_scheduled_items_job and
//! trigger_conversations_resolution_job, both every 5 minutes.
//! Mirrors conversations/reopen_snoozed_conversations_job.rb, notification/reopen_snoozed_notifications_job.rb,
//! account/conversations_resolution_scheduler_job.rb and conversations/resolution_job.rb.
use std::time::Duration;

use serde_json::json;

use crate::{common::AppState, conversations};

const BULK_LIMIT: i64 = 100; // Limits::BULK_ACTIONS_LIMIT

pub fn spawn(st: AppState) {
    let secs = std::env::var("PULSAR_SCHEDULER_INTERVAL_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(secs));
        loop {
            tick.tick().await;
            if let Err(e) = run_once(&st).await {
                tracing::error!("scheduler run failed: {e:#}");
            }
        }
    });
}

pub async fn run_once(st: &AppState) -> anyhow::Result<()> {
    reopen_snoozed_conversations(st).await?;
    reopen_snoozed_notifications(st).await?;
    auto_resolve(st).await?;
    Ok(())
}

async fn reopen_snoozed_conversations(st: &AppState) -> anyhow::Result<()> {
    let ids: Vec<i64> = sqlx::query_scalar(
        "UPDATE conversations SET status = 0, snoozed_until = NULL, status_changed_at = now(), updated_at = now()
         WHERE status = 3 AND snoozed_until BETWEEN now() - interval '3 days' AND now() RETURNING id::bigint",
    )
    .fetch_all(&st.db)
    .await?;
    for id in ids {
        conversations::emit_conv(st, id, "conversation.status_changed").await.ok();
    }
    Ok(())
}

async fn reopen_snoozed_notifications(st: &AppState) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE notifications SET meta = COALESCE(meta, '{}'::jsonb) || jsonb_build_object('last_snoozed_at', snoozed_until),
            snoozed_until = NULL, read_at = NULL, last_activity_at = now(), updated_at = now()
         WHERE snoozed_until BETWEEN now() - interval '3 days' AND now()",
    )
    .execute(&st.db)
    .await?;
    Ok(())
}

/// activity_message_handler.rb auto_resolve_message_key
pub fn auto_resolved_text(minutes: i64) -> String {
    let (n, unit) = if minutes >= 1440 && minutes % 1440 == 0 {
        (minutes / 1440, "days")
    } else if minutes >= 60 && minutes % 60 == 0 {
        (minutes / 60, "hours")
    } else {
        (minutes, "minutes")
    };
    format!("Conversation was marked resolved by system due to {n} {unit} of inactivity")
}

async fn auto_resolve(st: &AppState) -> anyhow::Result<()> {
    let accounts: Vec<(i64, i64, Option<String>, bool)> = sqlx::query_as(
        "SELECT id::bigint, (settings->>'auto_resolve_after')::bigint, NULLIF(settings->>'auto_resolve_message', ''),
                COALESCE((settings->>'auto_resolve_ignore_waiting')::boolean, false)
         FROM accounts WHERE (settings->>'auto_resolve_after')::int > 0 AND status = 0",
    )
    .fetch_all(&st.db)
    .await?;
    for (account_id, minutes, message, ignore_waiting) in accounts {
        // resolvable_all / resolvable_not_waiting, minus orphan conversations without a contact
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id::bigint FROM conversations
             WHERE account_id = $1 AND status = 0 AND contact_id IS NOT NULL
               AND last_activity_at < now() - make_interval(mins => $2::int)
               AND (NOT $3 OR waiting_since IS NULL)
             ORDER BY id LIMIT $4",
        )
        .bind(account_id)
        .bind(minutes as i32)
        .bind(ignore_waiting)
        .bind(BULK_LIMIT)
        .fetch_all(&st.db)
        .await?;
        for id in ids {
            let c = conversations::reload(&st.db, id).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
            if let Some(m) = &message {
                // ponytail: messaging-window check (can_reply?) skipped; only matters for WhatsApp/Facebook-style 24h windows.
                conversations::insert_message(st, &c, None, m, 3, false, 0, json!({}), None).await.ok();
            }
            sqlx::query("UPDATE conversations SET status = 1, status_changed_at = now(), updated_at = now() WHERE id = $1 AND status = 0")
                .bind(id)
                .execute(&st.db)
                .await?;
            let content_attributes = json!({ "activity": { "type": "conversation_status_changed", "status": "resolved" } });
            conversations::insert_message(st, &c, None, &auto_resolved_text(minutes), 2, false, 0, content_attributes, None).await.ok();
            conversations::emit_conv(st, id, "conversation.status_changed").await.ok();
        }
        // ponytail: auto_resolve_label not applied; add when the label setting is exposed in account settings UI.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn auto_resolved_text_units() {
        assert_eq!(super::auto_resolved_text(2880), "Conversation was marked resolved by system due to 2 days of inactivity");
        assert_eq!(super::auto_resolved_text(120), "Conversation was marked resolved by system due to 2 hours of inactivity");
        assert_eq!(super::auto_resolved_text(90), "Conversation was marked resolved by system due to 90 minutes of inactivity");
    }
}
