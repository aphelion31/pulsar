// Mirrors Rails 7.2 active_record/encryption/{encryptor,message_serializer,properties,
// key,key_provider,cipher/aes256_gcm}. load_defaults 7.0 uses PBKDF2-HMAC-SHA1.
use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use rand::RngCore;
use sha1::{Digest, Sha1};
use serde_json::{json, Value};
use std::{io::{Read, Write}, sync::OnceLock};

fn key() -> Option<&'static [u8; 32]> {
    static KEY: OnceLock<Option<[u8; 32]>> = OnceLock::new();
    KEY.get_or_init(|| {
        let (Ok(primary), Ok(salt)) = (std::env::var("ACTIVE_RECORD_ENCRYPTION_PRIMARY_KEY"), std::env::var("ACTIVE_RECORD_ENCRYPTION_KEY_DERIVATION_SALT")) else {
            tracing::warn!("Active Record encryption keys missing; email credentials will be stored as plaintext");
            return None;
        };
        if primary.is_empty() || salt.is_empty() {
            tracing::warn!("Active Record encryption keys empty; email credentials will be stored as plaintext");
            return None;
        }
        Some(derive(&primary, &salt))
    }).as_ref()
}

/// ActiveRecord::Encryption::KeyGenerator#derive_key_from: PBKDF2-HMAC-SHA1 (load_defaults 7.0), 2**16 iterations.
fn derive(primary: &str, salt: &str) -> [u8; 32] {
    let mut key = [0; 32];
    pbkdf2::pbkdf2_hmac::<Sha1>(primary.as_bytes(), salt.as_bytes(), 65_536, &mut key);
    key
}

