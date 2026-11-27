//! OWNED BY A WORKER. See WORKERS.md for the contract.
//! Routes here are mounted at /api/v2/accounts/{account_id} (reports), see main.rs.
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::get,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

use crate::common::{ApiError, ApiResult, AppState, CurrentUser, require_account};

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct ReportParams {
    pub metric: Option<String>,
    pub r#type: Option<String>,
    pub id: Option<String>,
    pub since: Option<String>,
    pub r#until: Option<String>,
    pub group_by: Option<String>,
    pub bucket_timestamp: Option<String>,
    pub page: Option<String>,
    pub per_page: Option<String>,
    pub business_hours: Option<Value>,
    pub timezone_offset: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
pub struct LiveParams {
    pub team_id: Option<i64>,
    pub group_by: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/reports", get(reports_index))
        .route("/reports/summary", get(reports_summary))
        .route("/reports/bot_summary", get(reports_bot_summary))
        .route("/reports/conversations", get(reports_conversations))
        .route(
            "/reports/conversation_traffic",
            get(reports_conversation_traffic),
        )
        .route(
            "/reports/conversations_summary",
            get(reports_conversations_summary),
        )
        .route("/reports/agents", get(reports_agents))
        .route("/reports/inboxes", get(reports_inboxes))
        .route("/reports/labels", get(reports_labels))
        .route("/reports/teams", get(reports_teams))
        .route("/reports/drilldown", get(reports_drilldown))
        .route("/reports/bot_metrics", get(reports_bot_metrics))
        .route(
            "/reports/inbox_label_matrix",
            get(reports_inbox_label_matrix),
        )
        .route(
            "/reports/first_response_time_distribution",
            get(reports_first_response_time_distribution),
        )
        .route(
            "/reports/outgoing_messages_count",
            get(reports_outgoing_messages_count),
        )
        // summary_reports API matches dashboard summary_reports Vue store
        .route("/summary_reports/agent", get(summary_reports_agent))
        .route("/summary_reports/team", get(summary_reports_team))
        .route("/summary_reports/inbox", get(summary_reports_inbox))
        .route("/summary_reports/label", get(summary_reports_label))
        .route("/summary_reports/channel", get(summary_reports_channel))
        // live_reports
        .route(
            "/live_reports/conversation_metrics",
            get(live_conversation_metrics),
        )
        .route(
            "/live_reports/grouped_conversation_metrics",
            get(live_grouped_conversation_metrics),
        )
        // ReportPolicy#view?: every report handler above calls `require_report_viewer`.
        // year_in_review is per-user (YearInReviewsController has no report check)
        .route("/year_in_review", get(year_in_review_show))
}

/// Chatwoot gates reports, summary_reports and live_reports with `authorize :report, :view?`:
/// administrators, or custom roles with report_manage.
async fn require_report_viewer(
    db: &sqlx::PgPool,
    user: &CurrentUser,
    account_id: i64,
) -> Result<crate::common::AccountUser, ApiError> {
    let au = require_account(db, user, account_id).await?;
    if au.is_admin()
        || au
            .custom_perms
            .as_ref()
            .is_some_and(|p| p.iter().any(|x| x == "report_manage"))
    {
        Ok(au)
    } else {
        Err(ApiError::unauthorized())
    }
}

fn parse_ts(val: Option<&str>) -> Option<NaiveDateTime> {
    let s = val?.trim();
    if s.is_empty() {
        return None;
    }
    let sec: i64 = s.parse().ok()?;
    DateTime::from_timestamp(sec, 0).map(|dt| dt.naive_utc())
}

fn parse_num_param(val: Option<&str>) -> Option<i64> {
    val.and_then(|s| s.trim().parse::<i64>().ok())
}

fn group_by_trunc(group_by: Option<&str>) -> &'static str {
    match group_by.unwrap_or("day") {
        "hour" => "hour",
        "week" => "week",
        "month" => "month",
        "year" => "year",
        _ => "day",
    }
}

// ---------------------------------------------------------------------------
// GET /reports?metric=...&type=account|agent|inbox|label|team&id=...
// ---------------------------------------------------------------------------
async fn reports_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let metric = params.metric.as_deref().unwrap_or("conversations_count");
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());
    let dim_type = params.r#type.as_deref().unwrap_or("account");
    let dim_id = parse_num_param(params.id.as_deref());
    let trunc = group_by_trunc(params.group_by.as_deref());

    let series = match metric {
        "conversations_count" => {
            query_conversations_timeseries(
                &st.db, account_id, dim_type, dim_id, since, until, trunc,
            )
            .await?
        }
        "incoming_messages_count" => {
            query_messages_timeseries(&st.db, account_id, 0, dim_type, dim_id, since, until, trunc)
                .await?
        }
        "outgoing_messages_count" => {
            query_messages_timeseries(&st.db, account_id, 1, dim_type, dim_id, since, until, trunc)
                .await?
        }
        "avg_first_response_time" => {
            query_avg_first_response_timeseries(
                &st.db, account_id, dim_type, dim_id, since, until, trunc,
            )
            .await?
        }
        "avg_resolution_time" => {
            query_avg_resolution_timeseries(
                &st.db, account_id, dim_type, dim_id, since, until, trunc,
            )
            .await?
        }
        "resolutions_count" => {
            query_resolutions_count_timeseries(
                &st.db, account_id, dim_type, dim_id, since, until, trunc,
            )
            .await?
        }
        "reply_time" => {
            query_reply_time_timeseries(&st.db, account_id, dim_type, dim_id, since, until, trunc)
                .await?
        }
        "bot_resolutions_count" => {
            query_reporting_events_timeseries(
                &st.db,
                account_id,
                "conversation_bot_resolved",
                dim_type,
                dim_id,
                since,
                until,
                trunc,
            )
            .await?
        }
        "bot_handoffs_count" => {
            query_reporting_events_timeseries(
                &st.db,
                account_id,
                "conversation_bot_handoff",
                dim_type,
                dim_id,
                since,
                until,
                trunc,
            )
            .await?
        }
        _ => Vec::new(),
    };

    Ok(Json(Value::Array(series)))
}

async fn query_conversations_timeseries(
    db: &PgPool,
    account_id: i64,
    dim_type: &str,
    dim_id: Option<i64>,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
    trunc: &str,
) -> Result<Vec<Value>, ApiError> {
    let sql = format!(
        r#"
        SELECT
            EXTRACT(EPOCH FROM date_trunc('{trunc}', c.created_at))::bigint AS ts,
            COUNT(c.id)::bigint AS count
        FROM conversations c
        WHERE c.account_id = $1
          AND ($2::timestamp IS NULL OR c.created_at >= $2)
          AND ($3::timestamp IS NULL OR c.created_at <= $3)
          AND (
            CASE $4
                WHEN 'agent' THEN c.assignee_id = $5
                WHEN 'inbox' THEN c.inbox_id = $5
                WHEN 'team'  THEN c.team_id = $5
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = c.id
                      AND tg.context = 'labels'
                      AND l.id = $5
                )
                ELSE TRUE
            END
          )
        GROUP BY 1
        ORDER BY 1 ASC
        "#
    );
    let rows = sqlx::query(&sql)
        .bind(account_id)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let ts: i64 = r.get("ts");
            let count: i64 = r.get("count");
            json!({ "value": count, "timestamp": ts })
        })
        .collect())
}

async fn query_messages_timeseries(
    db: &PgPool,
    account_id: i64,
    message_type: i32,
    dim_type: &str,
    dim_id: Option<i64>,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
    trunc: &str,
) -> Result<Vec<Value>, ApiError> {
    let sql = format!(
        r#"
        SELECT
            EXTRACT(EPOCH FROM date_trunc('{trunc}', m.created_at))::bigint AS ts,
            COUNT(m.id)::bigint AS count
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
        WHERE m.account_id = $1
          AND m.message_type = $2
          AND ($3::timestamp IS NULL OR m.created_at >= $3)
          AND ($4::timestamp IS NULL OR m.created_at <= $4)
          AND (
            CASE $5
                WHEN 'agent' THEN c.assignee_id = $6
                WHEN 'inbox' THEN m.inbox_id = $6
                WHEN 'team'  THEN c.team_id = $6
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = c.id
                      AND tg.context = 'labels'
                      AND l.id = $6
                )
                ELSE TRUE
            END
          )
        GROUP BY 1
        ORDER BY 1 ASC
        "#
    );
    let rows = sqlx::query(&sql)
        .bind(account_id)
        .bind(message_type)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let ts: i64 = r.get("ts");
            let count: i64 = r.get("count");
            json!({ "value": count, "timestamp": ts })
        })
        .collect())
}

