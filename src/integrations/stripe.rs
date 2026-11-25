//! Stripe integration: OAuth auth/callback, connection show/delete, customer billing summary & deauth webhook.
//! Mirrors chatwoot/app/controllers/api/v1/accounts/integrations/stripe_controller.rb,
//! stripe/callbacks_controller.rb, webhooks/stripe_app_controller.rb,
//! and chatwoot/app/services/integrations/stripe/*.rb.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2_compat::Sha256;
use std::collections::HashMap;

use crate::{
    common::{
        ApiError, ApiResult, AppState, CurrentUser, require_account, sign_oauth_claims,
        verify_oauth_claims,
    },
    integrations::{delete_hook, load_hook, post_form_json, upsert_hook},
};

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/integrations/stripe", get(show).delete(destroy))
        .route("/integrations/stripe/auth", post(auth))
        .route("/integrations/stripe/customer", get(customer))
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/stripe/callback", get(callback))
        .route("/webhooks/stripe_app", post(stripe_app_webhook))
        .route(
            "/enterprise/webhooks/stripe",
            post(enterprise_stripe_webhook),
        )
}

fn api_base() -> String {
    std::env::var("STRIPE_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://api.stripe.com".into())
        .trim_end_matches('/')
        .to_string()
}

fn oauth_token_url() -> String {
    std::env::var("STRIPE_OAUTH_TOKEN_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://connect.stripe.com/oauth/token".into())
}

async fn show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let hook = load_hook(&st.db, account_id, "stripe")
        .await?
        .ok_or_else(ApiError::not_found)?;
    let livemode = hook.3["livemode"].as_bool().unwrap_or(false);
    Ok(Json(json!({
        "account_id": hook.2,
        "connected_at": hook.3["connected_at"],
        "mode": if livemode { "live" } else { "sandbox" },
        "reauthorization_required": hook.3["reauthorization_required"].as_bool().unwrap_or(false),
    })))
}

async fn auth(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let client_id = std::env::var("STRIPE_APP_CLIENT_ID")
        .or_else(|_| std::env::var("STRIPE_CLIENT_ID"))
        .unwrap_or_default();
    let livemode = std::env::var("STRIPE_LIVEMODE").is_ok_and(|v| v == "true" || v == "1");
    let state = sign_oauth_claims(&json!({
        "account_id": account_id,
        "user_id": user.id,
        "livemode": livemode,
        "exp": chrono::Utc::now().timestamp() + 1800,
    }));
    let base = st.frontend_url.trim_end_matches('/');
    let qs = {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        ser.append_pair("response_type", "code");
        ser.append_pair("client_id", &client_id);
        ser.append_pair("scope", "read_only");
        ser.append_pair("redirect_uri", &format!("{base}/stripe/callback"));
        ser.append_pair("state", &state);
        ser.finish()
    };
    Ok(Json(json!({
        "url": format!("https://connect.stripe.com/oauth/authorize?{qs}")
    })))
}

async fn destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    delete_hook(&st.db, account_id, "stripe").await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn callback(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let base = st.frontend_url.trim_end_matches('/');
    let Some(claims) = q.get("state").and_then(|s| verify_oauth_claims(s)) else {
        return Redirect::temporary(base).into_response();
    };
    let Some(account_id) = claims["account_id"].as_i64() else {
        return Redirect::temporary(base).into_response();
    };
    let dest = format!("{base}/app/accounts/{account_id}/settings/integrations/stripe");
    if q.get("error").is_some_and(|e| !e.is_empty()) {
        return Redirect::temporary(&format!("{dest}?error=authorization_failed")).into_response();
    }
    let Some(code) = q.get("code").filter(|s| !s.is_empty()) else {
        return Redirect::temporary(&format!("{dest}?error=authorization_failed")).into_response();
    };
    let secret = std::env::var("STRIPE_APP_SECRET_KEY")
        .or_else(|_| std::env::var("STRIPE_SECRET_KEY"))
        .unwrap_or_default();
    let redirect_uri = format!("{base}/stripe/callback");
    let Ok(tok) = post_form_json(
        &oauth_token_url(),
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_secret", &secret),
            ("redirect_uri", &redirect_uri),
        ],
        None,
    )
    .await
    else {
        return Redirect::temporary(&format!("{dest}?error=authorization_failed")).into_response();
    };
    let Some(access_token) = tok["access_token"].as_str().filter(|s| !s.is_empty()) else {
        return Redirect::temporary(&format!("{dest}?error=authorization_failed")).into_response();
    };
    let settings = json!({
        "livemode": tok["livemode"].as_bool().unwrap_or(false),
        "connected_at": chrono::Utc::now().to_rfc3339(),
        "refresh_token": tok["refresh_token"],
        "reauthorization_required": false,
    });
    let _ = upsert_hook(
        &st.db,
        account_id,
        "stripe",
        access_token,
        tok["stripe_user_id"].as_str(),
        settings,
    )
    .await;
    Redirect::temporary(&dest).into_response()
}

async fn stripe_get(token: &str, path: &str, query: &[(&str, &str)]) -> Result<Value, ApiError> {
    let resp = reqwest::Client::new()
        .get(format!("{}{path}", api_base()))
        .bearer_auth(token)
        .query(query)
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("Stripe API failed: {e}")))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad(format!("Stripe response failed: {e}")))?;
    let val: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if status == StatusCode::UNAUTHORIZED {
        return Err(ApiError::bad("stripe_reauthorization_required"));
    }
    if !status.is_success() {
        return Err(ApiError::bad("stripe_unavailable"));
    }
    Ok(val)
}

