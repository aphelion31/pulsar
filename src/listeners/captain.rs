//! Captain AI auto-responder listener: answers incoming inbox messages with linked Captain assistants
//! when conversations are unassigned, and performs `[HANDOFF]` to open queue when needed.
use serde_json::{Value, json};
use sqlx::Row;

use crate::{
    captain::{call_llm_chat, resolve_llm_config},
    common::AppState,
    conversations::{emit_conv_changed, insert_message, reload},
    events::Event,
};

pub async fn handle(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    if let Err(error) = process(&st, &ev).await {
        tracing::warn!(%error, "captain listener failed");
    }
}

async fn process(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(msg_id) = ev.data["id"].as_i64() else {
        return Ok(());
    };
    let row = sqlx::query(
        "SELECT m.content, m.message_type, m.private, m.conversation_id::bigint AS conv_id,
                m.inbox_id::bigint AS inbox_id, c.assignee_id::bigint AS assignee_id, c.status,
                ca.id::bigint AS assistant_id, ca.name AS assistant_name, ca.description, ca.config
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         JOIN captain_inboxes ci ON ci.inbox_id = m.inbox_id
         JOIN captain_assistants ca ON ca.id = ci.captain_assistant_id AND ca.account_id = m.account_id
         WHERE m.id = $1 AND m.account_id = $2
         ORDER BY ca.id LIMIT 1",
    )
    .bind(msg_id)
    .bind(ev.account_id as i32)
    .fetch_optional(&st.db)
    .await?;
    let Some(r) = row else {
        return Ok(());
    };
    let msg_type: i32 = r.try_get("message_type")?;
    let private: bool = r.try_get("private")?;
    let assignee_id: Option<i64> = r.try_get("assignee_id")?;
    let status: i32 = r.try_get("status")?;
    let content: String = r.try_get::<Option<String>, _>("content")?.unwrap_or_default();
    if msg_type != 0
        || private
        || content.trim().is_empty()
        || assignee_id.is_some()
        || !matches!(status, 0 | 2)
    {
        return Ok(());
    }

    let Ok(cfg) = resolve_llm_config(&st.db, None).await else {
        return Ok(());
    };
    let assistant_id: i64 = r.try_get("assistant_id")?;
    let assistant_name: String = r.try_get("assistant_name")?;
    let description: Option<String> = r.try_get("description")?;
    let config: Value = r.try_get::<Option<Value>, _>("config")?.unwrap_or(json!({}));
    let instructions = config["instructions"]
        .as_str()
        .or(description.as_deref())
        .unwrap_or("Helpful customer support assistant.");

    let faqs: Vec<(String, String)> = sqlx::query_as(
        "SELECT question, answer FROM captain_assistant_responses
         WHERE assistant_id = $1 AND account_id = $2 AND status = 1
         ORDER BY id DESC LIMIT 5",
    )
    .bind(assistant_id)
    .bind(ev.account_id)
    .fetch_all(&st.db)
    .await
    .unwrap_or_default();
    let faq_ctx = faqs
        .into_iter()
        .map(|(q, a)| format!("Q: {q}\nA: {a}"))
        .collect::<Vec<_>>()
        .join("\n---\n");

    let system = format!(
        "You are {assistant_name}. {instructions}\nIf the user requests a human agent or you cannot help safely, include [HANDOFF] in your response.\n{}",
        if faq_ctx.is_empty() {
            String::new()
        } else {
            format!("Knowledge Base FAQs:\n{faq_ctx}")
        }
    );
    let (reply, _) = call_llm_chat(
        &cfg,
        vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": content }),
        ],
        config["temperature"].as_f64().unwrap_or(0.3),
    )
    .await
    .map_err(|e| anyhow::anyhow!("captain llm failed: {e:?}"))?;

    let conv_id: i64 = r.try_get("conv_id")?;
    let conv = reload(&st.db, conv_id)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let trimmed = reply.trim();
    if trimmed.contains("[HANDOFF]") {
        let cleaned = trimmed.replace("[HANDOFF]", "").trim().to_string();
        if !cleaned.is_empty() {
            insert_message(
                st,
                &conv,
                None,
                &cleaned,
                1,
                false,
                0,
                json!({ "is_captain": true }),
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        }
        sqlx::query(
            "UPDATE conversations SET status = 0, status_changed_at = now(), updated_at = now() WHERE id = $1",
        )
        .bind(conv_id)
        .execute(&st.db)
        .await?;
        let _ = emit_conv_changed(
            st,
            conv_id,
            "conversation.status_changed",
            Some(json!({ "status": { "previous_value": status, "current_value": 0 } })),
        )
        .await;
    } else if !trimmed.is_empty() {
        insert_message(
            st,
            &conv,
            None,
            trimmed,
            1,
            false,
            0,
            json!({ "is_captain": true }),
            None,
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    Ok(())
}
