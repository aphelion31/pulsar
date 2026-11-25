//! Conversation Monitors (AI quality / rule monitoring over conversations).
//! Mirrors chatwoot/enterprise/app/controllers/api/v1/accounts/monitors_controller.rb.
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

use crate::common::*;

const MAX_MONITORS: i64 = 10;
const MAX_DAILY_USAGE: i64 = 500;
const PAGE_SIZE: i64 = 25;

static PREVIEW_CACHE: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();

fn preview_cache() -> &'static Mutex<HashMap<String, Value>> {
    PREVIEW_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/monitors", get(index).post(create))
        .route("/monitors/preview", post(preview))
        .route("/monitors/preview/{token}", get(preview_status))
        .route(
            "/monitors/{id}",
            get(show).patch(update).put(update).delete(destroy),
        )
        .route("/monitors/{id}/timeseries", get(timeseries))
        .route("/monitors/{id}/conversations", get(conversations))
        .route("/monitors/{id}/retry_evaluations", post(retry_evaluations))
        .route("/monitors/{id}/resume", post(resume))
}

fn sopt(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

async fn usage_snapshot(db: &PgPool, account_id: i64) -> Value {
    let used: i64 = sqlx::query_scalar(
        "SELECT COALESCE(calls_count, 0)::bigint FROM conversation_monitor_daily_usages
         WHERE account_id = $1 AND usage_date = CURRENT_DATE",
    )
    .bind(account_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .unwrap_or(0);
    let remaining = (MAX_DAILY_USAGE - used).max(0);
    json!({
        "used": used,
        "limit": MAX_DAILY_USAGE,
        "remaining": remaining,
    })
}

async fn monitor_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT id, name, condition, icon, icon_color, slack_channel_id,
                data_revision, collection_version,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM history_since)::bigint AS history_since,
                EXTRACT(EPOCH FROM paused_at)::bigint AS paused_at,
                recheck_requested_at
         FROM conversation_monitors
         WHERE id = $1 AND account_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let paused_at: Option<i64> = r.try_get("paused_at")?;
    let recent_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_monitor_evaluations
         WHERE monitor_id = $1 AND account_id = $2 AND matched_at IS NOT NULL
           AND matched_at >= COALESCE((SELECT paused_at FROM conversation_monitors WHERE id = $1), now()) - interval '7 days'",
    )
    .bind(id)
    .bind(account_id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let pending_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_monitor_evaluations WHERE monitor_id = $1 AND status = 'pending'",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let error_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_monitor_evaluations
         WHERE monitor_id = $1 AND status = 'error' AND COALESCE(error_code, '') <> 'no_text'",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let recheck: Option<chrono::NaiveDateTime> = r.try_get("recheck_requested_at")?;
    let state = if paused_at.is_some() {
        "paused"
    } else if recheck.is_some() {
        "rechecking"
    } else if error_count > 0 {
        "needs_attention"
    } else if pending_count > 0 {
        "processing"
    } else {
        "live"
    };

    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "condition": r.try_get::<String, _>("condition")?,
        "icon": r.try_get::<String, _>("icon")?,
        "icon_color": r.try_get::<String, _>("icon_color")?,
        "slack_channel_id": r.try_get::<Option<String>, _>("slack_channel_id")?,
        "created_at": r.try_get::<i64, _>("created_at")?,
        "history_since": r.try_get::<i64, _>("history_since")?,
        "data_revision": r.try_get::<i64, _>("data_revision")?,
        "collection_version": r.try_get::<i64, _>("collection_version")?,
        "paused_at": paused_at,
        "recent_count": recent_count,
        "processing": {
            "state": state,
            "errors": error_count,
            "error_codes": [],
        },
    }))
}

#[derive(Deserialize, Default)]
struct IndexQuery {
    active: Option<String>,
}

