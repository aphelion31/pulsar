//! Profile MFA (TOTP + backup codes) and profile session revoke.
//! Mirrors chatwoot/app/controllers/api/v1/profile/{mfa,sessions}_controller.rb and app/services/mfa/*.
//! otp_secret and otp_backup_codes are Rails-encrypted via ar_encryption so the Rails app can read them.
//! Backup codes are encrypted (not hashed), exactly like CE's `encrypts :otp_backup_codes`.
use crate::{ar_encryption, common::*};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde_json::{json, Value};
use sha1::Sha1;
use sqlx::{PgPool, Postgres, Transaction};

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const USED_CODE: &str = "XXXXXXXX";
const ISSUER: &str = "Pulsar";
const MFA_TOKEN_TTL_SECS: i64 = 300;
const MFA_TOKEN_TYPE: &str = "mfa_login";

fn unprocessable(msg: &str) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, msg.into())
}

/// CE Chatwoot.mfa_enabled? (encryption_configured?). Without keys secrets are stored plaintext, so refuse.
fn feature() -> ApiResult<()> {
    if ar_encryption::configured() {
        return Ok(());
    }
    Err(ApiError(StatusCode::FORBIDDEN, "MFA feature is not available. Please configure encryption keys.".into()))
}

/// CE ensure_interactive_session: MFA settings never via api_access_token.
fn interactive(user: &CurrentUser) -> ApiResult<()> {
    if user.client.is_some() {
        return Ok(());
    }
    Err(ApiError(
        StatusCode::FORBIDDEN,
        "Two-factor authentication settings cannot be managed with an API access token. Sign in to the dashboard instead.".into(),
    ))
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn b32_encode(bytes: &[u8]) -> String {
    let (mut out, mut buf, mut bits) = (String::new(), 0u32, 0);
    for &b in bytes {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(B32[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn b32_decode(s: &str) -> Option<Vec<u8>> {
    let (mut out, mut buf, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.trim_end_matches('=').bytes() {
        let v = B32.iter().position(|&x| x == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// RFC 6238 TOTP: HMAC-SHA1, 30s step, 6 digits.
fn totp(key: &[u8], step: i64) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("hmac takes any key length");
    mac.update(&(step as u64).to_be_bytes());
    let h = mac.finalize().into_bytes();
    let o = (h[19] & 0x0f) as usize;
    let n = u32::from_be_bytes([h[o] & 0x7f, h[o + 1], h[o + 2], h[o + 3]]) % 1_000_000;
    format!("{n:06}")
}

/// Matched timestep for `code`, checking one step of drift either side. Steps at or before
/// `consumed` are refused so a code cannot be replayed (CE validate_and_consume_otp!).
fn otp_step(secret_b32: &str, code: &str, consumed: Option<i32>) -> Option<i64> {
    let key = b32_decode(secret_b32)?;
    if code.len() != 6 || !code.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let cur = chrono::Utc::now().timestamp() / 30;
    [cur - 1, cur, cur + 1].into_iter().find(|&s| {
        consumed.is_none_or(|c| s > c as i64) && ct_eq(totp(&key, s).as_bytes(), code.as_bytes())
    })
}

fn gen_backup_codes() -> Vec<String> {
    let mut rng = rand::thread_rng();
    (0..10)
        .map(|_| {
            let mut b = [0u8; 4];
            rng.fill_bytes(&mut b);
            hex::encode_upper(b)
        })
        .collect()
}

/// Psych-compatible YAML array (Rails `serialize :otp_backup_codes, coder: YAML`). Codes are always single-quoted
/// so Psych never reads an all-digit or exponent-looking code as a number.
fn yaml_dump(codes: &[String]) -> String {
    let mut s = String::from("---\n");
    for c in codes {
        s += &format!("- '{c}'\n");
    }
    s
}

fn yaml_load(s: &str) -> Vec<String> {
    s.lines()
        .filter_map(|l| l.strip_prefix("- "))
        .map(|v| v.trim().trim_matches('\'').to_string())
        .collect()
}

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn provisioning_uri(email: &str, secret: &str) -> String {
    format!("otpauth://totp/{}?secret={secret}&issuer={}", pct(&format!("{ISSUER}:{email}")), pct(ISSUER))
}

struct MfaRow {
    pw: String,
    secret: Option<String>,
    backup: Option<String>,
    enabled: bool,
    consumed: Option<i32>,
}

/// Locks the user row for the rest of the transaction so concurrent OTP/backup attempts serialize.
async fn lock_mfa(tx: &mut Transaction<'_, Postgres>, id: i64) -> ApiResult<MfaRow> {
    let (pw, secret, backup, enabled, consumed): (String, Option<String>, Option<String>, Option<bool>, Option<i32>) =
        sqlx::query_as(
            "SELECT encrypted_password, otp_secret, otp_backup_codes, otp_required_for_login, consumed_timestep
             FROM users WHERE id = $1 FOR UPDATE",
        )
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(MfaRow {
        pw,
        secret: secret.map(|s| ar_encryption::decrypt(&s)),
        backup: backup.map(|b| ar_encryption::decrypt(&b)),
        enabled: enabled.unwrap_or(false),
        consumed,
    })
}

async fn write_codes(tx: &mut Transaction<'_, Postgres>, id: i64, codes: &[String]) -> ApiResult<()> {
    sqlx::query("UPDATE users SET otp_backup_codes = $1, updated_at = now() WHERE id = $2")
        .bind(ar_encryption::encrypt(&yaml_dump(codes)))
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// CE Mfa::AuthenticationService: an otp_code takes precedence, backup_code is the fallback.
/// A used backup code is overwritten with XXXXXXXX. Runs inside the caller's transaction.
async fn consume_factor(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
    m: &MfaRow,
    otp: Option<&str>,
    backup: Option<&str>,
) -> ApiResult<bool> {
    if let Some(code) = otp.filter(|c| !c.is_empty()) {
        let Some(secret) = m.secret.as_deref() else { return Ok(false) };
        let Some(step) = otp_step(secret, code, m.consumed) else { return Ok(false) };
        sqlx::query("UPDATE users SET consumed_timestep = $1 WHERE id = $2")
            .bind(step as i32)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        return Ok(true);
    }
    let Some(code) = backup.filter(|c| !c.is_empty()) else { return Ok(false) };
    let mut codes = yaml_load(m.backup.as_deref().unwrap_or(""));
    let Some(i) = codes.iter().position(|c| c != USED_CODE && ct_eq(c.as_bytes(), code.as_bytes())) else {
        return Ok(false);
    };
    codes[i] = USED_CODE.into();
    write_codes(tx, id, &codes).await?;
    Ok(true)
}

fn body_of(b: Option<Json<Value>>) -> Value {
    b.map(|j| j.0).unwrap_or(Value::Null)
}

fn field(b: &Value, k: &str) -> Option<String> {
    b[k].as_str().map(str::to_string)
}

/// GET /api/v1/profile/mfa
pub async fn show(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    interactive(&user)?;
    feature()?;
    let mut tx = db.begin().await?;
    let m = lock_mfa(&mut tx, user.id).await?;
    let codes = m.backup.as_deref().map(yaml_load).unwrap_or_default();
    Ok(Json(json!({
        "feature_available": true,
        "enabled": m.enabled,
        "enforced": false,
        "backup_codes_generated": !codes.is_empty(),
        "remaining_backup_codes": codes.iter().filter(|c| c.as_str() != USED_CODE).count(),
    })))
}

/// POST /api/v1/profile/mfa: generates a pending secret. Not active until /verify.
pub async fn create(State(db): State<PgPool>, user: CurrentUser) -> ApiResult {
    interactive(&user)?;
    feature()?;
    let mut tx = db.begin().await?;
    let m = lock_mfa(&mut tx, user.id).await?;
    if m.enabled {
        return Err(unprocessable("MFA is already enabled"));
    }
    let mut raw = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut raw);
    let secret = b32_encode(&raw);
    sqlx::query("UPDATE users SET otp_secret = $1, updated_at = now() WHERE id = $2")
        .bind(ar_encryption::encrypt(&secret))
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "provisioning_url": provisioning_uri(&user.email, &secret), "secret": secret })))
}

/// POST /api/v1/profile/mfa/verify {otp_code}: activates MFA, returns backup codes on first activation.
pub async fn verify(State(db): State<PgPool>, user: CurrentUser, body: Option<Json<Value>>) -> ApiResult {
    interactive(&user)?;
    feature()?;
    let b = body_of(body);
    let mut tx = db.begin().await?;
    let m = lock_mfa(&mut tx, user.id).await?;
    if m.enabled {
        return Err(unprocessable("MFA is already enabled"));
    }
    let ok = consume_factor(&mut tx, user.id, &m, field(&b, "otp_code").as_deref(), field(&b, "backup_code").as_deref()).await?;
    if !ok {
        return Err(unprocessable("Invalid verification code"));
    }
    let codes = match m.backup.as_deref().map(yaml_load) {
        Some(c) if !c.is_empty() => None,
        _ => {
            let c = gen_backup_codes();
            write_codes(&mut tx, user.id, &c).await?;
            Some(c)
        }
    };
    sqlx::query("UPDATE users SET otp_required_for_login = true, updated_at = now() WHERE id = $1")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let mut out = json!({ "enabled": true });
    if let Some(c) = codes {
        out["backup_codes"] = json!(c);
    }
    Ok(Json(out))
}

/// DELETE /api/v1/profile/mfa {password, otp_code|backup_code}
pub async fn destroy(State(db): State<PgPool>, user: CurrentUser, body: Option<Json<Value>>) -> ApiResult {
    interactive(&user)?;
    feature()?;
    let b = body_of(body);
    let mut tx = db.begin().await?;
    let m = lock_mfa(&mut tx, user.id).await?;
    if !m.enabled {
        return Err(unprocessable("MFA is not enabled"));
    }
    let pw = field(&b, "password").unwrap_or_default();
    let hash = m.pw.clone();
    let pw_ok = tokio::task::spawn_blocking(move || bcrypt::verify(&pw, &hash).unwrap_or(false)).await.unwrap_or(false);
    if !pw_ok {
        return Err(unprocessable("Invalid credentials or verification code"));
    }
    let ok = consume_factor(&mut tx, user.id, &m, field(&b, "otp_code").as_deref(), field(&b, "backup_code").as_deref()).await?;
    if !ok {
        return Err(unprocessable("Invalid verification code"));
    }
    sqlx::query("UPDATE users SET otp_secret = NULL, otp_required_for_login = false, otp_backup_codes = NULL, updated_at = now() WHERE id = $1")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "enabled": false })))
}

