//! Inbox side actions: reset_secret, rotate_hmac_token, health, sync_templates, message_templates,
//! register_webhook, whatsapp_business_management_token, and csat_template.
//! Relative to /api/v1/accounts/{account_id}.
//! Mirrors chatwoot/app/controllers/api/v1/accounts/inboxes_controller.rb,
//! app/controllers/api/v1/accounts/concerns/{inbox_health_management,inbox_secret_management}.rb,
//! and app/controllers/api/v1/accounts/inbox_csat_templates_controller.rb.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

use crate::{
    ar_encryption,
    common::{AccountUser, ApiError, ApiResult, AppState, CurrentUser, require_account},
    inboxes::inbox_json,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/inboxes/{id}/reset_secret", post(reset_secret))
        .route("/inboxes/{id}/rotate_hmac_token", post(rotate_hmac_token))
        .route("/inboxes/{id}/health", get(health))
        .route("/inboxes/{id}/sync_templates", post(sync_templates))
        .route("/inboxes/{id}/message_templates", get(message_templates))
        .route("/inboxes/{id}/register_webhook", post(register_webhook))
        .route(
            "/inboxes/{id}/whatsapp_business_management_token",
            put(whatsapp_business_management_token),
        )
        .route(
            "/inboxes/{inbox_id}/csat_template",
            get(csat_show).post(csat_create),
        )
        .route(
            "/inboxes/{inbox_id}/csat_template/analyze",
            post(csat_analyze),
        )
}

struct InboxRow {
    id: i64,
    channel_type: String,
    channel_id: i64,
    csat_config: Option<Value>,
}

async fn load_inbox(db: &PgPool, account_id: i64, inbox_id: i64) -> Result<InboxRow, ApiError> {
    let r = sqlx::query(
        "SELECT id::bigint AS id, channel_type, channel_id::bigint AS channel_id, csat_config
         FROM inboxes WHERE id::bigint = $1 AND account_id::bigint = $2",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(InboxRow {
        id: r.try_get("id")?,
        channel_type: r.try_get("channel_type")?,
        channel_id: r.try_get("channel_id")?,
        csat_config: r.try_get("csat_config")?,
    })
}

/// Chatwoot `authorize @inbox, :show?`: admins see every inbox, agents only their memberships.
async fn authorize_show(db: &PgPool, au: &AccountUser, inbox_id: i64) -> Result<(), ApiError> {
    if au.is_admin() {
        return Ok(());
    }
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inbox_members WHERE user_id::bigint = $1 AND inbox_id::bigint = $2)",
    )
    .bind(au.user_id)
    .bind(inbox_id)
    .fetch_one(db)
    .await?;
    if member {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}

async fn wa_provider(db: &PgPool, channel_id: i64) -> Result<String, ApiError> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT provider FROM channel_whatsapp WHERE id::bigint = $1",
    )
    .bind(channel_id)
    .fetch_optional(db)
    .await?
    .unwrap_or_default())
}

async fn twilio_medium(db: &PgPool, channel_id: i64) -> Result<i32, ApiError> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT COALESCE(medium, 0) FROM channel_twilio_sms WHERE id::bigint = $1",
    )
    .bind(channel_id)
    .fetch_optional(db)
    .await?
    .unwrap_or(0))
}

async fn is_wa_cloud(db: &PgPool, inbox: &InboxRow) -> Result<bool, ApiError> {
    Ok(inbox.channel_type == "Channel::Whatsapp"
        && wa_provider(db, inbox.channel_id).await? == "whatsapp_cloud")
}

async fn is_whatsapp(db: &PgPool, inbox: &InboxRow) -> Result<bool, ApiError> {
    Ok(inbox.channel_type == "Channel::Whatsapp"
        || (inbox.channel_type == "Channel::TwilioSms"
            && twilio_medium(db, inbox.channel_id).await? == 1))
}

fn bad_status(status: StatusCode, msg: &str) -> ApiError {
    ApiError(status, msg.into())
}

fn wa_base() -> String {
    std::env::var("WHATSAPP_CLOUD_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://graph.facebook.com".into())
        .trim_end_matches('/')
        .to_string()
}

