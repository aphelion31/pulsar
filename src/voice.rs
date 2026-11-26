//! Voice & Calls (Twilio Voice + WhatsApp Calling + Inbox Voice Settings).
//! Mirrors chatwoot/enterprise/app/controllers/{api/v1/accounts/{calls,conference,whatsapp_calls,inboxes},twilio/voice}_controller.rb.
use std::collections::HashMap;

use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, patch, post, put},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2_compat::Sha256;
use sqlx::{PgPool, Row};

use crate::common::*;

const PAGE_SIZE: i64 = 25;

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/calls", get(calls_index))
        .route("/calls/{id}/transcribe", post(transcribe_call))
        .route("/contacts/{contact_id}/call", post(contact_call))
        .route("/whatsapp_calls/initiate", post(wa_call_initiate))
        .route("/whatsapp_calls/{id}", get(wa_call_show))
        .route("/whatsapp_calls/{id}/accept", post(wa_call_accept))
        .route("/whatsapp_calls/{id}/reject", post(wa_call_reject))
        .route("/whatsapp_calls/{id}/terminate", post(wa_call_terminate))
        .route(
            "/whatsapp_calls/{id}/upload_recording",
            post(wa_call_upload_recording),
        )
        .route(
            "/inboxes/{inbox_id}/conference",
            post(conference_create).delete(conference_destroy),
        )
        .route(
            "/inboxes/{inbox_id}/conference/token",
            get(conference_token),
        )
        .route(
            "/inboxes/{inbox_id}/conference/dismiss",
            post(conference_dismiss),
        )
        .route(
            "/inboxes/{inbox_id}/conference/hold",
            patch(conference_hold),
        )
        .route(
            "/inboxes/{id}/enable_whatsapp_calling",
            post(enable_whatsapp_calling),
        )
        .route(
            "/inboxes/{id}/disable_whatsapp_calling",
            post(disable_whatsapp_calling),
        )
        .route("/inboxes/{id}/set_inbound_calls", post(set_inbound_calls))
        .route("/inboxes/{id}/set_call_recording", post(set_call_recording))
        .route(
            "/inboxes/{id}/set_whatsapp_callback_permission",
            post(set_whatsapp_callback_permission),
        )
        .route(
            "/inboxes/{inbox_id}/whatsapp_call_hours",
            get(whatsapp_call_hours_show)
                .put(whatsapp_call_hours_update)
                .patch(whatsapp_call_hours_update),
        )
        .route(
            "/inboxes/{inbox_id}/call_announcements/{kind}",
            put(update_call_announcement).patch(update_call_announcement),
        )
        .route(
            "/inboxes/{inbox_id}/call_routing",
            put(update_call_routing).patch(update_call_routing),
        )
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/twilio/voice/call/{phone}", post(twilio_call_twiml))
        .route("/twilio/voice/wait/{phone}", get(twilio_wait_twiml))
        .route("/twilio/voice/resume/{phone}", get(twilio_resume_twiml))
        .route("/twilio/voice/status/{phone}", post(twilio_status))
        .route(
            "/twilio/voice/conference_status/{phone}",
            post(twilio_conference_status),
        )
        .route(
            "/twilio/voice/recording_status/{phone}",
            post(twilio_recording_status),
        )
}

fn sopt(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn i64_param(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| {
        x.as_i64()
            .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
    })
}

async fn call_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT ca.id, ca.account_id, ca.inbox_id, ca.conversation_id, ca.contact_id, ca.message_id,
                ca.accepted_by_agent_id, ca.provider_call_id, ca.provider, ca.direction, ca.status,
                ca.duration_seconds, ca.end_reason, ca.meta, ca.transcript, ca.ring_state,
                EXTRACT(EPOCH FROM ca.started_at)::bigint AS started_at,
                EXTRACT(EPOCH FROM ca.created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM ca.updated_at)::bigint AS updated_at,
                ct.name AS contact_name, ct.phone_number AS contact_phone,
                i.name AS inbox_name, i.channel_type AS inbox_channel_type,
                c.display_id::bigint AS conversation_display_id,
                u.name AS agent_name
         FROM calls ca
         JOIN contacts ct ON ct.id = ca.contact_id
         JOIN inboxes i ON i.id = ca.inbox_id
         JOIN conversations c ON c.id = ca.conversation_id
         LEFT JOIN users u ON u.id = ca.accepted_by_agent_id
         WHERE ca.id = $1 AND ca.account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let provider = if r.try_get::<i32, _>("provider")? == 1 {
        "whatsapp"
    } else {
        "twilio"
    };
    let direction = if r.try_get::<i32, _>("direction")? == 1 {
        "outbound"
    } else {
        "inbound"
    };

    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "account_id": r.try_get::<i64, _>("account_id")?,
        "inbox_id": r.try_get::<i64, _>("inbox_id")?,
        "conversation_id": r.try_get::<i64, _>("conversation_id")?,
        "conversation_display_id": r.try_get::<i64, _>("conversation_display_id")?,
        "contact_id": r.try_get::<i64, _>("contact_id")?,
        "message_id": r.try_get::<Option<i64>, _>("message_id")?,
        "accepted_by_agent_id": r.try_get::<Option<i64>, _>("accepted_by_agent_id")?,
        "accepted_by": r.try_get::<Option<String>, _>("agent_name")?.map(|n| json!({
            "id": r.try_get::<Option<i64>, _>("accepted_by_agent_id").ok().flatten(),
            "name": n,
        })),
        "provider_call_id": r.try_get::<String, _>("provider_call_id")?,
        "provider": provider,
        "direction": direction,
        "status": r.try_get::<String, _>("status")?,
        "started_at": r.try_get::<Option<i64>, _>("started_at")?,
        "duration_seconds": r.try_get::<Option<i32>, _>("duration_seconds")?,
        "end_reason": r.try_get::<Option<String>, _>("end_reason")?,
        "meta": r.try_get::<Option<Value>, _>("meta")?.unwrap_or_else(|| json!({})),
        "transcript": r.try_get::<Option<String>, _>("transcript")?,
        "ring_state": r.try_get::<Value, _>("ring_state")?,
        "contact": {
            "id": r.try_get::<i64, _>("contact_id")?,
            "name": r.try_get::<String, _>("contact_name")?,
            "phone_number": r.try_get::<Option<String>, _>("contact_phone")?,
        },
        "inbox": {
            "id": r.try_get::<i64, _>("inbox_id")?,
            "name": r.try_get::<String, _>("inbox_name")?,
            "channel_type": r.try_get::<String, _>("inbox_channel_type")?,
        },
        "created_at": r.try_get::<i64, _>("created_at")?,
        "updated_at": r.try_get::<i64, _>("updated_at")?,
    }))
}

