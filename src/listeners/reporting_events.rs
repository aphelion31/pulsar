//! Reporting event listener: populates `reporting_events` (`first_response`, `reply_time`,
//! `conversation_resolved`, `conversation_bot_handoff`, `conversation_bot_resolved`) so v2 reports stay accurate.
use chrono::NaiveDateTime;
use sqlx::{PgPool, Row};

use crate::{common::AppState, events::Event};

const INCOMING: i32 = 0;
const OUTGOING: i32 = 1;
const OPEN: i64 = 0;
const RESOLVED: i64 = 1;
const PENDING: i64 = 2;

pub async fn handle(st: AppState, ev: Event) {
    let res = match ev.name.as_str() {
        "message.created" => on_message_created(&st.db, &ev).await,
        "conversation.status_changed" => on_status_changed(&st.db, &ev).await,
        _ => return,
    };
    if let Err(error) = res {
        tracing::warn!(%error, event = %ev.name, "reporting_events listener failed");
    }
}

pub(crate) fn clamp_seconds(start: NaiveDateTime, end: NaiveDateTime) -> f64 {
    let ms = (end - start).num_milliseconds().max(0);
    ms as f64 / 1000.0
}

#[derive(Debug, PartialEq)]
pub(crate) struct StatusMetrics {
    pub resolved: bool,
    pub bot_resolved: bool,
    pub bot_handoff: bool,
}

pub(crate) fn classify_status_transition(
    prev_status: Option<i64>,
    curr_status: i64,
    assignee_id: Option<i64>,
) -> StatusMetrics {
    let resolved = curr_status == RESOLVED && prev_status != Some(RESOLVED);
    let bot_resolved = resolved && prev_status == Some(PENDING) && assignee_id.is_none();
    let bot_handoff = prev_status == Some(PENDING) && curr_status == OPEN;
    StatusMetrics {
        resolved,
        bot_resolved,
        bot_handoff,
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_event(
    db: &PgPool,
    name: &str,
    value: f64,
    account_id: i64,
    inbox_id: i64,
    user_id: Option<i64>,
    conversation_id: i64,
    start_time: NaiveDateTime,
    end_time: NaiveDateTime,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO reporting_events
         (name, value, value_in_business_hours, account_id, inbox_id, user_id, conversation_id,
          event_start_time, event_end_time, created_at, updated_at)
         VALUES ($1, $2, $2, $3, $4, $5, $6, $7, $8, $8, $8)",
    )
    .bind(name)
    .bind(value)
    .bind(account_id as i32)
    .bind(inbox_id as i32)
    .bind(user_id.map(|u| u as i32))
    .bind(conversation_id as i32)
    .bind(start_time)
    .bind(end_time)
    .execute(db)
    .await?;
    Ok(())
}

async fn on_message_created(db: &PgPool, ev: &Event) -> anyhow::Result<()> {
    let Some(msg_id) = ev.data["id"].as_i64() else {
        return Ok(());
    };
    let row = sqlx::query(
        "SELECT m.message_type, m.private, m.sender_type, m.sender_id::bigint AS sender_id,
                m.created_at AS msg_at, m.conversation_id::bigint AS conv_id,
                m.inbox_id::bigint AS inbox_id, c.created_at AS conv_at
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE m.id = $1 AND m.account_id = $2",
    )
    .bind(msg_id)
    .bind(ev.account_id as i32)
    .fetch_optional(db)
    .await?;
    let Some(r) = row else {
        return Ok(());
    };
    let msg_type: i32 = r.try_get("message_type")?;
    let private: bool = r.try_get("private")?;
    if msg_type != OUTGOING || private {
        return Ok(());
    }
    let user_id: Option<i64> = match r.try_get::<Option<String>, _>("sender_type")?.as_deref() {
        Some("User") => r.try_get("sender_id")?,
        _ => None,
    };
    let msg_at: NaiveDateTime = r.try_get("msg_at")?;
    let conv_at: NaiveDateTime = r.try_get("conv_at")?;
    let conv_id: i64 = r.try_get("conv_id")?;
    let inbox_id: i64 = r.try_get("inbox_id")?;

    let has_first: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM reporting_events WHERE conversation_id = $1 AND name = 'first_response')",
    )
    .bind(conv_id as i32)
    .fetch_one(db)
    .await?;
    if !has_first {
        let secs = clamp_seconds(conv_at, msg_at);
        insert_event(
            db,
            "first_response",
            secs,
            ev.account_id,
            inbox_id,
            user_id,
            conv_id,
            conv_at,
            msg_at,
        )
        .await?;
    }

    // Wait window since oldest unanswered incoming message after any earlier public outgoing message.
    let wait_start: Option<NaiveDateTime> = sqlx::query_scalar(
        "SELECT MIN(created_at) FROM messages
         WHERE conversation_id = $1 AND message_type = $2 AND id < $3
           AND id > COALESCE((
             SELECT MAX(id) FROM messages
             WHERE conversation_id = $1 AND message_type = $4 AND NOT private AND id < $3
           ), 0)",
    )
    .bind(conv_id as i32)
    .bind(INCOMING)
    .bind(msg_id)
    .bind(OUTGOING)
    .fetch_optional(db)
    .await?
    .flatten();
    if let Some(start) = wait_start {
        let secs = clamp_seconds(start, msg_at);
        insert_event(
            db,
            "reply_time",
            secs,
            ev.account_id,
            inbox_id,
            user_id,
            conv_id,
            start,
            msg_at,
        )
        .await?;
    }
    Ok(())
}

