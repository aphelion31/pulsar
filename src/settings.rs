//! Labels, canned responses, custom attributes, custom filters, notifications, notification settings,
//! cache keys, search, and empty stubs for unported settings pages.
use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::NaiveDateTime;
use serde_json::{json, Value};

use crate::common::*;

const NOTIFICATION_TYPES: [&str; 9] = [
    "conversation_creation", "conversation_assignment", "assigned_conversation_new_message", "conversation_mention",
    "participating_conversation_new_message", "sla_missed_first_response", "sla_missed_next_response",
    "sla_missed_resolution", "voice_call_missed",
];

pub fn routes() -> Router<AppState> {
    let empty_arr = || get(|| async { Json(json!([])) });
    let empty_payload = || get(|| async { Json(json!({ "payload": [] })) });
    Router::new()
        .route("/labels", get(labels_index).post(labels_create))
        .route("/labels/{id}", get(labels_show).patch(labels_update).put(labels_update).delete(labels_destroy))
        .route("/canned_responses", get(canned_index).post(canned_create))
        .route("/canned_responses/{id}", patch(canned_update).put(canned_update).delete(canned_destroy))
        .route("/custom_attribute_definitions", get(cad_index).post(cad_create))
        .route("/custom_attribute_definitions/{id}", get(cad_show).patch(cad_update).put(cad_update).delete(cad_destroy))
        .route("/custom_filters", get(cf_index).post(cf_create))
        .route("/custom_filters/{id}", get(cf_show).patch(cf_update).put(cf_update).delete(cf_destroy))
        .route("/notifications", get(notif_index))
        .route("/notifications/unread_count", get(notif_unread))
        .route("/notifications/read_all", post(notif_read_all))
        .route("/notifications/{id}", patch(notif_read).delete(notif_destroy))
        .route("/notification_settings", get(ns_show).patch(ns_update).put(ns_update))
        .route("/cache_keys", get(cache_keys))
        .route("/search", get(search_all))
        .route("/search/contacts", get(search_all))
        .route("/search/conversations", get(search_all))
        .route("/search/messages", get(search_all))
        // ponytail: unported settings pages; empty data in the shape each Vue store expects
        .route("/assignment_policies", empty_arr())
        .route("/working_hours", empty_arr())
        .route("/sla_policies", empty_payload())
        .route("/integrations/apps", empty_payload())
        .route("/portals", get(|| async { Json(json!({ "payload": [], "meta": { "all_articles_count": 0 } })) }))
        .route("/captain/assistants", get(|| async { Json(json!({ "payload": [], "meta": {} })) }))
}

fn s(b: &Value, k: &str) -> Option<String> {
    b[k].as_str().map(str::to_string)
}

// ---------- labels ----------

#[derive(sqlx::FromRow)]
struct Label {
    id: i64,
    title: Option<String>,
    description: Option<String>,
    color: String,
    show_on_sidebar: Option<bool>,
}
impl Label {
    fn json(&self) -> Value {
        json!({ "id": self.id, "title": self.title, "description": self.description, "color": self.color, "show_on_sidebar": self.show_on_sidebar })
    }
}
const LABEL_COLS: &str = "id, title, description, color, show_on_sidebar";

async fn labels_index(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let rows: Vec<Label> = sqlx::query_as(&format!("SELECT {LABEL_COLS} FROM labels WHERE account_id = $1 ORDER BY title"))
        .bind(account_id)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(json!({ "payload": rows.iter().map(Label::json).collect::<Vec<_>>() })))
}

async fn labels_show(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let l: Label = sqlx::query_as(&format!("SELECT {LABEL_COLS} FROM labels WHERE id = $1 AND account_id = $2"))
        .bind(id)
        .bind(account_id)
        .fetch_one(&st.db)
        .await?;
    Ok(Json(l.json()))
}

/// Rails Label: title lowercase, letters/digits/_/- only, unique per account.
fn label_title(b: &Value) -> Result<Option<String>, ApiError> {
    let Some(t) = b["title"].as_str() else { return Ok(None) };
    let t = t.trim().to_lowercase();
    if t.is_empty() || !t.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(ApiError::bad("Title is invalid"));
    }
    Ok(Some(t))
}

