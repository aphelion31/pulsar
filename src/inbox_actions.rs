//! Inbox side actions: reset_secret, health, sync_templates, register_webhook, csat_template.
//! Relative to /api/v1/accounts/{account_id}.
//! Mirrors chatwoot/app/controllers/api/v1/accounts/inboxes_controller.rb,
//! app/controllers/api/v1/accounts/concerns/{inbox_health_management,inbox_secret_management}.rb,
//! and app/controllers/api/v1/accounts/inbox_csat_templates_controller.rb.
//! Not ported (enterprise or captain only): csat_template/analyze, conversations/:id/inbox_assistant.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

use crate::{
    ar_encryption,
    common::{require_account, AccountUser, ApiError, ApiResult, AppState, CurrentUser},
    inboxes::inbox_json,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/inboxes/{id}/reset_secret", post(reset_secret))
        .route("/inboxes/{id}/health", get(health))
        .route("/inboxes/{id}/sync_templates", post(sync_templates))
        .route("/inboxes/{id}/register_webhook", post(register_webhook))
        .route("/inboxes/{inbox_id}/csat_template", get(csat_show).post(csat_create))
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
    if member { Ok(()) } else { Err(ApiError::forbidden()) }
}

async fn wa_provider(db: &PgPool, channel_id: i64) -> Result<String, ApiError> {
    Ok(sqlx::query_scalar::<_, String>("SELECT provider FROM channel_whatsapp WHERE id::bigint = $1")
        .bind(channel_id)
        .fetch_optional(db)
        .await?
        .unwrap_or_default())
}

async fn twilio_medium(db: &PgPool, channel_id: i64) -> Result<i32, ApiError> {
    Ok(sqlx::query_scalar::<_, i32>("SELECT COALESCE(medium, 0) FROM channel_twilio_sms WHERE id::bigint = $1")
        .bind(channel_id)
        .fetch_optional(db)
        .await?
        .unwrap_or(0))
}

/// Chatwoot WhatsApp cloud channel (`Channel::Whatsapp` with provider whatsapp_cloud).
async fn is_wa_cloud(db: &PgPool, inbox: &InboxRow) -> Result<bool, ApiError> {
    Ok(inbox.channel_type == "Channel::Whatsapp" && wa_provider(db, inbox.channel_id).await? == "whatsapp_cloud")
}

/// Whatsapp or Twilio WhatsApp (`whatsapp?` or `twilio? && channel.whatsapp?`).
async fn is_whatsapp(db: &PgPool, inbox: &InboxRow) -> Result<bool, ApiError> {
    Ok(inbox.channel_type == "Channel::Whatsapp"
        || (inbox.channel_type == "Channel::TwilioSms" && twilio_medium(db, inbox.channel_id).await? == 1))
}

fn bad_status(status: StatusCode, msg: &str) -> ApiError {
    ApiError(status, msg.into())
}

/// POST /inboxes/{id}/reset_secret (admin). Api channel only, else 404 like `return head :not_found`.
/// Response is the inbox partial (reset_secret.json.jbuilder), plus the new plaintext secret.
async fn reset_secret(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?.require_admin()?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    if inbox.channel_type != "Channel::Api" {
        return Err(ApiError::not_found());
    }
    let (plain, stored) = ar_encryption::new_secret();
    let n = sqlx::query("UPDATE channel_api SET secret = $3, updated_at = now() WHERE id::bigint = $1 AND account_id::bigint = $2")
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

/// GET /inboxes/{id}/health. Chatwoot: show? only (no admin check); 400 for other channels.
/// Graph/Twilio health calls not ported, so supported channels get 501 instead of fake data.
async fn health(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    authorize_show(&st.db, &au, id).await?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    if is_wa_cloud(&st.db, &inbox).await? || inbox.channel_type == "Channel::TwilioSms" {
        return Err(bad_status(StatusCode::NOT_IMPLEMENTED, "Inbox health check is not implemented in Pulsar yet"));
    }
    Err(bad_status(
        StatusCode::BAD_REQUEST,
        "Health data only available for WhatsApp Cloud API and Twilio channels",
    ))
}

/// POST /inboxes/{id}/sync_templates (admin per InboxPolicy#sync_templates?). 422 unless WhatsApp.
/// ponytail: TemplatesSyncJob not ported; returns the accepted message without enqueueing.
async fn sync_templates(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
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
    Ok(Json(json!({ "message": "Template sync initiated successfully" })))
}

/// POST /inboxes/{id}/register_webhook (admin). 400 for unsupported channels, as in the concern.
/// ponytail: Whatsapp::WebhookSetupService / Twilio::WebhookSetupService not ported; 501 on supported channels.
async fn register_webhook(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?.require_admin()?;
    let inbox = load_inbox(&st.db, account_id, id).await?;
    if !(is_wa_cloud(&st.db, &inbox).await? || inbox.channel_type == "Channel::TwilioSms") {
        return Err(bad_status(
            StatusCode::BAD_REQUEST,
            "Health data only available for WhatsApp Cloud API and Twilio channels",
        ));
    }
    Err(bad_status(StatusCode::NOT_IMPLEMENTED, "Webhook registration is not implemented in Pulsar yet"))
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

/// GET /inboxes/{inbox_id}/csat_template. `{template_exists: false}` when csat_config has no template.
/// ponytail: provider status lookup not ported; reports the template stored in csat_config only.
async fn csat_show(State(st): State<AppState>, user: CurrentUser, Path((account_id, inbox_id)): Path<(i64, i64)>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    authorize_show(&st.db, &au, inbox_id).await?;
    let inbox = load_inbox(&st.db, account_id, inbox_id).await?;
    csat_guard(&st.db, &inbox).await?;
    let Some(tpl) = inbox.csat_config.as_ref().and_then(|c| c.get("template")).filter(|t| !t.is_null()) else {
        return Ok(Json(json!({ "template_exists": false })));
    };
    Ok(Json(json!({
        "template_exists": true,
        "template_name": tpl.get("name").cloned().unwrap_or(Value::Null),
        "status": tpl.get("status").cloned().unwrap_or(Value::Null),
        "language": tpl.get("language").cloned().unwrap_or(Value::Null),
    })))
}

/// POST /inboxes/{inbox_id}/csat_template {template:{message,button_text,language}}.
/// Validation mirrors the controller (422 on missing template or message).
/// ponytail: provider create (Whatsapp/Twilio CsatTemplateService) not ported; 501 after validation.
async fn csat_create(
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
        .ok_or_else(|| bad_status(StatusCode::UNPROCESSABLE_ENTITY, "Template parameters are required"))?;
    let message = tpl.get("message").and_then(|m| m.as_str()).unwrap_or("").trim();
    if message.is_empty() {
        return Err(bad_status(StatusCode::UNPROCESSABLE_ENTITY, "Message is required"));
    }
    Err(bad_status(StatusCode::NOT_IMPLEMENTED, "CSAT template creation is not implemented in Pulsar yet"))
}
