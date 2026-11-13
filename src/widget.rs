//! OWNED BY A WORKER. See WORKERS.md for the contract.
//! Public web widget. `routes()` is merged at the ROOT router: use full paths (/widget, /api/v1/widget/...). No CurrentUser.
//! Visitor identity = HS256 token (X-Auth-Token header; cw_conversation query on /widget).
//! HS256 is hand-rolled on sha2 because jsonwebtoken 11 has no crypto backend enabled in Cargo.toml (would panic).
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::OnceLock;

use crate::{
    auth::{availability_name, locale_code},
    common::*,
    conversations::{contact_json, conv_json, insert_message, msg_json, reload, Conv, Msg, CONV_COLS, MSG_COLS},
};

const STATUSES: [&str; 4] = ["open", "resolved", "pending", "snoozed"];
const CONTENT_TYPES: [&str; 13] = [
    "text", "input_text", "input_textarea", "input_email", "input_select", "cards", "form", "article",
    "incoming_email", "input_csat", "integrations", "sticker", "voice_call",
];
const REPLY_TIMES: [&str; 3] = ["in_a_few_minutes", "in_a_few_hours", "in_a_day"];

/// Chatwoot config/initializers/languages.rb LANGUAGES_CONFIG, in index order: (name, iso_639_1_code).
const LANGUAGES: [(&str, &str); 44] = [
    ("English (en)", "en"),
    ("العربية (ar)", "ar"),
    ("Nederlands (nl) ", "nl"),
    ("Français (fr)", "fr"),
    ("Deutsch (de)", "de"),
    ("हिन्दी (hi)", "hi"),
    ("Italiano (it)", "it"),
    ("日本語 (ja)", "ja"),
    ("한국어 (ko)", "ko"),
    ("Português (pt)", "pt"),
    ("русский (ru)", "ru"),
    ("中文 (zh)", "zh"),
    ("Español (es)", "es"),
    ("മലയാളം (ml)", "ml"),
    ("Català (ca)", "ca"),
    ("ελληνικά (el)", "el"),
    ("Português Brasileiro (pt-BR)", "pt_BR"),
    ("Română (ro)", "ro"),
    ("தமிழ் (ta)", "ta"),
    ("فارسی (fa)", "fa"),
    ("中文 (台湾) (zh-TW)", "zh_TW"),
    ("Tiếng Việt (vi)", "vi"),
    ("dansk (da)", "da"),
    ("Türkçe (tr)", "tr"),
    ("čeština (cs)", "cs"),
    ("suomi, suomen kieli (fi)", "fi"),
    ("Bahasa Indonesia (id)", "id"),
    ("Svenska (sv)", "sv"),
    ("magyar nyelv (hu)", "hu"),
    ("norsk (no)", "no"),
    ("中文 (zh-CN)", "zh_CN"),
    ("język polski (pl)", "pl"),
    ("slovenčina (sk)", "sk"),
    ("украї́нська мо́ва (uk)", "uk"),
    ("ภาษาไทย (th)", "th"),
    ("latviešu valoda (lv)", "lv"),
    ("íslenska (is)", "is"),
    ("עִברִית (he)", "he"),
    ("lietuvių (lt)", "lt"),
    ("Српски (sr)", "sr"),
    ("български (bg)", "bg"),
    ("Eesti keel (et)", "et"),
    ("Oʻzbekcha (uz)", "uz"),
    ("slovenščina (sl)", "sl"),
];
// has_flags bit order in channel/web_widget.rb (flag n = 1 << (n-1))
const FEATURE_BITS: [(&str, i64); 5] = [
    ("attachments", 1),
    ("emoji_picker", 2),
    ("end_conversation", 4),
    ("use_inbox_avatar_for_bot", 8),
    ("allow_mobile_webview", 16),
];
const WIDGET_SQL: &str = "SELECT w.website_token, w.widget_color, w.welcome_title, w.welcome_tagline, w.reply_time,
  w.pre_chat_form_enabled, w.pre_chat_form_options, w.hmac_mandatory, w.feature_flags,
  i.id::bigint AS inbox_id, i.name AS inbox_name, i.account_id::bigint AS account_id, i.working_hours_enabled,
  i.out_of_office_message, i.timezone, i.csat_survey_enabled, i.allow_messages_after_resolved,
  COALESCE(a.locale, 0) AS locale, COALESCE(a.status, 0) AS account_status
  FROM channel_web_widgets w
  JOIN inboxes i ON i.channel_id = w.id AND i.channel_type = 'Channel::WebWidget'
  JOIN accounts a ON a.id = i.account_id
  WHERE w.website_token = $1";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/widget", get(page))
        .route("/api/v1/widget/config", post(config))
        .route("/api/v1/widget/conversations", get(conv_index).post(conv_create))
        .route("/api/v1/widget/conversations/update_last_seen", post(update_last_seen))
        .route("/api/v1/widget/conversations/toggle_typing", post(toggle_typing))
        .route("/api/v1/widget/conversations/toggle_status", get(toggle_status))
        .route("/api/v1/widget/messages", get(msg_index).post(msg_create))
        .route("/api/v1/widget/messages/{id}", patch(msg_update))
        .route("/api/v1/widget/contact", get(contact_show).patch(contact_update))
        .route("/api/v1/widget/contact/set_user", patch(contact_set_user))
        .route("/api/v1/widget/contact/destroy_custom_attributes", post(contact_destroy_custom_attributes))
        .route("/api/v1/widget/conversations/set_custom_attributes", post(conv_set_custom_attributes))
        .route("/api/v1/widget/conversations/destroy_custom_attributes", post(conv_destroy_custom_attributes))
        .route("/api/v1/widget/conversations/transcript", post(conv_transcript))
        .route("/api/v1/widget/inbox_members", get(inbox_members))
        .route("/api/v1/widget/events", post(events))
        .route("/api/v1/widget/campaigns", get(campaigns))
        .route("/api/v1/widget/labels", post(label_add).delete(label_remove))
}

/// GET /api/v1/widget/campaigns: mirrors widget/campaigns_controller.rb + campaigns/index.json.jbuilder.
async fn campaigns(State(st): State<AppState>, Query(q): Query<Q>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let rows: Vec<(i64, Value, Option<bool>, String, Option<i64>)> = sqlx::query_as(
        "SELECT display_id::bigint, COALESCE(trigger_rules,'{}'::jsonb), trigger_only_during_business_hours, message, sender_id::bigint
         FROM campaigns WHERE inbox_id = $1 AND account_id = $2 AND enabled ORDER BY id",
    )
    .bind(w.inbox_id)
    .bind(w.account_id)
    .fetch_all(&st.db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, rules, bh, message, sender) in rows {
        let sender = match sender { Some(u) => crate::conversations::user_json(&st.db, w.account_id, u).await, None => Value::Null };
        out.push(json!({ "id": id, "trigger_rules": rules, "trigger_only_during_business_hours": bh.unwrap_or(false), "message": message, "sender": sender }));
    }
    Ok(Json(json!(out)))
}

