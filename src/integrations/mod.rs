//! Account integrations: Linear, Notion, and Stripe.
pub mod linear;
pub mod notion;
pub mod stripe;

use crate::{ar_encryption, common::ApiError};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

pub(crate) async fn load_hook(
    db: &PgPool,
    account_id: i64,
    app_id: &str,
) -> Result<Option<(i64, Option<String>, Option<String>, Value, i32)>, ApiError> {
    let row = sqlx::query(
        "SELECT id::bigint AS id, access_token, reference_id, COALESCE(settings, '{}'::jsonb) AS settings, COALESCE(status, 0) AS status
         FROM integrations_hooks
         WHERE account_id = $1 AND app_id = $2
         ORDER BY id LIMIT 1",
    )
    .bind(account_id as i32)
    .bind(app_id)
    .fetch_optional(db)
    .await?;
    let Some(r) = row else {
        return Ok(None);
    };
    let raw_tok: Option<String> = r.try_get("access_token")?;
    let tok = raw_tok.map(|s| ar_encryption::decrypt(&s));
    Ok(Some((
        r.try_get("id")?,
        tok,
        r.try_get("reference_id")?,
        r.try_get("settings")?,
        r.try_get("status")?,
    )))
}

pub(crate) async fn upsert_hook(
    db: &PgPool,
    account_id: i64,
    app_id: &str,
    access_token: &str,
    reference_id: Option<&str>,
    settings: Value,
) -> Result<i64, ApiError> {
    let enc_token = ar_encryption::encrypt(access_token);
    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM integrations_hooks WHERE account_id = $1 AND app_id = $2 ORDER BY id LIMIT 1",
    )
    .bind(account_id as i32)
    .bind(app_id)
    .fetch_optional(db)
    .await?;
    if let Some(id) = existing {
        sqlx::query(
            "UPDATE integrations_hooks
             SET access_token = $2, status = 1, reference_id = COALESCE($3, reference_id),
                 settings = COALESCE(settings, '{}'::jsonb) || $4::jsonb, updated_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(&enc_token)
        .bind(reference_id)
        .bind(&settings)
        .execute(db)
        .await?;
        Ok(id)
    } else {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO integrations_hooks (account_id, app_id, hook_type, status, reference_id, access_token, settings, created_at, updated_at)
             VALUES ($1, $2, 0, 1, $3, $4, $5::jsonb, now(), now()) RETURNING id::bigint",
        )
        .bind(account_id as i32)
        .bind(app_id)
        .bind(reference_id)
        .bind(&enc_token)
        .bind(&settings)
        .fetch_one(db)
        .await?;
        Ok(id)
    }
}

pub(crate) async fn delete_hook(
    db: &PgPool,
    account_id: i64,
    app_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM integrations_hooks WHERE account_id = $1 AND app_id = $2")
        .bind(account_id as i32)
        .bind(app_id)
        .execute(db)
        .await?;
    Ok(())
}

pub(crate) async fn post_form_json(
    url: &str,
    pairs: &[(&str, &str)],
    basic_auth: Option<(&str, &str)>,
) -> Result<Value, ApiError> {
    let body = {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in pairs {
            ser.append_pair(k, v);
        }
        ser.finish()
    };
    let client = reqwest::Client::new();
    let mut req = client
        .post(url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body);
    if let Some((u, p)) = basic_auth {
        req = req.basic_auth(u, Some(p));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("OAuth request failed: {e}")))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad(format!("OAuth response failed: {e}")))?;
    let val: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    if !status.is_success() {
        return Err(ApiError::bad(
            val["error_description"]
                .as_str()
                .or_else(|| val["error"].as_str())
                .unwrap_or("OAuth token exchange failed"),
        ));
    }
    Ok(val)
}
