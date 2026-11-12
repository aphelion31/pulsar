//! Integration hooks and CSAT survey responses. Mirrors chatwoot
//! app/controllers/api/v1/accounts/{integrations/hooks,csat_survey_responses}_controller.rb.
use crate::common::*;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rand::{Rng, distributions::Alphanumeric};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/integrations/hooks", post(hook_create))
        .route(
            "/integrations/hooks/{id}",
            axum::routing::patch(hook_update).delete(hook_destroy),
        )
        .route("/csat_survey_responses", get(csat_index))
        .route("/csat_survey_responses/metrics", get(csat_metrics))
        .route("/csat_survey_responses/download", get(csat_download))
}

// ---------- integrations/hooks ----------

const HOOK_SELECT: &str = "SELECT jsonb_build_object('id',h.id,'app_id',h.app_id,'status',h.status=1,'inbox',CASE WHEN i.id IS NULL THEN NULL ELSE jsonb_build_object('id',i.id,'name',i.name) END,'account_id',h.account_id,'hook_type',CASE h.hook_type WHEN 1 THEN 'inbox' ELSE 'account' END,'settings',COALESCE(h.settings,'{}'::jsonb),'reference_id',h.reference_id) FROM integrations_hooks h LEFT JOIN inboxes i ON i.id=h.inbox_id AND i.account_id=h.account_id WHERE h.account_id=$1::bigint AND h.id=$2::bigint";

async fn hook_get(st: &AppState, a: i64, id: i64) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar(HOOK_SELECT)
        .bind(a)
        .bind(id)
        .fetch_one(&st.db)
        .await?)
}

/// Chatwoot: `params.require(:hook).permit(:app_id, :inbox_id, :status, settings: {})`.
fn hook_body(v: &Value) -> &Value {
    v.get("hook").filter(|x| x.is_object()).unwrap_or(v)
}

/// Chatwoot `status` enum: disabled=0, enabled=1. Accepts the int, the name or a boolean.
fn status_int(v: &Value) -> Result<Option<i32>, ApiError> {
    match v {
        Value::Null => Ok(None),
        Value::Bool(b) => Ok(Some(i32::from(*b))),
        Value::Number(n) if n.as_i64() == Some(0) || n.as_i64() == Some(1) => Ok(n.as_i64().map(|x| x as i32)),
        Value::String(s) if s == "enabled" => Ok(Some(1)),
        Value::String(s) if s == "disabled" => Ok(Some(0)),
        _ => Err(ApiError::bad("status is invalid")),
    }
}

fn new_access_token() -> String {
    rand::thread_rng().sample_iter(&Alphanumeric).take(24).map(char::from).collect()
}

async fn hook_create(State(st): State<AppState>, user: CurrentUser, Path(a): Path<i64>, Json(body): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let h = hook_body(&body);
    let app_id = h
        .get("app_id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::bad("app_id is required"))?;
    // Stripe is connected through its OAuth flow only (Chatwoot reject_stripe_hook).
    if app_id == "stripe" {
        return Err(ApiError::not_found());
    }
    let inbox_id = h.get("inbox_id").and_then(Value::as_i64);
    if let Some(i) = inbox_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inboxes WHERE account_id=$1::bigint AND id=$2::bigint)")
            .bind(a)
            .bind(i)
            .fetch_one(&st.db)
            .await?;
        if !ok {
            return Err(ApiError::bad("inbox must belong to account"));
        }
    }
    let status = status_int(h.get("status").unwrap_or(&Value::Null))?.unwrap_or(1);
    let settings = h.get("settings").filter(|s| s.is_object()).cloned().unwrap_or_else(|| json!({}));
    let hook_type: i32 = if inbox_id.is_some() { 1 } else { 0 };
    // Chatwoot validates app_id uniqueness per account (apps that allow several hooks are not modelled here).
    let dup: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM integrations_hooks WHERE account_id=$1::bigint AND app_id=$2)")
        .bind(a)
        .bind(app_id)
        .fetch_one(&st.db)
        .await?;
    if dup {
        return Err(ApiError::bad("App has already been taken"));
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO integrations_hooks (account_id, app_id, inbox_id, status, hook_type, settings, access_token, created_at, updated_at)
         VALUES ($1::bigint, $2, $3::bigint, $4, $5, $6, $7, now(), now()) RETURNING id::bigint",
    )
    .bind(a)
    .bind(app_id)
    .bind(inbox_id)
    .bind(status)
    .bind(hook_type)
    .bind(settings)
    .bind(new_access_token())
    .fetch_one(&st.db)
    .await?;
    Ok(Json(hook_get(&st, a, id).await?))
}

