//! Twilio SMS/WhatsApp channel: setup, inbound callback (/twilio/callback), delivery status (/twilio/delivery_status).
//! Mirrors chatwoot/app/models/channel/twilio_sms.rb,
//! app/controllers/api/v1/accounts/channels/twilio_channels_controller.rb,
//! app/controllers/twilio/{callback,delivery_status}_controller.rb,
//! and app/services/twilio/{incoming_message,delivery_status}_service.rb.

use std::collections::BTreeMap;

use axum::{
    body::Bytes,
    extract::{OriginalUri, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha1::Sha1;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::{
    ar_encryption,
    common::{require_account, ApiError, ApiResult, AppState, CurrentUser},
    conversations::{insert_message, reload},
};

pub(crate) fn api_base() -> String {
    std::env::var("TWILIO_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://api.twilio.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/twilio/callback", post(callback))
        .route("/twilio/delivery_status", post(delivery_status))
}

pub fn account_routes() -> Router<AppState> {
    Router::new().route("/channels/twilio_channel", post(create_twilio_channel))
}

fn pick<'a>(b: &'a Value, key: &str) -> &'a Value {
    if b[key].is_object() {
        &b[key]
    } else {
        b
    }
}

fn trim_opt(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_medium(v: &Value) -> Result<i32, ApiError> {
    match v {
        Value::Null => Ok(0),
        Value::Number(n) => match n.as_i64() {
            Some(0) => Ok(0),
            Some(1) => Ok(1),
            _ => Err(ApiError::bad("Invalid medium")),
        },
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "" | "sms" => Ok(0),
            "whatsapp" => Ok(1),
            _ => Err(ApiError::bad("Invalid medium")),
        },
        _ => Err(ApiError::bad("Invalid medium")),
    }
}

/// Validate Twilio channel fields and insert into channel_twilio_sms.
pub(crate) async fn create_channel(
    tx: &mut Transaction<'_, Postgres>,
    account_id: i64,
    ch: &Value,
) -> Result<(i64, &'static str), ApiError> {
    let account_sid = trim_opt(&ch["account_sid"])
        .ok_or_else(|| ApiError::bad("Account sid can't be blank"))?;
    let auth_token = trim_opt(&ch["auth_token"])
        .ok_or_else(|| ApiError::bad("Auth token can't be blank"))?;
    let api_key_sid = trim_opt(&ch["api_key_sid"]);
    let medium = parse_medium(&ch["medium"])?;
    let messaging_service_sid = trim_opt(&ch["messaging_service_sid"]);
    let raw_phone = trim_opt(&ch["phone_number"]);

    if messaging_service_sid.is_some() && raw_phone.is_some() {
        return Err(ApiError::bad(
            "Phone number must be blank when messaging service sid is present",
        ));
    }
    if messaging_service_sid.is_none() && raw_phone.is_none() {
        return Err(ApiError::bad(
            "Phone number or messaging service sid can't be blank",
        ));
    }

    let phone_number = raw_phone.map(|p| {
        if medium == 1 {
            let stripped = p.strip_prefix("whatsapp:").unwrap_or(&p);
            format!("whatsapp:{stripped}")
        } else {
            p
        }
    });

    if let Some(ref msid) = messaging_service_sid {
        let dup: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM channel_twilio_sms WHERE messaging_service_sid = $1 LIMIT 1",
        )
        .bind(msid)
        .fetch_optional(&mut **tx)
        .await?;
        if dup.is_some() {
            return Err(ApiError::bad("Messaging service sid has already been taken"));
        }
    }

    if let Some(ref phone) = phone_number {
        let dup: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM channel_twilio_sms WHERE phone_number = $1 LIMIT 1",
        )
        .bind(phone)
        .fetch_optional(&mut **tx)
        .await?;
        if dup.is_some() {
            return Err(ApiError::bad("Phone number has already been taken"));
        }
    }

    let encrypted_token = ar_encryption::encrypt(&auth_token);

    let channel_id: i64 = sqlx::query_scalar(
        "INSERT INTO channel_twilio_sms
            (account_id, account_sid, auth_token, api_key_sid, messaging_service_sid, phone_number, medium, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())
         RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(&account_sid)
    .bind(&encrypted_token)
    .bind(&api_key_sid)
    .bind(&messaging_service_sid)
    .bind(&phone_number)
    .bind(medium)
    .fetch_one(&mut **tx)
    .await?;

    Ok((channel_id, "Channel::TwilioSms"))
}