async fn query_avg_first_response_timeseries(
    db: &PgPool,
    account_id: i64,
    dim_type: &str,
    dim_id: Option<i64>,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
    trunc: &str,
) -> Result<Vec<Value>, ApiError> {
    let sql = format!(
        r#"
        SELECT
            EXTRACT(EPOCH FROM date_trunc('{trunc}', c.created_at))::bigint AS ts,
            COALESCE(AVG(EXTRACT(EPOCH FROM (c.first_reply_created_at - c.created_at))), 0)::double precision AS avg_sec,
            COUNT(c.id)::bigint AS count
        FROM conversations c
        WHERE c.account_id = $1
          AND c.first_reply_created_at IS NOT NULL
          AND c.first_reply_created_at >= c.created_at
          AND ($2::timestamp IS NULL OR c.created_at >= $2)
          AND ($3::timestamp IS NULL OR c.created_at <= $3)
          AND (
            CASE $4
                WHEN 'agent' THEN c.assignee_id = $5
                WHEN 'inbox' THEN c.inbox_id = $5
                WHEN 'team'  THEN c.team_id = $5
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = c.id
                      AND tg.context = 'labels'
                      AND l.id = $5
                )
                ELSE TRUE
            END
          )
        GROUP BY 1
        ORDER BY 1 ASC
        "#
    );
    let rows = sqlx::query(&sql)
        .bind(account_id)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let ts: i64 = r.get("ts");
            let avg: f64 = r.get("avg_sec");
            let count: i64 = r.get("count");
            json!({ "value": avg, "timestamp": ts, "count": count })
        })
        .collect())
}

async fn query_avg_resolution_timeseries(
    db: &PgPool,
    account_id: i64,
    dim_type: &str,
    dim_id: Option<i64>,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
    trunc: &str,
) -> Result<Vec<Value>, ApiError> {
    let sql = format!(
        r#"
        SELECT
            EXTRACT(EPOCH FROM date_trunc('{trunc}', c.status_changed_at))::bigint AS ts,
            COALESCE(AVG(EXTRACT(EPOCH FROM (c.status_changed_at - c.created_at))), 0)::double precision AS avg_sec,
            COUNT(c.id)::bigint AS count
        FROM conversations c
        WHERE c.account_id = $1
          AND c.status = 1
          AND c.status_changed_at IS NOT NULL
          AND c.status_changed_at >= c.created_at
          AND ($2::timestamp IS NULL OR c.status_changed_at >= $2)
          AND ($3::timestamp IS NULL OR c.status_changed_at <= $3)
          AND (
            CASE $4
                WHEN 'agent' THEN c.assignee_id = $5
                WHEN 'inbox' THEN c.inbox_id = $5
                WHEN 'team'  THEN c.team_id = $5
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = c.id
                      AND tg.context = 'labels'
                      AND l.id = $5
                )
                ELSE TRUE
            END
          )
        GROUP BY 1
        ORDER BY 1 ASC
        "#
    );
    let rows = sqlx::query(&sql)
        .bind(account_id)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let ts: i64 = r.get("ts");
            let avg: f64 = r.get("avg_sec");
            let count: i64 = r.get("count");
            json!({ "value": avg, "timestamp": ts, "count": count })
        })
        .collect())
}

async fn query_resolutions_count_timeseries(
    db: &PgPool,
    account_id: i64,
    dim_type: &str,
    dim_id: Option<i64>,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
    trunc: &str,
) -> Result<Vec<Value>, ApiError> {
    let sql = format!(
        r#"
        SELECT
            EXTRACT(EPOCH FROM date_trunc('{trunc}', c.status_changed_at))::bigint AS ts,
            COUNT(c.id)::bigint AS count
        FROM conversations c
        WHERE c.account_id = $1
          AND c.status = 1
          AND c.status_changed_at IS NOT NULL
          AND ($2::timestamp IS NULL OR c.status_changed_at >= $2)
          AND ($3::timestamp IS NULL OR c.status_changed_at <= $3)
          AND (
            CASE $4
                WHEN 'agent' THEN c.assignee_id = $5
                WHEN 'inbox' THEN c.inbox_id = $5
                WHEN 'team'  THEN c.team_id = $5
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = c.id
                      AND tg.context = 'labels'
                      AND l.id = $5
                )
                ELSE TRUE
            END
          )
        GROUP BY 1
        ORDER BY 1 ASC
        "#
    );
    let rows = sqlx::query(&sql)
        .bind(account_id)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let ts: i64 = r.get("ts");
            let count: i64 = r.get("count");
            json!({ "value": count, "timestamp": ts })
        })
        .collect())
}

async fn query_reply_time_timeseries(
    db: &PgPool,
    account_id: i64,
    dim_type: &str,
    dim_id: Option<i64>,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
    trunc: &str,
) -> Result<Vec<Value>, ApiError> {
    // reply_time = first response time as per Chatwoot fallback when reporting_events absent
    query_avg_first_response_timeseries(db, account_id, dim_type, dim_id, since, until, trunc).await
}

// ---------------------------------------------------------------------------
// GET /reports/summary?type=...&id=...&since=...&until=...
// ---------------------------------------------------------------------------
async fn calculate_summary_metrics(
    db: &PgPool,
    account_id: i64,
    dim_type: &str,
    dim_id: Option<i64>,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
) -> Result<Value, ApiError> {
    // 1. conversations_count
    let conv_sql = r#"
        SELECT COUNT(c.id)::bigint AS count
        FROM conversations c
        WHERE c.account_id = $1
          AND ($2::timestamp IS NULL OR c.created_at >= $2)
          AND ($3::timestamp IS NULL OR c.created_at <= $3)
          AND (
            CASE $4
                WHEN 'agent' THEN c.assignee_id = $5
                WHEN 'inbox' THEN c.inbox_id = $5
                WHEN 'team'  THEN c.team_id = $5
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = c.id
                      AND tg.context = 'labels'
                      AND l.id = $5
                )
                ELSE TRUE
            END
          )
    "#;
    let conv_row = sqlx::query(conv_sql)
        .bind(account_id)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_one(db)
        .await?;
    let conv_count: i64 = conv_row.get("count");

    // 2. incoming and outgoing messages count
    let msg_sql = r#"
        SELECT
            COUNT(CASE WHEN m.message_type = 0 THEN 1 END)::bigint AS incoming,
            COUNT(CASE WHEN m.message_type = 1 THEN 1 END)::bigint AS outgoing
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
        WHERE m.account_id = $1
          AND ($2::timestamp IS NULL OR m.created_at >= $2)
          AND ($3::timestamp IS NULL OR m.created_at <= $3)
          AND (
            CASE $4
                WHEN 'agent' THEN c.assignee_id = $5
                WHEN 'inbox' THEN m.inbox_id = $5
                WHEN 'team'  THEN c.team_id = $5
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = c.id
                      AND tg.context = 'labels'
                      AND l.id = $5
                )
                ELSE TRUE
            END
          )
    "#;
    let msg_row = sqlx::query(msg_sql)
        .bind(account_id)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_one(db)
        .await?;
    let incoming_count: i64 = msg_row.get("incoming");
    let outgoing_count: i64 = msg_row.get("outgoing");

    // 3. avg_first_response_time
    let frt_sql = r#"
        SELECT
            COALESCE(AVG(EXTRACT(EPOCH FROM (c.first_reply_created_at - c.created_at))), 0)::double precision AS avg_frt
        FROM conversations c
        WHERE c.account_id = $1
          AND c.first_reply_created_at IS NOT NULL
          AND c.first_reply_created_at >= c.created_at
          AND ($2::timestamp IS NULL OR c.created_at >= $2)
          AND ($3::timestamp IS NULL OR c.created_at <= $3)
          AND (
            CASE $4
                WHEN 'agent' THEN c.assignee_id = $5
                WHEN 'inbox' THEN c.inbox_id = $5
                WHEN 'team'  THEN c.team_id = $5
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = c.id
                      AND tg.context = 'labels'
                      AND l.id = $5
                )
                ELSE TRUE
            END
          )
    "#;
    let frt_row = sqlx::query(frt_sql)
        .bind(account_id)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_one(db)
        .await?;
    let avg_frt: f64 = frt_row.get("avg_frt");

    // 4. resolutions_count and avg_resolution_time
    let res_sql = r#"
        SELECT
            COUNT(c.id)::bigint AS count,
            COALESCE(AVG(EXTRACT(EPOCH FROM (c.status_changed_at - c.created_at))), 0)::double precision AS avg_res
        FROM conversations c
        WHERE c.account_id = $1
          AND c.status = 1
          AND c.status_changed_at IS NOT NULL
          AND c.status_changed_at >= c.created_at
          AND ($2::timestamp IS NULL OR c.status_changed_at >= $2)
          AND ($3::timestamp IS NULL OR c.status_changed_at <= $3)
          AND (
            CASE $4
                WHEN 'agent' THEN c.assignee_id = $5
                WHEN 'inbox' THEN c.inbox_id = $5
                WHEN 'team'  THEN c.team_id = $5
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = c.id
                      AND tg.context = 'labels'
                      AND l.id = $5
                )
                ELSE TRUE
            END
          )
    "#;
    let res_row = sqlx::query(res_sql)
        .bind(account_id)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_one(db)
        .await?;
    let res_count: i64 = res_row.get("count");
    let avg_res: f64 = res_row.get("avg_res");

    Ok(json!({
        "conversations_count": conv_count,
        "incoming_messages_count": incoming_count,
        "outgoing_messages_count": outgoing_count,
        "avg_first_response_time": avg_frt,
        "avg_resolution_time": avg_res,
        "resolutions_count": res_count,
        "reply_time": avg_frt,
        "bot_resolutions_count": 0,
        "bot_handoffs_count": 0,
    }))
}

