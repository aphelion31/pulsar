//! OWNED BY A WORKER. See WORKERS.md for the contract.
//! File storage + attachments + avatars. `routes()` is merged at the ROOT router (full paths, e.g. /rails/active_storage/...).
//!
//! Wiring for the coordinator:
//! - Message with files: on `POST /api/v1/accounts/{account_id}/conversations/{display_id}/messages`
//!   (multipart), call `storage::create_with_attachments(&st, &user, account_id, display_id, multipart).await`.
//!   Put `DefaultBodyLimit::max(storage::MAX_BYTES + 1024 * 1024)` on that route only.
//! - Message listing: add `"attachments": storage::attachments_json(&st.db, msg_id).await` per message.
//! - Profile avatar: `storage::set_avatar(&st.db, "User", user.id, field).await?` returns URL.
//!   Read URL with `storage::avatar_url(&st.db, "User", user.id).await`.
//!   Delete with `storage::remove_avatar(&st.db, "User", user.id).await?`.
// ponytail: dead_code allowed until coordinator wires the pub(crate) helpers into conversations/profile routes.
#![allow(dead_code)]

use std::path::{Path as FsPath, PathBuf};

use axum::{
    Json, Router,
    extract::{Multipart, Path, State, multipart::Field, multipart::MultipartError},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use chrono::NaiveDateTime;
use rand::Rng;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::common::{ApiError, ApiResult, AppState, CurrentUser, now, require_account};
use crate::conversations::{CONV_COLS, Conv, MSG_COLS, Msg, emit, msg_json, reload};

pub const MAX_BYTES: usize = 40 * 1024 * 1024;
const DEFAULT_SECRET: &str = "pulsar-dev-secret-change-me";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/widget/direct_uploads", post(widget_direct_upload_create))
        .route("/rails/active_storage/disk_uploads/{key}", put(disk_upload_put))
        .route("/rails/active_storage/blobs/redirect/{signed_id}/{filename}", get(serve))
        .route("/rails/active_storage/blobs/{signed_id}/{filename}", get(serve))
}

// ---------- disk service (Rails ActiveStorage Disk layout) ----------

fn storage_root() -> PathBuf {
    PathBuf::from(std::env::var("STORAGE_DIR").unwrap_or_else(|_| "./storage".into()))
}

fn valid_key(key: &str) -> bool {
    key.len() >= 4 && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn blob_path_in(root: &FsPath, key: &str) -> PathBuf {
    root.join(&key[0..2]).join(&key[2..4]).join(key)
}

fn blob_path(key: &str) -> PathBuf {
    blob_path_in(&storage_root(), key)
}

fn new_key() -> String {
    const C: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::thread_rng();
    (0..28).map(|_| C[rng.gen_range(0..C.len())] as char).collect()
}

fn io_err(e: std::io::Error) -> ApiError {
    tracing::error!("storage io: {e}");
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, "Internal error".into())
}

fn mp_err(e: MultipartError) -> ApiError {
    ApiError(e.status(), e.body_text())
}

async fn write_file(path: &FsPath, bytes: &[u8]) -> Result<(), ApiError> {
    if let Some(p) = path.parent() {
        tokio::fs::create_dir_all(p).await.map_err(io_err)?;
    }
    tokio::fs::write(path, bytes).await.map_err(io_err)
}

/// Strip path components and control chars, cap length.
fn clean_name(raw: &str) -> String {
    let base: String = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(255)
        .collect();
    let t = base.trim();
    if t.is_empty() { "file".into() } else { t.to_string() }
}

/// Keep only a plausible `type/subtype` value, otherwise octet-stream (header-safe).
fn clean_ct(raw: Option<&str>) -> String {
    match raw {
        Some(c) if c.contains('/') && c.bytes().all(|b| b.is_ascii_graphic() && b != b';' || b == b';') && c.len() <= 255 => {
            c.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
        }
        _ => "application/octet-stream".into(),
    }
}