/// POST /api/v1/accounts/{account_id}/channels/twilio_channel
async fn create_twilio_channel(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;

    let ch = pick(&body, "twilio_channel");
    let name = trim_opt(&ch["name"])
        .or_else(|| trim_opt(&body["name"]))
        .ok_or_else(|| ApiError::bad("Name can't be blank"))?;

    let mut tx = st.db.begin().await?;
    let (channel_id, channel_type) = create_channel(&mut tx, account_id, ch).await?;

    let inbox_id: i64 = sqlx::query_scalar(
        "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
         VALUES ($1::bigint, $2::bigint, $3, $4, now(), now()) RETURNING id::bigint",
    )
    .bind(channel_id)
    .bind(account_id)
    .bind(&name)
    .bind(channel_type)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO inbox_members (user_id, inbox_id, created_at, updated_at)
         VALUES ($1::bigint, $2::bigint, now(), now())",
    )
    .bind(user.id)
    .bind(inbox_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    st.hub.invalidate_cache(account_id, "inbox");

    let row = sqlx::query(
        "SELECT i.channel_id::bigint AS channel_id, i.name, i.channel_type,
                i.greeting_enabled, i.greeting_message, i.working_hours_enabled,
                i.enable_email_collect, i.csat_survey_enabled, i.csat_config,
                i.enable_auto_assignment, i.auto_assignment_config,
                i.out_of_office_message, i.timezone, i.allow_messages_after_resolved,
                i.lock_to_single_conversation, i.sender_name_type, i.business_name,
                ct.account_sid, ct.auth_token, ct.api_key_sid,
                ct.messaging_service_sid, ct.phone_number,
                COALESCE(ct.medium, 0)::int AS medium, ct.content_templates
         FROM inboxes i
         JOIN channel_twilio_sms ct ON ct.id = i.channel_id AND ct.account_id = i.account_id
         WHERE i.id::bigint = $1 AND i.account_id::bigint = $2",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    let sender: i32 = row.try_get("sender_name_type")?;
    let medium_int: i32 = row.try_get("medium")?;
    let medium_str = if medium_int == 1 { "whatsapp" } else { "sms" };
    let stored_token: String = row.try_get("auth_token")?;

    Ok(Json(json!({
        "id": inbox_id,
        "avatar_url": "",
        "channel_id": row.try_get::<i64, _>("channel_id")?,
        "name": row.try_get::<String, _>("name")?,
        "channel_type": row.try_get::<Option<String>, _>("channel_type")?,
        "greeting_enabled": row.try_get::<Option<bool>, _>("greeting_enabled")?,
        "greeting_message": row.try_get::<Option<String>, _>("greeting_message")?,
        "working_hours_enabled": row.try_get::<Option<bool>, _>("working_hours_enabled")?,
        "enable_email_collect": row.try_get::<Option<bool>, _>("enable_email_collect")?,
        "csat_survey_enabled": row.try_get::<Option<bool>, _>("csat_survey_enabled")?,
        "csat_config": row.try_get::<Option<Value>, _>("csat_config")?,
        "enable_auto_assignment": row.try_get::<Option<bool>, _>("enable_auto_assignment")?,
        "auto_assignment_config": row.try_get::<Option<Value>, _>("auto_assignment_config")?,
        "out_of_office_message": row.try_get::<Option<String>, _>("out_of_office_message")?,
        "working_hours": [],
        "timezone": row.try_get::<Option<String>, _>("timezone")?,
        "callback_webhook_url": null,
        "allow_messages_after_resolved": row.try_get::<Option<bool>, _>("allow_messages_after_resolved")?,
        "lock_to_single_conversation": row.try_get::<bool, _>("lock_to_single_conversation")?,
        "sender_name_type": if sender == 1 { "external" } else { "friendly" },
        "business_name": row.try_get::<Option<String>, _>("business_name")?,
        "messaging_service_sid": row.try_get::<Option<String>, _>("messaging_service_sid")?,
        "phone_number": row.try_get::<Option<String>, _>("phone_number")?,
        "medium": medium_str,
        "content_templates": row.try_get::<Option<Value>, _>("content_templates")?,
        "account_sid": row.try_get::<String, _>("account_sid")?,
        "auth_token": ar_encryption::decrypt(&stored_token),
        "api_key_sid": row.try_get::<Option<String>, _>("api_key_sid")?,
    })))
}

fn parse_form(body: &[u8]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(body) {
        map.insert(k.into_owned(), v.into_owned());
    }
    map
}

fn request_url(headers: &HeaderMap, uri: &OriginalUri) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("http");
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("localhost");
    let path_and_query = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(uri.path());
    format!("{scheme}://{host}{path_and_query}")
}

