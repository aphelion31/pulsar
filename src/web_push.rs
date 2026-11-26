//! Native RFC 8291 (`aes128gcm`) payload encryption and RFC 8292 VAPID ES256 authentication.

use aes_gcm::{
    Aes128Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD},
};
use hmac::{Hmac, Mac};
use p256::{
    PublicKey, SecretKey,
    ecdh::diffie_hellman,
    ecdsa::{Signature, SigningKey, signature::Signer},
    elliptic_curve::sec1::ToEncodedPoint,
};
use rand::RngCore;
use serde_json::json;
use sha2_compat::Sha256;

const DEFAULT_RECORD_SIZE: u32 = 4096;
const VAPID_TTL_SECS: i64 = 12 * 3600;

type HmacSha256 = Hmac<Sha256>;

fn decode_b64_any(raw: &str) -> anyhow::Result<Vec<u8>> {
    let trimmed = raw.trim();
    anyhow::ensure!(!trimmed.is_empty(), "empty key material");
    URL_SAFE_NO_PAD
        .decode(trimmed)
        .or_else(|_| URL_SAFE.decode(trimmed))
        .or_else(|_| STANDARD.decode(trimmed))
        .map_err(|e| anyhow::anyhow!("invalid base64 key: {e}"))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key size");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

fn hkdf_expand_one(prk: &[u8; 32], info: &[u8], len: usize) -> Vec<u8> {
    debug_assert!(len <= 32);
    let mut mac = <HmacSha256 as Mac>::new_from_slice(prk).expect("HMAC accepts 32-byte PRK");
    mac.update(info);
    mac.update(&[0x01]);
    mac.finalize().into_bytes()[..len].to_vec()
}

fn env_nonempty(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
}

pub(crate) fn vapid_configured() -> bool {
    env_nonempty(&["VAPID_PRIVATE_KEY", "WEB_PUSH_VAPID_PRIVATE_KEY"]).is_some()
}

/// Returns the URL-safe base64 VAPID public key (from explicit env or derived from the P-256 private key).
pub(crate) fn vapid_public_key() -> Option<String> {
    if let Some(raw_pub) = env_nonempty(&["VAPID_PUBLIC_KEY", "WEB_PUSH_VAPID_PUBLIC_KEY"])
        && let Ok(bytes) = decode_b64_any(&raw_pub)
    {
        return Some(URL_SAFE_NO_PAD.encode(bytes));
    }
    let raw_priv = env_nonempty(&["VAPID_PRIVATE_KEY", "WEB_PUSH_VAPID_PRIVATE_KEY"])?;
    let sk = parse_vapid_signing_key(&raw_priv).ok()?;
    Some(URL_SAFE_NO_PAD.encode(sk.verifying_key().to_encoded_point(false).as_bytes()))
}

/// Encrypts `plaintext` for a subscription using RFC 8291 (`aes128gcm` single record).
pub(crate) fn encrypt_aes128gcm(
    ua_public_b64: &str,
    auth_secret_b64: &str,
    plaintext: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let ua_pub = decode_b64_any(ua_public_b64)?;
    let auth = decode_b64_any(auth_secret_b64)?;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let as_secret = SecretKey::random(&mut rand::thread_rng());
    encrypt_aes128gcm_with_key_and_salt(&ua_pub, &auth, &as_secret, &salt, plaintext, DEFAULT_RECORD_SIZE)
}

pub(crate) fn encrypt_aes128gcm_with_key_and_salt(
    ua_pub_bytes: &[u8],
    auth_secret: &[u8],
    as_secret: &SecretKey,
    salt: &[u8; 16],
    plaintext: &[u8],
    record_size: u32,
) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(auth_secret.len() == 16, "auth secret must be 16 bytes");
    anyhow::ensure!(
        record_size >= 18 && plaintext.len() + 17 <= record_size as usize,
        "plaintext exceeds single aes128gcm record size"
    );
    let ua_pub = PublicKey::from_sec1_bytes(ua_pub_bytes)?;
    let ua_sec1 = ua_pub.to_encoded_point(false);
    anyhow::ensure!(ua_sec1.as_bytes().len() == 65, "invalid uncompressed UA key");
    let as_sec1 = as_secret.public_key().to_encoded_point(false);
    let as_bytes = as_sec1.as_bytes();
    anyhow::ensure!(as_bytes.len() == 65, "invalid uncompressed AS key");

    let shared = diffie_hellman(as_secret.to_nonzero_scalar(), ua_pub.as_affine());
    let prk_key = hmac_sha256(auth_secret, shared.raw_secret_bytes().as_slice());

    let mut key_info = Vec::with_capacity(14 + 65 + 65);
    key_info.extend_from_slice(b"WebPush: info\x00");
    key_info.extend_from_slice(ua_sec1.as_bytes());
    key_info.extend_from_slice(as_bytes);
    let ikm = hkdf_expand_one(&prk_key, &key_info, 32);

    let prk = hmac_sha256(salt, &ikm);
    let cek = hkdf_expand_one(&prk, b"Content-Encoding: aes128gcm\x00", 16);
    let nonce = hkdf_expand_one(&prk, b"Content-Encoding: nonce\x00", 12);

    let mut padded = Vec::with_capacity(plaintext.len() + 1);
    padded.extend_from_slice(plaintext);
    padded.push(0x02);

    let cipher = Aes128Gcm::new_from_slice(&cek)?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), padded.as_slice())
        .map_err(|_| anyhow::anyhow!("aes128gcm encryption failed"))?;

    let mut out = Vec::with_capacity(16 + 4 + 1 + 65 + ciphertext.len());
    out.extend_from_slice(salt);
    out.extend_from_slice(&record_size.to_be_bytes());
    out.push(65u8);
    out.extend_from_slice(as_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn endpoint_audience(raw_url: &str) -> anyhow::Result<String> {
    let url = url::Url::parse(raw_url)?;
    let scheme = url.scheme();
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("push endpoint missing host"))?;
    let host_part = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    Ok(match url.port() {
        Some(port) => format!("{scheme}://{host_part}:{port}"),
        None => format!("{scheme}://{host_part}"),
    })
}