async fn title_taken(st: &AppState, account_id: i64, title: &str, except: i64) -> Result<bool, ApiError> {
    let r: Option<i32> = sqlx::query_scalar("SELECT 1 FROM labels WHERE account_id = $1 AND title = $2 AND id <> $3")
        .bind(account_id)
        .bind(title)
        .bind(except)
        .fetch_optional(&st.db)
        .await?;
    Ok(r.is_some())
}

async fn labels_create(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Json(b): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?.require_admin()?;
    let title = label_title(&b)?.ok_or_else(|| ApiError::bad("Title can't be blank"))?;
    if title_taken(&st, account_id, &title, 0).await? {
        return Err(ApiError::bad("Title has already been taken"));
    }
    let l: Label = sqlx::query_as(&format!(
        "INSERT INTO labels (title, description, color, show_on_sidebar, account_id, created_at, updated_at)
         VALUES ($1, $2, COALESCE($3, '#1f93ff'), $4, $5, now(), now()) RETURNING {LABEL_COLS}"
    ))
    .bind(&title)
    .bind(s(&b, "description"))
    .bind(s(&b, "color"))
    .bind(b["show_on_sidebar"].as_bool())
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;
    st.hub.invalidate_cache(account_id, "label");
    Ok(Json(l.json()))
}

