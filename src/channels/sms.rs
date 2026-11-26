//! Bandwidth SMS channel: inbox setup, inbound webhook (/webhooks/sms/{phone_number}) and delivery status.
//! Mirrors chatwoot/app/models/channel/sms.rb, app/controllers/webhooks/sms_controller.rb,
//! app/jobs/webhooks/sms_events_job.rb, app/services/sms/{incoming_message,delivery_status,bandwidth_token}_service.rb
//! and app/controllers/api/v1/accounts/concerns/bandwidth_configuration.rb.
//! Outbound sends run in listeners/sms_outbound.rs through `send_text`.

use std::time::Duration;

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::{
    common::{ApiError, AppState},
    conversations::{Conv, insert_message, reload},
};

const INVALID_CONFIG: &str =
    "Provide a valid Bandwidth account ID, application ID, client ID, and client secret.";
const AUTH_FAILED: &str = "Unable to authenticate with Bandwidth. Check your client ID and client secret, then try again.";
const TOKEN_FAILED: &str = "Unable to reach Bandwidth authentication. Please try again later.";
const CONFIG_KEYS: [&str; 6] = [
    "account_id",
    "application_id",
    "api_key",
    "api_secret",
    "client_id",
    "client_secret",
];
const OAUTH_KEYS: [&str; 4] = ["account_id", "application_id", "client_id", "client_secret"];
const LEGACY_KEYS: [&str; 4] = ["account_id", "application_id", "api_key", "api_secret"];

pub fn routes() -> Router<AppState> {
    Router::new().route("/webhooks/sms/{phone_number}", post(webhook))
}

/// BANDWIDTH_API_URL overrides both the messaging and OAuth hosts (tests point it at a local mock).
fn base(default: &str) -> String {
    std::env::var("BANDWIDTH_API_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn is_oauth(cfg: &Map<String, Value>) -> bool {
    cfg.contains_key("client_id") || cfg.contains_key("client_secret")
}

fn cfg_str<'a>(cfg: &'a Map<String, Value>, key: &str) -> &'a str {
    cfg.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Sms model validation: required keys for the auth mode, no unknown keys, every value a non-blank string.
fn valid_config(cfg: &Map<String, Value>) -> bool {
    let required: &[&str] = if is_oauth(cfg) {
        &OAUTH_KEYS
    } else {
        &LEGACY_KEYS
    };
    required.iter().all(|k| cfg.contains_key(*k))
        && cfg.keys().all(|k| CONFIG_KEYS.contains(&k.as_str()))
        && cfg
            .values()
            .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
}

/// OAuth2 client credentials with per-(endpoint, credential-digest) cache (50m TTL).
async fn fetch_token(cfg: &Map<String, Value>) -> Result<String, ApiError> {
    use sha2::{Digest, Sha256};
    use std::{
        collections::HashMap,
        sync::{LazyLock, Mutex},
        time::Instant,
    };
    const TTL: Duration = Duration::from_secs(50 * 60);
    static TOKEN_CACHE: LazyLock<Mutex<HashMap<(String, [u8; 32]), (String, Instant)>>> =
        LazyLock::new(Default::default);

    let endpoint = format!("{}/api/v1/oauth2/token", base("https://api.bandwidth.com"));
    let client_id = cfg_str(cfg, "client_id");
    let client_secret = cfg_str(cfg, "client_secret");
    let cred_hash: [u8; 32] = Sha256::new()
        .chain_update(client_id)
        .chain_update([0])
        .chain_update(client_secret)
        .finalize()
        .into();
    let cache_key = (endpoint.clone(), cred_hash);
    if let Ok(m) = TOKEN_CACHE.lock()
        && let Some((tok, at)) = m.get(&cache_key)
        && at.elapsed() < TTL
    {
        return Ok(tok.clone());
    }

    let resp = reqwest::Client::new()
        .post(&endpoint)
        .timeout(Duration::from_secs(15))
        .basic_auth(client_id, Some(client_secret))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("grant_type=client_credentials")
        .send()
        .await
        .map_err(|_| ApiError(StatusCode::SERVICE_UNAVAILABLE, TOKEN_FAILED.into()))?;
    let status = resp.status();
    // Provider error bodies can echo credentials, so they are never surfaced.
    if matches!(status.as_u16(), 400 | 401 | 403) {
        return Err(ApiError::bad(AUTH_FAILED));
    }
    if !status.is_success() {
        return Err(ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            TOKEN_FAILED.into(),
        ));
    }
    let body: Value =
        serde_json::from_slice(&resp.bytes().await.unwrap_or_default()).unwrap_or(Value::Null);
    let tok = body["access_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ApiError(StatusCode::SERVICE_UNAVAILABLE, TOKEN_FAILED.into()))?;
    if let Ok(mut m) = TOKEN_CACHE.lock() {
        if m.len() >= 1_000 {
            m.retain(|_, (_, at)| at.elapsed() < TTL);
        }
        m.insert(cache_key, (tok.clone(), Instant::now()));
    }
    Ok(tok)
}