#[derive(Deserialize, Default)]
struct CallsQuery {
    page: Option<i64>,
    status: Option<String>,
    direction: Option<String>,
    inbox_id: Option<i64>,
}

async fn calls_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<CallsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;
    let dir_filter: Option<i32> = match q.direction.as_deref() {
        Some("inbound") | Some("0") => Some(0),
        Some("outbound") | Some("1") => Some(1),
        _ => None,
    };

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM calls
         WHERE account_id = $1
           AND ($2::text IS NULL OR status = $2)
           AND ($3::int IS NULL OR direction = $3)
           AND ($4::bigint IS NULL OR inbox_id = $4)",
    )
    .bind(account_id)
    .bind(q.status.as_deref().filter(|s| !s.is_empty()))
    .bind(dir_filter)
    .bind(q.inbox_id)
    .fetch_one(&st.db)
    .await?;

    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM calls
         WHERE account_id = $1
           AND ($2::text IS NULL OR status = $2)
           AND ($3::int IS NULL OR direction = $3)
           AND ($4::bigint IS NULL OR inbox_id = $4)
         ORDER BY created_at DESC, id DESC LIMIT $5 OFFSET $6",
    )
    .bind(account_id)
    .bind(q.status.as_deref().filter(|s| !s.is_empty()))
    .bind(dir_filter)
    .bind(q.inbox_id)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::with_capacity(ids.len());
    for id in ids {
        payload.push(call_json(&st.db, account_id, id).await?);
    }
    let total_pages = ((count + PAGE_SIZE - 1) / PAGE_SIZE).max(1);
    Ok(Json(json!({
        "payload": payload,
        "meta": {
            "count": count,
            "current_page": page,
            "total_pages": total_pages,
        }
    })))
}

async fn ensure_call_conversation(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    contact_id: i64,
    source_id: &str,
) -> Result<(i64, i64, i64), ApiError> {
    let ci_id: i64 = if let Some(id) = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM contact_inboxes WHERE contact_id = $1 AND inbox_id = $2 LIMIT 1",
    )
    .bind(contact_id)
    .bind(inbox_id)
    .fetch_optional(db)
    .await?
    {
        id
    } else {
        sqlx::query_scalar(
            "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at)
             VALUES ($1, $2, $3, now(), now()) RETURNING id",
        )
        .bind(contact_id)
        .bind(inbox_id)
        .bind(source_id)
        .fetch_one(db)
        .await?
    };

    if let Some((cid, did)) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT id::bigint, display_id::bigint FROM conversations
         WHERE account_id = $1 AND inbox_id = $2 AND contact_id = $3 AND status <> 1
         ORDER BY id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(inbox_id)
    .bind(contact_id)
    .fetch_optional(db)
    .await?
    {
        return Ok((ci_id, cid, did));
    }

    let next_did: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(display_id), 0)::bigint + 1 FROM conversations WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_one(db)
    .await?;

    let (cid, did): (i64, i64) = sqlx::query_as(
        "INSERT INTO conversations
            (account_id, inbox_id, contact_id, contact_inbox_id, status, display_id, uuid, last_activity_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 0, $5, gen_random_uuid(), now(), now(), now())
         RETURNING id::bigint, display_id::bigint",
    )
    .bind(account_id)
    .bind(inbox_id)
    .bind(contact_id)
    .bind(ci_id)
    .bind(next_did)
    .fetch_one(db)
    .await?;

    Ok((ci_id, cid, did))
}