async fn labels_update(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?.require_admin()?;
    let title = label_title(&b)?;
    if let Some(t) = &title {
        if title_taken(&st, account_id, t, id).await? {
            return Err(ApiError::bad("Title has already been taken"));
        }
    }
    let mut tx = st.db.begin().await?;
    let old: Option<Label> = sqlx::query_as(&format!(
        "SELECT {LABEL_COLS} FROM labels WHERE id = $1 AND account_id = $2 FOR UPDATE"
    ))
    .bind(id)
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await?;
    let old = old.ok_or_else(ApiError::not_found)?;

    let l: Label = sqlx::query_as(&format!(
        "UPDATE labels SET title = COALESCE($1, title), description = COALESCE($2, description), color = COALESCE($3, color),
           show_on_sidebar = COALESCE($4, show_on_sidebar), updated_at = now()
         WHERE id = $5 AND account_id = $6 RETURNING {LABEL_COLS}"
    ))
    .bind(&title)
    .bind(s(&b, "description"))
    .bind(s(&b, "color"))
    .bind(b["show_on_sidebar"].as_bool())
    .bind(id)
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await?;

    if let (Some(old_t), Some(new_t)) = (old.title.as_deref(), l.title.as_deref()) {
        if old_t != new_t {
            let new_tag_id: i32 = sqlx::query_scalar(
                "INSERT INTO tags (name) VALUES ($1) ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id",
            )
            .bind(new_t)
            .fetch_one(&mut *tx)
            .await?;

            let old_tag_id: Option<i32> = sqlx::query_scalar("SELECT id FROM tags WHERE name = $1")
                .bind(old_t)
                .fetch_optional(&mut *tx)
                .await?;

            if let Some(old_tag_id) = old_tag_id {
                let affected_conv_ids: Vec<i64> = sqlx::query_scalar(
                    "SELECT DISTINCT c.id::bigint
                     FROM conversations c
                     JOIN taggings tg ON tg.taggable_id = c.id
                                     AND tg.taggable_type = 'Conversation'
                                     AND tg.context = 'labels'
                     WHERE c.account_id::bigint = $1
                       AND tg.tag_id = $2",
                )
                .bind(account_id)
                .bind(old_tag_id)
                .fetch_all(&mut *tx)
                .await?;

                sqlx::query(
                    "DELETE FROM taggings tg
                     WHERE tg.tag_id = $1
                       AND tg.context = 'labels'
                       AND (
                         (tg.taggable_type = 'Conversation' AND EXISTS (
                             SELECT 1 FROM conversations c
                             WHERE c.id = tg.taggable_id AND c.account_id::bigint = $2
                         ))
                         OR
                         (tg.taggable_type = 'Contact' AND EXISTS (
                             SELECT 1 FROM contacts ct
                             WHERE ct.id = tg.taggable_id AND ct.account_id::bigint = $2
                         ))
                       )
                       AND EXISTS (
                         SELECT 1 FROM taggings dup
                         WHERE dup.tag_id = $3
                           AND dup.taggable_type = tg.taggable_type
                           AND dup.taggable_id = tg.taggable_id
                           AND dup.context = 'labels'
                       )",
                )
                .bind(old_tag_id)
                .bind(account_id)
                .bind(new_tag_id)
                .execute(&mut *tx)
                .await?;

                sqlx::query(
                    "DELETE FROM taggings tg
                     WHERE tg.tag_id = $1
                       AND tg.context = 'labels'
                       AND (
                         (tg.taggable_type = 'Conversation' AND EXISTS (
                             SELECT 1 FROM conversations c
                             WHERE c.id = tg.taggable_id AND c.account_id::bigint = $2
                         ))
                         OR
                         (tg.taggable_type = 'Contact' AND EXISTS (
                             SELECT 1 FROM contacts ct
                             WHERE ct.id = tg.taggable_id AND ct.account_id::bigint = $2
                         ))
                       )
                       AND tg.id NOT IN (
                         SELECT MIN(x.id)
                         FROM taggings x
                         WHERE x.tag_id = $1
                           AND x.context = 'labels'
                           AND x.taggable_type = tg.taggable_type
                           AND x.taggable_id = tg.taggable_id
                       )",
                )
                .bind(old_tag_id)
                .bind(account_id)
                .execute(&mut *tx)
                .await?;

                sqlx::query(
                    "UPDATE taggings tg
                     SET tag_id = $3
                     WHERE tg.tag_id = $1
                       AND tg.context = 'labels'
                       AND (
                         (tg.taggable_type = 'Conversation' AND EXISTS (
                             SELECT 1 FROM conversations c
                             WHERE c.id = tg.taggable_id AND c.account_id::bigint = $2
                         ))
                         OR
                         (tg.taggable_type = 'Contact' AND EXISTS (
                             SELECT 1 FROM contacts ct
                             WHERE ct.id = tg.taggable_id AND ct.account_id::bigint = $2
                         ))
                       )",
                )
                .bind(old_tag_id)
                .bind(account_id)
                .bind(new_tag_id)
                .execute(&mut *tx)
                .await?;

                if !affected_conv_ids.is_empty() {
                    sqlx::query(
                        "UPDATE conversations c
                         SET cached_label_list = (
                             SELECT string_agg(t.name, ', ' ORDER BY t.name)
                             FROM taggings tg
                             JOIN tags t ON t.id = tg.tag_id
                             WHERE tg.taggable_type = 'Conversation'
                               AND tg.taggable_id = c.id
                               AND tg.context = 'labels'
                         ),
                         updated_at = now()
                         WHERE c.account_id::bigint = $1
                           AND c.id::bigint = ANY($2::bigint[])",
                    )
                    .bind(account_id)
                    .bind(&affected_conv_ids)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
    }

    tx.commit().await?;
    st.hub.invalidate_cache(account_id, "label");
    Ok(Json(l.json()))
}

async fn labels_destroy(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?.require_admin()?;
    let n = sqlx::query("DELETE FROM labels WHERE id = $1 AND account_id = $2").bind(id).bind(account_id).execute(&st.db).await?;
    if n.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    st.hub.invalidate_cache(account_id, "label");
    Ok(Json(json!({})))
}

// ---------- canned responses ----------

#[derive(sqlx::FromRow)]
struct Canned {
    id: i64,
    account_id: i64,
    short_code: Option<String>,
    content: Option<String>,
}
impl Canned {
    fn json(&self) -> Value {
        json!({ "id": self.id, "account_id": self.account_id, "short_code": self.short_code, "content": self.content })
    }
}
const CANNED_COLS: &str = "id::bigint AS id, account_id::bigint AS account_id, short_code, content";

async fn canned_index(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let pat = q.get("search").filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
    let rows: Vec<Canned> = sqlx::query_as(&format!(
        "SELECT {CANNED_COLS} FROM canned_responses WHERE account_id = $1
           AND ($2::text IS NULL OR short_code ILIKE $2 OR content ILIKE $2)
         ORDER BY (short_code ILIKE $2) DESC NULLS LAST, short_code"
    ))
    .bind(account_id)
    .bind(pat)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(json!(rows.iter().map(Canned::json).collect::<Vec<_>>())))
}

fn canned_fields(b: &Value) -> (Option<String>, Option<String>) {
    let b = if b["canned_response"].is_object() { &b["canned_response"] } else { b };
    (s(b, "short_code"), s(b, "content"))
}

async fn canned_create(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Json(b): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let (code, content) = canned_fields(&b);
    let (Some(code), Some(content)) = (code.filter(|c| !c.trim().is_empty()), content.filter(|c| !c.trim().is_empty())) else {
        return Err(ApiError::bad("Short code and content can't be blank"));
    };
    let taken: Option<i32> = sqlx::query_scalar("SELECT 1 FROM canned_responses WHERE account_id = $1 AND short_code = $2")
        .bind(account_id as i32)
        .bind(&code)
        .fetch_optional(&st.db)
        .await?;
    if taken.is_some() {
        return Err(ApiError::bad("Short code has already been taken"));
    }
    let c: Canned = sqlx::query_as(&format!(
        "INSERT INTO canned_responses (account_id, short_code, content, created_at, updated_at) VALUES ($1, $2, $3, now(), now()) RETURNING {CANNED_COLS}"
    ))
    .bind(account_id as i32)
    .bind(code)
    .bind(content)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(c.json()))
}

async fn canned_update(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let (code, content) = canned_fields(&b);
    let c: Canned = sqlx::query_as(&format!(
        "UPDATE canned_responses SET short_code = COALESCE($1, short_code), content = COALESCE($2, content), updated_at = now()
         WHERE id = $3 AND account_id = $4 RETURNING {CANNED_COLS}"
    ))
    .bind(code)
    .bind(content)
    .bind(id as i32)
    .bind(account_id as i32)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(c.json()))
}

