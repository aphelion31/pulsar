//! Outbound Twilio listener on message.created.
//! Mirrors chatwoot/app/services/twilio/send_on_twilio_service.rb and channel/twilio_sms.rb.

use serde_json::Value;
use sqlx::Row;

use crate::{ar_encryption, channels::twilio::api_base, common::AppState, events::Event};

pub async fn handle(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    if let Err(e) = send(&st, &ev).await {
        tracing::warn!(error = %e, "outbound twilio failed");
    }
}

async fn mark_failed(st: &AppState, msg_id: i64, account_id: i64, err_msg: &str) -> anyhow::Result<()> {
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

    let Some(r) = sqlx::query(
        "SELECT m.id::bigint AS id, m.content, ci.source_id,
                ct.account_sid, ct.auth_token, ct.api_key_sid,
                ct.messaging_service_sid, ct.phone_number, COALESCE(ct.medium, 0)::int AS medium
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
         JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::TwilioSms'
         JOIN channel_twilio_sms ct ON ct.id = i.channel_id AND ct.account_id = i.account_id
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

    let source_id: String = r.try_get("source_id")?;
    let account_sid: String = r.try_get("account_sid")?;
    let stored_token: String = r.try_get("auth_token")?;
    let auth_token = ar_encryption::decrypt(&stored_token);
    let api_key_sid: Option<String> = r.try_get("api_key_sid")?;
    let messaging_service_sid: Option<String> = r.try_get("messaging_service_sid")?;
    let phone_number: Option<String> = r.try_get("phone_number")?;
    let medium: i32 = r.try_get("medium")?;

    let to = if medium == 1 && !source_id.starts_with("whatsapp:") {
        format!("whatsapp:{source_id}")
    } else {
        source_id
    };

    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("To", &to);
        form.append_pair("Body", &content);
        if let Some(msid) = messaging_service_sid.as_deref().filter(|s| !s.is_empty()) {
            form.append_pair("MessagingServiceSid", msid);
        } else if let Some(from) = phone_number.as_deref().filter(|s| !s.is_empty()) {
            if medium == 1 && !from.starts_with("whatsapp:") {
                form.append_pair("From", &format!("whatsapp:{from}"));
            } else {
                form.append_pair("From", from);
            }
        }
        form.finish()
    };

    let auth_user = api_key_sid
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&account_sid);

    let url = format!("{}/2010-04-01/Accounts/{account_sid}/Messages.json", api_base());
    let resp = match reqwest::Client::new()
        .post(&url)
        .basic_auth(auth_user, Some(&auth_token))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            mark_failed(st, id, ev.account_id, &format!("twilio request error: {e}")).await?;
            return Ok(());
        }
    };

    let status = resp.status();
    let bytes = resp.bytes().await?;
    let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

    if status.is_success() {
        if let Some(sid) = parsed["sid"].as_str().filter(|s| !s.is_empty()) {
            sqlx::query(
                "UPDATE messages SET source_id = $1, updated_at = now()
                 WHERE id::bigint = $2 AND account_id::bigint = $3",
            )
            .bind(sid)
            .bind(id)
            .bind(ev.account_id)
            .execute(&st.db)
            .await?;
        }
    } else {
        let err_msg = parsed["message"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("twilio error ({})", status.as_u16()));
        mark_failed(st, id, ev.account_id, &err_msg).await?;
    }

    Ok(())
}