async fn index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<IndexQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let active_only = match q.active.as_deref() {
        None => false,
        Some("true") => true,
        Some(_) => return Err(ApiError::bad("invalid_parameters")),
    };

    let ids: Vec<i64> = if active_only {
        sqlx::query_scalar(
            "SELECT id FROM conversation_monitors WHERE account_id = $1 AND deleted_at IS NULL AND paused_at IS NULL ORDER BY id DESC",
        )
        .bind(account_id)
        .fetch_all(&st.db)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT id FROM conversation_monitors WHERE account_id = $1 AND deleted_at IS NULL ORDER BY id DESC",
        )
        .bind(account_id)
        .fetch_all(&st.db)
        .await?
    };

    let mut payload = Vec::with_capacity(ids.len());
    for id in ids {
        payload.push(monitor_json(&st.db, account_id, id).await?);
    }
    Ok(Json(json!({
        "payload": payload,
        "usage": usage_snapshot(&st.db, account_id).await,
    })))
}

async fn show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(monitor_json(&st.db, account_id, id).await?))
}

async fn evaluate_conversation_text(condition: &str, text: &str) -> bool {
    let cond_lower = condition.to_ascii_lowercase();
    let text_lower = text.to_ascii_lowercase();
    cond_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| {
            w.len() >= 3 && !matches!(*w, "and" | "the" | "with" | "for" | "that" | "are" | "when")
        })
        .any(|kw| text_lower.contains(kw))
}

async fn create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_monitors WHERE account_id = $1 AND deleted_at IS NULL AND paused_at IS NULL",
    )
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;
    if active_count >= MAX_MONITORS {
        return Err(ApiError::bad("limit_reached"));
    }

    let name = sopt(&body, "name").ok_or_else(|| ApiError::bad("invalid_parameters"))?;
    let condition = sopt(&body, "condition").ok_or_else(|| ApiError::bad("invalid_parameters"))?;
    let icon = sopt(&body, "icon").unwrap_or_else(|| "chat-3-line".into());
    let icon_color = sopt(&body, "icon_color").unwrap_or_else(|| "#3B82F6".into());

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversation_monitors
            (account_id, user_id, name, condition, model, threshold, history_since, icon, icon_color, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'gpt-4o-mini', 0.7, now() - interval '30 days', $5, $6, now(), now())
         RETURNING id",
    )
    .bind(account_id)
    .bind(user.id)
    .bind(&name)
    .bind(&condition)
    .bind(&icon)
    .bind(&icon_color)
    .fetch_one(&st.db)
    .await?;

    // Run initial lightweight keyword evaluation against recent conversations so monitors show results immediately.
    let convs = sqlx::query(
        "SELECT c.id::bigint AS id, COALESCE(string_agg(m.content, ' '), '') AS text_body
         FROM conversations c
         LEFT JOIN messages m ON m.conversation_id = c.id AND m.content IS NOT NULL
         WHERE c.account_id = $1
         GROUP BY c.id ORDER BY c.id DESC LIMIT 50",
    )
    .bind(account_id)
    .fetch_all(&st.db)
    .await
    .unwrap_or_default();

    for c in convs {
        let cid: i64 = c.try_get("id").unwrap_or(0);
        let text: String = c.try_get("text_body").unwrap_or_default();
        let matched = evaluate_conversation_text(&condition, &text).await;
        let _ = sqlx::query(
            "INSERT INTO conversation_monitor_evaluations
                (account_id, monitor_id, conversation_id, status, model, score, matched_at, evaluated_at, first_matched_at, created_at, updated_at)
             VALUES ($1, $2, $3, 'complete', 'keyword-fallback', $4, CASE WHEN $5 THEN now() ELSE NULL END, now(), CASE WHEN $5 THEN now() ELSE NULL END, now(), now())
             ON CONFLICT (monitor_id, conversation_id) DO NOTHING",
        )
        .bind(account_id)
        .bind(id)
        .bind(cid)
        .bind(if matched { 0.9 } else { 0.1 })
        .bind(matched)
        .execute(&st.db)
        .await;
    }

    Ok((
        StatusCode::CREATED,
        Json(monitor_json(&st.db, account_id, id).await?),
    ))
}

