//! TikTok, Twitter/X, Google & Microsoft OAuth authorizations, callbacks, webhooks, and outbound delivery.
//! Mirrors chatwoot/app/controllers/{tiktok,twitter,google,microsoft}/callbacks_controller.rb,
//! api/v1/accounts/{tiktok,twitter,google,microsoft}/authorizations_controller.rb,
//! webhooks/tiktok_controller.rb, and api/v1/webhooks_controller.rb.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2_compat::Sha256;
use std::collections::HashMap;

use crate::{
    ar_encryption,
    channels::facebook::{find_or_create_social_contact, find_or_create_social_conversation},
    common::{ApiResult, AppState, CurrentUser, require_account},
    conversations::insert_message,
    events::Event,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/webhooks/tiktok", post(tiktok_webhook))
        .route("/webhooks/twitter", get(twitter_crc).post(twitter_events))
        .route("/tiktok/callback", get(tiktok_callback))
        .route("/twitter/callback", get(twitter_callback))
        .route("/google/callback", get(google_callback))
        .route("/microsoft/callback", get(microsoft_callback))
}

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/tiktok/authorization", post(tiktok_auth))
        .route("/twitter/authorization", post(twitter_auth))
        .route("/google/authorization", post(google_auth))
        .route("/microsoft/authorization", post(microsoft_auth))
}

fn sign_state(account_id: i64, extra: &str) -> String {
    crate::common::sign_oauth_claims(&json!({
        "account_id": account_id,
        "extra": extra,
        "exp": chrono::Utc::now().timestamp() + 1800,
    }))
}

fn verify_state(token: &str) -> Option<(i64, String)> {
    let claims = crate::common::verify_oauth_claims(token)?;
    Some((
        claims["account_id"].as_i64()?,
        claims["extra"].as_str().unwrap_or("").to_string(),
    ))
}

// ---------- OAuth initiation ----------

async fn tiktok_auth(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let client_key = std::env::var("TIKTOK_APP_ID").unwrap_or_default();
    let base = st.frontend_url.trim_end_matches('/');
    let state = sign_state(account_id, body["return_to"].as_str().unwrap_or("web"));
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    ser.append_pair("client_key", &client_key);
    ser.append_pair("response_type", "code");
    ser.append_pair("scope", "user.info.basic,im.message.send,im.message.read");
    ser.append_pair("redirect_uri", &format!("{base}/tiktok/callback"));
    ser.append_pair("state", &state);
    Ok(Json(json!({
        "success": true,
        "url": format!("https://www.tiktok.com/v2/auth/authorize/?{}", ser.finish()),
    })))
}

async fn twitter_auth(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let base = st.frontend_url.trim_end_matches('/');
    let state = sign_state(account_id, "twitter");
    let twitter_api = std::env::var("TWITTER_API_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.twitter.com".into());
    Ok(Json(json!({
        "success": true,
        "url": format!("{}/oauth/authorize?oauth_token={state}&callback={base}/twitter/callback", twitter_api.trim_end_matches('/')),
    })))
}

async fn google_auth(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let client_id = std::env::var("GOOGLE_OAUTH_CLIENT_ID").unwrap_or_default();
    let base = st.frontend_url.trim_end_matches('/');
    let state = sign_state(account_id, body["email"].as_str().unwrap_or(""));
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    ser.append_pair("client_id", &client_id);
    ser.append_pair("redirect_uri", &format!("{base}/google/callback"));
    ser.append_pair("response_type", "code");
    ser.append_pair("scope", "https://mail.google.com/ email profile");
    ser.append_pair("access_type", "offline");
    ser.append_pair("prompt", "consent");
    ser.append_pair("state", &state);
    Ok(Json(json!({
        "success": true,
        "url": format!("https://accounts.google.com/o/oauth2/v2/auth?{}", ser.finish()),
    })))
}

async fn microsoft_auth(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let client_id = std::env::var("AZURE_APP_ID").unwrap_or_default();
    let base = st.frontend_url.trim_end_matches('/');
    let state = sign_state(account_id, body["email"].as_str().unwrap_or(""));
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    ser.append_pair("client_id", &client_id);
    ser.append_pair("redirect_uri", &format!("{base}/microsoft/callback"));
    ser.append_pair("response_type", "code");
    ser.append_pair("scope", "offline_access https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send User.Read");
    ser.append_pair("prompt", "select_account");
    ser.append_pair("state", &state);
    Ok(Json(json!({
        "success": true,
        "url": format!("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?{}", ser.finish()),
    })))
}

