//! Account custom domain registration and DNS/Cloudflare verification.
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde_json::{Value, json};
use sqlx::Row;

use crate::common::{ApiError, ApiResult, AppState, CurrentUser, require_account};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/custom_domains", get(index).post(create))
        .route("/custom_domains/{id}", delete(destroy))
        .route("/custom_domains/{id}/verify", post(verify))
}

fn valid_domain(raw: &str) -> Option<String> {
    let d = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if d.is_empty()
        || d.len() > 253
        || !d.contains('.')
        || d.contains("://")
        || d.contains('/')
        || d.chars().any(char::is_whitespace)
    {
        return None;
    }
    if d.split('.')
        .any(|lbl| lbl.is_empty() || lbl.len() > 63 || !lbl.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    {
        return None;
    }
    Some(d)
}

fn row_json(r: &sqlx::postgres::PgRow) -> Value {
    let domain: String = r.get("domain");
    let token: String = r.get("verification_token");
    let status: String = r.get("status");
    json!({
        "id": r.get::<i64, _>("id"),
        "account_id": r.get::<i64, _>("account_id"),
        "domain": domain,
        "status": status,
        "verified": status == "verified",
        "verification_token": token,
        "dns_record": {
            "type": "TXT",
            "name": format!("_pulsar-challenge.{domain}"),
            "value": token,
        },
        "created_at": r.get::<i64, _>("created_at"),
        "updated_at": r.get::<i64, _>("updated_at"),
    })
}

async fn index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let rows = sqlx::query(
        "SELECT id, account_id, domain, status, verification_token,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at
         FROM pulsar_custom_domains
         WHERE account_id = $1
         ORDER BY id ASC",
    )
    .bind(account_id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(json!({
        "payload": rows.iter().map(row_json).collect::<Vec<_>>()
    })))
}

async fn create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let raw = body["domain"]
        .as_str()
        .or_else(|| body["custom_domain"]["domain"].as_str())
        .unwrap_or_default();
    let domain = valid_domain(raw).ok_or_else(|| ApiError::bad("Invalid domain name"))?;
    let token = format!("pulsar-verify-{}", uuid::Uuid::new_v4().simple());
    let row = sqlx::query(
        "INSERT INTO pulsar_custom_domains (account_id, domain, status, verification_token, created_at, updated_at)
         VALUES ($1, $2, 'pending', $3, now(), now())
         ON CONFLICT (account_id, domain) DO UPDATE SET updated_at = now()
         RETURNING id, account_id, domain, status, verification_token,
                   EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                   EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at",
    )
    .bind(account_id)
    .bind(&domain)
    .bind(&token)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(json!({ "payload": row_json(&row) })))
}

async fn verify_dns_txt(domain: &str, expected_token: &str) -> Result<bool, String> {
    if std::env::var("PULSAR_CUSTOM_DOMAIN_FORCE_VERIFY").as_deref() == Ok("1") {
        return Ok(true);
    }
    if let (Ok(cf_token), Ok(zone_id)) = (
        std::env::var("CLOUDFLARE_API_TOKEN"),
        std::env::var("CLOUDFLARE_ZONE_ID"),
    ) && !cf_token.trim().is_empty()
        && !zone_id.trim().is_empty()
    {
        let api_base = std::env::var("CLOUDFLARE_API_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "https://api.cloudflare.com/client/v4".into());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        let url = format!(
            "{}/zones/{}/custom_hostnames?hostname={}",
            api_base.trim_end_matches('/'),
            zone_id.trim(),
            url::form_urlencoded::byte_serialize(domain.as_bytes()).collect::<String>()
        );
        let resp = client
            .get(url)
            .bearer_auth(cf_token.trim())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status().is_success() {
            let v: Value =
                serde_json::from_slice(&resp.bytes().await.unwrap_or_default()).unwrap_or(Value::Null);
            let verified = v["result"]
                .as_array()
                .is_some_and(|arr| {
                    arr.iter().any(|item| {
                        item["status"].as_str() == Some("active")
                            || item["ssl"]["status"].as_str() == Some("active")
                    })
                });
            if verified {
                return Ok(true);
            }
        }
    }

    let doh = std::env::var("CUSTOM_DOMAIN_DOH_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://cloudflare-dns.com/dns-query".into());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    for host in [format!("_pulsar-challenge.{domain}"), domain.to_string()] {
        let qs = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("name", &host)
            .append_pair("type", "TXT")
            .finish();
        let Ok(resp) = client
            .get(format!("{doh}?{qs}"))
            .header("accept", "application/dns-json")
            .send()
            .await
        else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let body: Value =
            serde_json::from_slice(&resp.bytes().await.unwrap_or_default()).unwrap_or(Value::Null);
        let matched = body["Answer"]
            .as_array()
            .is_some_and(|ans| {
                ans.iter().any(|rec| {
                    rec["data"]
                        .as_str()
                        .is_some_and(|d| d.trim_matches('"').contains(expected_token))
                })
            });
        if matched {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn verify(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let existing = sqlx::query(
        "SELECT id, account_id, domain, status, verification_token,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at
         FROM pulsar_custom_domains WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let domain: String = existing.get("domain");
    let token: String = existing.get("verification_token");
    let ok = verify_dns_txt(&domain, &token).await.unwrap_or(false);
    let next_status = if ok { "verified" } else { "pending" };
    let updated = sqlx::query(
        "UPDATE pulsar_custom_domains SET status = $3, updated_at = now()
         WHERE account_id = $1 AND id = $2
         RETURNING id, account_id, domain, status, verification_token,
                   EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                   EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at",
    )
    .bind(account_id)
    .bind(id)
    .bind(next_status)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(json!({ "payload": row_json(&updated) })))
}

async fn destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query("DELETE FROM pulsar_custom_domains WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&st.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}
