//! Linear integration: OAuth auth/callback, GraphQL issues/teams lookup, issue linking & activity messages.
//! Mirrors chatwoot/app/controllers/api/v1/accounts/integrations/linear_controller.rb,
//! chatwoot/app/controllers/linear/callbacks_controller.rb,
//! and chatwoot/app/services/integrations/linear/processor_service.rb.

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
        ApiError, ApiResult, AppState, CurrentUser, require_account, sign_oauth_claims,
        verify_oauth_claims,
    },
    conversations::{insert_message, reload},
    integrations::{delete_hook, load_hook, post_form_json, upsert_hook},
};

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/integrations/linear", delete(destroy))
        .route("/integrations/linear/auth", post(auth))
        .route("/integrations/linear/teams", get(teams))
        .route("/integrations/linear/team_entities", get(team_entities))
        .route("/integrations/linear/create_issue", post(create_issue))
        .route("/integrations/linear/link_issue", post(link_issue))
        .route("/integrations/linear/unlink_issue", post(unlink_issue))
        .route("/integrations/linear/search_issue", get(search_issue))
        .route("/integrations/linear/linked_issues", get(linked_issues))
}

pub fn public_routes() -> Router<AppState> {
    Router::new().route("/linear/callback", get(callback))
}

fn graphql_url() -> String {
    std::env::var("LINEAR_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://api.linear.app/graphql".into())
}

fn token_url() -> String {
    std::env::var("LINEAR_OAUTH_TOKEN_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://api.linear.app/oauth/token".into())
}

async fn linear_graphql(token: &str, query: &str, variables: Value) -> Result<Value, ApiError> {
    let payload = serde_json::to_vec(&json!({ "query": query, "variables": variables }))
        .map_err(|e| ApiError::bad(e.to_string()))?;
    let resp = reqwest::Client::new()
        .post(graphql_url())
        .header(reqwest::header::AUTHORIZATION, token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload)
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("Linear request failed: {e}")))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad(format!("Linear response failed: {e}")))?;
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if !status.is_success() || v["errors"].as_array().is_some_and(|e| !e.is_empty()) {
        let msg = v["errors"][0]["message"]
            .as_str()
            .unwrap_or("Linear API request failed");
        return Err(ApiError::bad(msg));
    }
    Ok(v["data"].clone())
}

async fn require_linear_token(st: &AppState, account_id: i64) -> Result<String, ApiError> {
    let hook = load_hook(&st.db, account_id, "linear")
        .await?
        .ok_or_else(|| ApiError::not_found())?;
    hook.1
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Linear access token missing"))
}

async fn conversation_link(
    st: &AppState,
    account_id: i64,
    display_id: i64,
) -> Result<(crate::conversations::Conv, String), ApiError> {
    let conv_id: i64 = sqlx::query_scalar(
        "SELECT id::bigint FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id as i32)
    .bind(display_id as i32)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let conv = reload(&st.db, conv_id).await?;
    let link = format!(
        "{}/app/accounts/{}/conversations/{}",
        st.frontend_url.trim_end_matches('/'),
        account_id,
        display_id
    );
    Ok((conv, link))
}

async fn auth(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let client_id = std::env::var("LINEAR_CLIENT_ID").unwrap_or_default();
    let state = sign_oauth_claims(&json!({
        "account_id": account_id,
        "exp": chrono::Utc::now().timestamp() + 1800,
    }));
    let redirect_uri = format!("{}/linear/callback", st.frontend_url.trim_end_matches('/'));
    let qs = {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        ser.append_pair("client_id", &client_id);
        ser.append_pair("redirect_uri", &redirect_uri);
        ser.append_pair("response_type", "code");
        ser.append_pair("scope", "read,write,issues:create");
        ser.append_pair("state", &state);
        ser.finish()
    };
    Ok(Json(
        json!({ "url": format!("https://linear.app/oauth/authorize?{qs}") }),
    ))
}

async fn destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    delete_hook(&st.db, account_id, "linear").await?;
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
    let dest = format!("{base}/app/accounts/{account_id}/settings/integrations/linear");
    let Some(code) = q.get("code").filter(|s| !s.is_empty()) else {
        return Redirect::temporary(&dest).into_response();
    };
    let client_id = std::env::var("LINEAR_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("LINEAR_CLIENT_SECRET").unwrap_or_default();
    let redirect_uri = format!("{base}/linear/callback");
    let Ok(tok) = post_form_json(
        &token_url(),
        &[
            ("code", code),
            ("redirect_uri", &redirect_uri),
            ("client_id", &client_id),
            ("client_secret", &client_secret),
            ("grant_type", "authorization_code"),
        ],
        None,
    )
    .await
    else {
        return Redirect::temporary(&dest).into_response();
    };
    let Some(access_token) = tok["access_token"].as_str().filter(|s| !s.is_empty()) else {
        return Redirect::temporary(&dest).into_response();
    };
    let settings = json!({
        "token_type": tok["token_type"],
        "expires_in": tok["expires_in"],
        "scope": tok["scope"],
        "refresh_token": tok["refresh_token"],
    });
    let _ = upsert_hook(&st.db, account_id, "linear", access_token, None, settings).await;
    Redirect::temporary(&dest).into_response()
}

async fn teams(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let token = require_linear_token(&st, account_id).await?;
    let data = linear_graphql(&token, "{ teams { nodes { id name } } }", json!({})).await?;
    Ok(Json(data["teams"]["nodes"].clone()))
}

async fn team_entities(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let team_id = q
        .get("team_id")
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("team_id is required"))?;
    let token = require_linear_token(&st, account_id).await?;
    let query = "query($teamId: String!) { team(id: $teamId) { states { nodes { id name } } labels { nodes { id name } } members { nodes { id name } } projects { nodes { id name } } } }";
    let data = linear_graphql(&token, query, json!({ "teamId": team_id })).await?;
    let team = &data["team"];
    Ok(Json(json!({
        "states": team["states"]["nodes"],
        "labels": team["labels"]["nodes"],
        "users": team["members"]["nodes"],
        "projects": team["projects"]["nodes"],
    })))
}

