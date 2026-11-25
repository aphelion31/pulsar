//! SLA Policies, Applied SLAs, and SLA breach evaluation.
//! Mirrors chatwoot/enterprise/app/controllers/api/v1/accounts/{sla_policies,applied_slas}_controller.rb
//! and enterprise/app/services/sla/evaluate_applied_sla_service.rb.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, RawQuery, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{Datelike, Duration, NaiveDateTime, TimeZone, Utc};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

use crate::{
    common::{ApiError, ApiResult, AppState, CurrentUser, require_account, ts},
    conversations,
};

const RESULTS_PER_PAGE: i64 = 25;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sla_policies", get(policies_index).post(policies_create))
        .route(
            "/sla_policies/{id}",
            get(policies_show)
                .patch(policies_update)
                .put(policies_update)
                .delete(policies_destroy),
        )
        .route("/applied_slas", get(applied_index))
        .route("/applied_slas/metrics", get(applied_metrics))
        .route("/applied_slas/download", get(applied_download))
}

#[derive(sqlx::FromRow, Clone)]
pub(crate) struct SlaPolicy {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub first_response_time_threshold: Option<f64>,
    pub next_response_time_threshold: Option<f64>,
    pub resolution_time_threshold: Option<f64>,
    pub only_during_business_hours: bool,
}

impl SlaPolicy {
    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "first_response_time_threshold": self.first_response_time_threshold,
            "next_response_time_threshold": self.next_response_time_threshold,
            "resolution_time_threshold": self.resolution_time_threshold,
            "only_during_business_hours": self.only_during_business_hours,
        })
    }
}

const SLA_POLICY_COLS: &str = "id::bigint AS id, name, description,
    first_response_time_threshold, next_response_time_threshold, resolution_time_threshold,
    COALESCE(only_during_business_hours, false) AS only_during_business_hours";

fn pick_policy(body: &Value) -> &Value {
    body.get("sla_policy")
        .filter(|v| v.is_object())
        .unwrap_or(body)
}

fn opt_threshold(v: &Value, key: &str) -> Result<Option<f64>, ApiError> {
    match &v[key] {
        Value::Null => Ok(None),
        Value::Number(n) => {
            let f = n
                .as_f64()
                .ok_or_else(|| ApiError::bad("Invalid threshold"))?;
            if f <= 0.0 {
                return Err(ApiError::bad("Threshold must be positive"));
            }
            Ok(Some(f))
        }
        Value::String(s) if s.trim().is_empty() => Ok(None),
        Value::String(s) => {
            let f: f64 = s
                .trim()
                .parse()
                .map_err(|_| ApiError::bad("Invalid threshold"))?;
            if f <= 0.0 {
                return Err(ApiError::bad("Threshold must be positive"));
            }
            Ok(Some(f))
        }
        _ => Err(ApiError::bad("Invalid threshold")),
    }
}

pub(crate) fn sla_status_name(status: i32) -> &'static str {
    match status {
        1 => "hit",
        2 => "missed",
        3 => "active_with_misses",
        _ => "active",
    }
}

pub(crate) fn sla_event_type_name(t: i32) -> &'static str {
    match t {
        0 => "frt",
        1 => "nrt",
        _ => "rt",
    }
}

async fn policies_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let rows: Vec<SlaPolicy> = sqlx::query_as(&format!(
        "SELECT {SLA_POLICY_COLS} FROM sla_policies WHERE account_id = $1 ORDER BY id"
    ))
    .bind(account_id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(
        json!({ "payload": rows.iter().map(SlaPolicy::json).collect::<Vec<_>>() }),
    ))
}

async fn policies_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let row: SlaPolicy = sqlx::query_as(&format!(
        "SELECT {SLA_POLICY_COLS} FROM sla_policies WHERE id = $1 AND account_id = $2"
    ))
    .bind(id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Json(json!({ "payload": row.json() })))
}