async fn read_json(resp: reqwest::Response) -> Value {
    let Ok(bytes) = resp.bytes().await else {
        return Value::Null;
    };
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn form_body(pairs: &[(&str, &str)]) -> String {
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        ser.append_pair(k, v);
    }
    ser.finish()
}

/// POST /inboxes/{id}/reset_secret (admin). Api channel only, else 404.
async fn reset_secret(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    if inbox.channel_type != "Channel::Api" {
        return Err(ApiError::not_found());
    }
    let (plain, stored) = ar_encryption::new_secret();
    let n = sqlx::query(
        "UPDATE channel_api SET secret = $3, updated_at = now() WHERE id::bigint = $1 AND account_id::bigint = $2",
    )
    .bind(inbox.channel_id)
    .bind(account_id)
    .bind(&stored)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    let mut out = inbox_json(&st.db, account_id, inbox.id, true).await?;
    out["secret"] = Value::String(plain);
    Ok(Json(out))
}

/// POST /inboxes/{id}/rotate_hmac_token (admin). WebWidget or Api channel only, else 404.
async fn rotate_hmac_token(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    let new_tok = uuid::Uuid::new_v4().simple().to_string();
    let table = match inbox.channel_type.as_str() {
        "Channel::WebWidget" => "channel_web_widgets",
        "Channel::Api" => "channel_api",
        _ => return Err(ApiError::not_found()),
    };
    let n = sqlx::query(&format!(
        "UPDATE {table} SET hmac_token = $3, updated_at = now() WHERE id::bigint = $1 AND account_id::bigint = $2"
    ))
    .bind(inbox.channel_id)
    .bind(account_id)
    .bind(&new_tok)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(inbox_json(&st.db, account_id, inbox.id, true).await?))
}