async fn reports_summary(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let dim_type = params.r#type.as_deref().unwrap_or("account");
    let dim_id = parse_num_param(params.id.as_deref());

    let since_raw = parse_num_param(params.since.as_deref());
    let until_raw = parse_num_param(params.r#until.as_deref());

    let since = since_raw.and_then(|s| DateTime::from_timestamp(s, 0).map(|dt| dt.naive_utc()));
    let until = until_raw.and_then(|s| DateTime::from_timestamp(s, 0).map(|dt| dt.naive_utc()));

    let mut current =
        calculate_summary_metrics(&st.db, account_id, dim_type, dim_id, since, until).await?;

    let prev_metrics = if let (Some(s), Some(u)) = (since_raw, until_raw) {
        let diff = u - s;
        let prev_s = s - diff;
        let prev_u = s;
        let prev_since = DateTime::from_timestamp(prev_s, 0).map(|dt| dt.naive_utc());
        let prev_until = DateTime::from_timestamp(prev_u, 0).map(|dt| dt.naive_utc());
        calculate_summary_metrics(&st.db, account_id, dim_type, dim_id, prev_since, prev_until)
            .await?
    } else {
        json!({
            "conversations_count": 0,
            "incoming_messages_count": 0,
            "outgoing_messages_count": 0,
            "avg_first_response_time": 0,
            "avg_resolution_time": 0,
            "resolutions_count": 0,
            "reply_time": 0,
            "bot_resolutions_count": 0,
            "bot_handoffs_count": 0,
        })
    };

    if let Value::Object(ref mut map) = current {
        map.insert("previous".to_string(), prev_metrics);
    }

    Ok(Json(current))
}

async fn reports_bot_summary(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(_params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    Ok(Json(json!({
        "bot_resolutions_count": 0,
        "bot_handoffs_count": 0,
        "previous": {
            "bot_resolutions_count": 0,
            "bot_handoffs_count": 0
        }
    })))
}

// ---------------------------------------------------------------------------
// GET /reports/conversations?type=account -> {open, unattended, unassigned, pending}
// ---------------------------------------------------------------------------
async fn reports_conversations(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let dim_type = params.r#type.as_deref().unwrap_or("account");

    if dim_type == "account" {
        let counts = get_account_conversation_metrics(&st.db, account_id, None).await?;
        Ok(Json(json!(counts)))
    } else {
        // Agent metrics list
        let rows = sqlx::query(
            r#"
            SELECT
                u.id::bigint AS id, u.name, u.email,
                COALESCE(au.availability, 0) AS availability
            FROM account_users au
            JOIN users u ON u.id = au.user_id
            WHERE au.account_id = $1
            ORDER BY u.name ASC
            LIMIT 50
            "#,
        )
        .bind(account_id)
        .fetch_all(&st.db)
        .await?;

        let mut res = Vec::new();
        for r in rows {
            let user_id: i64 = r.get("id");
            let name: String = r.get("name");
            let email: String = r.get("email");
            let avatar = crate::storage::avatar_url(&st.db, "User", user_id).await;
            let availability: i32 = r.get("availability");
            let metrics = get_agent_conversation_counts(&st.db, account_id, user_id).await?;

            res.push(json!({
                "id": user_id,
                "name": name,
                "email": email,
                "thumbnail": avatar,
                "availability": match availability { 1 => "online", 2 => "busy", _ => "offline" },
                "metric": metrics
            }));
        }

        Ok(Json(Value::Array(res)))
    }
}

async fn get_account_conversation_metrics(
    db: &PgPool,
    account_id: i64,
    team_id: Option<i64>,
) -> Result<Value, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(CASE WHEN c.status = 0 THEN 1 END)::bigint AS open,
            COUNT(CASE WHEN c.status = 0 AND (c.first_reply_created_at IS NULL OR c.waiting_since IS NOT NULL) THEN 1 END)::bigint AS unattended,
            COUNT(CASE WHEN c.status = 0 AND c.assignee_id IS NULL THEN 1 END)::bigint AS unassigned,
            COUNT(CASE WHEN c.status = 2 THEN 1 END)::bigint AS pending
        FROM conversations c
        WHERE c.account_id = $1
          AND ($2::bigint IS NULL OR c.team_id = $2)
        "#,
    )
    .bind(account_id)
    .bind(team_id)
    .fetch_one(db)
    .await?;

    let open: i64 = row.get("open");
    let unattended: i64 = row.get("unattended");
    let unassigned: i64 = row.get("unassigned");
    let pending: i64 = row.get("pending");

    Ok(json!({
        "open": open,
        "unattended": unattended,
        "unassigned": unassigned,
        "pending": pending
    }))
}

async fn get_agent_conversation_counts(
    db: &PgPool,
    account_id: i64,
    user_id: i64,
) -> Result<Value, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(CASE WHEN c.status = 0 THEN 1 END)::bigint AS open,
            COUNT(CASE WHEN c.status = 0 AND (c.first_reply_created_at IS NULL OR c.waiting_since IS NOT NULL) THEN 1 END)::bigint AS unattended
        FROM conversations c
        WHERE c.account_id = $1 AND c.assignee_id = $2
        "#,
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let open: i64 = row.get("open");
    let unattended: i64 = row.get("unattended");

    Ok(json!({
        "open": open,
        "unattended": unattended
    }))
}

// ---------------------------------------------------------------------------
// GET /reports/conversation_traffic
// ---------------------------------------------------------------------------
async fn reports_conversation_traffic(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(_params): Query<ReportParams>,
) -> impl IntoResponse {
    let au = match require_report_viewer(&st.db, &user, account_id).await {
        Ok(au) => au,
        Err(e) => return e.into_response(),
    };
    let _ = au;

    let now_ts = Utc::now().naive_utc();
    let days_before: i64 = 6;
    let since = now_ts - chrono::Duration::days(days_before);

    let rows = match sqlx::query(
        r#"
        SELECT
            EXTRACT(EPOCH FROM date_trunc('hour', created_at))::bigint AS ts,
            COUNT(id)::bigint AS count
        FROM conversations
        WHERE account_id = $1
          AND created_at >= $2
        GROUP BY 1
        ORDER BY 1 ASC
        "#,
    )
    .bind(account_id)
    .bind(since)
    .fetch_all(&st.db)
    .await
    {
        Ok(r) => r,
        Err(e) => return ApiError::from(e).into_response(),
    };

    // Build array of {value, timestamp} for heatmap
    let data: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let ts: i64 = r.get("ts");
            let count: i64 = r.get("count");
            json!({ "value": count, "timestamp": ts })
        })
        .collect();

    // If client requested CSV, produce CSV format; otherwise return JSON
    ([("content-type", "application/json")], Json(data)).into_response()
}

// ---------------------------------------------------------------------------
// GET /reports/conversations_summary
// ---------------------------------------------------------------------------
async fn reports_conversations_summary(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> impl IntoResponse {
    let au = match require_report_viewer(&st.db, &user, account_id).await {
        Ok(au) => au,
        Err(e) => return e.into_response(),
    };
    let _ = au;

    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());

    let summary =
        match calculate_summary_metrics(&st.db, account_id, "account", None, since, until).await {
            Ok(s) => s,
            Err(e) => return e.into_response(),
        };

    // Chatwoot PR #15686: BOM so Excel opens the file as UTF-8 instead of the system codepage.
    let csv_content = format!(
        "\u{feff}Conversations,Incoming Messages,Outgoing Messages,First Response Time,Resolution Time,Resolutions,Reply Time\n{},{},{},{},{},{},{}\n",
        summary["conversations_count"],
        summary["incoming_messages_count"],
        summary["outgoing_messages_count"],
        summary["avg_first_response_time"],
        summary["avg_resolution_time"],
        summary["resolutions_count"],
        summary["reply_time"]
    );

    (
        [
            ("content-type", "text/csv"),
            (
                "content-disposition",
                "attachment; filename=conversations_summary.csv",
            ),
        ],
        csv_content,
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /reports/agents, /reports/inboxes, /reports/labels, /reports/teams
// ---------------------------------------------------------------------------
async fn build_agents_summary(
    db: &PgPool,
    account_id: i64,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
) -> Result<Vec<Value>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT
            u.id::bigint AS id, u.name, u.email,
            COALESCE(c_agg.conversations_count, 0)::bigint AS conversations_count,
            COALESCE(c_agg.resolved_count, 0)::bigint AS resolved_conversations_count,
            c_agg.avg_frt::double precision AS avg_first_response_time,
            c_agg.avg_res::double precision AS avg_resolution_time
        FROM account_users au
        JOIN users u ON u.id = au.user_id
        LEFT JOIN (
            SELECT
                c.assignee_id AS user_id,
                COUNT(c.id) AS conversations_count,
                COUNT(CASE WHEN c.status = 1 THEN 1 END) AS resolved_count,
                AVG(CASE WHEN c.first_reply_created_at IS NOT NULL AND c.first_reply_created_at >= c.created_at
                         THEN EXTRACT(EPOCH FROM (c.first_reply_created_at - c.created_at)) END) AS avg_frt,
                AVG(CASE WHEN c.status = 1 AND c.status_changed_at IS NOT NULL AND c.status_changed_at >= c.created_at
                         THEN EXTRACT(EPOCH FROM (c.status_changed_at - c.created_at)) END) AS avg_res
            FROM conversations c
            WHERE c.account_id = $1
              AND ($2::timestamp IS NULL OR c.created_at >= $2)
              AND ($3::timestamp IS NULL OR c.created_at <= $3)
            GROUP BY c.assignee_id
        ) c_agg ON c_agg.user_id = u.id
        WHERE au.account_id = $1
        ORDER BY u.name ASC
        "#,
    )
    .bind(account_id)
    .bind(since)
    .bind(until)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let id: i64 = r.get("id");
            let name: String = r.get("name");
            let email: String = r.get("email");
            let conv_count: i64 = r.get("conversations_count");
            let res_count: i64 = r.get("resolved_conversations_count");
            let frt: Option<f64> = r.get("avg_first_response_time");
            let res_time: Option<f64> = r.get("avg_resolution_time");

            json!({
                "id": id,
                "name": name,
                "email": email,
                "conversations_count": conv_count,
                "resolved_conversations_count": res_count,
                "avg_first_response_time": frt.unwrap_or(0.0),
                "avg_resolution_time": res_time.unwrap_or(0.0),
                "avg_reply_time": frt.unwrap_or(0.0)
            })
        })
        .collect())
}