/// Twilio RequestValidator algorithm:
/// full URL + sorted POST param key+value pairs (no delimiters), HMAC-SHA1 with auth_token, base64.
fn verify_twilio_signature(
    auth_token: &str,
    url: &str,
    params: &BTreeMap<String, String>,
    signature: Option<&str>,
) -> bool {
    let Some(sig) = signature.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let Ok(sig_bytes) = STANDARD.decode(sig) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(auth_token.as_bytes()) else {
        return false;
    };
    let mut data = String::with_capacity(url.len() + params.len() * 32);
    data.push_str(url);
    for (k, v) in params {
        data.push_str(k);
        data.push_str(v);
    }
    mac.update(data.as_bytes());
    mac.verify_slice(&sig_bytes).is_ok()
}

struct TwilioChannelRow {
    inbox_id: i64,
    account_id: i64,
    lock_to_single_conversation: bool,
    medium: i32,
    auth_token: String,
}

async fn find_channel_for_number(
    db: &PgPool,
    messaging_service_sid: Option<&str>,
    account_sid: Option<&str>,
    number: Option<&str>,
) -> Result<Option<TwilioChannelRow>, ApiError> {
    if let Some(msid) = messaging_service_sid.filter(|s| !s.is_empty()) {
        let row = sqlx::query(
            "SELECT i.id::bigint AS inbox_id, i.account_id::bigint AS account_id,
                    i.lock_to_single_conversation, COALESCE(ct.medium, 0)::int AS medium, ct.auth_token
             FROM channel_twilio_sms ct
             JOIN inboxes i ON i.channel_id = ct.id AND i.channel_type = 'Channel::TwilioSms'
             WHERE ct.messaging_service_sid = $1
             LIMIT 1",
        )
        .bind(msid)
        .fetch_optional(db)
        .await?;
        if let Some(r) = row {
            return Ok(Some(TwilioChannelRow {
                inbox_id: r.try_get("inbox_id")?,
                account_id: r.try_get("account_id")?,
                lock_to_single_conversation: r.try_get("lock_to_single_conversation")?,
                medium: r.try_get("medium")?,
                auth_token: ar_encryption::decrypt(&r.try_get::<String, _>("auth_token")?),
            }));
        }
    }

    let (Some(acct_sid), Some(num)) = (
        account_sid.filter(|s| !s.is_empty()),
        number.filter(|s| !s.is_empty()),
    ) else {
        return Ok(None);
    };

    let stripped = num.strip_prefix("whatsapp:").unwrap_or(num);
    let with_wa = format!("whatsapp:{stripped}");

    let row = sqlx::query(
        "SELECT i.id::bigint AS inbox_id, i.account_id::bigint AS account_id,
                i.lock_to_single_conversation, COALESCE(ct.medium, 0)::int AS medium, ct.auth_token
         FROM channel_twilio_sms ct
         JOIN inboxes i ON i.channel_id = ct.id AND i.channel_type = 'Channel::TwilioSms'
         WHERE ct.account_sid = $1 AND (ct.phone_number = $2 OR ct.phone_number = $3 OR ct.phone_number = $4)
         LIMIT 1",
    )
    .bind(acct_sid)
    .bind(num)
    .bind(stripped)
    .bind(&with_wa)
    .fetch_optional(db)
    .await?;

    match row {
        Some(r) => Ok(Some(TwilioChannelRow {
            inbox_id: r.try_get("inbox_id")?,
            account_id: r.try_get("account_id")?,
            lock_to_single_conversation: r.try_get("lock_to_single_conversation")?,
            medium: r.try_get("medium")?,
            auth_token: ar_encryption::decrypt(&r.try_get::<String, _>("auth_token")?),
        })),
        None => Ok(None),
    }
}