fn parse_vapid_signing_key(raw: &str) -> anyhow::Result<SigningKey> {
    let trimmed = raw.trim();
    let secret = if let Ok(bytes) = decode_b64_any(trimmed) {
        if bytes.len() == 32 {
            SecretKey::from_slice(&bytes)?
        } else {
            return Err(anyhow::anyhow!("VAPID private key must decode to 32 bytes"));
        }
    } else if trimmed.len() == 64 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        let bytes = hex::decode(trimmed)?;
        SecretKey::from_slice(&bytes)?
    } else {
        return Err(anyhow::anyhow!("unsupported VAPID private key format"));
    };
    Ok(SigningKey::from(secret))
}

/// Builds an RFC 8292 `vapid t=<jwt>, k=<pub>` Authorization header value.
pub(crate) fn sign_vapid_header(
    endpoint: &str,
    subject: &str,
    private_key_raw: &str,
    public_key_override: Option<&str>,
    exp_unix: i64,
) -> anyhow::Result<String> {
    let signing_key = parse_vapid_signing_key(private_key_raw)?;
    let aud = endpoint_audience(endpoint)?;
    let pub_key_b64 = match public_key_override.map(str::trim).filter(|s| !s.is_empty()) {
        Some(explicit) => URL_SAFE_NO_PAD.encode(decode_b64_any(explicit)?),
        None => URL_SAFE_NO_PAD.encode(
            signing_key
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes(),
        ),
    };
    let header_b64 = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = json!({ "aud": aud, "exp": exp_unix, "sub": subject });
    let claims_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);
    let signing_input = format!("{header_b64}.{claims_b64}");
    let sig: Signature = signing_key.sign(signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig.to_bytes());
    Ok(format!("vapid t={signing_input}.{sig_b64}, k={pub_key_b64}"))
}