async fn reports_agents(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());
    let list = build_agents_summary(&st.db, account_id, since, until).await?;
    Ok(Json(Value::Array(list)))
}

async fn build_inboxes_summary(
    db: &PgPool,
    account_id: i64,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
) -> Result<Vec<Value>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT
            i.id::bigint AS id, i.name, i.channel_type,
            COALESCE(c_agg.conversations_count, 0)::bigint AS conversations_count,
            COALESCE(c_agg.resolved_count, 0)::bigint AS resolved_conversations_count,
            c_agg.avg_frt::double precision AS avg_first_response_time,
            c_agg.avg_res::double precision AS avg_resolution_time
        FROM inboxes i
        LEFT JOIN (
            SELECT
                c.inbox_id,
                COUNT(c.id) AS conversations_count,
                COUNT(CASE WHEN c.status = 1 THEN 1 END) AS resolved_count,
                AVG(CASE WHEN c.first_reply_created_at IS NOT NULL AND c.first_reply_created_at >= c.created_at
                         THEN EXTRACT(EPOCH FROM (c.first_reply_created_at - c.created_at)) END) AS avg_frt,
                AVG(CASE WHEN c.status = 1 AND c.status_changed_at IS NOT NULL AND c.status_changed_at >= c.created_at
                         THEN EXTRACT(EPOCH FROM (c.status_changed_at - c.created_at)) END) AS avg_res
            FROM conversations c
            WHERE c.account_id = $1
              AND ($2::timestamp IS NULL OR c.created_at >= $2)
              AND ($3::timestamp IS NULL OR c.created_at <= $3)
            GROUP BY c.inbox_id
        ) c_agg ON c_agg.inbox_id = i.id
        WHERE i.account_id = $1
        ORDER BY i.name ASC
        "#,
    )
    .bind(account_id)
    .bind(since)
    .bind(until)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let id: i64 = r.get("id");
            let name: String = r.get("name");
            let channel_type: Option<String> = r.get("channel_type");
            let conv_count: i64 = r.get("conversations_count");
            let res_count: i64 = r.get("resolved_conversations_count");
            let frt: Option<f64> = r.get("avg_first_response_time");
            let res_time: Option<f64> = r.get("avg_resolution_time");

            json!({
                "id": id,
                "name": name,
                "channel_type": channel_type,
                "conversations_count": conv_count,
                "resolved_conversations_count": res_count,
                "avg_first_response_time": frt.unwrap_or(0.0),
                "avg_resolution_time": res_time.unwrap_or(0.0),
                "avg_reply_time": frt.unwrap_or(0.0)
            })
        })
        .collect())
}

async fn reports_inboxes(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());
    let list = build_inboxes_summary(&st.db, account_id, since, until).await?;
    Ok(Json(Value::Array(list)))
}

async fn build_teams_summary(
    db: &PgPool,
    account_id: i64,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
) -> Result<Vec<Value>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT
            t.id, t.name,
            COALESCE(c_agg.conversations_count, 0)::bigint AS conversations_count,
            COALESCE(c_agg.resolved_count, 0)::bigint AS resolved_conversations_count,
            c_agg.avg_frt::double precision AS avg_first_response_time,
            c_agg.avg_res::double precision AS avg_resolution_time
        FROM teams t
        LEFT JOIN (
            SELECT
                c.team_id,
                COUNT(c.id) AS conversations_count,
                COUNT(CASE WHEN c.status = 1 THEN 1 END) AS resolved_count,
                AVG(CASE WHEN c.first_reply_created_at IS NOT NULL AND c.first_reply_created_at >= c.created_at
                         THEN EXTRACT(EPOCH FROM (c.first_reply_created_at - c.created_at)) END) AS avg_frt,
                AVG(CASE WHEN c.status = 1 AND c.status_changed_at IS NOT NULL AND c.status_changed_at >= c.created_at
                         THEN EXTRACT(EPOCH FROM (c.status_changed_at - c.created_at)) END) AS avg_res
            FROM conversations c
            WHERE c.account_id = $1
              AND ($2::timestamp IS NULL OR c.created_at >= $2)
              AND ($3::timestamp IS NULL OR c.created_at <= $3)
            GROUP BY c.team_id
        ) c_agg ON c_agg.team_id = t.id
        WHERE t.account_id = $1
        ORDER BY t.name ASC
        "#,
    )
    .bind(account_id)
    .bind(since)
    .bind(until)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let id: i64 = r.get("id");
            let name: String = r.get("name");
            let conv_count: i64 = r.get("conversations_count");
            let res_count: i64 = r.get("resolved_conversations_count");
            let frt: Option<f64> = r.get("avg_first_response_time");
            let res_time: Option<f64> = r.get("avg_resolution_time");

            json!({
                "id": id,
                "name": name,
                "conversations_count": conv_count,
                "resolved_conversations_count": res_count,
                "avg_first_response_time": frt.unwrap_or(0.0),
                "avg_resolution_time": res_time.unwrap_or(0.0),
                "avg_reply_time": frt.unwrap_or(0.0)
            })
        })
        .collect())
}

async fn reports_teams(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());
    let list = build_teams_summary(&st.db, account_id, since, until).await?;
    Ok(Json(Value::Array(list)))
}

async fn build_labels_summary(
    db: &PgPool,
    account_id: i64,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
) -> Result<Vec<Value>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT
            l.id, l.title, l.color,
            COALESCE(c_agg.conversations_count, 0)::bigint AS conversations_count,
            COALESCE(c_agg.resolved_count, 0)::bigint AS resolved_conversations_count,
            c_agg.avg_frt::double precision AS avg_first_response_time,
            c_agg.avg_res::double precision AS avg_resolution_time
        FROM labels l
        LEFT JOIN (
            SELECT
                tg.tag_id,
                COUNT(DISTINCT c.id) AS conversations_count,
                COUNT(DISTINCT CASE WHEN c.status = 1 THEN c.id END) AS resolved_count,
                AVG(CASE WHEN c.first_reply_created_at IS NOT NULL AND c.first_reply_created_at >= c.created_at
                         THEN EXTRACT(EPOCH FROM (c.first_reply_created_at - c.created_at)) END) AS avg_frt,
                AVG(CASE WHEN c.status = 1 AND c.status_changed_at IS NOT NULL AND c.status_changed_at >= c.created_at
                         THEN EXTRACT(EPOCH FROM (c.status_changed_at - c.created_at)) END) AS avg_res
            FROM taggings tg
            JOIN tags t ON t.id = tg.tag_id
            JOIN conversations c ON c.id = tg.taggable_id
            WHERE tg.taggable_type = 'Conversation'
              AND tg.context = 'labels'
              AND c.account_id = $1
              AND ($2::timestamp IS NULL OR c.created_at >= $2)
              AND ($3::timestamp IS NULL OR c.created_at <= $3)
            GROUP BY tg.tag_id
        ) c_agg ON c_agg.tag_id IN (SELECT id FROM tags WHERE name = l.title)
        WHERE l.account_id = $1
        ORDER BY l.title ASC
        "#,
    )
    .bind(account_id)
    .bind(since)
    .bind(until)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let id: i64 = r.get("id");
            let title: String = r.get("title");
            let color: String = r.get("color");
            let conv_count: i64 = r.get("conversations_count");
            let res_count: i64 = r.get("resolved_conversations_count");
            let frt: Option<f64> = r.get("avg_first_response_time");
            let res_time: Option<f64> = r.get("avg_resolution_time");

            json!({
                "id": id,
                "name": title,
                "title": title,
                "color": color,
                "conversations_count": conv_count,
                "resolved_conversations_count": res_count,
                "avg_first_response_time": frt.unwrap_or(0.0),
                "avg_resolution_time": res_time.unwrap_or(0.0),
                "avg_reply_time": frt.unwrap_or(0.0)
            })
        })
        .collect())
}

