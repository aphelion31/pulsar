//! Agent Capacity Policies: CRUD, assigned agents, and per-inbox conversation limits.
//! Mirrors chatwoot/enterprise/app/controllers/api/v1/accounts/agent_capacity_policies_controller.rb
//! and agent_capacity_policies/{users,inbox_limits}_controller.rb.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use chrono::NaiveDateTime;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    auth::{availability_name, role_name},
    common::{ApiError, ApiResult, AppState, CurrentUser, require_account, ts},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/agent_capacity_policies", get(index).post(create))
        .route(
            "/agent_capacity_policies/{id}",
            get(show).patch(update).put(update).delete(destroy),
        )
        .route(
            "/agent_capacity_policies/{id}/users",
            get(users_index).post(users_create),
        )
        .route(
            "/agent_capacity_policies/{id}/users/{user_id}",
            delete(users_destroy),
        )
        .route(
            "/agent_capacity_policies/{id}/inbox_limits",
            post(inbox_limits_create),
        )
        .route(
            "/agent_capacity_policies/{id}/inbox_limits/{limit_id}",
            patch(inbox_limits_update)
                .put(inbox_limits_update)
                .delete(inbox_limits_destroy),
        )
}

#[derive(sqlx::FromRow)]
struct CapacityPolicyRow {
    id: i64,
    account_id: i64,
    name: String,
    description: Option<String>,
    exclusion_rules: Value,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

#[derive(sqlx::FromRow)]
struct InboxLimitRow {
    id: i64,
    inbox_id: i64,
    conversation_limit: i32,
}

fn pick(body: &Value) -> &Value {
    body.get("agent_capacity_policy")
        .filter(|v| v.is_object())
        .unwrap_or(body)
}

fn normalize_exclusion_rules(v: &Value) -> Result<Value, ApiError> {
    if v.is_null() {
        return Ok(json!({}));
    }
    let Some(obj) = v.as_object() else {
        return Err(ApiError::bad("exclusion_rules must be an object"));
    };
    let mut out = serde_json::Map::new();
    if let Some(hours) = obj.get("exclude_older_than_hours")
        && !hours.is_null()
    {
        let h = hours
            .as_i64()
            .or_else(|| hours.as_str().and_then(|s| s.trim().parse().ok()))
            .ok_or_else(|| ApiError::bad("exclude_older_than_hours must be an integer"))?;
        if h < 0 {
            return Err(ApiError::bad(
                "exclude_older_than_hours must be non-negative",
            ));
        }
        out.insert("exclude_older_than_hours".into(), json!(h));
    }
    if let Some(labels) = obj.get("excluded_labels")
        && !labels.is_null()
    {
        let arr = labels
            .as_array()
            .ok_or_else(|| ApiError::bad("excluded_labels must be an array"))?;
        let cleaned: Vec<String> = arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        out.insert("excluded_labels".into(), json!(cleaned));
    }
    Ok(Value::Object(out))
}

async fn policy_json(db: &PgPool, p: &CapacityPolicyRow) -> Result<Value, ApiError> {
    let assigned_agent_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM account_users WHERE account_id = $1 AND agent_capacity_policy_id = $2",
    )
    .bind(p.account_id)
    .bind(p.id)
    .fetch_one(db)
    .await?;

    let limits: Vec<InboxLimitRow> = sqlx::query_as(
        "SELECT id, inbox_id, conversation_limit FROM inbox_capacity_limits WHERE agent_capacity_policy_id = $1 ORDER BY id",
    )
    .bind(p.id)
    .fetch_all(db)
    .await?;

    let limits_json: Vec<Value> = limits
        .into_iter()
        .map(|l| {
            json!({
                "id": l.id,
                "inbox_id": l.inbox_id,
                "conversation_limit": l.conversation_limit,
            })
        })
        .collect();

    Ok(json!({
        "id": p.id,
        "name": p.name,
        "description": p.description,
        "exclusion_rules": p.exclusion_rules,
        "created_at": ts(Some(p.created_at)),
        "updated_at": ts(Some(p.updated_at)),
        "account_id": p.account_id,
        "assigned_agent_count": assigned_agent_count,
        "inbox_capacity_limits": limits_json,
    }))
}

async fn load_policy(db: &PgPool, account_id: i64, id: i64) -> Result<CapacityPolicyRow, ApiError> {
    sqlx::query_as(
        "SELECT id, account_id, name, description, exclusion_rules, created_at, updated_at
         FROM agent_capacity_policies WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)
}

async fn index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let rows: Vec<CapacityPolicyRow> = sqlx::query_as(
        "SELECT id, account_id, name, description, exclusion_rules, created_at, updated_at
         FROM agent_capacity_policies WHERE account_id = $1 ORDER BY id",
    )
    .bind(account_id)
    .fetch_all(&st.db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        out.push(policy_json(&st.db, r).await?);
    }
    Ok(Json(Value::Array(out)))
}

