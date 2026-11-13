//! Shopify integration: OAuth auth/callback, orders lookup, disconnect, and webhooks.
//! Mirrors chatwoot/app/controllers/api/v1/accounts/integrations/shopify_controller.rb,
//! chatwoot/app/controllers/shopify/callbacks_controller.rb,
//! chatwoot/app/controllers/webhooks/shopify_controller.rb,
//! chatwoot/app/helpers/shopify/integration_helper.rb, and
//! chatwoot/app/services/shopify/{callback_service,shop_domain,uninstallation_service}.rb.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Json, Router,
};
use base64::{
    engine::general_purpose::{STANDARD as B64_STD, URL_SAFE_NO_PAD as B64_URL},
    Engine,
};
use chrono::SecondsFormat;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2_compat::Sha256;
use sqlx::PgPool;

use crate::{
    ar_encryption,
    common::{require_account, ApiError, AppState, CurrentUser},
};

const REQUIRED_SCOPES: &str = "read_customers,read_orders,read_fulfillments";
const SHOPIFY_API_VERSION: &str = "2025-01";

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/integrations/shopify/auth", post(auth))
        .route("/integrations/shopify/orders", get(orders))
        .route("/integrations/shopify", delete(destroy))
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/shopify/callback", get(callback))
        .route("/webhooks/shopify", post(webhook_events))
}

// ---------- Config & ShopDomain helpers ----------

fn parse_config_scalar(v: &Value) -> Option<String> {
    if let Some(o) = v.as_object() {
        return o.get("value").and_then(|x| {
            x.as_str()
                .map(str::to_string)
                .or_else(|| x.as_bool().map(|b| b.to_string()))
        });
    }
    let s = v.as_str()?;
    let line = s.lines().find_map(|l| l.strip_prefix("value: "))?;
    Some(line.trim().trim_matches('\'').trim_matches('"').to_string())
}

async fn config_value(db: &PgPool, key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let row: Option<Value> = sqlx::query_scalar(
        "SELECT serialized_value FROM installation_configs WHERE name = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(key)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    row.and_then(|v| parse_config_scalar(&v)).filter(|s| !s.is_empty())
}

async fn shopify_enabled(db: &PgPool) -> bool {
    match config_value(db, "ENABLE_SHOPIFY_INTEGRATION").await.as_deref() {
        Some("false") | Some("0") | Some("no") => false,
        _ => true,
    }
}

fn normalize_shop_domain(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn valid_shop_domain(value: &str) -> bool {
    let norm = normalize_shop_domain(value);
    let sub = norm
        .strip_suffix(".myshopify.com")
        .or_else(|| norm.strip_suffix(".myshopify.io"));
    let Some(sub) = sub else { return false };
    if sub.is_empty() {
        return false;
    }
    let bytes = sub.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

fn shopify_api_base(shop_domain: &str) -> String {
    std::env::var("SHOPIFY_API_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("https://{shop_domain}"))
        .trim_end_matches('/')
        .to_string()
}

// ---------- Signed state JWT (mirrors Shopify::IntegrationHelper) ----------

#[derive(Serialize, Deserialize)]
struct StateClaims {
    sub: i64,
    iat: i64,
    exp: i64,
}

fn sign_state_token(account_id: i64, secret: &str) -> Option<String> {
    if secret.is_empty() {
        return None;
    }
    let now = chrono::Utc::now().timestamp();
    let claims = StateClaims {
        sub: account_id,
        iat: now,
        exp: now + 600,
    };
    let head = B64_URL.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let body = B64_URL.encode(serde_json::to_vec(&claims).ok()?);
    let input = format!("{head}.{body}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(input.as_bytes());
    let sig = B64_URL.encode(mac.finalize().into_bytes());
    Some(format!("{input}.{sig}"))
}

fn verify_state_token(token: &str, secret: &str) -> Option<i64> {
    if token.is_empty() || secret.is_empty() {
        return None;
    }
    let mut it = token.split('.');
    let (h, p, s) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let input = &token[..h.len() + 1 + p.len()];
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(input.as_bytes());
    let got = B64_URL.decode(s).ok()?;
    mac.verify_slice(&got).ok()?;
    let claims: StateClaims = serde_json::from_slice(&B64_URL.decode(p).ok()?).ok()?;
    if claims.exp < chrono::Utc::now().timestamp() {
        return None;
    }
    Some(claims.sub)
}

fn verify_callback_hmac(params: &[(String, String)], secret: &str) -> bool {
    let Some((_, hmac_val)) = params.iter().find(|(k, _)| k == "hmac") else {
        return false;
    };
    if hmac_val.len() != 64 || !hmac_val.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return false;
    }
    let Ok(expected_bytes) = hex::decode(hmac_val) else {
        return false;
    };
    let mut filtered: Vec<(&str, &str)> = params
        .iter()
        .filter(|(k, _)| k != "hmac")
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    filtered.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(b.1)));
    let query_string = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(filtered)
        .finish();
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(query_string.as_bytes());
    mac.verify_slice(&expected_bytes).is_ok()
}

fn verify_webhook_hmac(body: &[u8], hmac_header: &str, secret: &str) -> bool {
    if secret.is_empty() || hmac_header.is_empty() {
        return false;
    }
    let Ok(got) = B64_STD.decode(hmac_header.trim()) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&got).is_ok()
}

// ---------- Account handlers ----------

async fn auth(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    if !shopify_enabled(&st.db).await {
        return Err(ApiError::not_found());
    }
    require_account(&st.db, &user, account_id).await?.require_admin()?;

    let Some(shop_domain) = body.get("shop_domain").and_then(|v| v.as_str()) else {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "Invalid Shopify shop domain" }))).into_response());
    };
    if !valid_shop_domain(shop_domain) {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "Invalid Shopify shop domain" }))).into_response());
    }

    let client_id = config_value(&st.db, "SHOPIFY_CLIENT_ID").await.unwrap_or_default();
    let client_secret = config_value(&st.db, "SHOPIFY_CLIENT_SECRET").await.unwrap_or_default();
    let state = sign_state_token(account_id, &client_secret);
    if client_id.is_empty() || state.is_none() {
        return Err(ApiError(StatusCode::INTERNAL_SERVER_ERROR, "Shopify OAuth is not configured".into()));
    }

    let norm_shop = normalize_shop_domain(shop_domain);
    let redirect_uri = format!("{}/shopify/callback", st.frontend_url.trim_end_matches('/'));
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", REQUIRED_SCOPES)
        .append_pair("state", state.as_deref().unwrap_or(""))
        .finish();
    let redirect_url = format!("https://{norm_shop}/admin/oauth/authorize?{query}");
    Ok(Json(json!({ "redirect_url": redirect_url })).into_response())
}