async fn policies_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let p = pick_policy(&body);
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
    let frt = opt_threshold(p, "first_response_time_threshold")?;
    let nrt = opt_threshold(p, "next_response_time_threshold")?;
    let rt = opt_threshold(p, "resolution_time_threshold")?;
    let only_bh = p["only_during_business_hours"].as_bool().unwrap_or(false);

    let row: SlaPolicy = sqlx::query_as(&format!(
        "INSERT INTO sla_policies
         (name, description, first_response_time_threshold, next_response_time_threshold,
          resolution_time_threshold, only_during_business_hours, account_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())
         RETURNING {SLA_POLICY_COLS}"
    ))
    .bind(name)
    .bind(description)
    .bind(frt)
    .bind(nrt)
    .bind(rt)
    .bind(only_bh)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(json!({ "payload": row.json() })))
}

async fn policies_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let existing: SlaPolicy = sqlx::query_as(&format!(
        "SELECT {SLA_POLICY_COLS} FROM sla_policies WHERE id = $1 AND account_id = $2"
    ))
    .bind(id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let p = pick_policy(&body);
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
    let frt = if obj.is_some_and(|m| m.contains_key("first_response_time_threshold")) {
        opt_threshold(p, "first_response_time_threshold")?
    } else {
        existing.first_response_time_threshold
    };
    let nrt = if obj.is_some_and(|m| m.contains_key("next_response_time_threshold")) {
        opt_threshold(p, "next_response_time_threshold")?
    } else {
        existing.next_response_time_threshold
    };
    let rt = if obj.is_some_and(|m| m.contains_key("resolution_time_threshold")) {
        opt_threshold(p, "resolution_time_threshold")?
    } else {
        existing.resolution_time_threshold
    };
    let only_bh = p["only_during_business_hours"]
        .as_bool()
        .unwrap_or(existing.only_during_business_hours);

    let row: SlaPolicy = sqlx::query_as(&format!(
        "UPDATE sla_policies
         SET name = $1, description = $2, first_response_time_threshold = $3,
             next_response_time_threshold = $4, resolution_time_threshold = $5,
             only_during_business_hours = $6, updated_at = now()
         WHERE id = $7 AND account_id = $8
         RETURNING {SLA_POLICY_COLS}"
    ))
    .bind(name)
    .bind(description)
    .bind(frt)
    .bind(nrt)
    .bind(rt)
    .bind(only_bh)
    .bind(id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(json!({ "payload": row.json() })))
}

async fn policies_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult<StatusCode> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query("DELETE FROM sla_policies WHERE id = $1 AND account_id = $2")
        .bind(id)
        .bind(account_id)
        .execute(&st.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::OK)
}

// ---------- applied SLAs ----------

#[derive(Default)]
struct AppliedFilter {
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
    inbox_id: Option<i64>,
    team_id: Option<i64>,
    sla_policy_id: Option<i64>,
    assigned_agent_id: Option<i64>,
    label_list: Option<String>,
    page: i64,
}

fn parse_ts(s: &str) -> Option<NaiveDateTime> {
    s.trim()
        .parse::<i64>()
        .ok()
        .and_then(|secs| Utc.timestamp_opt(secs, 0).single())
        .map(|dt| dt.naive_utc())
}

fn parse_applied_filter(raw_query: Option<&str>) -> AppliedFilter {
    let mut f = AppliedFilter {
        page: 1,
        ..Default::default()
    };
    let Some(raw) = raw_query else { return f };
    let mut labels = Vec::new();
    for (k, v) in url::form_urlencoded::parse(raw.as_bytes()) {
        let val = v.trim();
        if val.is_empty() {
            continue;
        }
        match k.as_ref() {
            "since" => f.since = parse_ts(val),
            "until" => f.until = parse_ts(val),
            "inbox_id" => f.inbox_id = val.parse().ok(),
            "team_id" => f.team_id = val.parse().ok(),
            "sla_policy_id" => f.sla_policy_id = val.parse().ok(),
            "assigned_agent_id" => f.assigned_agent_id = val.parse().ok(),
            "label_list" | "label_list[]" => labels.push(val.to_string()),
            "page" => f.page = val.parse::<i64>().unwrap_or(1).max(1),
            _ => {}
        }
    }
    if !labels.is_empty() {
        f.label_list = Some(labels[0].clone());
    }
    f
}

