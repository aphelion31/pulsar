//! Periodic jobs (config/schedule.yml): trigger_scheduled_items_job and
//! trigger_conversations_resolution_job, both every 5 minutes.
//! Mirrors conversations/reopen_snoozed_conversations_job.rb, notification/reopen_snoozed_notifications_job.rb,
//! account/conversations_resolution_scheduler_job.rb and conversations/resolution_job.rb.
use std::time::Duration;

use serde_json::{Value, json};

use crate::{common::AppState, conversations};

const BULK_LIMIT: i64 = 100; // Limits::BULK_ACTIONS_LIMIT

pub fn spawn(st: AppState) {
    let secs = std::env::var("PULSAR_SCHEDULER_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
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
    dispatch_scheduled_campaigns(st).await?;
    crate::sla::evaluate_open_slas(st).await?;
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
        conversations::emit_conv_changed(
            st,
            id,
            "conversation.status_changed",
            Some(json!({ "status": { "previous_value": 3, "current_value": 0 } })),
        )
        .await
        .ok();
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
    let accounts: Vec<(i64, i64, Option<String>, Option<String>, bool)> = sqlx::query_as(
        "SELECT id::bigint, (settings->>'auto_resolve_after')::bigint,
                NULLIF(settings->>'auto_resolve_message', ''),
                NULLIF(settings->>'auto_resolve_label', ''),
                COALESCE((settings->>'auto_resolve_ignore_waiting')::boolean, false)
         FROM accounts WHERE (settings->>'auto_resolve_after')::int > 0 AND status = 0",
    )
    .fetch_all(&st.db)
    .await?;
    for (account_id, minutes, message, label, ignore_waiting) in accounts {
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
            let c = conversations::reload(&st.db, id)
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            if let Some(m) = &message {
                if conversations::within_messaging_window(&st.db, id).await {
                    conversations::insert_message(st, &c, None, m, 3, false, 0, json!({}), None)
                        .await
                        .ok();
                }
            }
            if let Some(lbl) = &label {
                conversations::attach_label_by_name(&st.db, account_id, id, lbl)
                    .await
                    .ok();
            }
            sqlx::query("UPDATE conversations SET status = 1, status_changed_at = now(), updated_at = now() WHERE id = $1 AND status = 0")
                .bind(id)
                .execute(&st.db)
                .await?;
            let content_attributes = json!({ "activity": { "type": "conversation_status_changed", "status": "resolved" } });
            conversations::insert_message(
                st,
                &c,
                None,
                &auto_resolved_text(minutes),
                2,
                false,
                0,
                content_attributes,
                None,
            )
            .await
            .ok();
            conversations::emit_conv_changed(
                st,
                id,
                "conversation.status_changed",
                Some(json!({ "status": { "previous_value": 0, "current_value": 1 } })),
            )
            .await
            .ok();
        }
    }
    Ok(())
}