async fn create_issue(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let conv_disp_id = body["conversation_id"]
        .as_i64()
        .or_else(|| {
            body["conversation_id"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .ok_or_else(|| ApiError::bad("conversation_id is required"))?;
    let team_id = body["team_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("team_id is required"))?;
    let title = body["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("title is required"))?;
    let (conv, link) = conversation_link(&st, account_id, conv_disp_id).await?;
    let token = require_linear_token(&st, account_id).await?;

    let mut input = json!({
        "teamId": team_id,
        "title": title,
    });
    if let Some(d) = body["description"].as_str().filter(|s| !s.is_empty()) {
        input["description"] = json!(d);
    }
    if let Some(p) = body["priority"].as_i64() {
        input["priority"] = json!(p);
    }
    if let Some(s) = body["state_id"].as_str().filter(|s| !s.is_empty()) {
        input["stateId"] = json!(s);
    }
    if let Some(a) = body["assignee_id"].as_str().filter(|s| !s.is_empty()) {
        input["assigneeId"] = json!(a);
    }
    if let Some(p) = body["project_id"].as_str().filter(|s| !s.is_empty()) {
        input["projectId"] = json!(p);
    }
    if let Some(l) = body["label_ids"].as_array() {
        input["labelIds"] = json!(l);
    }

    let query = "mutation($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { id identifier title url } } }";
    let data = linear_graphql(&token, query, json!({ "input": input })).await?;
    let issue = &data["issueCreate"]["issue"];
    let issue_id = issue["id"].as_str().unwrap_or("");
    let identifier = issue["identifier"].as_str().unwrap_or(issue_id);

    if !issue_id.is_empty() {
        let attach_q = "mutation($issueId: String!, $url: String!, $title: String!) { attachmentCreate(input: { issueId: $issueId, url: $url, title: $title }) { success attachment { id } } }";
        let _ = linear_graphql(
            &token,
            attach_q,
            json!({ "issueId": issue_id, "url": link, "title": format!("Conversation #{conv_disp_id}") }),
        )
        .await;
    }
    let _ = insert_message(
        &st,
        &conv,
        Some(("User", user.id)),
        &format!("Linear issue {identifier} created"),
        2,
        false,
        0,
        json!({}),
        None,
    )
    .await;

    Ok(Json(issue.clone()))
}

async fn link_issue(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let conv_disp_id = body["conversation_id"]
        .as_i64()
        .or_else(|| {
            body["conversation_id"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .ok_or_else(|| ApiError::bad("conversation_id is required"))?;
    let issue_id = body["issue_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("issue_id is required"))?;
    let title = body["title"].as_str().unwrap_or("Chatwoot Conversation");
    let (conv, link) = conversation_link(&st, account_id, conv_disp_id).await?;
    let token = require_linear_token(&st, account_id).await?;

    let q = "mutation($issueId: String!, $url: String!, $title: String!) { attachmentCreate(input: { issueId: $issueId, url: $url, title: $title }) { success attachment { id url title issue { id identifier title } } } }";
    let data = linear_graphql(
        &token,
        q,
        json!({ "issueId": issue_id, "url": link, "title": title }),
    )
    .await?;
    let _ = insert_message(
        &st,
        &conv,
        Some(("User", user.id)),
        &format!("Linked Linear issue {issue_id}"),
        2,
        false,
        0,
        json!({}),
        None,
    )
    .await;
    Ok(Json(data["attachmentCreate"]["attachment"].clone()))
}

async fn unlink_issue(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let conv_disp_id = body["conversation_id"]
        .as_i64()
        .or_else(|| {
            body["conversation_id"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .ok_or_else(|| ApiError::bad("conversation_id is required"))?;
    let link_id = body["link_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("link_id is required"))?;
    let (conv, _) = conversation_link(&st, account_id, conv_disp_id).await?;
    let token = require_linear_token(&st, account_id).await?;

    let q = "mutation($id: String!) { attachmentDelete(id: $id) { success } }";
    let data = linear_graphql(&token, q, json!({ "id": link_id })).await?;
    let _ = insert_message(
        &st,
        &conv,
        Some(("User", user.id)),
        &format!("Unlinked Linear issue ({link_id})"),
        2,
        false,
        0,
        json!({}),
        None,
    )
    .await;
    Ok(Json(data["attachmentDelete"].clone()))
}

async fn search_issue(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let term = q
        .get("q")
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::bad("Specify search string with parameter q"))?;
    let token = require_linear_token(&st, account_id).await?;
    let query = "query($q: String!) { searchIssues(term: $q) { nodes { id identifier title url state { name } } } }";
    let data = linear_graphql(&token, query, json!({ "q": term })).await?;
    Ok(Json(data["searchIssues"]["nodes"].clone()))
}

async fn linked_issues(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let conv_disp_id = q
        .get("conversation_id")
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| ApiError::bad("conversation_id is required"))?;
    let (_, link) = conversation_link(&st, account_id, conv_disp_id).await?;
    let token = require_linear_token(&st, account_id).await?;
    let query = "query($url: String!) { attachmentsForURL(url: $url) { nodes { id url title issue { id identifier title url state { name } assignee { name } } } } }";
    let data = linear_graphql(&token, query, json!({ "url": link })).await?;
    Ok(Json(data["attachmentsForURL"]["nodes"].clone()))
}