/// Sends one text. Ok(provider id, empty when absent) or Err(provider description), like Sms::SendOnSmsService.
pub async fn send_text(
    cfg: &Map<String, Value>,
    from: &str,
    to: &str,
    text: &str,
) -> Result<String, String> {
    let url = format!(
        "{}/api/v2/users/{}/messages",
        base("https://messaging.bandwidth.com"),
        cfg_str(cfg, "account_id")
    );
    let body = json!({ "to": to, "from": from, "text": text, "applicationId": cfg_str(cfg, "application_id") });
    let req = reqwest::Client::new()
        .post(&url)
        .timeout(Duration::from_secs(15))
        .header("content-type", "application/json");
    let req = if is_oauth(cfg) {
        req.bearer_auth(fetch_token(cfg).await.map_err(|e| e.1)?)
    } else {
        req.basic_auth(cfg_str(cfg, "api_key"), Some(cfg_str(cfg, "api_secret")))
    };
    let resp = req
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("Bandwidth request error: {e}"))?;
    let status = resp.status();
    let parsed: Value =
        serde_json::from_slice(&resp.bytes().await.unwrap_or_default()).unwrap_or(Value::Null);
    if status.is_success() {
        Ok(parsed["id"].as_str().unwrap_or_default().to_string())
    } else {
        Err(parsed["description"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Bandwidth error ({})", status.as_u16())))
    }
}

/// Validate and insert channel_sms. Credentials are checked against Bandwidth OAuth before insert, as Chatwoot does.
pub(crate) async fn create_channel(
    tx: &mut Transaction<'_, Postgres>,
    account_id: i64,
    ch: &Value,
) -> Result<(i64, &'static str), ApiError> {
    let phone = ch["phone_number"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Phone number can't be blank"))?
        .to_string();
    let taken: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM channel_sms WHERE phone_number = $1 LIMIT 1")
            .bind(&phone)
            .fetch_optional(&mut **tx)
            .await?;
    if taken.is_some() {
        return Err(ApiError::bad("Phone number has already been taken"));
    }

    let cfg = ch["provider_config"]
        .as_object()
        .ok_or_else(|| ApiError::bad(INVALID_CONFIG))?;
    if !valid_config(cfg) || !is_oauth(cfg) {
        return Err(ApiError::bad(INVALID_CONFIG));
    }
    fetch_token(cfg).await?;

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO channel_sms (account_id, phone_number, provider, provider_config, created_at, updated_at)
         VALUES ($1, $2, 'default', $3, now(), now()) RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(&phone)
    .bind(sqlx::types::Json(Value::Object(cfg.clone())))
    .fetch_one(&mut **tx)
    .await?;
    Ok((id, "Channel::Sms"))
}

struct SmsChannel {
    inbox_id: i64,
    account_id: i64,
    lock_to_single: bool,
}

/// POST /webhooks/sms/{phone_number}. Always 200 like Chatwoot's `head :ok`, so the provider does not retry.
/// The path phone is unused; the channel is looked up by the payload `to`, as Chatwoot's SmsEventsJob does.
async fn webhook(
    State(st): State<AppState>,
    Path(_phone): Path<String>,
    body: Bytes,
) -> StatusCode {
    let payload: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    // Chatwoot reads params['_json'].first: the provider posts an array of events.
    let event = payload.get(0).cloned().unwrap_or(Value::Null);
    if let Err(e) = process(&st, &event).await {
        tracing::warn!(error = ?e, "sms webhook processing failed");
    }
    StatusCode::OK
}

async fn process(st: &AppState, event: &Value) -> Result<(), ApiError> {
    let kind = event["type"].as_str().unwrap_or_default();
    if !matches!(
        kind,
        "message-received" | "message-delivered" | "message-failed"
    ) {
        return Ok(());
    }
    let Some(to) = event["to"].as_str().filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let Some(ch) = find_channel(&st.db, to).await? else {
        return Ok(());
    };
    match kind {
        "message-received" => incoming(st, &ch, &event["message"]).await,
        _ => delivery_status(&st.db, &ch, kind, event).await,
    }
}

async fn find_channel(db: &PgPool, phone: &str) -> Result<Option<SmsChannel>, ApiError> {
    let row = sqlx::query(
        "SELECT i.id::bigint AS inbox_id, i.account_id::bigint AS account_id, i.lock_to_single_conversation
         FROM channel_sms cs
         JOIN inboxes i ON i.channel_id = cs.id AND i.channel_type = 'Channel::Sms'
         WHERE cs.phone_number = $1
         LIMIT 1",
    )
    .bind(phone)
    .fetch_optional(db)
    .await?;
    let Some(r) = row else {
        return Ok(None);
    };
    Ok(Some(SmsChannel {
        inbox_id: r.try_get("inbox_id")?,
        account_id: r.try_get("account_id")?,
        lock_to_single: r.try_get("lock_to_single_conversation")?,
    }))
}

/// Sms::IncomingMessageService with SafeFetch MMS media download.
async fn incoming(st: &AppState, ch: &SmsChannel, msg: &Value) -> Result<(), ApiError> {
    let from = msg["from"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("From is required"))?;
    let source_id = msg["id"].as_str().unwrap_or_default();
    if !source_id.is_empty() {
        let dup: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM messages WHERE inbox_id = $1 AND source_id = $2 LIMIT 1",
        )
        .bind(ch.inbox_id as i32)
        .bind(source_id)
        .fetch_optional(&st.db)
        .await?;
        if dup.is_some() {
            return Ok(());
        }
    }

    let text = msg["text"].as_str().unwrap_or_default().replace('\0', "");
    let (contact_id, contact_inbox_id) = find_or_create_contact(&st.db, ch, from).await?;
    let conv = find_or_create_conversation(&st.db, ch, contact_id, contact_inbox_id).await?;
    let attrs = if source_id.is_empty() {
        json!({})
    } else {
        json!({ "__pulsar_source_id": source_id })
    };
    let inserted = insert_message(
        st,
        &conv,
        Some(("Contact", contact_id)),
        &text,
        0,
        false,
        0,
        attrs,
        None,
    )
    .await?;
    if let (Some(msg_id), Some(media)) = (
        inserted["id"].as_i64(),
        msg.get("media").and_then(Value::as_array),
    ) {
        for raw_url in media.iter().filter_map(Value::as_str).take(10) {
            let trimmed = raw_url.trim();
            if trimmed.is_empty() {
                continue;
            }
            match crate::listeners::webhooks::safe_fetch_bytes(trimmed, 15 * 1024 * 1024).await {
                Ok((bytes, ct)) => {
                    let fname = url::Url::parse(trimmed)
                        .ok()
                        .and_then(|u| {
                            u.path_segments()
                                .and_then(|mut segs| segs.next_back().map(str::to_string))
                        })
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "attachment".into());
                    if let Err(e) = crate::storage::attach_to_message(
                        &st.db,
                        ch.account_id,
                        msg_id,
                        &fname,
                        &ct,
                        &bytes,
                        None,
                    )
                    .await
                    {
                        tracing::warn!(url = trimmed, error = ?e, "sms media attach failed");
                    }
                }
                Err(e) => {
                    tracing::warn!(url = trimmed, error = %e, "sms media fetch failed");
                }
            }
        }
    }
    Ok(())
}