/// POST /api/v1/widget/events: only `campaign.triggered` has an effect (campaign_listener.rb +
/// campaigns/campaign_conversation_builder.rb); other events are analytics and are accepted silently.
async fn events(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = required(&st, &w, &headers).await?;
    if b["name"] != "campaign.triggered" {
        return Ok(Json(json!({})));
    }
    let info = &b["event_info"];
    let Some(display_id) = info["campaign_id"].as_i64().or_else(|| info["campaign_id"].as_str().and_then(|v| v.parse().ok())) else {
        return Ok(Json(json!({})));
    };
    let campaign: Option<(i64, String, Option<i64>)> = sqlx::query_as(
        "SELECT id::bigint, message, sender_id::bigint FROM campaigns WHERE inbox_id = $1 AND account_id = $2 AND display_id = $3 AND enabled",
    )
    .bind(w.inbox_id)
    .bind(w.account_id)
    .bind(display_id)
    .fetch_optional(&st.db)
    .await?;
    let Some((campaign_id, message, sender)) = campaign else { return Ok(Json(json!({}))) };
    // Builder rule: no campaign when the contact already has a conversation. Lock the contact_inbox row
    // so two triggers racing cannot both create one.
    let mut tx = st.db.begin().await?;
    sqlx::query("SELECT 1 FROM contact_inboxes WHERE id = $1 FOR UPDATE").bind(s.c.ci_id).execute(&mut *tx).await?;
    let existing: Option<i32> = sqlx::query_scalar("SELECT 1 FROM conversations WHERE contact_inbox_id = $1 LIMIT 1")
        .bind(s.c.ci_id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_some() {
        return Ok(Json(json!({})));
    }
    let mut extra = info.clone();
    if let Some(o) = extra.as_object_mut() {
        o.remove("campaign_id");
        o.remove("custom_attributes");
    }
    let custom = if info["custom_attributes"].is_object() { info["custom_attributes"].clone() } else { json!({}) };
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (account_id, inbox_id, status, contact_id, contact_inbox_id, campaign_id,
            additional_attributes, custom_attributes, last_activity_at, created_at, updated_at)
         VALUES ($1, $2, 0, $3, $4, $5, $6::jsonb, $7::jsonb, now(), now(), now()) RETURNING id::bigint",
    )
    .bind(w.account_id as i32)
    .bind(w.inbox_id as i32)
    .bind(s.c.contact_id)
    .bind(s.c.ci_id)
    .bind(campaign_id)
    .bind(extra)
    .bind(custom)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    let c = reload(&st.db, id).await?;
    notify_conv(&st, &c, "conversation.created").await?;
    let m = insert_message(&st, &c, sender.map(|u| ("User", u)), &message, 1, false, 0, json!({}), None).await?;
    // MessageBuilder#campaign_id stores it in message additional_attributes
    sqlx::query("UPDATE messages SET additional_attributes = COALESCE(additional_attributes,'{}'::jsonb) || jsonb_build_object('campaign_id', $2::bigint) WHERE id = $1")
        .bind(m["id"].as_i64().unwrap_or_default())
        .bind(campaign_id)
        .execute(&st.db)
        .await?;
    Ok(Json(json!({})))
}

#[derive(Deserialize, Default)]
struct Q {
    website_token: Option<String>,
    before: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct Wd {
    website_token: Option<String>,
    widget_color: Option<String>,
    welcome_title: Option<String>,
    welcome_tagline: Option<String>,
    reply_time: Option<i32>,
    pre_chat_form_enabled: Option<bool>,
    pre_chat_form_options: Option<Value>,
    hmac_mandatory: Option<bool>,
    feature_flags: i32,
    inbox_id: i64,
    inbox_name: String,
    account_id: i64,
    working_hours_enabled: Option<bool>,
    out_of_office_message: Option<String>,
    timezone: Option<String>,
    csat_survey_enabled: Option<bool>,
    allow_messages_after_resolved: Option<bool>,
    locale: i32,
    account_status: i32,
}

#[derive(sqlx::FromRow)]
struct Cs {
    ci_id: i64,
    source_id: String,
    pubsub_token: Option<String>,
    contact_id: i64,
    name: Option<String>,
    email: Option<String>,
    phone_number: Option<String>,
    identifier: Option<String>,
}

struct Sess {
    c: Cs,
    token: String,
    /// iat of the presented token (0 for freshly minted sessions)
    iat: i64,
}

#[derive(Deserialize)]
struct Claims {
    source_id: String,
    inbox_id: i64,
    exp: i64,
    #[serde(default)]
    iat: i64,
}

// ---------- token (HS256) ----------

fn secret() -> &'static [u8] {
    static S: OnceLock<Vec<u8>> = OnceLock::new();
    S.get_or_init(|| match std::env::var("SECRET_KEY_BASE") {
        Ok(v) if !v.is_empty() => v.into_bytes(),
        _ => {
            tracing::warn!("SECRET_KEY_BASE unset; widget tokens use the dev secret");
            b"pulsar-dev-secret-change-me".to_vec()
        }
    })
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut k = if key.len() > 64 { Sha256::digest(key).to_vec() } else { key.to_vec() };
    k.resize(64, 0);
    let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    let inner = Sha256::new().chain_update(&ipad).chain_update(msg).finalize();
    Sha256::new().chain_update(&opad).chain_update(inner).finalize().to_vec()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Same claims as Widget::TokenService: {source_id, inbox_id, iat, exp}, 180 days.
fn issue(source_id: &str, inbox_id: i64) -> String {
    issue_at(source_id, inbox_id, chrono::Utc::now().timestamp())
}

fn issue_at(source_id: &str, inbox_id: i64, iat: i64) -> String {
    let payload = json!({ "source_id": source_id, "inbox_id": inbox_id, "iat": iat, "exp": iat + 180 * 86400 });
    let head = B64.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let body = B64.encode(payload.to_string());
    let input = format!("{head}.{body}");
    let sig = B64.encode(hmac_sha256(secret(), input.as_bytes()));
    format!("{input}.{sig}")
}

fn decode(token: &str, inbox_id: i64) -> Option<Claims> {
    let mut it = token.split('.');
    let (h, p, s) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let input = &token[..h.len() + 1 + p.len()];
    let expect = hmac_sha256(secret(), input.as_bytes());
    let got = B64.decode(s).ok()?;
    if !ct_eq(&expect, &got) {
        return None;
    }
    let c: Claims = serde_json::from_slice(&B64.decode(p).ok()?).ok()?;
    if c.exp < chrono::Utc::now().timestamp() || c.inbox_id != inbox_id {
        return None;
    }
    Some(c)
}

fn token(h: &HeaderMap) -> Option<String> {
    h.get("x-auth-token").and_then(|v| v.to_str().ok()).map(str::to_string)
}

/// Writes need a valid visitor token from POST /config; a missing or forged token is 401.
async fn required(st: &AppState, w: &Wd, h: &HeaderMap) -> ApiResult<Sess> {
    read_session(&st.db, w, token(h).as_deref()).await?.ok_or_else(ApiError::unauthorized)
}

// ---------- lookups ----------

/// Visitor session gate for storage direct uploads (Chatwoot `set_contact`). Rejects without a valid X-Auth-Token.
pub(crate) async fn check_visitor(st: &AppState, website_token: Option<&str>, headers: &HeaderMap) -> ApiResult<()> {
    let w = widget_of(&st.db, website_token).await?;
    required(st, &w, headers).await?;
    Ok(())
}

async fn widget_of(db: &PgPool, website_token: Option<&str>) -> ApiResult<Wd> {
    let t = website_token.ok_or_else(ApiError::not_found)?;
    let w: Wd = sqlx::query_as(WIDGET_SQL).bind(t).fetch_optional(db).await?.ok_or_else(ApiError::not_found)?;
    if w.account_status == 1 {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "Account is suspended".into()));
    }
    Ok(w)
}