async fn show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let row = load_policy(&st.db, account_id, id).await?;
    Ok(Json(policy_json(&st.db, &row).await?))
}

async fn create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let p = pick(&body);
    let name = p["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Name can't be blank"))?;
    let description = p["description"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let exclusion_rules = normalize_exclusion_rules(&p["exclusion_rules"])?;

    let row: CapacityPolicyRow = sqlx::query_as(
        "INSERT INTO agent_capacity_policies (account_id, name, description, exclusion_rules, created_at, updated_at)
         VALUES ($1, $2, $3, $4, now(), now())
         RETURNING id, account_id, name, description, exclusion_rules, created_at, updated_at",
    )
    .bind(account_id)
    .bind(name)
    .bind(description)
    .bind(exclusion_rules)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(policy_json(&st.db, &row).await?))
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
    let existing = load_policy(&st.db, account_id, id).await?;
    let p = pick(&body);
    let obj = p.as_object();
    let name = match p.get("name") {
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                return Err(ApiError::bad("Name can't be blank"));
            }
            t.to_string()
        }
        Some(_) => return Err(ApiError::bad("Name is invalid")),
        None => existing.name,
    };
    let description = if obj.is_some_and(|m| m.contains_key("description")) {
        p["description"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    } else {
        existing.description
    };
    let exclusion_rules = if obj.is_some_and(|m| m.contains_key("exclusion_rules")) {
        normalize_exclusion_rules(&p["exclusion_rules"])?
    } else {
        existing.exclusion_rules
    };

    let row: CapacityPolicyRow = sqlx::query_as(
        "UPDATE agent_capacity_policies
         SET name = $1, description = $2, exclusion_rules = $3, updated_at = now()
         WHERE id = $4 AND account_id = $5
         RETURNING id, account_id, name, description, exclusion_rules, created_at, updated_at",
    )
    .bind(name)
    .bind(description)
    .bind(exclusion_rules)
    .bind(id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(policy_json(&st.db, &row).await?))
}

async fn destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult<StatusCode> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let mut tx = st.db.begin().await?;
    sqlx::query(
        "UPDATE account_users SET agent_capacity_policy_id = NULL, updated_at = now()
         WHERE account_id = $1 AND agent_capacity_policy_id = $2",
    )
    .bind(account_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM inbox_capacity_limits WHERE agent_capacity_policy_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let n = sqlx::query("DELETE FROM agent_capacity_policies WHERE id = $1 AND account_id = $2")
        .bind(id)
        .bind(account_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    tx.commit().await?;
    Ok(StatusCode::OK)
}

// ---------- users ----------

#[derive(sqlx::FromRow)]
struct AgentRow {
    id: i64,
    account_id: i64,
    name: String,
    available_name: Option<String>,
    email: String,
    confirmed: bool,
    role: i32,
    availability: i32,
    auto_offline: bool,
    custom_role_id: Option<i64>,
    agent_capacity_policy_id: Option<i64>,
}

fn agent_json(a: &AgentRow) -> Value {
    json!({
        "id": a.id,
        "account_id": a.account_id,
        "availability_status": availability_name(a.availability),
        "auto_offline": a.auto_offline,
        "confirmed": a.confirmed,
        "email": a.email,
        "available_name": a.available_name.as_deref().unwrap_or(&a.name),
        "name": a.name,
        "role": role_name(a.role),
        "custom_role_id": a.custom_role_id,
        "agent_capacity_policy_id": a.agent_capacity_policy_id,
        "thumbnail": "",
    })
}

async fn users_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, policy_id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    load_policy(&st.db, account_id, policy_id).await?;
    let rows: Vec<AgentRow> = sqlx::query_as(
        "SELECT u.id::bigint AS id, au.account_id, u.name, u.display_name AS available_name,
                COALESCE(u.email, '') AS email, (u.confirmed_at IS NOT NULL) AS confirmed,
                COALESCE(au.role, 0) AS role, au.availability, au.auto_offline,
                au.custom_role_id, au.agent_capacity_policy_id
         FROM users u
         JOIN account_users au ON au.user_id = u.id
         WHERE au.account_id = $1 AND au.agent_capacity_policy_id = $2
         ORDER BY u.id",
    )
    .bind(account_id)
    .bind(policy_id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(Value::Array(rows.iter().map(agent_json).collect())))
}

async fn users_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, policy_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    load_policy(&st.db, account_id, policy_id).await?;
    let user_id = body["user_id"]
        .as_i64()
        .or_else(|| body["user_id"].as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| ApiError::bad("user_id is required"))?;

    let updated = sqlx::query(
        "UPDATE account_users SET agent_capacity_policy_id = $3, updated_at = now()
         WHERE account_id = $1 AND user_id = $2",
    )
    .bind(account_id)
    .bind(user_id)
    .bind(policy_id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(ApiError::not_found());
    }

    let row: AgentRow = sqlx::query_as(
        "SELECT u.id::bigint AS id, au.account_id, u.name, u.display_name AS available_name,
                COALESCE(u.email, '') AS email, (u.confirmed_at IS NOT NULL) AS confirmed,
                COALESCE(au.role, 0) AS role, au.availability, au.auto_offline,
                au.custom_role_id, au.agent_capacity_policy_id
         FROM users u
         JOIN account_users au ON au.user_id = u.id
         WHERE au.account_id = $1 AND au.user_id = $2",
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(agent_json(&row)))
}