async fn find_channel_by_message_sid(
    db: &PgPool,
    message_sid: Option<&str>,
) -> Result<Option<TwilioChannelRow>, ApiError> {
    let Some(msid) = message_sid.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT i.id::bigint AS inbox_id, i.account_id::bigint AS account_id,
                i.lock_to_single_conversation, COALESCE(ct.medium, 0)::int AS medium, ct.auth_token
         FROM messages m
         JOIN inboxes i ON i.id = m.inbox_id AND i.channel_type = 'Channel::TwilioSms'
         JOIN channel_twilio_sms ct ON ct.id = i.channel_id
         WHERE m.source_id = $1
         LIMIT 1",
    )
    .bind(msid)
    .fetch_optional(db)
    .await?;

    match row {
        Some(r) => Ok(Some(TwilioChannelRow {
            inbox_id: r.try_get("inbox_id")?,
            account_id: r.try_get("account_id")?,
            lock_to_single_conversation: r.try_get("lock_to_single_conversation")?,
            medium: r.try_get("medium")?,
            auth_token: ar_encryption::decrypt(&r.try_get::<String, _>("auth_token")?),
        })),
        None => Ok(None),
    }
}

/// POST /twilio/callback
async fn callback(
    State(st): State<AppState>,
    headers: HeaderMap,
    uri: OriginalUri,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let params = parse_form(&body);
    let channel = find_channel_for_number(
        &st.db,
        params.get("MessagingServiceSid").map(String::as_str),
        params.get("AccountSid").map(String::as_str),
        params.get("To").map(String::as_str),
    )
    .await?;

    let Some(channel) = channel else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Channel not found" }))).into_response());
    };

    let url = request_url(&headers, &uri);
    let sig = headers.get("x-twilio-signature").and_then(|v| v.to_str().ok());
    if !verify_twilio_signature(&channel.auth_token, &url, &params, sig) {
        return Ok((StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid signature" }))).into_response());
    }

    if let Err(e) = process_incoming(&st, &channel, &params).await {
        tracing::warn!(inbox_id = channel.inbox_id, error = ?e, "twilio inbound processing failed");
    }

    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn process_incoming(
    st: &AppState,
    channel: &TwilioChannelRow,
    params: &BTreeMap<String, String>,
) -> Result<(), ApiError> {
    let from = params
        .get("From")
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("From is required"))?;

    let message_sid = params
        .get("MessageSid")
        .or_else(|| params.get("SmsSid"))
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("MessageSid is required"))?;

    let body = params
        .get("Body")
        .map(|s| s.replace('\0', ""))
        .unwrap_or_default();

    let dup: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM messages WHERE inbox_id = $1 AND source_id = $2 LIMIT 1",
    )
    .bind(channel.inbox_id as i32)
    .bind(message_sid)
    .fetch_optional(&st.db)
    .await?;
    if dup.is_some() {
        return Ok(());
    }

    let phone = if channel.medium == 1 {
        from.strip_prefix("whatsapp:").unwrap_or(from)
    } else {
        from
    };

    let name = params
        .get("ProfileName")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(phone);

    let (contact_id, contact_inbox_id) = find_or_create_contact(
        &st.db,
        channel.account_id,
        channel.inbox_id,
        from,
        phone,
        name,
    )
    .await?;

    let conv = find_or_create_conversation(
        &st.db,
        channel.account_id,
        channel.inbox_id,
        contact_id,
        contact_inbox_id,
        channel.lock_to_single_conversation,
    )
    .await?;

    insert_message(
        st,
        &conv,
        Some(("Contact", contact_id)),
        &body,
        0, // incoming
        false,
        0, // text
        json!({ "__pulsar_source_id": message_sid }),
        None,
    )
    .await?;

    Ok(())
}