#[derive(sqlx::FromRow)]
pub(crate) struct WorkingHourRow {
    day_of_week: i32,
    closed_all_day: bool,
    open_all_day: bool,
    open_hour: Option<i32>,
    open_minutes: Option<i32>,
    close_hour: Option<i32>,
    close_minutes: Option<i32>,
}

/// Compute due timestamp in unix seconds, optionally counting only inbox working hours.
/// Falls back to simple elapsed wall-clock seconds when working hours are empty/all closed.
pub(crate) fn compute_due_at(
    start: NaiveDateTime,
    threshold_secs: Option<f64>,
    only_business_hours: bool,
    wh_by_day: &HashMap<i32, WorkingHourRow>,
) -> Option<i64> {
    let secs = threshold_secs?;
    if secs <= 0.0 {
        return None;
    }
    let total_secs = secs.round() as i64;
    if !only_business_hours || wh_by_day.is_empty() {
        return Some(
            (start + Duration::seconds(total_secs))
                .and_utc()
                .timestamp(),
        );
    }
    let any_open = wh_by_day.values().any(|w| !w.closed_all_day);
    if !any_open {
        return Some(
            (start + Duration::seconds(total_secs))
                .and_utc()
                .timestamp(),
        );
    }
    // Walk day-by-day up to 60 days consuming business seconds.
    let mut remaining = total_secs;
    let mut cursor = start;
    for _ in 0..60 {
        let dow = cursor.weekday().num_days_from_sunday() as i32;
        if let Some(wh) = wh_by_day.get(&dow)
            && !wh.closed_all_day
        {
            let day_start = cursor.date().and_hms_opt(0, 0, 0).unwrap_or(cursor);
            let (win_start, win_end) = if wh.open_all_day {
                (day_start, day_start + Duration::days(1))
            } else {
                let oh = wh.open_hour.unwrap_or(9).clamp(0, 23) as u32;
                let om = wh.open_minutes.unwrap_or(0).clamp(0, 59) as u32;
                let ch = wh.close_hour.unwrap_or(17).clamp(0, 23) as u32;
                let cm = wh.close_minutes.unwrap_or(0).clamp(0, 59) as u32;
                let s = cursor.date().and_hms_opt(oh, om, 0).unwrap_or(day_start);
                let e = cursor
                    .date()
                    .and_hms_opt(ch, cm, 0)
                    .unwrap_or(day_start + Duration::hours(17));
                (s, e)
            };
            let effective_start = cursor.max(win_start);
            if effective_start < win_end {
                let avail = (win_end - effective_start).num_seconds();
                if remaining <= avail {
                    return Some(
                        (effective_start + Duration::seconds(remaining))
                            .and_utc()
                            .timestamp(),
                    );
                }
                remaining -= avail;
            }
        }
        cursor = (cursor.date() + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap_or(cursor + Duration::days(1));
    }
    Some(
        (start + Duration::seconds(total_secs))
            .and_utc()
            .timestamp(),
    )
}

async fn load_inbox_working_hours(
    db: &PgPool,
    inbox_id: i64,
) -> Result<HashMap<i32, WorkingHourRow>, ApiError> {
    let rows: Vec<WorkingHourRow> = sqlx::query_as(
        "SELECT day_of_week, COALESCE(closed_all_day, false) AS closed_all_day,
                COALESCE(open_all_day, false) AS open_all_day,
                open_hour, open_minutes, close_hour, close_minutes
         FROM working_hours WHERE inbox_id = $1",
    )
    .bind(inbox_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.day_of_week, r)).collect())
}