/// Dispatch due `one_off` campaigns (WhatsApp / SMS / Twilio), recording campaign_recipients.
pub async fn dispatch_scheduled_campaigns(st: &AppState) -> anyhow::Result<()> {
    use sqlx::Row;
    let due = sqlx::query(
        "UPDATE campaigns
         SET campaign_status = 2, started_at = COALESCE(started_at, now()), updated_at = now()
         WHERE id IN (
             SELECT id FROM campaigns
             WHERE campaign_type = 1 AND enabled IS TRUE AND campaign_status = 0 AND scheduled_at <= now()
             ORDER BY id LIMIT 20
         )
         RETURNING id::bigint, account_id::bigint, inbox_id::bigint, sender_id::bigint,
                   message, audience, template_params",
    )
    .fetch_all(&st.db)
    .await?;

    for row in due {
        let campaign_id: i64 = row.try_get("id")?;
        let account_id: i64 = row.try_get("account_id")?;
        let inbox_id: i64 = row.try_get("inbox_id")?;
        let sender_id: Option<i64> = row.try_get("sender_id")?;
        let message: String = row.try_get("message")?;
        let audience: Value = row
            .try_get::<Option<Value>, _>("audience")?
            .unwrap_or(json!([]));
        let template_params: Option<Value> = row.try_get("template_params")?;

        let channel_type: String = sqlx::query_scalar(
            "SELECT channel_type FROM inboxes WHERE id = $1 AND account_id = $2",
        )
        .bind(inbox_id)
        .bind(account_id)
        .fetch_optional(&st.db)
        .await?
        .unwrap_or_default();

        let label_ids: Vec<i64> = audience
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| {
                a["type"]
                    .as_str()
                    .unwrap_or("Label")
                    .eq_ignore_ascii_case("label")
            })
            .filter_map(|a| a["id"].as_i64())
            .collect();

        let contacts: Vec<(i64, Option<String>, Option<String>)> = if label_ids.is_empty() {
            Vec::new()
        } else {
            sqlx::query_as(
                "SELECT DISTINCT c.id::bigint, c.phone_number, c.name
                 FROM contacts c
                 JOIN taggings tg ON tg.taggable_type = 'Contact' AND tg.taggable_id = c.id AND tg.context = 'labels'
                 JOIN tags t ON t.id = tg.tag_id
                 JOIN labels l ON l.account_id = c.account_id AND l.title = t.name
                 WHERE c.account_id = $1 AND l.id = ANY($2)
                 ORDER BY 1",
            )
            .bind(account_id)
            .bind(&label_ids)
            .fetch_all(&st.db)
            .await?
        };

        for (contact_id, phone_opt, _name) in contacts {
            let phone = phone_opt.as_deref().map(str::trim).unwrap_or("");
            if phone.is_empty() {
                sqlx::query(
                    "INSERT INTO campaign_recipients
                     (account_id, campaign_id, contact_id, inbox_id, status, error_code, error_title, error_message, message_content, failed_at, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, 3, 'missing_phone', 'Missing phone number', 'Contact has no phone number', $5, now(), now(), now())",
                )
                .bind(account_id)
                .bind(campaign_id)
                .bind(contact_id)
                .bind(inbox_id)
                .bind(&message)
                .execute(&st.db)
                .await?;
                continue;
            }
            let source_id = if channel_type == "Channel::Whatsapp" {
                phone.trim_start_matches('+').to_string()
            } else {
                phone.to_string()
            };
            let ci_id: Option<i64> = sqlx::query_scalar(
                "SELECT id::bigint FROM contact_inboxes WHERE inbox_id = $1 AND contact_id = $2 LIMIT 1",
            )
            .bind(inbox_id)
            .bind(contact_id)
            .fetch_optional(&st.db)
            .await?;
            let ci_id = match ci_id {
                Some(id) => id,
                None => sqlx::query_scalar(
                    "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at)
                     VALUES ($1, $2, $3, now(), now()) RETURNING id::bigint",
                )
                .bind(contact_id)
                .bind(inbox_id)
                .bind(&source_id)
                .fetch_one(&st.db)
                .await?,
            };
            let conv = crate::channels::facebook::find_or_create_social_conversation(
                &st.db, account_id, inbox_id, contact_id, ci_id, false,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;

            let mut content_attrs = json!({});
            if let Some(tp) = template_params.as_ref().filter(|v| v.is_object()) {
                content_attrs[conversations::EXTRA_ADDITIONAL_ATTRS_KEY] =
                    json!({ "template_params": tp });
            }
            let sender = sender_id.map(|sid| ("User", sid));
            let sent = conversations::insert_message(
                st,
                &conv,
                sender,
                &message,
                1,
                false,
                0,
                content_attrs,
                None,
            )
            .await
            .is_ok();
            let status_code = if sent { 0 } else { 3 };
            sqlx::query(
                "INSERT INTO campaign_recipients
                 (account_id, campaign_id, contact_id, inbox_id, source_id, contact_inbox_id, status, message_content, sent_at, send_attempted_at, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CASE WHEN $7 = 0 THEN now() ELSE NULL END, now(), now(), now())",
            )
            .bind(account_id)
            .bind(campaign_id)
            .bind(contact_id)
            .bind(inbox_id)
            .bind(&source_id)
            .bind(ci_id)
            .bind(status_code)
            .bind(&message)
            .execute(&st.db)
            .await?;
        }

        sqlx::query(
            "UPDATE campaigns SET campaign_status = 1, completed_at = now(), updated_at = now() WHERE id = $1",
        )
        .bind(campaign_id)
        .execute(&st.db)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn auto_resolved_text_units() {
        assert_eq!(
            super::auto_resolved_text(2880),
            "Conversation was marked resolved by system due to 2 days of inactivity"
        );
        assert_eq!(
            super::auto_resolved_text(120),
            "Conversation was marked resolved by system due to 2 hours of inactivity"
        );
        assert_eq!(
            super::auto_resolved_text(90),
            "Conversation was marked resolved by system due to 90 minutes of inactivity"
        );
    }
}