async fn contact_call(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, contact_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let inbox_id =
        i64_param(&body, "inbox_id").ok_or_else(|| ApiError::bad("inbox_id is required"))?;

    let contact_phone: Option<String> =
        sqlx::query_scalar("SELECT phone_number FROM contacts WHERE id = $1 AND account_id = $2")
            .bind(contact_id)
            .bind(account_id)
            .fetch_optional(&st.db)
            .await?
            .ok_or_else(ApiError::not_found)?;
    let to_number = sopt(&body, "to_number")
        .or(contact_phone)
        .ok_or_else(|| ApiError::bad("Contact phone number is missing"))?;

    let row = sqlx::query(
        "SELECT t.account_sid, t.auth_token, t.phone_number, t.voice_enabled
         FROM inboxes i
         JOIN channel_twilio_sms t ON i.channel_type = 'Channel::TwilioSms' AND i.channel_id = t.id
         WHERE i.id = $1 AND i.account_id = $2",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(|| ApiError::bad("Voice inbox not found"))?;
    let voice_enabled: bool = row.try_get("voice_enabled")?;
    if !voice_enabled {
        return Err(ApiError::bad("Inbox does not have voice enabled"));
    }

    let account_sid: String = row.try_get("account_sid")?;
    let auth_token = crate::ar_encryption::decrypt(&row.try_get::<String, _>("auth_token")?);
    let from_number: String = row
        .try_get::<Option<String>, _>("phone_number")?
        .unwrap_or_default();

    let mut provider_call_id = format!("CA{}", uuid::Uuid::new_v4().simple());
    if std::env::var("TWILIO_API_URL").is_ok_and(|s| !s.trim().is_empty()) || !auth_token.is_empty()
    {
        let api_base = crate::channels::twilio::api_base();
        let url = format!("{api_base}/2010-04-01/Accounts/{account_sid}/Calls.json");
        let status_cb = format!(
            "{}/twilio/voice/status/{}",
            st.frontend_url.trim_end_matches('/'),
            urlencoding_simple(&from_number)
        );
        let form_body = {
            let mut ser = url::form_urlencoded::Serializer::new(String::new());
            ser.append_pair("To", &to_number);
            ser.append_pair("From", &from_number);
            ser.append_pair("StatusCallback", &status_cb);
            ser.finish()
        };
        if let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            if let Ok(resp) = client
                .post(&url)
                .basic_auth(&account_sid, Some(&auth_token))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(form_body)
                .send()
                .await
            {
                let status = resp.status();
                let raw = resp.bytes().await.unwrap_or_default();
                let parsed: Value = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
                if !status.is_success() {
                    let msg = parsed["message"].as_str().unwrap_or("Twilio call failed");
                    return Err(ApiError::bad(msg));
                }
                if let Some(sid) = parsed["sid"].as_str() {
                    provider_call_id = sid.to_string();
                }
            }
        }
    }

    let (_, conv_id, display_id) =
        ensure_call_conversation(&st.db, account_id, inbox_id, contact_id, &to_number).await?;

    let call_id: i64 = sqlx::query_scalar(
        "INSERT INTO calls
            (account_id, inbox_id, conversation_id, contact_id, accepted_by_agent_id, provider_call_id, provider, direction, status, started_at, meta, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, 0, 1, 'ringing', now(), $7, now(), now())
         RETURNING id",
    )
    .bind(account_id)
    .bind(inbox_id)
    .bind(conv_id)
    .bind(contact_id)
    .bind(user.id)
    .bind(&provider_call_id)
    .bind(json!({ "from": from_number, "to": to_number }))
    .fetch_one(&st.db)
    .await?;

    let call_val = call_json(&st.db, account_id, call_id).await?;
    st.hub
        .to_account(account_id, "voice_call.created", call_val.clone());

    Ok(Json(json!({
        "call": call_val,
        "conversation_id": display_id,
    })))
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ============================================================================
// WhatsApp Calling
// ============================================================================

async fn wa_call_initiate(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let sdp = sopt(&body, "sdp")
        .or_else(|| sopt(&body, "sdp_offer"))
        .ok_or_else(|| ApiError::bad("sdp is required"))?;

    let (inbox_id, contact_id, conv_id, _display_id) =
        if let Some(conv_did) = i64_param(&body, "conversation_id") {
            let r = sqlx::query(
                "SELECT id::bigint AS conv_id, inbox_id::bigint AS inbox_id, contact_id
                 FROM conversations WHERE account_id = $1 AND display_id = $2",
            )
            .bind(account_id)
            .bind(conv_did)
            .fetch_optional(&st.db)
            .await?
            .ok_or_else(ApiError::not_found)?;
            (
                r.try_get::<i64, _>("inbox_id")?,
                r.try_get::<i64, _>("contact_id")?,
                r.try_get::<i64, _>("conv_id")?,
                conv_did,
            )
        } else {
            let iid = i64_param(&body, "inbox_id")
                .ok_or_else(|| ApiError::bad("inbox_id is required"))?;
            let cid = i64_param(&body, "contact_id")
                .ok_or_else(|| ApiError::bad("contact_id is required"))?;
            let src = format!("wa_call_{cid}");
            let (_, conv_id, display_id) =
                ensure_call_conversation(&st.db, account_id, iid, cid, &src).await?;
            (iid, cid, conv_id, display_id)
        };

    let ch_row = sqlx::query(
        "SELECT w.phone_number, w.provider_config
         FROM inboxes i
         JOIN channel_whatsapp w ON i.channel_type = 'Channel::Whatsapp' AND i.channel_id = w.id
         WHERE i.id = $1 AND i.account_id = $2",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(|| ApiError::bad("WhatsApp inbox not found"))?;
    let cfg: Value = ch_row.try_get("provider_config")?;
    if !cfg["voice_enabled"].as_bool().unwrap_or(false) {
        return Err(ApiError::bad(
            "WhatsApp calling is not enabled on this inbox",
        ));
    }

    let contact_phone: String = sqlx::query_scalar(
        "SELECT COALESCE(phone_number, '') FROM contacts WHERE id = $1 AND account_id = $2",
    )
    .bind(contact_id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    let mut provider_call_id = format!("wacid.{}", uuid::Uuid::new_v4().simple());
    if let (Some(phone_id), Some(api_key)) =
        (cfg["phone_number_id"].as_str(), cfg["api_key"].as_str())
    {
        let base = std::env::var("WHATSAPP_CLOUD_BASE_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "https://graph.facebook.com".into());
        let url = format!("{}/{}/calls", base.trim_end_matches('/'), phone_id);
        let bytes = serde_json::to_vec(&json!({
            "messaging_product": "whatsapp",
            "to": contact_phone.trim_start_matches('+'),
            "action": "connect",
            "session": { "sdp_type": "offer", "sdp": sdp },
        }))
        .unwrap_or_default();
        if let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            && let Ok(resp) = client
                .post(&url)
                .header("authorization", format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(bytes)
                .send()
                .await
            && resp.status().is_success()
            && let Ok(raw) = resp.bytes().await
            && let Ok(parsed) = serde_json::from_slice::<Value>(&raw)
            && let Some(cid) = parsed["calls"][0]["id"].as_str()
        {
            provider_call_id = cid.to_string();
        }
    }

    let call_id: i64 = sqlx::query_scalar(
        "INSERT INTO calls
            (account_id, inbox_id, conversation_id, contact_id, accepted_by_agent_id, provider_call_id, provider, direction, status, started_at, meta, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, 1, 1, 'ringing', now(), $7, now(), now())
         RETURNING id",
    )
    .bind(account_id)
    .bind(inbox_id)
    .bind(conv_id)
    .bind(contact_id)
    .bind(user.id)
    .bind(&provider_call_id)
    .bind(json!({ "sdp_offer": sdp }))
    .fetch_one(&st.db)
    .await?;

    let val = call_json(&st.db, account_id, call_id).await?;
    st.hub
        .to_account(account_id, "voice_call.created", val.clone());
    Ok(Json(val))
}

async fn wa_call_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(call_json(&st.db, account_id, id).await?))
}

async fn wa_call_transition(
    st: &AppState,
    user: &CurrentUser,
    account_id: i64,
    id: i64,
    new_status: &str,
    extra_meta: Option<(&str, Value)>,
) -> ApiResult {
    require_account(&st.db, user, account_id).await?;
    let cur = call_json(&st.db, account_id, id).await?;
    if matches!(
        cur["status"].as_str(),
        Some("completed" | "ended" | "rejected" | "failed")
    ) {
        return Err(ApiError::bad("Call already ended"));
    }
    let mut meta = cur["meta"].clone();
    if let (Some((k, v)), Some(obj)) = (extra_meta, meta.as_object_mut()) {
        obj.insert(k.to_string(), v);
    }

    sqlx::query(
        "UPDATE calls
         SET status = $1,
             accepted_by_agent_id = CASE WHEN $1 = 'in-progress' THEN $2 ELSE accepted_by_agent_id END,
             meta = $3,
             updated_at = now()
         WHERE id = $4 AND account_id = $5 AND provider = 1",
    )
    .bind(new_status)
    .bind(user.id)
    .bind(meta)
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    let val = call_json(&st.db, account_id, id).await?;
    st.hub
        .to_account(account_id, "voice_call.updated", val.clone());
    Ok(Json(val))
}

async fn wa_call_accept(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    body: Option<Json<Value>>,
) -> ApiResult {
    let answer = body.and_then(|b| sopt(&b.0, "sdp_answer"));
    wa_call_transition(
        &st,
        &user,
        account_id,
        id,
        "in-progress",
        answer.map(|s| ("sdp_answer", json!(s))),
    )
    .await
}

async fn wa_call_reject(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    wa_call_transition(&st, &user, account_id, id, "rejected", None).await
}

async fn wa_call_terminate(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    wa_call_transition(&st, &user, account_id, id, "completed", None).await
}

async fn wa_call_upload_recording(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    body: Option<Json<Value>>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let url = body
        .and_then(|b| sopt(&b.0, "recording_url"))
        .unwrap_or_else(|| format!("/api/v1/accounts/{account_id}/whatsapp_calls/{id}"));
    sqlx::query(
        "UPDATE calls SET meta = COALESCE(meta, '{}'::jsonb) || $1, updated_at = now()
         WHERE id = $2 AND account_id = $3 AND provider = 1",
    )
    .bind(json!({ "recording_url": url, "recording_uploaded": true }))
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;
    Ok(Json(
        json!({ "status": "uploaded", "call": call_json(&st.db, account_id, id).await? }),
    ))
}

// ============================================================================
// Twilio Conference & Token
// ============================================================================

async fn conference_token(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let row = sqlx::query(
        "SELECT t.account_sid, t.api_key_sid, t.api_key_secret, t.twiml_app_sid, t.voice_enabled
         FROM inboxes i
         JOIN channel_twilio_sms t ON i.channel_type = 'Channel::TwilioSms' AND i.channel_id = t.id
         WHERE i.id = $1 AND i.account_id = $2",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    if !row.try_get::<bool, _>("voice_enabled")? {
        return Err(ApiError::bad("Inbox does not support conference calling"));
    }

    let account_sid: String = row.try_get("account_sid")?;
    let api_key_sid = row
        .try_get::<Option<String>, _>("api_key_sid")?
        .unwrap_or_else(|| account_sid.clone());
    let raw_secret = row
        .try_get::<Option<String>, _>("api_key_secret")?
        .map(|s| crate::ar_encryption::decrypt(&s))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "pulsar-voice-secret".into());
    let twiml_app_sid = row
        .try_get::<Option<String>, _>("twiml_app_sid")?
        .unwrap_or_default();

    let now_ts = chrono::Utc::now().timestamp();
    let identity = format!("agent_{account_id}_{}", user.id);
    let header_json = URL_SAFE_NO_PAD
        .encode(json!({ "typ": "JWT", "alg": "HS256", "cty": "twilio-fpa;v=1" }).to_string());
    let payload_json = URL_SAFE_NO_PAD.encode(
        json!({
            "jti": format!("{api_key_sid}-{now_ts}"),
            "iss": api_key_sid,
            "sub": account_sid,
            "iat": now_ts,
            "exp": now_ts + 3600,
            "grants": {
                "identity": identity,
                "voice": {
                    "incoming": { "allow": true },
                    "outgoing": { "application_sid": twiml_app_sid }
                }
            }
        })
        .to_string(),
    );
    let signing_input = format!("{header_json}.{payload_json}");
    let mut mac = Hmac::<Sha256>::new_from_slice(raw_secret.as_bytes())
        .map_err(|_| ApiError::bad("Token key error"))?;
    mac.update(signing_input.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    Ok(Json(json!({
        "token": format!("{signing_input}.{sig}"),
        "identity": identity,
        "expires_at": now_ts + 3600,
    })))
}

async fn conference_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let conv_did = i64_param(&body, "conversation_id")
        .ok_or_else(|| ApiError::bad("conversation_id is required"))?;
    let call_row = sqlx::query(
        "SELECT ca.id, ca.provider_call_id FROM calls ca
         JOIN conversations c ON c.id = ca.conversation_id
         WHERE ca.account_id = $1 AND ca.inbox_id = $2 AND c.display_id = $3
         ORDER BY ca.id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(inbox_id)
    .bind(conv_did)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let call_id: i64 = call_row.try_get("id")?;
    let conf_sid = format!("CF{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "UPDATE calls SET status = 'in-progress', accepted_by_agent_id = $1,
             meta = COALESCE(meta, '{}'::jsonb) || $2, updated_at = now()
         WHERE id = $3 AND account_id = $4",
    )
    .bind(user.id)
    .bind(json!({ "conference_sid": conf_sid, "attempt_id": body.get("attempt_id") }))
    .bind(call_id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    let val = call_json(&st.db, account_id, call_id).await?;
    st.hub.to_account(account_id, "voice_call.accepted", val);

    Ok(Json(json!({
        "status": "success",
        "id": conv_did,
        "conference_sid": conf_sid,
        "using_webrtc": true,
    })))
}

async fn conference_dismiss(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id).await?;
    if let Some(conv_did) = i64_param(&body, "conversation_id") {
        sqlx::query(
            "UPDATE calls SET ring_state = COALESCE(ring_state, '{}'::jsonb) || $1, updated_at = now()
             WHERE account_id = $2 AND inbox_id = $3 AND conversation_id = (SELECT id FROM conversations WHERE account_id = $2 AND display_id = $4)",
        )
        .bind(json!({ format!("dismissed_{}", user.id): true }))
        .bind(account_id)
        .bind(inbox_id)
        .bind(conv_did)
        .execute(&st.db)
        .await?;
    }
    Ok(StatusCode::OK)
}

async fn conference_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let conv_did = i64_param(&body, "conversation_id")
        .ok_or_else(|| ApiError::bad("conversation_id is required"))?;
    let call_id: Option<i64> = sqlx::query_scalar(
        "SELECT ca.id FROM calls ca JOIN conversations c ON c.id = ca.conversation_id
         WHERE ca.account_id = $1 AND ca.inbox_id = $2 AND c.display_id = $3 ORDER BY ca.id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(inbox_id)
    .bind(conv_did)
    .fetch_optional(&st.db)
    .await?;
    let call_id = call_id.ok_or_else(ApiError::not_found)?;

    sqlx::query(
        "UPDATE calls SET status = 'completed', end_reason = 'hangup', updated_at = now() WHERE id = $1 AND account_id = $2",
    )
    .bind(call_id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    let val = call_json(&st.db, account_id, call_id).await?;
    st.hub.to_account(account_id, "voice_call.ended", val);
    Ok(Json(json!({ "status": "success", "id": conv_did })))
}

async fn conference_hold(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id).await?;
    let hold = body
        .get("hold")
        .and_then(Value::as_bool)
        .ok_or_else(|| ApiError::bad("hold boolean is required"))?;
    let conv_did = i64_param(&body, "conversation_id")
        .ok_or_else(|| ApiError::bad("conversation_id is required"))?;

    let n = sqlx::query(
        "UPDATE calls SET meta = COALESCE(meta, '{}'::jsonb) || $1, updated_at = now()
         WHERE account_id = $2 AND inbox_id = $3 AND status = 'in-progress' AND accepted_by_agent_id = $4
           AND conversation_id = (SELECT id FROM conversations WHERE account_id = $2 AND display_id = $5)",
    )
    .bind(json!({ "on_hold": hold }))
    .bind(account_id)
    .bind(inbox_id)
    .bind(user.id)
    .bind(conv_did)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::bad("Not the handling agent of an active call"));
    }
    Ok(StatusCode::OK)
}

// ============================================================================
// Inbox Voice Settings (WhatsApp Calling / Inbound / Recording / Announcements / Routing)
// ============================================================================

async fn set_whatsapp_voice_flag(
    st: &AppState,
    user: &CurrentUser,
    account_id: i64,
    inbox_id: i64,
    enabled: bool,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query(
        "UPDATE channel_whatsapp w
         SET provider_config = COALESCE(w.provider_config, '{}'::jsonb) || $1, updated_at = now()
         FROM inboxes i
         WHERE i.id = $2 AND i.account_id = $3 AND i.channel_type = 'Channel::Whatsapp' AND i.channel_id = w.id",
    )
    .bind(json!({ "voice_enabled": enabled }))
    .bind(inbox_id)
    .bind(account_id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::bad("Inbox does not support WhatsApp calling"));
    }
    Ok(StatusCode::OK)
}

