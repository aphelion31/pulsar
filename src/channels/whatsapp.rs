//! WhatsApp Cloud webhooks, provider operations and outbound delivery.
use crate::{
    common::{ApiError, ApiResult, AppState, CurrentUser, require_account},
    conversations::{insert_message, reload},
    events::Event,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2_compat::Sha256;
use sqlx::{PgPool, Row};

fn base() -> String {
    std::env::var("WHATSAPP_CLOUD_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://graph.facebook.com".into())
        .trim_end_matches('/')
        .to_owned()
}
fn field<'a>(v: &'a Value, k: &str) -> Result<&'a str, ApiError> {
    v[k].as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::bad(format!("{k} can't be blank")))
}
fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn verify(secret: &str, signature: &str, body: &[u8]) -> bool {
    if secret.is_empty() {
        return false;
    }
    let Some(sig) = signature
        .strip_prefix("sha256=")
        .and_then(|s| hex::decode(s).ok())
    else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&sig).is_ok()
}
pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/webhooks/whatsapp/{phone_number}",
        get(verify_webhook).post(webhook),
    )
}
pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/channels/whatsapp_channel", post(create))
        .route("/whatsapp/authorization", post(embedded_authorization))
        .route("/whatsapp/manual/preview", post(manual_preview))
        .route("/whatsapp/manual/connect", post(manual_connect))
        .route(
            "/whatsapp/manual/{inbox_id}/webhook_status",
            get(manual_webhook_status),
        )
        .route(
            "/whatsapp/manual/{inbox_id}/setup_webhook",
            post(manual_setup_webhook),
        )
}
async fn create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account)
        .await?
        .require_admin()?;
    let ch = body
        .get("whatsapp_channel")
        .filter(|v| v.is_object())
        .unwrap_or(&body);
    let name = field(ch, "name").or_else(|_| field(&body, "name"))?;
    let phone = field(ch, "phone_number")?;
    let mut config = ch["provider_config"].clone();
    if ch["provider"].as_str().unwrap_or("whatsapp_cloud") != "whatsapp_cloud" {
        return Err(ApiError::bad("Unsupported WhatsApp provider"));
    }
    for key in [
        "api_key",
        "app_secret",
        "phone_number_id",
        "business_account_id",
    ] {
        field(&config, key)?;
    }
    for key in ["phone_number_id", "business_account_id"] {
        if !safe_id(field(&config, key)?) {
            return Err(ApiError::bad("Invalid provider ID"));
        }
    }
    if config["webhook_verify_token"]
        .as_str()
        .is_none_or(str::is_empty)
    {
        config["webhook_verify_token"] = json!(uuid::Uuid::new_v4().simple().to_string());
    }
    let mut tx = st.db.begin().await?;
    let id: i64 = sqlx::query_scalar("INSERT INTO channel_whatsapp (account_id, phone_number, provider, provider_config, created_at, updated_at) VALUES ($1, $2, 'whatsapp_cloud', $3, now(), now()) RETURNING id").bind(account as i32).bind(phone).bind(&config).fetch_one(&mut *tx).await?;
    let inbox: i64 = sqlx::query_scalar("INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at) VALUES ($1, $2, $3, 'Channel::Whatsapp', now(), now()) RETURNING id::bigint").bind(id).bind(account).bind(name).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO inbox_members (user_id, inbox_id, created_at, updated_at) VALUES ($1, $2, now(), now())").bind(user.id).bind(inbox).execute(&mut *tx).await?;
    tx.commit().await?;
    st.hub.invalidate_cache(account, "inbox");
    let mut result = crate::inboxes::inbox_json(&st.db, account, inbox, true).await?;
    result["provider"] = json!("whatsapp_cloud");
    result["provider_config"] = config;
    result["message_templates"] = json!([]);
    result["reauthorization_required"] = json!(false);
    Ok(Json(result))
}
async fn config(db: &PgPool, id: i64) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar("SELECT provider_config FROM channel_whatsapp WHERE id = $1 AND provider = 'whatsapp_cloud'").bind(id).fetch_optional(db).await?.ok_or_else(ApiError::not_found)?)
}
async fn channel(db: &PgPool, phone: &str) -> Result<Option<sqlx::postgres::PgRow>, ApiError> {
    Ok(sqlx::query("SELECT w.provider_config, i.id::bigint AS inbox_id, i.account_id::bigint AS account_id, i.lock_to_single_conversation FROM channel_whatsapp w JOIN inboxes i ON i.channel_id = w.id AND i.account_id = w.account_id AND i.channel_type = 'Channel::Whatsapp' WHERE w.phone_number = $1 AND w.provider = 'whatsapp_cloud'").bind(phone).fetch_optional(db).await?)
}
async fn verify_webhook(
    State(st): State<AppState>,
    Path(phone): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let Some(row) = channel(&st.db, &phone).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let cfg: Value = row.try_get("provider_config")?;
    let expected = cfg["webhook_verify_token"].as_str().unwrap_or("");
    if q.get("hub.mode").map(String::as_str) != Some("subscribe")
        || expected.is_empty()
        || q.get("hub.verify_token").map(String::as_str) != Some(expected)
    {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    Ok(q.get("hub.challenge")
        .cloned()
        .unwrap_or_default()
        .into_response())
}
async fn webhook(
    State(st): State<AppState>,
    Path(phone): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let Some(row) = channel(&st.db, &phone).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let cfg: Value = row.try_get("provider_config")?;
    let secret = cfg["app_secret"].as_str().unwrap_or("");
    if !verify(
        secret,
        headers
            .get("x-hub-signature-256")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
        &body,
    ) {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    let payload: Value =
        serde_json::from_slice(&body).map_err(|_| ApiError::bad("Invalid JSON"))?;
    let inbox: i64 = row.try_get("inbox_id")?;
    let account: i64 = row.try_get("account_id")?;
    let locked: bool = row.try_get("lock_to_single_conversation")?;
    for entry in payload["entry"].as_array().into_iter().flatten() {
        for change in entry["changes"].as_array().into_iter().flatten() {
            let v = &change["value"];
            // The signed endpoint must not process events for another phone ID.
            if v["metadata"]["phone_number_id"]
                .as_str()
                .is_some_and(|id| Some(id) != cfg["phone_number_id"].as_str())
            {
                continue;
            }
            for status in v["statuses"].as_array().into_iter().flatten() {
                let Some(id) = status["id"].as_str() else {
                    continue;
                };
                let n = match status["status"].as_str() {
                    Some("sent") => 0,
                    Some("delivered") => 1,
                    Some("read") => 2,
                    Some("failed") => 3,
                    _ => continue,
                };
                sqlx::query("UPDATE messages SET status = $1, updated_at = now() WHERE inbox_id = $2 AND source_id = $3 AND (status < $1 OR $1 = 3)").bind(n).bind(inbox as i32).bind(id).execute(&st.db).await?;
            }
            for msg in v["messages"].as_array().into_iter().flatten() {
                process_message(&st, account, inbox, locked, &cfg, v, msg).await?;
            }
        }
    }
    Ok(StatusCode::OK.into_response())
}
async fn process_message(
    st: &AppState,
    account: i64,
    inbox: i64,
    locked: bool,
    cfg: &Value,
    value: &Value,
    msg: &Value,
) -> Result<(), ApiError> {
    let (Some(id), Some(from)) = (msg["id"].as_str(), msg["from"].as_str()) else {
        return Ok(());
    };
    if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM messages WHERE inbox_id = $1 AND source_id = $2)",
    )
    .bind(inbox as i32)
    .bind(id)
    .fetch_one(&st.db)
    .await?
    {
        return Ok(());
    }
    let name = value["contacts"]
        .as_array()
        .and_then(|a| a.iter().find(|c| c["wa_id"].as_str() == Some(from)))
        .and_then(|c| c["profile"]["name"].as_str())
        .unwrap_or(from);
    let row = sqlx::query("SELECT id::bigint AS id, contact_id::bigint AS contact_id FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2").bind(inbox as i32).bind(from).fetch_optional(&st.db).await?;
    let (ci, contact) = if let Some(r) = row {
        (
            r.try_get::<i64, _>("id")?,
            r.try_get::<i64, _>("contact_id")?,
        )
    } else {
        let c: i64 = sqlx::query_scalar("INSERT INTO contacts (name, phone_number, account_id, created_at, updated_at, last_activity_at) VALUES ($1, $2, $3, now(), now(), now()) RETURNING id::bigint").bind(name).bind(format!("+{}", from.trim_start_matches('+'))).bind(account as i32).fetch_one(&st.db).await?;
        let ci: i64 = sqlx::query_scalar("INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at, pubsub_token) VALUES ($1, $2, $3, now(), now(), $4) RETURNING id::bigint").bind(c).bind(inbox as i32).bind(from).bind(uuid::Uuid::new_v4().to_string()).fetch_one(&st.db).await?;
        (ci, c)
    };
    let conv: Option<i64> = sqlx::query_scalar("SELECT id::bigint FROM conversations WHERE contact_inbox_id = $1 AND account_id = $2 AND ($3 OR status <> 1) ORDER BY id DESC LIMIT 1").bind(ci).bind(account as i32).bind(locked).fetch_optional(&st.db).await?;
    let conv = match conv { Some(id) => id, None => sqlx::query_scalar("INSERT INTO conversations (account_id, inbox_id, status, contact_id, contact_inbox_id, additional_attributes, custom_attributes, last_activity_at, created_at, updated_at) VALUES ($1, $2, 0, $3, $4, '{}', '{}', now(), now(), now()) RETURNING id::bigint").bind(account as i32).bind(inbox as i32).bind(contact).bind(ci).fetch_one(&st.db).await? };
    let conv = reload(&st.db, conv).await?;
    let kind = msg["type"].as_str().unwrap_or("");
    let content = match kind {
        "text" => msg["text"]["body"].as_str().unwrap_or("").to_owned(),
        "location" => format!(
            "{} {}",
            msg["location"]["latitude"], msg["location"]["longitude"]
        ),
        "contacts" => msg["contacts"].to_string(),
        "interactive" => msg["interactive"]["button_reply"]["title"]
            .as_str()
            .or_else(|| msg["interactive"]["list_reply"]["title"].as_str())
            .unwrap_or("")
            .to_owned(),
        _ => msg[kind]["caption"].as_str().unwrap_or("").to_owned(),
    };
    let mut attrs = json!({"__pulsar_source_id": id});
    if let Some(reply) = msg["context"]["id"].as_str() {
        attrs["in_reply_to_external_id"] = json!(reply);
    }
    let created = insert_message(
        st,
        &conv,
        Some(("Contact", contact)),
        &content,
        0,
        false,
        0,
        attrs,
        None,
    )
    .await?;
    if ["image", "video", "audio", "document", "sticker"].contains(&kind) {
        if let Err(e) = media(st, account, cfg, msg, kind, &created).await {
            tracing::warn!(error = %e, "whatsapp media download failed");
        }
    }
    Ok(())
}

