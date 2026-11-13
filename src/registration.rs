//! Account signup, email confirmation, and resend confirmation.
//! Mirrors Chatwoot CE:
//! - app/controllers/api/v1/accounts_controller.rb#create
//! - app/controllers/api/v2/accounts_controller.rb#create
//! - app/builders/account_builder.rb
//! - app/controllers/devise_overrides/confirmations_controller.rb#create
//! - app/controllers/auth/resend_confirmations_controller.rb#create
use axum::{
    extract::{FromRef, FromRequestParts, State},
    http::{request::Parts, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use rand::Rng;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::common::*;

const TOKEN_LIFESPAN_SECS: i64 = 60 * 24 * 3600;
const MAX_DEVICES: usize = 25;

/// Optional authenticated user (no 401 when unauthenticated).
pub struct OptionalUser(pub Option<CurrentUser>);

impl<S: Send + Sync> FromRequestParts<S> for OptionalUser
where
    PgPool: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(CurrentUser::from_request_parts(parts, state).await.ok()))
    }
}

fn random_token(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill(&mut b[..]);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
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

/// Extract scalar string from `installation_configs.serialized_value` (JSONB or YAML string).
fn parse_config_scalar(v: &Value) -> Option<String> {
    let val = if let Some(o) = v.as_object() {
        o.get("value").cloned()
    } else if let Some(s) = v.as_str() {
        s.lines()
            .find_map(|l| l.strip_prefix("value: "))
            .map(|l| Value::String(l.trim().trim_matches('\'').trim_matches('"').to_string()))
    } else {
        None
    }?;
    match val {
        Value::Null => None,
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if s.is_empty() => None,
        Value::String(s) => Some(s),
        other => Some(other.to_string()),
    }
}

/// Mirrors `GlobalConfigService.load(key, default)`.
async fn load_config(db: &PgPool, key: &str, default: &str) -> String {
    let row: Option<(Value,)> = sqlx::query_as(
        "SELECT serialized_value FROM installation_configs WHERE name = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(key)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    if let Some((v,)) = row {
        if let Some(s) = parse_config_scalar(&v) {
            return s;
        }
    }
    std::env::var(key).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| default.to_string())
}

async fn signup_enabled(db: &PgPool) -> (bool, String) {
    let raw = load_config(db, "ENABLE_ACCOUNT_SIGNUP", "false").await;
    let enabled = !raw.eq_ignore_ascii_case("false");
    (enabled, raw)
}

fn api_only_signup(raw_signup_cfg: &str) -> bool {
    let cw_api_only = std::env::var("CW_API_ONLY_SERVER")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "t" | "yes"))
        .unwrap_or(false);
    cw_api_only || raw_signup_cfg == "api_only"
}

/// Mirrors `ChatwootCaptcha#valid?`: skips when `HCAPTCHA_SERVER_KEY` is blank.
async fn verify_captcha(db: &PgPool, client_response: Option<&str>) -> bool {
    let server_key = load_config(db, "HCAPTCHA_SERVER_KEY", "").await;
    if server_key.trim().is_empty() {
        return true;
    }
    let Some(token) = client_response.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build() else {
        return false;
    };
    let Ok(resp) = client
        .post("https://hcaptcha.com/siteverify")
        .form(&[("response", token), ("secret", server_key.as_str())])
        .send()
        .await
    else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(bytes) = resp.bytes().await else {
        return false;
    };
    serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| v["success"].as_bool()).unwrap_or(false)
}

fn valid_email(email: &str) -> bool {
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }
    let (local, domain) = (parts[0], parts[1]);
    if local.is_empty() || domain.is_empty() || email.contains(char::is_whitespace) {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    labels.len() >= 2 && labels.iter().all(|l| !l.is_empty())
}

/// Mirrors Chatwoot `User` Devise `:validatable` (6..128) + `:password_has_required_content`
/// (1 uppercase, 1 lowercase, 1 digit, 1 special char).
fn validate_password(pw: &str) -> Result<(), &'static str> {
    if pw.len() < 6 {
        return Err("Password is too short (minimum is 6 characters)");
    }
    if pw.len() > 128 {
        return Err("Password is too long (maximum is 128 characters)");
    }
    if !pw.chars().any(|c| c.is_ascii_uppercase()) {
        return Err("Password must contain at least 1 uppercase letter");
    }
    if !pw.chars().any(|c| c.is_ascii_lowercase()) {
        return Err("Password must contain at least 1 lowercase letter");
    }
    if !pw.chars().any(|c| c.is_ascii_digit()) {
        return Err("Password must contain at least 1 number");
    }
    if !pw.chars().any(|c| !c.is_ascii_alphanumeric() && !c.is_whitespace()) {
        return Err("Password must contain at least 1 special character");
    }
    Ok(())
}