async fn users_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, policy_id, user_id)): Path<(i64, i64, i64)>,
) -> ApiResult<StatusCode> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    load_policy(&st.db, account_id, policy_id).await?;
    let n = sqlx::query(
        "UPDATE account_users SET agent_capacity_policy_id = NULL, updated_at = now()
         WHERE account_id = $1 AND user_id = $2 AND agent_capacity_policy_id = $3",
    )
    .bind(account_id)
    .bind(user_id)
    .bind(policy_id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::OK)
}

// ---------- inbox limits ----------

async fn inbox_limits_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, policy_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    load_policy(&st.db, account_id, policy_id).await?;
    let inbox_id = body["inbox_id"]
        .as_i64()
        .or_else(|| body["inbox_id"].as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| ApiError::bad("inbox_id is required"))?;
    let limit = body["conversation_limit"]
        .as_i64()
        .or_else(|| {
            body["conversation_limit"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .ok_or_else(|| ApiError::bad("conversation_limit is required"))?;
    if limit < 0 {
        return Err(ApiError::bad("conversation_limit must be non-negative"));
    }

    let inbox_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inboxes WHERE id::bigint = $1 AND account_id::bigint = $2)",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;
    if !inbox_exists {
        return Err(ApiError::not_found());
    }

    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inbox_capacity_limits WHERE agent_capacity_policy_id = $1 AND inbox_id = $2)",
    )
    .bind(policy_id)
    .bind(inbox_id)
    .fetch_one(&st.db)
    .await?;
    if dup {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Inbox already assigned to this capacity policy".into(),
        ));
    }

    let row: (i64, i64, i64, i32, NaiveDateTime, NaiveDateTime) = sqlx::query_as(
        "INSERT INTO inbox_capacity_limits
         (agent_capacity_policy_id, inbox_id, conversation_limit, created_at, updated_at)
         VALUES ($1, $2, $3, now(), now())
         RETURNING id, inbox_id, agent_capacity_policy_id, conversation_limit, created_at, updated_at",
    )
    .bind(policy_id)
    .bind(inbox_id)
    .bind(limit as i32)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(json!({
        "id": row.0,
        "inbox_id": row.1,
        "agent_capacity_policy_id": row.2,
        "conversation_limit": row.3,
        "created_at": ts(Some(row.4)),
        "updated_at": ts(Some(row.5)),
    })))
}

async fn inbox_limits_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, policy_id, limit_id)): Path<(i64, i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    load_policy(&st.db, account_id, policy_id).await?;
    let limit = body["conversation_limit"]
        .as_i64()
        .or_else(|| {
            body["conversation_limit"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .ok_or_else(|| ApiError::bad("conversation_limit is required"))?;
    if limit < 0 {
        return Err(ApiError::bad("conversation_limit must be non-negative"));
    }

    let row: (i64, i64, i64, i32, NaiveDateTime, NaiveDateTime) = sqlx::query_as(
        "UPDATE inbox_capacity_limits
         SET conversation_limit = $3, updated_at = now()
         WHERE id = $1 AND agent_capacity_policy_id = $2
         RETURNING id, inbox_id, agent_capacity_policy_id, conversation_limit, created_at, updated_at",
    )
    .bind(limit_id)
    .bind(policy_id)
    .bind(limit as i32)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let inbox_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM inboxes WHERE id::bigint = $1")
            .bind(row.1)
            .fetch_optional(&st.db)
            .await?;

    Ok(Json(json!({
        "id": row.0,
        "inbox_id": row.1,
        "inbox_name": inbox_name,
        "agent_capacity_policy_id": row.2,
        "conversation_limit": row.3,
        "created_at": ts(Some(row.4)),
        "updated_at": ts(Some(row.5)),
    })))
}

async fn inbox_limits_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, policy_id, limit_id)): Path<(i64, i64, i64)>,
) -> ApiResult<StatusCode> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    load_policy(&st.db, account_id, policy_id).await?;
    let n = sqlx::query(
        "DELETE FROM inbox_capacity_limits WHERE id = $1 AND agent_capacity_policy_id = $2",
    )
    .bind(limit_id)
    .bind(policy_id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_exclusion_rules() {
        let out = normalize_exclusion_rules(&json!({
            "exclude_older_than_hours": 24,
            "excluded_labels": ["vip", " ", "billing"]
        }))
        .unwrap();
        assert_eq!(out["exclude_older_than_hours"], 24);
        assert_eq!(out["excluded_labels"], json!(["vip", "billing"]));
        assert!(normalize_exclusion_rules(&json!({"exclude_older_than_hours": -1})).is_err());
    }
}