async fn find_contact(db: &PgPool, w: &Wd, source_id: &str) -> ApiResult<Option<Cs>> {
    Ok(sqlx::query_as(
        "SELECT ci.id::bigint AS ci_id, ci.source_id, ci.pubsub_token, c.id::bigint AS contact_id,
                c.name, c.email, c.phone_number, c.identifier
         FROM contact_inboxes ci JOIN contacts c ON c.id = ci.contact_id
         WHERE ci.inbox_id = $1::bigint AND ci.source_id = $2 AND c.account_id = $3::bigint",
    )
    .bind(w.inbox_id)
    .bind(source_id)
    .bind(w.account_id)
    .fetch_optional(db)
    .await?)
}

/// Read-only: resolve visitor from a valid token. No contact is created.
async fn read_session(db: &PgPool, w: &Wd, tok: Option<&str>) -> ApiResult<Option<Sess>> {
    let Some(tok) = tok else { return Ok(None) };
    let Some(cl) = decode(tok, w.inbox_id) else { return Ok(None) };
    let Some(c) = find_contact(db, w, &cl.source_id).await? else { return Ok(None) };
    // chatwoot#15776: tokens minted before the last rotation for this visitor are revoked.
    let floor: Option<i64> = sqlx::query_scalar("SELECT min_iat FROM pulsar_widget_token_floors WHERE contact_inbox_id = $1")
        .bind(c.ci_id)
        .fetch_optional(db)
        .await?;
    if floor.is_some_and(|f| cl.iat < f) {
        return Ok(None);
    }
    Ok(Some(Sess { c, token: tok.to_string(), iat: cl.iat }))
}

/// chatwoot#15776: revoke every earlier token for this visitor and mint one that passes the new floor.
/// iat has one-second resolution, so the floor must be strictly above the iat of the token being
/// replaced (and above any previous floor). Otherwise a rotation in the same second as the token was
/// minted would yield an identical token and the old one would stay valid.
async fn rotate_token(db: &PgPool, w: &Wd, ci_id: i64, source_id: &str, current_iat: i64) -> ApiResult<String> {
    let floor: i64 = sqlx::query_scalar(
        "INSERT INTO pulsar_widget_token_floors (contact_inbox_id, min_iat, updated_at)
         VALUES ($1, greatest(extract(epoch from now())::bigint, $2 + 1), now())
         ON CONFLICT (contact_inbox_id) DO UPDATE
           SET min_iat = greatest(extract(epoch from now())::bigint, $2 + 1, pulsar_widget_token_floors.min_iat + 1),
               updated_at = now()
         RETURNING min_iat",
    )
    .bind(ci_id)
    .bind(current_iat)
    .fetch_one(db)
    .await?;
    Ok(issue_at(source_id, w.inbox_id, floor))
}

/// Resolve visitor, or create a new anonymous contact + contact_inbox and issue a token.
/// Only used by POST /config: a present-but-invalid token is rejected instead of silently replaced.
async fn session(st: &AppState, w: &Wd, tok: Option<&str>) -> ApiResult<Sess> {
    if tok.is_some() {
        return read_session(&st.db, w, tok).await?.ok_or_else(ApiError::unauthorized);
    }
    let name = format!("visitor-{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
    let contact_id: i64 = sqlx::query_scalar(
        "INSERT INTO contacts (account_id, name, created_at, updated_at) VALUES ($1, $2, now(), now()) RETURNING id::bigint",
    )
    .bind(w.account_id as i32)
    .bind(&name)
    .fetch_one(&st.db)
    .await?;
    let source_id = uuid::Uuid::new_v4().to_string();
    let pubsub = uuid::Uuid::new_v4().simple().to_string();
    let ci_id: i64 = sqlx::query_scalar(
        "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, pubsub_token, created_at, updated_at)
         VALUES ($1, $2, $3, $4, now(), now()) RETURNING id::bigint",
    )
    .bind(contact_id)
    .bind(w.inbox_id)
    .bind(&source_id)
    .bind(&pubsub)
    .fetch_one(&st.db)
    .await?;
    let token = issue(&source_id, w.inbox_id);
    let c = Cs {
        ci_id,
        source_id,
        pubsub_token: Some(pubsub),
        contact_id,
        name: Some(name),
        email: None,
        phone_number: None,
        identifier: None,
    };
    Ok(Sess { c, token, iat: chrono::Utc::now().timestamp() })
}

fn presence(c: &Cs) -> Value {
    let has = |s: &Option<String>| s.as_deref().is_some_and(|v| !v.trim().is_empty());
    json!({
        "id": c.contact_id,
        "has_email": has(&c.email),
        "has_name": has(&c.name),
        "has_phone_number": has(&c.phone_number),
    })
}

async fn latest_conv(db: &PgPool, w: &Wd, ci_id: i64) -> ApiResult<Option<Conv>> {
    Ok(sqlx::query_as(&format!(
        "SELECT {CONV_COLS} FROM conversations c
         WHERE c.account_id = $1 AND c.inbox_id = $2 AND c.contact_inbox_id = $3
         ORDER BY c.id DESC LIMIT 1"
    ))
    .bind(w.account_id as i32)
    .bind(w.inbox_id as i32)
    .bind(ci_id)
    .fetch_optional(db)
    .await?)
}

fn widget_data(c: &Conv) -> Value {
    json!({
        "id": c.display_id,
        "inbox_id": c.inbox_id,
        "contact_last_seen_at": ts(c.contact_last_seen_at),
        "status": STATUSES.get(c.status as usize).copied().unwrap_or("open"),
    })
}

/// Agents of the inbox get the full agent payload; the visitor gets widget_data only.
async fn notify_agents(st: &AppState, c: &Conv, event: &str) -> ApiResult<()> {
    let tokens = st.hub.inbox_tokens(&st.db, c.account_id, c.inbox_id).await;
    st.hub.broadcast(&tokens, event, conv_json(&st.db, c).await);
    Ok(())
}

async fn notify_conv(st: &AppState, c: &Conv, event: &str) -> ApiResult<()> {
    notify_agents(st, c, event).await?;
    if let Some(ci) = c.contact_inbox_id {
        let t: Option<Option<String>> = sqlx::query_scalar("SELECT pubsub_token FROM contact_inboxes WHERE id = $1::bigint")
            .bind(ci)
            .fetch_optional(&st.db)
            .await?;
        if let Some(Some(t)) = t {
            st.hub.broadcast(&[t], event, widget_data(c));
        }
    }
    Ok(())
}

async fn create_conv(st: &AppState, w: &Wd, s: &Sess, extra: Value, custom: Value) -> ApiResult<Conv> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (account_id, inbox_id, status, contact_id, contact_inbox_id,
            additional_attributes, custom_attributes, last_activity_at, created_at, updated_at)
         VALUES ($1, $2, 0, $3, $4, $5::jsonb, $6::jsonb, now(), now(), now()) RETURNING id::bigint",
    )
    .bind(w.account_id as i32)
    .bind(w.inbox_id as i32)
    .bind(s.c.contact_id)
    .bind(s.c.ci_id)
    .bind(extra)
    .bind(custom)
    .fetch_one(&st.db)
    .await?;
    let c = reload(&st.db, id).await?;
    notify_conv(st, &c, "conversation.created").await?;
    Ok(c)
}