async fn reports_labels(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());
    let list = build_labels_summary(&st.db, account_id, since, until).await?;
    Ok(Json(Value::Array(list)))
}

// ---------------------------------------------------------------------------
// /summary_reports/* (same builders as reports/agents, inboxes, teams, labels)
// ---------------------------------------------------------------------------
async fn summary_reports_agent(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    reports_agents(State(st), user, Path(account_id), Query(params)).await
}

async fn summary_reports_team(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    reports_teams(State(st), user, Path(account_id), Query(params)).await
}

async fn summary_reports_inbox(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    reports_inboxes(State(st), user, Path(account_id), Query(params)).await
}

async fn summary_reports_label(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    reports_labels(State(st), user, Path(account_id), Query(params)).await
}

async fn query_reporting_events_timeseries(
    db: &PgPool,
    account_id: i64,
    event_name: &str,
    dim_type: &str,
    dim_id: Option<i64>,
    since: Option<NaiveDateTime>,
    until: Option<NaiveDateTime>,
    trunc: &str,
) -> Result<Vec<Value>, ApiError> {
    let sql = format!(
        r#"
        SELECT
            EXTRACT(EPOCH FROM date_trunc('{trunc}', r.created_at))::bigint AS ts,
            COUNT(DISTINCT r.conversation_id)::bigint AS count
        FROM reporting_events r
        LEFT JOIN conversations c ON c.id = r.conversation_id AND c.account_id = r.account_id
        WHERE r.account_id = $1
          AND r.name = $2
          AND ($3::timestamp IS NULL OR r.created_at >= $3)
          AND ($4::timestamp IS NULL OR r.created_at <= $4)
          AND (
            CASE $5
                WHEN 'agent' THEN COALESCE(r.user_id, c.assignee_id) = $6
                WHEN 'inbox' THEN COALESCE(r.inbox_id, c.inbox_id) = $6
                WHEN 'team'  THEN c.team_id = $6
                WHEN 'label' THEN EXISTS (
                    SELECT 1 FROM taggings tg
                    JOIN tags t ON t.id = tg.tag_id
                    JOIN labels l ON l.title = t.name AND l.account_id = r.account_id
                    WHERE tg.taggable_type = 'Conversation'
                      AND tg.taggable_id = r.conversation_id
                      AND tg.context = 'labels'
                      AND l.id = $6
                )
                ELSE TRUE
            END
          )
        GROUP BY 1
        ORDER BY 1 ASC
        "#
    );
    let rows = sqlx::query(&sql)
        .bind(account_id)
        .bind(event_name)
        .bind(since)
        .bind(until)
        .bind(dim_type)
        .bind(dim_id)
        .fetch_all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let ts: i64 = r.get("ts");
            let count: i64 = r.get("count");
            json!({ "value": count, "timestamp": ts })
        })
        .collect())
}

async fn summary_reports_channel(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());
    let rows = sqlx::query(
        "SELECT i.channel_type,
                COUNT(c.id)::bigint AS conversations_count,
                COUNT(c.id) FILTER (WHERE c.status = 0)::bigint AS open_count,
                COUNT(c.id) FILTER (WHERE c.status = 1)::bigint AS resolved_count,
                COUNT(c.id) FILTER (WHERE c.status = 2)::bigint AS pending_count,
                COUNT(c.id) FILTER (WHERE c.status = 3)::bigint AS snoozed_count
         FROM conversations c
         JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id
         WHERE c.account_id = $1
           AND ($2::timestamp IS NULL OR c.created_at >= $2)
           AND ($3::timestamp IS NULL OR c.created_at <= $3)
         GROUP BY i.channel_type
         ORDER BY conversations_count DESC, i.channel_type ASC",
    )
    .bind(account_id)
    .bind(since)
    .bind(until)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(Value::Array(
        rows.into_iter()
            .map(|r| {
                json!({
                    "channel_type": r.get::<String, _>("channel_type"),
                    "conversations_count": r.get::<i64, _>("conversations_count"),
                    "open": r.get::<i64, _>("open_count"),
                    "resolved": r.get::<i64, _>("resolved_count"),
                    "pending": r.get::<i64, _>("pending_count"),
                    "snoozed": r.get::<i64, _>("snoozed_count"),
                })
            })
            .collect(),
    )))
}

// ---------------------------------------------------------------------------
// GET /live_reports/conversation_metrics
// ---------------------------------------------------------------------------
async fn live_conversation_metrics(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<LiveParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let metrics = get_account_conversation_metrics(&st.db, account_id, params.team_id).await?;
    Ok(Json(metrics))
}

// ---------------------------------------------------------------------------
// GET /live_reports/grouped_conversation_metrics?group_by=assignee_id|team_id
// ---------------------------------------------------------------------------
async fn live_grouped_conversation_metrics(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<LiveParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let group = params.group_by.as_deref().unwrap_or("assignee_id");

    if group == "team_id" {
        let rows = sqlx::query(
            r#"
            SELECT
                c.team_id AS gid,
                COUNT(c.id)::bigint AS count,
                COUNT(CASE WHEN c.first_reply_created_at IS NULL OR c.waiting_since IS NOT NULL THEN 1 END)::bigint AS unattended,
                COUNT(CASE WHEN c.assignee_id IS NULL THEN 1 END)::bigint AS unassigned
            FROM conversations c
            WHERE c.account_id = $1
              AND c.status = 0
              AND c.team_id IS NOT NULL
            GROUP BY c.team_id
            "#,
        )
        .bind(account_id)
        .fetch_all(&st.db)
        .await?;

        let list: Vec<Value> = rows
            .into_iter()
            .map(|r| {
                let gid: i64 = r.get("gid");
                let count: i64 = r.get("count");
                let unattended: i64 = r.get("unattended");
                let unassigned: i64 = r.get("unassigned");
                json!({
                    "team_id": gid,
                    "open": count,
                    "unattended": unattended,
                    "unassigned": unassigned
                })
            })
            .collect();
        Ok(Json(Value::Array(list)))
    } else {
        let rows = sqlx::query(
            r#"
            SELECT
                c.assignee_id::bigint AS gid,
                COUNT(c.id)::bigint AS count,
                COUNT(CASE WHEN c.first_reply_created_at IS NULL OR c.waiting_since IS NOT NULL THEN 1 END)::bigint AS unattended,
                COUNT(CASE WHEN c.assignee_id IS NULL THEN 1 END)::bigint AS unassigned
            FROM conversations c
            WHERE c.account_id = $1
              AND c.status = 0
              AND c.assignee_id IS NOT NULL
            GROUP BY c.assignee_id
            "#,
        )
        .bind(account_id)
        .fetch_all(&st.db)
        .await?;

        let list: Vec<Value> = rows
            .into_iter()
            .map(|r| {
                let gid: i64 = r.get("gid");
                let count: i64 = r.get("count");
                let unattended: i64 = r.get("unattended");
                let unassigned: i64 = r.get("unassigned");
                json!({
                    "assignee_id": gid,
                    "open": count,
                    "unattended": unattended,
                    "unassigned": unassigned
                })
            })
            .collect();
        Ok(Json(Value::Array(list)))
    }
}

// ---------------------------------------------------------------------------
// Other Reports Endpoints to keep Vue dashboard happy
// ---------------------------------------------------------------------------
fn conv_status_label(status: i32) -> &'static str {
    match status {
        0 => "open",
        1 => "resolved",
        2 => "pending",
        3 => "snoozed",
        _ => "open",
    }
}

fn drilldown_bounds(params: &ReportParams) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());
    let Some(bts) = parse_num_param(params.bucket_timestamp.as_deref()) else {
        return (since, until);
    };
    let Some(b_start) = DateTime::from_timestamp(bts, 0) else {
        return (since, until);
    };
    let b_end = match params.group_by.as_deref().unwrap_or("day") {
        "hour" => b_start + chrono::Duration::hours(1),
        "week" => b_start + chrono::Duration::weeks(1),
        "month" => b_start
            .checked_add_months(chrono::Months::new(1))
            .unwrap_or_else(|| b_start + chrono::Duration::days(30)),
        "year" => b_start
            .checked_add_months(chrono::Months::new(12))
            .unwrap_or_else(|| b_start + chrono::Duration::days(365)),
        _ => b_start + chrono::Duration::days(1),
    };
    let start = match since {
        Some(s) => s.max(b_start.naive_utc()),
        None => b_start.naive_utc(),
    };
    let end = match until {
        Some(u) => u.min(b_end.naive_utc()),
        None => b_end.naive_utc(),
    };
    (Some(start), Some(end))
}