fn encrypt_with_key(plain: &str, key: &[u8; 32], iv: &[u8; 12]) -> String {
    let compressed = plain.len() > 140;
    let data = if compressed {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(plain.as_bytes()).expect("in-memory compression failed");
        encoder.finish().expect("in-memory compression failed")
    } else { plain.as_bytes().to_vec() };
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key");
    let mut encrypted = cipher.encrypt(Nonce::from_slice(iv), data.as_ref()).expect("encryption failed");
    let tag = encrypted.split_off(encrypted.len() - 16);
    let id = hex::encode(Sha1::digest(key));
    // Structs preserve Rails' p, h and iv, at, i, c ordering.
    #[derive(serde::Serialize)]
    struct Headers { iv: String, at: String, i: String, #[serde(skip_serializing_if = "Option::is_none")] c: Option<bool> }
    #[derive(serde::Serialize)]
    struct Message { p: String, h: Headers }
    serde_json::to_string(&Message { p: STANDARD.encode(encrypted), h: Headers { iv: STANDARD.encode(iv), at: STANDARD.encode(tag), i: STANDARD.encode(&id[..4]), c: compressed.then_some(true) } }).expect("JSON serialization failed")
}

/// WebhookSecretable (`has_secure_token :secret` + `encrypts :secret`): a fresh 24-char base58 secret,
/// returned as (plaintext, value to store). Stored encrypted when encryption keys are configured.
/// Decrypt the `secret` field of a JSON object in place (rows built with jsonb_build_object).
pub fn reveal_secret(mut v: serde_json::Value) -> serde_json::Value {
    if let Some(s) = v.get("secret").and_then(|s| s.as_str()).map(decrypt) {
        v["secret"] = serde_json::Value::String(s);
    }
    v
}

pub fn new_secret() -> (String, String) {
    use rand::Rng;
    const B58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut r = rand::thread_rng();
    let plain: String = (0..24).map(|_| B58[r.gen_range(0..B58.len())] as char).collect();
    let stored = encrypt(&plain);
    (plain, stored)
}

/// True when both encryption env keys are set. MFA secrets must not be stored without this.
pub fn configured() -> bool {
    key().is_some()
}

pub fn encrypt(plain: &str) -> String {
    match key() {
        Some(key) => {
            let mut iv = [0; 12];
            rand::rngs::OsRng.fill_bytes(&mut iv);
            encrypt_with_key(plain, key, &iv)
        }
        None => plain.to_owned(),
    }
}

fn decrypt_with_key(stored: &str, key: &[u8; 32]) -> Option<String> {
    let message: serde_json::Value = serde_json::from_str(stored).ok()?;
    let headers = message.get("h")?.as_object()?;
    let iv = STANDARD.decode(headers.get("iv")?.as_str()?).ok()?;
    let tag = STANDARD.decode(headers.get("at")?.as_str()?).ok()?;
    if iv.len() != 12 || tag.len() != 16 { return None; }
    if let Some(id) = headers.get("i") {
        let id = STANDARD.decode(id.as_str()?).ok()?;
        if id != hex::encode(Sha1::digest(key))[..4].as_bytes() { return None; }
    }
    let mut payload = STANDARD.decode(message.get("p")?.as_str()?).ok()?;
    payload.extend(tag);
    let mut plain = Aes256Gcm::new_from_slice(key).ok()?.decrypt(Nonce::from_slice(&iv), payload.as_ref()).ok()?;
    if headers.get("c").and_then(|v| v.as_bool()).unwrap_or(false) {
        let mut decoded = Vec::new();
        // Passwords are bounded to prevent untrusted compressed data exhausting memory.
        ZlibDecoder::new(plain.as_slice()).take(1_048_577).read_to_end(&mut decoded).ok()?;
        if decoded.len() > 1_048_576 { return None; }
        plain = decoded;
    }
    String::from_utf8(plain).ok()
}

pub fn decrypt(stored: &str) -> String {
    key().and_then(|key| decrypt_with_key(stored, key)).unwrap_or_else(|| stored.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rails_known_answer_and_roundtrip() {
        let mut key = [0; 32];
        pbkdf2::pbkdf2_hmac::<Sha1>(b"testprimarykey123", b"testsalt123", 65_536, &mut key);
        // Independently generated with Ruby OpenSSL PBKDF2 and AES-256-GCM.
        let expected = r#"{"p":"FK4h72X1","h":{"iv":"AAECAwQFBgcICQoL","at":"d/X7zrx2smSyct6rQuhtDA==","i":"MjgyYQ=="}}"#;
        assert_eq!(encrypt_with_key("secret", &key, &[0,1,2,3,4,5,6,7,8,9,10,11]), expected);
        assert_eq!(decrypt_with_key(expected, &key).as_deref(), Some("secret"));
        let long = "password".repeat(30);
        assert_eq!(decrypt_with_key(&encrypt_with_key(&long, &key, &[0; 12]), &key), Some(long));
        assert_eq!(decrypt("plaintext"), "plaintext");
    }
}

#[cfg(test)]
mod rails_compat {
    /// Ciphertext produced by real Rails 7.1.5.2 (Chatwoot 4.15 container, load_defaults 7.0,
    /// store_key_references) with primary key "crosscheckprimary" and salt "crosschecksalt".
    const RAILS_CT: &str = r#"{"p":"ao6CBTb08GbeDBJWfCR4tt4=","h":{"iv":"MT/IWTv+Y9Y8tFbf","at":"8aztJCrTT1KqBrzguFtorA==","i":"ZDI0NA=="}}"#;

    #[test]
    fn decrypts_rails_ciphertext() {
        let key = super::derive("crosscheckprimary", "crosschecksalt");
        assert_eq!(super::decrypt_with_key(RAILS_CT, &key).as_deref(), Some("s3cret-from-rails"));
    }
}

// Integrations::Hook `encrypts :access_token, deterministic: true` (moved from slack.rs).
// Rails encrypts this attribute deterministically, but only when Chatwoot.encryption_configured?
// (primary + deterministic + salt all present, config/application.rb). Otherwise it stores plaintext.
fn det_key() -> Result<Option<[u8; 32]>, crate::common::ApiError> {
    let primary = std::env::var("ACTIVE_RECORD_ENCRYPTION_PRIMARY_KEY").unwrap_or_default();
    let salt = std::env::var("ACTIVE_RECORD_ENCRYPTION_KEY_DERIVATION_SALT").unwrap_or_default();
    let deterministic = std::env::var("ACTIVE_RECORD_ENCRYPTION_DETERMINISTIC_KEY").unwrap_or_default();
    if primary.is_empty() || salt.is_empty() || deterministic.is_empty() { return Ok(None); }
    let mut key = [0; 32];
    pbkdf2::pbkdf2_hmac::<Sha1>(deterministic.as_bytes(), salt.as_bytes(), 65_536, &mut key);
    Ok(Some(key))
}
pub fn encrypt_deterministic(token: &str) -> Result<String, crate::common::ApiError> {
    let Some(key) = det_key()? else { return Ok(token.into()) };
    let compressed = token.len() > 140;
    let data = if compressed {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(token.as_bytes()).map_err(|_| crate::common::ApiError::bad("Token encryption failed"))?;
        encoder.finish().map_err(|_| crate::common::ApiError::bad("Token encryption failed"))?
    } else { token.as_bytes().to_vec() };
    let mut mac = <hmac::Hmac<sha2_compat::Sha256> as hmac::Mac>::new_from_slice(&key).expect("HMAC key");
    hmac::Mac::update(&mut mac, &data);
    let hash = hmac::Mac::finalize(mac).into_bytes();
    let iv = &hash[..12];
    let cipher = Aes256Gcm::new_from_slice(&key).expect("AES key");
    let mut encrypted = cipher.encrypt(Nonce::from_slice(iv), data.as_ref()).map_err(|_| crate::common::ApiError::bad("Token encryption failed"))?;
    let tag = encrypted.split_off(encrypted.len() - 16);
    let mut headers = json!({"iv": STANDARD.encode(iv), "at": STANDARD.encode(tag)});
    if compressed { headers["c"] = json!(true); }
    Ok(json!({"p": STANDARD.encode(encrypted), "h": headers}).to_string())
}
pub fn decrypt_deterministic(token: &str) -> Result<String, crate::common::ApiError> {
    let Some(key) = det_key()? else { return Ok(token.into()) };
    let Ok(v) = serde_json::from_str::<Value>(token) else { return Ok(token.into()) };
    let fail = || crate::common::ApiError(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Cannot decrypt token".into());
    let decode = |v: &Value| STANDARD.decode(v.as_str().ok_or_else(fail)?).map_err(|_| fail());
    let iv = decode(&v["h"]["iv"])?;
    let tag = decode(&v["h"]["at"])?;
    if iv.len() != 12 || tag.len() != 16 { return Err(fail()); }
    let mut payload = decode(&v["p"])?;
    payload.extend(tag);
    let mut plain = Aes256Gcm::new_from_slice(&key).expect("AES key").decrypt(Nonce::from_slice(&iv), payload.as_ref()).map_err(|_| fail())?;
    if v["h"]["c"] == true {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(plain.as_slice()).read_to_end(&mut out).map_err(|_| fail())?;
        plain = out;
    }
    String::from_utf8(plain).map_err(|_| fail())
}