#[derive(sqlx::FromRow)]
struct AppliedSlaFull {
    id: i64,
    sla_policy_id: i64,
    sla_status: i32,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    completed_at: Option<NaiveDateTime>,
    policy_name: String,
    policy_description: Option<String>,
    frt_threshold: Option<f64>,
    nrt_threshold: Option<f64>,
    rt_threshold: Option<f64>,
    only_during_business_hours: bool,
    conv_display_id: i64,
    conv_created_at: NaiveDateTime,
    conv_waiting_since: Option<NaiveDateTime>,
    conv_inbox_id: i64,
    conv_cached_label_list: Option<String>,
    contact_name: Option<String>,
    assignee_id: Option<i64>,
    assignee_name: Option<String>,
    assignee_email: Option<String>,
    inbox_name: Option<String>,
    team_name: Option<String>,
}

const APPLIED_BASE_FROM: &str = "
FROM applied_slas a
JOIN sla_policies p ON p.id = a.sla_policy_id
JOIN conversations c ON c.id = a.conversation_id
LEFT JOIN contacts ct ON ct.id = c.contact_id
LEFT JOIN users u ON u.id = c.assignee_id
LEFT JOIN inboxes i ON i.id = c.inbox_id
LEFT JOIN teams tm ON tm.id = c.team_id
WHERE a.account_id = $1
  AND ($2::timestamp IS NULL OR a.created_at >= $2)
  AND ($3::timestamp IS NULL OR a.created_at <= $3)
  AND ($4::bigint IS NULL OR c.inbox_id = $4)
  AND ($5::bigint IS NULL OR c.team_id = $5)
  AND ($6::bigint IS NULL OR a.sla_policy_id = $6)
  AND ($7::bigint IS NULL OR c.assignee_id = $7)
  AND ($8::text IS NULL OR COALESCE(c.cached_label_list, '') LIKE '%' || $8 || '%')
";

async fn sla_events_for(db: &PgPool, applied_sla_id: i64) -> Result<Vec<Value>, ApiError> {
    let rows: Vec<(
        i64,
        Option<i32>,
        Option<Value>,
        NaiveDateTime,
        NaiveDateTime,
    )> = sqlx::query_as(
        "SELECT id, event_type, meta, created_at, updated_at
             FROM sla_events WHERE applied_sla_id = $1 ORDER BY id",
    )
    .bind(applied_sla_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, event_type, meta, created_at, updated_at)| {
            json!({
                "id": id,
                "event_type": event_type.map(sla_event_type_name),
                "meta": meta.unwrap_or_else(|| json!({})),
                "created_at": ts(Some(created_at)),
                "updated_at": ts(Some(updated_at)),
            })
        })
        .collect())
}