async fn find_or_create_contact(
    db: &PgPool,
    ch: &SmsChannel,
    phone: &str,
) -> Result<(i64, i64), ApiError> {
    if let Some(r) = sqlx::query(
        "SELECT id::bigint AS ci_id, contact_id::bigint AS contact_id
         FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2 LIMIT 1",
    )
    .bind(ch.inbox_id as i32)
    .bind(phone)
    .fetch_optional(db)
    .await?
    {
        return Ok((r.try_get("contact_id")?, r.try_get("ci_id")?));
    }

    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM contacts WHERE account_id = $1 AND phone_number = $2 LIMIT 1",
    )
    .bind(ch.account_id as i32)
    .bind(phone)
    .fetch_optional(db)
    .await?;
    let contact_id = match existing {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO contacts (name, phone_number, account_id, created_at, updated_at, last_activity_at)
                 VALUES ($1, $2, $3, now(), now(), now()) RETURNING id::bigint",
            )
            .bind(phone)
            .bind(phone)
            .bind(ch.account_id as i32)
            .fetch_one(db)
            .await?
        }
    };

    sqlx::query(
        "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at, pubsub_token)
         VALUES ($1, $2, $3, now(), now(), $4)
         ON CONFLICT (inbox_id, source_id) DO NOTHING",
    )
    .bind(contact_id)
    .bind(ch.inbox_id as i32)
    .bind(phone)
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(db)
    .await?;

    let row = sqlx::query(
        "SELECT id::bigint AS ci_id, contact_id::bigint AS contact_id
         FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2 LIMIT 1",
    )
    .bind(ch.inbox_id as i32)
    .bind(phone)
    .fetch_one(db)
    .await?;
    Ok((row.try_get("contact_id")?, row.try_get("ci_id")?))
}

