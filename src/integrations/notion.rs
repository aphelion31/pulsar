//! Notion integration: OAuth authorization/callback & disconnect.
//! Mirrors chatwoot/app/controllers/api/v1/accounts/notion/authorizations_controller.rb,
//! chatwoot/app/controllers/api/v1/accounts/integrations/notion_controller.rb,
//! and chatwoot/app/controllers/notion/callbacks_controller.rb.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
};
use serde_json::{Value, json};
use std::collections::HashMap;

use crate::{
    common::{
        ApiResult, AppState, CurrentUser, require_account, sign_oauth_claims, verify_oauth_claims,
    },
    integrations::{delete_hook, post_form_json, upsert_hook},
};

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/notion/authorization", post(authorization))
        .route("/integrations/notion", delete(destroy))
}

pub fn public_routes() -> Router<AppState> {
    Router::new().route("/notion/callback", get(callback))
}

fn token_url() -> String {
    std::env::var("NOTION_OAUTH_TOKEN_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://api.notion.com/v1/oauth/token".into())
}

async fn authorization(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let client_id = std::env::var("NOTION_CLIENT_ID").unwrap_or_default();
    let base = st.frontend_url.trim_end_matches('/');
    let state = sign_oauth_claims(&json!({
        "account_id": account_id,
        "exp": chrono::Utc::now().timestamp() + 1800,
    }));
    let qs = {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        ser.append_pair("client_id", &client_id);
        ser.append_pair("response_type", "code");
        ser.append_pair("owner", "user");
        ser.append_pair("redirect_uri", &format!("{base}/notion/callback"));
        ser.append_pair("state", &state);
        ser.finish()
    };
    Ok(Json(json!({
        "success": true,
        "url": format!("https://api.notion.com/v1/oauth/authorize?{qs}"),
    })))
}

async fn destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> Result<StatusCode, crate::common::ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    delete_hook(&st.db, account_id, "notion").await?;
    Ok(StatusCode::OK)
}

async fn callback(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let base = st.frontend_url.trim_end_matches('/');
    let Some(account_id) = q
        .get("state")
        .and_then(|s| verify_oauth_claims(s))
        .and_then(|c| c["account_id"].as_i64())
    else {
        return Redirect::temporary(base).into_response();
    };
    let dest = format!("{base}/app/accounts/{account_id}/settings/integrations/notion");
    let Some(code) = q.get("code").filter(|s| !s.is_empty()) else {
        return Redirect::temporary(&dest).into_response();
    };
    let client_id = std::env::var("NOTION_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("NOTION_CLIENT_SECRET").unwrap_or_default();
    let redirect_uri = format!("{base}/notion/callback");
    let Ok(tok) = post_form_json(
        &token_url(),
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &redirect_uri),
        ],
        Some((&client_id, &client_secret)),
    )
    .await
    else {
        return Redirect::temporary(&dest).into_response();
    };
    let Some(access_token) = tok["access_token"].as_str().filter(|s| !s.is_empty()) else {
        return Redirect::temporary(&dest).into_response();
    };
    let settings: Value = json!({
        "token_type": tok["token_type"],
        "workspace_name": tok["workspace_name"],
        "workspace_id": tok["workspace_id"],
        "workspace_icon": tok["workspace_icon"],
        "bot_id": tok["bot_id"],
        "owner": tok["owner"],
    });
    let _ = upsert_hook(
        &st.db,
        account_id,
        "notion",
        access_token,
        tok["workspace_id"].as_str(),
        settings,
    )
    .await;
    Redirect::temporary(&dest).into_response()
}