async fn media(
    st: &AppState,
    account: i64,
    cfg: &Value,
    msg: &Value,
    kind: &str,
    created: &Value,
) -> anyhow::Result<()> {
    let id = msg[kind]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing media id"))?;
    if !safe_id(id) {
        anyhow::bail!("invalid media id")
    }
    let token = cfg["api_key"].as_str().unwrap_or("");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let info: Value = serde_json::from_slice(
        &client
            .get(format!("{}/v13.0/{id}", base()))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?,
    )?;
    let url = url::Url::parse(
        info["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing media URL"))?,
    )?;
    let api = url::Url::parse(&base())?;
    if url.scheme() != "https" && url.origin() != api.origin() {
        anyhow::bail!("unsafe media URL")
    }
    if url.origin() != api.origin()
        && !url.host_str().is_some_and(|h| {
            h == "facebook.com"
                || h.ends_with(".facebook.com")
                || h == "fbcdn.net"
                || h.ends_with(".fbcdn.net")
        })
    {
        anyhow::bail!("untrusted media host")
    }
    let mut response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > crate::storage::MAX_BYTES {
            anyhow::bail!("media too large")
        }
        bytes.extend_from_slice(&chunk);
    }
    let filename = msg[kind]["filename"].as_str().unwrap_or("attachment");
    let mime = msg[kind]["mime_type"]
        .as_str()
        .unwrap_or("application/octet-stream");
    if let Some(mid) = created["id"].as_i64() {
        crate::storage::attach_to_message(
            &st.db,
            account,
            mid,
            filename,
            mime,
            &bytes,
            match kind {
                "image" | "sticker" => Some(0),
                "audio" => Some(1),
                "video" => Some(2),
                _ => None,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    Ok(())
}

async fn graph_get(cfg: &Value, path: &str, query: &[(&str, &str)]) -> Result<Value, ApiError> {
    let token = field(cfg, "api_key")?;
    let response = reqwest::Client::new()
        .get(format!("{}{path}", base()))
        .bearer_auth(token)
        .query(query)
        .send()
        .await
        .map_err(|_| ApiError::bad("WhatsApp provider unavailable"))?;
    if !response.status().is_success() {
        return Err(ApiError::bad("WhatsApp provider request failed"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| ApiError::bad("WhatsApp provider response failed"))?;
    serde_json::from_slice(&bytes).map_err(|_| ApiError::bad("Invalid WhatsApp provider response"))
}

pub(crate) async fn sync_templates(db: &PgPool, id: i64) -> Result<(), ApiError> {
    let cfg = config(db, id).await?;
    let business = field(&cfg, "business_account_id")?;
    if !safe_id(business) {
        return Err(ApiError::bad("Invalid business account ID"));
    }
    let mut templates = Vec::new();
    let mut cursor = String::new();
    for _ in 0..100 {
        let query = if cursor.is_empty() {
            vec![]
        } else {
            vec![("after", cursor.as_str())]
        };
        let response = graph_get(
            &cfg,
            &format!("/v14.0/{business}/message_templates"),
            &query,
        )
        .await?;
        let data = response["data"]
            .as_array()
            .ok_or_else(|| ApiError::bad("Invalid templates response"))?;
        templates.extend(data.iter().cloned());
        let Some(next) = response["paging"]["cursors"]["after"]
            .as_str()
            .filter(|s| !s.is_empty())
        else {
            break;
        };
        if next == cursor {
            return Err(ApiError::bad("Invalid template pagination"));
        }
        cursor = next.to_owned();
    }
    sqlx::query("UPDATE channel_whatsapp SET message_templates = $2, message_templates_last_updated = now() WHERE id = $1").bind(id).bind(json!(templates)).execute(db).await?;
    Ok(())
}

pub(crate) async fn health(db: &PgPool, id: i64) -> Result<Value, ApiError> {
    let cfg = config(db, id).await?;
    let phone = field(&cfg, "phone_number_id")?;
    let business = field(&cfg, "business_account_id")?;
    if !safe_id(phone) || !safe_id(business) {
        return Err(ApiError::bad("Invalid provider ID"));
    }
    let mut result = graph_get(&cfg, &format!("/v24.0/{phone}"), &[("fields", "id,quality_rating,whatsapp_business_manager_messaging_limit,status,code_verification_status,account_mode,display_phone_number,name_status,verified_name,webhook_configuration,throughput,last_onboarded_time,is_on_biz_app,platform_type")]).await?;
    let business = graph_get(
        &cfg,
        &format!("/v24.0/{business}"),
        &[("fields", "id,name,owner_business_info")],
    )
    .await?;
    if !result.is_object() {
        return Err(ApiError::bad("Invalid health response"));
    }
    result["messaging_limit_tier"] = result["whatsapp_business_manager_messaging_limit"].clone();
    result["throughput_level"] = result["throughput"]["level"].clone();
    result["business_account_id"] = business["id"].clone();
    result["business_account_name"] = business["name"].clone();
    result["business_portfolio_id"] = business["owner_business_info"]["id"].clone();
    result["business_portfolio_name"] = business["owner_business_info"]["name"].clone();
    result["health_checked_at"] = json!(chrono::Utc::now().to_rfc3339());
    sqlx::query("UPDATE channel_whatsapp SET phone_number_health = $2, phone_number_health_checked_at = now(), phone_number_health_error = NULL WHERE id = $1").bind(id).bind(&result).execute(db).await?;
    Ok(result)
}

pub async fn outbound(st: AppState, ev: Event) {
    if ev.name != "message.created" {
        return;
    }
    if let Err(e) = send_outbound(&st, &ev).await {
        tracing::warn!(error = %e, "WhatsApp outbound failed");
    }
}
async fn send_outbound(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(id) = ev.data["id"].as_i64() else {
        return Ok(());
    };
    let Some(row) = sqlx::query("SELECT m.content, m.content_attributes, m.additional_attributes, ci.source_id, w.provider_config FROM messages m JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id JOIN contact_inboxes ci ON ci.id = c.contact_inbox_id JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::Whatsapp' JOIN channel_whatsapp w ON w.id = i.channel_id AND w.account_id = i.account_id AND w.provider = 'whatsapp_cloud' WHERE m.id = $1 AND m.account_id = $2 AND m.message_type = 1 AND NOT COALESCE(m.private, false)").bind(id).bind(ev.account_id).fetch_optional(&st.db).await? else { return Ok(()) };
    let cfg: Value = row.try_get("provider_config")?;
    let phone = cfg["phone_number_id"].as_str().unwrap_or("");
    let token = cfg["api_key"].as_str().unwrap_or("");
    let result: anyhow::Result<String> = async {
        if !safe_id(phone) || token.is_empty() { anyhow::bail!("Missing WhatsApp credentials") }
        let to: String = row.try_get("source_id")?; let content: Option<String> = row.try_get("content")?;
        let attrs: Option<Value> = row.try_get("additional_attributes")?; let template = attrs.as_ref().and_then(|v| v.get("template_params")).filter(|v| v.is_object());
        let attachments = crate::storage::attachments_json(&st.db, id).await;
        let mut payload = json!({"messaging_product":"whatsapp", "to":to});
        let version;
        if let Some(t) = template {
            let name = t["name"].as_str().filter(|v| !v.is_empty()).ok_or_else(|| anyhow::anyhow!("Missing template name"))?;
            payload["type"] = json!("template"); payload["template"] = json!({"name":name,"language":{"policy":"deterministic","code":t["language"].as_str().or_else(|| t["lang_code"].as_str()).unwrap_or("en")},"components":t["parameters"].as_array().cloned().unwrap_or_default()}); version = "v13.0";
        } else if let Some(a) = attachments.first() {
            let kind = a["file_type"].as_str().unwrap_or("file"); let kind = if ["image", "audio", "video"].contains(&kind) { kind } else { "document" };
            let link = a["data_url"].as_str().ok_or_else(|| anyhow::anyhow!("Missing attachment URL"))?;
            let link = if link.starts_with('/') { format!("{}{}", st.frontend_url.trim_end_matches('/'), link) } else { link.to_owned() };
            payload["type"] = json!(kind); payload[kind] = json!({"link":link});
            if kind != "audio" { payload[kind]["caption"] = json!(content.unwrap_or_default()); } version = "v24.0";
        } else { payload["type"] = json!("text"); payload["text"] = json!({"body":content.unwrap_or_default()}); version = "v13.0"; }
        let content_attrs: Option<Value> = row.try_get("content_attributes")?;
        if let Some(reply) = content_attrs.as_ref().and_then(|v| v["in_reply_to_external_id"].as_str()) { payload["context"] = json!({"message_id": reply}); }
        let response = reqwest::Client::new().post(format!("{}/{version}/{phone}/messages", base())).bearer_auth(token).header("content-type", "application/json").body(payload.to_string()).send().await?;
        let status = response.status(); let body: Value = serde_json::from_slice(&response.bytes().await?).unwrap_or(Value::Null);
        if !status.is_success() { anyhow::bail!("{}", body["error"]["message"].as_str().unwrap_or("WhatsApp provider request failed")); }
        Ok(body["messages"][0]["id"].as_str().ok_or_else(|| anyhow::anyhow!("Missing WhatsApp message ID"))?.to_owned())
    }.await;
    match result {
        Ok(source) => {
            sqlx::query("UPDATE messages SET source_id = $1, updated_at = now() WHERE id = $2 AND account_id = $3").bind(source).bind(id).bind(ev.account_id).execute(&st.db).await?;
        }
        Err(e) => {
            sqlx::query("UPDATE messages SET status = 3, content_attributes = COALESCE(content_attributes, '{}'::json)::jsonb || jsonb_build_object('external_error', $1::text), updated_at = now() WHERE id = $2 AND account_id = $3").bind(e.to_string()).bind(id).bind(ev.account_id).execute(&st.db).await?;
        }
    }
    Ok(())
}

fn normalize_phone_plus(raw: &str) -> String {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    format!("+{digits}")
}

fn api_version() -> String {
    std::env::var("WHATSAPP_API_VERSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "v22.0".into())
}

async fn meta_get(
    token: &str,
    path: &str,
    query: &[(&str, &str)],
    err_msg: &str,
) -> Result<Value, ApiError> {
    let resp = reqwest::Client::new()
        .get(format!("{}/{}{path}", base(), api_version()))
        .bearer_auth(token)
        .query(query)
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("{err_msg}: {e}")))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad(format!("{err_msg}: {e}")))?;
    let val: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if !status.is_success() {
        let detail = val["error"]["message"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(err_msg);
        return Err(ApiError::bad(detail));
    }
    Ok(val)
}

async fn meta_post(
    token: &str,
    path: &str,
    payload: Value,
    err_msg: &str,
) -> Result<Value, ApiError> {
    let body = serde_json::to_vec(&payload).map_err(|e| ApiError::bad(e.to_string()))?;
    let resp = reqwest::Client::new()
        .post(format!("{}/{}{path}", base(), api_version()))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("{err_msg}: {e}")))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad(format!("{err_msg}: {e}")))?;
    let val: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if !status.is_success() {
        let detail = val["error"]["message"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(err_msg);
        return Err(ApiError::bad(detail));
    }
    Ok(val)
}

async fn subscribe_webhook_meta(
    waba_id: &str,
    phone_number_id: &str,
    access_token: &str,
    callback_url: &str,
    verify_token: &str,
) -> Result<(), ApiError> {
    meta_post(
        access_token,
        &format!("/{waba_id}/subscribed_apps"),
        json!({ "subscribed_fields": ["messages", "smb_message_echoes"] }),
        "App subscription to WABA failed",
    )
    .await?;
    meta_post(
        access_token,
        &format!("/{phone_number_id}"),
        json!({
            "webhook_configuration": {
                "override_callback_uri": callback_url,
                "verify_token": verify_token,
            }
        }),
        "Phone number webhook callback override failed",
    )
    .await?;
    Ok(())
}

async fn validate_manual_setup(
    db: &PgPool,
    waba_id: &str,
    phone_number_id: &str,
    access_token: &str,
) -> Result<Value, ApiError> {
    if waba_id.trim().is_empty() {
        return Err(ApiError::bad("WABA ID is required"));
    }
    if phone_number_id.trim().is_empty() {
        return Err(ApiError::bad("Phone Number ID is required"));
    }
    if access_token.trim().is_empty() {
        return Err(ApiError::bad("Access token is required"));
    }
    if !safe_id(waba_id) || !safe_id(phone_number_id) {
        return Err(ApiError::bad("Invalid WABA or Phone Number ID"));
    }

    let numbers_resp = meta_get(
        access_token,
        &format!("/{waba_id}/phone_numbers"),
        &[],
        "WABA phone numbers fetch failed",
    )
    .await?;
    let Some(mut phone_data) = numbers_resp["data"].as_array().and_then(|arr| {
        arr.iter()
            .find(|p| p["id"].as_str() == Some(phone_number_id))
            .cloned()
    }) else {
        return Err(ApiError::bad(
            "This Phone Number ID does not belong to the WABA ID you entered.",
        ));
    };

    let status_data = meta_get(
        access_token,
        &format!("/{phone_number_id}"),
        &[("fields", "status,code_verification_status")],
        "Phone number fetch failed",
    )
    .await?;
    if let (Some(dst), Some(src)) = (phone_data.as_object_mut(), status_data.as_object()) {
        for (k, v) in src {
            dst.insert(k.clone(), v.clone());
        }
    }
    let ready = phone_data["status"].as_str() == Some("CONNECTED")
        || phone_data["code_verification_status"].as_str() == Some("VERIFIED");
    if !ready {
        return Err(ApiError::bad(
            "Complete phone number verification in Meta before continuing.",
        ));
    }

    let display_phone = phone_data["display_phone_number"]
        .as_str()
        .unwrap_or_default();
    let phone_number = normalize_phone_plus(display_phone);
    let dup_phone: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM channel_whatsapp WHERE phone_number = $1)")
            .bind(&phone_number)
            .fetch_one(db)
            .await?;
    if dup_phone {
        return Err(ApiError::bad(
            "This WhatsApp number is already connected to another inbox.",
        ));
    }
    let dup_pid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM channel_whatsapp WHERE provider_config->>'phone_number_id' = $1)",
    )
    .bind(phone_number_id)
    .fetch_one(db)
    .await?;
    if dup_pid {
        return Err(ApiError::bad(
            "This Phone Number ID is already used by another WhatsApp inbox.",
        ));
    }

    if meta_get(
        access_token,
        &format!("/{waba_id}/message_templates"),
        &[("limit", "1")],
        "template access",
    )
    .await
    .is_err()
    {
        return Err(ApiError::bad(
            "The token can access the number but cannot access message templates. Generate a token with whatsapp_business_management permission.",
        ));
    }

    let perms = meta_get(
        access_token,
        "/me/permissions",
        &[],
        "permissions fetch failed",
    )
    .await
    .map_err(|_| {
        ApiError::bad(
            "The token cannot access WhatsApp messaging. Generate a token with whatsapp_business_messaging permission.",
        )
    })?;
    let granted = perms["data"].as_array().is_some_and(|arr| {
        arr.iter().any(|p| {
            p["permission"].as_str() == Some("whatsapp_business_messaging")
                && p["status"].as_str() == Some("granted")
        })
    });
    if !granted {
        return Err(ApiError::bad(
            "The token cannot access WhatsApp messaging. Generate a token with whatsapp_business_messaging permission.",
        ));
    }

    let verified_name = phone_data["verified_name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);
    let suggested = format!(
        "{} WhatsApp",
        verified_name.as_deref().unwrap_or(&phone_number)
    );
    Ok(json!({
        "verified_name": verified_name,
        "display_phone_number": phone_number,
        "phone_number_id": phone_number_id,
        "waba_id": waba_id,
        "template_access": true,
        "suggested_inbox_name": suggested,
    }))
}