async fn canned_destroy(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?.require_admin()?;
    let n = sqlx::query("DELETE FROM canned_responses WHERE id = $1 AND account_id = $2")
        .bind(id as i32)
        .bind(account_id as i32)
        .execute(&st.db)
        .await?;
    if n.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({})))
}

// ---------- custom attribute definitions ----------
// attribute_model: 0 conversation_attribute, 1 contact_attribute.
// attribute_display_type: text number currency percent link date list checkbox.

const DISPLAY_TYPES: [&str; 8] = ["text", "number", "currency", "percent", "link", "date", "list", "checkbox"];
const ATTR_MODELS: [&str; 2] = ["conversation_attribute", "contact_attribute"];

#[derive(sqlx::FromRow)]
struct Cad {
    id: i64,
    attribute_display_name: Option<String>,
    attribute_key: Option<String>,
    attribute_display_type: Option<i32>,
    default_value: Option<i32>,
    attribute_model: Option<i32>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    attribute_description: Option<String>,
    attribute_values: Option<Value>,
    regex_pattern: Option<String>,
    regex_cue: Option<String>,
}
impl Cad {
    fn json(&self) -> Value {
        fn pick(l: &[&'static str], i: Option<i32>) -> &'static str {
            l.get(i.unwrap_or(0) as usize).copied().unwrap_or(l[0])
        }
        json!({
            "id": self.id, "attribute_display_name": self.attribute_display_name,
            "attribute_display_type": pick(&DISPLAY_TYPES, self.attribute_display_type),
            "attribute_description": self.attribute_description, "attribute_key": self.attribute_key,
            "regex_pattern": self.regex_pattern, "regex_cue": self.regex_cue,
            "attribute_values": self.attribute_values.clone().unwrap_or(json!([])),
            "attribute_model": pick(&ATTR_MODELS, self.attribute_model), "default_value": self.default_value,
            "created_at": self.created_at, "updated_at": self.updated_at,
        })
    }
}
const CAD_COLS: &str = "id, attribute_display_name, attribute_key, attribute_display_type, default_value, attribute_model,
  created_at, updated_at, attribute_description, attribute_values, regex_pattern, regex_cue";

/// Accepts int or enum-name for display type / model.
fn enum_val(v: &Value, list: &[&str]) -> Option<i32> {
    if let Some(i) = v.as_i64() {
        return Some(i as i32);
    }
    let s = v.as_str()?;
    s.parse().ok().or_else(|| list.iter().position(|x| *x == s).map(|i| i as i32))
}

async fn cad_index(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let model = q.get("attribute_model").and_then(|m| enum_val(&json!(m), &ATTR_MODELS));
    let rows: Vec<Cad> = sqlx::query_as(&format!(
        "SELECT {CAD_COLS} FROM custom_attribute_definitions WHERE account_id = $1 AND ($2::int IS NULL OR attribute_model = $2) ORDER BY id"
    ))
    .bind(account_id)
    .bind(model)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(json!(rows.iter().map(Cad::json).collect::<Vec<_>>())))
}

async fn cad_show(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let c: Cad = sqlx::query_as(&format!("SELECT {CAD_COLS} FROM custom_attribute_definitions WHERE id = $1 AND account_id = $2"))
        .bind(id)
        .bind(account_id)
        .fetch_one(&st.db)
        .await?;
    Ok(Json(c.json()))
}

fn cad_body(b: &Value) -> &Value {
    if b["custom_attribute_definition"].is_object() { &b["custom_attribute_definition"] } else { b }
}

async fn cad_create(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Json(b): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?.require_admin()?;
    let b = cad_body(&b);
    let key = s(b, "attribute_key").filter(|k| !k.trim().is_empty()).ok_or_else(|| ApiError::bad("Attribute key can't be blank"))?;
    let model = enum_val(&b["attribute_model"], &ATTR_MODELS).unwrap_or(0);
    let taken: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM custom_attribute_definitions WHERE account_id = $1 AND attribute_key = $2 AND attribute_model = $3")
            .bind(account_id)
            .bind(&key)
            .bind(model)
            .fetch_optional(&st.db)
            .await?;
    if taken.is_some() {
        return Err(ApiError::bad("Attribute key has already been taken"));
    }
    let c: Cad = sqlx::query_as(&format!(
        "INSERT INTO custom_attribute_definitions (attribute_display_name, attribute_key, attribute_display_type, attribute_model,
            account_id, attribute_description, attribute_values, regex_pattern, regex_cue, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, '[]'::jsonb), $8, $9, now(), now()) RETURNING {CAD_COLS}"
    ))
    .bind(s(b, "attribute_display_name"))
    .bind(key)
    .bind(enum_val(&b["attribute_display_type"], &DISPLAY_TYPES).unwrap_or(0))
    .bind(model)
    .bind(account_id)
    .bind(s(b, "attribute_description"))
    .bind(b.get("attribute_values").filter(|v| v.is_array()))
    .bind(s(b, "regex_pattern"))
    .bind(s(b, "regex_cue"))
    .fetch_one(&st.db)
    .await?;
    Ok(Json(c.json()))
}