#[derive(Deserialize, Default)]
struct OrdersQuery {
    contact_id: Option<i64>,
}

async fn orders(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<OrdersQuery>,
) -> Result<Response, ApiError> {
    if !shopify_enabled(&st.db).await {
        return Err(ApiError::not_found());
    }
    require_account(&st.db, &user, account_id).await?;

    let (reference_id, stored_token): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT reference_id, access_token FROM integrations_hooks
         WHERE account_id = $1::bigint AND app_id = 'shopify' AND status = 1 ORDER BY id LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let shop = reference_id.filter(|s| !s.is_empty()).ok_or_else(ApiError::not_found)?;
    let access_token = match stored_token {
        Some(t) => ar_encryption::decrypt_deterministic(&t)?,
        None => String::new(),
    };
    let access_token = Some(access_token)
        .filter(|t| !t.is_empty())
        .ok_or_else(ApiError::not_found)?;

    let contact = match q.contact_id {
        Some(cid) => {
            sqlx::query_as::<_, (Option<String>, Option<String>)>(
                "SELECT email, phone_number FROM contacts WHERE id = $1::bigint AND account_id = $2::bigint",
            )
            .bind(cid)
            .bind(account_id)
            .fetch_optional(&st.db)
            .await?
        }
        None => None,
    };
    let Some((email, phone)) = contact else {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "Contact information missing" }))).into_response());
    };
    let email = email.filter(|s| !s.trim().is_empty());
    let phone = phone.filter(|s| !s.trim().is_empty());
    if email.is_none() && phone.is_none() {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "Contact information missing" }))).into_response());
    }

    let mut query_parts = Vec::new();
    if let Some(e) = &email {
        query_parts.push(format!("email:{e}"));
    }
    if let Some(p) = &phone {
        query_parts.push(format!("phone:{p}"));
    }
    let search_query = query_parts.join(" OR ");

    let base = shopify_api_base(&shop);
    let client = reqwest::Client::new();
    let customers_url = format!("{base}/admin/api/{SHOPIFY_API_VERSION}/customers/search.json");
    let cust_resp = client
        .get(&customers_url)
        .header("X-Shopify-Access-Token", &access_token)
        .query(&[("query", search_query.as_str()), ("fields", "id,email,phone")])
        .send()
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;

    if !cust_resp.status().is_success() {
        let msg = cust_resp.text().await.unwrap_or_else(|_| "Shopify API error".into());
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": msg }))).into_response());
    }
    let cust_body: Value = cust_resp
        .text()
        .await
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or(Value::Null);
    let customers = cust_body
        .get("customers")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    if customers.is_empty() {
        return Ok(Json(json!({ "orders": [] })).into_response());
    }

    let customer_id = match &customers[0]["id"] {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => return Ok(Json(json!({ "orders": [] })).into_response()),
    };

    let orders_url = format!("{base}/admin/api/{SHOPIFY_API_VERSION}/orders.json");
    let ord_resp = client
        .get(&orders_url)
        .header("X-Shopify-Access-Token", &access_token)
        .query(&[
            ("customer_id", customer_id.as_str()),
            ("status", "any"),
            ("fields", "id,email,created_at,total_price,currency,fulfillment_status,financial_status"),
        ])
        .send()
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;

    if !ord_resp.status().is_success() {
        let msg = ord_resp.text().await.unwrap_or_else(|_| "Shopify API error".into());
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": msg }))).into_response());
    }
    let ord_body: Value = ord_resp
        .text()
        .await
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or(Value::Null);
    let raw_orders = ord_body
        .get("orders")
        .and_then(|o| o.as_array())
        .cloned()
        .unwrap_or_default();

    let orders: Vec<Value> = raw_orders
        .into_iter()
        .map(|mut o| {
            if let Some(obj) = o.as_object_mut() {
                let oid = match obj.get("id") {
                    Some(Value::Number(n)) => n.to_string(),
                    Some(Value::String(s)) => s.clone(),
                    _ => String::new(),
                };
                obj.insert("admin_url".into(), json!(format!("https://{shop}/admin/orders/{oid}")));
            }
            o
        })
        .collect();

    Ok(Json(json!({ "orders": orders })).into_response())
}

