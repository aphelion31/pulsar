//! CE Slack integration account routes.
use axum::{extract::{Path, State}, routing::{get, post}, Json, Router};
use serde_json::{json, Value};
use crate::common::*;
use axum::http::StatusCode;
use sqlx::PgPool;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/integrations/slack", post(create).patch(update).put(update).delete(destroy))
        .route("/integrations/slack/list_all_channels", get(channels))
        .route("/integrations/apps/{id}", get(app_show))
        .route("/integrations/hooks/{id}/process_event", post(process_event))
}

async fn config(db: &PgPool, name: &str, default: &str) -> Result<String, ApiError> {
    let row: Option<(Value,)> = sqlx::query_as("SELECT serialized_value FROM installation_configs WHERE name=$1")
        .bind(name).fetch_optional(db).await?;
    let stored = row.and_then(|(v,)| {
        v.get("value").and_then(Value::as_str).map(str::to_owned).or_else(|| {
            v.as_str().and_then(|s| s.lines().find_map(|l| l.strip_prefix("value: ")))
                .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_owned())
        })
    }).filter(|s| !s.is_empty() && s != "null" && s != "~");
    Ok(stored.or_else(|| std::env::var(name).ok().filter(|s| !s.is_empty())).unwrap_or_else(|| default.to_owned()))
}

async fn slack(method: &str, params: &[(&str, &str)], token: Option<&str>) -> Result<Value, ApiError> {
    let base = std::env::var("SLACK_API_URL").unwrap_or_else(|_| "https://slack.com/api".into());
    let body = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(params.iter().copied()).finish();
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|_| ApiError::bad("Slack request failed"))?;
    let mut req = client.post(format!("{}/{}", base.trim_end_matches('/'), method)).header("content-type", "application/x-www-form-urlencoded").body(body);
    if let Some(token) = token { req = req.bearer_auth(token); }
    let response = req.send().await.map_err(|_| ApiError::bad("Slack request failed"))?;
    if !response.status().is_success() { return Err(ApiError::bad("Slack request failed")); }
    let text = response.text().await.map_err(|_| ApiError::bad("Invalid Slack response"))?;
    let value: Value = serde_json::from_str(&text).map_err(|_| ApiError::bad("Invalid Slack response"))?;
    if value["ok"] != true { return Err(ApiError::bad("Slack request failed")); }
    Ok(value)
}

