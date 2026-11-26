//! devise_token_auth compatible auth + profile + account endpoints.
use std::collections::HashMap;

use axum::{
    Json,
    extract::{FromRequest, Multipart, Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use base64::Engine;
use rand::Rng;
use serde_json::{Map, Value, json};
use sqlx::PgPool;

use crate::common::*;

const TOKEN_LIFESPAN_SECS: i64 = 60 * 24 * 3600; // devise_token_auth: 2.months
const MAX_DEVICES: usize = 25;

pub async fn notification_subscription_destroy(
    State(db): State<PgPool>,
    user: CurrentUser,
    req: Request,
) -> ApiResult<StatusCode> {
    let mut query_token: Option<String> = None;
    for k in ["push_token", "endpoint", "identifier"] {
        if let Some(v) = req.uri().query().and_then(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .find(|(qk, _)| qk == k)
                .map(|(_, qv)| qv.into_owned())
        }) {
            query_token = Some(v);
            break;
        }
    }
    let content_type = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let body = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .map_err(|_| ApiError::bad("Invalid request body"))?;
    let body_token = if content_type.starts_with("application/json") && !body.is_empty() {
        let value: Value =
            serde_json::from_slice(&body).map_err(|_| ApiError::bad("Invalid JSON"))?;
        value
            .get("push_token")
            .or_else(|| value.get("endpoint"))
            .or_else(|| value.get("identifier"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    } else if content_type.starts_with("application/x-www-form-urlencoded") {
        url::form_urlencoded::parse(&body)
            .find(|(k, _)| matches!(k.as_ref(), "push_token" | "endpoint" | "identifier"))
            .map(|(_, v)| v.into_owned())
    } else {
        None
    };
    if let Some(token) = body_token.or(query_token).filter(|t| !t.trim().is_empty()) {
        sqlx::query(
            "DELETE FROM notification_subscriptions
             WHERE id IN (
                 SELECT id FROM notification_subscriptions
                 WHERE user_id = $1
                   AND (identifier = $2
                        OR subscription_attributes->>'push_token' = $2
                        OR subscription_attributes->>'endpoint' = $2)
                 ORDER BY id LIMIT 1
             )",
        )
        .bind(user.id)
        .bind(token)
        .execute(&db)
        .await?;
    }
    Ok(StatusCode::OK)
}

/// POST /api/v1/notification_subscriptions
pub async fn notification_subscription_create(
    State(db): State<PgPool>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let sub = body
        .get("notification_subscription")
        .filter(|v| v.is_object())
        .unwrap_or(&body);
    let sub_type = match &sub["subscription_type"] {
        Value::Number(n) => n.as_i64().unwrap_or(1) as i32,
        Value::String(s) if s == "fcm" => 2,
        _ => 1,
    };
    let attrs = sub
        .get("subscription_attributes")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let identifier = attrs
        .get("push_token")
        .or_else(|| attrs.get("endpoint"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let existing_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM notification_subscriptions WHERE identifier = $1 LIMIT 1",
    )
    .bind(&identifier)
    .fetch_optional(&db)
    .await?;

    let row: (i64, i64, i32, Value, Option<String>) = if let Some(eid) = existing_id {
        sqlx::query_as(
            "UPDATE notification_subscriptions
             SET user_id = $2, subscription_type = $3, subscription_attributes = $4, updated_at = now()
             WHERE id = $1
             RETURNING id, user_id, subscription_type, subscription_attributes, identifier",
        )
        .bind(eid)
        .bind(user.id)
        .bind(sub_type)
        .bind(attrs)
        .fetch_one(&db)
        .await?
    } else {
        sqlx::query_as(
            "INSERT INTO notification_subscriptions
             (user_id, subscription_type, subscription_attributes, identifier, created_at, updated_at)
             VALUES ($1, $2, $3, $4, now(), now())
             RETURNING id, user_id, subscription_type, subscription_attributes, identifier",
        )
        .bind(user.id)
        .bind(sub_type)
        .bind(attrs)
        .bind(&identifier)
        .fetch_one(&db)
        .await?
    };

    Ok(Json(json!({
        "id": row.0,
        "user_id": row.1,
        "subscription_type": row.2,
        "subscription_attributes": row.3,
        "identifier": row.4,
    })))
}

/// GET /api/v1/profile/sessions
pub async fn sessions_index(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    let tokens: Option<Value> = sqlx::query_scalar("SELECT tokens::jsonb FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_optional(&db)
        .await?
        .flatten();
    let now_ts = chrono::Utc::now().timestamp();
    let mut active_clients = Vec::new();
    if let Some(Value::Object(map)) = &tokens {
        for (client_id, entry) in map {
            let expiry = entry.get("expiry").and_then(Value::as_i64).unwrap_or(0);
            if expiry > now_ts {
                active_clients.push(client_id.clone());
            }
        }
    }
    if active_clients.is_empty() {
        return Ok(Json(Value::Array(Vec::new())));
    }
    for cid in &active_clients {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM user_sessions WHERE user_id = $1 AND client_id = $2)",
        )
        .bind(user.id)
        .bind(cid)
        .fetch_one(&db)
        .await?;
        if !exists {
            sqlx::query(
                "INSERT INTO user_sessions (user_id, client_id, browser_name, last_activity_at, created_at, updated_at)
                 VALUES ($1, $2, 'Web Browser', now(), now(), now())",
            )
            .bind(user.id)
            .bind(cid)
            .execute(&db)
            .await?;
        }
    }
    let rows: Vec<(
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<chrono::NaiveDateTime>,
        chrono::NaiveDateTime,
    )> = sqlx::query_as(
        "SELECT id, client_id, browser_name, browser_version, device_name, platform_name, platform_version,
                ip_address, city, country, country_code, last_activity_at, created_at
         FROM user_sessions
         WHERE user_id = $1 AND client_id = ANY($2)
         ORDER BY last_activity_at DESC NULLS LAST, id DESC",
    )
    .bind(user.id)
    .bind(&active_clients)
    .fetch_all(&db)
    .await?;

    let sessions: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let current = user.client.as_deref() == Some(r.1.as_str());
            json!({
                "id": r.0,
                "browser_name": r.2.unwrap_or_else(|| "Web Browser".into()),
                "browser_version": r.3,
                "device_name": r.4,
                "platform_name": r.5,
                "platform_version": r.6,
                "ip_address": r.7,
                "city": r.8,
                "country": r.9,
                "country_code": r.10,
                "last_activity_at": ts(r.11.or(Some(r.12))),
                "created_at": ts(Some(r.12)),
                "current": current,
            })
        })
        .collect();
    Ok(Json(Value::Array(sessions)))
}

/// DELETE /api/v1/profile/trusted_devices
pub async fn trusted_devices_destroy(
    State(db): State<PgPool>,
    user: CurrentUser,
) -> ApiResult<StatusCode> {
    sqlx::query(
        "UPDATE users SET custom_attributes = COALESCE(custom_attributes, '{}'::jsonb) - 'trusted_devices', updated_at = now()
         WHERE id = $1",
    )
    .bind(user.id)
    .execute(&db)
    .await?;
    Ok(StatusCode::OK)
}

pub(crate) fn random_token(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill(&mut b[..]);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Same check as devise_token_auth `token_is_current?` / `token_can_be_reused?` minus the reuse window.
pub fn token_valid(tokens: Option<&Value>, client: &str, token: &str) -> bool {
    let Some(entry) = tokens.and_then(|t| t.get(client)) else {
        return false;
    };
    let expiry = entry.get("expiry").and_then(Value::as_i64).unwrap_or(0);
    if expiry < chrono::Utc::now().timestamp() {
        return false;
    }
    let check = |k: &str| {
        entry
            .get(k)
            .and_then(Value::as_str)
            .is_some_and(|h| bcrypt_cached(token, h))
    };
    check("token") || check("last_token")
}

/// bcrypt cost 10 is ~50 ms, and every dashboard request carries a devise token: without this, each
/// request paid it on the async runtime. Cache successful checks keyed by sha256(token, stored hash), so a
/// rotated or revoked token (new stored hash) is re-verified and a wrong token never hits the cache.
/// Same idea as devise_token_auth's `token_validation_response` bounded FIFO cache.
fn bcrypt_cached(token: &str, hash: &str) -> bool {
    use sha2::{Digest, Sha256};
    use std::{
        collections::{HashSet, VecDeque},
        sync::Mutex,
    };
    const CAP: usize = 10_000;
    static OK: std::sync::LazyLock<Mutex<(HashSet<[u8; 32]>, VecDeque<[u8; 32]>)>> =
        std::sync::LazyLock::new(Default::default);
    let key: [u8; 32] = Sha256::new()
        .chain_update(token)
        .chain_update([0])
        .chain_update(hash)
        .finalize()
        .into();
    if OK.lock().map(|g| g.0.contains(&key)).unwrap_or(false) {
        return true;
    }
    let ok = bcrypt::verify(token, hash).unwrap_or(false);
    if ok {
        if let Ok(mut g) = OK.lock() {
            if !g.0.contains(&key) {
                if g.1.len() >= CAP
                    && let Some(oldest) = g.1.pop_front()
                {
                    g.0.remove(&oldest);
                }
                g.0.insert(key);
                g.1.push_back(key);
            }
        }
    }
    ok
}

/// Account feature flags -> {name: bool} using config/features.yml bit order per column.
pub fn enabled_features(flags: i64, flags_ext: i64) -> Map<String, Value> {
    static FEATURES: std::sync::LazyLock<Vec<(String, String)>> = std::sync::LazyLock::new(|| {
        let v: Vec<Value> = serde_json::from_str(include_str!("../features.json")).unwrap();
        v.into_iter()
            .map(|f| {
                (
                    f["name"].as_str().unwrap().to_string(),
                    f["column"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    });
    let (mut i0, mut i1) = (0u32, 0u32);
    let mut out = Map::new();
    for (name, col) in FEATURES.iter() {
        let on = if col == "feature_flags" {
            let on = flags >> i0 & 1 == 1;
            i0 += 1;
            on
        } else {
            let on = flags_ext >> i1 & 1 == 1;
            i1 += 1;
            on
        };
        if on {
            out.insert(name.clone(), Value::Bool(true));
        }
    }
    out
}

/// Default feature_flags bitset for a new account (features.yml `enabled: true`).
pub fn default_feature_flags() -> (i64, i64) {
    let v: Vec<Value> = serde_json::from_str(include_str!("../features.json")).unwrap();
    let (mut a, mut b, mut i0, mut i1) = (0i64, 0i64, 0, 0);
    for f in v {
        let on = f["enabled"].as_bool().unwrap_or(false);
        if f["column"] == "feature_flags" {
            if on {
                a |= 1 << i0;
            }
            i0 += 1;
        } else {
            if on {
                b |= 1 << i1;
            }
            i1 += 1;
        }
    }
    (a, b)
}

/// Toggle a named feature bit on (flags, feature_flags_ext_1) using config/features.yml order.
pub fn set_feature(flags: &mut i64, flags_ext: &mut i64, name: &str, enabled: bool) -> bool {
    let v: Vec<Value> = serde_json::from_str(include_str!("../features.json")).unwrap();
    let (mut i0, mut i1) = (0u32, 0u32);
    for f in v {
        let col = f["column"].as_str().unwrap_or("feature_flags");
        if f["name"].as_str() == Some(name) {
            let (slot, bit) = if col == "feature_flags" {
                (flags, 1i64 << i0)
            } else {
                (flags_ext, 1i64 << i1)
            };
            if enabled {
                *slot |= bit;
            } else {
                *slot &= !bit;
            }
            return true;
        }
        if col == "feature_flags" {
            i0 += 1;
        } else {
            i1 += 1;
        }
    }
    false
}

pub fn role_name(r: i32) -> &'static str {
    if r == 1 { "administrator" } else { "agent" }
}

pub fn availability_name(a: i32) -> &'static str {
    match a {
        1 => "offline",
        2 => "busy",
        _ => "online",
    }
}

/// Mirrors api/v1/models/_user.json.jbuilder for the authenticated user.
pub async fn user_json(db: &PgPool, user_id: i64) -> Result<Value, ApiError> {
    let u: (i64, String, Option<String>, Option<String>, Option<String>, String, Option<String>, Option<Value>, Option<Value>, String, Option<String>, Option<chrono::NaiveDateTime>) =
        sqlx::query_as(
            "SELECT id::bigint, name, display_name, email, message_signature, provider, pubsub_token,
                    ui_settings, custom_attributes, uid, type, confirmed_at FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_one(db)
        .await?;
    let accounts: Vec<(i64, String, i32, Option<chrono::NaiveDateTime>, i32, i32, bool, bool, bool, Option<i64>, i64, i64, Option<Value>)> =
        sqlx::query_as(&format!(
            "SELECT a.id::bigint, a.name, COALESCE(a.status,0), au.active_at, COALESCE(au.role,0), au.availability,
                    au.auto_offline, {}, au.inviter_id, a.feature_flags, a.feature_flags_ext_1,
                    a.custom_attributes
             FROM account_users au JOIN accounts a ON a.id = au.account_id
             WHERE au.user_id = $1 ORDER BY au.active_at DESC NULLS LAST, au.id",
            crate::common::work_pref_cols("au")
        ))
        .bind(user_id)
        .fetch_all(db)
        .await?;
    let access_token: Option<(String,)> =
        sqlx::query_as("SELECT token FROM access_tokens WHERE owner_type='User' AND owner_id=$1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    let active = accounts.first();
    let api_enabled = accounts
        .iter()
        .any(|a| enabled_features(a.10, a.11).contains_key("api_and_webhooks"));
    let presence = crate::cable::presence_snapshot();
    let accs: Vec<Value> = accounts
        .iter()
        .map(|a| {
            let online = presence.is_online(a.0, user_id);
            json!({
                "id": a.0, "name": a.1, "status": if a.2 == 1 { "suspended" } else { "active" },
                "onboarding_step": a.12.as_ref().and_then(|c| c.get("onboarding_step")).cloned(),
                "shopify_integration": false, "shopify_connected": false,
                "active_at": a.3, "role": role_name(a.4), "permissions": [role_name(a.4)],
                "availability": availability_name(a.5),
                "availability_status": if online { availability_name(a.5) } else { "offline" },
                "auto_offline": a.6, "accepts_conversations": a.7, "accepts_calls": a.8,
                "api_and_webhooks": enabled_features(a.10, a.11).contains_key("api_and_webhooks"),
            })
        })
        .collect();
    let name = u.1.clone();
    let mut v = json!({
        "access_token": if api_enabled { access_token.map(|t| t.0).unwrap_or_default() } else { String::new() },
        "account_id": active.map(|a| a.0),
        "available_name": u.2.clone().filter(|s| !s.is_empty()).unwrap_or(name),
        "avatar_url": crate::storage::avatar_url(db, "User", user_id).await,
        "confirmed": u.11.is_some(),
        "display_name": u.2, "message_signature": u.4, "email": u.3, "id": u.0,
        "inviter_id": active.and_then(|a| a.9), "name": u.1, "provider": u.5,
        "pubsub_token": u.6, "role": active.map(|a| role_name(a.4)),
        "ui_settings": u.7.unwrap_or(json!({})), "uid": u.9, "type": u.10,
        "accounts": accs,
    });
    if let Some(ca) = u.8.filter(|c| c.as_object().is_some_and(|o| !o.is_empty())) {
        v["custom_attributes"] = ca;
    }
    Ok(v)
}

pub(crate) fn auth_headers(uid: &str, client: &str, token: &str, expiry: i64) -> HeaderMap {
    let mut h = HeaderMap::new();
    let mut set = |k: &'static str, v: &str| {
        h.insert(k, HeaderValue::from_str(v).unwrap());
    };
    set("access-token", token);
    set("token-type", "Bearer");
    set("client", client);
    set("expiry", &expiry.to_string());
    set("uid", uid);
    h
}

/// Failed-login throttle per normalized email: 10 failures per 15 minutes lock further attempts.
/// Uses in-memory tracking on single-node deployments, and PostgreSQL (`pulsar_rate_limits`)
/// when `PULSAR_MULTI_NODE=1`.
/// Generic 15-minute sliding/bucket rate limit check backed by in-memory map or `pulsar_rate_limits`.
pub(crate) async fn check_rate_limit(db: &PgPool, key: &str, record_hit: bool) -> usize {
    if std::env::var("PULSAR_MULTI_NODE").as_deref() == Ok("1") {
        if record_hit {
            let hits: i32 = sqlx::query_scalar(
                "INSERT INTO pulsar_rate_limits (bucket, hits, window_start)
                 VALUES ($1, 1, now())
                 ON CONFLICT (bucket) DO UPDATE SET
                   hits = CASE WHEN pulsar_rate_limits.window_start < now() - interval '15 minutes' THEN 1 ELSE pulsar_rate_limits.hits + 1 END,
                   window_start = CASE WHEN pulsar_rate_limits.window_start < now() - interval '15 minutes' THEN now() ELSE pulsar_rate_limits.window_start END
                 RETURNING hits",
            )
            .bind(key)
            .fetch_one(db)
            .await
            .unwrap_or(0);
            return hits.max(0) as usize;
        }
        let hits: Option<i32> = sqlx::query_scalar(
            "SELECT hits FROM pulsar_rate_limits WHERE bucket = $1 AND window_start >= now() - interval '15 minutes'",
        )
        .bind(key)
        .fetch_optional(db)
        .await
        .unwrap_or(None);
        return hits.unwrap_or(0).max(0) as usize;
    }
    mem_login_failures(key, record_hit)
}

async fn check_login_failures(db: &PgPool, key: &str, record_failure: bool) -> usize {
    check_rate_limit(db, key, record_failure).await
}

fn mem_login_failures(email: &str, record_failure: bool) -> usize {
    use std::{
        collections::HashMap,
        sync::Mutex,
        time::{Duration, Instant},
    };
    const WINDOW: Duration = Duration::from_secs(15 * 60);
    static HITS: std::sync::LazyLock<Mutex<HashMap<String, Vec<Instant>>>> =
        std::sync::LazyLock::new(Default::default);
    let Ok(mut m) = HITS.lock() else { return 0 };
    let now = Instant::now();
    if m.len() > 50_000 {
        m.retain(|_, v| v.last().is_some_and(|t| now.duration_since(*t) < WINDOW));
    }
    let v = m.entry(email.to_string()).or_default();
    v.retain(|t| now.duration_since(*t) < WINDOW);
    if record_failure {
        v.push(now);
    }
    v.len()
}
const LOGIN_FAILURE_LIMIT: usize = 10;

pub(crate) fn reset_digest(token: &str) -> String {
    use hmac::{Hmac, Mac};
    let secret =
        std::env::var("SECRET_KEY_BASE").unwrap_or_else(|_| "pulsar-dev-secret-change-me".into());
    let mut mac = Hmac::<sha2_compat::Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(token.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub async fn request_password_reset(
    State(db): State<PgPool>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    if body.get("redirect_url").is_some_and(|v| !v.is_string()) {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"Invalid redirect_url"})),
        )
            .into_response());
    }
    let email = body["email"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    // Separate throttle namespace avoids affecting sign-in lockout.
    if check_login_failures(&db, &format!("reset:{email}"), true).await <= LOGIN_FAILURE_LIMIT {
        let token = random_token(24);
        let row: Option<(String,)> = sqlx::query_as("UPDATE users SET reset_password_token=$1, reset_password_sent_at=now(), updated_at=now() WHERE lower(email)=$2 AND provider='email' RETURNING email")
            .bind(reset_digest(&token)).bind(&email).fetch_optional(&db).await?;
        if let Some((email,)) = row {
            let base =
                std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".into());
            let link = format!(
                "{}/app/auth/password/edit?reset_password_token={token}",
                base.trim_end_matches('/')
            );
            if let Err(e) = crate::mailer::reset(&email, &link).await {
                tracing::error!("reset mail delivery failed: {e}");
            }
        }
    }
    Ok(Json(json!({"message":"You will receive an email with instructions on how to reset your password in a few minutes."})).into_response())
}

pub async fn reset_password(
    State(db): State<PgPool>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let invalid = || {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"message":"Invalid token","redirect_url":"/"})),
        )
            .into_response()
    };
    let Some(token) = body["reset_password_token"]
        .as_str()
        .filter(|s| !s.is_empty())
    else {
        return Ok(invalid());
    };
    let Some(password) = body["password"]
        .as_str()
        .filter(|s| s.len() >= 6 && Some(*s) == body["password_confirmation"].as_str())
    else {
        return Ok(invalid());
    };
    let password = password.to_owned();
    let hash = tokio::task::spawn_blocking(move || bcrypt::hash(password, 10))
        .await
        .map_err(|_| ApiError::bad("hash"))?
        .map_err(|_| ApiError::bad("hash"))?;
    // Atomic update prevents two concurrent requests from consuming the same token.
    let row: Option<(i64, String, Option<Value>)> = sqlx::query_as("UPDATE users SET encrypted_password=$1, reset_password_token=NULL, reset_password_sent_at=NULL, confirmation_token=NULL, confirmed_at=COALESCE(confirmed_at,now()), updated_at=now() WHERE reset_password_token=$2 AND reset_password_sent_at >= now() - CASE WHEN confirmed_at IS NULL AND EXISTS (SELECT 1 FROM account_users WHERE user_id=users.id AND inviter_id IS NOT NULL) THEN interval '7 days' ELSE interval '6 hours' END RETURNING id::bigint, uid, tokens::jsonb")
        .bind(hash).bind(reset_digest(token)).fetch_optional(&db).await?;
    let Some((id, uid, tokens)) = row else {
        return Ok(invalid());
    };
    let (client, token, expiry) = issue_token(&db, id, tokens).await?;
    let data = user_json(&db, id).await?;
    Ok((
        auth_headers(&uid, &client, &token, expiry),
        Json(json!({"data": data})),
    )
        .into_response())
}