// ---------- OAuth callbacks ----------

async fn tiktok_callback(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let base = st.frontend_url.trim_end_matches('/');
    let Some((account_id, _)) = q.get("state").and_then(|s| verify_state(s)) else {
        return Redirect::temporary(&format!("{base}/app/login")).into_response();
    };
    let Some(code) = q.get("code").filter(|s| !s.is_empty()) else {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/new/tiktok?error_type=missing_code"
        ))
        .into_response();
    };
    let token_url = std::env::var("TIKTOK_TOKEN_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://open.tiktokapis.com/v2/oauth/token/".into());
    let form_body = {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        ser.append_pair(
            "client_key",
            &std::env::var("TIKTOK_APP_ID").unwrap_or_default(),
        );
        ser.append_pair(
            "client_secret",
            &std::env::var("TIKTOK_APP_SECRET").unwrap_or_default(),
        );
        ser.append_pair("code", code);
        ser.append_pair("grant_type", "authorization_code");
        ser.append_pair("redirect_uri", &format!("{base}/tiktok/callback"));
        ser.finish()
    };

    let Ok(resp) = reqwest::Client::new()
        .post(&token_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_body)
        .send()
        .await
    else {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/new/tiktok?error_type=token_exchange"
        ))
        .into_response();
    };
    let v: Value =
        serde_json::from_slice(&resp.bytes().await.unwrap_or_default()).unwrap_or(Value::Null);
    let access_token = v["access_token"].as_str().unwrap_or("");
    let refresh_token = v["refresh_token"].as_str().unwrap_or("");
    let biz_id = v["open_id"].as_str().unwrap_or("");
    if access_token.is_empty() || biz_id.is_empty() {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/new/tiktok?error_type=invalid_token"
        ))
        .into_response();
    }

    let cid: i64 = sqlx::query_scalar(
        "INSERT INTO channel_tiktok (account_id, business_id, access_token, expires_at, refresh_token, refresh_token_expires_at, created_at, updated_at)
         VALUES ($1, $2, $3, now() + interval '1 day', $4, now() + interval '365 days', now(), now()) RETURNING id",
    )
    .bind(account_id as i32)
    .bind(biz_id)
    .bind(access_token)
    .bind(refresh_token)
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);

    let inbox_id: i64 = sqlx::query_scalar(
        "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
         VALUES ($1, $2, $3, 'Channel::Tiktok', now(), now()) RETURNING id::bigint",
    )
    .bind(cid)
    .bind(account_id)
    .bind(format!("TikTok {biz_id}"))
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);
    st.hub.invalidate_cache(account_id, "inbox");
    Redirect::temporary(&format!(
        "{base}/app/accounts/{account_id}/settings/inboxes/new/{inbox_id}/agents"
    ))
    .into_response()
}

async fn twitter_callback(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let base = st.frontend_url.trim_end_matches('/');
    let Some((account_id, _)) = q
        .get("state")
        .and_then(|s| verify_state(s))
        .or_else(|| q.get("oauth_token").and_then(|s| verify_state(s)))
    else {
        return Redirect::temporary(&format!("{base}/app/login")).into_response();
    };
    let profile_id = q
        .get("user_id")
        .cloned()
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
    let access_token = q.get("oauth_token").cloned().unwrap_or_default();
    let token_secret = q.get("oauth_verifier").cloned().unwrap_or_default();
    let cid: i64 = sqlx::query_scalar(
        "INSERT INTO channel_twitter_profiles (profile_id, twitter_access_token, twitter_access_token_secret, account_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, now(), now()) RETURNING id",
    )
    .bind(&profile_id)
    .bind(&access_token)
    .bind(&token_secret)
    .bind(account_id as i32)
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);
    let inbox_id: i64 = sqlx::query_scalar(
        "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
         VALUES ($1, $2, $3, 'Channel::TwitterProfile', now(), now()) RETURNING id::bigint",
    )
    .bind(cid)
    .bind(account_id)
    .bind(format!("X {profile_id}"))
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);
    st.hub.invalidate_cache(account_id, "inbox");
    Redirect::temporary(&format!(
        "{base}/app/accounts/{account_id}/settings/inboxes/new/{inbox_id}/agents"
    ))
    .into_response()
}