async fn enable_whatsapp_calling(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    set_whatsapp_voice_flag(&st, &user, account_id, id, true).await
}

async fn disable_whatsapp_calling(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    set_whatsapp_voice_flag(&st, &user, account_id, id, false).await
}

async fn merge_inbox_voice_config(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    patch: Value,
) -> Result<(), ApiError> {
    let ch_type: Option<(String, i64)> = sqlx::query_as(
        "SELECT channel_type, channel_id::bigint FROM inboxes WHERE id = $1 AND account_id = $2",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_optional(db)
    .await?;
    let (ctype, cid) = ch_type.ok_or_else(ApiError::not_found)?;

    match ctype.as_str() {
        "Channel::TwilioSms" => {
            let n = sqlx::query(
                "UPDATE channel_twilio_sms SET provider_config = COALESCE(provider_config, '{}'::jsonb) || $1, updated_at = now()
                 WHERE id::bigint = $2 AND account_id = $3 AND voice_enabled = true",
            )
            .bind(&patch)
            .bind(cid)
            .bind(account_id as i32)
            .execute(db)
            .await?
            .rows_affected();
            if n == 0 {
                return Err(ApiError::bad("Inbox does not support calling"));
            }
        }
        "Channel::Whatsapp" => {
            sqlx::query(
                "UPDATE channel_whatsapp SET provider_config = COALESCE(provider_config, '{}'::jsonb) || $1, updated_at = now()
                 WHERE id = $2 AND account_id = $3",
            )
            .bind(&patch)
            .bind(cid)
            .bind(account_id as i32)
            .execute(db)
            .await?;
        }
        _ => return Err(ApiError::bad("Inbox does not support calling")),
    }
    Ok(())
}

async fn set_inbound_calls(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let enabled = body["inbound_calls_enabled"].as_bool().unwrap_or(false);
    merge_inbox_voice_config(
        &st.db,
        account_id,
        id,
        json!({ "inbound_calls_enabled": enabled }),
    )
    .await?;
    Ok(StatusCode::OK)
}

async fn set_call_recording(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let rec = body["recording_enabled"].as_bool().unwrap_or(false);
    let trans = body["transcription_enabled"].as_bool().unwrap_or(false);
    merge_inbox_voice_config(
        &st.db,
        account_id,
        id,
        json!({ "recording_enabled": rec, "transcription_enabled": trans }),
    )
    .await?;
    Ok(StatusCode::OK)
}

async fn update_call_announcement(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id, kind)): Path<(i64, i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    if !matches!(kind.as_str(), "greeting" | "unanswered" | "out_of_hours") {
        return Err(ApiError::not_found());
    }
    let enabled = body
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| ApiError::bad("enabled must be boolean"))?;
    let msg = body
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .ok_or_else(|| ApiError::bad("message must be string"))?;
    if enabled && msg.is_empty() {
        return Err(ApiError::bad("Announcement message is required"));
    }

    let mut patch = json!({
        "call_announcements": {
            kind: { "enabled": enabled, "message": msg }
        }
    });
    if let Some(timeout) = i64_param(&body, "timeout_seconds") {
        patch["unanswered_call_timeout_seconds"] = json!(timeout.clamp(10, 300));
    }

    merge_inbox_voice_config(&st.db, account_id, inbox_id, patch).await?;
    Ok(Json(
        crate::inboxes::inbox_json(&st.db, account_id, inbox_id, au.is_admin()).await?,
    ))
}