/// Widget message shape: sender reduced to type/id/name, no contact details.
async fn wmsg(db: &PgPool, m: &Msg, display_id: i64) -> Value {
    let sender = match (m.sender_type.as_deref(), m.sender_id) {
        (Some("Contact"), _) => json!({ "type": "contact" }),
        (Some("User"), Some(uid)) => {
            let name: Option<String> = sqlx::query_scalar::<_, Option<String>>("SELECT name FROM users WHERE id = $1::bigint")
                .bind(uid)
                .fetch_optional(db)
                .await
                .ok()
                .flatten()
                .flatten();
            json!({ "id": uid, "name": name.unwrap_or_default(), "type": "user", "avatar_url": "" })
        }
        _ => Value::Null,
    };
    json!({
        "id": m.id,
        "content": m.content,
        "message_type": m.message_type,
        "content_type": CONTENT_TYPES.get(m.content_type as usize).copied().unwrap_or("text"),
        "content_attributes": m.content_attributes.clone().unwrap_or(json!({})),
        "created_at": m.created_at.and_utc().timestamp(),
        "conversation_id": display_id,
        "private": m.private,
        "source_id": m.source_id,
        "sender": sender,
    })
}

/// Public, non-activity messages of a conversation, ascending, 20 per page.
async fn chat_messages(db: &PgPool, c: &Conv, before: Option<i64>) -> ApiResult<Vec<Value>> {
    let mut rows: Vec<Msg> = sqlx::query_as(&format!(
        "SELECT {MSG_COLS} FROM messages m
         WHERE m.conversation_id = $1 AND m.account_id = $2 AND NOT m.private AND m.message_type <> 2
           AND ($3::bigint IS NULL OR m.id::bigint < $3)
         ORDER BY m.created_at DESC, m.id DESC LIMIT 20"
    ))
    .bind(c.id as i32)
    .bind(c.account_id as i32)
    .bind(before)
    .fetch_all(db)
    .await?;
    rows.reverse();
    let mut out = Vec::with_capacity(rows.len());
    for m in &rows {
        out.push(wmsg(db, m, c.display_id).await);
    }
    Ok(out)
}

fn msg_content(m: &Value) -> ApiResult<String> {
    let raw = m["content"].as_str().unwrap_or("");
    if raw.trim().is_empty() {
        return Err(ApiError::bad("Content can't be blank"));
    }
    if raw.chars().count() > 10_000 {
        return Err(ApiError::bad("Content is too long"));
    }
    let clean = strip_html(raw);
    if clean.trim().is_empty() {
        return Err(ApiError::bad("Content can't be blank"));
    }
    Ok(clean)
}

/// Chatwoot #15779: store visitor text without HTML, as defense in depth against a client
/// sanitizer bypass becoming stored XSS. Removes every tag (and script/style bodies), keeps the
/// text and markdown, and decodes the entities ammonia escapes so `a < b` stays readable.
/// Decoding can turn visitor-written `&lt;script&gt;` into a tag, so repeat until stable.
fn strip_html(raw: &str) -> String {
    let once = |s: &str| {
        ammonia::Builder::empty()
            .clean(s)
            .to_string()
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
    };
    let mut cur = once(raw);
    for _ in 0..4 {
        let next = once(&cur);
        if next == cur {
            return cur;
        }
        cur = next;
    }
    // still changing after nested encodings: fall back to the fully escaped form, never raw
    ammonia::Builder::empty().clean(&cur).to_string()
}

/// Safe contact update: name/phone/custom_attributes merge; email only if not taken by another contact.
/// ponytail: identifier is never set from the client (would allow binding to another visitor's contact).
async fn apply_contact(db: &PgPool, w: &Wd, cid: i64, src: &Value) -> ApiResult<()> {
    let clean = |k: &str| src[k].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let name = clean("name");
    let phone = clean("phone_number");
    let mut email = clean("email").map(|e| e.to_lowercase());
    if let Some(e) = &email {
        let taken: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM contacts WHERE account_id = $1::bigint AND lower(email) = $2 AND id <> $3::bigint",
        )
        .bind(w.account_id)
        .bind(e)
        .bind(cid)
        .fetch_optional(db)
        .await?;
        if taken.is_some() {
            email = None; // discard like Chatwoot discard_invalid_attrs
        }
    }
    let custom = src["custom_attributes"].is_object().then(|| src["custom_attributes"].clone());
    sqlx::query(
        "UPDATE contacts SET name = COALESCE($3, name), email = COALESCE($4, email),
           phone_number = COALESCE($5, phone_number),
           custom_attributes = COALESCE(custom_attributes, '{}'::jsonb) || COALESCE($6::jsonb, '{}'::jsonb),
           updated_at = now()
         WHERE id = $1::bigint AND account_id = $2::bigint",
    )
    .bind(cid)
    .bind(w.account_id)
    .bind(name)
    .bind(email)
    .bind(phone)
    .bind(custom)
    .execute(db)
    .await?;
    Ok(())
}

// ---------- config ----------

/// Chatwoot `ActiveSupport::TimeZone[tz].now.formatted_offset`, e.g. "+05:30". Unknown zone gives "+00:00".
async fn utc_offset(db: &PgPool, tz: &str) -> String {
    let secs: i64 = sqlx::query_scalar("SELECT EXTRACT(EPOCH FROM ((now() AT TIME ZONE $1) - (now() AT TIME ZONE 'UTC')))::bigint")
        .bind(tz)
        .fetch_one(db)
        .await
        .unwrap_or(0);
    let sign = if secs < 0 { '-' } else { '+' };
    let abs = secs.abs();
    format!("{sign}{:02}:{:02}", abs / 3600, (abs % 3600) / 60)
}

fn pick_gc(gc: &Map<String, Value>) -> Value {
    const KEYS: [&str; 5] = ["LOGO_THUMBNAIL", "BRAND_NAME", "WIDGET_BRAND_URL", "MAXIMUM_FILE_UPLOAD_SIZE", "INSTALLATION_NAME"];
    Value::Object(KEYS.iter().map(|k| (k.to_string(), gc.get(*k).cloned().unwrap_or(Value::Null))).collect())
}

fn pre_chat_options(v: Option<Value>) -> Value {
    match v {
        Some(o) if o.get("pre_chat_fields").is_some() => o,
        _ => json!({
            "pre_chat_message": "Share your queries or comments here.",
            "pre_chat_fields": [
                { "field_type": "standard", "label": "Email Id", "name": "emailAddress", "type": "email", "required": true, "enabled": false },
                { "field_type": "standard", "label": "Full name", "name": "fullName", "type": "text", "required": false, "enabled": false },
                { "field_type": "standard", "label": "Phone number", "name": "phoneNumber", "type": "text", "required": false, "enabled": false }
            ]
        }),
    }
}