/// POST /api/v1/profile/mfa/backup_codes {otp_code|backup_code}: regenerates the 10 codes.
pub async fn backup_codes(State(db): State<PgPool>, user: CurrentUser, body: Option<Json<Value>>) -> ApiResult {
    interactive(&user)?;
    feature()?;
    let b = body_of(body);
    let mut tx = db.begin().await?;
    let m = lock_mfa(&mut tx, user.id).await?;
    if !m.enabled {
        return Err(unprocessable("MFA is not enabled"));
    }
    let ok = consume_factor(&mut tx, user.id, &m, field(&b, "otp_code").as_deref(), field(&b, "backup_code").as_deref()).await?;
    if !ok {
        return Err(unprocessable("Invalid verification code"));
    }
    let codes = gen_backup_codes();
    write_codes(&mut tx, user.id, &codes).await?;
    tx.commit().await?;
    Ok(Json(json!({ "backup_codes": codes })))
}

/// Second sign-in step (CE handle_mfa_verification). Called by auth::sign_in once the mfa_token checks out.
pub async fn consume_for_sign_in(db: &PgPool, user_id: i64, otp: Option<&str>, backup: Option<&str>) -> ApiResult<bool> {
    let mut tx = db.begin().await?;
    let m = lock_mfa(&mut tx, user_id).await?;
    let ok = consume_factor(&mut tx, user_id, &m, otp, backup).await?;
    tx.commit().await?;
    Ok(ok)
}