/// Insert file on disk plus `active_storage_blobs` row. Returns (blob_id, key).
pub(crate) async fn save_blob(db: &PgPool, bytes: &[u8], filename: &str, content_type: &str) -> Result<(i64, String), ApiError> {
    let key = new_key();
    let path = blob_path(&key);
    write_file(&path, bytes).await?;
    // ponytail: checksum is base64(sha256[..16]), not Rails' base64(md5). Nothing verifies it on read.
    let checksum = B64.encode(&Sha256::digest(bytes).to_vec()[..16]);
    let res: Result<i64, sqlx::Error> = sqlx::query_scalar(
        "INSERT INTO active_storage_blobs (key, filename, content_type, metadata, byte_size, checksum, created_at, service_name)
         VALUES ($1, $2, $3, '{\"identified\":true}', $4, $5, $6, 'local') RETURNING id::bigint",
    )
    .bind(&key)
    .bind(filename)
    .bind(content_type)
    .bind(bytes.len() as i64)
    .bind(&checksum)
    .bind(now())
    .fetch_one(db)
    .await;
    match res {
        Ok(id) => Ok((id, key)),
        Err(e) => {
            let _ = tokio::fs::remove_file(&path).await;
            Err(e.into())
        }
    }
}

/// Link blob to a record (Rails `attach`). Used for `Attachment#file` and `User#avatar`.
pub(crate) async fn attach(db: &PgPool, record_type: &str, record_id: i64, name: &str, blob_id: i64) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO active_storage_attachments (name, record_type, record_id, blob_id, created_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(name)
    .bind(record_type)
    .bind(record_id)
    .bind(blob_id)
    .bind(now())
    .execute(db)
    .await?;
    Ok(())
}

/// Replace the single attachment `(record_type, record_id, name)` in one transaction.
/// Returns the blob ids that were detached so the caller can purge them.
async fn swap_attachment(db: &PgPool, record_type: &str, record_id: i64, name: &str, blob_id: Option<i64>) -> Result<Vec<i64>, ApiError> {
    let mut tx = db.begin().await?;
    let old: Vec<i64> = sqlx::query_scalar(
        "DELETE FROM active_storage_attachments WHERE record_type = $1 AND record_id = $2 AND name = $3 RETURNING blob_id",
    )
    .bind(record_type)
    .bind(record_id)
    .bind(name)
    .fetch_all(&mut *tx)
    .await?;
    if let Some(b) = blob_id {
        sqlx::query("INSERT INTO active_storage_attachments (name, record_type, record_id, blob_id, created_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(name)
            .bind(record_type)
            .bind(record_id)
            .bind(b)
            .bind(now())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(old)
}

/// Delete blob rows (and files) that no attachment references anymore.
async fn purge_blobs(db: &PgPool, ids: &[i64]) {
    for id in ids {
        let key: Option<String> = sqlx::query_scalar(
            "DELETE FROM active_storage_blobs b WHERE b.id = $1
               AND NOT EXISTS (SELECT 1 FROM active_storage_attachments a WHERE a.blob_id = b.id)
             RETURNING key",
        )
        .bind(id)
        .fetch_optional(db)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("purge blob {id}: {e}");
            None
        });
        if let Some(k) = key {
            let _ = tokio::fs::remove_file(blob_path(&k)).await;
        }
    }
}

// ---------- signed URLs ----------

fn secret() -> String {
    std::env::var("SECRET_KEY_BASE").unwrap_or_else(|_| DEFAULT_SECRET.into())
}

/// RFC 2104 HMAC-SHA256 built on sha2 (no extra crate).
fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let d = Sha256::digest(key).to_vec();
        k[..32].copy_from_slice(&d);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(msg);
    let inner = inner.finalize().to_vec();
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(&inner);
    outer.finalize().to_vec()
}

fn sig_for(blob_id: i64) -> String {
    hex::encode(hmac_sha256(secret().as_bytes(), format!("blob:{blob_id}").as_bytes()))[..32].to_string()
}

fn signed_id(blob_id: i64) -> String {
    format!("{blob_id}--{}", sig_for(blob_id))
}

