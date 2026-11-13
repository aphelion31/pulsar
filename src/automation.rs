use crate::common::{ApiError, ApiResult, AppState, CurrentUser, require_account};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde_json::{Value, json};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/webhooks", get(webhooks_index).post(webhooks_create))
        .route(
            "/webhooks/{id}",
            axum::routing::patch(webhooks_update).put(webhooks_update).delete(webhooks_delete),
        )
        .route("/macros", get(macros_index).post(macros_create))
        .route(
            "/macros/{id}",
            get(macros_show).patch(macros_update).put(macros_update).delete(macros_delete),
        )
        .route("/macros/{id}/execute", post(macros_execute))
        .route("/automation_rules", get(rules_index).post(rules_create))
        .route(
            "/automation_rules/{id}",
            get(rules_show).patch(rules_update).put(rules_update).delete(rules_delete),
        )
        .route("/automation_rules/{id}/clone", post(rules_clone))
}

const WEBHOOK: &str = "jsonb_build_object('id', w.id, 'name', w.name, 'url', w.url, 'account_id', w.account_id, 'subscriptions', w.subscriptions, 'secret', w.secret) || CASE WHEN i.id IS NULL THEN '{}'::jsonb ELSE jsonb_build_object('inbox', jsonb_build_object('id',i.id,'name',i.name)) END";
const MACRO: &str = "jsonb_build_object('id',m.id,'name',m.name,'visibility',CASE WHEN m.visibility=1 THEN 'global' ELSE 'personal' END,'account_id',m.account_id,'actions',m.actions,'created_by',CASE WHEN u.id IS NULL THEN NULL ELSE jsonb_build_object('id',u.id,'name',u.name,'email',u.email,'account_id',m.account_id) END,'updated_by',CASE WHEN v.id IS NULL THEN NULL ELSE jsonb_build_object('id',v.id,'name',v.name,'email',v.email,'account_id',m.account_id) END)";
const RULE: &str = "jsonb_build_object('id',r.id,'account_id',r.account_id,'name',r.name,'description',r.description,'event_name',r.event_name,'conditions',r.conditions,'actions',r.actions,'created_on',extract(epoch from r.created_at)::bigint,'active',r.active,'execution_delay',r.execution_delay)";
async fn webhook(db: &sqlx::PgPool, account: i64, id: i64) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar(&format!("SELECT {WEBHOOK} FROM webhooks w LEFT JOIN inboxes i ON i.id=w.inbox_id AND i.account_id=w.account_id WHERE w.account_id=$1 AND w.id=$2")).bind(account).bind(id).fetch_one(db).await.map(crate::ar_encryption::reveal_secret)?)
}
async fn macro_get(db: &sqlx::PgPool, account: i64, id: i64, user: i64) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar(&format!("SELECT {MACRO} FROM macros m LEFT JOIN users u ON u.id=m.created_by_id LEFT JOIN users v ON v.id=m.updated_by_id WHERE m.account_id=$1 AND m.id=$2 AND (m.visibility=1 OR m.created_by_id=$3)")).bind(account).bind(id).bind(user).fetch_one(db).await?)
}
async fn rule(db: &sqlx::PgPool, account: i64, id: i64) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar(&format!(
        "SELECT {RULE} FROM automation_rules r WHERE r.account_id=$1 AND r.id=$2"
    ))
    .bind(account)
    .bind(id)
    .fetch_one(db)
    .await?)
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::bad(format!("{key} is required")))
}
fn array(v: &Value, key: &str) -> Result<Value, ApiError> {
    let a = v.get(key).cloned().unwrap_or_else(|| json!([]));
    if !a.is_array() {
        return Err(ApiError::bad(format!("{key} must be an array")));
    }
    Ok(a)
}
fn validate_url(v: &str) -> Result<(), ApiError> {
    let url = url::Url::parse(v).map_err(|_| ApiError::bad("Invalid URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(ApiError::bad("URL must use http or https"));
    }
    Ok(())
}
async fn check_inbox(db: &sqlx::PgPool, account: i64, v: &Value) -> Result<(), ApiError> {
    if let Some(id) = v.get("inbox_id").filter(|v| !v.is_null()) {
        let id = id
            .as_i64()
            .ok_or_else(|| ApiError::bad("Invalid inbox_id"))?;
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inboxes WHERE account_id=$1 AND id=$2)",
        )
        .bind(account)
        .bind(id)
        .fetch_one(db)
        .await?;
        if !ok {
            return Err(ApiError::bad("Invalid inbox_id"));
        }
    }
    Ok(())
}
async fn webhooks_index(
    State(st): State<AppState>,
    Path(a): Path<i64>,
    u: CurrentUser,
) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    let rows: Vec<Value> = sqlx::query_scalar(&format!("SELECT {WEBHOOK} FROM webhooks w LEFT JOIN inboxes i ON i.id=w.inbox_id AND i.account_id=w.account_id WHERE w.account_id=$1 ORDER BY w.id")).bind(a).fetch_all(&st.db).await?;
    let rows: Vec<Value> = rows.into_iter().map(crate::ar_encryption::reveal_secret).collect();
    Ok(Json(json!({"payload":{"webhooks":rows}})))
}
async fn webhooks_create(
    State(st): State<AppState>,
    Path(a): Path<i64>,
    u: CurrentUser,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    let v = body
        .get("webhook")
        .ok_or_else(|| ApiError::bad("webhook is required"))?;
    let url = string(v, "url")?;
    validate_url(url)?;
    check_inbox(&st.db, a, v).await?;
    let subs = array(v, "subscriptions")?;
    let id: i64=sqlx::query_scalar("INSERT INTO webhooks(account_id,url,inbox_id,name,subscriptions,secret,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,now(),now()) RETURNING id").bind(a as i32).bind(url).bind(v.get("inbox_id").and_then(Value::as_i64).map(|x| x as i32)).bind(v.get("name").and_then(Value::as_str)).bind(subs).bind(crate::ar_encryption::new_secret().1).fetch_one(&st.db).await?;
    Ok(Json(
        json!({"payload":{"webhook":webhook(&st.db,a,id).await?}}),
    ))
}
async fn webhooks_update(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    let v = body
        .get("webhook")
        .ok_or_else(|| ApiError::bad("webhook is required"))?;
    let old = webhook(&st.db, a, id).await?;
    let url = v
        .get("url")
        .unwrap_or(&old["url"])
        .as_str()
        .ok_or_else(|| ApiError::bad("Invalid URL"))?;
    validate_url(url)?;
    check_inbox(&st.db, a, v).await?;
    let subs = if v.get("subscriptions").is_some() {
        array(v, "subscriptions")?
    } else {
        old["subscriptions"].clone()
    };
    let inbox = if v.get("inbox_id").is_some() {
        v["inbox_id"].as_i64()
    } else {
        old.pointer("/inbox/id").and_then(Value::as_i64)
    };
    let name = v.get("name").unwrap_or(&old["name"]).as_str();
    sqlx::query("UPDATE webhooks SET url=$3,inbox_id=$4,name=$5,subscriptions=$6,updated_at=now() WHERE account_id=$1 AND id=$2").bind(a as i32).bind(id).bind(url).bind(inbox.map(|x| x as i32)).bind(name).bind(subs).execute(&st.db).await?;
    Ok(Json(
        json!({"payload":{"webhook":webhook(&st.db,a,id).await?}}),
    ))
}
async fn webhooks_delete(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    let n = sqlx::query("DELETE FROM webhooks WHERE account_id=$1 AND id=$2")
        .bind(a as i32)
        .bind(id)
        .execute(&st.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({})))
}
async fn macros_index(State(st): State<AppState>, Path(a): Path<i64>, u: CurrentUser) -> ApiResult {
    require_account(&st.db, &u, a).await?;
    let rows:Vec<Value>=sqlx::query_scalar(&format!("SELECT {MACRO} FROM macros m LEFT JOIN users u ON u.id=m.created_by_id LEFT JOIN users v ON v.id=m.updated_by_id WHERE m.account_id=$1 AND (m.visibility=1 OR m.created_by_id=$2) ORDER BY m.id")).bind(a).bind(u.id).fetch_all(&st.db).await?;
    Ok(Json(json!({"payload":rows})))
}
async fn macros_show(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
) -> ApiResult {
    require_account(&st.db, &u, a).await?;
    Ok(Json(json!({"payload":macro_get(&st.db,a,id,u.id).await?})))
}
fn visibility(v: &Value, admin: bool) -> Result<i32, ApiError> {
    match v.as_str().unwrap_or("personal") {
        "personal" => Ok(0),
        "global" => Ok(if admin { 1 } else { 0 }),
        _ => Err(ApiError::bad("Invalid visibility")),
    }
}
fn actions(v: &Value) -> Result<Value, ApiError> {
    let list = array(v, "actions")?;
    for action in list.as_array().unwrap() {
        let name = string(action, "action_name")?;
        if !matches!(
            name,
            "assign_agent"
                | "assign_team"
                | "add_label"
                | "remove_label"
                | "change_status"
                | "change_priority"
                | "send_message"
                | "add_private_note"
                | "resolve_conversation"
                | "snooze_conversation"
                | "open_conversation"
                | "pending_conversation"
                | "mute_conversation"
                | "remove_assigned_agent"
                | "remove_assigned_team"
                | "send_webhook_event"
        ) {
            return Err(ApiError::bad("Unsupported macro action"));
        }
        if let Some(p) = action.get("action_params") {
            if !p.is_array() {
                return Err(ApiError::bad("action_params must be an array"));
            }
        }
    }
    Ok(list)
}
async fn macros_create(
    State(st): State<AppState>,
    Path(a): Path<i64>,
    u: CurrentUser,
    Json(v): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &u, a).await?;
    let name = string(&v, "name")?;
    let vis = visibility(&v["visibility"], au.is_admin())?;
    let acts = actions(&v)?;
    let id:i64=sqlx::query_scalar("INSERT INTO macros(account_id,name,visibility,created_by_id,updated_by_id,actions,created_at,updated_at) VALUES($1,$2,$3,$4,$4,$5,now(),now()) RETURNING id").bind(a).bind(name).bind(vis).bind(u.id).bind(acts).fetch_one(&st.db).await?;
    Ok(Json(json!({"payload":macro_get(&st.db,a,id,u.id).await?})))
}
async fn macros_update(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
    Json(v): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &u, a).await?;
    let old = macro_get(&st.db, a, id, u.id).await?;
    if old["visibility"] == "global" && !au.is_admin() {
        return Err(ApiError::forbidden());
    }
    let name = if v.get("name").is_some() {
        string(&v, "name")?
    } else {
        old["name"].as_str().unwrap()
    };
    let vis = visibility(
        v.get("visibility").unwrap_or(&old["visibility"]),
        au.is_admin(),
    )?;
    let acts = if v.get("actions").is_some() {
        actions(&v)?
    } else {
        old["actions"].clone()
    };
    sqlx::query("UPDATE macros SET name=$3,visibility=$4,actions=$5,updated_by_id=$6,updated_at=now() WHERE account_id=$1 AND id=$2").bind(a).bind(id).bind(name).bind(vis).bind(acts).bind(u.id).execute(&st.db).await?;
    Ok(Json(json!({"payload":macro_get(&st.db,a,id,u.id).await?})))
}
async fn macros_delete(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
) -> ApiResult {
    let au = require_account(&st.db, &u, a).await?;
    let old = macro_get(&st.db, a, id, u.id).await?;
    if old["visibility"] == "global" && !au.is_admin() {
        return Err(ApiError::forbidden());
    }
    sqlx::query("DELETE FROM macros WHERE account_id=$1 AND id=$2")
        .bind(a)
        .bind(id)
        .execute(&st.db)
        .await?;
    Ok(Json(json!({})))
}
// ponytail: rules stored, not executed; add event listener when needed
async fn rules_index(State(st): State<AppState>, Path(a): Path<i64>, u: CurrentUser) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    let rows: Vec<Value> = sqlx::query_scalar(&format!(
        "SELECT {RULE} FROM automation_rules r WHERE r.account_id=$1 ORDER BY r.id"
    ))
    .bind(a)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(json!({"payload":rows})))
}
async fn rules_show(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    Ok(Json(json!({"payload":rule(&st.db,a,id).await?})))
}
async fn rules_create(
    State(st): State<AppState>,
    Path(a): Path<i64>,
    u: CurrentUser,
    Json(v): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    if !v["execution_delay"].is_null() {
        return Err(ApiError::bad(
            "Delayed automations are not enabled for this account.",
        ));
    }
    let id:i64=sqlx::query_scalar("INSERT INTO automation_rules(account_id,name,description,event_name,conditions,actions,active,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,now(),now()) RETURNING id").bind(a).bind(string(&v,"name")?).bind(v["description"].as_str()).bind(string(&v,"event_name")?).bind(array(&v,"conditions")?).bind(array(&v,"actions")?).bind(v["active"].as_bool().unwrap_or(true)).fetch_one(&st.db).await?;
    Ok(Json(rule(&st.db, a, id).await?))
}
async fn rules_update(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
    Json(v): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    if !v["execution_delay"].is_null() {
        return Err(ApiError::bad(
            "Delayed automations are not enabled for this account.",
        ));
    }
    let old = rule(&st.db, a, id).await?;
    let name = if v.get("name").is_some() {
        string(&v, "name")?
    } else {
        old["name"].as_str().unwrap()
    };
    let event = if v.get("event_name").is_some() {
        string(&v, "event_name")?
    } else {
        old["event_name"].as_str().unwrap()
    };
    let conditions = if v.get("conditions").is_some() {
        array(&v, "conditions")?
    } else {
        old["conditions"].clone()
    };
    let acts = if v.get("actions").is_some() {
        array(&v, "actions")?
    } else {
        old["actions"].clone()
    };
    sqlx::query("UPDATE automation_rules SET name=$3,description=$4,event_name=$5,conditions=$6,actions=$7,active=$8,updated_at=now() WHERE account_id=$1 AND id=$2").bind(a).bind(id).bind(name).bind(v.get("description").unwrap_or(&old["description"]).as_str()).bind(event).bind(conditions).bind(acts).bind(v.get("active").unwrap_or(&old["active"]).as_bool().unwrap_or(true)).execute(&st.db).await?;
    Ok(Json(json!({"payload":rule(&st.db,a,id).await?})))
}
async fn rules_delete(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    let n = sqlx::query("DELETE FROM automation_rules WHERE account_id=$1 AND id=$2")
        .bind(a)
        .bind(id)
        .execute(&st.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({})))
}
async fn rules_clone(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
) -> ApiResult {
    require_account(&st.db, &u, a).await?.require_admin()?;
    let old = rule(&st.db, a, id).await?;
    if !old["execution_delay"].is_null() {
        return Err(ApiError::bad(
            "Delayed automations are not enabled for this account.",
        ));
    }
    let new_id:i64=sqlx::query_scalar("INSERT INTO automation_rules(account_id,name,description,event_name,conditions,actions,active,created_at,updated_at) SELECT account_id,name,description,event_name,conditions,actions,active,now(),now() FROM automation_rules WHERE account_id=$1 AND id=$2 RETURNING id").bind(a).bind(id).fetch_one(&st.db).await?;
    Ok(Json(json!({"payload":rule(&st.db,a,new_id).await?})))
}