async fn update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let cur = monitor_json(&st.db, account_id, id).await?;

    let name = sopt(&body, "name").unwrap_or_else(|| cur["name"].as_str().unwrap_or("").into());
    let condition =
        sopt(&body, "condition").unwrap_or_else(|| cur["condition"].as_str().unwrap_or("").into());
    let icon =
        sopt(&body, "icon").unwrap_or_else(|| cur["icon"].as_str().unwrap_or("chat-3-line").into());
    let icon_color = sopt(&body, "icon_color")
        .unwrap_or_else(|| cur["icon_color"].as_str().unwrap_or("#3B82F6").into());
    let slack_channel_id = if body.get("slack_channel_id").is_some() {
        sopt(&body, "slack_channel_id")
    } else {
        cur["slack_channel_id"].as_str().map(str::to_string)
    };
    let set_paused = body.get("paused").and_then(Value::as_bool) == Some(true);
    let bump_revision = sopt(&body, "condition").is_some();

    sqlx::query(
        "UPDATE conversation_monitors
         SET name = $1, condition = $2, icon = $3, icon_color = $4, slack_channel_id = $5,
             paused_at = CASE WHEN $6 THEN COALESCE(paused_at, now()) ELSE paused_at END,
             data_revision = data_revision + CASE WHEN $7 THEN 1 ELSE 0 END,
             updated_at = now()
         WHERE id = $8 AND account_id = $9 AND deleted_at IS NULL",
    )
    .bind(&name)
    .bind(&condition)
    .bind(&icon)
    .bind(&icon_color)
    .bind(&slack_channel_id)
    .bind(set_paused)
    .bind(bump_revision)
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    Ok(Json(monitor_json(&st.db, account_id, id).await?))
}

async fn destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query(
        "UPDATE conversation_monitors SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND account_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn preview(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let condition = sopt(&body, "condition").ok_or_else(|| ApiError::bad("invalid_parameters"))?;
    let convs = sqlx::query(
        "SELECT c.id::bigint AS id, c.display_id::bigint AS display_id, c.status,
                COALESCE(ct.name, '') AS contact_name,
                COALESCE(i.name, '') AS inbox_name,
                COALESCE(string_agg(m.content, ' '), '') AS text_body
         FROM conversations c
         LEFT JOIN contacts ct ON ct.id = c.contact_id
         LEFT JOIN inboxes i ON i.id = c.inbox_id
         LEFT JOIN messages m ON m.conversation_id = c.id AND m.content IS NOT NULL
         WHERE c.account_id = $1
         GROUP BY c.id, ct.name, i.name ORDER BY c.id DESC LIMIT 25",
    )
    .bind(account_id)
    .fetch_all(&st.db)
    .await?;

    let mut matched_payload = Vec::new();
    for c in convs {
        let text: String = c.try_get("text_body").unwrap_or_default();
        if evaluate_conversation_text(&condition, &text).await {
            matched_payload.push(json!({
                "id": c.try_get::<i64, _>("display_id")?,
                "conversation_id": c.try_get::<i64, _>("id")?,
                "status": c.try_get::<i32, _>("status")?,
                "contact": { "name": c.try_get::<String, _>("contact_name")? },
                "inbox": { "name": c.try_get::<String, _>("inbox_name")? },
            }));
        }
    }

    let token = uuid::Uuid::new_v4().to_string();
    let result = json!({
        "token": token,
        "account_id": account_id,
        "status": "complete",
        "matched_count": matched_payload.len(),
        "payload": matched_payload,
    });
    if let Ok(mut map) = preview_cache().lock() {
        map.insert(format!("{account_id}:{token}"), result.clone());
    }

    Ok((StatusCode::ACCEPTED, Json(result)))
}