async fn update_call_routing(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    merge_inbox_voice_config(
        &st.db,
        account_id,
        inbox_id,
        json!({
            "ring_strategy": body.get("ring_strategy"),
            "ring_step_seconds": body.get("ring_step_seconds"),
            "prefer_free_agents": body.get("prefer_free_agents"),
            "call_cooldown_seconds": body.get("call_cooldown_seconds"),
        }),
    )
    .await?;
    Ok(Json(
        crate::inboxes::inbox_json(&st.db, account_id, inbox_id, au.is_admin()).await?,
    ))
}

// ============================================================================
// Public Twilio Voice Webhooks
// ============================================================================

fn xml_response(xml: String) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, HeaderValue::from_static("text/xml"))],
        xml,
    )
        .into_response()
}

async fn find_twilio_voice_inbox(db: &PgPool, phone: &str) -> Result<(i64, i64), ApiError> {
    let clean = phone.trim();
    let with_plus = if clean.starts_with('+') {
        clean.to_string()
    } else {
        format!("+{clean}")
    };
    let row = sqlx::query(
        "SELECT i.account_id::bigint AS account_id, i.id::bigint AS inbox_id
         FROM inboxes i
         JOIN channel_twilio_sms t ON i.channel_type = 'Channel::TwilioSms' AND i.channel_id = t.id
         WHERE t.voice_enabled = true AND (t.phone_number = $1 OR t.phone_number = $2)
         LIMIT 1",
    )
    .bind(clean)
    .bind(&with_plus)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok((row.try_get("account_id")?, row.try_get("inbox_id")?))
}