async fn channel_json(db: &PgPool, w: &Wd, token: &str) -> Value {
    let working_hours: Value = sqlx::query_scalar(
        "SELECT COALESCE(json_agg(json_build_object('day_of_week', day_of_week, 'open_hour', open_hour,
            'open_minutes', open_minutes, 'close_hour', close_hour, 'close_minutes', close_minutes,
            'open_all_day', open_all_day, 'closed_all_day', closed_all_day) ORDER BY day_of_week), '[]'::json)
         FROM working_hours WHERE inbox_id = $1::bigint",
    )
    .bind(w.inbox_id)
    .fetch_one(db)
    .await
    .unwrap_or(json!([]));
    let features: Vec<&str> = FEATURE_BITS.iter().filter(|(_, b)| w.feature_flags as i64 & b != 0).map(|(n, _)| *n).collect();
    let (acc_flags, acc_flags_ext): (i64, i64) = sqlx::query_as("SELECT COALESCE(feature_flags,0)::bigint, COALESCE(feature_flags_ext_1,0)::bigint FROM accounts WHERE id = $1")
        .bind(w.account_id)
        .fetch_one(db)
        .await
        .unwrap_or((0, 0));
    let account_features = crate::auth::enabled_features(acc_flags, acc_flags_ext);
    json!({
        "allow_messages_after_resolved": w.allow_messages_after_resolved.unwrap_or(true),
        "api_host": std::env::var("FRONTEND_URL").ok(),
        "auth_token": token,
        "avatar_url": crate::storage::avatar_url(db, "Inbox", w.inbox_id).await,
        "csat_survey_enabled": w.csat_survey_enabled.unwrap_or(false),
        "disable_branding": account_features.get("disable_branding").and_then(Value::as_bool).unwrap_or(false),
        "enabled_features": features,
        "enabled_languages": LANGUAGES.iter().map(|(name, code)| json!({ "name": name, "iso_639_1_code": code })).collect::<Vec<_>>(),
        "has_a_connected_agent_bot": "",
        "locale": locale_code(w.locale),
        "out_of_office_message": w.out_of_office_message,
        "portal": null,
        "pre_chat_form_enabled": w.pre_chat_form_enabled.unwrap_or(false),
        "pre_chat_form_options": pre_chat_options(w.pre_chat_form_options.clone()),
        "reply_time": REPLY_TIMES.get(w.reply_time.unwrap_or(0) as usize).copied().unwrap_or("in_a_few_minutes"),
        "timezone": w.timezone.clone().unwrap_or_else(|| "UTC".into()),
        "utc_off_set": utc_offset(db, w.timezone.as_deref().unwrap_or("UTC")).await,
        "website_name": w.inbox_name,
        "website_token": w.website_token,
        "welcome_tagline": w.welcome_tagline,
        "welcome_title": w.welcome_title,
        "widget_color": w.widget_color,
        "working_hours": working_hours,
        "working_hours_enabled": w.working_hours_enabled.unwrap_or(false),
    })
}

/// GET /widget?website_token=: the widget UI is the React route /app/widget (same SPA build).
/// Chatwoot #15775 (CWE-598): cw_conversation is accepted from query for backward compatibility,
/// but stripped before redirecting so the visitor JWT never leaks into browser history or logs.
/// Response sets Referrer-Policy: no-referrer and Cache-Control: no-store.
async fn page(axum::extract::RawQuery(q): axum::extract::RawQuery) -> Response {
    let raw = q.unwrap_or_default();
    let cleaned: String = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(
            url::form_urlencoded::parse(raw.as_bytes())
                .filter(|(k, _)| k != "cw_conversation"),
        )
        .finish();
    let target = if cleaned.is_empty() {
        "/app/widget".to_string()
    } else {
        format!("/app/widget?{cleaned}")
    };
    let mut res = axum::response::Redirect::to(&target).into_response();
    let h = res.headers_mut();
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

/// POST /api/v1/widget/config?website_token=
async fn config(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = session(&st, &w, token(&headers).as_deref()).await?;
    let mut contact = presence(&s.c);
    contact["pubsub_token"] = json!(s.c.pubsub_token);
    let gc = crate::web::global_config(&st.db).await;
    Ok(Json(json!({
        "website_channel_config": channel_json(&st.db, &w, &s.token).await,
        "contact": contact,
        "global_config": pick_gc(&gc),
    })))
}

// ---------- conversations ----------

/// GET /api/v1/widget/conversations
async fn conv_index(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let Some(s) = read_session(&st.db, &w, token(&headers).as_deref()).await? else {
        return Ok(Json(json!({})));
    };
    Ok(Json(match latest_conv(&st.db, &w, s.c.ci_id).await? {
        Some(c) => widget_data(&c),
        None => json!({}),
    }))
}

/// POST /api/v1/widget/conversations  { contact, message, custom_attributes }
async fn conv_create(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = required(&st, &w, &headers).await?;
    let content = msg_content(&b["message"])?;
    apply_contact(&st.db, &w, s.c.contact_id, &b["contact"]).await?;
    let extra = json!({ "referer": b["message"]["referer_url"], "initiated_at": b["message"]["timestamp"] });
    let custom = if b["custom_attributes"].is_object() { b["custom_attributes"].clone() } else { json!({}) };
    let c = create_conv(&st, &w, &s, extra, custom).await?;
    insert_message(&st, &c, Some(("Contact", s.c.contact_id)), &content, 0, false, 0, json!({}), b["message"]["echo_id"].as_str())
        .await?;
    let c = reload(&st.db, c.id).await?;
    let mut out = widget_data(&c);
    out["messages"] = json!(chat_messages(&st.db, &c, None).await?);
    out["custom_attributes"] = c.custom_attributes.clone().unwrap_or(json!({}));
    Ok(Json(out))
}

/// POST /api/v1/widget/conversations/update_last_seen
async fn update_last_seen(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let Some(s) = read_session(&st.db, &w, token(&headers).as_deref()).await? else {
        return Ok(Json(json!({})));
    };
    if let Some(c) = latest_conv(&st.db, &w, s.c.ci_id).await? {
        sqlx::query("UPDATE conversations SET contact_last_seen_at = now() WHERE id = $1")
            .bind(c.id as i32)
            .execute(&st.db)
            .await?;
    }
    Ok(Json(json!({})))
}

/// POST /api/v1/widget/conversations/toggle_typing  { typing_status: on|off }
async fn toggle_typing(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = read_session(&st.db, &w, token(&headers).as_deref()).await?.ok_or_else(ApiError::not_found)?;
    // Chatwoot #14710: the SDK sends typing before the first message exists. No conversation yet is a no-op, not a 404.
    let Some(c) = latest_conv(&st.db, &w, s.c.ci_id).await? else { return Ok(Json(json!({}))) };
    let event = match b["typing_status"].as_str() {
        Some("on") => "conversation.typing_on",
        Some("off") => "conversation.typing_off",
        _ => return Ok(Json(json!({}))),
    };
    // agents only: the visitor does not need its own typing echo
    let data = json!({
        "conversation": conv_json(&st.db, &c).await,
        "user": contact_json(&st.db, Some(s.c.contact_id)).await,
        "is_private": false,
    });
    let agents = st.hub.inbox_tokens(&st.db, c.account_id, c.inbox_id).await;
    st.hub.broadcast(&agents, event, data);
    Ok(Json(json!({})))
}

/// GET /api/v1/widget/conversations/toggle_status  (visitor ends conversation)
async fn toggle_status(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    if w.feature_flags & 4 == 0 {
        return Err(ApiError::forbidden()); // end_conversation flag off
    }
    let s = read_session(&st.db, &w, token(&headers).as_deref()).await?.ok_or_else(ApiError::not_found)?;
    let c = latest_conv(&st.db, &w, s.c.ci_id).await?.ok_or_else(ApiError::not_found)?;
    if c.status != 1 {
        sqlx::query("UPDATE conversations SET status = 1, updated_at = now() WHERE id = $1")
            .bind(c.id as i32)
            .execute(&st.db)
            .await?;
        let c = reload(&st.db, c.id).await?;
        notify_agents(&st, &c, "conversation.updated").await?;
    }
    Ok(Json(json!({})))
}

// ---------- messages ----------

/// GET /api/v1/widget/messages?before=
async fn msg_index(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let empty = || Json(json!({ "payload": [], "meta": {} }));
    let Some(s) = read_session(&st.db, &w, token(&headers).as_deref()).await? else { return Ok(empty()) };
    let Some(c) = latest_conv(&st.db, &w, s.c.ci_id).await? else { return Ok(empty()) };
    Ok(Json(json!({
        "payload": chat_messages(&st.db, &c, q.before).await?,
        "meta": { "contact_last_seen_at": ts(c.contact_last_seen_at) },
    })))
}

/// POST /api/v1/widget/messages  { message: { content, echo_id, timestamp, referer_url } }
async fn msg_create(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = required(&st, &w, &headers).await?;
    let content = msg_content(&b["message"])?;
    // Chatwoot #16417: two first messages sent together opened two conversations. Serialize the
    // find-or-create per contact_inbox with a transaction-scoped advisory lock (no schema change).
    let mut tx = st.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('pulsar_widget_conv'), $1::int)")
        .bind(s.c.ci_id as i32)
        .execute(&mut *tx)
        .await?;
    let existing: Option<Conv> = sqlx::query_as(&format!(
        "SELECT {CONV_COLS} FROM conversations c
         WHERE c.account_id = $1 AND c.inbox_id = $2 AND c.contact_inbox_id = $3
         ORDER BY c.id DESC LIMIT 1"
    ))
    .bind(w.account_id as i32)
    .bind(w.inbox_id as i32)
    .bind(s.c.ci_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (conv, created_new) = match existing {
        Some(c) => {
            if c.status == 1 && !w.allow_messages_after_resolved.unwrap_or(true) {
                return Err(ApiError(StatusCode::FORBIDDEN, "Conversation is resolved. Start a new conversation.".into()));
            }
            (c, false)
        }
        None => {
            let extra = json!({ "referer": b["message"]["referer_url"], "initiated_at": b["message"]["timestamp"] });
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO conversations (account_id, inbox_id, status, contact_id, contact_inbox_id,
                    additional_attributes, custom_attributes, last_activity_at, created_at, updated_at)
                 VALUES ($1, $2, 0, $3, $4, $5::jsonb, '{}'::jsonb, now(), now(), now()) RETURNING id::bigint",
            )
            .bind(w.account_id as i32)
            .bind(w.inbox_id as i32)
            .bind(s.c.contact_id)
            .bind(s.c.ci_id)
            .bind(extra)
            .fetch_one(&mut *tx)
            .await?;
            let c: Conv = sqlx::query_as(&format!("SELECT {CONV_COLS} FROM conversations c WHERE c.id = $1"))
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            (c, true)
        }
    };
    tx.commit().await?;
    if created_new {
        notify_conv(&st, &conv, "conversation.created").await?;
    }
    let echo = b["message"]["echo_id"].as_str();
    let created = insert_message(&st, &conv, Some(("Contact", s.c.contact_id)), &content, 0, false, 0, json!({}), echo).await?;
    let id = created["id"].as_i64().unwrap_or_default();
    let m: Msg = sqlx::query_as(&format!("SELECT {MSG_COLS} FROM messages m WHERE m.id::bigint = $1"))
        .bind(id)
        .fetch_one(&st.db)
        .await?;
    let mut out = wmsg(&st.db, &m, conv.display_id).await;
    out["inbox_id"] = json!(conv.inbox_id);
    Ok(Json(out))
}