async fn send_confirmation_email(email: &str, name: &str, token: &str, redirect_url: Option<&str>) {
    let base = std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    let mut link = format!("{}/app/auth/confirmation?confirmation_token={token}", base.trim_end_matches('/'));
    if let Some(r) = redirect_url.filter(|s| !s.is_empty()) {
        link.push_str("&route_url=");
        link.push_str(&url::form_urlencoded::byte_serialize(r.as_bytes()).collect::<String>());
    }
    let subject = "Confirmation instructions";
    let display = if name.is_empty() { email } else { name };
    if crate::mailer::configured() {
        let html = format!(
            "<p>Welcome, {display}!</p><p>You can confirm your account email through the link below:</p><p><a href=\"{link}\">Confirm my account</a></p>"
        );
        if let Err(e) = crate::mailer::send_html(email, subject, html).await {
            tracing::error!(to = email, "confirmation mail delivery failed: {e}");
        }
    } else if std::env::var("PULSAR_LOG_MAIL").as_deref() == Ok("1") {
        tracing::info!(to = email, subject, link, "confirmation email");
    } else {
        tracing::warn!(to = email, "SMTP_ADDRESS not set; confirmation email not sent");
    }
}

fn msg_err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "message": msg }))).into_response()
}

async fn create_account_inner(db: &PgPool, current_user: Option<CurrentUser>, body: Value, is_v2: bool) -> ApiResult<Response> {
    let (enabled, raw_cfg) = signup_enabled(db).await;
    if !enabled {
        return Ok(msg_err(StatusCode::NOT_FOUND, "Not Found"));
    }

    let account_name = body["account_name"].as_str().map(str::trim).unwrap_or("");
    let user_full_name = body["user_full_name"].as_str().map(str::trim).unwrap_or("");
    if !is_v2 && account_name.is_empty() && user_full_name.is_empty() {
        return Ok(msg_err(
            StatusCode::FORBIDDEN,
            "Invalid, please check the signup paramters and try again",
        ));
    }

    let captcha_resp = body["h_captcha_client_response"].as_str();
    if !verify_captcha(db, captcha_resp).await {
        return Ok(msg_err(StatusCode::UNPROCESSABLE_ENTITY, "Invalid Captcha"));
    }

    let email_raw = body["email"].as_str().unwrap_or("").trim();
    let email = email_raw.to_lowercase();
    let password = body["password"].as_str().unwrap_or("");

    if current_user.is_none() {
        if !valid_email(&email) {
            return Ok(msg_err(StatusCode::FORBIDDEN, "You have entered an invalid email"));
        }
        let blocked = load_config(db, "BLOCKED_EMAIL_DOMAINS", "").await;
        if !blocked.is_empty() {
            let domain = email.split('@').nth(1).unwrap_or("");
            if blocked.lines().map(str::trim).filter(|d| !d.is_empty()).any(|d| domain.contains(&d.to_lowercase())) {
                return Ok(msg_err(
                    StatusCode::FORBIDDEN,
                    "This domain is not allowed. If you believe this is a mistake, please contact support.",
                ));
            }
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE lower(email) = $1)")
            .bind(&email)
            .fetch_one(db)
            .await?;
        if exists {
            return Ok(msg_err(
                StatusCode::UNPROCESSABLE_ENTITY,
                &format!("You have already signed up for an account with {email}"),
            ));
        }
        if let Err(msg) = validate_password(password) {
            return Ok(msg_err(StatusCode::UNPROCESSABLE_ENTITY, msg));
        }
    }

    let locale_idx = body["locale"]
        .as_str()
        .and_then(crate::auth::locale_index)
        .unwrap_or(0);
    let (ff, ff1) = crate::auth::default_feature_flags();
    let onboarding_step = if is_v2 { "profile_update" } else { "account_details" };
    let custom_attrs = json!({ "onboarding_step": onboarding_step });

    let mut tx = db.begin().await?;
    let account_id: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (name, locale, feature_flags, feature_flags_ext_1, custom_attributes, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, now(), now()) RETURNING id::bigint",
    )
    .bind(account_name)
    .bind(locale_idx)
    .bind(ff)
    .bind(ff1)
    .bind(&custom_attrs)
    .fetch_one(&mut *tx)
    .await?;

    let mut confirmation_token: Option<String> = None;
    let user_id: i64 = if let Some(ref u) = current_user {
        u.id
    } else {
        let pw = password.to_string();
        let hash = tokio::task::spawn_blocking(move || bcrypt::hash(pw, 10))
            .await
            .map_err(|_| ApiError::bad("hash"))?
            .map_err(|_| ApiError::bad("hash"))?;
        let ctok = random_token(20);
        let pubsub = uuid::Uuid::new_v4().simple().to_string();
        let uid: i64 = sqlx::query_scalar(
            "INSERT INTO users (provider, uid, encrypted_password, name, email, confirmation_token, confirmation_sent_at, pubsub_token, tokens, created_at, updated_at)
             VALUES ('email', $1, $2, $3, $1, $4, now(), $5, '{}', now(), now()) RETURNING id::bigint",
        )
        .bind(&email)
        .bind(&hash)
        .bind(user_full_name)
        .bind(&ctok)
        .bind(&pubsub)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at)
             VALUES ('User', $1, $2, now(), now())",
        )
        .bind(uid)
        .bind(random_token(16))
        .execute(&mut *tx)
        .await?;
        confirmation_token = Some(ctok);
        uid
    };

    sqlx::query(
        "INSERT INTO account_users (account_id, user_id, role, created_at, updated_at, active_at)
         VALUES ($1, $2, 1, now(), now(), now())",
    )
    .bind(account_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    if let Some(ref ctok) = confirmation_token {
        send_confirmation_email(&email, user_full_name, ctok, None).await;
    }

    if is_v2 || current_user.is_some() || api_only_signup(&raw_cfg) {
        let (uid_str, tokens): (String, Option<Value>) = sqlx::query_as("SELECT uid, tokens::jsonb FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(db)
            .await?;
        let (client, token, expiry) = issue_token(db, user_id, tokens).await?;
        let mut data = crate::auth::user_json(db, user_id).await?;
        data["account_id"] = json!(account_id);
        Ok((auth_headers(&uid_str, &client, &token, expiry), Json(json!({ "data": data }))).into_response())
    } else {
        Ok(Json(json!({ "email": email })).into_response())
    }
}

/// POST /api/v1/accounts
pub async fn v1_create(State(db): State<PgPool>, OptionalUser(user): OptionalUser, Json(body): Json<Value>) -> ApiResult<Response> {
    create_account_inner(&db, user, body, false).await
}

/// POST /api/v2/accounts
pub async fn v2_create(State(db): State<PgPool>, OptionalUser(user): OptionalUser, Json(body): Json<Value>) -> ApiResult<Response> {
    create_account_inner(&db, user, body, true).await
}

/// POST /auth/confirmation {confirmation_token}
pub async fn confirm(State(db): State<PgPool>, Json(body): Json<Value>) -> ApiResult<Response> {
    let token = body["confirmation_token"].as_str().unwrap_or("").trim();
    if token.is_empty() {
        return Ok(msg_err(StatusCode::UNPROCESSABLE_ENTITY, "Invalid token"));
    }
    let row: Option<(i64, String, Option<Value>, Option<chrono::NaiveDateTime>)> = sqlx::query_as(
        "SELECT id::bigint, uid, tokens::jsonb, confirmed_at FROM users WHERE confirmation_token = $1",
    )
    .bind(token)
    .fetch_optional(&db)
    .await?;
    let Some((id, uid, tokens, confirmed_at)) = row else {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "message": "Invalid token", "redirect_url": "/" })),
        )
            .into_response());
    };
    if confirmed_at.is_some() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "message": "Already confirmed", "redirect_url": "/" })),
        )
            .into_response());
    }
    sqlx::query("UPDATE users SET confirmed_at = now(), updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await?;
    let (client, auth_tok, expiry) = issue_token(&db, id, tokens).await?;
    let data = crate::auth::user_json(&db, id).await?;
    Ok((auth_headers(&uid, &client, &auth_tok, expiry), Json(json!({ "data": data }))).into_response())
}