async fn destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if !shopify_enabled(&st.db).await {
        return Err(ApiError::not_found());
    }
    require_account(&st.db, &user, account_id).await?.require_admin()?;

    let hook_id: Option<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM integrations_hooks WHERE account_id = $1::bigint AND app_id = 'shopify' ORDER BY id LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?;
    let hook_id = hook_id.ok_or_else(ApiError::not_found)?;

    sqlx::query("DELETE FROM integrations_hooks WHERE id = $1::bigint AND account_id = $2::bigint")
        .bind(hook_id)
        .bind(account_id)
        .execute(&st.db)
        .await?;
    Ok(StatusCode::OK)
}

// ---------- Public OAuth callback & webhook ----------

async fn callback(
    State(st): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> Redirect {
    let frontend = st.frontend_url.trim_end_matches('/').to_string();
    let get = |k: &str| params.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());

    let client_secret = config_value(&st.db, "SHOPIFY_CLIENT_SECRET").await.unwrap_or_default();
    let account_id = get("state").and_then(|s| verify_state_token(s, &client_secret));
    let error_redirect = || match account_id {
        Some(aid) => format!("{frontend}/app/accounts/{aid}/settings/integrations/shopify?error=true"),
        None => format!("{frontend}?error=true"),
    };

    if get("error").is_some_and(|e| !e.is_empty()) {
        return Redirect::to(&error_redirect());
    }
    let Some(aid) = account_id else {
        tracing::error!("Shopify callback error: Invalid state parameter");
        return Redirect::to(&error_redirect());
    };

    let acct_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE id = $1::bigint)")
        .bind(aid)
        .fetch_one(&st.db)
        .await
        .unwrap_or(false);
    if !acct_exists || !shopify_enabled(&st.db).await {
        return Redirect::to(&error_redirect());
    }

    if !verify_callback_hmac(&params, &client_secret) {
        tracing::error!("Shopify callback error: Invalid HMAC signature");
        return Redirect::to(&error_redirect());
    }

    let Some(shop) = get("shop").filter(|s| valid_shop_domain(s)) else {
        tracing::error!("Shopify callback error: Invalid shop domain");
        return Redirect::to(&error_redirect());
    };
    let norm_shop = normalize_shop_domain(shop);

    let Some(code) = get("code").filter(|c| !c.is_empty()) else {
        tracing::error!("Shopify callback error: Missing authorization code");
        return Redirect::to(&error_redirect());
    };
    let client_id = config_value(&st.db, "SHOPIFY_CLIENT_ID").await.unwrap_or_default();
    if client_id.is_empty() {
        tracing::error!("Shopify callback error: Missing SHOPIFY_CLIENT_ID");
        return Redirect::to(&error_redirect());
    }

    let token_url = format!("{}/admin/oauth/access_token", shopify_api_base(&norm_shop));
    let client = reqwest::Client::new();
    let resp = match client
        .post(&token_url)
        .header("Content-Type", "application/json")
        .body(
            json!({
                "client_id": client_id,
                "client_secret": client_secret,
                "code": code,
            })
            .to_string(),
        )
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            tracing::error!("Shopify callback token exchange failed: HTTP {}", r.status());
            return Redirect::to(&error_redirect());
        }
        Err(e) => {
            tracing::error!("Shopify callback token exchange error: {e}");
            return Redirect::to(&error_redirect());
        }
    };

    let token_json: Value = resp
        .text()
        .await
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or(Value::Null);
    let Some(access_token) = token_json.get("access_token").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
        tracing::error!("Shopify callback error: access_token missing from response");
        return Redirect::to(&error_redirect());
    };
    let scope = token_json.get("scope").and_then(|v| v.as_str()).unwrap_or("");
    // Integrations::Hook encrypts access_token deterministically (only when all 3 keys are set).
    let Ok(encrypted_token) = ar_encryption::encrypt_deterministic(access_token) else {
        tracing::error!("Shopify callback error: token encryption failed");
        return Redirect::to(&error_redirect());
    };
    let settings = json!({
        "scope": scope,
        "connected_at": chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true),
        "installation_id": uuid::Uuid::new_v4().to_string(),
    });

    let existing_id: Option<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM integrations_hooks WHERE account_id = $1::bigint AND app_id = 'shopify' ORDER BY id LIMIT 1",
    )
    .bind(aid)
    .fetch_optional(&st.db)
    .await
    .ok()
    .flatten();

    let db_res = match existing_id {
        Some(id) => {
            sqlx::query(
                "UPDATE integrations_hooks
                 SET access_token = $2, status = 1, reference_id = $3, settings = $4, updated_at = now()
                 WHERE id = $1::bigint",
            )
            .bind(id)
            .bind(&encrypted_token)
            .bind(&norm_shop)
            .bind(&settings)
            .execute(&st.db)
            .await
        }
        None => {
            sqlx::query(
                "INSERT INTO integrations_hooks (account_id, app_id, hook_type, status, reference_id, access_token, settings, created_at, updated_at)
                 VALUES ($1::bigint, 'shopify', 0, 1, $2, $3, $4, now(), now())",
            )
            .bind(aid)
            .bind(&norm_shop)
            .bind(&encrypted_token)
            .bind(&settings)
            .execute(&st.db)
            .await
        }
    };
    if let Err(e) = db_res {
        tracing::error!("Shopify callback DB error: {e}");
        return Redirect::to(&error_redirect());
    }

    Redirect::to(&format!("{frontend}/app/accounts/{aid}/settings/integrations/shopify"))
}