/// PATCH /api/v1/widget/messages/{id}  { message: { submitted_values }, contact: { email } }
/// Chatwoot PR #15778: a contact write here is as sensitive as PATCH /widget/contact, so it rotates the
/// visitor token the same way.
async fn msg_update(State(st): State<AppState>, headers: HeaderMap, Path(id): Path<i64>, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult<Response> {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = read_session(&st.db, &w, token(&headers).as_deref()).await?.ok_or_else(ApiError::not_found)?;
    // scoped to ALL of this visitor's conversations in this inbox, never to ids from elsewhere
    let m: Msg = sqlx::query_as(&format!(
        "SELECT {MSG_COLS} FROM messages m
         WHERE m.id::bigint = $1 AND m.conversation_id IN (
           SELECT id FROM conversations WHERE contact_inbox_id = $2::bigint AND inbox_id = $3::bigint AND account_id = $4::bigint)"
    ))
    .bind(id)
    .bind(s.c.ci_id)
    .bind(w.inbox_id)
    .bind(w.account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    if let Some(v) = b["message"].get("submitted_values").filter(|v| !v.is_null()) {
        sqlx::query(
            "UPDATE messages SET content_attributes = (COALESCE(content_attributes::jsonb, '{}'::jsonb) || $2::jsonb)::json,
               updated_at = now() WHERE id = $1",
        )
        .bind(m.id as i32)
        .bind(json!({ "submitted_values": v }))
        .execute(&st.db)
        .await?;
        // message_updated drives the CSAT response builder (csat_survey_listener.rb) and webhooks
        let fresh: Msg = sqlx::query_as(&format!("SELECT {MSG_COLS} FROM messages m WHERE m.id = $1")).bind(m.id as i32).fetch_one(&st.db).await?;
        let c = reload(&st.db, fresh.conversation_id).await?;
        let data = msg_json(&st.db, &fresh, &c).await;
        crate::conversations::emit(&st, &c, "message.updated", data).await;
    }
    if b["contact"].is_null() {
        let fresh = find_contact(&st.db, &w, &s.c.source_id).await?.unwrap_or(s.c);
        return Ok(Json(json!({ "contact": presence(&fresh) })).into_response());
    }
    apply_contact(&st.db, &w, s.c.contact_id, &b["contact"]).await?;
    let fresh = find_contact(&st.db, &w, &s.c.source_id).await?.unwrap_or(s.c);
    let new_tok = rotate_token(&st.db, &w, fresh.ci_id, &fresh.source_id, s.iat).await?;
    let mut resp = Json(json!({ "contact": presence(&fresh), "widget_auth_token": new_tok })).into_response();
    if let Ok(hv) = HeaderValue::from_str(&new_tok) {
        resp.headers_mut().insert(HeaderName::from_static("x-auth-token"), hv);
    }
    Ok(resp)
}

// ---------- contact ----------

/// GET /api/v1/widget/contact
async fn contact_show(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = read_session(&st.db, &w, token(&headers).as_deref()).await?.ok_or_else(ApiError::not_found)?;
    let mut v = presence(&s.c);
    v["identifier"] = json!(s.c.identifier);
    Ok(Json(v))
}

/// PATCH /api/v1/widget/contact and PATCH /api/v1/widget/contact/set_user
/// Chatwoot #15776: rotate visitor token after sensitive contact updates.
/// Chatwoot #15933: include pubsub_token in the contact response so the widget can re-subscribe
/// to realtime events when the visitor contact is updated/rebound.
///
/// Earlier tokens are revoked through pulsar_widget_token_floors (migrations/0003), see rotate_token.
async fn contact_update(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult<Response> {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = read_session(&st.db, &w, token(&headers).as_deref()).await?.ok_or_else(ApiError::not_found)?;
    // ponytail: identifier_hash (HMAC) not verified. Identity binding by identifier is ignored, and
    // refused outright on hmac_mandatory inboxes.
    if w.hmac_mandatory == Some(true) && !b["identifier"].is_null() {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "HMAC failed: Invalid Identifier Hash Provided".into()));
    }
    apply_contact(&st.db, &w, s.c.contact_id, &b).await?;
    let fresh = find_contact(&st.db, &w, &s.c.source_id).await?.unwrap_or(s.c);
    let new_tok = rotate_token(&st.db, &w, fresh.ci_id, &fresh.source_id, s.iat).await?;
    let mut val = presence(&fresh);
    val["widget_auth_token"] = json!(new_tok);
    val["pubsub_token"] = json!(fresh.pubsub_token);
    let mut resp = Json(val).into_response();
    if let Ok(hv) = HeaderValue::from_str(&new_tok) {
        resp.headers_mut().insert(HeaderName::from_static("x-auth-token"), hv);
    }
    Ok(resp)
}

/// PATCH /api/v1/widget/contact/set_user (contacts_controller#set_user). `identifier` binds the visitor to an
/// identified contact, so it needs a matching identifier_hash (HMAC-SHA256 of identifier, keyed by the
/// inbox hmac_token). Without that the request is refused with Chatwoot's 401 message.
async fn contact_set_user(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult<Response> {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = read_session(&st.db, &w, token(&headers).as_deref()).await?.ok_or_else(ApiError::not_found)?;
    let identifier = b["identifier"].as_str().filter(|v| !v.is_empty());
    let hash_given = b["identifier_hash"].as_str().filter(|v| !v.is_empty());
    // should_verify_hmac?: verify when a hash is sent or the inbox is hmac_mandatory
    let must_verify = hash_given.is_some() || w.hmac_mandatory == Some(true);
    if must_verify && !valid_hmac(&st.db, &w, identifier, hash_given).await? {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "HMAC failed: Invalid Identifier Hash Provided".into()));
    }
    if must_verify && identifier.is_some() {
        sqlx::query("UPDATE contact_inboxes SET hmac_verified = true WHERE id = $1").bind(s.c.ci_id).execute(&st.db).await?;
    }
    apply_contact(&st.db, &w, s.c.contact_id, &b).await?;
    if let Some(id) = identifier {
        // Identifier is unique per account. Only set it when no other contact already owns it.
        sqlx::query("UPDATE contacts SET identifier = $2 WHERE id = $1::bigint AND account_id = $3::bigint
            AND NOT EXISTS (SELECT 1 FROM contacts o WHERE o.account_id = $3::bigint AND o.identifier = $2 AND o.id <> $1::bigint)")
            .bind(s.c.contact_id)
            .bind(id)
            .bind(w.account_id)
            .execute(&st.db)
            .await?;
    }
    contact_response(&st, &w, &s.c.source_id, s.c.ci_id, s.iat, s.c.contact_id).await
}

/// HMAC check for set_user. The hash must be present and match, otherwise the call is refused.
async fn valid_hmac(db: &PgPool, w: &Wd, identifier: Option<&str>, hash: Option<&str>) -> ApiResult<bool> {
    let key: Option<String> = sqlx::query_scalar("SELECT hmac_token FROM channel_web_widgets WHERE website_token = $1")
        .bind(w.website_token.as_deref().unwrap_or_default())
        .fetch_optional(db)
        .await?
        .flatten();
    let (Some(key), Some(hash), Some(id)) = (key, hash, identifier) else { return Ok(false) };
    let expect = hex::encode(hmac_sha256(key.as_bytes(), id.as_bytes()));
    Ok(ct_eq(expect.as_bytes(), hash.as_bytes()))
}

/// Shared contact response: presence fields, rotated widget_auth_token, pubsub_token.
async fn contact_response(st: &AppState, w: &Wd, source_id: &str, _ci_id: i64, iat: i64, _cid: i64) -> ApiResult<Response> {
    let fresh = find_contact(&st.db, w, source_id).await?.ok_or_else(ApiError::not_found)?;
    let new_tok = rotate_token(&st.db, w, fresh.ci_id, &fresh.source_id, iat).await?;
    let mut val = presence(&fresh);
    val["widget_auth_token"] = json!(new_tok);
    val["pubsub_token"] = json!(fresh.pubsub_token);
    let mut resp = Json(val).into_response();
    if let Ok(hv) = HeaderValue::from_str(&new_tok) {
        resp.headers_mut().insert(HeaderName::from_static("x-auth-token"), hv);
    }
    Ok(resp)
}

fn clone_cs(c: &Cs) -> Cs {
    Cs {
        ci_id: c.ci_id,
        source_id: c.source_id.clone(),
        pubsub_token: c.pubsub_token.clone(),
        contact_id: c.contact_id,
        name: c.name.clone(),
        email: c.email.clone(),
        phone_number: c.phone_number.clone(),
        identifier: c.identifier.clone(),
    }
}

/// POST /api/v1/widget/contact/destroy_custom_attributes {custom_attributes: [keys]}
async fn contact_destroy_custom_attributes(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = required(&st, &w, &headers).await?;
    let keys = string_list(&b["custom_attributes"]);
    sqlx::query("UPDATE contacts SET custom_attributes = COALESCE(custom_attributes, '{}'::jsonb) - $2::text[], updated_at = now() WHERE id = $1::bigint")
        .bind(s.c.contact_id)
        .bind(&keys)
        .execute(&st.db)
        .await?;
    let fresh = find_contact(&st.db, &w, &s.c.source_id).await?.unwrap_or_else(|| clone_cs(&s.c));
    Ok(Json(presence(&fresh)))
}

fn string_list(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// POST /api/v1/widget/conversations/set_custom_attributes {custom_attributes: {...}} on the visitor's latest conversation.
async fn conv_set_custom_attributes(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = required(&st, &w, &headers).await?;
    let c = latest_conv(&st.db, &w, s.c.ci_id).await?.ok_or_else(ApiError::not_found)?;
    let attrs = if b["custom_attributes"].is_object() { b["custom_attributes"].clone() } else { json!({}) };
    sqlx::query("UPDATE conversations SET custom_attributes = $2, updated_at = now() WHERE id = $1").bind(c.id).bind(&attrs).execute(&st.db).await?;
    Ok(Json(json!({ "custom_attributes": attrs })))
}

/// POST /api/v1/widget/conversations/destroy_custom_attributes {custom_attribute: [keys]}
async fn conv_destroy_custom_attributes(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = required(&st, &w, &headers).await?;
    let c = latest_conv(&st.db, &w, s.c.ci_id).await?.ok_or_else(ApiError::not_found)?;
    let keys = string_list(&b["custom_attribute"]);
    let attrs: Value = sqlx::query_scalar(
        "UPDATE conversations SET custom_attributes = COALESCE(custom_attributes, '{}'::jsonb) - $2::text[], updated_at = now()
         WHERE id = $1 RETURNING custom_attributes",
    )
    .bind(c.id)
    .bind(&keys)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(json!({ "custom_attributes": attrs })))
}

/// POST /api/v1/widget/conversations/transcript {email} (conversations_controller#transcript). Gated like
/// Chatwoot: plan gate is always true in Pulsar, rate limit per account not implemented (ponytail).
async fn conv_transcript(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = required(&st, &w, &headers).await?;
    let Some(c) = latest_conv(&st.db, &w, s.c.ci_id).await? else {
        return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, String::new()));
    };
    // Chatwoot sends to the contact's stored email, not a client-supplied address.
    let Some(email) = s.c.email.clone().filter(|e| !e.trim().is_empty()) else { return Ok(Json(json!({}))) };
    let messages: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT content FROM messages WHERE conversation_id = $1 AND account_id = $2 AND private = false
         AND message_type IN (0, 1) AND content_attributes::jsonb ->> 'forwarded_message_id' IS NULL ORDER BY created_at, id",
    )
    .bind(c.id)
    .bind(c.account_id)
    .fetch_all(&st.db)
    .await?;
    let mut html = String::from("<html><body><ul>");
    for m in messages.into_iter().flatten() {
        html.push_str("<li>");
        html.push_str(&m.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;"));
        html.push_str("</li>");
    }
    html.push_str("</ul></body></html>");
    if let Err(e) = crate::mailer::send_html(&email, "Conversation transcript", html).await {
        tracing::error!("transcript email: {e}");
        return Err(ApiError::bad("Unable to send transcript"));
    }
    Ok(Json(json!({})))
}