/// Sends an encrypted RFC 8291 + VAPID RFC 8292 Web Push request using the SSRF-validated client.
pub(crate) async fn send_encrypted(
    endpoint: &str,
    p256dh: &str,
    auth: &str,
    payload: &[u8],
) -> anyhow::Result<reqwest::Response> {
    let priv_key = env_nonempty(&["VAPID_PRIVATE_KEY", "WEB_PUSH_VAPID_PRIVATE_KEY"])
        .ok_or_else(|| anyhow::anyhow!("VAPID_PRIVATE_KEY not configured"))?;
    let pub_key = env_nonempty(&["VAPID_PUBLIC_KEY", "WEB_PUSH_VAPID_PUBLIC_KEY"]);
    let subject = env_nonempty(&["VAPID_SUBJECT", "WEB_PUSH_VAPID_SUBJECT"])
        .unwrap_or_else(|| "mailto:support@pulsar.local".to_string());
    let body = encrypt_aes128gcm(p256dh, auth, payload)?;
    let exp = chrono::Utc::now().timestamp() + VAPID_TTL_SECS;
    let auth_header = sign_vapid_header(endpoint, &subject, &priv_key, pub_key.as_deref(), exp)?;
    let (url, client) = crate::listeners::webhooks::validated_client(endpoint, 10).await?;
    let resp = client
        .post(url)
        .header("content-type", "application/octet-stream")
        .header("content-encoding", "aes128gcm")
        .header("ttl", "2419200")
        .header("urgency", "normal")
        .header("authorization", auth_header)
        .body(body)
        .send()
        .await?;
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{VerifyingKey, signature::Verifier};

    #[test]
    fn rfc8291_deterministic_header_and_decrypt() {
        // Deterministic known-key test verifying RFC 8291 header layout (16B salt + 4B rs + 1B idlen + 65B AS pub)
        // and key derivation ("WebPush: info\0" + "Content-Encoding: aes128gcm\0" + "Content-Encoding: nonce\0").
        let ua_secret = SecretKey::from_slice(&[0x11u8; 32]).unwrap();
        let ua_public = ua_secret.public_key().to_encoded_point(false);
        let auth_secret = URL_SAFE_NO_PAD.decode("BTBZMqHH6r4Tts7J_aSIgg").unwrap();
        let as_key = SecretKey::from_slice(&[0x22u8; 32]).unwrap();
        let salt: [u8; 16] = URL_SAFE_NO_PAD
            .decode("DGv6ra1nlYgDCS1FRnbzlw")
            .unwrap()
            .try_into()
            .unwrap();
        let plaintext = b"When I grow up, I want to be a watermelon";
        let encrypted = encrypt_aes128gcm_with_key_and_salt(
            ua_public.as_bytes(),
            &auth_secret,
            &as_key,
            &salt,
            plaintext,
            4096,
        )
        .unwrap();
        assert_eq!(&encrypted[0..16], &salt);
        assert_eq!(&encrypted[16..20], &4096u32.to_be_bytes());
        assert_eq!(encrypted[20], 65);
        assert_eq!(&encrypted[21..86], as_key.public_key().to_encoded_point(false).as_bytes());
        assert_eq!(encrypted.len(), 86 + plaintext.len() + 1 + 16);

        let shared = diffie_hellman(
            ua_secret.to_nonzero_scalar(),
            as_key.public_key().as_affine(),
        );
        let prk_key = hmac_sha256(&auth_secret, shared.raw_secret_bytes().as_slice());
        let mut key_info = Vec::new();
        key_info.extend_from_slice(b"WebPush: info\x00");
        key_info.extend_from_slice(ua_public.as_bytes());
        key_info.extend_from_slice(&encrypted[21..86]);
        let ikm = hkdf_expand_one(&prk_key, &key_info, 32);
        let prk = hmac_sha256(&salt, &ikm);
        let cek = hkdf_expand_one(&prk, b"Content-Encoding: aes128gcm\x00", 16);
        let nonce = hkdf_expand_one(&prk, b"Content-Encoding: nonce\x00", 12);
        let decrypted = Aes128Gcm::new_from_slice(&cek)
            .unwrap()
            .decrypt(Nonce::from_slice(&nonce), &encrypted[86..])
            .unwrap();
        assert_eq!(decrypted, [plaintext.as_slice(), &[0x02]].concat());
    }

    #[test]
    fn roundtrip_encrypt_decrypt_and_vapid_es256() {
        let ua_secret = SecretKey::random(&mut rand::thread_rng());
        let ua_pub_b64 = URL_SAFE_NO_PAD.encode(ua_secret.public_key().to_encoded_point(false).as_bytes());
        let mut auth = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut auth);
        let auth_b64 = URL_SAFE_NO_PAD.encode(auth);
        let msg = br#"{"title":"New message","body":"Hello"}"#;

        let packet = encrypt_aes128gcm(&ua_pub_b64, &auth_b64, msg).unwrap();
        assert!(packet.len() > 86 + msg.len());

        // Receiver decrypts packet per RFC 8291
        let salt = &packet[0..16];
        let rs = u32::from_be_bytes(packet[16..20].try_into().unwrap());
        assert_eq!(rs, 4096);
        let idlen = packet[20] as usize;
        assert_eq!(idlen, 65);
        let as_pub_bytes = &packet[21..21 + 65];
        let ciphertext = &packet[21 + 65..];
        let as_pub = PublicKey::from_sec1_bytes(as_pub_bytes).unwrap();
        let shared = diffie_hellman(ua_secret.to_nonzero_scalar(), as_pub.as_affine());
        let prk_key = hmac_sha256(&auth, shared.raw_secret_bytes().as_slice());
        let mut key_info = Vec::new();
        key_info.extend_from_slice(b"WebPush: info\x00");
        key_info.extend_from_slice(ua_secret.public_key().to_encoded_point(false).as_bytes());
        key_info.extend_from_slice(as_pub_bytes);
        let ikm = hkdf_expand_one(&prk_key, &key_info, 32);
        let prk = hmac_sha256(salt, &ikm);
        let cek = hkdf_expand_one(&prk, b"Content-Encoding: aes128gcm\x00", 16);
        let nonce = hkdf_expand_one(&prk, b"Content-Encoding: nonce\x00", 12);
        let decrypted = Aes128Gcm::new_from_slice(&cek)
            .unwrap()
            .decrypt(Nonce::from_slice(&nonce), ciphertext)
            .unwrap();
        assert_eq!(&decrypted[..decrypted.len() - 1], msg);
        assert_eq!(decrypted[decrypted.len() - 1], 0x02);

        // VAPID ES256 signature verification
        let vapid_priv = SecretKey::random(&mut rand::thread_rng());
        let vapid_priv_b64 = URL_SAFE_NO_PAD.encode(vapid_priv.to_bytes());
        let hdr = sign_vapid_header(
            "https://push.example.com:8443/wpush/v2/abc",
            "mailto:ops@example.com",
            &vapid_priv_b64,
            None,
            1_900_000_000,
        )
        .unwrap();
        let rest = hdr.strip_prefix("vapid t=").unwrap();
        let (jwt, k_b64) = rest.split_once(", k=").unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        let sig = Signature::from_slice(&sig_bytes).unwrap();
        let vk = VerifyingKey::from_sec1_bytes(&URL_SAFE_NO_PAD.decode(k_b64).unwrap()).unwrap();
        vk.verify(signing_input.as_bytes(), &sig).unwrap();
        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://push.example.com:8443");
        assert_eq!(claims["sub"], "mailto:ops@example.com");
        let sk = parse_vapid_signing_key(&vapid_priv_b64).unwrap();
        let derived = URL_SAFE_NO_PAD.encode(sk.verifying_key().to_encoded_point(false).as_bytes());
        assert_eq!(derived, k_b64);
    }
}
