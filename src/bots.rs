//! Account campaigns, agent bots and dashboard applications.
use crate::common::*;
use axum::{
    Json, Router,
    extract::{FromRequest, Multipart, Path, Request, State},
    routing::{get, post},
};
use rand::{Rng, distributions::Alphanumeric};
use serde_json::{Value, json};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/campaigns", get(campaigns).post(campaign_create))
        .route(
            "/campaigns/{id}",
            get(campaign_show)
                .patch(campaign_update).put(campaign_update)
                .delete(campaign_delete),
        )
        .route("/agent_bots", get(bots).post(bot_create))
        .route(
            "/agent_bots/{id}",
            get(bot_show).patch(bot_update).put(bot_update).delete(bot_delete),
        )
        .route("/agent_bots/{id}/avatar", axum::routing::delete(bot_avatar_delete))
        .route("/agent_bots/{id}/reset_access_token", post(bot_reset_token))
        .route("/agent_bots/{id}/reset_secret", post(bot_reset_secret))
        .route("/inboxes/{inbox_id}/agent_bot", get(inbox_bot))
        .route("/inboxes/{inbox_id}/set_agent_bot", post(set_inbox_bot))
        .route("/dashboard_apps", get(apps).post(app_create))
        .route(
            "/dashboard_apps/{id}",
            get(app_show).patch(app_update).put(app_update).delete(app_delete),
        )
}