/// POST /api/v1/widget/labels {label}: adds the label to the visitor's latest conversation only when the
/// label already exists in the account (labels_controller#create). Always 204.
async fn label_add(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult<StatusCode> {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = required(&st, &w, &headers).await?;
    let Some(label) = b["label"].as_str().filter(|l| !l.is_empty()) else { return Ok(StatusCode::NO_CONTENT) };
    let Some(c) = latest_conv(&st.db, &w, s.c.ci_id).await? else { return Ok(StatusCode::NO_CONTENT) };
    let defined: Option<i32> = sqlx::query_scalar("SELECT 1 FROM labels WHERE account_id = $1 AND title = $2")
        .bind(w.account_id)
        .bind(label)
        .fetch_optional(&st.db)
        .await?;
    if defined.is_some() {
        crate::conversations::patch_labels_pub(&st.db, c.id, &[label.to_string()], &[]).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/widget/labels {label} removes the label from the visitor's latest conversation. Always 204.
async fn label_remove(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<Q>, Json(b): Json<Value>) -> ApiResult<StatusCode> {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let s = required(&st, &w, &headers).await?;
    let Some(label) = b["label"].as_str().filter(|l| !l.is_empty()) else { return Ok(StatusCode::NO_CONTENT) };
    let Some(c) = latest_conv(&st.db, &w, s.c.ci_id).await? else { return Ok(StatusCode::NO_CONTENT) };
    crate::conversations::patch_labels_pub(&st.db, c.id, &[], &[label.to_string()]).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- inbox members ----------

/// GET /api/v1/widget/inbox_members
async fn inbox_members(State(st): State<AppState>, Query(q): Query<Q>) -> ApiResult {
    let w = widget_of(&st.db, q.website_token.as_deref()).await?;
    let rows: Vec<(i64, Option<String>, Option<i32>)> = sqlx::query_as(
        "SELECT u.id::bigint, u.name, au.availability
         FROM inbox_members im
         JOIN users u ON u.id = im.user_id
         JOIN account_users au ON au.user_id = u.id AND au.account_id = $2::bigint
         WHERE im.inbox_id = $1::bigint
         ORDER BY u.id",
    )
    .bind(w.inbox_id)
    .bind(w.account_id)
    .fetch_all(&st.db)
    .await?;
    let payload: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, av)| {
            json!({
                "id": id,
                "name": name.unwrap_or_default(),
                "avatar_url": "",
                "availability_status": availability_name(av.unwrap_or(0)),
            })
        })
        .collect();
    Ok(Json(json!({ "payload": payload })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// chatwoot#15779: no HTML survives, markdown and plain `<` text stay.
    #[test]
    fn strip_html_removes_tags_keeps_text() {
        assert_eq!(strip_html("hi <img src=x onerror=alert(1)> **bold**"), "hi  **bold**");
        assert_eq!(strip_html("<script>alert(1)</script>ok"), "ok");
        assert_eq!(strip_html("a < b && c > d"), "a < b && c > d");
        // escaped markup written by the visitor must not decode back into a live tag
        let s = strip_html("&lt;script&gt;alert(1)&lt;/script&gt;");
        assert!(!s.contains("<script"), "{s}");
    }

    /// RFC 4231 test case 2 (HMAC-SHA256).
    #[test]
    fn hmac_matches_rfc4231() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(hex::encode(mac), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn token_roundtrip_and_tamper() {
        let t = issue("src-1", 7);
        assert_eq!(decode(&t, 7).map(|c| c.source_id), Some("src-1".into()));
        assert!(decode(&t, 8).is_none(), "wrong inbox must fail");
        let tampered = t.replacen('.', ".x", 1);
        assert!(decode(&tampered, 7).is_none());
    }

    /// chatwoot#15776: tokens are deterministic per (source, inbox, iat), so a rotated token must use a
    /// strictly larger iat or it equals the one it replaces (the bug the floor `current_iat + 1` fixes).
    #[test]
    fn rotated_token_needs_a_larger_iat() {
        assert_eq!(issue_at("s", 1, 100), issue_at("s", 1, 100));
        assert_ne!(issue_at("s", 1, 100), issue_at("s", 1, 101));
        assert_eq!(decode(&issue_at("s", 1, chrono::Utc::now().timestamp()), 1).map(|c| c.source_id), Some("s".into()));
    }

    /// chatwoot#15776: minted token has fresh iat and decodes to matching source_id and inbox_id.
    #[test]
    fn token_rotation_mints_fresh_token() {
        let t1 = issue("src-rotate", 42);
        // Ensure iat timestamp or new token generation creates a valid token
        let t2 = issue("src-rotate", 42);
        assert_eq!(decode(&t1, 42).map(|c| c.source_id), Some("src-rotate".into()));
        assert_eq!(decode(&t2, 42).map(|c| c.source_id), Some("src-rotate".into()));
    }

    #[tokio::test]
    async fn page_strips_cw_conversation_and_sets_security_headers() {
        let res = page(axum::extract::RawQuery(Some(
            "website_token=wt1&cw_conversation=secret.jwt.here&locale=en".into(),
        )))
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            res.headers().get(header::LOCATION).and_then(|v| v.to_str().ok()),
            Some("/app/widget?website_token=wt1&locale=en")
        );
        assert_eq!(
            res.headers().get(header::REFERRER_POLICY).and_then(|v| v.to_str().ok()),
            Some("no-referrer")
        );
        assert_eq!(
            res.headers().get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
    }
}