fn verify_signed_id(s: &str) -> Option<i64> {
    let (id_s, sig) = s.split_once("--")?;
    let id: i64 = id_s.parse().ok()?;
    if id <= 0 {
        return None;
    }
    let expected = sig_for(id);
    // constant-time compare
    if sig.len() != expected.len() {
        return None;
    }
    let diff = sig.bytes().zip(expected.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b));
    if diff == 0 { Some(id) } else { None }
}

/// Percent-encode everything except RFC 3986 unreserved chars.
fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~' {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

pub(crate) fn blob_url(blob_id: i64, filename: &str) -> String {
    format!("/rails/active_storage/blobs/redirect/{}/{}", signed_id(blob_id), pct(filename))
}

// ---------- serving ----------

fn inline_ok(ct: &str) -> bool {
    (ct.starts_with("image/") && ct != "image/svg+xml")
        || ct.starts_with("audio/")
        || ct.starts_with("video/")
        || ct == "application/pdf"
        || ct == "text/plain"
}

async fn serve(State(st): State<AppState>, Path((signed, _filename)): Path<(String, String)>) -> ApiResult<Response> {
    let blob_id = verify_signed_id(&signed).ok_or_else(ApiError::not_found)?;
    let row: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT key, filename, content_type FROM active_storage_blobs WHERE id = $1")
            .bind(blob_id)
            .fetch_optional(&st.db)
            .await?;
    let (key, filename, ct) = row.ok_or_else(ApiError::not_found)?;
    if !valid_key(&key) {
        return Err(ApiError::not_found());
    }
    let bytes = tokio::fs::read(blob_path(&key)).await.map_err(|_| ApiError::not_found())?;
    let ct = ct.filter(|c| !c.is_empty()).unwrap_or_else(|| "application/octet-stream".into());
    let disp = if inline_ok(&ct) { "inline" } else { "attachment" };
    let ascii: String = filename.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' { c } else { '_' }).collect();
    let cd = format!("{disp}; filename=\"{ascii}\"; filename*=UTF-8''{}", pct(&filename));
    Ok((
        [
            (header::CONTENT_TYPE, ct),
            (header::CONTENT_DISPOSITION, cd),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        bytes,
    )
        .into_response())
}

// ---------- attachments in messages ----------

fn file_type_name(t: i32) -> &'static str {
    match t {
        0 => "image",
        1 => "audio",
        2 => "video",
        4 => "location",
        5 => "fallback",
        6 => "share",
        7 => "story_mention",
        _ => "file",
    }
}

fn file_type_of(ct: &str) -> i32 {
    if ct.starts_with("image/") {
        0
    } else if ct.starts_with("audio/") {
        1
    } else if ct.starts_with("video/") {
        2
    } else {
        3
    }
}

fn extension_of(filename: &str) -> Option<String> {
    let (_, e) = filename.rsplit_once('.')?;
    if e.is_empty() || e.len() > 16 { None } else { Some(e.to_ascii_lowercase()) }
}

/// Attachment JSON in the shape of `_attachment.json.jbuilder`. Empty when the message has none.
pub(crate) async fn attachments_json(db: &PgPool, message_id: i64) -> Vec<Value> {
    let rows: Vec<(i64, i32, Option<String>, i64, i64, NaiveDateTime, i64, String, Option<String>, i64)> = sqlx::query_as(
        "SELECT at.id::bigint, COALESCE(at.file_type, 3), at.extension, at.account_id::bigint, at.message_id::bigint, at.created_at,
                b.id, b.filename, b.content_type, b.byte_size
         FROM attachments at
         JOIN active_storage_attachments asa ON asa.record_type = 'Attachment' AND asa.record_id = at.id AND asa.name = 'file'
         JOIN active_storage_blobs b ON b.id = asa.blob_id
         WHERE at.message_id = $1 ORDER BY at.id",
    )
    .bind(message_id)
    .fetch_all(db)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("attachments for message {message_id}: {e}");
        vec![]
    });
    rows.into_iter()
        .map(|(id, ft, ext, account_id, msg_id, created, blob_id, filename, ct, size)| {
            let data_url = blob_url(blob_id, &filename);
            let is_image = ct.as_deref().is_some_and(|c| c.starts_with("image/"));
            json!({
                "id": id, "message_id": msg_id, "account_id": account_id,
                "file_type": file_type_name(ft), "extension": ext,
                "data_url": data_url,
                "thumb_url": if is_image { data_url.clone() } else { String::new() },
                "file_size": size, "width": null, "height": null,
                "created_at": created.and_utc().timestamp(),
            })
        })
        .collect()
}