async fn twilio_call_twiml(
    State(st): State<AppState>,
    Path(phone): Path<String>,
    Form(params): Form<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let (account_id, inbox_id) = find_twilio_voice_inbox(&st.db, &phone).await?;
    let call_sid = params
        .get("CallSid")
        .cloned()
        .unwrap_or_else(|| format!("CA{}", uuid::Uuid::new_v4().simple()));
    let from = params
        .get("From")
        .cloned()
        .unwrap_or_else(|| "unknown".into());
    let to = params.get("To").cloned().unwrap_or_else(|| phone.clone());

    let contact_id: i64 = if let Some(cid) = sqlx::query_scalar::<_, i64>(
        "SELECT id::bigint FROM contacts WHERE account_id = $1 AND phone_number = $2 LIMIT 1",
    )
    .bind(account_id)
    .bind(&from)
    .fetch_optional(&st.db)
    .await?
    {
        cid
    } else {
        sqlx::query_scalar(
            "INSERT INTO contacts (account_id, name, phone_number, created_at, updated_at)
             VALUES ($1, $2, $2, now(), now()) RETURNING id::bigint",
        )
        .bind(account_id)
        .bind(&from)
        .fetch_one(&st.db)
        .await?
    };

    let (_, conv_id, _) =
        ensure_call_conversation(&st.db, account_id, inbox_id, contact_id, &from).await?;

    let call_id: i64 = sqlx::query_scalar(
        "INSERT INTO calls
            (account_id, inbox_id, conversation_id, contact_id, provider_call_id, provider, direction, status, started_at, meta, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 0, 0, 'ringing', now(), $6, now(), now())
         ON CONFLICT (provider, provider_call_id) DO UPDATE SET updated_at = now()
         RETURNING id",
    )
    .bind(account_id)
    .bind(inbox_id)
    .bind(conv_id)
    .bind(contact_id)
    .bind(&call_sid)
    .bind(json!({ "from": from, "to": to }))
    .fetch_one(&st.db)
    .await?;

    if let Ok(val) = call_json(&st.db, account_id, call_id).await {
        st.hub.to_account(account_id, "voice_call.created", val);
    }

    let conf_name = format!("pulsar_call_{call_id}");
    let wait_url = format!(
        "{}/twilio/voice/wait/{}",
        st.frontend_url.trim_end_matches('/'),
        urlencoding_simple(&phone)
    );
    Ok(xml_response(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Response><Dial><Conference waitUrl="{wait_url}" startConferenceOnEnter="false" endConferenceOnExit="true">{conf_name}</Conference></Dial></Response>"#
    )))
}