async fn cad_update(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?.require_admin()?;
    let b = cad_body(&b);
    let c: Cad = sqlx::query_as(&format!(
        "UPDATE custom_attribute_definitions SET attribute_display_name = COALESCE($1, attribute_display_name),
           attribute_description = COALESCE($2, attribute_description), attribute_values = COALESCE($3, attribute_values),
           regex_pattern = COALESCE($4, regex_pattern), regex_cue = COALESCE($5, regex_cue), updated_at = now()
         WHERE id = $6 AND account_id = $7 RETURNING {CAD_COLS}"
    ))
    .bind(s(b, "attribute_display_name"))
    .bind(s(b, "attribute_description"))
    .bind(b.get("attribute_values").filter(|v| v.is_array()))
    .bind(s(b, "regex_pattern"))
    .bind(s(b, "regex_cue"))
    .bind(id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(c.json()))
}

async fn cad_destroy(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?.require_admin()?;
    let n = sqlx::query("DELETE FROM custom_attribute_definitions WHERE id = $1 AND account_id = $2").bind(id).bind(account_id).execute(&st.db).await?;
    if n.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({})))
}

// ---------- custom filters (per user) ----------
// filter_type: 0 conversation, 1 contact, 2 report.

const FILTER_TYPES: [&str; 3] = ["conversation", "contact", "report"];

#[derive(sqlx::FromRow)]
struct Cf {
    id: i64,
    name: String,
    filter_type: i32,
    query: Value,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}
impl Cf {
    fn json(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "filter_type": FILTER_TYPES.get(self.filter_type as usize).copied().unwrap_or("conversation"),
            "query": self.query, "created_at": self.created_at, "updated_at": self.updated_at,
        })
    }
}
const CF_COLS: &str = "id, name, filter_type, query, created_at, updated_at";

async fn cf_index(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let ft = q.get("filter_type").and_then(|t| enum_val(&json!(t), &FILTER_TYPES)).unwrap_or(0);
    let rows: Vec<Cf> = sqlx::query_as(&format!(
        "SELECT {CF_COLS} FROM custom_filters WHERE account_id = $1 AND user_id = $2 AND filter_type = $3 ORDER BY id"
    ))
    .bind(account_id)
    .bind(user.id)
    .bind(ft)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(json!(rows.iter().map(Cf::json).collect::<Vec<_>>())))
}

