//! Instagram channel: OAuth authorization + callback, webhook verification/events, outbound DM delivery.
//! Mirrors chatwoot/app/controllers/api/v1/accounts/instagram/authorizations_controller.rb,
//! instagram/callbacks_controller.rb, and webhooks/instagram_controller.rb.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2_compat::Sha256;
use std::collections::HashMap;

use crate::{
    channels::facebook::{
        find_or_create_social_contact, find_or_create_social_conversation, graph_base,
    },
    common::{ApiError, ApiResult, AppState, CurrentUser, require_account},
    conversations::insert_message,
    events::Event,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/webhooks/instagram", get(verify_webhook).post(events))
        .route("/instagram/callback", get(callback))
}

pub fn account_routes() -> Router<AppState> {
    Router::new().route("/instagram/authorization", post(authorization))
}

fn ig_api_base() -> String {
    std::env::var("INSTAGRAM_GRAPH_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://graph.instagram.com".into())
        .trim_end_matches('/')
        .to_string()
}

fn sign_oauth_state(account_id: i64, return_to: Option<&str>) -> String {
    crate::common::sign_oauth_claims(&json!({
        "account_id": account_id,
        "return_to": return_to.unwrap_or("web"),
        "exp": chrono::Utc::now().timestamp() + 1800,
    }))
}

fn verify_oauth_state(token: &str) -> Option<(i64, String)> {
    let claims = crate::common::verify_oauth_claims(token)?;
    let aid = claims["account_id"].as_i64()?;
    let ret = claims["return_to"].as_str().unwrap_or("web").to_string();
    Some((aid, ret))
}

async fn authorization(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let app_id = std::env::var("INSTAGRAM_APP_ID").unwrap_or_default();
    let base = st.frontend_url.trim_end_matches('/');
    let redirect_uri = format!("{base}/instagram/callback");
    let state = sign_oauth_state(account_id, body["return_to"].as_str());
    let scopes = "instagram_business_basic,instagram_business_manage_messages,instagram_business_manage_comments";
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    ser.append_pair("client_id", &app_id);
    ser.append_pair("redirect_uri", &redirect_uri);
    ser.append_pair("scope", scopes);
    ser.append_pair("enable_fb_login", "0");
    ser.append_pair("force_reauth", "true");
    ser.append_pair("response_type", "code");
    ser.append_pair("state", &state);
    let url = format!("https://www.instagram.com/oauth/authorize?{}", ser.finish());
    Ok(Json(json!({ "success": true, "url": url })))
}

