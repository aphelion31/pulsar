//! Conversations, messages, labels, assignments.
//! Port of Api::V1::Accounts::{Conversations,Conversations::Messages,Conversations::Labels,Conversations::Assignments}Controller.
use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::NaiveDateTime;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::auth::{availability_name, role_name};
use crate::common::*;

const STATUSES: [&str; 4] = ["open", "resolved", "pending", "snoozed"];
const PRIORITIES: [&str; 4] = ["low", "medium", "high", "urgent"];
const MSG_STATUSES: [&str; 4] = ["sent", "delivered", "read", "failed"];
const MSG_TYPES: [&str; 4] = ["incoming", "outgoing", "activity", "template"];
const CONTENT_TYPES: [&str; 13] = [
    "text", "input_text", "input_textarea", "input_email", "input_select", "cards", "form", "article",
    "incoming_email", "input_csat", "integrations", "sticker", "voice_call",
];
const CONTACT_TYPES: [&str; 3] = ["visitor", "lead", "customer"];
const PER_PAGE: i64 = 25;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/conversations", get(index).post(create))
        .route("/conversations/meta", get(meta))
        .route("/conversations/{id}", get(show))
        .route("/conversations/{id}/toggle_status", post(toggle_status))
        .route("/conversations/{id}/toggle_priority", post(toggle_priority))
        .route("/conversations/{id}/toggle_typing_status", post(toggle_typing))
        .route("/conversations/{id}/assignments", post(assign))
        .route("/conversations/{id}/update_last_seen", post(update_last_seen))
        .route("/conversations/{id}/unread", post(unread))
        .route("/conversations/{id}/custom_attributes", post(custom_attributes))
        .route("/conversations/{id}/labels", get(labels_index).post(labels_create))
        .route("/conversations/{id}/labels/add", post(labels_add))
        .route("/conversations/{id}/labels/remove", post(labels_remove))
        .route("/conversations/{id}/participants", get(empty_list))
        .route("/conversations/{id}/attachments", get(|| async { Json(json!({ "payload": [] })) }))
        .route(
            "/conversations/{id}/messages",
            get(messages_index)
                .post(messages_create_any)
                .layer(axum::extract::DefaultBodyLimit::max(crate::storage::MAX_BYTES + 1024 * 1024)),
        )
        .route("/conversations/{id}/messages/{message_id}", delete(messages_destroy))
}