async fn google_callback(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    oauth_email_callback(
        &st,
        q,
        "google",
        "https://oauth2.googleapis.com/token",
        "imap.gmail.com",
        993,
        "smtp.gmail.com",
        587,
    )
    .await
}

async fn microsoft_callback(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    oauth_email_callback(
        &st,
        q,
        "microsoft",
        "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        "outlook.office365.com",
        993,
        "smtp.office365.com",
        587,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn oauth_email_callback(
    st: &AppState,
    q: HashMap<String, String>,
    provider: &str,
    default_token_url: &str,
    imap_host: &str,
    imap_port: i32,
    smtp_host: &str,
    smtp_port: i32,
) -> Response {
    let base = st.frontend_url.trim_end_matches('/');
    let Some((account_id, hint_email)) = q.get("state").and_then(|s| verify_state(s)) else {
        return Redirect::temporary(&format!("{base}/app/login")).into_response();
    };
    let Some(code) = q.get("code").filter(|s| !s.is_empty()) else {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/new/email"
        ))
        .into_response();
    };
    let env_key = format!("{}_OAUTH_TOKEN_URL", provider.to_ascii_uppercase());
    let token_url = std::env::var(&env_key)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_token_url.to_string());
    let (cid_var, csec_var) = if provider == "google" {
        ("GOOGLE_OAUTH_CLIENT_ID", "GOOGLE_OAUTH_CLIENT_SECRET")
    } else {
        ("AZURE_APP_ID", "AZURE_APP_SECRET")
    };
    let form_body = {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        ser.append_pair("client_id", &std::env::var(cid_var).unwrap_or_default());
        ser.append_pair(
            "client_secret",
            &std::env::var(csec_var).unwrap_or_default(),
        );
        ser.append_pair("code", code);
        ser.append_pair("grant_type", "authorization_code");
        ser.append_pair("redirect_uri", &format!("{base}/{provider}/callback"));
        ser.finish()
    };

    let Ok(resp) = reqwest::Client::new()
        .post(&token_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_body)
        .send()
        .await
    else {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/new/email"
        ))
        .into_response();
    };
    let v: Value =
        serde_json::from_slice(&resp.bytes().await.unwrap_or_default()).unwrap_or(Value::Null);
    let access_token = v["access_token"].as_str().unwrap_or("");
    let refresh_token = v["refresh_token"].as_str().unwrap_or("");
    let email = v["email"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| (!hint_email.is_empty()).then_some(hint_email))
        .unwrap_or_else(|| format!("{provider}@oauth.local"));
    let cfg = json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "provider": provider,
    });
    let enc_cfg = ar_encryption::encrypt(&cfg.to_string());
    let cid: i64 = sqlx::query_scalar(
        "INSERT INTO channel_email (account_id, email, forward_to_email, imap_enabled, imap_address, imap_port, imap_login, imap_enable_ssl, smtp_enabled, smtp_address, smtp_port, smtp_login, provider, provider_config, created_at, updated_at)
         VALUES ($1, $2, $3, true, $4, $5, $2, true, true, $6, $7, $2, $8, $9::jsonb, now(), now())
         ON CONFLICT (email) DO UPDATE SET provider_config = EXCLUDED.provider_config, updated_at = now()
         RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(&email)
    .bind(format!("{}@localhost", uuid::Uuid::new_v4().simple()))
    .bind(imap_host)
    .bind(imap_port)
    .bind(smtp_host)
    .bind(smtp_port)
    .bind(provider)
    .bind(json!({ "encrypted": enc_cfg }))
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);
    let inbox_id: i64 = sqlx::query_scalar(
        "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
         VALUES ($1, $2, $3, 'Channel::Email', now(), now()) RETURNING id::bigint",
    )
    .bind(cid as i32)
    .bind(account_id)
    .bind(&email)
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);
    st.hub.invalidate_cache(account_id, "inbox");
    Redirect::temporary(&format!(
        "{base}/app/accounts/{account_id}/settings/inboxes/new/{inbox_id}/agents"
    ))
    .into_response()
}