async fn webhook_events(
    State(st): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let topic = headers
        .get("x-shopify-topic")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let mandatory_cleanup = matches!(
        topic,
        "customers/data_request" | "customers/redact" | "shop/redact" | "app/uninstalled"
    );
    if !mandatory_cleanup && !shopify_enabled(&st.db).await {
        return Err(ApiError::not_found());
    }

    let secret = config_value(&st.db, "SHOPIFY_CLIENT_SECRET").await.unwrap_or_default();
    let hmac_hdr = headers
        .get("x-shopify-hmac-sha256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !verify_webhook_hmac(&body, hmac_hdr, &secret) {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "Invalid HMAC".into()));
    }

    let payload: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let triggered_at = headers
        .get("x-shopify-triggered-at")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let shop_key = match topic {
        "app/uninstalled" => payload.get("myshopify_domain").and_then(|v| v.as_str()),
        "shop/redact" => payload.get("shop_domain").and_then(|v| v.as_str()),
        _ => None,
    };

    if let Some(shop) = shop_key.filter(|s| valid_shop_domain(s)) {
        let norm = normalize_shop_domain(shop);
        let rows: Vec<(i64, Value)> = sqlx::query_as(
            "SELECT id::bigint, COALESCE(settings, '{}'::jsonb) FROM integrations_hooks
             WHERE app_id = 'shopify' AND LOWER(reference_id) = $1",
        )
        .bind(&norm)
        .fetch_all(&st.db)
        .await?;

        for (id, settings) in rows {
            let stale = match (
                triggered_at,
                settings
                    .get("connected_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()),
            ) {
                (Some(trig), Some(conn)) => trig < conn.with_timezone(&chrono::Utc),
                _ => false,
            };
            if !stale {
                sqlx::query("DELETE FROM integrations_hooks WHERE id = $1::bigint")
                    .bind(id)
                    .execute(&st.db)
                    .await?;
            }
        }
    }

    Ok(StatusCode::OK)
}