fn idx(list: &[&str], s: &str) -> Option<i32> {
    list.iter().position(|x| *x == s).map(|i| i as i32)
}
fn name_of(list: &[&'static str], i: i32) -> &'static str {
    list.get(i as usize).copied().unwrap_or(list[0])
}
fn num(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

// ---------- rows ----------

#[derive(sqlx::FromRow, Clone)]
pub(crate) struct Conv {
    pub(crate) id: i64,
    pub(crate) account_id: i64,
    pub(crate) inbox_id: i64,
    pub(crate) status: i32,
    pub(crate) assignee_id: Option<i64>,
    pub(crate) created_at: NaiveDateTime,
    pub(crate) updated_at: NaiveDateTime,
    pub(crate) contact_id: Option<i64>,
    pub(crate) display_id: i64,
    pub(crate) contact_last_seen_at: Option<NaiveDateTime>,
    pub(crate) agent_last_seen_at: Option<NaiveDateTime>,
    pub(crate) additional_attributes: Option<Value>,
    pub(crate) contact_inbox_id: Option<i64>,
    pub(crate) uuid: String,
    pub(crate) last_activity_at: NaiveDateTime,
    pub(crate) team_id: Option<i64>,
    pub(crate) snoozed_until: Option<NaiveDateTime>,
    pub(crate) custom_attributes: Option<Value>,
    pub(crate) assignee_last_seen_at: Option<NaiveDateTime>,
    pub(crate) first_reply_created_at: Option<NaiveDateTime>,
    pub(crate) priority: Option<i32>,
    pub(crate) waiting_since: Option<NaiveDateTime>,
    pub(crate) cached_label_list: Option<String>,
}

pub(crate) const CONV_COLS: &str = "c.id::bigint AS id, c.account_id::bigint AS account_id, c.inbox_id::bigint AS inbox_id, c.status,
  c.assignee_id::bigint AS assignee_id, c.created_at, c.updated_at, c.contact_id, c.display_id::bigint AS display_id,
  c.contact_last_seen_at, c.agent_last_seen_at, c.additional_attributes, c.contact_inbox_id, c.uuid::text AS uuid,
  c.last_activity_at, c.team_id, c.snoozed_until, c.custom_attributes, c.assignee_last_seen_at, c.first_reply_created_at,
  c.priority, c.waiting_since, c.cached_label_list";

#[derive(sqlx::FromRow)]
pub(crate) struct Msg {
    pub(crate) id: i64,
    pub(crate) content: Option<String>,
    pub(crate) account_id: i64,
    pub(crate) inbox_id: i64,
    pub(crate) conversation_id: i64,
    pub(crate) message_type: i32,
    pub(crate) created_at: NaiveDateTime,
    pub(crate) updated_at: NaiveDateTime,
    pub(crate) private: bool,
    pub(crate) status: Option<i32>,
    pub(crate) source_id: Option<String>,
    pub(crate) content_type: i32,
    pub(crate) content_attributes: Option<Value>,
    pub(crate) sender_type: Option<String>,
    pub(crate) sender_id: Option<i64>,
    pub(crate) external_source_ids: Option<Value>,
    pub(crate) additional_attributes: Option<Value>,
    pub(crate) processed_message_content: Option<String>,
    pub(crate) sentiment: Option<Value>,
}

pub(crate) const MSG_COLS: &str = "m.id::bigint AS id, m.content, m.account_id::bigint AS account_id, m.inbox_id::bigint AS inbox_id,
  m.conversation_id::bigint AS conversation_id, m.message_type, m.created_at, m.updated_at, m.private, m.status, m.source_id,
  m.content_type, m.content_attributes::jsonb AS content_attributes, m.sender_type, m.sender_id, m.external_source_ids,
  m.additional_attributes, m.processed_message_content, m.sentiment";

impl Conv {
    fn labels(&self) -> Vec<String> {
        self.cached_label_list
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

async fn load(db: &PgPool, au: &AccountUser, display_id: i64) -> Result<Conv, ApiError> {
    let c: Conv = sqlx::query_as(&format!("SELECT {CONV_COLS} FROM conversations c WHERE c.account_id = $1 AND c.display_id = $2"))
        .bind(au.account_id)
        .bind(display_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if !au.is_admin() && !is_inbox_member(db, c.inbox_id, au.user_id).await? {
        return Err(ApiError::not_found());
    }
    if au.custom_perms.is_some() && !au.is_admin() {
        let ok: bool = sqlx::query_scalar(&format!("SELECT {} FROM conversations c WHERE c.id = $1", au.custom_scope_sql("$2")))
            .bind(c.id)
            .bind(au.user_id)
            .fetch_one(db)
            .await?;
        if !ok {
            return Err(ApiError::not_found());
        }
    }
    Ok(c)
}

pub(crate) async fn reload(db: &PgPool, id: i64) -> Result<Conv, ApiError> {
    Ok(sqlx::query_as(&format!("SELECT {CONV_COLS} FROM conversations c WHERE c.id = $1")).bind(id).fetch_one(db).await?)
}

async fn is_inbox_member(db: &PgPool, inbox_id: i64, user_id: i64) -> Result<bool, ApiError> {
    let r: Option<i32> = sqlx::query_scalar("SELECT 1 FROM inbox_members WHERE inbox_id = $1 AND user_id = $2")
        .bind(inbox_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(r.is_some())
}

// ---------- serializers ----------

/// contact push_event_data + _contact.json.jbuilder fields (superset).
pub(crate) async fn contact_json(db: &PgPool, id: Option<i64>) -> Value {
    let Some(id) = id else { return json!({}) };
    type Row = (i64, Option<String>, Option<String>, Option<String>, Option<String>, Option<Value>, Option<Value>, bool, Option<i32>, Option<NaiveDateTime>, NaiveDateTime);
    let r: Option<Row> = sqlx::query_as(
        "SELECT id::bigint, name, email, phone_number, identifier, additional_attributes, custom_attributes, blocked, contact_type,
                last_activity_at, created_at FROM contacts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let Some(r) = r else { return json!({}) };
    json!({
        "id": r.0, "name": r.1, "email": r.2, "phone_number": r.3, "identifier": r.4,
        "additional_attributes": r.5.unwrap_or(json!({})), "custom_attributes": r.6.unwrap_or(json!({})),
        "blocked": r.7, "contact_type": name_of(&CONTACT_TYPES, r.8.unwrap_or(0)), "type": "contact",
        "thumbnail": crate::storage::avatar_url(db, "Contact", r.0).await, "availability_status": "offline",
        "last_activity_at": r.9.map(|t| t.and_utc().timestamp()), "created_at": r.10.and_utc().timestamp(),
    })
}

/// user push_event_data + _agent.json.jbuilder fields (superset).
async fn user_json(db: &PgPool, account_id: i64, uid: i64) -> Value {
    type Row = (i64, String, Option<String>, Option<String>, Option<i32>, Option<bool>, Option<i32>, Option<NaiveDateTime>);
    let r: Option<Row> = sqlx::query_as(
        "SELECT u.id::bigint, u.name, u.display_name, u.email, au.availability, au.auto_offline, au.role, u.confirmed_at
         FROM users u LEFT JOIN account_users au ON au.user_id = u.id AND au.account_id = $2 WHERE u.id = $1",
    )
    .bind(uid)
    .bind(account_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let Some(r) = r else { return Value::Null };
    let online = crate::cable::presence_snapshot().is_online(account_id, uid);
    json!({
        "id": r.0, "name": r.1, "available_name": r.2.clone().filter(|s| !s.is_empty()).unwrap_or(r.1.clone()),
        "email": r.3, "avatar_url": "", "thumbnail": "", "type": "user",
        "availability_status": if online { availability_name(r.4.unwrap_or(0)) } else { "offline" },
        "auto_offline": r.5.unwrap_or(true), "role": role_name(r.6.unwrap_or(0)), "confirmed": r.7.is_some(),
        "provider": "email", "account_id": account_id,
    })
}

async fn team_json(db: &PgPool, team_id: Option<i64>) -> Value {
    let Some(t) = team_id else { return Value::Null };
    let r: Option<(i64, String, Option<String>, Option<bool>, i64)> =
        sqlx::query_as("SELECT id, name, description, allow_auto_assign, account_id FROM teams WHERE id = $1")
            .bind(t)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    r.map(|r| json!({ "id": r.0, "name": r.1, "description": r.2, "allow_auto_assign": r.3, "account_id": r.4 }))
        .unwrap_or(Value::Null)
}

async fn unread_count(db: &PgPool, c: &Conv) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM (SELECT 1 FROM messages WHERE conversation_id = $1 AND account_id = $2 AND message_type = 0
           AND ($3::timestamp IS NULL OR created_at > $3) LIMIT 10) s",
    )
    .bind(c.id)
    .bind(c.account_id)
    .bind(c.agent_last_seen_at)
    .fetch_one(db)
    .await
    .unwrap_or(0)
}

async fn source_id(db: &PgPool, c: &Conv) -> (Option<String>, Option<bool>) {
    let Some(ci) = c.contact_inbox_id else { return (None, None) };
    sqlx::query_as("SELECT source_id, hmac_verified FROM contact_inboxes WHERE id = $1")
        .bind(ci)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .map(|(s, h): (String, Option<bool>)| (Some(s), h))
        .unwrap_or((None, None))
}

/// Message#push_event_data (also used for the REST message payload, superset of _message.json.jbuilder).
pub(crate) async fn msg_json(db: &PgPool, m: &Msg, c: &Conv) -> Value {
    let sender = match (m.sender_type.as_deref(), m.sender_id) {
        (Some("User"), Some(id)) => user_json(db, m.account_id, id).await,
        (Some("Contact"), Some(id)) => contact_json(db, Some(id)).await,
        _ => Value::Null,
    };
    let (sid, _) = source_id(db, c).await;
    let mut v = json!({
        "id": m.id, "content": m.content, "account_id": m.account_id, "inbox_id": m.inbox_id,
        "conversation_id": c.display_id, "message_type": m.message_type,
        "content_type": name_of(&CONTENT_TYPES, m.content_type),
        "status": name_of(&MSG_STATUSES, m.status.unwrap_or(0)),
        "content_attributes": m.content_attributes.clone().unwrap_or(json!({})),
        "created_at": m.created_at.and_utc().timestamp(), "updated_at": m.updated_at,
        "private": m.private, "source_id": m.source_id, "sender_type": m.sender_type, "sender_id": m.sender_id,
        "external_source_ids": m.external_source_ids.clone().unwrap_or(json!({})),
        "additional_attributes": m.additional_attributes.clone().unwrap_or(json!({})),
        "processed_message_content": m.processed_message_content, "sentiment": m.sentiment.clone().unwrap_or(json!({})),
        "conversation": {
            "assignee_id": c.assignee_id, "unread_count": unread_count(db, c).await,
            "last_activity_at": c.last_activity_at.and_utc().timestamp(), "contact_inbox": { "source_id": sid },
        },
    });
    if !sender.is_null() {
        v["sender"] = sender;
    }
    let atts = crate::storage::attachments_json(db, m.id).await;
    if !atts.is_empty() {
        v["attachments"] = json!(atts);
    }
    v
}

async fn last_message(db: &PgPool, c: &Conv, non_activity: bool) -> Value {
    let filter = if non_activity { "AND m.message_type <> 2" } else { "" };
    let m: Option<Msg> = sqlx::query_as(&format!(
        "SELECT {MSG_COLS} FROM messages m WHERE m.conversation_id = $1 AND m.account_id = $2 {filter} ORDER BY m.created_at DESC, m.id DESC LIMIT 1"
    ))
    .bind(c.id)
    .bind(c.account_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    match m {
        Some(m) => msg_json(db, &m, c).await,
        None => Value::Null,
    }
}

/// conversations/partials/_conversation.json.jbuilder + EventDataPresenter#push_data.
pub(crate) async fn conv_json(db: &PgPool, c: &Conv) -> Value {
    let channel: Option<String> = sqlx::query_scalar("SELECT channel_type FROM inboxes WHERE id = $1")
        .bind(c.inbox_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten();
    let (sid, hmac) = source_id(db, c).await;
    let mut meta = json!({ "sender": contact_json(db, c.contact_id).await, "channel": channel, "hmac_verified": hmac });
    if let Some(a) = c.assignee_id {
        meta["assignee"] = user_json(db, c.account_id, a).await;
        meta["assignee_type"] = json!("User");
    }
    let team = team_json(db, c.team_id).await;
    if !team.is_null() {
        meta["team"] = team;
    }
    let last = last_message(db, c, false).await;
    let ts = |t: Option<NaiveDateTime>| t.map(|t| t.and_utc().timestamp()).unwrap_or(0);
    json!({
        "meta": meta, "id": c.display_id, "messages": if last.is_null() { json!([]) } else { json!([last]) },
        "account_id": c.account_id, "uuid": c.uuid, "inbox_id": c.inbox_id, "channel": channel,
        "additional_attributes": c.additional_attributes.clone().unwrap_or(json!({})),
        "custom_attributes": c.custom_attributes.clone().unwrap_or(json!({})),
        "agent_last_seen_at": ts(c.agent_last_seen_at), "assignee_last_seen_at": ts(c.assignee_last_seen_at),
        "contact_last_seen_at": ts(c.contact_last_seen_at), "can_reply": true, "anonymous_caller": false,
        "contact_inbox": { "source_id": sid },
        "labels": c.labels(), "muted": false, "snoozed_until": c.snoozed_until,
        "status": name_of(&STATUSES, c.status), "created_at": c.created_at.and_utc().timestamp(),
        "updated_at": c.updated_at.and_utc().timestamp_millis() as f64 / 1000.0,
        "timestamp": c.last_activity_at.and_utc().timestamp(), "last_activity_at": c.last_activity_at.and_utc().timestamp(),
        "first_reply_created_at": ts(c.first_reply_created_at), "unread_count": unread_count(db, c).await,
        "last_non_activity_message": last_message(db, c, true).await,
        "priority": c.priority.map(|p| name_of(&PRIORITIES, p)), "waiting_since": ts(c.waiting_since),
        "sla_policy_id": null,
    })
}

pub(crate) async fn emit(st: &AppState, c: &Conv, event: &str, data: Value) {
    crate::events::publish(event, c.account_id, data.clone(), None);
    st.hub.to_inbox(&st.db, c.account_id, c.inbox_id, c.contact_inbox_id, event, data).await;
}

pub(crate) async fn emit_conv(st: &AppState, id: i64, event: &str) -> Result<Conv, ApiError> {
    let c = reload(&st.db, id).await?;
    let data = conv_json(&st.db, &c).await;
    emit(st, &c, event, data).await;
    Ok(c)
}

/// Internal carrier for messages.source_id through insert_message; never stored in content_attributes.
const SOURCE_ID_KEY: &str = "__pulsar_source_id";

/// Insert a message, bump conversation activity, broadcast message.created. Returns the push payload.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_message(
    st: &AppState, c: &Conv, sender: Option<(&str, i64)>, content: &str, message_type: i32, private: bool,
    content_type: i32, mut content_attributes: Value, echo_id: Option<&str>,
) -> Result<Value, ApiError> {
    let source_id = content_attributes.as_object_mut().and_then(|o| o.remove(SOURCE_ID_KEY)).and_then(|v| v.as_str().map(str::to_string));
    // chatwoot#15743: a client retrying a send with the same echo_id gets the original message back
    // instead of a duplicate. The echo_id is kept in additional_attributes; the advisory lock makes
    // concurrent retries safe without a schema change.
    let mut tx = st.db.begin().await?;
    if let Some(e) = echo_id.filter(|e| !e.is_empty()) {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('pulsar_echo'), hashtext($1 || ':' || $2::text))")
            .bind(e)
            .bind(c.id)
            .execute(&mut *tx)
            .await?;
        let prior: Option<Msg> = sqlx::query_as(&format!(
            "SELECT {MSG_COLS} FROM messages m
             WHERE m.conversation_id = $1 AND m.account_id = $2 AND m.additional_attributes ->> 'echo_id' = $3
             ORDER BY m.id LIMIT 1"
        ))
        .bind(c.id as i32)
        .bind(c.account_id as i32)
        .bind(e)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(m) = prior {
            tx.commit().await?;
            let mut data = msg_json(&st.db, &m, c).await;
            data["echo_id"] = json!(e);
            return Ok(data);
        }
    }
    let m: Msg = sqlx::query_as(&format!(
        "INSERT INTO messages AS m (content, account_id, inbox_id, conversation_id, message_type, private, status, content_type,
            content_attributes, sender_type, sender_id, additional_attributes, source_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, 0, $7, $8::json, $9, $10,
                 CASE WHEN $11::text IS NULL THEN '{{}}'::jsonb ELSE jsonb_build_object('echo_id', $11::text) END,
                 $12, now(), now()) RETURNING {MSG_COLS}"
    ))
    .bind(content)
    .bind(c.account_id as i32)
    .bind(c.inbox_id as i32)
    .bind(c.id as i32)
    .bind(message_type)
    .bind(private)
    .bind(content_type)
    .bind(content_attributes)
    .bind(sender.map(|s| s.0))
    .bind(sender.map(|s| s.1))
    .bind(echo_id.filter(|e| !e.is_empty()))
    .bind(source_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    if message_type != 2 {
        // outgoing public reply: first reply + clear waiting; incoming: start waiting
        sqlx::query(
            "UPDATE conversations SET last_activity_at = now(), updated_at = now(),
               first_reply_created_at = CASE WHEN $2 = 1 AND NOT $3 THEN COALESCE(first_reply_created_at, now()) ELSE first_reply_created_at END,
               waiting_since = CASE WHEN $2 = 1 AND NOT $3 THEN NULL WHEN $2 = 0 THEN COALESCE(waiting_since, now()) ELSE waiting_since END
             WHERE id = $1",
        )
        .bind(c.id)
        .bind(message_type)
        .bind(private)
        .execute(&st.db)
        .await?;
    }
    let c = reload(&st.db, c.id).await?;
    let mut data = msg_json(&st.db, &m, &c).await;
    if let Some(e) = echo_id {
        data["echo_id"] = json!(e);
    }
    emit(st, &c, "message.created", data.clone()).await;
    Ok(data)
}

async fn activity(st: &AppState, c: &Conv, content: String) -> Result<(), ApiError> {
    insert_message(st, c, None, &content, 2, false, 0, json!({}), None).await.map(|_| ())
}

// ---------- conversations ----------

struct Filter {
    status: String,
    status_i: i32,
    inbox_id: Option<i64>,
    team_id: Option<i64>,
    labels: Option<Vec<String>>,
    q: Option<String>,
    conv_type: Option<String>,
    assignee: &'static str,
    page: i64,
    sort: &'static str,
}

fn parse_filter(q: &[(String, String)]) -> Filter {
    let get = |k: &str| q.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone()).filter(|v| !v.is_empty());
    let labels: Vec<String> = q.iter().filter(|(k, _)| k == "labels[]" || k == "labels").map(|(_, v)| v.clone()).collect();
    let status = get("status").unwrap_or_else(|| "open".into());
    Filter {
        status_i: idx(&STATUSES, &status).unwrap_or(-1),
        status,
        inbox_id: get("inbox_id").and_then(|v| v.parse().ok()),
        team_id: get("team_id").and_then(|v| v.parse().ok()),
        labels: if labels.is_empty() { None } else { Some(labels) },
        q: get("q").map(|s| format!("%{s}%")),
        conv_type: get("conversation_type").filter(|t| ["mention", "participating", "unattended"].contains(&t.as_str())),
        assignee: match get("assignee_type").as_deref() {
            Some("me") => "c.assignee_id = $8",
            Some("unassigned") => "c.assignee_id IS NULL",
            Some("assigned") => "c.assignee_id IS NOT NULL",
            _ => "TRUE",
        },
        page: get("page").and_then(|v| v.parse().ok()).unwrap_or(1i64).max(1),
        sort: match get("sort_by").as_deref() {
            Some("last_activity_at_asc") => "c.last_activity_at ASC, c.id ASC",
            Some("created_at_desc") => "c.created_at DESC, c.id DESC",
            Some("created_at_asc") => "c.created_at ASC, c.id ASC",
            Some("priority_desc") => "c.priority DESC NULLS LAST, c.last_activity_at DESC",
            Some("priority_asc") => "c.priority ASC NULLS FIRST, c.last_activity_at DESC",
            Some("waiting_since_desc") => "c.waiting_since DESC NULLS LAST, c.id DESC",
            Some("waiting_since_asc") => "c.waiting_since ASC NULLS LAST, c.id ASC",
            _ => "c.last_activity_at DESC, c.id DESC",
        },
    }
}

/// Shared WHERE for list + counts. $1 account, $2 status name, $3 status int, $4 inbox, $5 team, $6 labels,
/// $7 q pattern, $8 user id, $9 is_admin, $10 conversation_type.
const BASE: &str = "FROM conversations c WHERE c.account_id = $1
  AND ($2::text = 'all' OR c.status = $3)
  AND ($4::bigint IS NULL OR c.inbox_id = $4)
  AND ($5::bigint IS NULL OR c.team_id = $5)
  AND ($6::text[] IS NULL OR c.id IN (SELECT tg.taggable_id FROM taggings tg JOIN tags t ON t.id = tg.tag_id
       WHERE tg.taggable_type = 'Conversation' AND tg.context = 'labels' AND t.name = ANY($6)))
  AND ($7::text IS NULL OR c.id IN (SELECT m.conversation_id FROM messages m
       WHERE m.account_id = $1 AND m.message_type IN (0, 1) AND m.content ILIKE $7))
  AND ($9 OR c.inbox_id IN (SELECT inbox_id FROM inbox_members WHERE user_id = $8))
  AND ($10::text IS NULL
       OR ($10 = 'mention' AND c.id IN (SELECT conversation_id FROM mentions WHERE user_id = $8 AND account_id = $1))
       OR ($10 = 'participating' AND c.id IN (SELECT conversation_id FROM conversation_participants WHERE user_id = $8 AND account_id = $1))
       OR ($10 = 'unattended' AND (c.first_reply_created_at IS NULL OR c.waiting_since IS NOT NULL)))";

macro_rules! bind_base {
    ($q:expr, $f:expr, $au:expr) => {
        $q.bind($au.account_id)
            .bind(&$f.status)
            .bind($f.status_i)
            .bind($f.inbox_id)
            .bind($f.team_id)
            .bind(&$f.labels)
            .bind(&$f.q)
            .bind($au.user_id)
            .bind($au.is_admin())
            .bind(&$f.conv_type)
    };
}

async fn counts(db: &PgPool, f: &Filter, au: &AccountUser) -> Result<Value, ApiError> {
    let sql = format!(
        "SELECT COUNT(*) FILTER (WHERE c.assignee_id = $8), COUNT(*) FILTER (WHERE c.assignee_id IS NULL), COUNT(*) {BASE} AND {}",
        au.custom_scope_sql("$8")
    );
    let (mine, unassigned, all): (i64, i64, i64) = bind_base!(sqlx::query_as(&sql), f, au).fetch_one(db).await?;
    Ok(json!({ "mine_count": mine, "unassigned_count": unassigned, "assigned_count": all - unassigned, "all_count": all }))
}

async fn index(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Query(q): Query<Vec<(String, String)>>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let f = parse_filter(&q);
    let meta = counts(&st.db, &f, &au).await?;
    let sql = format!(
        "SELECT {CONV_COLS} {BASE} AND {} AND {} ORDER BY {} LIMIT {PER_PAGE} OFFSET {}",
        au.custom_scope_sql("$8"),
        f.assignee,
        f.sort,
        (f.page - 1) * PER_PAGE
    );
    let rows: Vec<Conv> = bind_base!(sqlx::query_as(&sql), f, au).fetch_all(&st.db).await?;
    // Chatwoot PR #15719 (N+1 on the list): each row needs ~8 small lookups. Run rows concurrently, capped
    // below the pool size so one list request cannot starve the rest.
    // ponytail: still 8 queries per row; batch them into set-based queries if lists grow past 25 rows.
    let mut payload = Vec::with_capacity(rows.len());
    for chunk in rows.chunks(4) {
        let mut futs = Vec::with_capacity(chunk.len());
        for c in chunk {
            futs.push(conv_json(&st.db, c));
        }
        payload.extend(futures::future::join_all(futs).await);
    }
    Ok(Json(json!({ "data": { "meta": meta, "payload": payload } })))
}

async fn meta(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Query(q): Query<Vec<(String, String)>>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let f = parse_filter(&q);
    Ok(Json(json!({ "meta": counts(&st.db, &f, &au).await? })))
}

async fn show(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    Ok(Json(conv_json(&st.db, &c).await))
}

async fn empty_list() -> Json<Value> {
    Json(json!([]))
}

/// POST /conversations {inbox_id, contact_id, source_id?, status?, assignee_id?, team_id?, message:{content}}
async fn create(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Json(b): Json<Value>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let inbox_id = num(&b["inbox_id"]).ok_or_else(|| ApiError::bad("inbox_id is required"))?;
    let ok: Option<i32> = sqlx::query_scalar("SELECT 1 FROM inboxes WHERE id = $1 AND account_id = $2")
        .bind(inbox_id)
        .bind(account_id)
        .fetch_optional(&st.db)
        .await?;
    if ok.is_none() || (!au.is_admin() && !is_inbox_member(&st.db, inbox_id, user.id).await?) {
        return Err(ApiError::not_found());
    }
    let contact_id = num(&b["contact_id"]).ok_or_else(|| ApiError::bad("contact_id is required"))?;
    let ok: Option<i32> = sqlx::query_scalar("SELECT 1 FROM contacts WHERE id = $1 AND account_id = $2")
        .bind(contact_id)
        .bind(account_id)
        .fetch_optional(&st.db)
        .await?;
    ok.ok_or_else(ApiError::not_found)?;

    let ci: Option<i64> = sqlx::query_scalar("SELECT id FROM contact_inboxes WHERE contact_id = $1 AND inbox_id = $2 ORDER BY id LIMIT 1")
        .bind(contact_id)
        .bind(inbox_id)
        .fetch_optional(&st.db)
        .await?;
    let ci = match ci {
        Some(ci) => ci,
        None => {
            let source = b["source_id"].as_str().map(str::to_string).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            sqlx::query_scalar(
                "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, pubsub_token, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, now(), now()) RETURNING id",
            )
            .bind(contact_id)
            .bind(inbox_id)
            .bind(source)
            .bind(uuid::Uuid::new_v4().simple().to_string())
            .fetch_one(&st.db)
            .await?
        }
    };
    let status = b["status"].as_str().and_then(|s| idx(&STATUSES, s)).unwrap_or(0);
    let assignee = num(&b["assignee_id"]);
    if let Some(a) = assignee {
        require_member(&st.db, account_id, a).await?;
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (account_id, inbox_id, status, assignee_id, contact_id, contact_inbox_id, team_id,
            additional_attributes, custom_attributes, last_activity_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(), now(), now()) RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(inbox_id as i32)
    .bind(status)
    .bind(assignee.map(|a| a as i32))
    .bind(contact_id)
    .bind(ci)
    .bind(num(&b["team_id"]))
    .bind(b.get("additional_attributes").filter(|v| v.is_object()).cloned().unwrap_or(json!({})))
    .bind(b.get("custom_attributes").filter(|v| v.is_object()).cloned().unwrap_or(json!({})))
    .fetch_one(&st.db)
    .await?;
    // chatwoot#16151: labels in the create payload are applied before conversation.created fires,
    // so listeners never see the conversation without them.
    let labels = known_labels(&st.db, account_id, parse_labels(&b["labels"])).await?;
    if !labels.is_empty() {
        set_labels(&st.db, id, &labels).await?;
    }
    let c = emit_conv(&st, id, "conversation.created").await?;
    if let Some(content) = b["message"]["content"].as_str().filter(|s| !s.trim().is_empty()) {
        insert_message(&st, &c, Some(("User", user.id)), content, 1, false, 0, json!({}), None).await?;
    }
    let c = reload(&st.db, id).await?;
    Ok(Json(conv_json(&st.db, &c).await))
}

async fn require_member(db: &PgPool, account_id: i64, user_id: i64) -> Result<(), ApiError> {
    let ok: Option<i32> = sqlx::query_scalar("SELECT 1 FROM account_users WHERE account_id = $1 AND user_id = $2")
        .bind(account_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    ok.map(|_| ()).ok_or_else(|| ApiError::bad("Assignee is not a member of this account"))
}

/// POST /conversations/{id}/toggle_status {status?, snoozed_until?(unix)}
async fn toggle_status(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, body: Option<Json<Value>>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    let b = body.map(|b| b.0).unwrap_or_default();
    let new = match b["status"].as_str() {
        Some(s) => idx(&STATUSES, s).ok_or_else(|| ApiError::bad("Invalid status"))?,
        None => if c.status == 0 { 1 } else { 0 },
    };
    let snooze = if new == 3 {
        num(&b["snoozed_until"]).and_then(|t| chrono::DateTime::from_timestamp(t, 0)).map(|t| t.naive_utc())
    } else {
        None
    };
    sqlx::query("UPDATE conversations SET status = $1, snoozed_until = $2, status_changed_at = now(), updated_at = now() WHERE id = $3")
        .bind(new)
        .bind(snooze)
        .bind(c.id)
        .execute(&st.db)
        .await?;
    if new != c.status {
        let text = match new {
            1 => format!("Conversation was marked resolved by {}", user.name),
            2 => format!("Conversation was marked as pending by {}", user.name),
            3 => format!("Conversation was snoozed by {}", user.name),
            _ => format!("Conversation was reopened by {}", user.name),
        };
        activity(&st, &c, text).await?;
    }
    emit_conv(&st, c.id, "conversation.status_changed").await?;
    Ok(Json(json!({ "meta": {}, "payload": {
        "success": true, "conversation_id": c.display_id, "current_status": name_of(&STATUSES, new), "snoozed_until": snooze,
    } })))
}

/// POST /conversations/{id}/toggle_priority {priority: low|medium|high|urgent|null}
async fn toggle_priority(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    let new = match b["priority"].as_str().filter(|s| !s.is_empty() && *s != "none") {
        Some(p) => Some(idx(&PRIORITIES, p).ok_or_else(|| ApiError::bad("Invalid priority"))?),
        None => None,
    };
    sqlx::query("UPDATE conversations SET priority = $1, updated_at = now() WHERE id = $2").bind(new).bind(c.id).execute(&st.db).await?;
    if new != c.priority {
        let text = match (c.priority, new) {
            (None, Some(n)) => format!("{} set the priority to {}", user.name, name_of(&PRIORITIES, n)),
            (Some(o), Some(n)) => format!("{} changed the priority from {} to {}", user.name, name_of(&PRIORITIES, o), name_of(&PRIORITIES, n)),
            _ => format!("{} removed the priority", user.name),
        };
        activity(&st, &c, text).await?;
    }
    emit_conv(&st, c.id, "conversation.updated").await?;
    Ok(Json(json!({})))
}

/// POST /conversations/{id}/toggle_typing_status {typing_status: on|off, is_private}
async fn toggle_typing(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    let event = if b["typing_status"] == "on" { "conversation.typing_on" } else { "conversation.typing_off" };
    let data = json!({
        "conversation": { "id": c.display_id, "inbox_id": c.inbox_id, "account_id": c.account_id },
        "user": user_json(&st.db, account_id, user.id).await,
        "is_private": b["is_private"].as_bool().unwrap_or(false),
    });
    emit(&st, &c, event, data).await;
    Ok(Json(json!({})))
}

/// POST /conversations/{id}/assignments {assignee_id} | {team_id}
async fn assign(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    if let Some(v) = b.get("assignee_id") {
        let new = num(v).filter(|a| *a > 0);
        if let Some(a) = new {
            require_member(&st.db, account_id, a).await?;
        }
        sqlx::query("UPDATE conversations SET assignee_id = $1, updated_at = now() WHERE id = $2")
            .bind(new.map(|a| a as i32))
            .bind(c.id)
            .execute(&st.db)
            .await?;
        let agent = match new {
            Some(a) => user_json(&st.db, account_id, a).await,
            None => Value::Null,
        };
        if new != c.assignee_id {
            let text = match new {
                Some(a) if a == user.id => format!("{} self-assigned this conversation", user.name),
                Some(_) => format!("Assigned to {} by {}", agent["available_name"].as_str().unwrap_or_default(), user.name),
                None => format!("Conversation unassigned by {}", user.name),
            };
            activity(&st, &c, text).await?;
        }
        emit_conv(&st, c.id, "assignee.changed").await?;
        return Ok(Json(agent));
    }
    if let Some(v) = b.get("team_id") {
        let new = num(v).filter(|t| *t > 0);
        if let Some(t) = new {
            let ok: Option<i32> = sqlx::query_scalar("SELECT 1 FROM teams WHERE id = $1 AND account_id = $2")
                .bind(t)
                .bind(account_id)
                .fetch_optional(&st.db)
                .await?;
            ok.ok_or_else(ApiError::not_found)?;
        }
        sqlx::query("UPDATE conversations SET team_id = $1, updated_at = now() WHERE id = $2").bind(new).bind(c.id).execute(&st.db).await?;
        let team = team_json(&st.db, new).await;
        if new != c.team_id {
            let text = match new {
                Some(_) => format!("Assigned to {} by {}", team["name"].as_str().unwrap_or_default(), user.name),
                None => {
                    let old = team_json(&st.db, c.team_id).await;
                    format!("Unassigned from {} by {}", old["name"].as_str().unwrap_or_default(), user.name)
                }
            };
            activity(&st, &c, text).await?;
        }
        emit_conv(&st, c.id, "team.changed").await?;
        return Ok(Json(team));
    }
    Err(ApiError::bad("assignee_id or team_id is required"))
}

async fn update_last_seen(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    sqlx::query(
        "UPDATE conversations SET agent_last_seen_at = now(),
           assignee_last_seen_at = CASE WHEN assignee_id = $2 THEN now() ELSE assignee_last_seen_at END WHERE id = $1",
    )
    .bind(c.id)
    .bind(user.id)
    .execute(&st.db)
    .await?;
    let c = reload(&st.db, c.id).await?;
    Ok(Json(conv_json(&st.db, &c).await))
}

/// POST /conversations/{id}/unread: move last-seen to just before the latest incoming message.
async fn unread(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    sqlx::query(
        "UPDATE conversations SET agent_last_seen_at = x.t, assignee_last_seen_at = x.t
         FROM (SELECT MAX(created_at) - interval '1 second' AS t FROM messages WHERE conversation_id = $1 AND message_type = 0) x
         WHERE id = $1 AND x.t IS NOT NULL",
    )
    .bind(c.id)
    .execute(&st.db)
    .await?;
    let c = emit_conv(&st, c.id, "conversation.updated").await?;
    Ok(Json(conv_json(&st.db, &c).await))
}

async fn custom_attributes(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    let attrs = b.get("custom_attributes").filter(|v| v.is_object()).cloned().unwrap_or(json!({}));
    sqlx::query("UPDATE conversations SET custom_attributes = $1, updated_at = now() WHERE id = $2").bind(&attrs).bind(c.id).execute(&st.db).await?;
    emit_conv(&st, c.id, "conversation.updated").await?;
    Ok(Json(json!({ "custom_attributes": attrs })))
}

// ---------- labels ----------

async fn labels_index(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    Ok(Json(json!({ "payload": c.labels() })))
}

/// Parse a `labels` array: trimmed, non-empty, duplicates removed keeping first order.
fn parse_labels(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in v.as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
        let s = s.trim().to_string();
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// Write the label set on an open connection (taggings + cached_label_list).
async fn write_labels(conn: &mut sqlx::PgConnection, conv_id: i64, labels: &[String]) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM taggings WHERE taggable_type = 'Conversation' AND taggable_id = $1 AND context = 'labels'")
        .bind(conv_id as i32)
        .execute(&mut *conn)
        .await?;
    for l in labels {
        let tag: i32 = sqlx::query_scalar("INSERT INTO tags (name) VALUES ($1) ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id")
            .bind(l)
            .fetch_one(&mut *conn)
            .await?;
        sqlx::query("INSERT INTO taggings (tag_id, taggable_type, taggable_id, context, created_at) VALUES ($1, 'Conversation', $2, 'labels', now())")
            .bind(tag)
            .bind(conv_id as i32)
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("UPDATE conversations SET cached_label_list = $1, updated_at = now() WHERE id = $2")
        .bind(labels.join(", "))
        .bind(conv_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Replace the conversation's label set in one transaction (taggings + cached_label_list).
async fn set_labels(db: &PgPool, conv_id: i64, labels: &[String]) -> Result<(), ApiError> {
    let mut tx = db.begin().await?;
    write_labels(&mut *tx, conv_id, labels).await?;
    tx.commit().await?;
    Ok(())
}

/// Apply adds, then removes, to a label list. Order-preserving, no duplicates.
fn apply_label_patch(old: &[String], add: &[String], remove: &[String]) -> Vec<String> {
    let mut out: Vec<String> = old.to_vec();
    for l in add {
        if !out.contains(l) {
            out.push(l.clone());
        }
    }
    out.retain(|l| !remove.contains(l));
    out
}

/// Add/remove labels under a row lock, so concurrent writers converge. Returns (labels, added, removed).
async fn patch_labels(db: &PgPool, conv_id: i64, add: &[String], remove: &[String]) -> Result<(Vec<String>, Vec<String>, Vec<String>), ApiError> {
    let mut tx = db.begin().await?;
    let cached: Option<String> = sqlx::query_scalar("SELECT cached_label_list FROM conversations WHERE id = $1 FOR UPDATE")
        .bind(conv_id)
        .fetch_one(&mut *tx)
        .await?;
    let old: Vec<String> = cached.unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let new = apply_label_patch(&old, add, remove);
    let added: Vec<String> = new.iter().filter(|l| !old.contains(l)).cloned().collect();
    let removed: Vec<String> = old.iter().filter(|l| !new.contains(l)).cloned().collect();
    if !added.is_empty() || !removed.is_empty() {
        write_labels(&mut *tx, conv_id, &new).await?;
    }
    tx.commit().await?;
    Ok((new, added, removed))
}

fn labels_arg(b: &Value) -> Result<Vec<String>, ApiError> {
    if !b["labels"].is_array() {
        return Err(ApiError::bad("labels must be an array"));
    }
    Ok(parse_labels(&b["labels"]))
}

/// Chatwoot #13918 / PR #14159: only labels defined in the account may be attached. Unknown names are
/// dropped (case-insensitive match, stored with the account's spelling) instead of creating stray tags.
async fn known_labels(db: &PgPool, account_id: i64, wanted: Vec<String>) -> Result<Vec<String>, ApiError> {
    if wanted.is_empty() {
        return Ok(wanted);
    }
    let defined: Vec<String> = sqlx::query_scalar("SELECT title FROM labels WHERE account_id = $1 AND lower(title) = ANY($2)")
        .bind(account_id)
        .bind(wanted.iter().map(|l| l.to_lowercase()).collect::<Vec<_>>())
        .fetch_all(db)
        .await?;
    Ok(wanted.into_iter().filter_map(|w| defined.iter().find(|d| d.eq_ignore_ascii_case(&w)).cloned()).collect())
}

async fn labels_patch(st: &AppState, user: &CurrentUser, account_id: i64, id: i64, add: Vec<String>, remove: Vec<String>) -> ApiResult {
    let au = require_account(&st.db, user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    let (labels, added, removed) = patch_labels(&st.db, c.id, &add, &remove).await?;
    if !added.is_empty() {
        activity(st, &c, format!("{} added {}", user.name, added.join(", "))).await?;
    }
    if !removed.is_empty() {
        activity(st, &c, format!("{} removed {}", user.name, removed.join(", "))).await?;
    }
    emit_conv(st, c.id, "conversation.updated").await?;
    Ok(Json(json!({ "payload": labels })))
}

/// POST /conversations/{id}/labels/add {labels: [...]} adds only the given labels.
async fn labels_add(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    let add = known_labels(&st.db, account_id, labels_arg(&b)?).await?;
    labels_patch(&st, &user, account_id, id, add, vec![]).await
}

/// POST /conversations/{id}/labels/remove {labels: [...]} removes only the given labels.
async fn labels_remove(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    let remove = labels_arg(&b)?;
    labels_patch(&st, &user, account_id, id, vec![], remove).await
}

#[cfg(test)]
mod label_patch_tests {
    use super::apply_label_patch;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn add_keeps_existing_and_skips_duplicates() {
        assert_eq!(apply_label_patch(&s(&["a", "b"]), &s(&["b", "c"]), &[]), s(&["a", "b", "c"]));
    }

    #[test]
    fn remove_only_given_labels() {
        assert_eq!(apply_label_patch(&s(&["a", "b", "c"]), &[], &s(&["b", "x"])), s(&["a", "c"]));
    }

    #[test]
    fn add_then_remove_same_label_drops_it() {
        assert_eq!(apply_label_patch(&s(&["a"]), &s(&["z"]), &s(&["z"])), s(&["a"]));
    }
}

/// POST /conversations/{id}/labels {labels: [...]} replaces the label set.
async fn labels_create(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    let labels = known_labels(&st.db, account_id, parse_labels(&b["labels"])).await?;
    let old = c.labels();
    set_labels(&st.db, c.id, &labels).await?;
    let added: Vec<&String> = labels.iter().filter(|l| !old.contains(l)).collect();
    let removed: Vec<&String> = old.iter().filter(|l| !labels.contains(l)).collect();
    let fmt = |v: &[&String]| v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
    if !added.is_empty() {
        activity(&st, &c, format!("{} added {}", user.name, fmt(&added))).await?;
    }
    if !removed.is_empty() {
        activity(&st, &c, format!("{} removed {}", user.name, fmt(&removed))).await?;
    }
    emit_conv(&st, c.id, "conversation.updated").await?;
    Ok(Json(json!({ "payload": labels })))
}

// ---------- messages ----------

/// Message fields Chatwoot's messages/index.json.jbuilder does not render.
const INDEX_ONLY_OMIT: &[&str] = &[
    "account_id", "additional_attributes", "conversation", "external_source_ids", "processed_message_content",
    "sender_id", "sender_type", "sentiment", "updated_at",
];

/// GET /conversations/{id}/messages?before=&after=
async fn messages_index(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Query(p): Query<HashMap<String, String>>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    let before = p.get("before").and_then(|v| v.parse::<i64>().ok());
    let after = p.get("after").and_then(|v| v.parse::<i64>().ok());
    let (cond, order, limit, cursor) = match (before, after) {
        (Some(b), _) => ("AND m.id < $3", "DESC", 20, b),
        (None, Some(a)) => ("AND m.id > $3", "ASC", 100, a),
        _ => ("AND $3 = $3", "DESC", 20, 0),
    };
    let mut rows: Vec<Msg> = sqlx::query_as(&format!(
        "SELECT {MSG_COLS} FROM messages m WHERE m.conversation_id = $1 AND m.account_id = $2 {cond} ORDER BY m.id {order} LIMIT {limit}"
    ))
    .bind(c.id)
    .bind(c.account_id)
    .bind(cursor)
    .fetch_all(&st.db)
    .await?;
    if order == "DESC" {
        rows.reverse();
    }
    let mut payload = Vec::with_capacity(rows.len());
    for m in &rows {
        let mut j = msg_json(&st.db, m, &c).await;
        // msg_json is the push_event_data superset (cable events need it). messages/index.json.jbuilder
        // omits these, and they roughly doubled the body; drop them to match Chatwoot 4.19.1.
        if let Some(o) = j.as_object_mut() {
            for k in INDEX_ONLY_OMIT {
                o.remove(*k);
            }
            if let Some(s) = o.get_mut("sender").and_then(Value::as_object_mut) {
                for k in ["account_id", "auto_offline", "confirmed", "email", "provider", "role"] {
                    s.remove(k);
                }
            }
        }
        payload.push(j);
    }
    let mut meta = json!({
        "labels": c.labels(), "additional_attributes": c.additional_attributes.clone().unwrap_or(json!({})),
        "contact": contact_json(&st.db, c.contact_id).await,
        "agent_last_seen_at": c.agent_last_seen_at, "assignee_last_seen_at": c.assignee_last_seen_at,
    });
    if let Some(a) = c.assignee_id {
        meta["assignee"] = user_json(&st.db, account_id, a).await;
    }
    Ok(Json(json!({ "meta": meta, "payload": payload })))
}

/// POST /conversations/{id}/messages: JSON, or multipart/form-data with `attachments[]` files (handled by storage).
async fn messages_create_any(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    req: axum::extract::Request,
) -> ApiResult {
    use axum::extract::FromRequest;
    let is_multipart = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));
    if is_multipart {
        let mp = axum::extract::Multipart::from_request(req, &st).await.map_err(|e| ApiError::bad(e.to_string()))?;
        return crate::storage::create_with_attachments(&st, &user, account_id, id, mp).await;
    }
    let Json(b) = Json::<Value>::from_request(req, &st).await.map_err(|e| ApiError::bad(e.to_string()))?;
    messages_create(State(st), user, Path((account_id, id)), Json(b)).await
}

/// POST /conversations/{id}/messages {content, private, message_type?, content_type?, content_attributes?, echo_id?}
/// ponytail: JSON only, no multipart attachments; API-channel webhook delivery not ported.
async fn messages_create(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    let content = b["content"].as_str().unwrap_or_default();
    if content.trim().is_empty() {
        return Err(ApiError::bad("Message content can't be blank"));
    }
    let message_type = match &b["message_type"] {
        Value::String(s) => idx(&MSG_TYPES, s).ok_or_else(|| ApiError::bad("Invalid message_type"))?,
        Value::Number(n) => n.as_i64().filter(|n| (0..4).contains(n)).ok_or_else(|| ApiError::bad("Invalid message_type"))? as i32,
        _ => 1,
    };
    let content_type = b["content_type"].as_str().and_then(|s| idx(&CONTENT_TYPES, s)).unwrap_or(0);
    let mut attrs = b.get("content_attributes").filter(|v| v.is_object()).cloned().unwrap_or(json!({}));
    resolve_in_reply_to(&st.db, c.id, &mut attrs).await?;
    // API channels pass the external id so later replies can reference it (Chatwoot permits source_id here).
    // Client-sent carrier keys are dropped first, so only the top-level source_id is honored.
    if let Some(o) = attrs.as_object_mut() {
        o.remove(SOURCE_ID_KEY);
    }
    if let Some(sid) = b["source_id"].as_str().filter(|s| !s.is_empty()) {
        attrs[SOURCE_ID_KEY] = json!(sid);
    }
    let sender = if message_type == 0 { c.contact_id.map(|id| ("Contact", id)) } else { Some(("User", user.id)) };
    let data = insert_message(
        &st, &c, sender, content, message_type, b["private"].as_bool().unwrap_or(false), content_type, attrs, b["echo_id"].as_str(),
    )
    .await?;
    Ok(Json(data))
}

/// Chatwoot #10859 (Messages::InReplyToMessageBuilder): `in_reply_to` (message id) or
/// `in_reply_to_external_id` (source_id) is looked up in the same conversation and both keys are set from
/// the found message, or cleared when nothing matches.
async fn resolve_in_reply_to(db: &PgPool, conv_id: i64, attrs: &mut Value) -> Result<(), ApiError> {
    let id = attrs.get("in_reply_to").and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()));
    let ext = attrs.get("in_reply_to_external_id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    if id.is_none() && ext.is_none() {
        return Ok(());
    }
    let found: Option<(i64, Option<String>)> = match id {
        Some(id) => sqlx::query_as("SELECT id::bigint, source_id FROM messages WHERE conversation_id = $1 AND id = $2").bind(conv_id).bind(id),
        None => sqlx::query_as("SELECT id::bigint, source_id FROM messages WHERE conversation_id = $1 AND source_id = $2 ORDER BY id LIMIT 1")
            .bind(conv_id)
            .bind(ext.clone().unwrap_or_default()),
    }
    .fetch_optional(db)
    .await?;
    attrs["in_reply_to"] = json!(found.as_ref().map(|f| f.0));
    attrs["in_reply_to_external_id"] = json!(found.and_then(|f| f.1));
    Ok(())
}

/// DELETE /conversations/{id}/messages/{message_id}: soft delete like Rails.
async fn messages_destroy(State(st): State<AppState>, user: CurrentUser, Path((account_id, id, message_id)): Path<(i64, i64, i64)>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let c = load(&st.db, &au, id).await?;
    let m: Msg = sqlx::query_as(&format!(
        "UPDATE messages AS m SET content = 'This message was deleted',
           content_attributes = (COALESCE(m.content_attributes::jsonb, '{{}}'::jsonb) || '{{\"deleted\": true}}'::jsonb)::json, updated_at = now()
         WHERE m.id = $1 AND m.conversation_id = $2 AND m.account_id = $3 RETURNING {MSG_COLS}"
    ))
    .bind(message_id)
    .bind(c.id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let data = msg_json(&st.db, &m, &c).await;
    emit(&st, &c, "message.updated", data.clone()).await;
    Ok(Json(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filter_parsing() {
        let q = vec![
            ("status".into(), "resolved".into()),
            ("assignee_type".into(), "me".into()),
            ("labels[]".into(), "vip".into()),
            ("sort_by".into(), "x; DROP TABLE users".into()),
            ("conversation_type".into(), "evil".into()),
        ];
        let f = parse_filter(&q);
        assert_eq!(f.status_i, 1);
        assert_eq!(f.assignee, "c.assignee_id = $8");
        assert_eq!(f.labels, Some(vec!["vip".to_string()]));
        assert_eq!(f.sort, "c.last_activity_at DESC, c.id DESC");
        assert_eq!(f.conv_type, None);
    }
}