async fn macros_execute(
    State(st): State<AppState>,
    Path((a, id)): Path<(i64, i64)>,
    u: CurrentUser,
    Json(v): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &u, a).await?;
    let mac = macro_get(&st.db, a, id, u.id).await?;
    let ids = v["conversation_ids"]
        .as_array()
        .ok_or_else(|| ApiError::bad("conversation_ids must be an array"))?;
    for display_id in ids {
        let display_id = display_id
            .as_i64()
            .ok_or_else(|| ApiError::bad("Invalid conversation id"))?;
        let conv: crate::conversations::Conv = sqlx::query_as(&format!(
            "SELECT {} FROM conversations c WHERE c.account_id=$1 AND c.display_id=$2",
            crate::conversations::CONV_COLS
        ))
        .bind(a)
        .bind(display_id)
        .fetch_one(&st.db)
        .await?;
        // agents may only act on conversations in inboxes they belong to (same rule as conversations.rs)
        if !au.is_admin() {
            let member: Option<i32> = sqlx::query_scalar("SELECT 1 FROM inbox_members WHERE inbox_id = $1 AND user_id = $2")
                .bind(conv.inbox_id)
                .bind(u.id)
                .fetch_optional(&st.db)
                .await?;
            member.ok_or_else(ApiError::not_found)?;
        }
        for action in mac["actions"]
            .as_array()
            .ok_or_else(|| ApiError::bad("Invalid actions"))?
        {
            let empty = vec![];
            let params = action["action_params"].as_array().unwrap_or(&empty);
            let first = params.first().unwrap_or(&Value::Null);
            let text = first.as_str().unwrap_or("");
            match action["action_name"].as_str().unwrap_or("") {
                "add_label" | "remove_label" => {
                    for label in params {
                        let label = label
                            .as_str()
                            .filter(|s| !s.trim().is_empty())
                            .ok_or_else(|| ApiError::bad("Invalid label"))?;
                        if action["action_name"] == "add_label" {
                            sqlx::query(
                                "INSERT INTO tags(name) VALUES($1) ON CONFLICT(name) DO NOTHING",
                            )
                            .bind(label)
                            .execute(&st.db)
                            .await?;
                            sqlx::query("INSERT INTO taggings(tag_id,taggable_type,taggable_id,context,created_at) SELECT t.id,'Conversation',c.id,'labels',now() FROM tags t JOIN conversations c ON c.account_id=$1 AND c.id=$2 WHERE t.name=$3 AND NOT EXISTS (SELECT 1 FROM taggings x WHERE x.tag_id=t.id AND x.taggable_type='Conversation' AND x.taggable_id=c.id AND x.context='labels')").bind(a).bind(conv.id).bind(label).execute(&st.db).await?;
                        } else {
                            sqlx::query("DELETE FROM taggings tg USING tags t, conversations c WHERE c.account_id=$1 AND c.id=$2 AND tg.taggable_id=c.id AND tg.taggable_type='Conversation' AND tg.context='labels' AND tg.tag_id=t.id AND t.name=$3").bind(a).bind(conv.id).bind(label).execute(&st.db).await?;
                        }
                    }
                    sqlx::query("UPDATE conversations c SET cached_label_list=(SELECT string_agg(t.name, ', ' ORDER BY t.name) FROM taggings tg JOIN tags t ON t.id=tg.tag_id WHERE tg.taggable_type='Conversation' AND tg.taggable_id=c.id AND tg.context='labels'),updated_at=now() WHERE c.account_id=$1 AND c.id=$2").bind(a).bind(conv.id).execute(&st.db).await?;
                }
                // mirrors chatwoot/app/services/action_service.rb + macros/execution_service.rb
                "mute_conversation" => {
                    if let Some(cid) = conv.contact_id {
                        sqlx::query("UPDATE contacts SET blocked=true,updated_at=now() WHERE id=$1").bind(cid).execute(&st.db).await?;
                    }
                    sqlx::query("UPDATE conversations SET status=1,status_changed_at=now(),updated_at=now() WHERE account_id=$1 AND id=$2").bind(a).bind(conv.id).execute(&st.db).await?;
                }
                "remove_assigned_agent" | "remove_assigned_team" => {
                    let col = if action["action_name"] == "remove_assigned_agent" { "assignee_id" } else { "team_id" };
                    sqlx::query(&format!("UPDATE conversations SET {col}=NULL,updated_at=now() WHERE account_id=$1 AND id=$2")).bind(a).bind(conv.id).execute(&st.db).await?;
                }
                "send_webhook_event" => {
                    if !text.is_empty() {
                        let mut payload = crate::conversations::conv_json(&st.db, &conv).await;
                        payload["event"] = json!("macro.executed");
                        let url = text.to_string();
                        tokio::spawn(async move { let _ = crate::listeners::automation::send_webhook(&url, &payload).await; });
                    }
                }
                "change_status" | "resolve_conversation" | "snooze_conversation" | "open_conversation" | "pending_conversation" => {
                    let status = match action["action_name"].as_str().unwrap() {
                        "resolve_conversation" => 1,
                        "snooze_conversation" => 3,
                        "open_conversation" => 0,
                        "pending_conversation" => 2,
                        _ => match text {
                            "open" => 0,
                            "resolved" => 1,
                            "pending" => 2,
                            "snoozed" => 3,
                            _ => return Err(ApiError::bad("Invalid status")),
                        },
                    };
                    sqlx::query("UPDATE conversations SET status=$3,status_changed_at=now(),updated_at=now() WHERE account_id=$1 AND id=$2").bind(a).bind(conv.id).bind(status).execute(&st.db).await?;
                }
                "change_priority" => {
                    let priority = match text {
                        "nil" | "" => None,
                        "low" => Some(0),
                        "medium" => Some(1),
                        "high" => Some(2),
                        "urgent" => Some(3),
                        _ => return Err(ApiError::bad("Invalid priority")),
                    };
                    sqlx::query("UPDATE conversations SET priority=$3,updated_at=now() WHERE account_id=$1 AND id=$2").bind(a).bind(conv.id).bind(priority).execute(&st.db).await?;
                }
                "assign_agent" | "assign_team" => {
                    let target = if first.is_null() || text == "nil" || text == "0" {
                        None
                    } else if text == "self" && action["action_name"] == "assign_agent" {
                        Some(u.id as i64)
                    } else {
                        Some(
                            first
                                .as_i64()
                                .or_else(|| text.parse().ok())
                                .ok_or_else(|| ApiError::bad("Invalid assignee"))?,
                        )
                    };
                    let q = if action["action_name"] == "assign_agent" {
                        "UPDATE conversations SET assignee_id=$3,updated_at=now() WHERE account_id=$1 AND id=$2 AND ($3::bigint IS NULL OR EXISTS (SELECT 1 FROM account_users WHERE account_id=$1 AND user_id=$3))"
                    } else {
                        "UPDATE conversations SET team_id=$3,updated_at=now() WHERE account_id=$1 AND id=$2 AND ($3::bigint IS NULL OR EXISTS (SELECT 1 FROM teams WHERE account_id=$1 AND id=$3))"
                    };
                    let n = sqlx::query(q)
                        .bind(a)
                        .bind(conv.id)
                        .bind(target)
                        .execute(&st.db)
                        .await?
                        .rows_affected();
                    if n == 0 {
                        return Err(ApiError::bad("Invalid assignee"));
                    }
                }
                "send_message" | "add_private_note" => {
                    if text.is_empty() {
                        return Err(ApiError::bad("Message is required"));
                    }
                    crate::conversations::insert_message(
                        &st,
                        &conv,
                        Some(("User", u.id)),
                        text,
                        1,
                        action["action_name"] == "add_private_note",
                        0,
                        json!({}),
                        None,
                    )
                    .await?;
                }
                _ => return Err(ApiError::bad("Unsupported macro action")),
            }
        }
        crate::conversations::emit_conv(&st, conv.id, "conversation.updated").await?;
    }
    Ok(Json(json!({})))
}