/// POST /auth/sign_in {email, password}
pub async fn sign_in(
    State(db): State<PgPool>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> ApiResult<Response> {
    let body = body.map(|b| b.0).unwrap_or_default();
    // CE devise_overrides/sessions_controller: second factor step, mfa_token + otp_code|backup_code.
    if let Some(mfa_token) = body["mfa_token"].as_str() {
        return sign_in_mfa(&db, mfa_token, &body).await;
    }
    let hdr = |k: &str| {
        headers
            .get(k)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let email = body["email"]
        .as_str()
        .map(str::to_string)
        .or_else(|| hdr("email"))
        .unwrap_or_default();
    let password = body["password"]
        .as_str()
        .map(str::to_string)
        .or_else(|| hdr("password"))
        .unwrap_or_default();
    let bad = || {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "Invalid login credentials. Please try again.".into(),
        )
    };
    if email.is_empty() || password.is_empty() {
        return Err(bad());
    }
    // Chatwoot #5171 (lockable) / rack_attack 'login/email': after 10 failed attempts for an email within 15
    // minutes, refuse further attempts, whether or not the account exists.
    let key = email.trim().to_lowercase();
    if check_login_failures(&db, &key, false).await >= LOGIN_FAILURE_LIMIT {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many login attempts. Please try again later.".into(),
        ));
    }
    let row: Option<(i64, String, String, Option<Value>, Option<chrono::NaiveDateTime>)> = sqlx::query_as(
        "SELECT id::bigint, uid, encrypted_password, tokens::jsonb, confirmed_at FROM users WHERE lower(email) = $1 AND provider='email'",
    )
    .bind(&key)
    .fetch_optional(&db)
    .await?;
    let Some((id, uid, enc, tokens, confirmed)) = row else {
        check_login_failures(&db, &key, true).await;
        return Err(bad());
    };
    let pw_ok =
        tokio::task::spawn_blocking(move || bcrypt::verify(&password, &enc).unwrap_or(false))
            .await
            .unwrap_or(false);
    if !pw_ok {
        check_login_failures(&db, &key, true).await;
        return Err(bad());
    }
    if confirmed.is_none() {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            format!(
                "A confirmation email was sent to your account at '{email}'. You must follow the instructions in the email before your account can be activated"
            ),
        ));
    }
    // CE handle_mfa_required: password ok but OTP enrolled -> 206 + short-lived mfa_token, no session yet.
    let otp_on: Option<bool> =
        sqlx::query_scalar("SELECT otp_required_for_login FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(&db)
            .await?;
    if otp_on.unwrap_or(false) {
        let mfa_token = crate::mfa::issue_mfa_token(id);
        return Ok((
            StatusCode::PARTIAL_CONTENT,
            Json(json!({ "mfa_required": true, "mfa_token": mfa_token })),
        )
            .into_response());
    }
    let (client, token, expiry) = issue_token(&db, id, tokens).await?;
    sqlx::query(
        "UPDATE users SET sign_in_count = sign_in_count + 1, last_sign_in_at = current_sign_in_at, current_sign_in_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(&db)
    .await?;
    let data = user_json(&db, id).await?;
    Ok((
        auth_headers(&uid, &client, &token, expiry),
        Json(json!({ "data": data })),
    )
        .into_response())
}

/// Second sign-in step. Failures share the login_failures counter (CE rack_attack mfa_verification/ip: 5/min).
async fn sign_in_mfa(db: &PgPool, mfa_token: &str, body: &Value) -> ApiResult<Response> {
    let bad = || {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "Invalid or expired MFA token".into(),
        )
    };
    let id = crate::mfa::verify_mfa_token(mfa_token).ok_or_else(bad)?;
    let key = format!("mfa:{id}");
    if check_login_failures(db, &key, false).await >= LOGIN_FAILURE_LIMIT {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many login attempts. Please try again later.".into(),
        ));
    }
    let otp = body["otp_code"].as_str();
    let backup = body["backup_code"].as_str();
    if !crate::mfa::consume_for_sign_in(db, id, otp, backup).await? {
        check_login_failures(db, &key, true).await;
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid verification code".into(),
        ));
    }
    let (uid, tokens): (String, Option<Value>) =
        sqlx::query_as("SELECT uid, tokens::jsonb FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(db)
            .await?;
    let (client, token, expiry) = issue_token(db, id, tokens).await?;
    sqlx::query("UPDATE users SET sign_in_count = sign_in_count + 1, last_sign_in_at = current_sign_in_at, current_sign_in_at = now() WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    let data = user_json(db, id).await?;
    Ok((
        auth_headers(&uid, &client, &token, expiry),
        Json(json!({ "data": data })),
    )
        .into_response())
}

pub(crate) async fn issue_token(
    db: &PgPool,
    user_id: i64,
    tokens: Option<Value>,
) -> Result<(String, String, i64), ApiError> {
    let client = random_token(16);
    let token = random_token(16);
    let expiry = chrono::Utc::now().timestamp() + TOKEN_LIFESPAN_SECS;
    let t2 = token.clone();
    let hash = tokio::task::spawn_blocking(move || bcrypt::hash(t2, 10))
        .await
        .map_err(|_| ApiError::bad("hash"))?
        .map_err(|_| ApiError::bad("hash"))?;
    let mut tokens = tokens
        .and_then(|t| t.as_object().cloned())
        .unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    tokens.retain(|_, v| v["expiry"].as_i64().unwrap_or(0) > now);
    while tokens.len() >= MAX_DEVICES {
        let oldest = tokens
            .iter()
            .min_by_key(|(_, v)| v["expiry"].as_i64().unwrap_or(0))
            .map(|(k, _)| k.clone());
        match oldest {
            Some(k) => tokens.remove(&k),
            None => break,
        };
    }
    tokens.insert(
        client.clone(),
        json!({ "token": hash, "expiry": expiry, "updated_at": chrono::Utc::now().to_rfc3339() }),
    );
    sqlx::query("UPDATE users SET tokens = $1::json, updated_at = now() WHERE id = $2")
        .bind(Value::Object(tokens))
        .bind(user_id)
        .execute(db)
        .await?;
    sqlx::query(
        "INSERT INTO user_sessions (user_id, client_id, browser_name, last_activity_at, created_at, updated_at)
         VALUES ($1, $2, 'Web Browser', now(), now(), now())",
    )
    .bind(user_id)
    .bind(&client)
    .execute(db)
    .await?;
    Ok((client, token, expiry))
}

/// GET /auth/validate_token
pub async fn validate_token(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    let data = user_json(&db, user.id).await?;
    Ok(Json(
        json!({ "payload": { "success": true, "data": data } }),
    ))
}

/// DELETE /auth/sign_out
pub async fn sign_out(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    if let Some(client) = user.client {
        sqlx::query("UPDATE users SET tokens = (tokens::jsonb - $1)::json WHERE id = $2")
            .bind(&client)
            .bind(user.id)
            .execute(&db)
            .await?;
        sqlx::query("DELETE FROM user_sessions WHERE user_id = $1 AND client_id = $2")
            .bind(user.id)
            .bind(&client)
            .execute(&db)
            .await?;
    }
    Ok(Json(json!({ "success": true })))
}

/// GET /api/v1/profile
pub async fn profile_show(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    Ok(Json(user_json(&db, user.id).await?))
}

/// PUT /api/v1/profile  {profile: {name, display_name, email, message_signature, ui_settings, password, current_password}}
/// Or multipart/form-data with `profile[...]` text fields and a `profile[avatar]` image file.
pub async fn profile_update(
    State(db): State<PgPool>,
    user: CurrentUser,
    req: Request,
) -> ApiResult {
    let multipart = req
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.starts_with("multipart/form-data"));
    let body: Value = if multipart {
        let mut form = Multipart::from_request(req, &db)
            .await
            .map_err(|_| ApiError::bad("invalid multipart"))?;
        let mut body = json!({});
        while let Some(field) = form
            .next_field()
            .await
            .map_err(|_| ApiError::bad("invalid multipart"))?
        {
            let name = field.name().unwrap_or("").to_string();
            let key = name
                .strip_prefix("profile[")
                .and_then(|s| s.strip_suffix(']'))
                .unwrap_or(&name)
                .to_string();
            if field.file_name().is_some() {
                if key == "avatar" {
                    crate::storage::set_avatar(&db, "User", user.id, field).await?;
                }
                continue;
            }
            let text = field
                .text()
                .await
                .map_err(|_| ApiError::bad("invalid field"))?;
            if !key.is_empty() {
                body["profile"][&key] = json!(text);
            }
        }
        body
    } else {
        let Json(v) = Json::<Value>::from_request(req, &db)
            .await
            .map_err(|_| ApiError::bad("invalid JSON"))?;
        v
    };
    let p = &body["profile"];
    if let Some(pw) = p["password"].as_str() {
        let current = p["current_password"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let (enc,): (String,) = sqlx::query_as("SELECT encrypted_password FROM users WHERE id=$1")
            .bind(user.id)
            .fetch_one(&db)
            .await?;
        if !bcrypt::verify(&current, &enc).unwrap_or(false) {
            return Err(ApiError::bad("Current password is incorrect"));
        }
        if pw.len() < 6 || Some(pw) != p["password_confirmation"].as_str() {
            return Err(ApiError::bad(
                "Password is invalid or confirmation does not match",
            ));
        }
        let hash = bcrypt::hash(pw, 11).map_err(|_| ApiError::bad("hash"))?;
        sqlx::query("UPDATE users SET encrypted_password=$1, updated_at=now() WHERE id=$2")
            .bind(hash)
            .bind(user.id)
            .execute(&db)
            .await?;
    }
    if let Some(ui) = p.get("ui_settings").filter(|v| v.is_object()) {
        sqlx::query("UPDATE users SET ui_settings = COALESCE(ui_settings,'{}'::jsonb) || $1, updated_at=now() WHERE id=$2")
            .bind(ui)
            .bind(user.id)
            .execute(&db)
            .await?;
    }
    for col in ["name", "display_name", "message_signature"] {
        if let Some(v) = p[col].as_str() {
            sqlx::query(&format!(
                "UPDATE users SET {col}=$1, updated_at=now() WHERE id=$2"
            ))
            .bind(v)
            .bind(user.id)
            .execute(&db)
            .await?;
        }
    }
    if let Some(email) = p["email"].as_str() {
        let e = email.trim().to_lowercase();
        sqlx::query("UPDATE users SET email=$1, uid=$1, updated_at=now() WHERE id=$2")
            .bind(e)
            .bind(user.id)
            .execute(&db)
            .await?;
    }
    Ok(Json(user_json(&db, user.id).await?))
}

/// DELETE /api/v1/profile/avatar
pub async fn profile_avatar_destroy(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    crate::storage::remove_avatar(&db, "User", user.id).await?;
    Ok(Json(user_json(&db, user.id).await?))
}

/// POST /api/v1/profile/resend_confirmation: generates a fresh confirmation token and sends the confirmation email if unconfirmed.
pub async fn resend_confirmation(
    State(db): State<PgPool>,
    user: CurrentUser,
) -> ApiResult<StatusCode> {
    let row: Option<(String, String, Option<chrono::NaiveDateTime>)> =
        sqlx::query_as("SELECT name, email, confirmed_at FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_optional(&db)
            .await?;
    if let Some((name, email, None)) = row {
        let token = random_token(20);
        sqlx::query(
            "UPDATE users SET confirmation_token = $1, confirmation_sent_at = now(), updated_at = now() WHERE id = $2",
        )
        .bind(&token)
        .bind(user.id)
        .execute(&db)
        .await?;
        crate::registration::send_confirmation_email(&email, &name, &token, None).await;
    }
    Ok(StatusCode::OK)
}

/// POST /api/v1/profile/reset_access_token: regenerates the user's API access token.
pub async fn reset_access_token(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    let token = random_token(16);
    let updated = sqlx::query("UPDATE access_tokens SET token=$1, updated_at=now() WHERE owner_type='User' AND owner_id=$2")
        .bind(&token)
        .bind(user.id)
        .execute(&db)
        .await?;
    if updated.rows_affected() == 0 {
        sqlx::query("INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at) VALUES ('User', $1, $2, now(), now())")
            .bind(user.id)
            .bind(&token)
            .execute(&db)
            .await?;
    }
    Ok(Json(user_json(&db, user.id).await?))
}

/// PUT /api/v1/profile/set_active_account {profile: {account_id}}
pub async fn set_active_account(
    State(db): State<PgPool>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> ApiResult<StatusCode> {
    let aid = body["profile"]["account_id"].as_i64().or_else(|| {
        body["profile"]["account_id"]
            .as_str()
            .and_then(|s| s.parse().ok())
    });
    if let Some(aid) = aid {
        require_account(&db, &user, aid).await?;
        sqlx::query(
            "UPDATE account_users SET active_at = now() WHERE user_id=$1 AND account_id=$2",
        )
        .bind(user.id)
        .bind(aid)
        .execute(&db)
        .await?;
    }
    Ok(StatusCode::OK)
}

/// POST /api/v1/profile/availability {profile: {account_id, availability}}
pub async fn availability(
    State(st): State<AppState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let p = &body["profile"];
    let aid = p["account_id"]
        .as_i64()
        .or_else(|| p["account_id"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0);
    require_account(&st.db, &user, aid).await?;
    let a = match p["availability"].as_str() {
        Some("offline") => 1,
        Some("busy") => 2,
        _ => 0,
    };
    sqlx::query("UPDATE account_users SET availability=$1, updated_at=now() WHERE user_id=$2 AND account_id=$3")
        .bind(a)
        .bind(user.id)
        .bind(aid)
        .execute(&st.db)
        .await?;
    st.hub.broadcast_presence(&st.db, aid).await;
    Ok(Json(user_json(&st.db, user.id).await?))
}

/// POST /api/v1/profile/auto_offline and /work_preferences: toggles on account_users.
pub async fn auto_offline(
    State(db): State<PgPool>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let p = &body["profile"];
    let aid = p["account_id"]
        .as_i64()
        .or_else(|| p["account_id"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0);
    require_account(&db, &user, aid).await?;
    for col in ["auto_offline", "accepts_conversations", "accepts_calls"] {
        if col != "auto_offline" && !crate::common::has_work_preferences() {
            continue; // column absent on Chatwoot 4.19.1; keep Chatwoot's default
        }
        if let Some(b) = p[col].as_bool() {
            sqlx::query(&format!(
                "UPDATE account_users SET {col}=$1 WHERE user_id=$2 AND account_id=$3"
            ))
            .bind(b)
            .bind(user.id)
            .bind(aid)
            .execute(&db)
            .await?;
        }
    }
    Ok(Json(user_json(&db, user.id).await?))
}

/// Mirrors api/v1/models/_account.json.jbuilder + accounts/show.
pub async fn account_json(db: &PgPool, id: i64) -> Result<Value, ApiError> {
    let a: (i64, String, i32, Option<String>, Option<String>, i32, i64, i64, Option<Value>, Option<Value>, Option<chrono::NaiveDateTime>, Option<i32>) =
        sqlx::query_as(
            "SELECT id::bigint, name, COALESCE(locale,0), domain, support_email, COALESCE(status,0), feature_flags, feature_flags_ext_1,
                    settings, custom_attributes, created_at, auto_resolve_duration FROM accounts WHERE id=$1",
        )
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(json!({
        "id": a.0, "name": a.1, "locale": locale_code(a.2), "domain": a.3, "support_email": a.4,
        "status": if a.5 == 1 { "suspended" } else { "active" },
        "features": enabled_features(a.6, a.7),
        "settings": a.8.unwrap_or(json!({})),
        "custom_attributes": a.9.unwrap_or(json!({})),
        "created_at": a.10, "auto_resolve_duration": a.11,
        "cache_keys": crate::cable::cache_keys(id),
        "latest_chatwoot_version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Inverse of `locale_code`: language code to Chatwoot's integer index.
pub fn locale_index(code: &str) -> Option<i32> {
    (0..46).find(|i| locale_code(*i) == code)
}

pub fn locale_code(i: i32) -> &'static str {
    const L: [&str; 46] = [
        "en", "ar", "nl", "fr", "de", "hi", "it", "ja", "ko", "pt", "ru", "zh", "es", "ml", "ca",
        "el", "pt_BR", "ro", "ta", "fa", "zh_TW", "vi", "da", "tr", "cs", "fi", "id", "sv", "hu",
        "no", "zh_CN", "pl", "sk", "uk", "th", "lv", "is", "he", "lt", "sr", "bg", "sq", "hr",
        "az", "ms", "bn",
    ];
    L.get(i as usize).copied().unwrap_or("en")
}

/// GET /api/v1/accounts/{id}
pub async fn account_show(
    State(db): State<PgPool>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> ApiResult {
    require_account(&db, &user, id).await?;
    Ok(Json(account_json(&db, id).await?))
}

/// PATCH /api/v1/accounts/{id} (admin)
pub async fn account_update(
    State(db): State<PgPool>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(b): Json<Value>,
) -> ApiResult {
    require_account(&db, &user, id).await?.require_admin()?;
    for col in ["name", "domain", "support_email"] {
        if let Some(v) = b[col].as_str() {
            sqlx::query(&format!(
                "UPDATE accounts SET {col}=$1, updated_at=now() WHERE id=$2"
            ))
            .bind(v)
            .bind(id)
            .execute(&db)
            .await?;
        }
    }
    if let Some(s) = b.get("settings").filter(|v| v.is_object()) {
        sqlx::query(
            "UPDATE accounts SET settings = COALESCE(settings,'{}'::jsonb) || $1 WHERE id=$2",
        )
        .bind(s)
        .bind(id)
        .execute(&db)
        .await?;
    }
    if let Some(d) = b["auto_resolve_duration"].as_i64() {
        sqlx::query("UPDATE accounts SET auto_resolve_duration=$1 WHERE id=$2")
            .bind(d as i32)
            .bind(id)
            .execute(&db)
            .await?;
    }
    Ok(Json(account_json(&db, id).await?))
}

/// PUT /api/v1/accounts/{id}/onboarding {onboarding_step, name?, locale?, industry?, company_size?, timezone?, referral_source?, user_role?, website?}
/// Self-host path of Chatwoot OnboardingsController. `account_details` completes onboarding here. Cloud `inbox_setup` is not ported.
/// Each step only acts while the stored cursor points at it, so replays are no-ops.
pub async fn onboarding_update(
    State(db): State<PgPool>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(b): Json<Value>,
) -> ApiResult {
    require_account(&db, &user, id).await?.require_admin()?;
    let step = b["onboarding_step"].as_str().unwrap_or_default();
    if step != "account_details" && step != "inbox_setup" {
        return Err(ApiError::bad("Invalid onboarding step"));
    }
    let (cur,): (Option<Value>,) =
        sqlx::query_as("SELECT custom_attributes FROM accounts WHERE id = $1")
            .bind(id)
            .fetch_one(&db)
            .await?;
    let mut ca = cur.unwrap_or(json!({}));
    let cursor = ca
        .get("onboarding_step")
        .and_then(Value::as_str)
        .map(str::to_string);
    if cursor.as_deref() != Some(step) {
        return Ok(Json(account_json(&db, id).await?));
    }
    if step == "account_details" {
        if let Some(n) = b["name"].as_str() {
            sqlx::query("UPDATE accounts SET name = $1, updated_at = now() WHERE id = $2")
                .bind(n)
                .bind(id)
                .execute(&db)
                .await?;
        }
        if let Some(code) = b["locale"]
            .as_str()
            .and_then(|l| crate::auth::locale_index(l))
        {
            sqlx::query("UPDATE accounts SET locale = $1, updated_at = now() WHERE id = $2")
                .bind(code)
                .bind(id)
                .execute(&db)
                .await?;
        }
        for k in [
            "industry",
            "company_size",
            "timezone",
            "referral_source",
            "user_role",
            "website",
        ] {
            if let Some(v) = b.get(k) {
                ca[k] = v.clone();
            }
        }
    }
    if let Some(o) = ca.as_object_mut() {
        o.remove("onboarding_step");
    }
    sqlx::query("UPDATE accounts SET custom_attributes = $1, updated_at = now() WHERE id = $2")
        .bind(&ca)
        .bind(id)
        .execute(&db)
        .await?;
    Ok(Json(account_json(&db, id).await?))
}

/// GET /api/v1/accounts/{id}/onboarding/help_center_generation (Chatwoot returns an empty status when no generation exists)
pub async fn help_center_generation(
    State(db): State<PgPool>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> ApiResult {
    require_account(&db, &user, id).await?.require_admin()?;
    Ok(Json(
        json!({ "generation_id": null, "state": null, "articles_count": 0, "categories_count": 0 }),
    ))
}

/// POST /api/v1/accounts/{id}/update_active_at
pub async fn update_active_at(
    State(db): State<PgPool>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    require_account(&db, &user, id).await?;
    sqlx::query("UPDATE account_users SET active_at=now() WHERE user_id=$1 AND account_id=$2")
        .bind(user.id)
        .bind(id)
        .execute(&db)
        .await?;
    Ok(StatusCode::OK)
}

/// Mint a single-use, 5-minute SSO token stored hashed in `pulsar_sso_tokens`.
pub(crate) async fn issue_sso_token(db: &PgPool, user_id: i64) -> Result<String, ApiError> {
    let _ = sqlx::query("DELETE FROM pulsar_sso_tokens WHERE expires_at < now()")
        .execute(db)
        .await;
    let raw = random_token(32);
    sqlx::query(
        "INSERT INTO pulsar_sso_tokens (token, user_id, expires_at) VALUES ($1, $2, now() + interval '5 minutes')",
    )
    .bind(reset_digest(&raw))
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(raw)
}

/// POST /auth/sso_exchange {email?, sso_auth_token}
pub async fn sso_exchange(State(db): State<PgPool>, Json(body): Json<Value>) -> ApiResult<Response> {
    let raw = body["sso_auth_token"]
        .as_str()
        .or_else(|| body["token"].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "Invalid SSO token".into()))?;
    let email_filter = body["email"]
        .as_str()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());

    let user_id: Option<i64> = sqlx::query_scalar(
        "DELETE FROM pulsar_sso_tokens WHERE token = $1 AND expires_at > now() RETURNING user_id::bigint",
    )
    .bind(reset_digest(raw))
    .fetch_optional(&db)
    .await?;
    let Some(user_id) = user_id else {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Invalid or expired SSO token".into(),
        ));
    };

    let row: Option<(String, String, Option<Value>, Option<chrono::NaiveDateTime>)> = sqlx::query_as(
        "SELECT uid, email, tokens::jsonb, confirmed_at FROM users WHERE id = $1::bigint",
    )
    .bind(user_id)
    .fetch_optional(&db)
    .await?;
    let Some((uid, user_email, tokens, confirmed)) = row else {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "Invalid SSO user".into()));
    };
    if let Some(ref want) = email_filter
        && user_email.trim().to_lowercase() != *want
    {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "SSO email mismatch".into(),
        ));
    }
    if confirmed.is_none() {
        sqlx::query("UPDATE users SET confirmed_at = now(), updated_at = now() WHERE id = $1::bigint")
            .bind(user_id)
            .execute(&db)
            .await?;
    }
    let (client, token, expiry) = issue_token(&db, user_id, tokens).await?;
    sqlx::query(
        "UPDATE users SET sign_in_count = sign_in_count + 1, last_sign_in_at = current_sign_in_at, current_sign_in_at = now() WHERE id = $1::bigint",
    )
    .bind(user_id)
    .execute(&db)
    .await?;
    let data = user_json(&db, user_id).await?;
    Ok((
        auth_headers(&uid, &client, &token, expiry),
        Json(json!({ "data": data })),
    )
        .into_response())
}

fn sign_oauth_state(ts: i64, nonce: &str) -> String {
    let sig = &reset_digest(&format!("google_oauth:{ts}.{nonce}"))[..32];
    format!("{ts}.{nonce}.{sig}")
}

fn verify_oauth_state(state: &str) -> bool {
    let mut parts = state.split('.');
    let (Some(ts_s), Some(nonce), Some(sig), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Ok(ts) = ts_s.parse::<i64>() else {
        return false;
    };
    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > 600 || nonce.is_empty() {
        return false;
    }
    let expected = &reset_digest(&format!("google_oauth:{ts}.{nonce}"))[..32];
    crate::mfa::ct_eq(expected.as_bytes(), sig.as_bytes())
}

fn google_redirect_uri(st: &AppState) -> String {
    std::env::var("GOOGLE_OAUTH_REDIRECT_URI")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            format!(
                "{}/omniauth/google_oauth2/callback",
                st.frontend_url.trim_end_matches('/')
            )
        })
}

/// GET /omniauth/google_oauth2
pub async fn google_omniauth_start(State(st): State<AppState>) -> Response {
    let base = st.frontend_url.trim_end_matches('/');
    let Some(client_id) = std::env::var("GOOGLE_OAUTH_CLIENT_ID")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return Redirect::to(&format!("{base}/app/login?error=omniauth_not_configured"))
            .into_response();
    };
    let auth_url = std::env::var("GOOGLE_OAUTH_AUTH_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://accounts.google.com/o/oauth2/v2/auth".into());
    let state = sign_oauth_state(chrono::Utc::now().timestamp(), &random_token(12));
    let redirect_uri = google_redirect_uri(&st);
    let qs = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "openid email profile")
        .append_pair("state", &state)
        .append_pair("prompt", "select_account")
        .append_pair("access_type", "online")
        .finish();
    let sep = if auth_url.contains('?') { "&" } else { "?" };
    Redirect::to(&format!("{auth_url}{sep}{qs}")).into_response()
}