async fn callback(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let base = st.frontend_url.trim_end_matches('/');
    let Some((account_id, return_to)) = q.get("state").and_then(|s| verify_oauth_state(s)) else {
        return Redirect::temporary(&format!("{base}/app/login")).into_response();
    };
    if let Some(err) = q.get("error") {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/new/instagram?error_type={err}"
        ))
        .into_response();
    }
    let Some(code) = q.get("code").filter(|s| !s.is_empty()) else {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/new/instagram?error_type=missing_code"
        ))
        .into_response();
    };

    let app_id = std::env::var("INSTAGRAM_APP_ID").unwrap_or_default();
    let app_secret = std::env::var("INSTAGRAM_APP_SECRET").unwrap_or_default();
    let redirect_uri = format!("{base}/instagram/callback");
    let token_url = std::env::var("INSTAGRAM_OAUTH_TOKEN_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.instagram.com/oauth/access_token".into());

    let form_body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("client_id", &app_id);
        form.append_pair("client_secret", &app_secret);
        form.append_pair("grant_type", "authorization_code");
        form.append_pair("redirect_uri", &redirect_uri);
        form.append_pair("code", code);
        form.finish()
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
            "{base}/app/accounts/{account_id}/settings/inboxes/new/instagram?error_type=token_exchange"
        ))
        .into_response();
    };
    let bytes = resp.bytes().await.unwrap_or_default();
    let tok_json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let access_token = tok_json["access_token"].as_str().unwrap_or("");
    let ig_user_id = tok_json["user_id"]
        .as_str()
        .map(str::to_string)
        .or_else(|| tok_json["user_id"].as_i64().map(|n| n.to_string()))
        .unwrap_or_default();
    if access_token.is_empty() || ig_user_id.is_empty() {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/new/instagram?error_type=invalid_token"
        ))
        .into_response();
    }

    let me_resp = reqwest::Client::new()
        .get(format!("{}/me", ig_api_base()))
        .query(&[
            ("fields", "user_id,username,name"),
            ("access_token", access_token),
        ])
        .send()
        .await
        .ok();
    let me_json: Value = match me_resp {
        Some(r) => {
            serde_json::from_slice(&r.bytes().await.unwrap_or_default()).unwrap_or(Value::Null)
        }
        None => Value::Null,
    };
    let username = me_json["username"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(&ig_user_id);

    let existing_inbox: Option<i64> = sqlx::query_scalar(
        "SELECT i.id::bigint FROM inboxes i
         JOIN channel_instagram c ON c.id = i.channel_id AND i.channel_type = 'Channel::Instagram'
         WHERE c.instagram_id = $1 AND c.account_id = $2 LIMIT 1",
    )
    .bind(&ig_user_id)
    .bind(account_id as i32)
    .fetch_optional(&st.db)
    .await
    .ok()
    .flatten();

    if let Some(iid) = existing_inbox {
        let _ = sqlx::query(
            "UPDATE channel_instagram SET access_token = $2, expires_at = now() + interval '60 days', updated_at = now()
             WHERE instagram_id = $1 AND account_id = $3",
        )
        .bind(&ig_user_id)
        .bind(access_token)
        .bind(account_id as i32)
        .execute(&st.db)
        .await;
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/{iid}"
        ))
        .into_response();
    }

    let channel_id: Result<i64, _> = sqlx::query_scalar(
        "INSERT INTO channel_instagram (account_id, instagram_id, access_token, expires_at, created_at, updated_at)
         VALUES ($1, $2, $3, now() + interval '60 days', now(), now()) RETURNING id",
    )
    .bind(account_id as i32)
    .bind(&ig_user_id)
    .bind(access_token)
    .fetch_one(&st.db)
    .await;

    let Ok(cid) = channel_id else {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/settings/inboxes/new/instagram?error_type=db_error"
        ))
        .into_response();
    };
    let inbox_id: i64 = sqlx::query_scalar(
        "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
         VALUES ($1, $2, $3, 'Channel::Instagram', now(), now()) RETURNING id::bigint",
    )
    .bind(cid)
    .bind(account_id)
    .bind(username)
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);

    st.hub.invalidate_cache(account_id, "inbox");
    if return_to == "onboarding" {
        return Redirect::temporary(&format!(
            "{base}/app/accounts/{account_id}/onboarding/inbox-setup"
        ))
        .into_response();
    }
    Redirect::temporary(&format!(
        "{base}/app/accounts/{account_id}/settings/inboxes/new/{inbox_id}/agents"
    ))
    .into_response()
}

fn verify_ig_sig(secret: &str, sig_header: &str, body: &[u8]) -> bool {
    if secret.is_empty() {
        return false;
    }
    let Some(sig) = sig_header
        .strip_prefix("sha256=")
        .and_then(|s| hex::decode(s).ok())
    else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&sig).is_ok()
}