async fn cf_show(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let c: Cf = sqlx::query_as(&format!("SELECT {CF_COLS} FROM custom_filters WHERE id = $1 AND account_id = $2 AND user_id = $3"))
        .bind(id)
        .bind(account_id)
        .bind(user.id)
        .fetch_one(&st.db)
        .await?;
    Ok(Json(c.json()))
}

async fn cf_create(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Json(b): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let b = if b["custom_filter"].is_object() { &b["custom_filter"] } else { &b };
    let name = s(b, "name").filter(|n| !n.trim().is_empty()).ok_or_else(|| ApiError::bad("Name can't be blank"))?;
    let c: Cf = sqlx::query_as(&format!(
        "INSERT INTO custom_filters (name, filter_type, query, account_id, user_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, now(), now()) RETURNING {CF_COLS}"
    ))
    .bind(name)
    .bind(enum_val(&b["filter_type"], &FILTER_TYPES).unwrap_or(0))
    .bind(b.get("query").cloned().unwrap_or(json!({})))
    .bind(account_id)
    .bind(user.id)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(c.json()))
}

async fn cf_update(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>, Json(b): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let b = if b["custom_filter"].is_object() { &b["custom_filter"] } else { &b };
    let c: Cf = sqlx::query_as(&format!(
        "UPDATE custom_filters SET name = COALESCE($1, name), query = COALESCE($2, query), updated_at = now()
         WHERE id = $3 AND account_id = $4 AND user_id = $5 RETURNING {CF_COLS}"
    ))
    .bind(s(b, "name"))
    .bind(b.get("query").filter(|q| !q.is_null()))
    .bind(id)
    .bind(account_id)
    .bind(user.id)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(c.json()))
}

async fn cf_destroy(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let n = sqlx::query("DELETE FROM custom_filters WHERE id = $1 AND account_id = $2 AND user_id = $3")
        .bind(id)
        .bind(account_id)
        .bind(user.id)
        .execute(&st.db)
        .await?;
    if n.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({})))
}

// ---------- notifications ----------
// ponytail: notifications are read/managed here but not generated yet (no listener port). Add when mentions/assignment
// notifications are needed.

#[derive(sqlx::FromRow)]
struct Notif {
    id: i64,
    notification_type: i32,
    primary_actor_type: String,
    primary_actor_id: i64,
    read_at: Option<NaiveDateTime>,
    created_at: NaiveDateTime,
    last_activity_at: Option<NaiveDateTime>,
    snoozed_until: Option<NaiveDateTime>,
    meta: Option<Value>,
}
impl Notif {
    fn json(&self, user: &CurrentUser) -> Value {
        json!({
            "id": self.id,
            "notification_type": NOTIFICATION_TYPES.get((self.notification_type - 1).max(0) as usize).copied().unwrap_or("conversation_creation"),
            "push_message_title": "", "push_message_body": "",
            "primary_actor_type": self.primary_actor_type, "primary_actor_id": self.primary_actor_id,
            "primary_actor": { "id": self.primary_actor_id }, "read_at": self.read_at, "secondary_actor": null,
            "user": { "id": user.id, "name": user.name, "type": "user" },
            "created_at": self.created_at.and_utc().timestamp(), "last_activity_at": ts(self.last_activity_at),
            "snoozed_until": self.snoozed_until, "meta": self.meta.clone().unwrap_or(json!({})),
        })
    }
}

async fn unread(st: &AppState, account_id: i64, user_id: i64) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE account_id = $1 AND user_id = $2 AND read_at IS NULL")
        .bind(account_id)
        .bind(user_id)
        .fetch_one(&st.db)
        .await?)
}

async fn notif_index(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let page = q.get("page").and_then(|p| p.parse::<i64>().ok()).unwrap_or(1).max(1);
    let rows: Vec<Notif> = sqlx::query_as(
        "SELECT id, notification_type, primary_actor_type, primary_actor_id, read_at, created_at, last_activity_at, snoozed_until, meta
         FROM notifications WHERE account_id = $1 AND user_id = $2 ORDER BY last_activity_at DESC NULLS LAST, id DESC LIMIT 15 OFFSET $3",
    )
    .bind(account_id)
    .bind(user.id)
    .bind((page - 1) * 15)
    .fetch_all(&st.db)
    .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE account_id = $1 AND user_id = $2")
        .bind(account_id)
        .bind(user.id)
        .fetch_one(&st.db)
        .await?;
    Ok(Json(json!({ "data": {
        "meta": { "unread_count": unread(&st, account_id, user.id).await?, "count": count, "current_page": page },
        "payload": rows.iter().map(|n| n.json(&user)).collect::<Vec<_>>(),
    } })))
}