async fn customer(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let conv_disp_id = q
        .get("conversation_id")
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|&id| id > 0)
        .ok_or_else(|| ApiError::bad("conversation_id must be a positive integer"))?;
    let hook = load_hook(&st.db, account_id, "stripe")
        .await?
        .filter(|h| h.4 == 1)
        .ok_or_else(ApiError::not_found)?;
    let token = hook
        .1
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("stripe_reauthorization_required"))?;

    let email: Option<String> = sqlx::query_scalar(
        "SELECT ct.email
         FROM conversations c
         JOIN contacts ct ON ct.id = c.contact_id
         WHERE c.account_id = $1 AND c.display_id = $2",
    )
    .bind(account_id as i32)
    .bind(conv_disp_id as i32)
    .fetch_optional(&st.db)
    .await?
    .flatten();
    let Some(email) = email.filter(|s| !s.trim().is_empty()) else {
        return Ok(Json(json!({ "customers": [], "missing_email": true })));
    };

    let escaped = email.replace('\\', "\\\\").replace('"', "\\\"");
    let query_str = format!("email:\"{escaped}\"");
    let search_res = stripe_get(
        &token,
        "/v1/customers/search",
        &[("query", &query_str), ("limit", "100")],
    )
    .await?;
    let matches: Vec<Value> = search_res["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c["email"]
                        .as_str()
                        .is_some_and(|e| e.eq_ignore_ascii_case(&email))
                })
                .map(|c| {
                    json!({
                        "id": c["id"],
                        "name": c["name"],
                        "email": c["email"],
                        "phone": c["phone"],
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let requested_id = q.get("customer_id").map(String::as_str);
    let selected = if let Some(cid) = requested_id {
        matches
            .iter()
            .find(|c| c["id"].as_str() == Some(cid))
            .cloned()
            .ok_or_else(ApiError::not_found)?
    } else if matches.len() == 1 {
        matches[0].clone()
    } else {
        return Ok(Json(json!({
            "customers": matches,
            "has_more_customers": search_res["has_more"].as_bool().unwrap_or(false),
        })));
    };

    let cid = selected["id"].as_str().unwrap_or("");
    let subs = stripe_get(
        &token,
        "/v1/subscriptions",
        &[("customer", cid), ("status", "all"), ("limit", "5")],
    )
    .await?;
    let invs = stripe_get(&token, "/v1/invoices", &[("customer", cid), ("limit", "5")]).await?;

    let invoices: Vec<Value> = invs["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|inv| {
                    json!({
                        "id": inv["id"],
                        "number": inv["number"],
                        "status": inv["status"],
                        "total": inv["total"],
                        "currency": inv["currency"],
                        "created": inv["created"],
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(json!({
        "customers": matches,
        "has_more_customers": search_res["has_more"].as_bool().unwrap_or(false),
        "customer": selected,
        "subscriptions": subs["data"].as_array().cloned().unwrap_or_default(),
        "invoices": invoices,
    })))
}

fn verify_stripe_sig(secret: &str, hdr: &str, body: &[u8]) -> bool {
    let mut ts = "";
    let mut v1 = "";
    for part in hdr.split(',') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("t=") {
            ts = v;
        } else if let Some(v) = part.strip_prefix("v1=") {
            v1 = v;
        }
    }
    let Ok(sig_bytes) = hex::decode(v1) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(ts.as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&sig_bytes).is_ok()
}

async fn stripe_app_webhook(
    State(st): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let secret = std::env::var("STRIPE_APP_WEBHOOK_SECRET").unwrap_or_default();
    if secret.is_empty() {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !verify_stripe_sig(&secret, sig, &body) {
        return StatusCode::BAD_REQUEST;
    }
    let Ok(ev) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST;
    };
    if ev["type"].as_str() == Some("account.application.deauthorized") {
        let Some(acct) = ev["account"].as_str().filter(|s| s.starts_with("acct_")) else {
            return StatusCode::BAD_REQUEST;
        };
        let _ = sqlx::query(
            "DELETE FROM integrations_hooks WHERE app_id = 'stripe' AND reference_id = $1",
        )
        .bind(acct)
        .execute(&st.db)
        .await;
    }
    StatusCode::OK
}

async fn enterprise_stripe_webhook(
    State(st): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let secret = std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default();
    if !secret.is_empty() {
        let sig = headers
            .get("stripe-signature")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !verify_stripe_sig(&secret, sig, &body) {
            return StatusCode::BAD_REQUEST;
        }
    }
    let Ok(ev) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST;
    };
    if let Some(acct_id) = ev["data"]["object"]["metadata"]["account_id"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| ev["data"]["object"]["metadata"]["account_id"].as_i64())
    {
        let ev_type = ev["type"].as_str().unwrap_or("");
        let _ = sqlx::query(
            "UPDATE accounts SET custom_attributes = COALESCE(custom_attributes, '{}'::jsonb) || $1, updated_at = now()
             WHERE id = $2::bigint",
        )
        .bind(json!({ "stripe_last_event": ev_type }))
        .bind(acct_id)
        .execute(&st.db)
        .await;
    }
    StatusCode::OK
}