// ponytail: campaigns are stored only, not scheduled or sent.
const CAMPAIGN_SELECT: &str = "SELECT jsonb_build_object('id',c.display_id,'title',c.title,'description',c.description,'account_id',c.account_id,'inbox',to_jsonb(i) - 'account_id','sender',CASE WHEN u.id IS NULL THEN '{}'::jsonb ELSE jsonb_build_object('id',u.id,'name',u.name,'email',u.email,'account_id',c.account_id) END,'message',c.message,'template_params',c.template_params,'campaign_status',(ARRAY['active','completed','processing'])[c.campaign_status+1],'enabled',c.enabled,'campaign_type',(ARRAY['ongoing','one_off'])[c.campaign_type+1],'trigger_rules',c.trigger_rules,'trigger_only_during_business_hours',c.trigger_only_during_business_hours,'created_at',c.created_at,'updated_at',c.updated_at) || CASE WHEN c.campaign_type=1 THEN jsonb_build_object('scheduled_at',COALESCE(EXTRACT(EPOCH FROM c.scheduled_at)::bigint,0),'started_at',EXTRACT(EPOCH FROM c.started_at)::bigint,'completed_at',EXTRACT(EPOCH FROM c.completed_at)::bigint,'audience',c.audience) ELSE '{}'::jsonb END FROM campaigns c JOIN inboxes i ON i.id=c.inbox_id AND i.account_id=c.account_id LEFT JOIN users u ON u.id=c.sender_id AND EXISTS (SELECT 1 FROM account_users au WHERE au.account_id=c.account_id AND au.user_id=u.id) WHERE c.account_id=$1";
const BOT_SELECT: &str = "SELECT jsonb_build_object('id',b.id,'name',b.name,'description',b.description,'thumbnail',NULL,'outgoing_url',b.outgoing_url,'bot_type',CASE b.bot_type WHEN 0 THEN 'webhook' WHEN 1 THEN 'dialogflow' ELSE 'webhook' END,'bot_config',b.bot_config,'account_id',b.account_id,'system_bot',false) || CASE WHEN $2 THEN jsonb_build_object('access_token',t.token,'secret',b.secret) ELSE '{}'::jsonb END FROM agent_bots b LEFT JOIN access_tokens t ON t.owner_type='AgentBot' AND t.owner_id=b.id WHERE b.account_id=$1";
const APP_SELECT: &str = "SELECT jsonb_build_object('id',id,'title',title,'content',content,'created_at',created_at) FROM dashboard_apps WHERE account_id=$1";
fn payload<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.get(key).filter(|x| x.is_object()).unwrap_or(v)
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|x| !x.trim().is_empty())
        .ok_or_else(|| ApiError::bad(format!("{key} is required")))
}
fn validate_app(v: &Value) -> Result<(), ApiError> {
    required(v, "title")?;
    if !v["content"].as_array().is_some_and(|a| {
        a.iter()
            .all(|x| x["type"] == "frame" && x["url"].as_str().is_some_and(|s| !s.is_empty()))
    }) {
        return Err(ApiError::bad("content must contain frame URLs"));
    }
    Ok(())
}
async fn inbox_exists(st: &AppState, account: i64, id: i64) -> Result<(), ApiError> {
    let ok: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inboxes WHERE account_id=$1 AND id=$2)")
            .bind(account)
            .bind(id)
            .fetch_one(&st.db)
            .await?;
    if ok {
        Ok(())
    } else {
        Err(ApiError::not_found())
    }
}
async fn campaign_validate(st: &AppState, account: i64, v: &Value) -> Result<(), ApiError> {
    required(v, "title")?;
    required(v, "message")?;
    let inbox = v["inbox_id"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("inbox_id is required"))?;
    inbox_exists(st, account, inbox).await?;
    if let Some(id) = v["sender_id"].as_i64() {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM account_users WHERE account_id=$1 AND user_id=$2)",
        )
        .bind(account)
        .bind(id)
        .fetch_one(&st.db)
        .await?;
        if !exists {
            return Err(ApiError::bad("sender must belong to account"));
        }
    }
    if !v["trigger_rules"].is_object() || !v["audience"].is_array() {
        return Err(ApiError::bad("invalid campaign rules or audience"));
    }
    if !matches!(v["campaign_type"].as_str(), Some("ongoing" | "one_off")) {
        return Err(ApiError::bad("invalid campaign_type"));
    }
    Ok(())
}
async fn campaign_get(st: &AppState, a: i64, id: i64) -> Result<Value, ApiError> {
    Ok(
        sqlx::query_scalar(&format!("{CAMPAIGN_SELECT} AND c.display_id=$2"))
            .bind(a)
            .bind(id)
            .fetch_one(&st.db)
            .await?,
    )
}
async fn campaigns(State(st): State<AppState>, user: CurrentUser, Path(a): Path<i64>) -> ApiResult {
    require_account(&st.db, &user, a).await?;
    let rows: Vec<Value> = sqlx::query_scalar(&format!("{CAMPAIGN_SELECT} ORDER BY c.id"))
        .bind(a)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(json!(rows)))
}
async fn campaign_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?;
    Ok(Json(campaign_get(&st, a, id).await?))
}
async fn campaign_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(a): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let mut v = json!({"enabled":true,"trigger_rules":{},"audience":[],"campaign_type":"ongoing","trigger_only_during_business_hours":false});
    merge(&mut v, payload(&body, "campaign"));
    normalize_type(&mut v)?;
    campaign_validate(&st, a, &v).await?;
    let id:i32=sqlx::query_scalar("INSERT INTO campaigns(account_id,inbox_id,sender_id,title,description,message,enabled,trigger_rules,audience,campaign_type,scheduled_at,trigger_only_during_business_hours,template_params,created_at,updated_at) VALUES($1,($2->>'inbox_id')::bigint,($2->>'sender_id')::integer,$2->>'title',$2->>'description',$2->>'message',($2->>'enabled')::boolean,$2->'trigger_rules',$2->'audience',CASE WHEN $2->>'campaign_type'='one_off' THEN 1 ELSE 0 END,CASE WHEN jsonb_typeof($2->'scheduled_at')='number' THEN to_timestamp(($2->>'scheduled_at')::double precision) AT TIME ZONE 'UTC' ELSE ($2->>'scheduled_at')::timestamp END,($2->>'trigger_only_during_business_hours')::boolean,$2->'template_params',now(),now()) RETURNING display_id").bind(a).bind(&v).fetch_one(&st.db).await?;
    Ok(Json(campaign_get(&st, a, id.into()).await?))
}
fn merge(dst: &mut Value, src: &Value) {
    if let (Some(d), Some(s)) = (dst.as_object_mut(), src.as_object()) {
        for (k, v) in s {
            d.insert(k.clone(), v.clone());
        }
    }
}
fn normalize_type(v: &mut Value) -> Result<(), ApiError> {
    if v["campaign_type"] == 0 {
        v["campaign_type"] = json!("ongoing")
    } else if v["campaign_type"] == 1 {
        v["campaign_type"] = json!("one_off")
    }
    Ok(())
}
async fn campaign_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let mut v:Value=sqlx::query_scalar("SELECT to_jsonb(c) || jsonb_build_object('campaign_type',CASE c.campaign_type WHEN 1 THEN 'one_off' ELSE 'ongoing' END) FROM campaigns c WHERE account_id=$1 AND display_id=$2").bind(a).bind(id).fetch_one(&st.db).await?;
    if v["campaign_type"] == "one_off" && v["campaign_status"] != 0 {
        return Err(ApiError::bad("The campaign has already started"));
    }
    merge(&mut v, payload(&body, "campaign"));
    normalize_type(&mut v)?;
    campaign_validate(&st, a, &v).await?;
    sqlx::query("UPDATE campaigns SET inbox_id=($3->>'inbox_id')::bigint,sender_id=($3->>'sender_id')::integer,title=$3->>'title',description=$3->>'description',message=$3->>'message',enabled=($3->>'enabled')::boolean,trigger_rules=$3->'trigger_rules',audience=$3->'audience',campaign_type=CASE WHEN $3->>'campaign_type'='one_off' THEN 1 ELSE 0 END,scheduled_at=CASE WHEN jsonb_typeof($3->'scheduled_at')='number' THEN to_timestamp(($3->>'scheduled_at')::double precision) AT TIME ZONE 'UTC' ELSE ($3->>'scheduled_at')::timestamp END,trigger_only_during_business_hours=($3->>'trigger_only_during_business_hours')::boolean,template_params=$3->'template_params',updated_at=now() WHERE account_id=$1 AND display_id=$2").bind(a).bind(id).bind(&v).execute(&st.db).await?;
    Ok(Json(campaign_get(&st, a, id).await?))
}
async fn campaign_delete(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let r = sqlx::query("DELETE FROM campaigns WHERE account_id=$1 AND display_id=$2")
        .bind(a)
        .bind(id)
        .execute(&st.db)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({})))
}