async fn find_or_create_conversation(
    db: &PgPool,
    ch: &SmsChannel,
    contact_id: i64,
    contact_inbox_id: i64,
) -> Result<Conv, ApiError> {
    // Chatwoot: lock_to_single uses the last conversation of any status; otherwise the last non-resolved one (1 = resolved).
    let existing: Option<i64> = if ch.lock_to_single {
        sqlx::query_scalar(
            "SELECT id::bigint FROM conversations WHERE contact_inbox_id = $1 AND account_id = $2 ORDER BY id DESC LIMIT 1",
        )
    } else {
        sqlx::query_scalar(
            "SELECT id::bigint FROM conversations WHERE contact_inbox_id = $1 AND account_id = $2 AND status <> 1 ORDER BY id DESC LIMIT 1",
        )
    }
    .bind(contact_inbox_id)
    .bind(ch.account_id as i32)
    .fetch_optional(db)
    .await?;

    let conv_id = match existing {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO conversations (account_id, inbox_id, status, contact_id, contact_inbox_id,
                                            additional_attributes, custom_attributes, last_activity_at, created_at, updated_at)
                 VALUES ($1, $2, 0, $3, $4, '{}', '{}', now(), now(), now()) RETURNING id::bigint",
            )
            .bind(ch.account_id as i32)
            .bind(ch.inbox_id as i32)
            .bind(contact_id)
            .bind(contact_inbox_id)
            .fetch_one(db)
            .await?
        }
    };
    reload(db, conv_id).await
}

/// Sms::DeliveryStatusService. message-failed carries errorCode/description at the top level.
async fn delivery_status(
    db: &PgPool,
    ch: &SmsChannel,
    kind: &str,
    event: &Value,
) -> Result<(), ApiError> {
    let Some(msg_id) = event["message"]["id"].as_str().filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let (status, external_error): (i32, Option<String>) = if kind == "message-delivered" {
        (1, None)
    } else {
        // Bandwidth sends errorCode as a number (e.g. 30003); accept string too.
        let code = match &event["errorCode"] {
            Value::Number(n) => Some(n.to_string()),
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };
        (
            3,
            code.map(|c| {
                format!(
                    "{c} - {}",
                    event["description"].as_str().unwrap_or_default()
                )
            }),
        )
    };

    match external_error {
        Some(err) => {
            sqlx::query(
                "UPDATE messages
                 SET status = $1,
                     content_attributes = COALESCE(content_attributes, '{}'::json)::jsonb || jsonb_build_object('external_error', $2::text),
                     updated_at = now()
                 WHERE inbox_id::bigint = $3 AND source_id = $4",
            )
            .bind(status)
            .bind(err)
            .bind(ch.inbox_id)
            .bind(msg_id)
            .execute(db)
            .await?;
        }
        None => {
            sqlx::query("UPDATE messages SET status = $1, updated_at = now() WHERE inbox_id::bigint = $2 AND source_id = $3")
                .bind(status)
                .bind(ch.inbox_id)
                .bind(msg_id)
                .execute(db)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bandwidth_provider_config() {
        let oauth = json!({"account_id": "a", "application_id": "b", "client_id": "c", "client_secret": "d"});
        let legacy =
            json!({"account_id": "a", "application_id": "b", "api_key": "k", "api_secret": "s"});
        assert!(valid_config(oauth.as_object().unwrap()));
        assert!(valid_config(legacy.as_object().unwrap()));
        assert!(!valid_config(
            json!({"account_id": "a", "application_id": "b", "client_id": "c"})
                .as_object()
                .unwrap()
        ));
        assert!(!valid_config(json!({"account_id": "a", "application_id": "b", "client_id": "c", "client_secret": "d", "x": "y"}).as_object().unwrap()));
        assert!(!valid_config(json!({"account_id": "a", "application_id": " ", "client_id": "c", "client_secret": "d"}).as_object().unwrap()));
        assert!(!valid_config(
            json!({"account_id": 1, "application_id": "b", "client_id": "c", "client_secret": "d"})
                .as_object()
                .unwrap()
        ));
    }
}