async fn notif_unread(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(json!(unread(&st, account_id, user.id).await?)))
}

async fn notif_read_all(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    sqlx::query("UPDATE notifications SET read_at = now() WHERE account_id = $1 AND user_id = $2 AND read_at IS NULL")
        .bind(account_id)
        .bind(user.id)
        .execute(&st.db)
        .await?;
    Ok(Json(json!({})))
}

async fn notif_read(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let n = sqlx::query("UPDATE notifications SET read_at = now() WHERE id = $1 AND account_id = $2 AND user_id = $3")
        .bind(id)
        .bind(account_id)
        .bind(user.id)
        .execute(&st.db)
        .await?;
    if n.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({})))
}

async fn notif_destroy(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    sqlx::query("DELETE FROM notifications WHERE id = $1 AND account_id = $2 AND user_id = $3")
        .bind(id)
        .bind(account_id)
        .bind(user.id)
        .execute(&st.db)
        .await?;
    Ok(Json(json!({})))
}

// ---------- notification settings (bit flags, bit i <-> NOTIFICATION_TYPES[i]) ----------

fn flags_to_names(prefix: &str, flags: i32) -> Vec<String> {
    NOTIFICATION_TYPES.iter().enumerate().filter(|(i, _)| flags >> i & 1 == 1).map(|(_, n)| format!("{prefix}_{n}")).collect()
}
fn names_to_flags(prefix: &str, v: &Value) -> Option<i32> {
    let arr = v.as_array()?;
    Some(NOTIFICATION_TYPES.iter().enumerate().fold(0, |acc, (i, n)| {
        if arr.iter().any(|x| x.as_str() == Some(&format!("{prefix}_{n}"))) { acc | 1 << i } else { acc }
    }))
}

async fn ns_json(st: &AppState, account_id: i64, user_id: i64) -> Result<Value, ApiError> {
    let r: Option<(i64, i32, i32)> = sqlx::query_as("SELECT id, email_flags, push_flags FROM notification_settings WHERE account_id = $1 AND user_id = $2")
        .bind(account_id as i32)
        .bind(user_id as i32)
        .fetch_optional(&st.db)
        .await?;
    let (id, e, p) = match r {
        Some(r) => r,
        None => sqlx::query_as(
            "INSERT INTO notification_settings (account_id, user_id, email_flags, push_flags, created_at, updated_at)
             VALUES ($1, $2, 0, 0, now(), now()) RETURNING id, email_flags, push_flags",
        )
        .bind(account_id as i32)
        .bind(user_id as i32)
        .fetch_one(&st.db)
        .await?,
    };
    Ok(json!({
        "id": id, "user_id": user_id, "account_id": account_id,
        "all_email_flags": flags_to_names("email", -1), "selected_email_flags": flags_to_names("email", e),
        "all_push_flags": flags_to_names("push", -1), "selected_push_flags": flags_to_names("push", p),
    }))
}

async fn ns_show(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(ns_json(&st, account_id, user.id).await?))
}

async fn ns_update(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Json(b): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    ns_json(&st, account_id, user.id).await?; // ensure row exists
    let ns = &b["notification_settings"];
    sqlx::query(
        "UPDATE notification_settings SET email_flags = COALESCE($1, email_flags), push_flags = COALESCE($2, push_flags), updated_at = now()
         WHERE account_id = $3 AND user_id = $4",
    )
    .bind(names_to_flags("email", &ns["selected_email_flags"]))
    .bind(names_to_flags("push", &ns["selected_push_flags"]))
    .bind(account_id as i32)
    .bind(user.id as i32)
    .execute(&st.db)
    .await?;
    Ok(Json(ns_json(&st, account_id, user.id).await?))
}

// ---------- cache keys, search ----------

async fn cache_keys(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(json!({ "cache_keys": crate::cable::cache_keys(account_id) })))
}