async fn load_twilio_creds(
    db: &PgPool,
    channel_id: i64,
) -> Result<(String, String, Option<String>, Option<String>, i32), ApiError> {
    let r = sqlx::query(
        "SELECT account_sid, auth_token, phone_number, messaging_service_sid, COALESCE(medium, 0) AS medium
         FROM channel_twilio_sms WHERE id::bigint = $1",
    )
    .bind(channel_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let sid: String = r.try_get("account_sid")?;
    let tok_raw: String = r.try_get("auth_token")?;
    let tok = ar_encryption::decrypt(&tok_raw);
    let phone: Option<String> = r.try_get("phone_number")?;
    let msid: Option<String> = r.try_get("messaging_service_sid")?;
    let medium: i32 = r.try_get("medium")?;
    Ok((sid, tok, phone, msid, medium))
}

/// GET /inboxes/{id}/health. Chatwoot: show? only (no admin check); 400 for other channels.
async fn health(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    authorize_show(&st.db, &au, id).await?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    if is_wa_cloud(&st.db, &inbox).await? {
        return Ok(Json(
            crate::channels::whatsapp::health(&st.db, inbox.channel_id).await?,
        ));
    }
    if inbox.channel_type == "Channel::TwilioSms" {
        let (sid, tok, phone, msid, medium) = load_twilio_creds(&st.db, inbox.channel_id).await?;
        let base = crate::channels::twilio::api_base();
        let client = reqwest::Client::new();
        let acc_resp = client
            .get(format!("{base}/2010-04-01/Accounts/{sid}.json"))
            .basic_auth(&sid, Some(&tok))
            .send()
            .await
            .map_err(|_| bad_status(StatusCode::UNPROCESSABLE_ENTITY, "Twilio API unavailable"))?;
        let account_json = if acc_resp.status().is_success() {
            let v = read_json(acc_resp).await;
            Some(json!({
                "sid": v["sid"],
                "friendly_name": v["friendly_name"],
                "status": v["status"],
                "type": v["type"],
            }))
        } else if matches!(acc_resp.status().as_u16(), 401 | 403) {
            None
        } else {
            return Err(bad_status(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Twilio health check failed",
            ));
        };

        let expected_cb = format!("{}/twilio/callback", st.frontend_url.trim_end_matches('/'));
        let (sender_json, configured_url) = if let Some(ref m) = msid {
            let r = client
                .get(format!("https://messaging.twilio.com/v1/Services/{m}"))
                .basic_auth(&sid, Some(&tok))
                .send()
                .await
                .ok();
            let v = match r {
                Some(res) if res.status().is_success() => read_json(res).await,
                _ => Value::Null,
            };
            (
                json!({ "type": "messaging_service", "sid": m, "label": v["friendly_name"].as_str().unwrap_or(m) }),
                v["inbound_request_url"].as_str().map(str::to_string),
            )
        } else {
            let p = phone.as_deref().unwrap_or("");
            let r = client
                .get(format!(
                    "{base}/2010-04-01/Accounts/{sid}/IncomingPhoneNumbers.json"
                ))
                .query(&[("PhoneNumber", p)])
                .basic_auth(&sid, Some(&tok))
                .send()
                .await
                .map_err(|_| {
                    bad_status(StatusCode::UNPROCESSABLE_ENTITY, "Twilio API unavailable")
                })?;
            let v = read_json(r).await;
            let num = v["incoming_phone_numbers"]
                .as_array()
                .and_then(|a| a.first())
                .cloned()
                .unwrap_or(Value::Null);
            let sms_url = num["sms_url"].as_str().map(str::to_string);
            (
                json!({
                    "type": if medium == 1 { "whatsapp_sender" } else { "phone_number" },
                    "sid": num["sid"],
                    "label": p,
                    "friendly_name": num["friendly_name"],
                    "capabilities": num["capabilities"],
                }),
                sms_url,
            )
        };
        let configured = configured_url.as_deref() == Some(expected_cb.as_str());
        let account_usable = account_json
            .as_ref()
            .and_then(|a| a["status"].as_str())
            .is_none_or(|s| s.eq_ignore_ascii_case("active"));
        return Ok(Json(json!({
            "status": if configured && account_usable { "healthy" } else { "misconfigured" },
            "account": account_json,
            "sender": sender_json,
            "voice_enabled": false,
            "medium": if medium == 1 { "whatsapp" } else { "sms" },
            "webhooks": [{
                "type": "messaging",
                "expected_url": expected_cb,
                "configured_url": configured_url,
                "configured": configured,
            }],
        })));
    }
    Err(bad_status(
        StatusCode::BAD_REQUEST,
        "Health data only available for WhatsApp Cloud API and Twilio channels",
    ))
}

/// POST /inboxes/{id}/sync_templates (admin). 422 unless WhatsApp.
async fn sync_templates(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    authorize_show(&st.db, &au, id).await?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    if !is_whatsapp(&st.db, &inbox).await? {
        return Err(bad_status(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Template sync is only available for WhatsApp channels",
        ));
    }
    if is_wa_cloud(&st.db, &inbox).await? {
        crate::channels::whatsapp::sync_templates(&st.db, inbox.channel_id).await?;
    }
    Ok(Json(
        json!({ "message": "Template sync initiated successfully" }),
    ))
}

/// GET /inboxes/{id}/message_templates
async fn message_templates(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    authorize_show(&st.db, &au, id).await?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    if !is_whatsapp(&st.db, &inbox).await? {
        return Err(bad_status(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Message templates are only available for WhatsApp channels",
        ));
    }
    let name_filter = q.get("name").map(|s| s.trim()).filter(|s| !s.is_empty());
    let (mut templates, last_sync, name_key) = if inbox.channel_type == "Channel::Whatsapp" {
        let r: (Option<Value>, Option<chrono::NaiveDateTime>) = sqlx::query_as(
            "SELECT message_templates, message_templates_last_updated FROM channel_whatsapp WHERE id::bigint = $1",
        )
        .bind(inbox.channel_id)
        .fetch_one(&st.db)
        .await?;
        let arr = r.0.and_then(|v| v.as_array().cloned()).unwrap_or_default();
        (arr, r.1.map(|dt| crate::common::ts(Some(dt))), "name")
    } else {
        let r: (Option<Value>, Option<chrono::NaiveDateTime>) = sqlx::query_as(
            "SELECT content_templates, content_templates_last_updated FROM channel_twilio_sms WHERE id::bigint = $1",
        )
        .bind(inbox.channel_id)
        .fetch_one(&st.db)
        .await?;
        let arr =
            r.0.and_then(|v| v.get("templates").and_then(|t| t.as_array()).cloned())
                .unwrap_or_default();
        (
            arr,
            r.1.map(|dt| crate::common::ts(Some(dt))),
            "friendly_name",
        )
    };

    if let Some(nf) = name_filter {
        templates.retain(|t| t.get(name_key).and_then(Value::as_str) == Some(nf));
    }

    Ok(Json(json!({
        "payload": templates,
        "meta": { "last_sync_attempt_at": last_sync },
    })))
}

/// PUT /inboxes/{id}/whatsapp_business_management_token (admin)
async fn whatsapp_business_management_token(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult<StatusCode> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    if inbox.channel_type != "Channel::Whatsapp" {
        return Err(ApiError::not_found());
    }
    let token = body["business_management_token"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            bad_status(
                StatusCode::UNPROCESSABLE_ENTITY,
                "business_management_token is required",
            )
        })?;
    sqlx::query(
        "UPDATE channel_whatsapp
         SET provider_config = COALESCE(provider_config, '{}'::jsonb) || jsonb_build_object('business_management_token', $2::text),
             updated_at = now()
         WHERE id::bigint = $1",
    )
    .bind(inbox.channel_id)
    .bind(token)
    .execute(&st.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /inboxes/{id}/register_webhook (admin).
async fn register_webhook(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    if is_wa_cloud(&st.db, &inbox).await? {
        let (phone, cfg): (String, Value) = sqlx::query_as(
            "SELECT phone_number, provider_config FROM channel_whatsapp WHERE id::bigint = $1",
        )
        .bind(inbox.channel_id)
        .fetch_one(&st.db)
        .await?;
        let waba_id = cfg["business_account_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                bad_status(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "business_account_id is missing",
                )
            })?;
        let api_key = cfg["api_key"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| bad_status(StatusCode::UNPROCESSABLE_ENTITY, "api_key is missing"))?;
        let verify_token = cfg["webhook_verify_token"].as_str().unwrap_or("");
        let callback_uri = format!(
            "{}/webhooks/whatsapp/{phone}",
            st.frontend_url.trim_end_matches('/')
        );
        let payload = serde_json::to_vec(&json!({
            "override_callback_uri": callback_uri,
            "verify_token": verify_token,
        }))
        .map_err(|e| ApiError::bad(e.to_string()))?;
        let resp = reqwest::Client::new()
            .post(format!("{}/v24.0/{waba_id}/subscribed_apps", wa_base()))
            .bearer_auth(api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|e| {
                bad_status(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    &format!("Webhook registration failed: {e}"),
                )
            })?;
        if !resp.status().is_success() {
            return Err(bad_status(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Webhook registration failed",
            ));
        }
        return Ok(Json(
            json!({ "message": "Webhook registered successfully" }),
        ));
    }
    if inbox.channel_type == "Channel::TwilioSms" {
        let (sid, tok, phone, msid, _medium) = load_twilio_creds(&st.db, inbox.channel_id).await?;
        let base = crate::channels::twilio::api_base();
        let client = reqwest::Client::new();
        let cb_url = format!("{}/twilio/callback", st.frontend_url.trim_end_matches('/'));
        if let Some(m) = msid {
            let resp = client
                .post(format!("https://messaging.twilio.com/v1/Services/{m}"))
                .basic_auth(&sid, Some(&tok))
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(form_body(&[
                    ("InboundRequestUrl", cb_url.as_str()),
                    ("InboundMethod", "POST"),
                ]))
                .send()
                .await
                .map_err(|e| bad_status(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()))?;
            if !resp.status().is_success() {
                return Err(bad_status(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Twilio webhook registration failed",
                ));
            }
        } else if let Some(p) = phone {
            let list = client
                .get(format!(
                    "{base}/2010-04-01/Accounts/{sid}/IncomingPhoneNumbers.json"
                ))
                .query(&[("PhoneNumber", p.as_str())])
                .basic_auth(&sid, Some(&tok))
                .send()
                .await
                .map_err(|e| bad_status(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()))?;
            let v = read_json(list).await;
            let num_sid = v["incoming_phone_numbers"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|n| n["sid"].as_str())
                .ok_or_else(|| {
                    bad_status(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Twilio phone number not found",
                    )
                })?;
            let resp = client
                .post(format!(
                    "{base}/2010-04-01/Accounts/{sid}/IncomingPhoneNumbers/{num_sid}.json"
                ))
                .basic_auth(&sid, Some(&tok))
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(form_body(&[
                    ("SmsUrl", cb_url.as_str()),
                    ("SmsMethod", "POST"),
                ]))
                .send()
                .await
                .map_err(|e| bad_status(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()))?;
            if !resp.status().is_success() {
                return Err(bad_status(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Twilio webhook registration failed",
                ));
            }
        }
        return Ok(Json(
            json!({ "message": "Webhook registered successfully" }),
        ));
    }
    Err(bad_status(
        StatusCode::BAD_REQUEST,
        "Health data only available for WhatsApp Cloud API and Twilio channels",
    ))
}