async fn applied_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    RawQuery(raw): RawQuery,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let f = parse_applied_filter(raw.as_deref());
    let offset = (f.page - 1) * RESULTS_PER_PAGE;

    let count_sql =
        format!("SELECT COUNT(*)::bigint {APPLIED_BASE_FROM} AND a.sla_status IN (2, 3)");
    let count: i64 = sqlx::query_scalar(&count_sql)
        .bind(account_id)
        .bind(f.since)
        .bind(f.until)
        .bind(f.inbox_id)
        .bind(f.team_id)
        .bind(f.sla_policy_id)
        .bind(f.assigned_agent_id)
        .bind(&f.label_list)
        .fetch_one(&st.db)
        .await?;

    let rows_sql = format!(
        "SELECT a.id, a.sla_policy_id, COALESCE(a.sla_status, 0) AS sla_status,
                a.created_at, a.updated_at, a.completed_at,
                p.name AS policy_name, p.description AS policy_description,
                p.first_response_time_threshold AS frt_threshold,
                p.next_response_time_threshold AS nrt_threshold,
                p.resolution_time_threshold AS rt_threshold,
                COALESCE(p.only_during_business_hours, false) AS only_during_business_hours,
                c.display_id::bigint AS conv_display_id,
                c.created_at AS conv_created_at, c.waiting_since AS conv_waiting_since,
                c.inbox_id::bigint AS conv_inbox_id, c.cached_label_list AS conv_cached_label_list,
                ct.name AS contact_name,
                u.id::bigint AS assignee_id, u.name AS assignee_name, u.email AS assignee_email,
                i.name AS inbox_name, tm.name AS team_name
         {APPLIED_BASE_FROM}
         AND a.sla_status IN (2, 3)
         ORDER BY a.created_at DESC
         LIMIT $9 OFFSET $10"
    );
    let rows: Vec<AppliedSlaFull> = sqlx::query_as(&rows_sql)
        .bind(account_id)
        .bind(f.since)
        .bind(f.until)
        .bind(f.inbox_id)
        .bind(f.team_id)
        .bind(f.sla_policy_id)
        .bind(f.assigned_agent_id)
        .bind(&f.label_list)
        .bind(RESULTS_PER_PAGE)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;

    let mut wh_cache: HashMap<i64, HashMap<i32, WorkingHourRow>> = HashMap::new();
    let mut payload = Vec::with_capacity(rows.len());
    for r in rows {
        if r.only_during_business_hours && !wh_cache.contains_key(&r.conv_inbox_id) {
            wh_cache.insert(
                r.conv_inbox_id,
                load_inbox_working_hours(&st.db, r.conv_inbox_id).await?,
            );
        }
        let empty_wh = HashMap::new();
        let wh = wh_cache.get(&r.conv_inbox_id).unwrap_or(&empty_wh);
        let frt_due = compute_due_at(
            r.conv_created_at,
            r.frt_threshold,
            r.only_during_business_hours,
            wh,
        );
        let nrt_due = r
            .conv_waiting_since
            .and_then(|ws| compute_due_at(ws, r.nrt_threshold, r.only_during_business_hours, wh));
        let rt_due = compute_due_at(
            r.conv_created_at,
            r.rt_threshold,
            r.only_during_business_hours,
            wh,
        );
        let events = sla_events_for(&st.db, r.id).await?;
        let assignee = r.assignee_id.map(|uid| {
            json!({
                "id": uid,
                "name": r.assignee_name,
                "email": r.assignee_email,
            })
        });
        payload.push(json!({
            "applied_sla": {
                "id": r.id,
                "sla_id": r.sla_policy_id,
                "sla_status": sla_status_name(r.sla_status),
                "created_at": ts(Some(r.created_at)),
                "updated_at": ts(Some(r.updated_at)),
                "sla_completed_at": r.completed_at.map(|d| ts(Some(d))),
                "sla_description": r.policy_description,
                "sla_name": r.policy_name,
                "sla_first_response_time_threshold": r.frt_threshold,
                "sla_next_response_time_threshold": r.nrt_threshold,
                "sla_only_during_business_hours": r.only_during_business_hours,
                "sla_resolution_time_threshold": r.rt_threshold,
                "sla_frt_due_at": frt_due,
                "sla_nrt_due_at": nrt_due,
                "sla_rt_due_at": rt_due,
            },
            "conversation": {
                "id": r.conv_display_id,
                "contact": r.contact_name.as_ref().map(|n| json!({ "name": n })),
                "labels": r.conv_cached_label_list,
                "assignee": assignee,
            },
            "sla_events": events,
        }));
    }

    Ok(Json(json!({
        "payload": payload,
        "meta": {
            "count": count,
            "current_page": f.page,
        }
    })))
}