async fn bot_payload(req: Request, st: &AppState) -> Result<Value, ApiError> {
    let multipart = req
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.starts_with("multipart/form-data"));
    if !multipart {
        let Json(v) = Json::<Value>::from_request(req, st)
            .await
            .map_err(|_| ApiError::bad("invalid JSON"))?;
        return Ok(v);
    }
    let mut form = Multipart::from_request(req, st)
        .await
        .map_err(|_| ApiError::bad("invalid multipart"))?;
    let mut v = json!({});
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|_| ApiError::bad("invalid multipart"))?
    {
        let name = field.name().unwrap_or("").to_string();
        if field.file_name().is_some() {
            continue;
        }
        let text = field
            .text()
            .await
            .map_err(|_| ApiError::bad("invalid field"))?;
        if name == "bot_config" {
            v[&name] =
                serde_json::from_str(&text).map_err(|_| ApiError::bad("invalid bot_config"))?;
        } else if !name.is_empty() {
            v[&name] = json!(text);
        }
    }
    Ok(v)
}
async fn bot_get(st: &AppState, a: i64, id: i64, admin: bool) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar(&format!("{BOT_SELECT} AND b.id=$3"))
        .bind(a)
        .bind(admin)
        .bind(id)
        .fetch_one(&st.db)
        .await?)
}
async fn bots(State(st): State<AppState>, user: CurrentUser, Path(a): Path<i64>) -> ApiResult {
    let au = require_account(&st.db, &user, a).await?;
    let rows: Vec<Value> = sqlx::query_scalar(&format!("{BOT_SELECT} ORDER BY b.id"))
        .bind(a)
        .bind(au.is_admin())
        .fetch_all(&st.db)
        .await?;
    Ok(Json(json!(rows)))
}
async fn bot_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, a).await?;
    Ok(Json(bot_get(&st, a, id, au.is_admin()).await?))
}
fn bot_validate(v: &Value) -> Result<(), ApiError> {
    required(v, "name")?;
    if !v["bot_config"].is_object() {
        return Err(ApiError::bad("invalid bot_config"));
    }
    if !matches!(v["bot_type"].as_str(), Some("webhook")) {
        return Err(ApiError::bad("invalid bot_type"));
    }
    Ok(())
}
async fn bot_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(a): Path<i64>,
    req: Request,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let body = bot_payload(req, &st).await?;
    let mut v = json!({"bot_type":"webhook","bot_config":{}});
    merge(&mut v, payload(&body, "agent_bot"));
    bot_validate(&v)?;
    let mut tx = st.db.begin().await?;
    let id:i64=sqlx::query_scalar("INSERT INTO agent_bots(account_id,name,description,outgoing_url,bot_type,bot_config,created_at,updated_at) VALUES($1,$2->>'name',$2->>'description',$2->>'outgoing_url',CASE WHEN $2->>'bot_type'='dialogflow' THEN 1 ELSE 0 END,$2->'bot_config',now(),now()) RETURNING id").bind(a).bind(&v).fetch_one(&mut *tx).await?;
    let token: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();
    sqlx::query("INSERT INTO access_tokens(owner_type,owner_id,token,created_at,updated_at) VALUES('AgentBot',$1,$2,now(),now())").bind(id).bind(token).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(bot_get(&st, a, id, true).await?))
}
async fn bot_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
    req: Request,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let body = bot_payload(req, &st).await?;
    let mut v = bot_get(&st, a, id, true).await?;
    merge(&mut v, payload(&body, "agent_bot"));
    bot_validate(&v)?;
    sqlx::query("UPDATE agent_bots SET name=$3->>'name',description=$3->>'description',outgoing_url=$3->>'outgoing_url',bot_type=CASE WHEN $3->>'bot_type'='dialogflow' THEN 1 ELSE 0 END,bot_config=$3->'bot_config',updated_at=now() WHERE account_id=$1 AND id=$2").bind(a).bind(id).bind(&v).execute(&st.db).await?;
    Ok(Json(bot_get(&st, a, id, true).await?))
}
async fn bot_delete(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let mut tx = st.db.begin().await?;
    let r = sqlx::query("DELETE FROM agent_bots WHERE account_id=$1 AND id=$2")
        .bind(a)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    sqlx::query("DELETE FROM agent_bot_inboxes WHERE account_id=$1 AND agent_bot_id=$2")
        .bind(a)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM access_tokens WHERE owner_type='AgentBot' AND owner_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({})))
}
async fn inbox_bot(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, a).await?;
    inbox_exists(&st, a, id).await?;
    let bot:Option<i64>=sqlx::query_scalar("SELECT abi.agent_bot_id::bigint FROM agent_bot_inboxes abi JOIN agent_bots b ON b.id=abi.agent_bot_id AND b.account_id=$1 WHERE abi.inbox_id=$2 AND abi.account_id=$1 ORDER BY abi.id LIMIT 1").bind(a).bind(id).fetch_optional(&st.db).await?;
    Ok(Json(
        json!({"agent_bot":match bot {Some(b)=>Some(bot_get(&st,a,b,au.is_admin()).await?),None=>None}}),
    ))
}
async fn set_inbox_bot(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    inbox_exists(&st, a, id).await?;
    if !body
        .get("agent_bot")
        .is_some_and(|v| v.is_null() || v.as_i64().is_some())
    {
        return Err(ApiError::bad("agent_bot must be an id or null"));
    }
    let bot = body["agent_bot"].as_i64();
    if let Some(b) = bot {
        bot_get(&st, a, b, true).await?;
    }
    let mut tx = st.db.begin().await?;
    sqlx::query("SELECT id FROM inboxes WHERE account_id=$1 AND id=$2 FOR UPDATE")
        .bind(a)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM agent_bot_inboxes WHERE account_id=$1 AND inbox_id=$2")
        .bind(a)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if let Some(b) = bot {
        sqlx::query("INSERT INTO agent_bot_inboxes(account_id,inbox_id,agent_bot_id,created_at,updated_at) VALUES($1,$2,$3,now(),now())").bind(a).bind(id).bind(b).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({})))
}
async fn app_get(st: &AppState, a: i64, id: i64) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar(&format!("{APP_SELECT} AND id=$2"))
        .bind(a)
        .bind(id)
        .fetch_one(&st.db)
        .await?)
}
async fn apps(State(st): State<AppState>, user: CurrentUser, Path(a): Path<i64>) -> ApiResult {
    require_account(&st.db, &user, a).await?;
    let v: Vec<Value> = sqlx::query_scalar(&format!("{APP_SELECT} ORDER BY id"))
        .bind(a)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(json!(v)))
}
async fn app_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?;
    Ok(Json(app_get(&st, a, id).await?))
}
async fn app_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(a): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let mut v = json!({"content":[]});
    merge(&mut v, payload(&body, "dashboard_app"));
    validate_app(&v)?;
    let id:i64=sqlx::query_scalar("INSERT INTO dashboard_apps(account_id,user_id,title,content,created_at,updated_at) VALUES($1,$2,$3->>'title',$3->'content',now(),now()) RETURNING id").bind(a).bind(user.id).bind(&v).fetch_one(&st.db).await?;
    Ok(Json(app_get(&st, a, id).await?))
}
async fn app_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let mut v = app_get(&st, a, id).await?;
    merge(&mut v, payload(&body, "dashboard_app"));
    validate_app(&v)?;
    sqlx::query("UPDATE dashboard_apps SET title=$3->>'title',content=$3->'content',updated_at=now() WHERE account_id=$1 AND id=$2").bind(a).bind(id).bind(&v).execute(&st.db).await?;
    Ok(Json(app_get(&st, a, id).await?))
}
async fn app_delete(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let r = sqlx::query("DELETE FROM dashboard_apps WHERE account_id=$1 AND id=$2")
        .bind(a)
        .bind(id)
        .execute(&st.db)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bot_and_dashboard_payloads() {
        assert!(bot_validate(&json!({"name":"bot","bot_type":"webhook","bot_config":{}})).is_ok());
        assert!(bot_validate(&json!({"name":"bot","bot_type":"other","bot_config":{}})).is_err());
        assert!(
            validate_app(
                &json!({"title":"app","content":[{"type":"frame","url":"https://example.com"}]})
            )
            .is_ok()
        );
        assert!(
            validate_app(&json!({"title":"app","content":[{"type":"script","url":"x"}]})).is_err()
        );
    }
}