async fn verify_webhook(Query(q): Query<HashMap<String, String>>) -> Response {
    let mode = q.get("hub.mode").map(String::as_str);
    let token = q.get("hub.verify_token").map(String::as_str);
    let challenge = q.get("hub.challenge").cloned().unwrap_or_default();
    let expected = std::env::var("IG_VERIFY_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("INSTAGRAM_VERIFY_TOKEN")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .or_else(|| std::env::var("FB_VERIFY_TOKEN").ok())
        .unwrap_or_default();
    if mode == Some("subscribe") && !expected.is_empty() && token == Some(expected.as_str()) {
        (StatusCode::OK, challenge).into_response()
    } else {
        StatusCode::FORBIDDEN.into_response()
    }
}

async fn events(State(st): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let secret = std::env::var("INSTAGRAM_APP_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("FB_APP_SECRET").ok())
        .unwrap_or_default();
    let sig = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !verify_ig_sig(&secret, sig, &body) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    if !payload["object"]
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case("instagram"))
    {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    }
    if let Err(e) = process_ig_entries(&st, &payload).await {
        tracing::warn!(error = ?e, "instagram webhook processing failed");
    }
    (StatusCode::OK, Json(json!("ok"))).into_response()
}

async fn process_ig_entries(st: &AppState, payload: &Value) -> Result<(), ApiError> {
    let Some(entries) = payload["entry"].as_array() else {
        return Ok(());
    };
    for entry in entries {
        let Some(ig_id) = entry["id"].as_str() else {
            continue;
        };
        let channel: Option<(i64, i64, bool)> = sqlx::query_as(
            "SELECT i.id::bigint, i.account_id::bigint, i.lock_to_single_conversation
             FROM channel_instagram c
             JOIN inboxes i ON i.channel_id = c.id AND i.account_id = c.account_id AND i.channel_type = 'Channel::Instagram'
             WHERE c.instagram_id = $1 LIMIT 1",
        )
        .bind(ig_id)
        .fetch_optional(&st.db)
        .await?;
        let Some((inbox_id, account_id, lock_single)) = channel else {
            continue;
        };
        let Some(messaging) = entry["messaging"].as_array() else {
            continue;
        };
        for ev in messaging {
            let Some(msg) = ev.get("message").filter(|m| m.is_object()) else {
                continue;
            };
            let is_echo = msg["is_echo"].as_bool().unwrap_or(false);
            let Some(mid) = msg["mid"].as_str().filter(|s| !s.is_empty()) else {
                continue;
            };
            let dup: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM messages WHERE inbox_id = $1 AND source_id = $2)",
            )
            .bind(inbox_id as i32)
            .bind(mid)
            .fetch_one(&st.db)
            .await?;
            if dup {
                continue;
            }
            let sender_id = if is_echo {
                ev["recipient"]["id"].as_str()
            } else {
                ev["sender"]["id"].as_str()
            };
            let Some(ig_sid) = sender_id.filter(|s| !s.is_empty()) else {
                continue;
            };
            let content = msg["text"].as_str().unwrap_or("");
            let (contact_id, ci_id) = find_or_create_social_contact(
                &st.db,
                account_id,
                inbox_id,
                ig_sid,
                &format!("Instagram User {ig_sid}"),
            )
            .await?;
            let conv = find_or_create_social_conversation(
                &st.db,
                account_id,
                inbox_id,
                contact_id,
                ci_id,
                lock_single,
            )
            .await?;
            let msg_type = if is_echo { 1 } else { 0 };
            let sender = (!is_echo).then_some(("Contact", contact_id));
            insert_message(
                st,
                &conv,
                sender,
                content,
                msg_type,
                false,
                0,
                json!({}),
                Some(mid),
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn outbound(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    if let Err(e) = send_outbound(&st, &ev).await {
        tracing::warn!(error = %e, "Instagram outbound failed");
    }
}

async fn send_outbound(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(msg_id) = ev.data["id"].as_i64() else {
        return Ok(());
    };
    let row: Option<(Option<String>, String, String, String)> = sqlx::query_as(
        "SELECT m.content, ci.source_id, c_ig.instagram_id, c_ig.access_token
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
         JOIN contact_inboxes ci ON ci.id = c.contact_inbox_id
         JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::Instagram'
         JOIN channel_instagram c_ig ON c_ig.id = i.channel_id AND c_ig.account_id = i.account_id
         WHERE m.id = $1 AND m.account_id = $2 AND m.message_type = 1 AND NOT COALESCE(m.private, false)",
    )
    .bind(msg_id)
    .bind(ev.account_id)
    .fetch_optional(&st.db)
    .await?;
    let Some((content, recipient_id, ig_id, token)) = row else {
        return Ok(());
    };
    let body_bytes = serde_json::to_vec(&json!({
        "recipient": { "id": recipient_id },
        "message": { "text": content.unwrap_or_default() },
    }))?;
    let resp = reqwest::Client::new()
        .post(format!("{}/v21.0/{ig_id}/messages", graph_base()))
        .bearer_auth(&token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body_bytes)
        .send()
        .await?;
    let status = resp.status();
    let bytes = resp.bytes().await.unwrap_or_default();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if !status.is_success() {
        sqlx::query(
            "UPDATE messages SET status = 3, external_error = $2, updated_at = now() WHERE id = $1",
        )
        .bind(msg_id)
        .bind(
            v["error"]["message"]
                .as_str()
                .unwrap_or("Instagram send failed"),
        )
        .execute(&st.db)
        .await?;
        return Ok(());
    }
    if let Some(mid) = v["message_id"].as_str() {
        sqlx::query(
            "UPDATE messages SET source_id = $2, status = 1, updated_at = now() WHERE id = $1",
        )
        .bind(msg_id)
        .bind(mid)
        .execute(&st.db)
        .await?;
    }
    Ok(())
}