/// POST /whatsapp/authorization — embedded signup create or reauthorize
async fn embedded_authorization(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let code = body["code"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Required parameters are missing: code"))?;
    let waba_id = body["waba_id"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && safe_id(s))
        .ok_or_else(|| ApiError::bad("Required parameters are missing: waba_id"))?;
    let phone_number_id_param = body["phone_number_id"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && safe_id(s));
    let inbox_id_param = body["inbox_id"].as_i64();

    let app_id = std::env::var("WHATSAPP_APP_ID").unwrap_or_default();
    let app_secret = std::env::var("WHATSAPP_APP_SECRET").unwrap_or_default();
    let tok_resp = reqwest::Client::new()
        .get(format!("{}/{}/oauth/access_token", base(), api_version()))
        .query(&[
            ("client_id", app_id.as_str()),
            ("client_secret", app_secret.as_str()),
            ("code", code),
        ])
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("Token exchange failed: {e}")))?;
    if !tok_resp.status().is_success() {
        return Err(ApiError::bad("Token exchange failed"));
    }
    let tok_val: Value =
        serde_json::from_slice(&tok_resp.bytes().await.unwrap_or_default()).unwrap_or(Value::Null);
    let access_token = tok_val["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Token exchange returned empty access_token"))?;

    let numbers = meta_get(
        access_token,
        &format!("/{waba_id}/phone_numbers"),
        &[],
        "WABA phone numbers fetch failed",
    )
    .await?;
    let phone_entry = numbers["data"]
        .as_array()
        .and_then(|arr| {
            if let Some(pid) = phone_number_id_param {
                arr.iter().find(|p| p["id"].as_str() == Some(pid)).cloned()
            } else {
                arr.first().cloned()
            }
        })
        .ok_or_else(|| ApiError::bad("No phone numbers found for WABA"))?;
    let pid = phone_entry["id"]
        .as_str()
        .unwrap_or(phone_number_id_param.unwrap_or(""));
    let phone_number =
        normalize_phone_plus(phone_entry["display_phone_number"].as_str().unwrap_or(pid));
    let verified_name = phone_entry["verified_name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(&phone_number);
    let verify_token = uuid::Uuid::new_v4().simple().to_string();
    let provider_config = json!({
        "api_key": access_token,
        "phone_number_id": pid,
        "business_account_id": waba_id,
        "webhook_verify_token": verify_token,
        "source": "embedded_signup",
    });

    let (inbox_id, inbox_name) = if let Some(iid) = inbox_id_param {
        let row = sqlx::query(
            "SELECT i.channel_id::bigint AS channel_id, i.name FROM inboxes i WHERE i.id::bigint = $1 AND i.account_id::bigint = $2 AND i.channel_type = 'Channel::Whatsapp'",
        )
        .bind(iid)
        .bind(account_id)
        .fetch_optional(&st.db)
        .await?
        .ok_or_else(ApiError::not_found)?;
        let ch_id: i64 = row.try_get("channel_id")?;
        let name: String = row.try_get("name")?;
        sqlx::query(
            "UPDATE channel_whatsapp SET phone_number = $2, provider_config = $3, updated_at = now() WHERE id::bigint = $1",
        )
        .bind(ch_id)
        .bind(&phone_number)
        .bind(&provider_config)
        .execute(&st.db)
        .await?;
        (iid, name)
    } else {
        let mut tx = st.db.begin().await?;
        let ch_id: i64 = sqlx::query_scalar(
            "INSERT INTO channel_whatsapp (account_id, phone_number, provider, provider_config, created_at, updated_at)
             VALUES ($1, $2, 'whatsapp_cloud', $3, now(), now()) RETURNING id::bigint",
        )
        .bind(account_id as i32)
        .bind(&phone_number)
        .bind(&provider_config)
        .fetch_one(&mut *tx)
        .await?;
        let iid: i64 = sqlx::query_scalar(
            "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
             VALUES ($1, $2, $3, 'Channel::Whatsapp', now(), now()) RETURNING id::bigint",
        )
        .bind(ch_id)
        .bind(account_id)
        .bind(verified_name)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO inbox_members (user_id, inbox_id, created_at, updated_at) VALUES ($1, $2, now(), now())",
        )
        .bind(user.id)
        .bind(iid)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        (iid, verified_name.to_string())
    };
    st.hub.invalidate_cache(account_id, "inbox");

    let callback_url = format!(
        "{}/webhooks/whatsapp/{phone_number}",
        st.frontend_url.trim_end_matches('/')
    );
    let _ = subscribe_webhook_meta(waba_id, pid, access_token, &callback_url, &verify_token).await;

    let mut resp = json!({
        "success": true,
        "id": inbox_id,
        "name": inbox_name,
        "channel_type": "whatsapp",
    });
    if inbox_id_param.is_some() {
        resp["message"] = json!("Inbox reauthorized successfully");
    }
    Ok(Json(resp))
}