#[derive(sqlx::FromRow)]
struct Hook { id: i64, inbox_id: Option<i32>, status: Option<i32>, reference_id: Option<String>, settings: Option<Value>, access_token: Option<String> }
async fn hook(db: &PgPool, account: i64) -> Result<Hook, ApiError> {
    Ok(sqlx::query_as("SELECT id,inbox_id,status,reference_id,settings,access_token FROM integrations_hooks WHERE account_id=$1 AND app_id='slack' ORDER BY id LIMIT 1").bind(account).fetch_one(db).await?)
}
fn hook_json(h: &Hook, account: i64, admin: bool) -> Value {
    let mut value = json!({"id":h.id,"app_id":"slack","status":h.status == Some(1),"inbox":null,"account_id":account,"hook_type":"account"});
    if admin {
        let settings = h.settings.as_ref().and_then(Value::as_object);
        let mut visible = serde_json::Map::new();
        for key in ["channel_name", "message_mode"] { if let Some(v) = settings.and_then(|s| s.get(key)) { visible.insert(key.into(), v.clone()); } }
        value["settings"] = Value::Object(visible);
        value["reference_id"] = json!(h.reference_id);
    }
    value
}
async fn app(db: &PgPool, account: i64, admin: bool) -> Result<Value, ApiError> {
    let rows: Vec<Hook> = sqlx::query_as("SELECT id,inbox_id,status,reference_id,settings,access_token FROM integrations_hooks WHERE account_id=$1 AND app_id='slack' ORDER BY id").bind(account).fetch_all(db).await?;
    let mut hooks = Vec::new();
    for h in &rows {
        let mut v = hook_json(h, account, admin);
        if let Some(id) = h.inbox_id {
            let inbox: Option<(i32, String)> = sqlx::query_as("SELECT id,name FROM inboxes WHERE id=$1 AND account_id=$2").bind(id).bind(account).fetch_optional(db).await?;
            v["inbox"] = inbox.map(|(id,name)| json!({"id":id,"name":name})).unwrap_or(Value::Null);
        }
        hooks.push(v);
    }
    let mut out = json!({"id":"slack","name":"Slack","description":"Integrate Pulsar with Slack to keep your team in sync. This integration allows you to receive notifications for new conversations and respond to them directly within Slack's interface.","short_description":"Receive notifications and respond to conversations directly in Slack.","enabled":!rows.is_empty(),"hooks":hooks});
    if admin {
        let redirect = format!("{}/app/accounts/{account}/settings/integrations/slack", std::env::var("FRONTEND_URL").unwrap_or_default().trim_end_matches('/'));
        let action = format!("https://slack.com/oauth/v2/authorize?scope=commands,chat:write,channels:read,channels:manage,channels:join,groups:read,groups:write,im:write,mpim:write,users:read,users:read.email,chat:write.customize,channels:history,groups:history,mpim:history,im:history,files:read,files:write&client_id={}&redirect_uri={}", config(db,"SLACK_CLIENT_ID", "").await?, redirect);
        out["logo"] = json!("slack.png"); out["i18n_key"] = json!("slack"); out["hook_type"] = json!("account"); out["allow_multiple_hooks"] = json!(false);
        out["visible_properties"] = json!(["channel_name","message_mode"]);
        out["settings_json_schema"] = json!({"type":"object","properties":{"channel_name":{"type":"string"},"message_mode":{"type":"string","enum":["two_way","alert"]}}});
        out["action"] = json!(action); out["button"] = out["action"].clone();
    }
    Ok(out)
}
async fn app_show(State(st): State<AppState>, user: CurrentUser, Path((account,id)): Path<(i64,String)>) -> ApiResult {
    let au = require_account(&st.db, &user, account).await?;
    if id != "slack" { return Err(ApiError::not_found()); }
    Ok(Json(app(&st.db, account, au.is_admin()).await?))
}
async fn create(State(st): State<AppState>, user: CurrentUser, Path(account): Path<i64>, Json(body): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account).await?.require_admin()?;
    let code = body["code"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| ApiError::bad("Code is required"))?;
    if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM integrations_hooks WHERE account_id=$1 AND app_id='slack')").bind(account).fetch_one(&st.db).await? { return Err(ApiError::bad("App has already been taken")); }
    let inbox = body["inbox_id"].as_i64();
    if let Some(id) = inbox { if !sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM inboxes WHERE id=$1 AND account_id=$2)").bind(id).bind(account).fetch_one(&st.db).await? { return Err(ApiError::not_found()); } }
    let id = config(&st.db,"SLACK_CLIENT_ID","TEST_CLIENT_ID").await?;
    let secret = config(&st.db,"SLACK_CLIENT_SECRET","TEST_CLIENT_SECRET").await?;
    let redirect = format!("{}/app/accounts/{account}/settings/integrations/slack", std::env::var("FRONTEND_URL").unwrap_or_default().trim_end_matches('/'));
    let response = slack("oauth.v2.access", &[("client_id", &id),("client_secret", &secret),("code",code),("redirect_uri",&redirect)], None).await?;
    let token = response["access_token"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| ApiError::bad("Invalid Slack response"))?;
    let encrypted = crate::ar_encryption::encrypt_deterministic(token)?;
    if encrypted.len() > 255 { return Err(ApiError::bad("Slack token is too long")); }
    sqlx::query("INSERT INTO integrations_hooks (account_id,app_id,inbox_id,hook_type,status,access_token,settings,created_at,updated_at) VALUES ($1,'slack',$2,0,0,$3,'{}',NOW(),NOW())").bind(account).bind(inbox).bind(encrypted).execute(&st.db).await?;
    Ok(Json(app(&st.db,account,true).await?))
}
async fn channel_list(h: &Hook) -> Result<Vec<Value>, ApiError> {
    let token = crate::ar_encryption::decrypt_deterministic(h.access_token.as_deref().ok_or_else(|| ApiError::bad("Missing Slack token"))?)?;
    let mut all = Vec::new();
    for kind in ["private_channel", "public_channel"] {
        let mut cursor = String::new();
        loop {
            let result = slack("conversations.list", &[("types",kind),("exclude_archived","true"),("limit","1000"),("cursor",&cursor)], Some(&token)).await?;
            all.extend(result["channels"].as_array().ok_or_else(|| ApiError::bad("Invalid Slack response"))?.iter().cloned());
            let next = result["response_metadata"]["next_cursor"].as_str().unwrap_or("");
            if next.is_empty() { break; }
            if next == cursor { return Err(ApiError::bad("Invalid Slack pagination")); }
            cursor = next.to_owned();
        }
    }
    Ok(all)
}
async fn channels(State(st): State<AppState>, user: CurrentUser, Path(account): Path<i64>) -> ApiResult {
    require_account(&st.db,&user,account).await?.require_admin()?;
    let h = hook(&st.db,account).await?;
    Ok(Json(json!(channel_list(&h).await?.iter().map(|c| json!({"id":c["id"],"name":c["name"],"is_private":c["is_private"]})).collect::<Vec<_>>())))
}
async fn update(State(st): State<AppState>, user: CurrentUser, Path(account): Path<i64>, Json(body): Json<Value>) -> ApiResult {
    require_account(&st.db,&user,account).await?.require_admin()?;
    let h = hook(&st.db,account).await?;
    if let Some(mode) = body["message_mode"].as_str().filter(|s| !s.is_empty()) {
        if !["alert","two_way"].contains(&mode) { return Err(ApiError::bad("Invalid settings data")); }
        sqlx::query("UPDATE integrations_hooks SET settings=COALESCE(settings,'{}'::jsonb) || jsonb_build_object('message_mode',$1::text), updated_at=NOW() WHERE id=$2 AND account_id=$3").bind(mode).bind(h.id).bind(account).execute(&st.db).await?;
    } else {
        let reference = body["reference_id"].as_str().unwrap_or("");
        let channels = channel_list(&h).await?;
        let channel = channels.iter().find(|c| c["id"].as_str() == Some(reference)).ok_or_else(|| ApiError::bad("Invalid slack channel. Please try again"))?;
        if channel["is_private"] == false {
            let token = crate::ar_encryption::decrypt_deterministic(h.access_token.as_deref().ok_or_else(|| ApiError::bad("Missing Slack token"))?)?;
            slack("conversations.join", &[("channel",reference)],Some(&token)).await?;
        }
        let name = channel["name"].as_str().ok_or_else(|| ApiError::bad("Invalid Slack response"))?;
        sqlx::query("UPDATE integrations_hooks SET reference_id=$1,settings=COALESCE(settings,'{}'::jsonb) || jsonb_build_object('channel_name',$2::text),status=1,updated_at=NOW() WHERE id=$3 AND account_id=$4").bind(reference).bind(name).bind(h.id).bind(account).execute(&st.db).await?;
    }
    Ok(Json(app(&st.db,account,true).await?))
}
async fn destroy(State(st): State<AppState>, user: CurrentUser, Path(account): Path<i64>) -> Result<StatusCode,ApiError> {
    require_account(&st.db,&user,account).await?.require_admin()?;
    let h = hook(&st.db,account).await?;
    sqlx::query("DELETE FROM integrations_hooks WHERE id=$1 AND account_id=$2").bind(h.id).bind(account).execute(&st.db).await?;
    Ok(StatusCode::OK)
}
async fn process_event(State(st): State<AppState>, user: CurrentUser, Path((account,id)): Path<(i64,i64)>) -> ApiResult {
    require_account(&st.db,&user,account).await?.require_admin()?;
    let app_id: (String,) = sqlx::query_as("SELECT app_id FROM integrations_hooks WHERE id=$1 AND account_id=$2").bind(id).bind(account).fetch_one(&st.db).await?;
    if app_id.0 == "stripe" { return Err(ApiError::not_found()); }
    Err(ApiError::bad("No processor found"))
}