/// POST /resend_confirmation {email, h_captcha_client_response?}
pub async fn resend_confirmation(State(db): State<PgPool>, Json(body): Json<Value>) -> StatusCode {
    if !verify_captcha(&db, body["h_captcha_client_response"].as_str()).await {
        return StatusCode::OK;
    }
    let Some(email_raw) = body["email"].as_str() else {
        return StatusCode::OK;
    };
    let email = email_raw.trim().to_lowercase();
    if email.is_empty() {
        return StatusCode::OK;
    }
    let row: Option<(i64, String, String, Option<chrono::NaiveDateTime>)> = sqlx::query_as(
        "SELECT id::bigint, name, email, confirmed_at FROM users WHERE lower(email) = $1 AND provider = 'email'",
    )
    .bind(&email)
    .fetch_optional(&db)
    .await
    .ok()
    .flatten();
    if let Some((id, name, user_email, None)) = row {
        let token = random_token(20);
        if sqlx::query("UPDATE users SET confirmation_token = $1, confirmation_sent_at = now(), updated_at = now() WHERE id = $2")
            .bind(&token)
            .bind(id)
            .execute(&db)
            .await
            .is_ok()
        {
            send_confirmation_email(&user_email, &name, &token, None).await;
        }
    }
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_rules() {
        assert!(validate_password("Short1!").is_ok());
        assert!(validate_password("Password123!").is_ok());
        assert!(validate_password("Ab1!a").is_err()); // < 6
        assert!(validate_password("password123!").is_err()); // no uppercase
        assert!(validate_password("PASSWORD123!").is_err()); // no lowercase
        assert!(validate_password("Password!").is_err()); // no digit
        assert!(validate_password("Password123").is_err()); // no special
    }

    #[test]
    fn email_validation() {
        assert!(valid_email("user@example.com"));
        assert!(!valid_email("not-an-email"));
        assert!(!valid_email("user@localhost"));
        assert!(!valid_email("bad user@example.com"));
    }

    #[test]
    fn config_scalar_parsing() {
        assert_eq!(parse_config_scalar(&json!({"value": false})).as_deref(), Some("false"));
        assert_eq!(parse_config_scalar(&json!({"value": "api_only"})).as_deref(), Some("api_only"));
        assert_eq!(parse_config_scalar(&json!("---\nvalue: true\n")).as_deref(), Some("true"));
    }
}