/// POST /whatsapp/manual/preview
async fn manual_preview(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let waba_id = body["waba_id"].as_str().unwrap_or("");
    let phone_number_id = body["phone_number_id"].as_str().unwrap_or("");
    let access_token = body["access_token"].as_str().unwrap_or("");
    Ok(Json(
        validate_manual_setup(&st.db, waba_id, phone_number_id, access_token).await?,
    ))
}

/// POST /whatsapp/manual/connect
async fn manual_connect(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let waba_id = body["waba_id"].as_str().unwrap_or("");
    let phone_number_id = body["phone_number_id"].as_str().unwrap_or("");
    let access_token = body["access_token"].as_str().unwrap_or("");
    let app_secret = body["app_secret"].as_str().unwrap_or("");
    let preview = validate_manual_setup(&st.db, waba_id, phone_number_id, access_token).await?;
    let phone_number = preview["display_phone_number"].as_str().unwrap_or("");
    let inbox_name = body["inbox_name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            preview["suggested_inbox_name"]
                .as_str()
                .unwrap_or("WhatsApp")
        });
    let verify_token = uuid::Uuid::new_v4().simple().to_string();
    let cfg = json!({
        "api_key": access_token,
        "phone_number_id": preview["phone_number_id"],
        "business_account_id": preview["waba_id"],
        "app_secret": app_secret,
        "webhook_verify_token": verify_token,
        "source": "manual_setup_v2",
    });

    let mut tx = st.db.begin().await?;
    let ch_id: i64 = sqlx::query_scalar(
        "INSERT INTO channel_whatsapp (account_id, phone_number, provider, provider_config, created_at, updated_at)
         VALUES ($1, $2, 'whatsapp_cloud', $3, now(), now()) RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(phone_number)
    .bind(&cfg)
    .fetch_one(&mut *tx)
    .await?;
    let inbox_id: i64 = sqlx::query_scalar(
        "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
         VALUES ($1, $2, $3, 'Channel::Whatsapp', now(), now()) RETURNING id::bigint",
    )
    .bind(ch_id)
    .bind(account_id)
    .bind(inbox_name)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO inbox_members (user_id, inbox_id, created_at, updated_at) VALUES ($1, $2, now(), now())",
    )
    .bind(user.id)
    .bind(inbox_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    st.hub.invalidate_cache(account_id, "inbox");

    let callback_url = format!(
        "{}/webhooks/whatsapp/{phone_number}",
        st.frontend_url.trim_end_matches('/')
    );
    let webhook_err = subscribe_webhook_meta(
        preview["waba_id"].as_str().unwrap_or(waba_id),
        preview["phone_number_id"]
            .as_str()
            .unwrap_or(phone_number_id),
        access_token,
        &callback_url,
        &verify_token,
    )
    .await
    .err()
    .map(|e| format!("{e:?}"));

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": inbox_id,
            "name": inbox_name,
            "number_access": true,
            "template_access": true,
            "webhook_setup": webhook_err.is_none(),
            "webhook_error": webhook_err,
        })),
    ))
}

