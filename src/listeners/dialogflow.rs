//! Dialogflow integration listener: forwards incoming messages on Dialogflow-hooked inboxes and posts bot replies.
use serde_json::{Value, json};
use sqlx::Row;

use crate::{
    common::AppState,
    conversations::{insert_message, reload},
    events::Event,
};

pub async fn handle(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    if let Err(e) = process(&st, &ev).await {
        tracing::warn!(error = %e, "dialogflow listener failed");
    }
}

async fn process(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(msg_id) = ev.data["id"].as_i64() else {
        return Ok(());
    };
    let row = sqlx::query(
        "SELECT m.content, m.conversation_id::bigint AS conv_id, m.message_type,
                h.settings
         FROM messages m
         JOIN integrations_hooks h
           ON h.account_id = m.account_id AND h.inbox_id = m.inbox_id
          AND h.app_id = 'dialogflow' AND h.status = 1
         WHERE m.id = $1 AND m.account_id = $2",
    )
    .bind(msg_id)
    .bind(ev.account_id)
    .fetch_optional(&st.db)
    .await?;

    let Some(r) = row else {
        return Ok(());
    };
    let msg_type: i32 = r.try_get("message_type")?;
    if msg_type != 0 {
        return Ok(());
    }
    let content: Option<String> = r.try_get("content")?;
    let conv_id: i64 = r.try_get("conv_id")?;
    let settings: Value = r
        .try_get::<Option<Value>, _>("settings")?
        .unwrap_or(json!({}));

    let conv = reload(&st.db, conv_id)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    // Do not intercept conversations already assigned to an agent/team and open without bot status
    if conv.assignee_id.is_some() && conv.status == 0 {
        return Ok(());
    }

    let endpoint = std::env::var("DIALOGFLOW_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            settings["webhook_url"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| {
            let pid = settings["project_id"].as_str().unwrap_or("default");
            format!("https://dialogflow.googleapis.com/v2/projects/{pid}/agent/sessions/{}:detectIntent", conv.uuid)
        });

    let payload = serde_json::to_vec(&json!({
        "queryInput": {
            "text": { "text": content.unwrap_or_default(), "languageCode": settings["region"].as_str().unwrap_or("en") }
        },
        "conversation_id": conv.display_id,
    }))?;
    let resp = reqwest::Client::new()
        .post(&endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(());
    }
    let body: Value = serde_json::from_slice(&resp.bytes().await?).unwrap_or(Value::Null);
    let reply = body["queryResult"]["fulfillmentText"]
        .as_str()
        .or_else(|| body["fulfillmentText"].as_str())
        .or_else(|| body["reply"].as_str())
        .map(str::trim)
        .unwrap_or("");
    if !reply.is_empty() {
        insert_message(st, &conv, None, reply, 1, false, 0, json!({}), None)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    if body["handoff"].as_bool().unwrap_or(false)
        || body["queryResult"]["diagnosticInfo"]["handoff"]
            .as_bool()
            .unwrap_or(false)
    {
        sqlx::query("UPDATE conversations SET status = 0, updated_at = now() WHERE id = $1")
            .bind(conv_id)
            .execute(&st.db)
            .await?;
    }
    Ok(())
}