async fn preview_status(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, token)): Path<(i64, String)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let cached = preview_cache()
        .lock()
        .ok()
        .and_then(|map| map.get(&format!("{account_id}:{token}")).cloned())
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(cached))
}

#[derive(Deserialize, Default)]
struct MonitorQuery {
    since: Option<i64>,
    until: Option<i64>,
    page: Option<i64>,
}

async fn timeseries(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<MonitorQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    monitor_json(&st.db, account_id, id).await?;

    let rows = sqlx::query(
        "SELECT date_trunc('day', matched_at)::date::text AS day,
                EXTRACT(EPOCH FROM date_trunc('day', matched_at))::bigint AS ts,
                COUNT(*) AS value
         FROM conversation_monitor_evaluations
         WHERE monitor_id = $1 AND account_id = $2 AND matched_at IS NOT NULL
           AND ($3::bigint IS NULL OR matched_at >= to_timestamp($3::bigint))
           AND ($4::bigint IS NULL OR matched_at <= to_timestamp($4::bigint))
         GROUP BY 1, 2 ORDER BY 2 ASC",
    )
    .bind(id)
    .bind(account_id)
    .bind(q.since)
    .bind(q.until)
    .fetch_all(&st.db)
    .await?;

    let data: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "timestamp": r.try_get::<i64, _>("ts").ok()?,
                "date": r.try_get::<String, _>("day").ok()?,
                "value": r.try_get::<i64, _>("value").ok()?,
            }))
        })
        .collect();
    Ok(Json(Value::Array(data)))
}

async fn conversations(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<MonitorQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    monitor_json(&st.db, account_id, id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_monitor_evaluations e
         WHERE e.monitor_id = $1 AND e.account_id = $2 AND e.matched_at IS NOT NULL",
    )
    .bind(id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    let rows = sqlx::query(
        "SELECT c.id::bigint AS conv_id, c.display_id::bigint AS display_id, c.status,
                COALESCE(ct.name, '') AS contact_name, COALESCE(i.name, '') AS inbox_name,
                e.score, EXTRACT(EPOCH FROM e.matched_at)::bigint AS matched_at
         FROM conversation_monitor_evaluations e
         JOIN conversations c ON c.id = e.conversation_id AND c.account_id = e.account_id
         LEFT JOIN contacts ct ON ct.id = c.contact_id
         LEFT JOIN inboxes i ON i.id = c.inbox_id
         WHERE e.monitor_id = $1 AND e.account_id = $2 AND e.matched_at IS NOT NULL
         ORDER BY e.matched_at DESC, c.id DESC
         LIMIT $3 OFFSET $4",
    )
    .bind(id)
    .bind(account_id)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("display_id").ok()?,
                "conversation_id": r.try_get::<i64, _>("conv_id").ok()?,
                "status": r.try_get::<i32, _>("status").ok()?,
                "score": r.try_get::<Option<f64>, _>("score").ok()?,
                "matched_at": r.try_get::<Option<i64>, _>("matched_at").ok()?,
                "contact": { "name": r.try_get::<String, _>("contact_name").ok()? },
                "inbox": { "name": r.try_get::<String, _>("inbox_name").ok()? },
            }))
        })
        .collect();

    Ok(Json(json!({
        "payload": payload,
        "meta": {
            "total_count": total,
            "current_page": page,
        },
    })))
}

async fn retry_evaluations(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let m = monitor_json(&st.db, account_id, id).await?;
    if !m["paused_at"].is_null() {
        return Err(ApiError::bad("monitor_paused"));
    }
    sqlx::query(
        "UPDATE conversation_monitors SET recheck_requested_at = NULL, updated_at = now() WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;
    Ok(StatusCode::ACCEPTED)
}

async fn resume(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let _mode = body["mode"].as_str().unwrap_or("continue");
    let n = sqlx::query(
        "UPDATE conversation_monitors SET paused_at = NULL, resumed_at = now(), updated_at = now()
         WHERE id = $1 AND account_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(monitor_json(&st.db, account_id, id).await?))
}