// ---------- TikTok & Twitter Webhooks + Outbound ----------

fn verify_tiktok_sig(secret: &str, header: &str, body: &[u8], now_ts: i64) -> bool {
    if secret.is_empty() {
        return false;
    }
    let mut ts_val: Option<i64> = None;
    let mut sig_val: Option<String> = None;
    for part in header.split(',') {
        if let Some((k, v)) = part.trim().split_once('=') {
            match k.trim() {
                "t" => ts_val = v.trim().parse().ok(),
                "s" => sig_val = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    let (Some(ts), Some(sig)) = (ts_val, sig_val) else {
        return false;
    };
    if (now_ts - ts).abs() > 300 {
        return false;
    }
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(format!("{ts}.").as_bytes());
    mac.update(body);
    let Ok(expected) = hex::decode(sig) else {
        return false;
    };
    mac.verify_slice(&expected).is_ok()
}

async fn tiktok_webhook(State(st): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let secret = std::env::var("TIKTOK_APP_SECRET").unwrap_or_default();
    let hdr = headers
        .get("tiktok-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !verify_tiktok_sig(&secret, hdr, &body, chrono::Utc::now().timestamp()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(ev) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if let Some(biz_id) = ev["user_openid"].as_str()
        && let Ok(Some((inbox_id, account_id, lock_single))) = sqlx::query_as::<_, (i64, i64, bool)>(
            "SELECT i.id::bigint, i.account_id::bigint, i.lock_to_single_conversation
             FROM channel_tiktok t
             JOIN inboxes i ON i.channel_id = t.id AND i.account_id = t.account_id AND i.channel_type = 'Channel::Tiktok'
             WHERE t.business_id = $1 LIMIT 1",
        )
        .bind(biz_id)
        .fetch_optional(&st.db)
        .await
    {
        let content_obj = &ev["content"];
        let text = content_obj["text"].as_str().unwrap_or("");
        let sender = content_obj["from_user_id"].as_str().unwrap_or("tiktok-user");
        let msg_id = content_obj["message_id"].as_str().unwrap_or("");
        let is_echo = ev["event"].as_str() == Some("im_send_msg");
        if let Ok((cid, ci_id)) = find_or_create_social_contact(&st.db, account_id, inbox_id, sender, sender).await
            && let Ok(conv) = find_or_create_social_conversation(&st.db, account_id, inbox_id, cid, ci_id, lock_single).await
        {
            let _ = insert_message(
                &st,
                &conv,
                (!is_echo).then_some(("Contact", cid)),
                text,
                if is_echo { 1 } else { 0 },
                false,
                0,
                json!({}),
                (!msg_id.is_empty()).then_some(msg_id),
            )
            .await;
        }
    }
    StatusCode::OK.into_response()
}

pub(crate) fn compute_twitter_crc(secret: &str, crc_token: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(crc_token.as_bytes());
    format!("sha256={}", STANDARD.encode(mac.finalize().into_bytes()))
}

async fn twitter_crc(Query(q): Query<HashMap<String, String>>) -> Response {
    let token = q.get("crc_token").map(String::as_str).unwrap_or("");
    let secret = std::env::var("TWITTER_CONSUMER_SECRET").unwrap_or_default();
    Json(json!({ "response_token": compute_twitter_crc(&secret, token) })).into_response()
}

async fn twitter_events(State(st): State<AppState>, Json(payload): Json<Value>) -> StatusCode {
    let Some(for_user) = payload["for_user_id"].as_str() else {
        return StatusCode::OK;
    };
    let Ok(Some((inbox_id, account_id, lock_single))) = sqlx::query_as::<_, (i64, i64, bool)>(
        "SELECT i.id::bigint, i.account_id::bigint, i.lock_to_single_conversation
         FROM channel_twitter_profiles t
         JOIN inboxes i ON i.channel_id = t.id AND i.account_id = t.account_id AND i.channel_type = 'Channel::TwitterProfile'
         WHERE t.profile_id = $1 LIMIT 1",
    )
    .bind(for_user)
    .fetch_optional(&st.db)
    .await else {
        return StatusCode::OK;
    };
    if let Some(dms) = payload["direct_message_events"].as_array() {
        for dm in dms {
            let sender_id = dm["message_create"]["sender_id"].as_str().unwrap_or("");
            if sender_id == for_user || sender_id.is_empty() {
                continue;
            }
            let text = dm["message_create"]["message_data"]["text"]
                .as_str()
                .unwrap_or("");
            let mid = dm["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            if let Ok((cid, ci_id)) = find_or_create_social_contact(
                &st.db,
                account_id,
                inbox_id,
                sender_id,
                &format!("X User {sender_id}"),
            )
            .await
                && let Ok(conv) = find_or_create_social_conversation(
                    &st.db,
                    account_id,
                    inbox_id,
                    cid,
                    ci_id,
                    lock_single,
                )
                .await
            {
                let _ = insert_message(
                    &st,
                    &conv,
                    Some(("Contact", cid)),
                    text,
                    0,
                    false,
                    0,
                    json!({}),
                    mid.as_deref(),
                )
                .await;
            }
        }
    }
    StatusCode::OK
}

pub async fn tiktok_outbound(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    let Some(msg_id) = ev.data["id"].as_i64() else {
        return;
    };
    let row: Option<(Option<String>, String, String)> = sqlx::query_as(
        "SELECT m.content, ci.source_id, t.access_token
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
         JOIN contact_inboxes ci ON ci.id = c.contact_inbox_id
         JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::Tiktok'
         JOIN channel_tiktok t ON t.id = i.channel_id AND t.account_id = i.account_id
         WHERE m.id = $1 AND m.account_id = $2 AND m.message_type = 1 AND NOT COALESCE(m.private, false)",
    )
    .bind(msg_id)
    .bind(ev.account_id)
    .fetch_optional(&st.db)
    .await
    .ok()
    .flatten();
    if let Some((content, recipient, token)) = row {
        let api_base = std::env::var("TIKTOK_API_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://open.tiktokapis.com".into());
        let body = serde_json::to_vec(&json!({
            "recipient": { "open_id": recipient },
            "message": { "text": content.unwrap_or_default() },
        }))
        .unwrap_or_default();
        let _ = reqwest::Client::new()
            .post(format!(
                "{}/v2/business/message/send/",
                api_base.trim_end_matches('/')
            ))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await;
    }
}

pub async fn twitter_outbound(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    let Some(msg_id) = ev.data["id"].as_i64() else {
        return;
    };
    let row: Option<(Option<String>, String, String)> = sqlx::query_as(
        "SELECT m.content, ci.source_id, t.twitter_access_token
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
         JOIN contact_inboxes ci ON ci.id = c.contact_inbox_id
         JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::TwitterProfile'
         JOIN channel_twitter_profiles t ON t.id = i.channel_id AND t.account_id = i.account_id
         WHERE m.id = $1 AND m.account_id = $2 AND m.message_type = 1 AND NOT COALESCE(m.private, false)",
    )
    .bind(msg_id)
    .bind(ev.account_id)
    .fetch_optional(&st.db)
    .await
    .ok()
    .flatten();
    if let Some((content, recipient, token)) = row {
        let api_base = std::env::var("TWITTER_API_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://api.twitter.com".into());
        let body = serde_json::to_vec(&json!({
            "event": {
                "type": "message_create",
                "message_create": {
                    "target": { "recipient_id": recipient },
                    "message_data": { "text": content.unwrap_or_default() },
                }
            }
        }))
        .unwrap_or_default();
        let _ = reqwest::Client::new()
            .post(format!(
                "{}/1.1/direct_messages/events/new.json",
                api_base.trim_end_matches('/')
            ))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twitter_crc_and_tiktok_sig_match_spec() {
        let crc = compute_twitter_crc("secret", "abc");
        assert!(crc.starts_with("sha256="));

        let secret = "tt-secret";
        let body = br#"{"event":"im_receive_msg"}"#;
        let now = 1_700_000_000i64;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{now}.").as_bytes());
        mac.update(body);
        let hdr = format!("t={now},s={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_tiktok_sig(secret, &hdr, body, now + 10));
        assert!(!verify_tiktok_sig(secret, &hdr, body, now + 600));
    }
}