async fn applied_metrics(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    RawQuery(raw): RawQuery,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let f = parse_applied_filter(raw.as_deref());

    let sql = format!(
        "SELECT COUNT(*)::bigint,
                COUNT(*) FILTER (WHERE a.sla_status IN (2, 3))::bigint
         {APPLIED_BASE_FROM}"
    );
    let (total, misses): (i64, i64) = sqlx::query_as(&sql)
        .bind(account_id)
        .bind(f.since)
        .bind(f.until)
        .bind(f.inbox_id)
        .bind(f.team_id)
        .bind(f.sla_policy_id)
        .bind(f.assigned_agent_id)
        .bind(&f.label_list)
        .fetch_one(&st.db)
        .await?;

    let hit_rate = if misses == 0 || total == 0 {
        "100%".to_string()
    } else {
        let pct = ((total - misses) as f64 / total as f64) * 100.0;
        let rounded = (pct * 100.0).round() / 100.0;
        format!("{rounded}%")
    };

    Ok(Json(json!({
        "total_applied_slas": total,
        "number_of_sla_misses": misses,
        "hit_rate": hit_rate,
    })))
}

fn csv_cell(v: &str) -> String {
    let safe = if v.starts_with(['=', '+', '-', '@']) {
        format!("'{v}")
    } else {
        v.to_string()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}

async fn applied_download(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    RawQuery(raw): RawQuery,
) -> Result<Response, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let f = parse_applied_filter(raw.as_deref());

    let rows_sql = format!(
        "SELECT a.id, a.sla_policy_id, COALESCE(a.sla_status, 0) AS sla_status,
                a.created_at, a.updated_at, a.completed_at,
                p.name AS policy_name, p.description AS policy_description,
                p.first_response_time_threshold AS frt_threshold,
                p.next_response_time_threshold AS nrt_threshold,
                p.resolution_time_threshold AS rt_threshold,
                COALESCE(p.only_during_business_hours, false) AS only_during_business_hours,
                c.display_id::bigint AS conv_display_id,
                c.created_at AS conv_created_at, c.waiting_since AS conv_waiting_since,
                c.inbox_id::bigint AS conv_inbox_id, c.cached_label_list AS conv_cached_label_list,
                ct.name AS contact_name,
                u.id::bigint AS assignee_id, u.name AS assignee_name, u.email AS assignee_email,
                i.name AS inbox_name, tm.name AS team_name
         {APPLIED_BASE_FROM}
         AND a.sla_status IN (2, 3)
         ORDER BY a.created_at DESC"
    );
    let rows: Vec<AppliedSlaFull> = sqlx::query_as(&rows_sql)
        .bind(account_id)
        .bind(f.since)
        .bind(f.until)
        .bind(f.inbox_id)
        .bind(f.team_id)
        .bind(f.sla_policy_id)
        .bind(f.assigned_agent_id)
        .bind(&f.label_list)
        .fetch_all(&st.db)
        .await?;

    let mut csv = String::from(
        "Conversation ID,SLA Policy Breached,Assignee,Team,Inbox,Labels,Conversation Link,Breached Events\r\n",
    );
    let base = st.frontend_url.trim_end_matches('/');
    for r in rows {
        let events = sla_events_for(&st.db, r.id).await?;
        let event_names: Vec<&str> = events
            .iter()
            .filter_map(|e| e["event_type"].as_str())
            .collect();
        let conv_url = format!(
            "{base}/app/accounts/{account_id}/conversations/{}",
            r.conv_display_id
        );
        let cells = [
            r.conv_display_id.to_string(),
            r.policy_name,
            r.assignee_name.unwrap_or_default(),
            r.team_name.unwrap_or_default(),
            r.inbox_name.unwrap_or_default(),
            r.conv_cached_label_list.unwrap_or_default(),
            conv_url,
            event_names.join(", "),
        ];
        csv.push_str(
            &cells
                .iter()
                .map(|c| csv_cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push_str("\r\n");
    }

    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=breached_conversation.csv",
            ),
        ],
        csv,
    )
        .into_response())
}

// ---------- SLA evaluation (called by scheduler & conversation updates) ----------