/// GET /search?q= (and /search/{contacts,conversations,messages}): ILIKE over contacts, messages. 15 per kind.
/// Agents only see conversations/messages from inboxes they belong to.
async fn search_all(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let Some(term) = q.get("q").map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Ok(Json(json!({ "payload": { "contacts": [], "conversations": [], "messages": [], "articles": [] } })));
    };
    let pat = format!("%{term}%");
    let contacts: Vec<(i64, Option<String>, Option<String>, Option<String>, Option<String>, Option<NaiveDateTime>)> = sqlx::query_as(
        "SELECT id::bigint, name, email, phone_number, identifier, last_activity_at FROM contacts WHERE account_id = $1
           AND (name ILIKE $2 OR email ILIKE $2 OR phone_number ILIKE $2 OR identifier ILIKE $2)
         ORDER BY last_activity_at DESC NULLS LAST LIMIT 15",
    )
    .bind(account_id)
    .bind(&pat)
    .fetch_all(&st.db)
    .await?;
    // Chatwoot #11885: search must honor the custom role scope too, not only inbox membership.
    let visible = format!(
        "($3 OR m.inbox_id IN (SELECT inbox_id FROM inbox_members WHERE user_id = $4)) AND {}",
        au.custom_scope_sql("$4")
    );
    let messages: Vec<(i64, Option<String>, i32, i64, i32, NaiveDateTime, bool, Option<String>, Option<i64>)> = sqlx::query_as(&format!(
        "SELECT m.id::bigint, m.content, m.message_type, c.display_id::bigint, m.inbox_id, m.created_at, m.private, m.sender_type, m.sender_id
         FROM messages m JOIN conversations c ON c.id = m.conversation_id
         WHERE m.account_id = $1 AND m.message_type IN (0, 1) AND m.content ILIKE $2 AND {visible}
         ORDER BY m.created_at DESC LIMIT 15"
    ))
    .bind(account_id)
    .bind(&pat)
    .bind(au.is_admin())
    .bind(user.id)
    .fetch_all(&st.db)
    .await?;
    let convs: Vec<(i64, NaiveDateTime, i64, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(&format!(
        "SELECT DISTINCT ON (c.id) c.display_id::bigint, c.created_at, c.inbox_id::bigint, ct.name, ct.email, i.name
         FROM conversations c JOIN messages m ON m.conversation_id = c.id
         LEFT JOIN contacts ct ON ct.id = c.contact_id LEFT JOIN inboxes i ON i.id = c.inbox_id
         WHERE c.account_id = $1 AND m.account_id = $1 AND m.message_type IN (0, 1)
           AND (m.content ILIKE $2 OR ct.name ILIKE $2 OR ct.email ILIKE $2) AND {visible}
         ORDER BY c.id DESC LIMIT 15"
    ))
    .bind(account_id)
    .bind(&pat)
    .bind(au.is_admin())
    .bind(user.id)
    .fetch_all(&st.db)
    .await?;
    let mut contact_rows = Vec::with_capacity(contacts.len());
    for c in &contacts {
        contact_rows.push(json!({
            "id": c.0, "name": c.1, "email": c.2, "phone_number": c.3, "identifier": c.4,
            "thumbnail": crate::storage::avatar_url(&st.db, "Contact", c.0 as i64).await,
            "additional_attributes": {}, "last_activity_at": ts(c.5),
        }));
    }
    Ok(Json(json!({ "payload": {
        "contacts": contact_rows,
        "messages": messages.iter().map(|m| json!({
            "id": m.0, "content": m.1, "message_type": m.2, "conversation_id": m.3, "inbox_id": m.4,
            "created_at": m.5.and_utc().timestamp(), "private": m.6, "sender_type": m.7, "sender_id": m.8,
            "content_type": "text", "content_attributes": {}, "attachments": [],
        })).collect::<Vec<_>>(),
        "conversations": convs.iter().map(|c| json!({
            "id": c.0, "account_id": account_id, "created_at": c.1.and_utc().timestamp(),
            "contact": { "name": c.3, "email": c.4 }, "inbox": { "id": c.2, "name": c.5 }, "message": {}, "agent": {},
        })).collect::<Vec<_>>(),
        "articles": [],
    } })))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_flags_roundtrip() {
        let names = flags_to_names("email", 0b101);
        assert_eq!(names, vec!["email_conversation_creation", "email_assigned_conversation_new_message"]);
        assert_eq!(names_to_flags("email", &json!(names)), Some(0b101));
        assert_eq!(enum_val(&json!("contact_attribute"), &ATTR_MODELS), Some(1));
        assert_eq!(enum_val(&json!("1"), &ATTR_MODELS), Some(1));
    }
}