async fn load_manual_whatsapp_inbox(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
) -> Result<(String, Value), ApiError> {
    let row = sqlx::query(
        "SELECT w.phone_number, w.provider_config
         FROM inboxes i
         JOIN channel_whatsapp w ON w.id = i.channel_id AND w.account_id = i.account_id
         WHERE i.id::bigint = $1 AND i.account_id::bigint = $2 AND i.channel_type = 'Channel::Whatsapp'",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let phone: String = row.try_get("phone_number")?;
    let cfg: Value = row.try_get("provider_config")?;
    if cfg["source"].as_str() != Some("manual_setup_v2") {
        return Err(ApiError::not_found());
    }
    Ok((phone, cfg))
}

async fn fetch_manual_webhook_status(
    frontend_url: &str,
    phone: &str,
    cfg: &Value,
) -> Result<Value, ApiError> {
    let token = cfg["api_key"].as_str().unwrap_or("");
    let pid = cfg["phone_number_id"].as_str().unwrap_or("");
    let waba_id = cfg["business_account_id"].as_str().unwrap_or("");
    let callback_url = format!(
        "{}/webhooks/whatsapp/{phone}",
        frontend_url.trim_end_matches('/')
    );
    let phone_resp = meta_get(
        token,
        &format!("/{pid}"),
        &[("fields", "webhook_configuration")],
        "Phone number fetch failed",
    )
    .await?;
    let wh_cfg = &phone_resp["webhook_configuration"];
    let callback_configured = [
        "override_callback_uri",
        "phone_number",
        "whatsapp_business_account",
        "application",
    ]
    .iter()
    .any(|k| wh_cfg[*k].as_str() == Some(callback_url.as_str()));
    let subs = meta_get(
        token,
        &format!("/{waba_id}/subscribed_apps"),
        &[],
        "WABA webhook subscription fetch failed",
    )
    .await?;
    let subscription_verified = subs["data"].as_array().is_some_and(|arr| !arr.is_empty());
    Ok(json!({
        "callback_verified": callback_configured,
        "callback_configured": callback_configured,
        "callback_url": callback_url,
        "subscription_verified": subscription_verified,
    }))
}

/// GET /whatsapp/manual/{inbox_id}/webhook_status
async fn manual_webhook_status(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let (phone, cfg) = load_manual_whatsapp_inbox(&st.db, account_id, inbox_id).await?;
    Ok(Json(
        fetch_manual_webhook_status(&st.frontend_url, &phone, &cfg).await?,
    ))
}

/// POST /whatsapp/manual/{inbox_id}/setup_webhook
async fn manual_setup_webhook(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let (phone, cfg) = load_manual_whatsapp_inbox(&st.db, account_id, inbox_id).await?;
    let token = cfg["api_key"].as_str().unwrap_or("");
    let pid = cfg["phone_number_id"].as_str().unwrap_or("");
    let waba_id = cfg["business_account_id"].as_str().unwrap_or("");
    let verify_token = cfg["webhook_verify_token"].as_str().unwrap_or("");
    let callback_url = format!(
        "{}/webhooks/whatsapp/{phone}",
        st.frontend_url.trim_end_matches('/')
    );
    subscribe_webhook_meta(waba_id, pid, token, &callback_url, verify_token).await?;
    Ok(Json(
        fetch_manual_webhook_status(&st.frontend_url, &phone, &cfg).await?,
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn signatures_fail_closed() {
        assert!(super::verify(
            "key",
            "sha256=9307b3b915efb5171ff14d8cb55fbcc798c6c0ef1456d66ded1a6aa723a58b7b",
            b"hello"
        ));
        assert!(!super::verify("", "sha256=00", b"hello"));
        assert!(!super::verify("key", "sha256=00", b"hello"));
    }
}