fn multipart_text_flag(s: &str) -> bool {
    matches!(s.trim(), "true" | "1")
}

/// POST messages with multipart fields: content, private, echo_id, content_attributes (JSON string),
/// attachments[] (files). Writes blobs first, then the message, then attachment rows, then broadcasts
/// one message.created with attachments already populated.
pub(crate) async fn create_with_attachments(
    st: &AppState,
    user: &CurrentUser,
    account_id: i64,
    display_id: i64,
    mut multipart: Multipart,
) -> ApiResult {
    let au = require_account(&st.db, user, account_id).await?;
    let conv: Conv = sqlx::query_as(&format!("SELECT {CONV_COLS} FROM conversations c WHERE c.account_id = $1 AND c.display_id = $2"))
        .bind(account_id)
        .bind(display_id)
        .fetch_optional(&st.db)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if !au.is_admin() {
        let member: Option<i32> = sqlx::query_scalar("SELECT 1 FROM inbox_members WHERE inbox_id = $1 AND user_id = $2")
            .bind(conv.inbox_id)
            .bind(user.id)
            .fetch_optional(&st.db)
            .await?;
        if member.is_none() {
            return Err(ApiError::not_found());
        }
    }

    let mut content: Option<String> = None;
    let mut private = false;
    let mut echo_id: Option<String> = None;
    let mut content_attributes = json!({});
    let mut files: Vec<(String, String, Vec<u8>)> = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(mp_err)? {
        match field.name().unwrap_or("") {
            "content" => content = Some(field.text().await.map_err(mp_err)?),
            "private" => private = multipart_text_flag(&field.text().await.map_err(mp_err)?),
            "echo_id" => echo_id = Some(field.text().await.map_err(mp_err)?),
            "content_attributes" => {
                let t = field.text().await.map_err(mp_err)?;
                if !t.trim().is_empty() {
                    content_attributes = serde_json::from_str(&t).map_err(|_| ApiError::bad("content_attributes must be JSON"))?;
                    if !content_attributes.is_object() {
                        return Err(ApiError::bad("content_attributes must be an object"));
                    }
                }
            }
            "attachments[]" | "attachments" => {
                let Some(fname) = field.file_name().map(clean_name) else { continue };
                let ct = clean_ct(field.content_type());
                let bytes = field.bytes().await.map_err(mp_err)?;
                if bytes.len() > MAX_BYTES {
                    return Err(ApiError::bad("File is too large (max 40 MB)"));
                }
                files.push((fname, ct, bytes.to_vec()));
            }
            _ => {}
        }
    }
    let content = content.filter(|c| !c.trim().is_empty());
    if content.is_none() && files.is_empty() {
        return Err(ApiError::bad("Content or attachments required"));
    }

    // 1. blobs on disk first (orphans are harmless, purge later)
    let mut blobs: Vec<(i64, String, String)> = Vec::with_capacity(files.len());
    for (fname, ct, bytes) in &files {
        let (blob_id, _) = save_blob(&st.db, bytes, fname, ct).await?;
        blobs.push((blob_id, fname.clone(), ct.clone()));
    }

    // 2. message row (outgoing, 1)
    let msg_id: i64 = sqlx::query_scalar(
        "INSERT INTO messages (content, account_id, inbox_id, conversation_id, message_type, private, status, content_type,
            content_attributes, sender_type, sender_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 1, $5, 0, 0, $6::json, 'User', $7, now(), now()) RETURNING id::bigint",
    )
    .bind(&content)
    .bind(conv.account_id as i32)
    .bind(conv.inbox_id as i32)
    .bind(conv.id as i32)
    .bind(private)
    .bind(&content_attributes)
    .bind(user.id)
    .fetch_one(&st.db)
    .await?;
    // ponytail: mirrors the activity bookkeeping of conversations::insert_message for outgoing messages
    sqlx::query(
        "UPDATE conversations SET last_activity_at = now(), updated_at = now(),
           first_reply_created_at = CASE WHEN NOT $2 THEN COALESCE(first_reply_created_at, now()) ELSE first_reply_created_at END,
           waiting_since = CASE WHEN NOT $2 THEN NULL ELSE waiting_since END
         WHERE id = $1",
    )
    .bind(conv.id)
    .bind(private)
    .execute(&st.db)
    .await?;

    // 3. attachment rows + blob links
    for (blob_id, fname, ct) in &blobs {
        let att_id: i64 = sqlx::query_scalar(
            "INSERT INTO attachments (file_type, message_id, account_id, extension, created_at, updated_at)
             VALUES ($1, $2, $3, $4, now(), now()) RETURNING id::bigint",
        )
        .bind(file_type_of(ct))
        .bind(msg_id as i32)
        .bind(conv.account_id as i32)
        .bind(extension_of(fname))
        .fetch_one(&st.db)
        .await?;
        attach(&st.db, "Attachment", att_id, "file", *blob_id).await?;
    }

    // 4. broadcast with attachments populated
    let m: Msg = sqlx::query_as(&format!("SELECT {MSG_COLS} FROM messages m WHERE m.id = $1"))
        .bind(msg_id)
        .fetch_one(&st.db)
        .await?;
    let conv = reload(&st.db, conv.id).await?;
    let mut data = msg_json(&st.db, &m, &conv).await;
    data["attachments"] = json!(attachments_json(&st.db, msg_id).await);
    if let Some(e) = echo_id {
        data["echo_id"] = json!(e);
    }
    emit(st, &conv, "message.created", data.clone()).await;
    Ok(Json(data))
}