/// GET /omniauth/google_oauth2/callback
pub async fn google_omniauth_callback(
    State(st): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let base = st.frontend_url.trim_end_matches('/');
    let fail = |code: &str| Redirect::to(&format!("{base}/app/login?error={code}")).into_response();

    if params.contains_key("error") {
        return fail("omniauth_denied");
    }
    let Some(state) = params.get("state").map(String::as_str) else {
        return fail("omniauth_invalid_state");
    };
    if !verify_oauth_state(state) {
        return fail("omniauth_invalid_state");
    }
    let Some(code) = params
        .get("code")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    else {
        return fail("omniauth_missing_code");
    };
    let (Some(client_id), Some(client_secret)) = (
        std::env::var("GOOGLE_OAUTH_CLIENT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        std::env::var("GOOGLE_OAUTH_CLIENT_SECRET")
            .ok()
            .filter(|s| !s.trim().is_empty()),
    ) else {
        return fail("omniauth_not_configured");
    };
    let token_url = std::env::var("GOOGLE_OAUTH_TOKEN_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://oauth2.googleapis.com/token".into());
    let userinfo_url = std::env::var("GOOGLE_OAUTH_USERINFO_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://www.googleapis.com/oauth2/v3/userinfo".into());
    let redirect_uri = google_redirect_uri(&st);

    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    else {
        return fail("omniauth_http_error");
    };
    let body_form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("code", code)
        .append_pair("client_id", client_id.trim())
        .append_pair("client_secret", client_secret.trim())
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("grant_type", "authorization_code")
        .finish();
    let Ok(tok_resp) = client
        .post(&token_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body_form)
        .send()
        .await
    else {
        return fail("omniauth_token_exchange_failed");
    };
    if !tok_resp.status().is_success() {
        return fail("omniauth_token_exchange_failed");
    }
    let tok_json: Value = serde_json::from_slice(&tok_resp.bytes().await.unwrap_or_default())
        .unwrap_or(Value::Null);
    let Some(access_token) = tok_json["access_token"].as_str().filter(|s| !s.is_empty()) else {
        return fail("omniauth_token_missing");
    };
    let Ok(ui_resp) = client
        .get(&userinfo_url)
        .bearer_auth(access_token)
        .send()
        .await
    else {
        return fail("omniauth_userinfo_failed");
    };
    if !ui_resp.status().is_success() {
        return fail("omniauth_userinfo_failed");
    }
    let ui_json: Value =
        serde_json::from_slice(&ui_resp.bytes().await.unwrap_or_default()).unwrap_or(Value::Null);
    if ui_json.get("email_verified").and_then(Value::as_bool) == Some(false) {
        return fail("omniauth_email_unverified");
    }
    let Some(email) = ui_json["email"]
        .as_str()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s.contains('@'))
    else {
        return fail("omniauth_email_missing");
    };
    let user_id: Option<i64> =
        sqlx::query_scalar("SELECT id::bigint FROM users WHERE lower(email) = $1 ORDER BY id LIMIT 1")
            .bind(&email)
            .fetch_optional(&st.db)
            .await
            .unwrap_or(None);
    let Some(uid) = user_id else {
        return fail("omniauth_user_not_found");
    };
    let Ok(sso) = issue_sso_token(&st.db, uid).await else {
        return fail("omniauth_sso_failed");
    };
    let qs = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("email", &email)
        .append_pair("sso_auth_token", &sso)
        .finish();
    Redirect::to(&format!("{base}/app/login?{qs}")).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_roundtrip() {
        let h = bcrypt::hash("tok", 4).unwrap();
        let exp = chrono::Utc::now().timestamp() + 100;
        let t = json!({ "c1": { "token": h, "expiry": exp } });
        assert!(token_valid(Some(&t), "c1", "tok"));
        assert!(!token_valid(Some(&t), "c1", "bad"));
        assert!(!token_valid(Some(&t), "c2", "tok"));
    }
    #[test]
    fn features_bits() {
        let (a, b) = default_feature_flags();
        let f = enabled_features(a, b);
        assert!(f.contains_key("inbound_emails"));
        assert!(!f.contains_key("conversation_unread_counts"));
    }
}
