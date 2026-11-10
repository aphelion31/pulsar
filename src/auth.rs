//! devise_token_auth compatible auth + profile + account endpoints.
use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use rand::Rng;
use serde_json::{json, Map, Value};
use sqlx::PgPool;

use crate::common::*;

const TOKEN_LIFESPAN_SECS: i64 = 60 * 24 * 3600; // devise_token_auth: 2.months
const MAX_DEVICES: usize = 25;

fn random_token(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill(&mut b[..]);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Same check as devise_token_auth `token_is_current?` / `token_can_be_reused?` minus the reuse window.
pub fn token_valid(tokens: Option<&Value>, client: &str, token: &str) -> bool {
    let Some(entry) = tokens.and_then(|t| t.get(client)) else { return false };
    let expiry = entry.get("expiry").and_then(Value::as_i64).unwrap_or(0);
    if expiry < chrono::Utc::now().timestamp() {
        return false;
    }
    let check = |k: &str| {
        entry.get(k).and_then(Value::as_str).is_some_and(|h| bcrypt::verify(token, h).unwrap_or(false))
    };
    check("token") || check("last_token")
}

/// Account feature flags -> {name: bool} using config/features.yml bit order per column.
pub fn enabled_features(flags: i64, flags_ext: i64) -> Map<String, Value> {
    static FEATURES: std::sync::LazyLock<Vec<(String, String)>> = std::sync::LazyLock::new(|| {
        let v: Vec<Value> = serde_json::from_str(include_str!("../features.json")).unwrap();
        v.into_iter()
            .map(|f| (f["name"].as_str().unwrap().to_string(), f["column"].as_str().unwrap().to_string()))
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
            if on { a |= 1 << i0; }
            i0 += 1;
        } else {
            if on { b |= 1 << i1; }
            i1 += 1;
        }
    }
    (a, b)
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
        sqlx::query_as(
            "SELECT a.id::bigint, a.name, COALESCE(a.status,0), au.active_at, COALESCE(au.role,0), au.availability,
                    au.auto_offline, au.accepts_conversations, au.accepts_calls, au.inviter_id, a.feature_flags, a.feature_flags_ext_1,
                    a.custom_attributes
             FROM account_users au JOIN accounts a ON a.id = au.account_id
             WHERE au.user_id = $1 ORDER BY au.active_at DESC NULLS LAST, au.id",
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;
    let access_token: Option<(String,)> =
        sqlx::query_as("SELECT token FROM access_tokens WHERE owner_type='User' AND owner_id=$1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    let active = accounts.first();
    let api_enabled = accounts.iter().any(|a| enabled_features(a.10, a.11).contains_key("api_and_webhooks"));
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

fn auth_headers(uid: &str, client: &str, token: &str, expiry: i64) -> HeaderMap {
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

/// POST /auth/sign_in {email, password}
pub async fn sign_in(State(db): State<PgPool>, headers: HeaderMap, body: Option<Json<Value>>) -> ApiResult<Response> {
    let body = body.map(|b| b.0).unwrap_or_default();
    let hdr = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).map(str::to_string);
    let email = body["email"].as_str().map(str::to_string).or_else(|| hdr("email")).unwrap_or_default();
    let password = body["password"].as_str().map(str::to_string).or_else(|| hdr("password")).unwrap_or_default();
    let bad = || ApiError(StatusCode::UNAUTHORIZED, "Invalid login credentials. Please try again.".into());
    if email.is_empty() || password.is_empty() {
        return Err(bad());
    }
    let row: Option<(i64, String, String, Option<Value>, Option<chrono::NaiveDateTime>)> = sqlx::query_as(
        "SELECT id::bigint, uid, encrypted_password, tokens::jsonb, confirmed_at FROM users WHERE lower(email) = $1 AND provider='email'",
    )
    .bind(email.trim().to_lowercase())
    .fetch_optional(&db)
    .await?;
    let (id, uid, enc, tokens, confirmed) = row.ok_or_else(bad)?;
    let pw_ok = tokio::task::spawn_blocking(move || bcrypt::verify(&password, &enc).unwrap_or(false))
        .await
        .unwrap_or(false);
    if !pw_ok {
        return Err(bad());
    }
    if confirmed.is_none() {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            format!("A confirmation email was sent to your account at '{email}'. You must follow the instructions in the email before your account can be activated"),
        ));
    }
    let (client, token, expiry) = issue_token(&db, id, tokens).await?;
    sqlx::query(
        "UPDATE users SET sign_in_count = sign_in_count + 1, last_sign_in_at = current_sign_in_at, current_sign_in_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(&db)
    .await?;
    let data = user_json(&db, id).await?;
    Ok((auth_headers(&uid, &client, &token, expiry), Json(json!({ "data": data }))).into_response())
}

async fn issue_token(db: &PgPool, user_id: i64, tokens: Option<Value>) -> Result<(String, String, i64), ApiError> {
    let client = random_token(16);
    let token = random_token(16);
    let expiry = chrono::Utc::now().timestamp() + TOKEN_LIFESPAN_SECS;
    let t2 = token.clone();
    let hash = tokio::task::spawn_blocking(move || bcrypt::hash(t2, 10))
        .await
        .map_err(|_| ApiError::bad("hash"))?
        .map_err(|_| ApiError::bad("hash"))?;
    let mut tokens = tokens.and_then(|t| t.as_object().cloned()).unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    tokens.retain(|_, v| v["expiry"].as_i64().unwrap_or(0) > now);
    while tokens.len() >= MAX_DEVICES {
        let oldest = tokens.iter().min_by_key(|(_, v)| v["expiry"].as_i64().unwrap_or(0)).map(|(k, _)| k.clone());
        match oldest {
            Some(k) => tokens.remove(&k),
            None => break,
        };
    }
    tokens.insert(client.clone(), json!({ "token": hash, "expiry": expiry, "updated_at": chrono::Utc::now().to_rfc3339() }));
    sqlx::query("UPDATE users SET tokens = $1::json, updated_at = now() WHERE id = $2")
        .bind(Value::Object(tokens))
        .bind(user_id)
        .execute(db)
        .await?;
    Ok((client, token, expiry))
}

/// GET /auth/validate_token
pub async fn validate_token(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    let data = user_json(&db, user.id).await?;
    Ok(Json(json!({ "payload": { "success": true, "data": data } })))
}

/// DELETE /auth/sign_out
pub async fn sign_out(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    if let Some(client) = user.client {
        sqlx::query("UPDATE users SET tokens = (tokens::jsonb - $1)::json WHERE id = $2")
            .bind(client)
            .bind(user.id)
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
/// Multipart (avatar upload) is not supported: ponytail, add when ActiveStorage port lands.
pub async fn profile_update(State(db): State<PgPool>, user: CurrentUser, Json(body): Json<Value>) -> ApiResult {
    let p = &body["profile"];
    if let Some(pw) = p["password"].as_str() {
        let current = p["current_password"].as_str().unwrap_or_default().to_string();
        let (enc,): (String,) = sqlx::query_as("SELECT encrypted_password FROM users WHERE id=$1")
            .bind(user.id)
            .fetch_one(&db)
            .await?;
        if !bcrypt::verify(&current, &enc).unwrap_or(false) {
            return Err(ApiError::bad("Current password is incorrect"));
        }
        if pw.len() < 6 || Some(pw) != p["password_confirmation"].as_str() {
            return Err(ApiError::bad("Password is invalid or confirmation does not match"));
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
            sqlx::query(&format!("UPDATE users SET {col}=$1, updated_at=now() WHERE id=$2"))
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

/// PUT /api/v1/profile/set_active_account {profile: {account_id}}
pub async fn set_active_account(State(db): State<PgPool>, user: CurrentUser, Json(body): Json<Value>) -> ApiResult<StatusCode> {
    let aid = body["profile"]["account_id"].as_i64().or_else(|| body["profile"]["account_id"].as_str().and_then(|s| s.parse().ok()));
    if let Some(aid) = aid {
        require_account(&db, &user, aid).await?;
        sqlx::query("UPDATE account_users SET active_at = now() WHERE user_id=$1 AND account_id=$2")
            .bind(user.id)
            .bind(aid)
            .execute(&db)
            .await?;
    }
    Ok(StatusCode::OK)
}

/// POST /api/v1/profile/availability {profile: {account_id, availability}}
pub async fn availability(State(st): State<AppState>, user: CurrentUser, Json(body): Json<Value>) -> ApiResult {
    let p = &body["profile"];
    let aid = p["account_id"].as_i64().or_else(|| p["account_id"].as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
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
pub async fn auto_offline(State(db): State<PgPool>, user: CurrentUser, Json(body): Json<Value>) -> ApiResult {
    let p = &body["profile"];
    let aid = p["account_id"].as_i64().or_else(|| p["account_id"].as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
    require_account(&db, &user, aid).await?;
    for col in ["auto_offline", "accepts_conversations", "accepts_calls"] {
        if let Some(b) = p[col].as_bool() {
            sqlx::query(&format!("UPDATE account_users SET {col}=$1 WHERE user_id=$2 AND account_id=$3"))
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

pub fn locale_code(i: i32) -> &'static str {
    const L: [&str; 46] = [
        "en", "ar", "nl", "fr", "de", "hi", "it", "ja", "ko", "pt", "ru", "zh", "es", "ml", "ca", "el", "pt_BR", "ro",
        "ta", "fa", "zh_TW", "vi", "da", "tr", "cs", "fi", "id", "sv", "hu", "no", "zh_CN", "pl", "sk", "uk", "th",
        "lv", "is", "he", "lt", "sr", "bg", "sq", "hr", "az", "ms", "bn",
    ];
    L.get(i as usize).copied().unwrap_or("en")
}

/// GET /api/v1/accounts/{id}
pub async fn account_show(State(db): State<PgPool>, user: CurrentUser, Path(id): Path<i64>) -> ApiResult {
    require_account(&db, &user, id).await?;
    Ok(Json(account_json(&db, id).await?))
}

/// PATCH /api/v1/accounts/{id} (admin)
pub async fn account_update(State(db): State<PgPool>, user: CurrentUser, Path(id): Path<i64>, Json(b): Json<Value>) -> ApiResult {
    require_account(&db, &user, id).await?.require_admin()?;
    for col in ["name", "domain", "support_email"] {
        if let Some(v) = b[col].as_str() {
            sqlx::query(&format!("UPDATE accounts SET {col}=$1, updated_at=now() WHERE id=$2")).bind(v).bind(id).execute(&db).await?;
        }
    }
    if let Some(s) = b.get("settings").filter(|v| v.is_object()) {
        sqlx::query("UPDATE accounts SET settings = COALESCE(settings,'{}'::jsonb) || $1 WHERE id=$2").bind(s).bind(id).execute(&db).await?;
    }
    if let Some(d) = b["auto_resolve_duration"].as_i64() {
        sqlx::query("UPDATE accounts SET auto_resolve_duration=$1 WHERE id=$2").bind(d as i32).bind(id).execute(&db).await?;
    }
    Ok(Json(account_json(&db, id).await?))
}

/// POST /api/v1/accounts/{id}/update_active_at
pub async fn update_active_at(State(db): State<PgPool>, user: CurrentUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    require_account(&db, &user, id).await?;
    sqlx::query("UPDATE account_users SET active_at=now() WHERE user_id=$1 AND account_id=$2").bind(user.id).bind(id).execute(&db).await?;
    Ok(StatusCode::OK)
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