// ---------- avatars ----------

/// Store an uploaded image as the record's `avatar`, replacing the previous one. Returns its URL.
pub(crate) async fn set_avatar(db: &PgPool, record_type: &str, record_id: i64, field: Field<'_>) -> Result<String, ApiError> {
    let ct = clean_ct(field.content_type());
    if !ct.starts_with("image/") {
        return Err(ApiError::bad("Avatar must be an image"));
    }
    let fname = clean_name(field.file_name().unwrap_or("avatar"));
    let bytes = field.bytes().await.map_err(mp_err)?;
    if bytes.len() > MAX_BYTES {
        return Err(ApiError::bad("File is too large (max 40 MB)"));
    }
    let (blob_id, _) = save_blob(db, &bytes, &fname, &ct).await?;
    let old = swap_attachment(db, record_type, record_id, "avatar", Some(blob_id)).await?;
    purge_blobs(db, &old).await;
    Ok(blob_url(blob_id, &fname))
}

/// URL of the record's avatar, or "" when none.
pub(crate) async fn avatar_url(db: &PgPool, record_type: &str, record_id: i64) -> String {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT b.id, b.filename FROM active_storage_attachments a
         JOIN active_storage_blobs b ON b.id = a.blob_id
         WHERE a.record_type = $1 AND a.record_id = $2 AND a.name = 'avatar'
         ORDER BY a.id DESC LIMIT 1",
    )
    .bind(record_type)
    .bind(record_id)
    .fetch_optional(db)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("avatar_url: {e}");
        None
    });
    row.map(|(id, f)| blob_url(id, &f)).unwrap_or_default()
}