async fn twilio_wait_twiml(Path(_phone): Path<String>) -> Response {
    xml_response(
        r#"<?xml version="1.0" encoding="UTF-8"?><Response><Say>Please hold while we connect you to an agent.</Say><Pause length="10"/></Response>"#.into(),
    )
}

async fn twilio_resume_twiml(Path(_phone): Path<String>) -> Response {
    xml_response(r#"<?xml version="1.0" encoding="UTF-8"?><Response/>"#.into())
}

async fn twilio_status(
    State(st): State<AppState>,
    Path(phone): Path<String>,
    Form(params): Form<HashMap<String, String>>,
) -> Result<StatusCode, ApiError> {
    let (account_id, _) = find_twilio_voice_inbox(&st.db, &phone).await?;
    let Some(call_sid) = params.get("CallSid") else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let status = params
        .get("CallStatus")
        .map(String::as_str)
        .unwrap_or("completed");
    let dur: Option<i32> = params
        .get("CallDuration")
        .and_then(|s| s.parse::<i32>().ok());

    if let Some(id) = sqlx::query_scalar::<_, i64>(
        "UPDATE calls SET status = $1, duration_seconds = COALESCE($2, duration_seconds), updated_at = now()
         WHERE account_id = $3 AND provider = 0 AND provider_call_id = $4
         RETURNING id",
    )
    .bind(status)
    .bind(dur)
    .bind(account_id)
    .bind(call_sid)
    .fetch_optional(&st.db)
    .await?
        && let Ok(val) = call_json(&st.db, account_id, id).await
    {
        st.hub.to_account(account_id, "voice_call.updated", val);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn twilio_conference_status(
    State(st): State<AppState>,
    Path(phone): Path<String>,
    Form(params): Form<HashMap<String, String>>,
) -> Result<StatusCode, ApiError> {
    let (account_id, _) = find_twilio_voice_inbox(&st.db, &phone).await?;
    if let (Some(call_sid), Some(conf_sid)) = (params.get("CallSid"), params.get("ConferenceSid")) {
        let _ = sqlx::query(
            "UPDATE calls SET meta = COALESCE(meta, '{}'::jsonb) || $1, updated_at = now()
             WHERE account_id = $2 AND provider = 0 AND provider_call_id = $3",
        )
        .bind(json!({ "conference_sid": conf_sid, "conference_event": params.get("StatusCallbackEvent") }))
        .bind(account_id)
        .bind(call_sid)
        .execute(&st.db)
        .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn twilio_recording_status(
    State(st): State<AppState>,
    Path(phone): Path<String>,
    Form(params): Form<HashMap<String, String>>,
) -> Result<StatusCode, ApiError> {
    let (account_id, _) = find_twilio_voice_inbox(&st.db, &phone).await?;
    if let (Some(call_sid), Some(rec_url)) = (params.get("CallSid"), params.get("RecordingUrl")) {
        let _ = sqlx::query(
            "UPDATE calls SET meta = COALESCE(meta, '{}'::jsonb) || $1, updated_at = now()
             WHERE account_id = $2 AND provider = 0 AND provider_call_id = $3",
        )
        .bind(json!({ "recording_url": rec_url }))
        .bind(account_id)
        .bind(call_sid)
        .execute(&st.db)
        .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn whatsapp_call_hours_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let cfg: Option<Value> = sqlx::query_scalar(
        "SELECT w.provider_config
         FROM inboxes i
         JOIN channel_whatsapp w ON i.channel_type = 'Channel::Whatsapp' AND i.channel_id = w.id
         WHERE i.id = $1 AND i.account_id = $2",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?;
    let cfg = cfg.ok_or_else(|| ApiError::bad("Inbox does not support WhatsApp calling"))?;
    if !cfg["voice_enabled"].as_bool().unwrap_or(false) {
        return Err(ApiError::bad("Inbox does not support WhatsApp calling"));
    }
    Ok(Json(json!({
        "matches_whatsapp": true,
        "call_hours": cfg.get("call_hours").cloned().unwrap_or_else(|| json!({})),
    })))
}

async fn whatsapp_call_hours_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
    body: Option<Json<Value>>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let hours = body
        .map(|j| j.0.get("call_hours").cloned().unwrap_or(j.0))
        .unwrap_or_else(|| json!({ "synced": true }));
    let n = sqlx::query(
        "UPDATE channel_whatsapp w
         SET provider_config = COALESCE(w.provider_config, '{}'::jsonb) || jsonb_build_object('call_hours', $1::jsonb), updated_at = now()
         FROM inboxes i
         WHERE i.id = $2 AND i.account_id = $3 AND i.channel_type = 'Channel::Whatsapp' AND i.channel_id = w.id",
    )
    .bind(hours)
    .bind(inbox_id)
    .bind(account_id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::bad("Inbox does not support WhatsApp calling"));
    }
    Ok(StatusCode::OK)
}

async fn set_whatsapp_callback_permission(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let enabled = body["callback_permission_enabled"]
        .as_bool()
        .unwrap_or(true);
    merge_inbox_voice_config(
        &st.db,
        account_id,
        inbox_id,
        json!({ "callback_permission_enabled": enabled }),
    )
    .await?;
    Ok(StatusCode::OK)
}

async fn call_whisper_transcribe(
    db: &PgPool,
    filename: &str,
    content_type: &str,
    audio_bytes: &[u8],
) -> Result<String, ApiError> {
    let llm = crate::captain::resolve_llm_config(db, None).await?;
    let base_url = std::env::var("WHISPER_API_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(llm.base_url);
    let api_key = std::env::var("WHISPER_API_KEY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(llm.api_key);
    if api_key.is_empty() && base_url.contains("api.openai.com") {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Audio transcription requires OPENAI_API_KEY or CAPTAIN_LLM_API_KEY".into(),
        ));
    }
    let model = std::env::var("WHISPER_MODEL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "whisper-1".into());
    let safe_name: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe_name = if safe_name.is_empty() {
        "audio.ogg".to_string()
    } else {
        safe_name
    };
    let safe_ct = if content_type.contains('/') && !content_type.contains(['\r', '\n']) {
        content_type
    } else {
        "audio/ogg"
    };

    let boundary = format!("----PulsarWhisper{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::with_capacity(audio_bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{safe_name}\"\r\nContent-Type: {safe_ct}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(audio_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|_| ApiError::bad("Failed to initialize transcription client"))?;
    let mut req = client
        .post(format!("{base_url}/audio/transcriptions"))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body);
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let resp = req.send().await.map_err(|e| {
        tracing::warn!(error = %e, "whisper request failed");
        ApiError(
            StatusCode::BAD_GATEWAY,
            "Transcription provider request failed".into(),
        )
    })?;
    let status = resp.status();
    let raw = resp.bytes().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ApiError(
            StatusCode::BAD_GATEWAY,
            format!("Transcription provider returned {status}"),
        ));
    }
    let parsed: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    let text = parsed["text"]
        .as_str()
        .map(str::trim)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| String::from_utf8_lossy(&raw).trim().to_string());
    Ok(text)
}

/// POST /api/v1/accounts/{account_id}/conversations/{display_id}/messages/{message_id}/transcribe
pub(crate) async fn transcribe_message(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, display_id, message_id)): Path<(i64, i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let row: Option<(i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT c.id::bigint, c.inbox_id::bigint, m.content_attributes::jsonb->>'transcribed_text'
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE c.account_id = $1 AND c.display_id = $2 AND m.id = $3 AND m.account_id = $1",
    )
    .bind(account_id)
    .bind(display_id)
    .bind(message_id)
    .fetch_optional(&st.db)
    .await?;
    let (conv_id, inbox_id, existing_text) = row.ok_or_else(ApiError::not_found)?;
    if !au.is_admin() {
        let member: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM inbox_members WHERE inbox_id = $1 AND user_id = $2")
                .bind(inbox_id as i32)
                .bind(user.id as i32)
                .fetch_optional(&st.db)
                .await?;
        if member.is_none() {
            return Err(ApiError::not_found());
        }
    }
    if let Some(existing) = existing_text.filter(|s| !s.trim().is_empty()) {
        return Ok(Json(json!({
            "content": existing,
            "transcribed_text": existing,
            "message_id": message_id,
            "conversation_id": conv_id,
        })));
    }
    let (filename, ct, bytes) = crate::storage::read_first_audio_blob(&st.db, message_id).await?;
    let text = call_whisper_transcribe(&st.db, &filename, &ct, &bytes).await?;
    sqlx::query(
        "UPDATE messages
         SET content_attributes = (COALESCE(content_attributes::jsonb, '{}'::jsonb) || jsonb_build_object('transcribed_text', $1::text))::json,
             updated_at = now()
         WHERE id = $2::bigint AND account_id = $3",
    )
    .bind(&text)
    .bind(message_id)
    .bind(account_id as i32)
    .execute(&st.db)
    .await?;
    Ok(Json(json!({
        "content": text,
        "transcribed_text": text,
        "message_id": message_id,
        "conversation_id": conv_id,
    })))
}

/// POST /api/v1/accounts/{account_id}/calls/{id}/transcribe
async fn transcribe_call(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    body: Option<Json<Value>>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let row: Option<(Option<Value>, Option<String>)> = sqlx::query_as(
        "SELECT meta, transcript FROM calls WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?;
    let (meta, existing_transcript) = row.ok_or_else(ApiError::not_found)?;
    if let Some(t) = existing_transcript.filter(|s| !s.trim().is_empty()) {
        let mut call = call_json(&st.db, account_id, id).await?;
        call["transcript"] = json!(t);
        return Ok(Json(call));
    }
    let rec_url = body
        .as_ref()
        .and_then(|b| sopt(&b.0, "recording_url"))
        .or_else(|| {
            meta.as_ref()
                .and_then(|m| m["recording_url"].as_str())
                .map(str::to_string)
        })
        .ok_or_else(|| ApiError::bad("Call has no recording_url"))?;
    let (bytes, ct) = crate::listeners::webhooks::safe_fetch_bytes(&rec_url, 25 * 1024 * 1024)
        .await
        .map_err(|_| ApiError::bad("Failed to fetch call recording"))?;
    let text = call_whisper_transcribe(&st.db, "call-recording.mp3", &ct, &bytes).await?;
    sqlx::query("UPDATE calls SET transcript = $1, updated_at = now() WHERE account_id = $2 AND id = $3")
        .bind(&text)
        .bind(account_id)
        .bind(id)
        .execute(&st.db)
        .await?;
    let val = call_json(&st.db, account_id, id).await?;
    st.hub.to_account(account_id, "voice_call.updated", val.clone());
    Ok(Json(val))
}