fn dim_predicate_conv(dim_type_idx: &str, id_idx: &str) -> String {
    format!(
        "CASE {dim_type_idx}
            WHEN 'agent' THEN c.assignee_id = {id_idx}
            WHEN 'inbox' THEN c.inbox_id = {id_idx}
            WHEN 'team' THEN c.team_id = {id_idx}
            WHEN 'label' THEN EXISTS (
                SELECT 1 FROM taggings tg
                JOIN tags t ON t.id = tg.tag_id
                JOIN labels l ON l.title = t.name AND l.account_id = c.account_id
                WHERE tg.taggable_type = 'Conversation'
                  AND tg.taggable_id = c.id
                  AND tg.context = 'labels'
                  AND l.id = {id_idx}
            )
            ELSE TRUE
         END"
    )
}

#[allow(clippy::too_many_arguments)]
fn conv_drilldown_json(
    cid: i64,
    did: i64,
    contact_id: Option<i64>,
    contact_name: Option<&str>,
    inbox_id: i64,
    inbox_name: Option<&str>,
    assignee_id: Option<i64>,
    assignee_name: Option<&str>,
    status: i32,
    created_at: NaiveDateTime,
    last_activity_at: NaiveDateTime,
) -> Value {
    json!({
        "id": cid,
        "display_id": did,
        "contact_id": contact_id,
        "contact_name": contact_name,
        "inbox_id": inbox_id,
        "inbox_name": inbox_name,
        "assignee_id": assignee_id,
        "assignee_name": assignee_name,
        "status": conv_status_label(status),
        "created_at": created_at.and_utc().timestamp(),
        "last_activity_at": last_activity_at.and_utc().timestamp(),
    })
}

async fn reports_drilldown(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let metric = params
        .metric
        .clone()
        .unwrap_or_else(|| "conversations_count".into());
    let (since, until) = drilldown_bounds(&params);
    let dim_type = params.r#type.as_deref().unwrap_or("account");
    let dim_id = parse_num_param(params.id.as_deref());
    let page = parse_num_param(params.page.as_deref()).unwrap_or(1).max(1);
    let per_page = parse_num_param(params.per_page.as_deref())
        .unwrap_or(25)
        .clamp(1, 100);
    let offset = (page - 1) * per_page;
    let dim_sql = dim_predicate_conv("$4", "$5");

    let (record_type, total_count, conversation_count, payload): (&str, i64, i64, Vec<Value>) =
        match metric.as_str() {
            "conversations_count" => {
                let total: i64 = sqlx::query_scalar(&format!(
                    "SELECT COUNT(c.id)::bigint
                     FROM conversations c
                     WHERE c.account_id = $1
                       AND ($2::timestamp IS NULL OR c.created_at >= $2)
                       AND ($3::timestamp IS NULL OR c.created_at <= $3)
                       AND ({dim_sql})"
                ))
                .bind(account_id)
                .bind(since)
                .bind(until)
                .bind(dim_type)
                .bind(dim_id)
                .fetch_one(&st.db)
                .await?;

                let rows = sqlx::query(&format!(
                    "SELECT c.id::bigint AS cid, c.display_id::bigint AS did,
                            c.contact_id::bigint AS contact_id, ct.name AS contact_name,
                            c.inbox_id::bigint AS inbox_id, i.name AS inbox_name,
                            c.assignee_id::bigint AS assignee_id, u.name AS assignee_name,
                            c.status, c.created_at, c.last_activity_at
                     FROM conversations c
                     LEFT JOIN contacts ct ON ct.id = c.contact_id
                     LEFT JOIN inboxes i ON i.id = c.inbox_id
                     LEFT JOIN users u ON u.id = c.assignee_id
                     WHERE c.account_id = $1
                       AND ($2::timestamp IS NULL OR c.created_at >= $2)
                       AND ($3::timestamp IS NULL OR c.created_at <= $3)
                       AND ({dim_sql})
                     ORDER BY c.created_at DESC, c.id DESC
                     LIMIT $6 OFFSET $7"
                ))
                .bind(account_id)
                .bind(since)
                .bind(until)
                .bind(dim_type)
                .bind(dim_id)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&st.db)
                .await?;

                let items = rows
                    .into_iter()
                    .map(|r| {
                        let c_created: NaiveDateTime = r.get("created_at");
                        let c_last: NaiveDateTime = r.get("last_activity_at");
                        json!({
                            "record_type": "conversation",
                            "conversation": conv_drilldown_json(
                                r.get("cid"), r.get("did"), r.get("contact_id"), r.get("contact_name"),
                                r.get("inbox_id"), r.get("inbox_name"), r.get("assignee_id"),
                                r.get("assignee_name"), r.get("status"), c_created, c_last,
                            ),
                            "message": Value::Null,
                            "metric_value": Value::Null,
                            "occurred_at": c_created.and_utc().timestamp(),
                        })
                    })
                    .collect();
                ("conversation", total, total, items)
            }
            "incoming_messages_count" | "outgoing_messages_count" => {
                let mtype = if metric == "incoming_messages_count" { 0 } else { 1 };
                let (total, conv_cnt): (i64, i64) = sqlx::query_as(&format!(
                    "SELECT COUNT(m.id)::bigint, COUNT(DISTINCT m.conversation_id)::bigint
                     FROM messages m
                     JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
                     WHERE m.account_id = $1
                       AND m.message_type = $6
                       AND ($2::timestamp IS NULL OR m.created_at >= $2)
                       AND ($3::timestamp IS NULL OR m.created_at <= $3)
                       AND ({dim_sql})"
                ))
                .bind(account_id)
                .bind(since)
                .bind(until)
                .bind(dim_type)
                .bind(dim_id)
                .bind(mtype)
                .fetch_one(&st.db)
                .await?;

                let rows = sqlx::query(&format!(
                    "SELECT m.id::bigint AS mid, m.content, m.message_type, m.created_at AS m_created,
                            COALESCE(su.name, sc.name) AS sender_name,
                            c.id::bigint AS cid, c.display_id::bigint AS did,
                            c.contact_id::bigint AS contact_id, ct.name AS contact_name,
                            c.inbox_id::bigint AS inbox_id, i.name AS inbox_name,
                            c.assignee_id::bigint AS assignee_id, au.name AS assignee_name,
                            c.status, c.created_at AS c_created, c.last_activity_at AS c_last
                     FROM messages m
                     JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
                     LEFT JOIN contacts ct ON ct.id = c.contact_id
                     LEFT JOIN inboxes i ON i.id = c.inbox_id
                     LEFT JOIN users au ON au.id = c.assignee_id
                     LEFT JOIN users su ON m.sender_type = 'User' AND su.id = m.sender_id
                     LEFT JOIN contacts sc ON m.sender_type = 'Contact' AND sc.id = m.sender_id
                     WHERE m.account_id = $1
                       AND m.message_type = $6
                       AND ($2::timestamp IS NULL OR m.created_at >= $2)
                       AND ($3::timestamp IS NULL OR m.created_at <= $3)
                       AND ({dim_sql})
                     ORDER BY m.created_at DESC, m.id DESC
                     LIMIT $7 OFFSET $8"
                ))
                .bind(account_id)
                .bind(since)
                .bind(until)
                .bind(dim_type)
                .bind(dim_id)
                .bind(mtype)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&st.db)
                .await?;

                let items = rows
                    .into_iter()
                    .map(|r| {
                        let m_created: NaiveDateTime = r.get("m_created");
                        let c_created: NaiveDateTime = r.get("c_created");
                        let c_last: NaiveDateTime = r.get("c_last");
                        json!({
                            "record_type": "message",
                            "conversation": conv_drilldown_json(
                                r.get("cid"), r.get("did"), r.get("contact_id"), r.get("contact_name"),
                                r.get("inbox_id"), r.get("inbox_name"), r.get("assignee_id"),
                                r.get("assignee_name"), r.get("status"), c_created, c_last,
                            ),
                            "message": {
                                "id": r.get::<i64, _>("mid"),
                                "content": r.get::<Option<String>, _>("content"),
                                "message_type": r.get::<i32, _>("message_type"),
                                "sender_name": r.get::<Option<String>, _>("sender_name"),
                                "created_at": m_created.and_utc().timestamp(),
                            },
                            "metric_value": Value::Null,
                            "occurred_at": m_created.and_utc().timestamp(),
                        })
                    })
                    .collect();
                ("message", total, conv_cnt, items)
            }
            _ => {
                let (ev_name, rec_type) = match metric.as_str() {
                    "avg_first_response_time" => ("first_response", "message"),
                    "reply_time" => ("reply_time", "message"),
                    "bot_resolutions_count" => ("conversation_bot_resolved", "conversation"),
                    "bot_handoffs_count" => ("conversation_bot_handoff", "conversation"),
                    _ => ("conversation_resolved", "conversation"),
                };
                let (total, conv_cnt): (i64, i64) = sqlx::query_as(&format!(
                    "SELECT COUNT(re.id)::bigint, COUNT(DISTINCT re.conversation_id)::bigint
                     FROM reporting_events re
                     JOIN conversations c ON c.id = re.conversation_id AND c.account_id = re.account_id
                     WHERE re.account_id = $1
                       AND re.name = $6
                       AND ($2::timestamp IS NULL OR re.created_at >= $2)
                       AND ($3::timestamp IS NULL OR re.created_at <= $3)
                       AND ({dim_sql})"
                ))
                .bind(account_id)
                .bind(since)
                .bind(until)
                .bind(dim_type)
                .bind(dim_id)
                .bind(ev_name)
                .fetch_one(&st.db)
                .await?;

                let use_bh = params
                    .business_hours
                    .as_ref()
                    .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
                    .unwrap_or(false);

                let rows = sqlx::query(&format!(
                    "SELECT re.name AS ev_name, re.value::float8 AS ev_val,
                            COALESCE(re.value_in_business_hours, re.value)::float8 AS ev_bh,
                            COALESCE(re.event_end_time, re.created_at) AS ev_at,
                            c.id::bigint AS cid, c.display_id::bigint AS did,
                            c.contact_id::bigint AS contact_id, ct.name AS contact_name,
                            c.inbox_id::bigint AS inbox_id, i.name AS inbox_name,
                            c.assignee_id::bigint AS assignee_id, au.name AS assignee_name,
                            c.status, c.created_at AS c_created, c.last_activity_at AS c_last
                     FROM reporting_events re
                     JOIN conversations c ON c.id = re.conversation_id AND c.account_id = re.account_id
                     LEFT JOIN contacts ct ON ct.id = c.contact_id
                     LEFT JOIN inboxes i ON i.id = c.inbox_id
                     LEFT JOIN users au ON au.id = c.assignee_id
                     WHERE re.account_id = $1
                       AND re.name = $6
                       AND ($2::timestamp IS NULL OR re.created_at >= $2)
                       AND ($3::timestamp IS NULL OR re.created_at <= $3)
                       AND ({dim_sql})
                     ORDER BY re.created_at DESC, re.id DESC
                     LIMIT $7 OFFSET $8"
                ))
                .bind(account_id)
                .bind(since)
                .bind(until)
                .bind(dim_type)
                .bind(dim_id)
                .bind(ev_name)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&st.db)
                .await?;

                let items = rows
                    .into_iter()
                    .map(|r| {
                        let ev_at: NaiveDateTime = r.get("ev_at");
                        let c_created: NaiveDateTime = r.get("c_created");
                        let c_last: NaiveDateTime = r.get("c_last");
                        let val: f64 = if use_bh { r.get("ev_bh") } else { r.get("ev_val") };
                        json!({
                            "record_type": rec_type,
                            "event_name": r.get::<String, _>("ev_name"),
                            "conversation": conv_drilldown_json(
                                r.get("cid"), r.get("did"), r.get("contact_id"), r.get("contact_name"),
                                r.get("inbox_id"), r.get("inbox_name"), r.get("assignee_id"),
                                r.get("assignee_name"), r.get("status"), c_created, c_last,
                            ),
                            "message": Value::Null,
                            "metric_value": val,
                            "occurred_at": ev_at.and_utc().timestamp(),
                        })
                    })
                    .collect();
                (rec_type, total, conv_cnt, items)
            }
        };

    Ok(Json(json!({
        "meta": {
            "metric": metric,
            "record_type": record_type,
            "current_page": page,
            "per_page": per_page,
            "total_count": total_count,
            "conversation_count": conversation_count,
        },
        "payload": payload,
    })))
}