/// Detach and purge the record's avatar.
pub(crate) async fn remove_avatar(db: &PgPool, record_type: &str, record_id: i64) -> Result<(), ApiError> {
    let old = swap_attachment(db, record_type, record_id, "avatar", None).await?;
    purge_blobs(db, &old).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_id_roundtrip() {
        let s = signed_id(42);
        assert_eq!(verify_signed_id(&s), Some(42));
        assert_eq!(verify_signed_id(&format!("43--{}", &s[4..])), None);
        assert_eq!(verify_signed_id("42--deadbeef"), None);
        assert_eq!(verify_signed_id("0--x"), None);
    }

    #[test]
    fn key_validation_and_layout() {
        assert!(valid_key("abcd1234x9"));
        assert!(!valid_key("../etc"));
        assert!(!valid_key("ab"));
        assert!(!valid_key("AB12cd34"));
        assert_eq!(blob_path_in(FsPath::new("s"), "abcdefgh"), PathBuf::from("s/ab/cd/abcdefgh"));
    }

    #[test]
    fn hmac_rfc4231_case2() {
        let out = hex::encode(hmac_sha256(b"Jefe", b"what do ya want for nothing?"));
        assert_eq!(out, "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }
}

// ---------- direct uploads (ActiveStorage DirectUploadsController protocol) ----------

/// Signed PUT target for a pending blob. Rails signs the disk URL too; we HMAC the key.
fn direct_upload_token(key: &str) -> String {
    hex::encode(hmac_sha256(secret().as_bytes(), format!("upload:{key}").as_bytes()))[..32].to_string()
}

fn direct_upload_url(key: &str) -> String {
    format!("/rails/active_storage/disk_uploads/{key}?token={}", direct_upload_token(key))
}

/// POST /api/v1/widget/direct_uploads?website_token= {blob:{filename,content_type,byte_size,checksum}}
/// Needs a visitor session (X-Auth-Token), same gate as Chatwoot's `set_contact`.
pub(crate) async fn widget_direct_upload_create(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = q.get("website_token").map(String::as_str);
    crate::widget::check_visitor(&st, token, &headers).await?;
    let b = &body["blob"];
    let filename = clean_name(b["filename"].as_str().unwrap_or("file"));
    let ct = clean_ct(b["content_type"].as_str());
    let size = b["byte_size"].as_i64().filter(|s| *s >= 0).ok_or_else(|| ApiError::bad("byte_size is required"))?;
    if size as usize > MAX_BYTES {
        return Err(ApiError::bad("File is too large (max 40 MB)"));
    }
    let checksum = b["checksum"].as_str().unwrap_or_default().to_string();
    let key = new_key();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO active_storage_blobs (key, filename, content_type, metadata, byte_size, checksum, created_at, service_name)
         VALUES ($1, $2, $3, '{\"identified\":true}', $4, $5, $6, 'local') RETURNING id::bigint",
    )
    .bind(&key)
    .bind(&filename)
    .bind(&ct)
    .bind(size)
    .bind(&checksum)
    .bind(now())
    .fetch_one(&st.db)
    .await?;
    Ok(Json(json!({
        "id": id,
        "key": key,
        "filename": filename,
        "content_type": ct,
        "metadata": { "identified": true },
        "byte_size": size,
        "checksum": checksum,
        "signed_id": signed_id(id),
        "direct_upload": {
            "url": direct_upload_url(&key),
            "headers": { "Content-Type": ct },
        },
    })))
}

/// PUT /rails/active_storage/disk_uploads/{key}?token=: stores the bytes of a pending blob.
async fn disk_upload_put(
    State(st): State<AppState>,
    Path(key): Path<String>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: axum::body::Bytes,
) -> ApiResult<StatusCode> {
    let tok = q.get("token").cloned().unwrap_or_default();
    if !valid_key(&key) || tok != direct_upload_token(&key) {
        return Err(ApiError::forbidden());
    }
    let row: Option<(i64, i64)> = sqlx::query_as("SELECT id::bigint, byte_size FROM active_storage_blobs WHERE key = $1")
        .bind(&key)
        .fetch_optional(&st.db)
        .await?;
    let (_, size) = row.ok_or_else(ApiError::not_found)?;
    if body.len() as i64 != size {
        return Err(ApiError::bad("Uploaded size does not match byte_size"));
    }
    write_file(&blob_path(&key), &body).await?;
    Ok(StatusCode::NO_CONTENT)
}