pub(crate) async fn evaluate_open_slas(st: &AppState) -> anyhow::Result<()> {
    let rows = sqlx::query(
        "SELECT a.id, a.account_id, a.sla_policy_id, a.conversation_id, COALESCE(a.sla_status, 0) AS sla_status,
                p.first_response_time_threshold AS frt, p.next_response_time_threshold AS nrt,
                p.resolution_time_threshold AS rt, COALESCE(p.only_during_business_hours, false) AS only_bh,
                c.status AS conv_status, c.inbox_id::bigint AS inbox_id, c.created_at AS conv_created_at,
                c.first_reply_created_at, c.waiting_since
         FROM applied_slas a
         JOIN sla_policies p ON p.id = a.sla_policy_id
         JOIN conversations c ON c.id = a.conversation_id
         WHERE COALESCE(a.sla_status, 0) IN (0, 3)
         ORDER BY a.id
         LIMIT 200",
    )
    .fetch_all(&st.db)
    .await?;

    let now_ts = Utc::now().timestamp();
    let mut wh_cache: HashMap<i64, HashMap<i32, WorkingHourRow>> = HashMap::new();

    for r in rows {
        let id: i64 = r.try_get("id")?;
        let account_id: i64 = r.try_get("account_id")?;
        let sla_policy_id: i64 = r.try_get("sla_policy_id")?;
        let conversation_id: i64 = r.try_get("conversation_id")?;
        let mut sla_status: i32 = r.try_get("sla_status")?;
        let frt: Option<f64> = r.try_get("frt")?;
        let nrt: Option<f64> = r.try_get("nrt")?;
        let rt: Option<f64> = r.try_get("rt")?;
        let only_bh: bool = r.try_get("only_bh")?;
        let conv_status: i32 = r.try_get("conv_status")?;
        let inbox_id: i64 = r.try_get("inbox_id")?;
        let conv_created_at: NaiveDateTime = r.try_get("conv_created_at")?;
        let first_reply_created_at: Option<NaiveDateTime> = r.try_get("first_reply_created_at")?;
        let waiting_since: Option<NaiveDateTime> = r.try_get("waiting_since")?;

        if only_bh && !wh_cache.contains_key(&inbox_id) {
            wh_cache.insert(
                inbox_id,
                load_inbox_working_hours(&st.db, inbox_id)
                    .await
                    .unwrap_or_default(),
            );
        }
        let empty_wh = HashMap::new();
        let wh = wh_cache.get(&inbox_id).unwrap_or(&empty_wh);
        let old_status = sla_status;

        // FRT check (event_type = 0)
        if let Some(due) = compute_due_at(conv_created_at, frt, only_bh, wh) {
            let frt_hit = first_reply_created_at.is_some_and(|fr| ts(Some(fr)) <= due);
            if !frt_hit && now_ts >= due {
                sla_status = record_sla_miss(
                    &st.db,
                    id,
                    conversation_id,
                    account_id,
                    sla_policy_id,
                    inbox_id,
                    0,
                    json!({}),
                    sla_status,
                )
                .await?;
            }
        }

        // NRT check (event_type = 1)
        if first_reply_created_at.is_some()
            && let Some(ws) = waiting_since
            && let Some(due) = compute_due_at(ws, nrt, only_bh, wh)
            && now_ts >= due
        {
            let last_msg_id: Option<i64> = sqlx::query_scalar(
                "SELECT id::bigint FROM messages
                 WHERE account_id = $1 AND conversation_id = $2 AND message_type = 0
                 ORDER BY id DESC LIMIT 1",
            )
            .bind(account_id as i32)
            .bind(conversation_id as i32)
            .fetch_optional(&st.db)
            .await?;
            let meta = json!({ "message_id": last_msg_id });
            sla_status = record_sla_miss(
                &st.db,
                id,
                conversation_id,
                account_id,
                sla_policy_id,
                inbox_id,
                1,
                meta,
                sla_status,
            )
            .await?;
        }

        // RT check (event_type = 2)
        if conv_status != 1
            && let Some(due) = compute_due_at(conv_created_at, rt, only_bh, wh)
            && now_ts >= due
        {
            sla_status = record_sla_miss(
                &st.db,
                id,
                conversation_id,
                account_id,
                sla_policy_id,
                inbox_id,
                2,
                json!({}),
                sla_status,
            )
            .await?;
        }

        // Resolved conversation finalizes applied SLA to hit (1) or missed (2)
        if conv_status == 1 {
            let final_status = if sla_status == 0 { 1 } else { 2 };
            sqlx::query(
                "UPDATE applied_slas SET sla_status = $2, completed_at = COALESCE(completed_at, now()), updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .bind(final_status)
            .execute(&st.db)
            .await?;
            sla_status = final_status;
        }

        if sla_status != old_status {
            conversations::emit_conv(st, conversation_id, "conversation.updated")
                .await
                .ok();
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn record_sla_miss(
    db: &PgPool,
    applied_sla_id: i64,
    conversation_id: i64,
    account_id: i64,
    sla_policy_id: i64,
    inbox_id: i64,
    event_type: i32,
    meta: Value,
    current_status: i32,
) -> anyhow::Result<i32> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sla_events WHERE applied_sla_id = $1 AND event_type = $2 AND meta = $3)",
    )
    .bind(applied_sla_id)
    .bind(event_type)
    .bind(&meta)
    .fetch_one(db)
    .await?;
    if !exists {
        sqlx::query(
            "INSERT INTO sla_events (applied_sla_id, conversation_id, account_id, sla_policy_id, inbox_id, event_type, meta, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())",
        )
        .bind(applied_sla_id)
        .bind(conversation_id)
        .bind(account_id)
        .bind(sla_policy_id)
        .bind(inbox_id)
        .bind(event_type)
        .bind(meta)
        .execute(db)
        .await?;
    }
    if current_status != 3 {
        sqlx::query("UPDATE applied_slas SET sla_status = 3, updated_at = now() WHERE id = $1")
            .bind(applied_sla_id)
            .execute(db)
            .await?;
    }
    Ok(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn compute_due_at_wall_clock_and_business_hours() {
        let start = NaiveDate::from_ymd_opt(2026, 11, 25)
            .unwrap()
            .and_hms_opt(16, 30, 0)
            .unwrap(); // Wednesday 16:30 UTC
        let empty = HashMap::new();
        // 1 hour wall clock -> 17:30 UTC
        assert_eq!(
            compute_due_at(start, Some(3600.0), false, &empty),
            Some((start + Duration::hours(1)).and_utc().timestamp())
        );

        let mut wh = HashMap::new();
        // Wed (3) and Thu (4): 09:00..17:00
        for d in [3, 4] {
            wh.insert(
                d,
                WorkingHourRow {
                    day_of_week: d,
                    closed_all_day: false,
                    open_all_day: false,
                    open_hour: Some(9),
                    open_minutes: Some(0),
                    close_hour: Some(17),
                    close_minutes: Some(0),
                },
            );
        }
        // 1 hour starting at Wed 16:30 -> 30m left on Wed + 30m on Thu = Thu 09:30 UTC
        let expected = NaiveDate::from_ymd_opt(2026, 11, 26)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        assert_eq!(
            compute_due_at(start, Some(3600.0), true, &wh),
            Some(expected)
        );
    }

    #[test]
    fn status_and_event_names_match_chatwoot() {
        assert_eq!(sla_status_name(0), "active");
        assert_eq!(sla_status_name(1), "hit");
        assert_eq!(sla_status_name(2), "missed");
        assert_eq!(sla_status_name(3), "active_with_misses");
        assert_eq!(sla_event_type_name(0), "frt");
        assert_eq!(sla_event_type_name(1), "nrt");
        assert_eq!(sla_event_type_name(2), "rt");
    }
}