async fn hook_update(State(st): State<AppState>, user: CurrentUser, Path((a, id)): Path<(i64, i64)>, Json(body): Json<Value>) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let h = hook_body(&body);
    let app_id: String = sqlx::query_scalar("SELECT COALESCE(app_id,'') FROM integrations_hooks WHERE account_id=$1::bigint AND id=$2::bigint")
        .bind(a)
        .bind(id)
        .fetch_optional(&st.db)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if app_id == "stripe" {
        return Err(ApiError::not_found());
    }
    // Chatwoot: update!(permitted_params.slice(:status, :settings)). app_id, inbox_id and hook_type are read-only.
    let status = status_int(h.get("status").unwrap_or(&Value::Null))?;
    let settings = h.get("settings").filter(|s| s.is_object()).cloned();
    sqlx::query(
        "UPDATE integrations_hooks SET status = COALESCE($3, status), settings = COALESCE($4, settings), updated_at = now()
         WHERE account_id=$1::bigint AND id=$2::bigint",
    )
    .bind(a)
    .bind(id)
    .bind(status)
    .bind(settings)
    .execute(&st.db)
    .await?;
    Ok(Json(hook_get(&st, a, id).await?))
}

async fn hook_destroy(State(st): State<AppState>, user: CurrentUser, Path((a, id)): Path<(i64, i64)>) -> Result<Response, ApiError> {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let n = sqlx::query("DELETE FROM integrations_hooks WHERE account_id=$1::bigint AND id=$2::bigint AND COALESCE(app_id,'') <> 'stripe'")
        .bind(a)
        .bind(id)
        .execute(&st.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::OK.into_response())
}

// ---------- csat_survey_responses ----------

#[derive(Deserialize, Default)]
struct CsatParams {
    since: Option<String>,
    until: Option<String>,
    /// Chatwoot passes `user_ids[]`, `rating[]`, `inbox_id`, `team_id` as comma lists or repeated keys.
    #[serde(alias = "user_ids[]")]
    user_ids: Option<String>,
    #[serde(alias = "rating[]")]
    rating: Option<String>,
    inbox_id: Option<String>,
    team_id: Option<String>,
    page: Option<String>,
}

/// Parsed filters. Each is always bound (NULL = not filtered), so the SQL placeholders stay fixed.
struct Filter {
    account: i64,
    since: Option<i64>,
    until: Option<i64>,
    user_ids: Option<Vec<i64>>,
    ratings: Option<Vec<i32>>,
    inbox: Option<i64>,
    team: Option<i64>,
}

fn ids<T: std::str::FromStr>(s: &Option<String>) -> Option<Vec<T>> {
    let v: Vec<T> = s.as_deref()?.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    (!v.is_empty()).then_some(v)
}

fn one<T: std::str::FromStr>(s: &Option<String>) -> Option<T> {
    s.as_deref()?.trim().parse().ok()
}

fn filter(account: i64, p: &CsatParams) -> Filter {
    // Chatwoot DateRangeHelper#range: both bounds or none (epoch seconds).
    let (since, until) = match (one::<i64>(&p.since), one::<i64>(&p.until)) {
        (Some(s), Some(u)) => (Some(s), Some(u)),
        _ => (None, None),
    };
    Filter {
        account,
        since,
        until,
        user_ids: ids(&p.user_ids),
        ratings: ids::<i32>(&p.rating),
        inbox: one(&p.inbox_id),
        team: one(&p.team_id),
    }
}

/// Fixed WHERE clause; `r` = csat_survey_responses, `c` = conversations. Bind order: see `bind_filter!`.
const CSAT_FROM: &str = " FROM csat_survey_responses r LEFT JOIN conversations c ON c.id = r.conversation_id AND c.account_id = r.account_id";
const CSAT_WHERE: &str = " WHERE r.account_id = $1::bigint \
    AND ($2::bigint IS NULL OR r.created_at >= (to_timestamp($2::bigint) AT TIME ZONE 'UTC')) \
    AND ($3::bigint IS NULL OR r.created_at < (to_timestamp($3::bigint) AT TIME ZONE 'UTC')) \
    AND ($4::bigint[] IS NULL OR r.assigned_agent_id = ANY($4::bigint[])) \
    AND ($5::int[] IS NULL OR r.rating = ANY($5::int[])) \
    AND ($6::bigint IS NULL OR c.inbox_id = $6::bigint) \
    AND ($7::bigint IS NULL OR c.team_id = $7::bigint)";

macro_rules! bind_filter {
    ($q:expr, $f:expr) => {{
        let f = &$f;
        $q.bind(f.account)
            .bind(f.since)
            .bind(f.until)
            .bind(f.user_ids.clone())
            .bind(f.ratings.clone())
            .bind(f.inbox)
            .bind(f.team)
    }};
}