/// Chatwoot CSAT template ops: WhatsApp cloud and Twilio WhatsApp only, else 400.
async fn csat_guard(db: &PgPool, inbox: &InboxRow) -> Result<(), ApiError> {
    if is_whatsapp(db, inbox).await? {
        return Ok(());
    }
    Err(bad_status(
        StatusCode::BAD_REQUEST,
        "CSAT template operations only available for WhatsApp and Twilio WhatsApp channels",
    ))
}

/// GET /inboxes/{inbox_id}/csat_template
async fn csat_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    authorize_show(&st.db, &au, inbox_id).await?;
    let inbox = load_inbox(&st.db, account_id, inbox_id).await?;
    csat_guard(&st.db, &inbox).await?;
    let Some(tpl) = inbox
        .csat_config
        .as_ref()
        .and_then(|c| c.get("template"))
        .filter(|t| !t.is_null())
    else {
        return Ok(Json(json!({ "template_exists": false })));
    };
    Ok(Json(json!({
        "template_exists": true,
        "template_name": tpl.get("name").cloned().unwrap_or(Value::Null),
        "status": tpl.get("status").cloned().unwrap_or(Value::Null),
        "language": tpl.get("language").cloned().unwrap_or(Value::Null),
    })))
}

/// POST /inboxes/{inbox_id}/csat_template {template:{message,button_text,language}}
async fn csat_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let au = require_account(&st.db, &user, account_id).await?;
    authorize_show(&st.db, &au, inbox_id).await?;
    let inbox = load_inbox(&st.db, account_id, inbox_id).await?;
    csat_guard(&st.db, &inbox).await?;
    let tpl = body
        .get("template")
        .filter(|t| t.is_object())
        .ok_or_else(|| {
            bad_status(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Template parameters are required",
            )
        })?;
    let message = tpl
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .trim();
    if message.is_empty() {
        return Err(bad_status(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Message is required",
        ));
    }
    let button_text = tpl
        .get("button_text")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Please rate us");
    let language = tpl
        .get("language")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("en");
    let tpl_name = format!("customer_satisfaction_survey_{inbox_id}");
    let base_url = st.frontend_url.trim_end_matches('/');

    if is_wa_cloud(&st.db, &inbox).await? {
        let cfg: Value = sqlx::query_scalar(
            "SELECT provider_config FROM channel_whatsapp WHERE id::bigint = $1",
        )
        .bind(inbox.channel_id)
        .fetch_one(&st.db)
        .await?;
        let waba_id = cfg["business_account_id"].as_str().unwrap_or("");
        let api_key = cfg["api_key"].as_str().unwrap_or("");
        let req_body = json!({
            "name": tpl_name,
            "language": language,
            "category": "UTILITY",
            "components": [
                { "type": "BODY", "text": message },
                {
                    "type": "BUTTONS",
                    "buttons": [{
                        "type": "URL",
                        "text": button_text,
                        "url": format!("{base_url}/survey/responses/{{{{1}}}}"),
                        "example": ["12345"]
                    }]
                }
            ]
        });
        let req_bytes = serde_json::to_vec(&req_body).map_err(|e| ApiError::bad(e.to_string()))?;
        let resp = reqwest::Client::new()
            .post(format!("{}/v14.0/{waba_id}/message_templates", wa_base()))
            .bearer_auth(api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(req_bytes)
            .send()
            .await
            .map_err(|e| bad_status(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
        if !resp.status().is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            return Err(bad_status(
                StatusCode::UNPROCESSABLE_ENTITY,
                &format!("Template creation failed: {err_body}"),
            ));
        }
        let created = read_json(resp).await;
        let template_id = created["id"].clone();
        let saved_name = created["name"].as_str().unwrap_or(&tpl_name).to_string();
        sqlx::query(
            "UPDATE inboxes
             SET csat_config = COALESCE(csat_config, '{}'::jsonb) || jsonb_build_object('template', jsonb_build_object('name', $2::text, 'template_id', $3::text, 'status', 'PENDING', 'language', $4::text)),
                 updated_at = now()
             WHERE id::bigint = $1",
        )
        .bind(inbox_id)
        .bind(&saved_name)
        .bind(template_id.as_str().unwrap_or_default())
        .bind(language)
        .execute(&st.db)
        .await?;
        return Ok((
            StatusCode::CREATED,
            Json(json!({
                "template": {
                    "name": saved_name,
                    "template_id": template_id,
                    "status": "PENDING",
                    "language": language,
                }
            })),
        ));
    }

    // Twilio WhatsApp Content API
    let (sid, tok, _phone, _msid, _medium) = load_twilio_creds(&st.db, inbox.channel_id).await?;
    let twilio_content_url = std::env::var("TWILIO_CONTENT_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://content.twilio.com".into());
    let req_body = json!({
        "friendly_name": tpl_name,
        "language": language,
        "types": {
            "twilio/call-to-action": {
                "body": message,
                "actions": [{
                    "type": "URL",
                    "title": button_text,
                    "url": format!("{base_url}/survey/responses/{{{{1}}}}"),
                }]
            }
        }
    });
    let req_bytes = serde_json::to_vec(&req_body).map_err(|e| ApiError::bad(e.to_string()))?;
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/v1/Content",
            twilio_content_url.trim_end_matches('/')
        ))
        .basic_auth(&sid, Some(&tok))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(req_bytes)
        .send()
        .await
        .map_err(|e| bad_status(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    if !resp.status().is_success() {
        return Err(bad_status(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Twilio CSAT template creation failed",
        ));
    }
    let created = read_json(resp).await;
    let content_sid = created["sid"].as_str().unwrap_or("").to_string();
    sqlx::query(
        "UPDATE inboxes
         SET csat_config = COALESCE(csat_config, '{}'::jsonb) || jsonb_build_object('template', jsonb_build_object('name', $2::text, 'content_sid', $3::text, 'status', 'pending', 'language', $4::text)),
             updated_at = now()
         WHERE id::bigint = $1",
    )
    .bind(inbox_id)
    .bind(&tpl_name)
    .bind(&content_sid)
    .bind(language)
    .execute(&st.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "template": {
                "friendly_name": tpl_name,
                "content_sid": content_sid,
                "status": "pending",
                "language": language,
            }
        })),
    ))
}

/// POST /inboxes/{inbox_id}/csat_template/analyze
async fn csat_analyze(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    authorize_show(&st.db, &au, inbox_id).await?;
    let inbox = load_inbox(&st.db, account_id, inbox_id).await?;
    csat_guard(&st.db, &inbox).await?;
    let tpl = body
        .get("template")
        .filter(|t| t.is_object())
        .ok_or_else(|| {
            bad_status(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Template parameters are required",
            )
        })?;
    let message = tpl
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if message.is_empty() {
        return Err(bad_status(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Message is required",
        ));
    }
    let lower = message.to_ascii_lowercase();
    let promotional = ["discount", "sale", "promo", "offer", "buy now", "coupon"]
        .iter()
        .any(|w| lower.contains(w));
    Ok(Json(json!({
        "classification": if promotional { "MARKETING" } else { "UTILITY" },
        "is_utility": !promotional,
        "optimized_message": message,
        "suggestions": if promotional {
            vec!["Remove promotional keywords so Meta classifies the template as UTILITY"]
        } else {
            Vec::<&str>::new()
        },
    })))
}