async fn on_status_changed(db: &PgPool, ev: &Event) -> anyhow::Result<()> {
    let Some(display_id) = ev.data["id"].as_i64() else {
        return Ok(());
    };
    let row = sqlx::query(
        "SELECT id::bigint AS conv_id, inbox_id::bigint AS inbox_id,
                assignee_id::bigint AS assignee_id, status,
                created_at AS conv_at, COALESCE(status_changed_at, updated_at) AS changed_at
         FROM conversations
         WHERE account_id = $1 AND display_id = $2",
    )
    .bind(ev.account_id as i32)
    .bind(display_id)
    .fetch_optional(db)
    .await?;
    let Some(r) = row else {
        return Ok(());
    };
    let conv_id: i64 = r.try_get("conv_id")?;
    let inbox_id: i64 = r.try_get("inbox_id")?;
    let assignee_id: Option<i64> = r.try_get("assignee_id")?;
    let db_status: i32 = r.try_get("status")?;
    let conv_at: NaiveDateTime = r.try_get("conv_at")?;
    let changed_at: NaiveDateTime = r.try_get("changed_at")?;

    let prev_status = ev
        .changed
        .as_ref()
        .and_then(|c| c["status"]["previous_value"].as_i64());
    let curr_status = ev
        .changed
        .as_ref()
        .and_then(|c| c["status"]["current_value"].as_i64())
        .unwrap_or(db_status as i64);

    let m = classify_status_transition(prev_status, curr_status, assignee_id);
    if m.resolved {
        let secs = clamp_seconds(conv_at, changed_at);
        insert_event(
            db,
            "conversation_resolved",
            secs,
            ev.account_id,
            inbox_id,
            assignee_id,
            conv_id,
            conv_at,
            changed_at,
        )
        .await?;
    }
    if m.bot_resolved {
        insert_event(
            db,
            "conversation_bot_resolved",
            0.0,
            ev.account_id,
            inbox_id,
            None,
            conv_id,
            conv_at,
            changed_at,
        )
        .await?;
    }
    if m.bot_handoff {
        insert_event(
            db,
            "conversation_bot_handoff",
            0.0,
            ev.account_id,
            inbox_id,
            None,
            conv_id,
            conv_at,
            changed_at,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_and_clamps_deltas() {
        let t0 = chrono::DateTime::from_timestamp(1_000, 0)
            .unwrap()
            .naive_utc();
        let t1 = chrono::DateTime::from_timestamp(1_042, 500_000_000)
            .unwrap()
            .naive_utc();
        assert_eq!(clamp_seconds(t0, t1), 42.5);
        assert_eq!(clamp_seconds(t1, t0), 0.0);

        assert_eq!(
            classify_status_transition(Some(PENDING), RESOLVED, None),
            StatusMetrics {
                resolved: true,
                bot_resolved: true,
                bot_handoff: false,
            }
        );
        assert_eq!(
            classify_status_transition(Some(PENDING), OPEN, Some(5)),
            StatusMetrics {
                resolved: false,
                bot_resolved: false,
                bot_handoff: true,
            }
        );
        assert_eq!(
            classify_status_transition(Some(OPEN), RESOLVED, Some(5)),
            StatusMetrics {
                resolved: true,
                bot_resolved: false,
                bot_handoff: false,
            }
        );
    }
}
