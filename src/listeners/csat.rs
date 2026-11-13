//! CSAT survey listener (app/listeners/csat_survey_listener.rb):
//! - conversation.status_changed to resolved: send the survey message (csat_survey_service.rb +
//!   message_templates/template/csat_survey.rb) when the inbox has CSAT on, none was sent yet, and the
//!   label survey rules allow it.
//! - message.updated on an input_csat message: store the rating (csat_surveys/response_builder.rb).
use serde_json::{json, Value};

use crate::{common::AppState, conversations, events::Event};

const INPUT_CSAT: i32 = 9;
const TEMPLATE: i32 = 3;

pub async fn handle(st: AppState, ev: Event) {
    let r = match ev.name.as_str() {
        "conversation.status_changed" => send_survey(&st, &ev).await,
        "message.updated" => record_response(&st, &ev).await,
        _ => Ok(()),
    };
    if let Err(e) = r {
        tracing::warn!(error = %e, event = %ev.name, "csat listener failed");
    }
}

/// csat_allowed_by_survey_rules?
pub fn rules_allow(config: &Value, labels: &[String]) -> bool {
    let values: Vec<&str> = config["survey_rules"]["values"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if values.is_empty() {
        return true;
    }
    match config["survey_rules"]["operator"].as_str().unwrap_or("contains") {
        "contains" => values.iter().any(|v| labels.iter().any(|l| l == v)),
        "does_not_contain" => values.iter().all(|v| !labels.iter().any(|l| l == v)),
        _ => true,
    }
}

async fn send_survey(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(id) = ev.data["id"].as_i64() else { return Ok(()) };
    let row: Option<(i64, i32, Option<bool>, Option<Value>, bool)> = sqlx::query_as(
        "SELECT c.id::bigint, c.status, i.csat_survey_enabled, i.csat_config,
                EXISTS (SELECT 1 FROM messages m WHERE m.conversation_id = c.id AND m.content_type = $3)
         FROM conversations c JOIN inboxes i ON i.id = c.inbox_id
         WHERE c.account_id = $1 AND c.display_id = $2",
    )
    .bind(ev.account_id)
    .bind(id)
    .bind(INPUT_CSAT)
    .fetch_optional(&st.db)
    .await?;
    let Some((conv_id, status, enabled, config, already)) = row else { return Ok(()) };
    let config = config.unwrap_or(json!({}));
    if status != 1 || !enabled.unwrap_or(false) || already {
        return Ok(());
    }
    let c = conversations::reload(&st.db, conv_id).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    if !rules_allow(&config, &c.labels()) {
        return Ok(());
    }
    // ponytail: WhatsApp/Twilio template surveys and the 24h messaging-window check are skipped; the
    // channels that need them are not ported yet.
    let content = config["message"].as_str().filter(|s| !s.is_empty()).unwrap_or("Please rate the conversation");
    let attrs = json!({ "display_type": config["display_type"].as_str().unwrap_or("emoji") });
    conversations::insert_message(st, &c, None, content, TEMPLATE, false, INPUT_CSAT, attrs, None)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(())
}

async fn record_response(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(id) = ev.data["id"].as_i64() else { return Ok(()) };
    let row: Option<(i64, i64, i32, Option<Value>, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT m.id::bigint, m.conversation_id::bigint, m.content_type, m.content_attributes::jsonb, c.contact_id::bigint, c.assignee_id::bigint
         FROM messages m JOIN conversations c ON c.id = m.conversation_id WHERE m.id = $1 AND m.account_id = $2",
    )
    .bind(id)
    .bind(ev.account_id)
    .fetch_optional(&st.db)
    .await?;
    let Some((mid, conv, ctype, attrs, contact, assignee)) = row else { return Ok(()) };
    if ctype != INPUT_CSAT {
        return Ok(());
    }
    let resp = attrs.as_ref().map(|a| a["submitted_values"]["csat_survey_response"].clone()).unwrap_or(Value::Null);
    let Some(rating) = resp["rating"].as_i64().or_else(|| resp["rating"].as_str().and_then(|s| s.parse().ok())) else { return Ok(()) };
    let feedback = resp["feedback_message"].as_str();
    let updated = sqlx::query("UPDATE csat_survey_responses SET rating = $2, feedback_message = $3, updated_at = now() WHERE message_id = $1")
        .bind(mid)
        .bind(rating as i32)
        .bind(feedback)
        .execute(&st.db)
        .await?
        .rows_affected();
    if updated == 0 {
        sqlx::query(
            "INSERT INTO csat_survey_responses (account_id, conversation_id, message_id, rating, feedback_message, contact_id, assigned_agent_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now()) ON CONFLICT DO NOTHING",
        )
        .bind(ev.account_id)
        .bind(conv)
        .bind(mid)
        .bind(rating as i32)
        .bind(feedback)
        .bind(contact)
        .bind(assignee)
        .execute(&st.db)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::rules_allow;
    use serde_json::json;

    #[test]
    fn survey_rules() {
        let l = vec!["vip".to_string()];
        assert!(rules_allow(&json!({}), &l));
        assert!(rules_allow(&json!({"survey_rules": {"operator": "contains", "values": ["vip"]}}), &l));
        assert!(!rules_allow(&json!({"survey_rules": {"operator": "contains", "values": ["x"]}}), &l));
        assert!(!rules_allow(&json!({"survey_rules": {"operator": "does_not_contain", "values": ["vip"]}}), &l));
    }
}