async fn reports_bot_metrics(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    // mirrors chatwoot/app/builders/v2/reports/bot_metrics_builder.rb (empty range = no rows, as Rails `where(created_at: nil)`)
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());
    let (convs, msgs, resolved, handoffs): (i64, i64, i64, i64) = sqlx::query_as(
        "WITH bi AS (
           SELECT i.id FROM inboxes i WHERE i.account_id=$1 AND (
             EXISTS (SELECT 1 FROM agent_bot_inboxes abi JOIN agent_bots ab ON ab.id=abi.agent_bot_id WHERE abi.inbox_id=i.id AND abi.status=0)
             OR EXISTS (SELECT 1 FROM integrations_hooks h WHERE h.inbox_id=i.id AND h.app_id='dialogflow' AND h.status=1))),
         bc AS (SELECT c.id FROM conversations c WHERE c.account_id=$1 AND c.inbox_id IN (SELECT id FROM bi) AND c.created_at >= $2 AND c.created_at < $3),
         ho AS (SELECT DISTINCT r.conversation_id FROM reporting_events r JOIN conversations c ON c.id=r.conversation_id WHERE r.account_id=$1 AND r.name='conversation_bot_handoff' AND r.created_at >= $2 AND r.created_at < $3)
         SELECT (SELECT count(*) FROM bc),
                (SELECT count(*) FROM messages m WHERE m.account_id=$1 AND m.message_type=1 AND m.private=false AND m.sender_type IN ('AgentBot', 'Captain::Assistant') AND m.conversation_id IN (SELECT id FROM bc) AND m.created_at >= $2 AND m.created_at < $3),
                (SELECT count(DISTINCT r.conversation_id) FROM reporting_events r JOIN conversations c ON c.id=r.conversation_id WHERE r.account_id=$1 AND r.name='conversation_bot_resolved' AND r.created_at >= $2 AND r.created_at < $3 AND r.conversation_id NOT IN (SELECT conversation_id FROM ho)),
                (SELECT count(*) FROM ho)",
    )
    .bind(account_id)
    .bind(since)
    .bind(until)
    .fetch_one(&st.db)
    .await?;
    let rate = |n: i64| {
        if convs == 0 {
            0
        } else {
            (n as f64 / convs as f64 * 100.0) as i64
        }
    };
    Ok(Json(json!({
        "conversation_count": convs,
        "message_count": msgs,
        "resolution_rate": rate(resolved),
        "handoff_rate": rate(handoffs)
    })))
}

