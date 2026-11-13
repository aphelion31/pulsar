// Mirrors Rails 7.2 active_record/encryption/{encryptor,message_serializer,properties,
// key,key_provider,cipher/aes256_gcm}. load_defaults 7.0 uses PBKDF2-HMAC-SHA1.
use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use rand::RngCore;
use sha1::{Digest, Sha1};
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