async fn find_or_create_contact(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    source_id: &str,
    phone: &str,
    name: &str,
) -> Result<(i64, i64), ApiError> {
    if let Some(row) = sqlx::query(
        "SELECT id::bigint AS ci_id, contact_id::bigint AS contact_id
         FROM contact_inboxes
         WHERE inbox_id = $1 AND source_id = $2
         LIMIT 1",
    )
    .bind(inbox_id as i32)
    .bind(source_id)
    .fetch_optional(db)
    .await?
    {
        return Ok((row.try_get("contact_id")?, row.try_get("ci_id")?));
    }

    let existing_contact: Option<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM contacts
         WHERE account_id = $1 AND phone_number = $2
         LIMIT 1",
    )
    .bind(account_id as i32)
    .bind(phone)
    .fetch_optional(db)
    .await?;

    let contact_id = match existing_contact {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO contacts (name, phone_number, account_id, created_at, updated_at, last_activity_at)
                 VALUES ($1, $2, $3, now(), now(), now()) RETURNING id::bigint",
            )
            .bind(name)
            .bind(phone)
            .bind(account_id as i32)
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
    .bind(inbox_id as i32)
    .bind(source_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(db)
    .await?;

    let row = sqlx::query(
        "SELECT id::bigint AS ci_id, contact_id::bigint AS contact_id
         FROM contact_inboxes
         WHERE inbox_id = $1 AND source_id = $2
         LIMIT 1",
    )
    .bind(inbox_id as i32)
    .bind(source_id)
    .fetch_one(db)
    .await?;

    Ok((row.try_get("contact_id")?, row.try_get("ci_id")?))
}

async fn find_or_create_conversation(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    contact_id: i64,
    contact_inbox_id: i64,
    lock_to_single: bool,
) -> Result<crate::conversations::Conv, ApiError> {
    let existing: Option<i64> = if lock_to_single {
        sqlx::query_scalar(
            "SELECT id::bigint FROM conversations
             WHERE contact_inbox_id = $1 AND account_id = $2
             ORDER BY id DESC LIMIT 1",
        )
        .bind(contact_inbox_id)
        .bind(account_id as i32)
        .fetch_optional(db)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT id::bigint FROM conversations
             WHERE contact_inbox_id = $1 AND account_id = $2 AND status <> 1
             ORDER BY id DESC LIMIT 1",
        )
        .bind(contact_inbox_id)
        .bind(account_id as i32)
        .fetch_optional(db)
        .await?
    };

    let conv_id = match existing {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO conversations (account_id, inbox_id, status, contact_id, contact_inbox_id,
                                            additional_attributes, custom_attributes, last_activity_at, created_at, updated_at)
                 VALUES ($1, $2, 0, $3, $4, '{}', '{}', now(), now(), now()) RETURNING id::bigint",
            )
            .bind(account_id as i32)
            .bind(inbox_id as i32)
            .bind(contact_id)
            .bind(contact_inbox_id)
            .fetch_one(db)
            .await?
        }
    };

    reload(db, conv_id).await
}