fn query_param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn query_i64_list(params: &[(String, String)], key: &str) -> Vec<i64> {
    let bracketed = format!("{key}[]");
    let mut ids = Vec::new();
    for (k, v) in params {
        if k != key && *k != bracketed {
            continue;
        }
        for part in v.split(',') {
            if let Ok(id) = part.trim().parse::<i64>() {
                ids.push(id);
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

async fn reports_inbox_label_matrix(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<Vec<(String, String)>>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let since = parse_ts(query_param(&q, "since"));
    let until = parse_ts(query_param(&q, "until"));
    let inbox_filter = query_i64_list(&q, "inbox_ids");
    let label_filter = query_i64_list(&q, "label_ids");

    let inboxes: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id::bigint, name FROM inboxes
         WHERE account_id = $1 AND ($2::bigint[] IS NULL OR id = ANY($2))
         ORDER BY name ASC, id ASC",
    )
    .bind(account_id)
    .bind((!inbox_filter.is_empty()).then_some(inbox_filter.as_slice()))
    .fetch_all(&st.db)
    .await?;

    let labels: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id::bigint, title FROM labels
         WHERE account_id = $1 AND ($2::bigint[] IS NULL OR id = ANY($2))
         ORDER BY title ASC, id ASC",
    )
    .bind(account_id)
    .bind((!label_filter.is_empty()).then_some(label_filter.as_slice()))
    .fetch_all(&st.db)
    .await?;

    let label_titles: Vec<&str> = labels.iter().map(|(_, t)| t.as_str()).collect();
    let inbox_ids: Vec<i64> = inboxes.iter().map(|(id, _)| *id).collect();
    let mut counts: std::collections::HashMap<(i64, String), i64> = std::collections::HashMap::new();
    if !inbox_ids.is_empty() && !label_titles.is_empty() {
        let rows: Vec<(i64, String, i64)> = sqlx::query_as(
            "SELECT c.inbox_id::bigint, t.name, COUNT(*)::bigint
             FROM taggings tg
             JOIN conversations c ON c.id = tg.taggable_id
             JOIN tags t ON t.id = tg.tag_id
             WHERE tg.taggable_type = 'Conversation'
               AND tg.context = 'labels'
               AND c.account_id = $1
               AND c.inbox_id = ANY($2)
               AND t.name = ANY($3)
               AND ($4::timestamp IS NULL OR c.created_at >= $4)
               AND ($5::timestamp IS NULL OR c.created_at <= $5)
             GROUP BY c.inbox_id, t.name",
        )
        .bind(account_id)
        .bind(&inbox_ids)
        .bind(&label_titles)
        .bind(since)
        .bind(until)
        .fetch_all(&st.db)
        .await?;
        for (iid, title, cnt) in rows {
            counts.insert((iid, title), cnt);
        }
    }

    let matrix: Vec<Vec<i64>> = inboxes
        .iter()
        .map(|(iid, _)| {
            labels
                .iter()
                .map(|(_, title)| *counts.get(&(*iid, title.clone())).unwrap_or(&0))
                .collect()
        })
        .collect();

    Ok(Json(json!({
        "inboxes": inboxes.into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
        "labels": labels.into_iter().map(|(id, title)| json!({ "id": id, "title": title })).collect::<Vec<_>>(),
        "matrix": matrix,
    })))
}

async fn reports_first_response_time_distribution(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());

    let rows: Vec<(String, i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT COALESCE(i.channel_type, 'Channel::Api'),
                COUNT(CASE WHEN re.value < 3600 THEN 1 END)::bigint,
                COUNT(CASE WHEN re.value >= 3600 AND re.value < 14400 THEN 1 END)::bigint,
                COUNT(CASE WHEN re.value >= 14400 AND re.value < 28800 THEN 1 END)::bigint,
                COUNT(CASE WHEN re.value >= 28800 AND re.value < 86400 THEN 1 END)::bigint,
                COUNT(CASE WHEN re.value >= 86400 THEN 1 END)::bigint
         FROM reporting_events re
         JOIN inboxes i ON i.id = re.inbox_id AND i.account_id = re.account_id
         WHERE re.account_id = $1
           AND re.name = 'first_response'
           AND ($2::timestamp IS NULL OR re.created_at >= $2)
           AND ($3::timestamp IS NULL OR re.created_at <= $3)
         GROUP BY i.channel_type",
    )
    .bind(account_id)
    .bind(since)
    .bind(until)
    .fetch_all(&st.db)
    .await?;

    let mut out = serde_json::Map::new();
    for (channel_type, b0, b1, b2, b3, b4) in rows {
        out.insert(
            channel_type,
            json!({
                "0-1h": b0,
                "1-4h": b1,
                "4-8h": b2,
                "8-24h": b3,
                "24h+": b4,
            }),
        );
    }
    Ok(Json(Value::Object(out)))
}

async fn reports_outgoing_messages_count(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(params): Query<ReportParams>,
) -> ApiResult {
    let _au = require_report_viewer(&st.db, &user, account_id).await?;
    let since = parse_ts(params.since.as_deref());
    let until = parse_ts(params.r#until.as_deref());
    let group_by = params.group_by.as_deref().unwrap_or("agent");

    let list: Vec<Value> = match group_by {
        "team" => {
            let rows: Vec<(i64, String, i64)> = sqlx::query_as(
                "SELECT t.id::bigint, t.name, COUNT(m.id)::bigint AS cnt
                 FROM messages m
                 JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
                 JOIN teams t ON t.id = c.team_id AND t.account_id = m.account_id
                 WHERE m.account_id = $1
                   AND m.message_type = 1
                   AND ($2::timestamp IS NULL OR m.created_at >= $2)
                   AND ($3::timestamp IS NULL OR m.created_at <= $3)
                 GROUP BY t.id, t.name
                 ORDER BY cnt DESC, t.id ASC",
            )
            .bind(account_id)
            .bind(since)
            .bind(until)
            .fetch_all(&st.db)
            .await?;
            rows.into_iter()
                .map(|(id, name, cnt)| json!({ "id": id, "name": name, "outgoing_messages_count": cnt }))
                .collect()
        }
        "inbox" => {
            let rows: Vec<(i64, String, i64)> = sqlx::query_as(
                "SELECT i.id::bigint, i.name, COUNT(m.id)::bigint AS cnt
                 FROM messages m
                 JOIN inboxes i ON i.id = m.inbox_id AND i.account_id = m.account_id
                 WHERE m.account_id = $1
                   AND m.message_type = 1
                   AND ($2::timestamp IS NULL OR m.created_at >= $2)
                   AND ($3::timestamp IS NULL OR m.created_at <= $3)
                 GROUP BY i.id, i.name
                 ORDER BY cnt DESC, i.id ASC",
            )
            .bind(account_id)
            .bind(since)
            .bind(until)
            .fetch_all(&st.db)
            .await?;
            rows.into_iter()
                .map(|(id, name, cnt)| json!({ "id": id, "name": name, "outgoing_messages_count": cnt }))
                .collect()
        }
        "label" => {
            let rows: Vec<(Option<i64>, String, i64)> = sqlx::query_as(
                "SELECT MIN(l.id)::bigint, t.name, COUNT(m.id)::bigint AS cnt
                 FROM messages m
                 JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id
                 JOIN taggings tg ON tg.taggable_id = c.id AND tg.taggable_type = 'Conversation' AND tg.context = 'labels'
                 JOIN tags t ON t.id = tg.tag_id
                 LEFT JOIN labels l ON l.title = t.name AND l.account_id = m.account_id
                 WHERE m.account_id = $1
                   AND m.message_type = 1
                   AND ($2::timestamp IS NULL OR m.created_at >= $2)
                   AND ($3::timestamp IS NULL OR m.created_at <= $3)
                 GROUP BY t.name
                 ORDER BY cnt DESC, t.name ASC",
            )
            .bind(account_id)
            .bind(since)
            .bind(until)
            .fetch_all(&st.db)
            .await?;
            rows.into_iter()
                .map(|(id, name, cnt)| json!({ "id": id, "name": name, "outgoing_messages_count": cnt }))
                .collect()
        }
        _ => {
            let rows: Vec<(i64, String, i64)> = sqlx::query_as(
                "SELECT u.id::bigint, u.name, COUNT(m.id)::bigint AS cnt
                 FROM messages m
                 JOIN users u ON u.id = m.sender_id
                 WHERE m.account_id = $1
                   AND m.message_type = 1
                   AND m.sender_type = 'User'
                   AND m.sender_id IS NOT NULL
                   AND ($2::timestamp IS NULL OR m.created_at >= $2)
                   AND ($3::timestamp IS NULL OR m.created_at <= $3)
                 GROUP BY u.id, u.name
                 ORDER BY cnt DESC, u.id ASC",
            )
            .bind(account_id)
            .bind(since)
            .bind(until)
            .fetch_all(&st.db)
            .await?;
            rows.into_iter()
                .map(|(id, name, cnt)| json!({ "id": id, "name": name, "outgoing_messages_count": cnt }))
                .collect()
        }
    };
    Ok(Json(Value::Array(list)))
}

async fn year_in_review_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    // Chatwoot api/v2/accounts/year_in_reviews_controller.rb: year defaults to 2025, result cached in user.ui_settings.
    let year: i32 = q.get("year").and_then(|y| y.parse().ok()).unwrap_or(2025);
    let cache_key = format!("year_in_review_{account_id}_{year}");
    let cached: Option<Value> =
        sqlx::query_scalar("SELECT ui_settings -> $1 FROM users WHERE id = $2")
            .bind(&cache_key)
            .bind(user.id)
            .fetch_one(&st.db)
            .await?;
    if let Some(c) = cached.filter(|v| !v.is_null()) {
        return Ok(Json(c));
    }
    let start = chrono::NaiveDate::from_ymd_opt(year, 1, 1)
        .ok_or_else(|| ApiError::bad("invalid year"))?
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(year, 12, 31)
        .ok_or_else(|| ApiError::bad("invalid year"))?
        .and_hms_opt(23, 59, 59)
        .unwrap();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM conversations WHERE account_id = $1 AND assignee_id = $2 AND created_at BETWEEN $3 AND $4")
        .bind(account_id as i32)
        .bind(user.id as i32)
        .bind(start)
        .bind(end)
        .fetch_one(&st.db)
        .await?;
    let busiest: Option<(chrono::NaiveDate, i64)> = sqlx::query_as(
        "SELECT created_at::date, COUNT(*)::bigint FROM conversations WHERE account_id = $1 AND assignee_id = $2 AND created_at BETWEEN $3 AND $4
         GROUP BY created_at::date ORDER BY COUNT(*) DESC, created_at::date ASC LIMIT 1",
    )
    .bind(account_id as i32)
    .bind(user.id as i32)
    .bind(start)
    .bind(end)
    .fetch_optional(&st.db)
    .await?;
    let avg: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(value)::float8 FROM reporting_events WHERE account_id = $1 AND name = 'first_response' AND user_id = $2 AND created_at BETWEEN $3 AND $4",
    )
    .bind(account_id as i32)
    .bind(user.id as i32)
    .bind(start)
    .bind(end)
    .fetch_one(&st.db)
    .await?;
    let data = json!({
        "year": year,
        "total_conversations": total,
        "busiest_day": busiest.filter(|(_, c)| *c > 0).map(|(d, c)| json!({ "date": d.format("%b %d").to_string(), "count": c })),
        "support_personality": { "avg_response_time_seconds": avg.map(|a| a as i64).unwrap_or(0) },
    });
    sqlx::query("UPDATE users SET ui_settings = COALESCE(ui_settings,'{}'::jsonb) || jsonb_build_object($1::text, $2::jsonb), updated_at = now() WHERE id = $3")
        .bind(&cache_key)
        .bind(&data)
        .bind(user.id)
        .execute(&st.db)
        .await?;
    Ok(Json(data))
}