async fn csat_index(State(st): State<AppState>, user: CurrentUser, Path(a): Path<i64>, Query(p): Query<CsatParams>) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let f = filter(a, &p);
    let page = one::<i64>(&p.page).unwrap_or(1).max(1);
    let sql = format!(
        "SELECT jsonb_build_object('id',r.id,'rating',r.rating,'feedback_message',r.feedback_message,'csat_review_notes',r.csat_review_notes,\
         'review_notes_updated_at',EXTRACT(EPOCH FROM r.review_notes_updated_at)::bigint,'account_id',r.account_id,'message_id',r.message_id,\
         'conversation_id',c.display_id,'created_at',EXTRACT(EPOCH FROM r.created_at)::bigint) \
         {CSAT_FROM}{CSAT_WHERE} ORDER BY r.created_at DESC, r.id DESC LIMIT 25 OFFSET $8::bigint"
    );
    let rows: Vec<Value> = bind_filter!(sqlx::query_scalar::<_, Value>(&sql), f)
        .bind((page - 1) * 25)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(json!(rows)))
}

async fn csat_metrics(State(st): State<AppState>, user: CurrentUser, Path(a): Path<i64>, Query(p): Query<CsatParams>) -> ApiResult {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let f = filter(a, &p);
    let total_sql = format!("SELECT COUNT(*){CSAT_FROM}{CSAT_WHERE}");
    let total: i64 = bind_filter!(sqlx::query_scalar::<_, i64>(&total_sql), f).fetch_one(&st.db).await?;
    let ratings_sql = format!("SELECT r.rating, COUNT(*){CSAT_FROM}{CSAT_WHERE} GROUP BY r.rating");
    let ratings: Vec<(i32, i64)> = bind_filter!(sqlx::query_as::<_, (i32, i64)>(&ratings_sql), f).fetch_all(&st.db).await?;
    let ratings_count: serde_json::Map<String, Value> = ratings.into_iter().map(|(k, v)| (k.to_string(), json!(v))).collect();

    // Chatwoot: total_sent_messages_count = input_csat messages in the date range, not the response filters.
    // content_type 9 = "input_csat" in the Pulsar CONTENT_TYPES list (conversations.rs).
    let sent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages m WHERE m.account_id = $1::bigint AND m.content_type = 9 \
         AND ($2::bigint IS NULL OR m.created_at >= (to_timestamp($2::bigint) AT TIME ZONE 'UTC')) \
         AND ($3::bigint IS NULL OR m.created_at < (to_timestamp($3::bigint) AT TIME ZONE 'UTC'))",
    )
    .bind(a)
    .bind(f.since)
    .bind(f.until)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(json!({
        "total_count": total,
        "ratings_count": ratings_count,
        "total_sent_messages_count": sent,
    })))
}

/// CSVSafe: quote every cell, double embedded quotes, and prefix formula-leading cells with `'`.
fn csv_cell(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@']) { format!("'{s}") } else { s.to_string() };
    format!("\"{}\"", s.replace('"', "\"\""))
}

type CsvRow = (String, String, i32, String, String, String, String, Option<i32>, String);

async fn csat_download(State(st): State<AppState>, user: CurrentUser, Path(a): Path<i64>, Query(p): Query<CsatParams>) -> Result<Response, ApiError> {
    require_account(&st.db, &user, a).await?.require_admin()?;
    let f = filter(a, &p);
    let sql = format!(
        "SELECT COALESCE(u.name,''), COALESCE(u.email,''), r.rating, COALESCE(r.feedback_message,''), \
         COALESCE(ct.name,''), COALESCE(ct.email,''), COALESCE(ct.phone_number,''), c.display_id, \
         to_char(r.created_at,'YYYY-MM-DD HH24:MI:SS') \
         {CSAT_FROM} LEFT JOIN users u ON u.id = r.assigned_agent_id \
         LEFT JOIN contacts ct ON ct.id = r.contact_id AND ct.account_id = r.account_id{CSAT_WHERE} \
         ORDER BY r.created_at DESC, r.id DESC"
    );
    let rows: Vec<(String, String, i32, String, String, String, String, Option<i32>, String)> =
        bind_filter!(sqlx::query_as::<_, CsvRow>(&sql), f).fetch_all(&st.db).await?;

    let mut out = String::from("\u{feff}Agent Name,Rating,Feedback,Contact Name,Contact Email Address,Contact Phone Number,Link to the conversation,Recorded at\r\n");
    for (agent, agent_email, rating, feedback, cname, cemail, cphone, conv, recorded) in rows {
        let agent_cell = if agent.is_empty() && agent_email.is_empty() { String::new() } else { format!("{agent} ({agent_email})") };
        let link = conv.map(|c| format!("/app/accounts/{a}/conversations/{c}")).unwrap_or_default();
        let cells = [agent_cell, rating.to_string(), feedback, cname, cemail, cphone, link, recorded];
        out.push_str(&cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/csv")),
            (header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment; filename=csat_report.csv")),
        ],
        out,
    )
        .into_response())
}