/// has_secure_token: 24 chars of base58 (Rails SecureRandom.base58).
fn secure_token() -> String {
    const B58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut r = rand::thread_rng();
    (0..24).map(|_| B58[r.gen_range(0..B58.len())] as char).collect()
}

/// DELETE /agent_bots/{id}/avatar (AgentBotsController#avatar)
async fn bot_avatar_delete(State(st): State<AppState>, user: CurrentUser, Path((a, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    bot_get(&st, a, id, true).await?;
    crate::storage::remove_avatar(&st.db, "AgentBot", id).await?;
    Ok(Json(bot_get(&st, a, id, true).await?))
}

/// POST /agent_bots/{id}/reset_access_token
async fn bot_reset_token(State(st): State<AppState>, user: CurrentUser, Path((a, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    bot_get(&st, a, id, true).await?;
    let token = secure_token();
    let n = sqlx::query("UPDATE access_tokens SET token=$2, updated_at=now() WHERE owner_type='AgentBot' AND owner_id=$1")
        .bind(id)
        .bind(&token)
        .execute(&st.db)
        .await?
        .rows_affected();
    if n == 0 {
        sqlx::query("INSERT INTO access_tokens(owner_type,owner_id,token,created_at,updated_at) VALUES('AgentBot',$1,$2,now(),now())")
            .bind(id)
            .bind(&token)
            .execute(&st.db)
            .await?;
    }
    Ok(Json(bot_get(&st, a, id, true).await?))
}

/// POST /agent_bots/{id}/reset_secret (WebhookSecretable#reset_secret!)
/// ponytail: stored plaintext like existing rows; encrypt with ar_encryption together with webhook secrets.
async fn bot_reset_secret(State(st): State<AppState>, user: CurrentUser, Path((a, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let n = sqlx::query("UPDATE agent_bots SET secret=$3, updated_at=now() WHERE account_id=$1 AND id=$2")
        .bind(a)
        .bind(id)
        .bind(secure_token())
        .execute(&st.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(bot_get(&st, a, id, true).await?))
}