fn mfa_sig(payload: &str) -> String {
    let key = std::env::var("SECRET_KEY_BASE").unwrap_or_else(|_| "pulsar-dev-secret-change-me".into());
    let mut mac = Hmac::<sha2_compat::Sha256>::new_from_slice(key.as_bytes()).expect("hmac takes any key length");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Short-lived signed token returned in the 206 sign-in response. Payload carries no secrets.
pub fn issue_mfa_token(user_id: i64) -> String {
    let claims = json!({ "uid": user_id, "typ": MFA_TOKEN_TYPE, "exp": chrono::Utc::now().timestamp() + MFA_TOKEN_TTL_SECS });
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
    format!("{payload}.{}", mfa_sig(&payload))
}

pub fn verify_mfa_token(token: &str) -> Option<i64> {
    let (payload, sig) = token.split_once('.')?;
    if !ct_eq(sig.as_bytes(), mfa_sig(payload).as_bytes()) {
        return None;
    }
    let v: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
    if v["typ"] != MFA_TOKEN_TYPE || v["exp"].as_i64()? <= chrono::Utc::now().timestamp() {
        return None;
    }
    v["uid"].as_i64()
}

/// DELETE /api/v1/profile/sessions/:id. Session id is user_sessions.id; revokes its devise token.
pub async fn session_destroy(State(db): State<PgPool>, user: CurrentUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let mut tx = db.begin().await?;
    let client: String = sqlx::query_scalar("SELECT client_id FROM user_sessions WHERE id = $1 AND user_id = $2 FOR UPDATE")
        .bind(id)
        .bind(user.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if user.client.as_deref() == Some(client.as_str()) {
        return Err(unprocessable("You cannot revoke the current session."));
    }
    sqlx::query("UPDATE users SET tokens = (COALESCE(tokens::jsonb, '{}'::jsonb) - $1::text)::json, updated_at = now() WHERE id = $2")
        .bind(&client)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_sessions WHERE id = $1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vector_and_base32() {
        // RFC 6238 Appendix B, SHA1, T=59 -> 8-digit 94287082, 6-digit 287082.
        assert_eq!(totp(b"12345678901234567890", 1), "287082");
        let raw = b"12345678901234567890";
        assert_eq!(b32_decode(&b32_encode(raw)).as_deref(), Some(&raw[..]));
    }

    #[test]
    fn backup_yaml_roundtrip() {
        let codes = vec!["1234ABCD".to_string(), "12345678".to_string()];
        assert_eq!(yaml_load(&yaml_dump(&codes)), codes);
    }
}
