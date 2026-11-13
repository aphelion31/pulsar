use crate::common::*;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde_json::{Value, json};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/assignment_policies", get(index).post(create))
        .route(
            "/assignment_policies/{id}",
            get(show).patch(update).put(update).delete(destroy),
        )
        .route("/assignment_policies/{id}/inboxes", get(policy_inboxes))
        .route(
            "/inboxes/{id}/assignment_policy",
            get(inbox_show).post(inbox_create).delete(inbox_destroy),
        )
}
const SELECT: &str = "SELECT jsonb_build_object('id',p.id,'name',p.name,'description',p.description,'assignment_order',CASE p.assignment_order WHEN 1 THEN 'balanced' ELSE 'round_robin' END,'conversation_priority',CASE p.conversation_priority WHEN 1 THEN 'longest_waiting' ELSE 'earliest_created' END,'fair_distribution_limit',p.fair_distribution_limit,'fair_distribution_window',p.fair_distribution_window,'exclude_older_than_hours',p.exclude_older_than_hours,'enabled',p.enabled,'assigned_inbox_count',(SELECT count(*) FROM inbox_assignment_policies ip WHERE ip.assignment_policy_id=p.id),'created_at',floor(extract(epoch from p.created_at))::bigint,'updated_at',floor(extract(epoch from p.updated_at))::bigint) FROM assignment_policies p WHERE p.account_id=$1";
async fn admin(st: &AppState, user: &CurrentUser, a: i64) -> Result<(), ApiError> {
    require_account(&st.db, user, a).await?.require_admin()?;
    Ok(())
}
async fn policy(st: &AppState, a: i64, id: i64) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar(&format!("{SELECT} AND p.id=$2"))
        .bind(a)
        .bind(id)
        .fetch_one(&st.db)
        .await?)
}
fn payload(v: &Value) -> &Value {
    v.get("assignment_policy")
        .filter(|x| x.is_object())
        .unwrap_or(v)
}
fn validate(v: &Value) -> Result<(), ApiError> {
    if !v["name"].as_str().is_some_and(|s| !s.trim().is_empty()) {
        return Err(ApiError::bad("Name can't be blank"));
    }
    if !matches!(
        v["assignment_order"].as_str(),
        Some("round_robin" | "balanced")
    ) {
        return Err(ApiError::bad("Invalid assignment order"));
    }
    if !matches!(
        v["conversation_priority"].as_str(),
        Some("earliest_created" | "longest_waiting")
    ) {
        return Err(ApiError::bad("Invalid conversation priority"));
    }
    for k in ["fair_distribution_limit", "fair_distribution_window"] {
        if !v[k].as_i64().is_some_and(|n| n > 0 && n <= i32::MAX as i64) {
            return Err(ApiError::bad(format!("Invalid {k}")));
        }
    }
    if !v["exclude_older_than_hours"].is_null()
        && !v["exclude_older_than_hours"]
            .as_i64()
            .is_some_and(|n| n > 0 && n <= i32::MAX as i64)
    {
        return Err(ApiError::bad("Invalid exclude_older_than_hours"));
    }
    if !v["enabled"].is_boolean() || !(v["description"].is_null() || v["description"].is_string()) {
        return Err(ApiError::bad("Invalid policy"));
    }
    Ok(())
}
fn merge(dst: &mut Value, src: &Value) {
    for k in [
        "name",
        "description",
        "assignment_order",
        "conversation_priority",
        "fair_distribution_limit",
        "fair_distribution_window",
        "exclude_older_than_hours",
        "enabled",
    ] {
        if let Some(v) = src.get(k) {
            dst[k] = v.clone();
        }
    }
}
fn db_error(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref d) = e {
        if d.is_unique_violation() {
            return ApiError::bad("Name has already been taken");
        }
    }
    e.into()
}
async fn index(State(st): State<AppState>, user: CurrentUser, Path(a): Path<i64>) -> ApiResult {
    admin(&st, &user, a).await?;
    let rows: Vec<Value> = sqlx::query_scalar(&format!("{SELECT} ORDER BY p.id"))
        .bind(a)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(json!(rows)))
}
async fn show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    admin(&st, &user, a).await?;
    Ok(Json(policy(&st, a, id).await?))
}
async fn create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(a): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    admin(&st, &user, a).await?;
    let mut v = json!({"name":null,"description":null,"assignment_order":"round_robin","conversation_priority":"earliest_created","fair_distribution_limit":100,"fair_distribution_window":3600,"exclude_older_than_hours":168,"enabled":true});
    merge(&mut v, payload(&body));
    validate(&v)?;
    let id:i64=sqlx::query_scalar("INSERT INTO assignment_policies(account_id,name,description,assignment_order,conversation_priority,fair_distribution_limit,fair_distribution_window,exclude_older_than_hours,enabled,created_at,updated_at) VALUES($1,$2->>'name',$2->>'description',CASE WHEN $2->>'assignment_order'='balanced' THEN 1 ELSE 0 END,CASE WHEN $2->>'conversation_priority'='longest_waiting' THEN 1 ELSE 0 END,($2->>'fair_distribution_limit')::int,($2->>'fair_distribution_window')::int,($2->>'exclude_older_than_hours')::int,($2->>'enabled')::boolean,now(),now()) RETURNING id").bind(a).bind(&v).fetch_one(&st.db).await.map_err(db_error)?;
    Ok(Json(policy(&st, a, id).await?))
}
async fn update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    admin(&st, &user, a).await?;
    let mut v = policy(&st, a, id).await?;
    merge(&mut v, payload(&body));
    validate(&v)?;
    sqlx::query("UPDATE assignment_policies SET name=$3->>'name',description=$3->>'description',assignment_order=CASE WHEN $3->>'assignment_order'='balanced' THEN 1 ELSE 0 END,conversation_priority=CASE WHEN $3->>'conversation_priority'='longest_waiting' THEN 1 ELSE 0 END,fair_distribution_limit=($3->>'fair_distribution_limit')::int,fair_distribution_window=($3->>'fair_distribution_window')::int,exclude_older_than_hours=($3->>'exclude_older_than_hours')::int,enabled=($3->>'enabled')::boolean,updated_at=now() WHERE account_id=$1 AND id=$2").bind(a).bind(id).bind(&v).execute(&st.db).await.map_err(db_error)?;
    Ok(Json(policy(&st, a, id).await?))
}
async fn destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    admin(&st, &user, a).await?;
    let mut tx = st.db.begin().await?;
    let r=sqlx::query("DELETE FROM inbox_assignment_policies WHERE assignment_policy_id IN (SELECT id FROM assignment_policies WHERE account_id=$1 AND id=$2)").bind(a).bind(id).execute(&mut *tx).await?;
    let _ = r;
    let r = sqlx::query("DELETE FROM assignment_policies WHERE account_id=$1 AND id=$2")
        .bind(a)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    tx.commit().await?;
    Ok(Json(json!({})))
}
async fn inbox_exists(st: &AppState, a: i64, id: i64) -> Result<(), ApiError> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inboxes WHERE account_id=$1 AND id=$2)")
            .bind(a)
            .bind(id)
            .fetch_one(&st.db)
            .await?;
    if exists {
        Ok(())
    } else {
        Err(ApiError::not_found())
    }
}
async fn policy_inboxes(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    admin(&st, &user, a).await?;
    policy(&st, a, id).await?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',i.id,'name',i.name,'channel_id',i.channel_id,'channel_type',i.channel_type,'greeting_enabled',i.greeting_enabled,'greeting_message',i.greeting_message,'working_hours_enabled',i.working_hours_enabled,'enable_email_collect',i.enable_email_collect,'csat_survey_enabled',i.csat_survey_enabled,'csat_config',i.csat_config,'enable_auto_assignment',i.enable_auto_assignment,'auto_assignment_config',i.auto_assignment_config,'out_of_office_message',i.out_of_office_message,'timezone',i.timezone,'callback_webhook_url',i.callback_webhook_url,'allow_messages_after_resolved',i.allow_messages_after_resolved,'lock_to_single_conversation',i.lock_to_single_conversation,'sender_name_type',i.sender_name_type,'business_name',i.business_name) FROM inboxes i JOIN inbox_assignment_policies ip ON ip.inbox_id=i.id WHERE i.account_id=$1 AND ip.assignment_policy_id=$2 ORDER BY i.id").bind(a).bind(id).fetch_all(&st.db).await?;
    Ok(Json(json!({"inboxes":rows})))
}
async fn inbox_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    admin(&st, &user, a).await?;
    inbox_exists(&st, a, id).await?;
    let pid: i64 = sqlx::query_scalar(
        "SELECT assignment_policy_id FROM inbox_assignment_policies WHERE inbox_id=$1",
    )
    .bind(id)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(policy(&st, a, pid).await?))
}
async fn inbox_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    admin(&st, &user, a).await?;
    inbox_exists(&st, a, id).await?;
    let pid = body["assignment_policy_id"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("assignment_policy_id is required"))?;
    policy(&st, a, pid).await?;
    let mut tx = st.db.begin().await?;
    sqlx::query("SELECT id FROM inboxes WHERE id=$1 FOR UPDATE")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM inbox_assignment_policies WHERE inbox_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO inbox_assignment_policies(inbox_id,assignment_policy_id,created_at,updated_at) VALUES($1,$2,now(),now())").bind(id).bind(pid).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(policy(&st, a, pid).await?))
}
async fn inbox_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((a, id)): Path<(i64, i64)>,
) -> ApiResult {
    admin(&st, &user, a).await?;
    inbox_exists(&st, a, id).await?;
    let r=sqlx::query("DELETE FROM inbox_assignment_policies WHERE inbox_id=$1 AND assignment_policy_id IN (SELECT id FROM assignment_policies WHERE account_id=$2)").bind(id).bind(a).execute(&st.db).await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({})))
}