/// POST /twilio/delivery_status
async fn delivery_status(
    State(st): State<AppState>,
    headers: HeaderMap,
    uri: OriginalUri,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let params = parse_form(&body);
    let mut channel = find_channel_for_number(
        &st.db,
        params.get("MessagingServiceSid").map(String::as_str),
        params.get("AccountSid").map(String::as_str),
        params.get("From").map(String::as_str),
    )
    .await?;

    if channel.is_none() {
        channel = find_channel_by_message_sid(
            &st.db,
            params.get("MessageSid").map(String::as_str),
        )
        .await?;
    }

    let Some(channel) = channel else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Channel not found" }))).into_response());
    };

    let url = request_url(&headers, &uri);
    let sig = headers.get("x-twilio-signature").and_then(|v| v.to_str().ok());
    if !verify_twilio_signature(&channel.auth_token, &url, &params, sig) {
        return Ok((StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid signature" }))).into_response());
    }

    let Some(message_sid) = params.get("MessageSid").filter(|s| !s.is_empty()) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let Some(raw_status) = params.get("MessageStatus").map(String::as_str) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };

    let status_int: Option<i32> = match raw_status {
        "sent" => Some(0),
        "delivered" => Some(1),
        "read" => Some(2),
        "failed" | "undelivered" => Some(3),
        _ => None,
    };

    let Some(status_int) = status_int else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };

    if status_int == 3 {
        let error_code = params.get("ErrorCode").map(String::as_str).unwrap_or("");
        let error_msg = params.get("ErrorMessage").map(String::as_str).unwrap_or("");
        let external_error = match (error_code.is_empty(), error_msg.is_empty()) {
            (false, false) => Some(format!("{error_code} - {error_msg}")),
            (false, true) => Some(format!("Error code: {error_code}")),
            (true, false) => Some(error_msg.to_string()),
            (true, true) => None,
        };
        if let Some(err) = external_error {
            sqlx::query(
                "UPDATE messages
                 SET status = $1,
                     content_attributes = COALESCE(content_attributes, '{}'::json)::jsonb || jsonb_build_object('external_error', $2::text),
                     updated_at = now()
                 WHERE inbox_id = $3 AND source_id = $4",
            )
            .bind(status_int)
            .bind(&err)
            .bind(channel.inbox_id as i32)
            .bind(message_sid)
            .execute(&st.db)
            .await?;
            return Ok(StatusCode::NO_CONTENT.into_response());
        }
    }

    sqlx::query(
        "UPDATE messages SET status = $1, updated_at = now()
         WHERE inbox_id = $2 AND source_id = $3",
    )
    .bind(status_int)
    .bind(channel.inbox_id as i32)
    .bind(message_sid)
    .execute(&st.db)
    .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_twilio_hmac_sha1_signature() {
        let mut params = BTreeMap::new();
        params.insert("To".into(), "+15558675309".into());
        params.insert("From".into(), "+15551234567".into());
        params.insert("Body".into(), "Hello".into());
        let url = "https://example.com/twilio/callback";
        let token = "test_auth_token_123";

        let mut mac = Hmac::<Sha1>::new_from_slice(token.as_bytes()).unwrap();
        mac.update(format!("{url}BodyHelloFrom+15551234567To+15558675309").as_bytes());
        let sig = STANDARD.encode(mac.finalize().into_bytes());

        assert!(verify_twilio_signature(token, url, &params, Some(&sig)));
        assert!(!verify_twilio_signature(token, url, &params, Some("bad")));
        assert!(!verify_twilio_signature(token, url, &params, None));
    }
}
