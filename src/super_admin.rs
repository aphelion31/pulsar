//! Installation onboarding and Super Admin management endpoints.
//! Mirrors Chatwoot Installation::OnboardingController and SuperAdmin::* controllers.
use axum::{
    Json, Router,
    extract::{FromRef, FromRequestParts, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row};

use crate::{
    auth,
    common::{ApiError, ApiResult, AppState, sign_oauth_claims, ts, verify_oauth_claims},
};

const SA_COOKIE: &str = "pulsar_super_admin_token";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/installation/onboarding",
            get(onboarding_show).post(onboarding_create),
        )
        .route(
            "/super_admin",
            get(|| async { Redirect::to("/app/super_admin") }),
        )
        .route(
            "/super_admin/sign_in",
            get(|| async { Redirect::to("/app/super_admin/sign_in") }).post(super_admin_sign_in),
        )
        .route("/super_admin/sign_out", delete(super_admin_logout))
        .route("/super_admin/logout", get(super_admin_logout))
        .route("/super_admin/api/dashboard", get(dashboard_index))
        .route(
            "/super_admin/api/accounts",
            get(accounts_index).post(accounts_create),
        )
        .route(
            "/super_admin/api/accounts/{id}",
            get(accounts_show)
                .patch(accounts_update)
                .put(accounts_update)
                .delete(accounts_destroy),
        )
        .route("/super_admin/api/accounts/{id}/seed", post(accounts_seed))
        .route(
            "/super_admin/api/accounts/{id}/reset_cache",
            post(accounts_reset_cache),
        )
        .route(
            "/super_admin/api/users",
            get(users_index).post(users_create),
        )
        .route(
            "/super_admin/api/users/{id}",
            get(users_show)
                .patch(users_update)
                .put(users_update)
                .delete(users_destroy),
        )
        .route(
            "/super_admin/api/users/{id}/impersonate",
            post(users_impersonate),
        )
        .route(
            "/super_admin/api/account_users",
            get(account_users_index).post(account_users_create),
        )
        .route(
            "/super_admin/api/account_users/{id}",
            delete(account_users_destroy),
        )
        .route(
            "/super_admin/api/agent_bots",
            get(agent_bots_index).post(agent_bots_create),
        )
        .route(
            "/super_admin/api/agent_bots/{id}",
            get(agent_bots_show)
                .patch(agent_bots_update)
                .put(agent_bots_update)
                .delete(agent_bots_destroy),
        )
        .route(
            "/super_admin/api/platform_apps",
            get(platform_apps_index).post(platform_apps_create),
        )
        .route(
            "/super_admin/api/platform_apps/{id}",
            get(platform_apps_show)
                .patch(platform_apps_update)
                .put(platform_apps_update)
                .delete(platform_apps_destroy),
        )
        .route(
            "/super_admin/api/platform_banners",
            get(platform_banners_index).post(platform_banners_create),
        )
        .route(
            "/super_admin/api/platform_banners/{id}",
            get(platform_banners_show)
                .patch(platform_banners_update)
                .put(platform_banners_update)
                .delete(platform_banners_destroy),
        )
        .route("/super_admin/api/access_tokens", get(access_tokens_index))
        .route(
            "/super_admin/api/installation_configs",
            get(installation_configs_index).post(installation_configs_create),
        )
        .route(
            "/super_admin/api/installation_configs/{id}",
            get(installation_configs_show)
                .patch(installation_configs_update)
                .put(installation_configs_update),
        )
        .route(
            "/super_admin/api/app_config",
            get(app_config_show).post(app_config_update),
        )
        .route(
            "/super_admin/api/instance_status",
            get(instance_status_show),
        )
        .route("/super_admin/api/settings", get(settings_show))
        .route("/super_admin/api/settings/refresh", get(settings_refresh))
        .route("/super_admin/api/search/accounts", get(search_accounts))
        .route("/super_admin/api/search/users", get(search_users))
        .route(
            "/super_admin/api/user_diagnostics",
            get(user_diagnostics_show).post(user_diagnostics_show),
        )
        .route(
            "/super_admin/api/user_diagnostics/destroy_subscriptions",
            post(user_diagnostics_destroy_subscriptions),
        )
}

// ============================================================================
// SuperAdmin Guard & Auth
// ============================================================================

#[derive(Clone, Debug)]
pub struct SuperAdminUser {
    pub id: i64,
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let mut kv = part.trim().splitn(2, '=');
        if kv.next()?.trim() == name {
            return kv.next().map(|v| v.trim().to_string());
        }
    }
    None
}

impl<S: Send + Sync> FromRequestParts<S> for SuperAdminUser
where
    PgPool: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let db = PgPool::from_ref(state);
        let hdr = |k: &str| {
            parts
                .headers
                .get(k)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };

        // 1. Devise token_auth headers
        if let (Some(token), Some(client), Some(uid)) =
            (hdr("access-token"), hdr("client"), hdr("uid"))
        {
            let row: Option<(i64, Option<Value>, Option<String>)> = sqlx::query_as(
                "SELECT id::bigint, tokens::jsonb, \"type\" FROM users WHERE uid = $1 AND provider = 'email'",
            )
            .bind(&uid)
            .fetch_optional(&db)
            .await?;
            let (id, tokens, user_type) = row.ok_or_else(ApiError::unauthorized)?;
            if !auth::token_valid(tokens.as_ref(), &client, &token) {
                return Err(ApiError::unauthorized());
            }
            if user_type.as_deref() != Some("SuperAdmin") {
                return Err(ApiError::forbidden());
            }
            return Ok(Self { id });
        }

        // 2. SuperAdmin session cookie
        if let Some(tok) = cookie_value(&parts.headers, SA_COOKIE)
            && let Some(claims) = verify_oauth_claims(&tok)
            && let Some(uid) = claims["super_admin_id"].as_i64()
        {
            let user_type: Option<Option<String>> =
                sqlx::query_scalar("SELECT \"type\" FROM users WHERE id = $1")
                    .bind(uid)
                    .fetch_optional(&db)
                    .await?;
            let Some(ut) = user_type else {
                return Err(ApiError::unauthorized());
            };
            if ut.as_deref() != Some("SuperAdmin") {
                return Err(ApiError::forbidden());
            }
            return Ok(Self { id: uid });
        }

        Err(ApiError::unauthorized())
    }
}

fn sa_cookie_header(user_id: i64) -> HeaderValue {
    let tok = sign_oauth_claims(&json!({
        "super_admin_id": user_id,
        "exp": chrono::Utc::now().timestamp() + 7 * 86400,
    }));
    HeaderValue::from_str(&format!(
        "{SA_COOKIE}={tok}; HttpOnly; SameSite=Lax; Path=/"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn clear_sa_cookie_header() -> HeaderValue {
    HeaderValue::from_static("pulsar_super_admin_token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
}

async fn super_admin_sign_in(
    State(db): State<PgPool>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let email = body["email"]
        .as_str()
        .or_else(|| body["super_admin"]["email"].as_str())
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let password = body["password"]
        .as_str()
        .or_else(|| body["super_admin"]["password"].as_str())
        .unwrap_or_default()
        .to_string();
    if email.is_empty() || password.is_empty() {
        return Err(ApiError::unauthorized());
    }

    let row: Option<(i64, String, String, Option<Value>, Option<String>)> = sqlx::query_as(
        "SELECT id::bigint, uid, encrypted_password, tokens::jsonb, \"type\"
         FROM users WHERE lower(email) = $1 AND provider = 'email'",
    )
    .bind(&email)
    .fetch_optional(&db)
    .await?;
    let (id, uid, enc, tokens, user_type) = row.ok_or_else(ApiError::unauthorized)?;

    let rehash_pw = auth::needs_rehash(&enc).then(|| password.clone());
    let ok = auth::verify_password_async(password, enc).await;
    if !ok {
        return Err(ApiError::unauthorized());
    }
    if user_type.as_deref() != Some("SuperAdmin") {
        return Err(ApiError::forbidden());
    }
    if let Some(raw_pw) = rehash_pw
        && let Ok(upgraded) = auth::hash_password_async(raw_pw).await
    {
        let _ = sqlx::query(
            "UPDATE users SET encrypted_password = $1, updated_at = now() WHERE id = $2",
        )
        .bind(upgraded)
        .bind(id)
        .execute(&db)
        .await;
    }

    let (client, token, expiry) = auth::issue_token(&db, id, tokens).await?;
    let mut headers = auth::auth_headers(&uid, &client, &token, expiry);
    headers.insert(header::SET_COOKIE, sa_cookie_header(id));
    let data = auth::user_json(&db, id).await?;
    Ok((headers, Json(json!({ "data": data }))).into_response())
}

async fn super_admin_logout(State(db): State<PgPool>, headers: HeaderMap) -> ApiResult<Response> {
    if let (Some(client), Some(uid)) = (
        headers.get("client").and_then(|v| v.to_str().ok()),
        headers.get("uid").and_then(|v| v.to_str().ok()),
    ) {
        let _ = sqlx::query(
            "UPDATE users SET tokens = (tokens::jsonb - $1)::json WHERE uid = $2 AND provider = 'email'",
        )
        .bind(client)
        .bind(uid)
        .execute(&db)
        .await;
    }
    Ok((
        [(header::SET_COOKIE, clear_sa_cookie_header())],
        Json(json!({ "ok": true })),
    )
        .into_response())
}

// ============================================================================
// Installation Onboarding
// ============================================================================

async fn onboarding_needed(db: &PgPool) -> Result<bool, ApiError> {
    let super_admins: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE \"type\" = 'SuperAdmin'")
            .fetch_one(db)
            .await?;
    if super_admins == 0 {
        return Ok(true);
    }
    let cfg: Option<Value> = sqlx::query_scalar(
        "SELECT serialized_value FROM installation_configs WHERE name = 'INSTALLATION_ONBOARDING' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(db)
    .await?;
    let flag = cfg
        .as_ref()
        .and_then(|v| v.get("value").or(Some(v)))
        .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")));
    Ok(flag == Some(true))
}

async fn onboarding_show(State(db): State<PgPool>, headers: HeaderMap) -> ApiResult<Response> {
    let needed = onboarding_needed(&db).await?;
    let wants_json = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("application/json"));
    if wants_json {
        return Ok(Json(json!({ "onboarding_needed": needed })).into_response());
    }
    if needed {
        Ok(Redirect::to("/app/installation/onboarding").into_response())
    } else {
        Ok(Redirect::to("/app/").into_response())
    }
}

async fn onboarding_create(
    State(db): State<PgPool>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    if !onboarding_needed(&db).await? {
        return Err(ApiError::forbidden());
    }
    let u = body.get("user").filter(|v| v.is_object()).unwrap_or(&body);
    let company = u["company"]
        .as_str()
        .or_else(|| body["account_name"].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Company name is required"))?
        .to_string();
    let name = u["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Name is required"))?
        .to_string();
    let email = u["email"]
        .as_str()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s.contains('@'))
        .ok_or_else(|| ApiError::bad("Valid email is required"))?;
    let password = u["password"]
        .as_str()
        .filter(|s| s.len() >= 6)
        .ok_or_else(|| ApiError::bad("Password must be at least 6 characters"))?
        .to_string();

    let hash = auth::hash_password_async(password).await?;
    let (ff, ff1) = auth::default_feature_flags();

    let mut tx = db.begin().await?;
    let uid: i64 = sqlx::query_scalar(
        "INSERT INTO users (provider, uid, encrypted_password, name, display_name, email, confirmed_at, pubsub_token, \"type\", created_at, updated_at, tokens)
         VALUES ('email', $1, $2, $3, $3, $1, now(), $4, 'SuperAdmin', now(), now(), '{}')
         RETURNING id::bigint",
    )
    .bind(&email)
    .bind(&hash)
    .bind(&name)
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .fetch_one(&mut *tx)
    .await?;

    let aid: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (name, created_at, updated_at, feature_flags, feature_flags_ext_1)
         VALUES ($1, now(), now(), $2, $3) RETURNING id::bigint",
    )
    .bind(&company)
    .bind(ff)
    .bind(ff1)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO account_users (account_id, user_id, role, created_at, updated_at, active_at)
         VALUES ($1, $2, 1, now(), now(), now())",
    )
    .bind(aid)
    .bind(uid)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at)
         VALUES ('User', $1, $2, now(), now())",
    )
    .bind(uid)
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO installation_configs (name, serialized_value, locked, created_at, updated_at)
         VALUES ('INSTALLATION_ONBOARDING', $1, true, now(), now())
         ON CONFLICT (name) DO UPDATE SET serialized_value = EXCLUDED.serialized_value, updated_at = now()",
    )
    .bind(json!({ "value": false }))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let (client, token, expiry) = auth::issue_token(&db, uid, None).await?;
    let mut hdrs = auth::auth_headers(&email, &client, &token, expiry);
    hdrs.insert(header::SET_COOKIE, sa_cookie_header(uid));
    let user_data = auth::user_json(&db, uid).await?;

    Ok((
        hdrs,
        Json(json!({
            "account": { "id": aid, "name": company },
            "user": user_data,
        })),
    )
        .into_response())
}

// ============================================================================
// Dashboard & Instance Status
// ============================================================================

async fn dashboard_index(State(db): State<PgPool>, _sa: SuperAdminUser) -> ApiResult {
    let accounts_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
        .fetch_one(&db)
        .await?;
    let users_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&db)
        .await?;
    let inboxes_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM inboxes")
        .fetch_one(&db)
        .await?;
    let conversations_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations")
        .fetch_one(&db)
        .await?;
    let messages_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
        .fetch_one(&db)
        .await?;

    let chart_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT to_char(date_trunc('day', created_at), 'YYYY-MM-DD') AS day, COUNT(*)
         FROM conversations
         WHERE created_at >= now() - interval '30 days'
         GROUP BY 1 ORDER BY 1 ASC",
    )
    .fetch_all(&db)
    .await?;

    let chart: Vec<Value> = chart_rows.into_iter().map(|(d, c)| json!([d, c])).collect();
    Ok(Json(json!({
        "accountsCount": accounts_count,
        "usersCount": users_count,
        "inboxesCount": inboxes_count,
        "conversationsCount": conversations_count,
        "messagesCount": messages_count,
        "chartData": chart,
    })))
}

async fn instance_status_show(State(db): State<PgPool>, _sa: SuperAdminUser) -> ApiResult {
    let pg_version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&db)
        .await
        .unwrap_or_else(|_| "unknown".into());
    Ok(Json(json!({
        "metrics": {
            "Pulsar version": env!("CARGO_PKG_VERSION"),
            "Chatwoot edition": "Enterprise",
            "Runtime": "Single-binary Rust (Axum + PostgreSQL)",
            "Postgres alive": "true",
            "Postgres version": pg_version,
            "Database Migrations": "completed",
        }
    })))
}

async fn settings_show(State(db): State<PgPool>, sa: SuperAdminUser) -> ApiResult {
    dashboard_index(State(db), sa).await
}

async fn settings_refresh(State(db): State<PgPool>, sa: SuperAdminUser) -> ApiResult {
    instance_status_show(State(db), sa).await
}

// ============================================================================
// Accounts & Users & AccountUsers
// ============================================================================

#[derive(Deserialize, Default)]
struct SearchQ {
    q: Option<String>,
    page: Option<i64>,
}

async fn account_summary_json(db: &PgPool, id: i64) -> Result<Value, ApiError> {
    let mut a = auth::account_json(db, id).await?;
    let row = sqlx::query(
        "SELECT limits, status,
                (SELECT COUNT(*) FROM account_users WHERE account_id = accounts.id) AS users_count,
                (SELECT COUNT(*) FROM inboxes WHERE account_id = accounts.id) AS inboxes_count,
                (SELECT COUNT(*) FROM conversations WHERE account_id = accounts.id) AS conversations_count
         FROM accounts WHERE id = $1::bigint",
    )
    .bind(id)
    .fetch_one(db)
    .await?;
    if let Some(obj) = a.as_object_mut() {
        obj.insert(
            "limits".into(),
            row.try_get::<Option<Value>, _>("limits")?
                .unwrap_or_else(|| json!({})),
        );
        obj.insert(
            "users_count".into(),
            json!(row.try_get::<i64, _>("users_count")?),
        );
        obj.insert(
            "inboxes_count".into(),
            json!(row.try_get::<i64, _>("inboxes_count")?),
        );
        obj.insert(
            "conversations_count".into(),
            json!(row.try_get::<i64, _>("conversations_count")?),
        );
    }
    Ok(a)
}

async fn accounts_index(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Query(q): Query<SearchQ>,
) -> ApiResult {
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * 50;
    let term =
        q.q.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("%{s}%"));
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM accounts WHERE ($1::text IS NULL OR name ILIKE $1 OR domain ILIKE $1 OR support_email ILIKE $1)",
    )
    .bind(&term)
    .fetch_one(&db)
    .await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM accounts
         WHERE ($1::text IS NULL OR name ILIKE $1 OR domain ILIKE $1 OR support_email ILIKE $1)
         ORDER BY id DESC LIMIT 50 OFFSET $2",
    )
    .bind(&term)
    .bind(offset)
    .fetch_all(&db)
    .await?;
    let mut payload = Vec::with_capacity(ids.len());
    for id in ids {
        payload.push(account_summary_json(&db, id).await?);
    }
    Ok(Json(
        json!({ "payload": payload, "meta": { "total_count": total, "page": page } }),
    ))
}

async fn accounts_show(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> ApiResult {
    Ok(Json(account_summary_json(&db, id).await?))
}

async fn accounts_create(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["account"].is_object() {
        &body["account"]
    } else {
        &body
    };
    let name = b["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Account name is required"))?;
    let (mut ff, mut ff1) = auth::default_feature_flags();
    if let Some(feats) = b.get("features").and_then(Value::as_object) {
        for (k, v) in feats {
            if let Some(en) = v.as_bool() {
                auth::set_feature(&mut ff, &mut ff1, k, en);
            }
        }
    }
    let limits = b
        .get("limits")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let status = if b["status"].as_str() == Some("suspended") {
        1i32
    } else {
        0i32
    };

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (name, domain, support_email, limits, status, feature_flags, feature_flags_ext_1, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now()) RETURNING id::bigint",
    )
    .bind(name)
    .bind(b["domain"].as_str())
    .bind(b["support_email"].as_str())
    .bind(limits)
    .bind(status)
    .bind(ff)
    .bind(ff1)
    .fetch_one(&db)
    .await?;

    Ok(Json(account_summary_json(&db, id).await?))
}

async fn accounts_update(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["account"].is_object() {
        &body["account"]
    } else {
        &body
    };
    let (mut ff, mut ff1, cur_limits, cur_status): (i64, i64, Value, Option<i32>) = sqlx::query_as(
        "SELECT feature_flags, feature_flags_ext_1, COALESCE(limits, '{}'::jsonb), status FROM accounts WHERE id = $1::bigint",
    )
    .bind(id)
    .fetch_optional(&db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    if let Some(feats) = b.get("features").and_then(Value::as_object) {
        for (k, v) in feats {
            if let Some(en) = v.as_bool() {
                auth::set_feature(&mut ff, &mut ff1, k, en);
            }
        }
    }
    let new_limits = b
        .get("limits")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or(cur_limits);
    let new_status = match b["status"].as_str() {
        Some("suspended") => Some(1i32),
        Some("active") => Some(0i32),
        _ => cur_status,
    };

    sqlx::query(
        "UPDATE accounts
         SET name = COALESCE($1, name), domain = COALESCE($2, domain), support_email = COALESCE($3, support_email),
             limits = $4, status = $5, feature_flags = $6, feature_flags_ext_1 = $7, updated_at = now()
         WHERE id = $8::bigint",
    )
    .bind(b["name"].as_str().map(str::trim).filter(|s| !s.is_empty()))
    .bind(b["domain"].as_str())
    .bind(b["support_email"].as_str())
    .bind(new_limits)
    .bind(new_status)
    .bind(ff)
    .bind(ff1)
    .bind(id)
    .execute(&db)
    .await?;

    Ok(Json(account_summary_json(&db, id).await?))
}

async fn accounts_destroy(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let n = sqlx::query("DELETE FROM accounts WHERE id = $1::bigint")
        .bind(id)
        .execute(&db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn accounts_seed(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> ApiResult {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id::bigint FROM accounts WHERE id = $1::bigint")
            .bind(id)
            .fetch_optional(&db)
            .await?;
    if exists.is_none() {
        return Err(ApiError::not_found());
    }
    let mut tx = db.begin().await?;
    let ch_id: i64 = sqlx::query_scalar(
        "INSERT INTO channel_web_widgets (account_id, website_url, website_token, hmac_token, created_at, updated_at)
         VALUES ($1, 'https://example.com', $2, $3, now(), now()) RETURNING id::bigint",
    )
    .bind(id)
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .fetch_one(&mut *tx)
    .await?;
    let inbox_id: i64 = sqlx::query_scalar(
        "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
         VALUES ($1, $2, 'Sample Website', 'Channel::WebWidget', now(), now()) RETURNING id::bigint",
    )
    .bind(ch_id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let contact_id: i64 = sqlx::query_scalar(
        "INSERT INTO contacts (account_id, name, email, created_at, updated_at)
         VALUES ($1, 'Sample Customer', $2, now(), now()) RETURNING id::bigint",
    )
    .bind(id)
    .bind(format!(
        "sample+{}@example.com",
        uuid::Uuid::new_v4().simple()
    ))
    .fetch_one(&mut *tx)
    .await?;
    let ci_id: i64 = sqlx::query_scalar(
        "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at)
         VALUES ($1, $2, $3, now(), now()) RETURNING id",
    )
    .bind(contact_id)
    .bind(inbox_id)
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .fetch_one(&mut *tx)
    .await?;
    let conv_id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (account_id, inbox_id, contact_id, contact_inbox_id, status, display_id, uuid, last_activity_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 0, 1, gen_random_uuid(), now(), now(), now()) RETURNING id::bigint",
    )
    .bind(id)
    .bind(inbox_id)
    .bind(contact_id)
    .bind(ci_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO messages (account_id, inbox_id, conversation_id, message_type, content, content_type, sender_type, sender_id, created_at, updated_at)
         VALUES ($1, $2, $3, 0, 'Hello! Exploring the seeded workspace.', 0, 'Contact', $4, now(), now())",
    )
    .bind(id)
    .bind(inbox_id)
    .bind(conv_id)
    .bind(contact_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE conversations SET last_message_at = now() WHERE id = $1")
        .bind(conv_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(
        json!({ "ok": true, "account_id": id, "conversation_id": conv_id }),
    ))
}

async fn accounts_reset_cache(
    State(st): State<AppState>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> ApiResult {
    st.hub.invalidate_cache(id, "inbox");
    st.hub.invalidate_cache(id, "label");
    st.hub.invalidate_cache(id, "team");
    Ok(Json(json!({ "ok": true, "account_id": id })))
}

async fn user_summary_json(db: &PgPool, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT id::bigint AS id, name, display_name, email, uid, \"type\", confirmed_at, created_at, sign_in_count
         FROM users WHERE id = $1::bigint",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let confirmed: Option<chrono::NaiveDateTime> = r.try_get("confirmed_at")?;
    let created: chrono::NaiveDateTime = r.try_get("created_at")?;
    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "display_name": r.try_get::<Option<String>, _>("display_name")?,
        "email": r.try_get::<Option<String>, _>("email")?,
        "uid": r.try_get::<String, _>("uid")?,
        "type": r.try_get::<Option<String>, _>("type")?,
        "confirmed": confirmed.is_some(),
        "sign_in_count": r.try_get::<i32, _>("sign_in_count")?,
        "created_at": ts(Some(created)),
    }))
}

async fn users_index(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Query(q): Query<SearchQ>,
) -> ApiResult {
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * 50;
    let term =
        q.q.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("%{s}%"));
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE ($1::text IS NULL OR name ILIKE $1 OR email ILIKE $1)",
    )
    .bind(&term)
    .fetch_one(&db)
    .await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM users WHERE ($1::text IS NULL OR name ILIKE $1 OR email ILIKE $1) ORDER BY id DESC LIMIT 50 OFFSET $2",
    )
    .bind(&term)
    .bind(offset)
    .fetch_all(&db)
    .await?;
    let mut payload = Vec::with_capacity(ids.len());
    for id in ids {
        payload.push(user_summary_json(&db, id).await?);
    }
    Ok(Json(
        json!({ "payload": payload, "meta": { "total_count": total, "page": page } }),
    ))
}

async fn users_show(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> ApiResult {
    Ok(Json(user_summary_json(&db, id).await?))
}

async fn users_create(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["user"].is_object() {
        &body["user"]
    } else {
        &body
    };
    let name = b["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Name is required"))?
        .to_string();
    let email = b["email"]
        .as_str()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s.contains('@'))
        .ok_or_else(|| ApiError::bad("Valid email is required"))?;
    let pw = b["password"]
        .as_str()
        .filter(|s| s.len() >= 6)
        .unwrap_or("Password123!")
        .to_string();
    let user_type = b["type"]
        .as_str()
        .filter(|s| *s == "SuperAdmin")
        .map(str::to_string);
    let hash = auth::hash_password_async(pw).await?;

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (provider, uid, encrypted_password, name, display_name, email, confirmed_at, pubsub_token, \"type\", created_at, updated_at, tokens)
         VALUES ('email', $1, $2, $3, $4, $1, now(), $5, $6, now(), now(), '{}')
         RETURNING id::bigint",
    )
    .bind(&email)
    .bind(&hash)
    .bind(&name)
    .bind(b["display_name"].as_str())
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .bind(user_type)
    .fetch_one(&db)
    .await?;

    Ok(Json(user_summary_json(&db, id).await?))
}

async fn users_update(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["user"].is_object() {
        &body["user"]
    } else {
        &body
    };
    let cur = user_summary_json(&db, id).await?;
    let name = b["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cur["name"].as_str().unwrap_or(""));
    let user_type: Option<String> = if b.get("type").is_some() {
        b["type"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    } else {
        cur["type"].as_str().map(str::to_string)
    };
    sqlx::query(
        "UPDATE users SET name = $1, display_name = COALESCE($2, display_name), \"type\" = $3, updated_at = now()
         WHERE id = $4::bigint",
    )
    .bind(name)
    .bind(b["display_name"].as_str())
    .bind(user_type)
    .bind(id)
    .execute(&db)
    .await?;

    if let Some(pw) = b["password"].as_str().filter(|s| s.len() >= 6) {
        let hash = auth::hash_password_async(pw.to_string()).await?;
        sqlx::query("UPDATE users SET encrypted_password = $1 WHERE id = $2::bigint")
            .bind(hash)
            .bind(id)
            .execute(&db)
            .await?;
    }

    Ok(Json(user_summary_json(&db, id).await?))
}

async fn users_destroy(
    State(db): State<PgPool>,
    sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if id == sa.id {
        return Err(ApiError::bad("Cannot delete yourself"));
    }
    let n = sqlx::query("DELETE FROM users WHERE id = $1::bigint")
        .bind(id)
        .execute(&db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn users_impersonate(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let row: Option<(String, Option<Value>)> =
        sqlx::query_as("SELECT uid, tokens::jsonb FROM users WHERE id = $1::bigint")
            .bind(id)
            .fetch_optional(&db)
            .await?;
    let (uid, tokens) = row.ok_or_else(ApiError::not_found)?;
    let (client, token, expiry) = auth::issue_token(&db, id, tokens).await?;
    let user = auth::user_json(&db, id).await?;
    let hdrs = auth::auth_headers(&uid, &client, &token, expiry);
    Ok((
        hdrs,
        Json(json!({
            "user": user,
            "session": {
                "access-token": token,
                "client": client,
                "uid": uid,
                "expiry": expiry.to_string(),
            }
        })),
    )
        .into_response())
}

async fn account_users_index(State(db): State<PgPool>, _sa: SuperAdminUser) -> ApiResult {
    let rows = sqlx::query(
        "SELECT au.id, au.account_id, au.user_id, au.role, a.name AS account_name, u.name AS user_name, u.email AS user_email
         FROM account_users au
         JOIN accounts a ON a.id = au.account_id
         JOIN users u ON u.id = au.user_id
         ORDER BY au.id DESC LIMIT 200",
    )
    .fetch_all(&db)
    .await?;
    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("id").ok()?,
                "account_id": r.try_get::<Option<i64>, _>("account_id").ok()?,
                "account_name": r.try_get::<String, _>("account_name").ok()?,
                "user_id": r.try_get::<Option<i64>, _>("user_id").ok()?,
                "user_name": r.try_get::<String, _>("user_name").ok()?,
                "user_email": r.try_get::<Option<String>, _>("user_email").ok()?,
                "role": if r.try_get::<Option<i32>, _>("role").ok().flatten() == Some(1) { "administrator" } else { "agent" },
            }))
        })
        .collect();
    Ok(Json(json!({ "payload": payload })))
}

async fn account_users_create(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["account_user"].is_object() {
        &body["account_user"]
    } else {
        &body
    };
    let account_id = b["account_id"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("account_id is required"))?;
    let user_id = b["user_id"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("user_id is required"))?;
    let role = if b["role"].as_str() == Some("administrator") || b["role"].as_i64() == Some(1) {
        1i32
    } else {
        0i32
    };
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO account_users (account_id, user_id, role, created_at, updated_at, active_at)
         VALUES ($1, $2, $3, now(), now(), now())
         ON CONFLICT (account_id, user_id) DO UPDATE SET role = EXCLUDED.role, updated_at = now()
         RETURNING id",
    )
    .bind(account_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(&db)
    .await?;
    Ok(Json(
        json!({ "id": id, "account_id": account_id, "user_id": user_id, "role": if role == 1 { "administrator" } else { "agent" } }),
    ))
}

async fn account_users_destroy(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let n = sqlx::query("DELETE FROM account_users WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn search_accounts(st: State<PgPool>, sa: SuperAdminUser, q: Query<SearchQ>) -> ApiResult {
    accounts_index(st, sa, q).await
}

async fn search_users(st: State<PgPool>, sa: SuperAdminUser, q: Query<SearchQ>) -> ApiResult {
    users_index(st, sa, q).await
}

// ============================================================================
// Agent Bots, Platform Apps, Platform Banners, Access Tokens
// ============================================================================

async fn agent_bots_index(State(db): State<PgPool>, _sa: SuperAdminUser) -> ApiResult {
    let rows = sqlx::query(
        "SELECT id, name, description, outgoing_url, account_id, bot_type FROM agent_bots ORDER BY id DESC LIMIT 100",
    )
    .fetch_all(&db)
    .await?;
    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("id").ok()?,
                "name": r.try_get::<Option<String>, _>("name").ok()?,
                "description": r.try_get::<Option<String>, _>("description").ok()?,
                "outgoing_url": r.try_get::<Option<String>, _>("outgoing_url").ok()?,
                "account_id": r.try_get::<Option<i64>, _>("account_id").ok()?,
                "bot_type": r.try_get::<Option<i32>, _>("bot_type").ok()?,
            }))
        })
        .collect();
    Ok(Json(json!({ "payload": payload })))
}

async fn agent_bots_show(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> ApiResult {
    let r = sqlx::query(
        "SELECT id, name, description, outgoing_url, account_id, bot_type FROM agent_bots WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Json(json!({
        "id": r.try_get::<i64, _>("id")?,
        "name": r.try_get::<Option<String>, _>("name")?,
        "description": r.try_get::<Option<String>, _>("description")?,
        "outgoing_url": r.try_get::<Option<String>, _>("outgoing_url")?,
        "account_id": r.try_get::<Option<i64>, _>("account_id")?,
        "bot_type": r.try_get::<Option<i32>, _>("bot_type")?,
    })))
}

async fn agent_bots_create(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["agent_bot"].is_object() {
        &body["agent_bot"]
    } else {
        &body
    };
    let name = b["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Name is required"))?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO agent_bots (name, description, outgoing_url, account_id, bot_type, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 0, now(), now()) RETURNING id",
    )
    .bind(name)
    .bind(b["description"].as_str())
    .bind(b["outgoing_url"].as_str())
    .bind(b["account_id"].as_i64())
    .fetch_one(&db)
    .await?;
    agent_bots_show(State(db), _sa, Path(id)).await
}

async fn agent_bots_update(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["agent_bot"].is_object() {
        &body["agent_bot"]
    } else {
        &body
    };
    let n = sqlx::query(
        "UPDATE agent_bots SET name = COALESCE($1, name), description = COALESCE($2, description), outgoing_url = COALESCE($3, outgoing_url), updated_at = now()
         WHERE id = $4",
    )
    .bind(b["name"].as_str())
    .bind(b["description"].as_str())
    .bind(b["outgoing_url"].as_str())
    .bind(id)
    .execute(&db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    agent_bots_show(State(db), _sa, Path(id)).await
}

async fn agent_bots_destroy(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let n = sqlx::query("DELETE FROM agent_bots WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn platform_app_json(db: &PgPool, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT p.id, p.name, t.token, EXTRACT(EPOCH FROM p.created_at)::bigint AS created_at
         FROM platform_apps p
         LEFT JOIN access_tokens t ON t.owner_type = 'PlatformApp' AND t.owner_id = p.id
         WHERE p.id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "access_token": r.try_get::<Option<String>, _>("token")?,
        "created_at": r.try_get::<i64, _>("created_at")?,
    }))
}

async fn platform_apps_index(State(db): State<PgPool>, _sa: SuperAdminUser) -> ApiResult {
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM platform_apps ORDER BY id DESC LIMIT 100")
            .fetch_all(&db)
            .await?;
    let mut payload = Vec::with_capacity(ids.len());
    for id in ids {
        payload.push(platform_app_json(&db, id).await?);
    }
    Ok(Json(json!({ "payload": payload })))
}

async fn platform_apps_show(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> ApiResult {
    Ok(Json(platform_app_json(&db, id).await?))
}

async fn platform_apps_create(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["platform_app"].is_object() {
        &body["platform_app"]
    } else {
        &body
    };
    let name = b["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Name is required"))?;
    let mut tx = db.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO platform_apps (name, created_at, updated_at) VALUES ($1, now(), now()) RETURNING id",
    )
    .bind(name)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at)
         VALUES ('PlatformApp', $1, $2, now(), now())",
    )
    .bind(id)
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(platform_app_json(&db, id).await?))
}

async fn platform_apps_update(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["platform_app"].is_object() {
        &body["platform_app"]
    } else {
        &body
    };
    let n = sqlx::query(
        "UPDATE platform_apps SET name = COALESCE($1, name), updated_at = now() WHERE id = $2",
    )
    .bind(b["name"].as_str())
    .bind(id)
    .execute(&db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(platform_app_json(&db, id).await?))
}

async fn platform_apps_destroy(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let n = sqlx::query("DELETE FROM platform_apps WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn platform_banners_index(State(db): State<PgPool>, _sa: SuperAdminUser) -> ApiResult {
    let rows = sqlx::query(
        "SELECT id, title, banner_message, banner_type, active, video_url FROM platform_banners ORDER BY id DESC",
    )
    .fetch_all(&db)
    .await?;
    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("id").ok()?,
                "title": r.try_get::<Option<String>, _>("title").ok()?,
                "banner_message": r.try_get::<String, _>("banner_message").ok()?,
                "banner_type": r.try_get::<i32, _>("banner_type").ok()?,
                "active": r.try_get::<bool, _>("active").ok()?,
                "video_url": r.try_get::<Option<String>, _>("video_url").ok()?,
            }))
        })
        .collect();
    Ok(Json(json!({ "payload": payload })))
}

async fn platform_banners_show(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> ApiResult {
    let r = sqlx::query(
        "SELECT id, title, banner_message, banner_type, active, video_url FROM platform_banners WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Json(json!({
        "id": r.try_get::<i64, _>("id")?,
        "title": r.try_get::<Option<String>, _>("title")?,
        "banner_message": r.try_get::<String, _>("banner_message")?,
        "banner_type": r.try_get::<i32, _>("banner_type")?,
        "active": r.try_get::<bool, _>("active")?,
        "video_url": r.try_get::<Option<String>, _>("video_url")?,
    })))
}

async fn platform_banners_create(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["platform_banner"].is_object() {
        &body["platform_banner"]
    } else {
        &body
    };
    let msg = b["banner_message"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("banner_message is required"))?;
    let active = b["active"].as_bool().unwrap_or(true);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO platform_banners (title, banner_message, banner_type, active, video_url, created_at, updated_at)
         VALUES ($1, $2, 0, $3, $4, now(), now()) RETURNING id",
    )
    .bind(b["title"].as_str())
    .bind(msg)
    .bind(active)
    .bind(b["video_url"].as_str())
    .fetch_one(&db)
    .await?;
    platform_banners_show(State(db), _sa, Path(id)).await
}

async fn platform_banners_update(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["platform_banner"].is_object() {
        &body["platform_banner"]
    } else {
        &body
    };
    let n = sqlx::query(
        "UPDATE platform_banners
         SET title = COALESCE($1, title), banner_message = COALESCE($2, banner_message),
             active = COALESCE($3, active), video_url = COALESCE($4, video_url), updated_at = now()
         WHERE id = $5",
    )
    .bind(b["title"].as_str())
    .bind(b["banner_message"].as_str())
    .bind(b["active"].as_bool())
    .bind(b["video_url"].as_str())
    .bind(id)
    .execute(&db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    platform_banners_show(State(db), _sa, Path(id)).await
}

async fn platform_banners_destroy(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let n = sqlx::query("DELETE FROM platform_banners WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn access_tokens_index(State(db): State<PgPool>, _sa: SuperAdminUser) -> ApiResult {
    let rows = sqlx::query(
        "SELECT id, owner_type, owner_id, token, EXTRACT(EPOCH FROM created_at)::bigint AS created_at
         FROM access_tokens ORDER BY id DESC LIMIT 200",
    )
    .fetch_all(&db)
    .await?;
    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("id").ok()?,
                "owner_type": r.try_get::<Option<String>, _>("owner_type").ok()?,
                "owner_id": r.try_get::<Option<i64>, _>("owner_id").ok()?,
                "token": r.try_get::<Option<String>, _>("token").ok()?,
                "created_at": r.try_get::<i64, _>("created_at").ok()?,
            }))
        })
        .collect();
    Ok(Json(json!({ "payload": payload })))
}

// ============================================================================
// Installation Configs & App Config
// ============================================================================

fn unwrap_serialized_value(v: &Value) -> Value {
    v.get("value").cloned().unwrap_or_else(|| v.clone())
}

async fn installation_configs_index(State(db): State<PgPool>, _sa: SuperAdminUser) -> ApiResult {
    let rows = sqlx::query(
        "SELECT id, name, serialized_value, locked, EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at
         FROM installation_configs ORDER BY name ASC",
    )
    .fetch_all(&db)
    .await?;
    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            let sv: Value = r.try_get("serialized_value").ok()?;
            Some(json!({
                "id": r.try_get::<i64, _>("id").ok()?,
                "name": r.try_get::<String, _>("name").ok()?,
                "value": unwrap_serialized_value(&sv),
                "locked": r.try_get::<bool, _>("locked").ok()?,
                "updated_at": r.try_get::<i64, _>("updated_at").ok()?,
            }))
        })
        .collect();
    Ok(Json(json!({ "payload": payload })))
}

async fn installation_configs_show(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
) -> ApiResult {
    let r = sqlx::query(
        "SELECT id, name, serialized_value, locked FROM installation_configs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let sv: Value = r.try_get("serialized_value")?;
    Ok(Json(json!({
        "id": r.try_get::<i64, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "value": unwrap_serialized_value(&sv),
        "locked": r.try_get::<bool, _>("locked")?,
    })))
}

async fn installation_configs_create(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["installation_config"].is_object() {
        &body["installation_config"]
    } else {
        &body
    };
    let name = b["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("name is required"))?;
    let val = b.get("value").cloned().unwrap_or(Value::Null);
    let locked = b["locked"].as_bool().unwrap_or(false);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO installation_configs (name, serialized_value, locked, created_at, updated_at)
         VALUES ($1, $2, $3, now(), now())
         ON CONFLICT (name) DO UPDATE SET serialized_value = EXCLUDED.serialized_value, locked = EXCLUDED.locked, updated_at = now()
         RETURNING id",
    )
    .bind(name)
    .bind(json!({ "value": val }))
    .bind(locked)
    .fetch_one(&db)
    .await?;
    installation_configs_show(State(db), _sa, Path(id)).await
}

async fn installation_configs_update(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let b = if body["installation_config"].is_object() {
        &body["installation_config"]
    } else {
        &body
    };
    let val = b.get("value").cloned().unwrap_or(Value::Null);
    let n = sqlx::query(
        "UPDATE installation_configs SET serialized_value = $1, locked = false, updated_at = now() WHERE id = $2",
    )
    .bind(json!({ "value": val }))
    .bind(id)
    .execute(&db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    installation_configs_show(State(db), _sa, Path(id)).await
}

fn allowed_app_config_keys(category: &str) -> &'static [&'static str] {
    match category {
        "facebook" => &[
            "FB_APP_ID",
            "FB_VERIFY_TOKEN",
            "FB_APP_SECRET",
            "IG_VERIFY_TOKEN",
            "FACEBOOK_API_VERSION",
            "ENABLE_MESSENGER_CHANNEL_HUMAN_AGENT",
        ],
        "shopify" => &[
            "ENABLE_SHOPIFY_INTEGRATION",
            "SHOPIFY_CLIENT_ID",
            "SHOPIFY_CLIENT_SECRET",
            "SHOPIFY_APP_STORE_URL",
        ],
        "microsoft" => &["AZURE_APP_ID", "AZURE_APP_SECRET"],
        "email" => &["MAILER_INBOUND_EMAIL_DOMAIN", "ACCOUNT_EMAILS_LIMIT"],
        "linear" => &["LINEAR_CLIENT_ID", "LINEAR_CLIENT_SECRET"],
        "stripe" => &[
            "STRIPE_APP_AUTHORIZE_URL",
            "STRIPE_APP_SECRET_KEY",
            "STRIPE_APP_WEBHOOK_SECRET",
        ],
        "slack" => &[
            "SLACK_CLIENT_ID",
            "SLACK_CLIENT_SECRET",
            "SLACK_SIGNING_SECRET",
        ],
        "instagram" => &[
            "INSTAGRAM_APP_ID",
            "INSTAGRAM_APP_SECRET",
            "INSTAGRAM_VERIFY_TOKEN",
            "INSTAGRAM_API_VERSION",
        ],
        "tiktok" => &["TIKTOK_APP_ID", "TIKTOK_APP_SECRET", "TIKTOK_API_VERSION"],
        "whatsapp_embedded" => &[
            "WHATSAPP_APP_ID",
            "WHATSAPP_APP_SECRET",
            "WHATSAPP_CONFIGURATION_ID",
            "WHATSAPP_API_VERSION",
        ],
        "notion" => &["NOTION_CLIENT_ID", "NOTION_CLIENT_SECRET"],
        "google" => &[
            "GOOGLE_OAUTH_CLIENT_ID",
            "GOOGLE_OAUTH_CLIENT_SECRET",
            "GOOGLE_OAUTH_REDIRECT_URI",
            "ENABLE_GOOGLE_OAUTH_LOGIN",
        ],
        "captain" => &[
            "CAPTAIN_OPEN_AI_API_KEY",
            "CAPTAIN_OPEN_AI_MODEL",
            "CAPTAIN_OPEN_AI_ENDPOINT",
            "CAPTAIN_TOOLS_MANIFEST_ENABLED",
        ],
        _ => &[
            "ENABLE_ACCOUNT_SIGNUP",
            "FIREBASE_PROJECT_ID",
            "WEBHOOK_TIMEOUT",
            "MAXIMUM_FILE_UPLOAD_SIZE",
            "WIDGET_TOKEN_EXPIRY",
        ],
    }
}

#[derive(Deserialize, Default)]
struct AppConfigQuery {
    config: Option<String>,
}

async fn app_config_show(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Query(q): Query<AppConfigQuery>,
) -> ApiResult {
    let cat = q.config.as_deref().unwrap_or("general");
    let keys: Vec<String> = allowed_app_config_keys(cat)
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let rows =
        sqlx::query("SELECT name, serialized_value FROM installation_configs WHERE name = ANY($1)")
            .bind(&keys)
            .fetch_all(&db)
            .await?;

    let mut values = Map::new();
    for k in &keys {
        values.insert(k.clone(), Value::Null);
    }
    for r in rows {
        if let (Ok(k), Ok(sv)) = (
            r.try_get::<String, _>("name"),
            r.try_get::<Value, _>("serialized_value"),
        ) {
            values.insert(k, unwrap_serialized_value(&sv));
        }
    }
    Ok(Json(json!({
        "config": cat,
        "keys": keys,
        "values": values,
    })))
}

async fn app_config_update(
    State(db): State<PgPool>,
    sa: SuperAdminUser,
    Query(q): Query<AppConfigQuery>,
    Json(body): Json<Value>,
) -> ApiResult {
    let cat = q.config.as_deref().unwrap_or("general");
    let allowed = allowed_app_config_keys(cat);
    let src = body
        .get("app_config")
        .and_then(Value::as_object)
        .or_else(|| body.get("values").and_then(Value::as_object))
        .or_else(|| body.as_object())
        .ok_or_else(|| ApiError::bad("app_config object required"))?;

    for (k, v) in src {
        if !allowed.contains(&k.as_str()) {
            continue;
        }
        sqlx::query(
            "INSERT INTO installation_configs (name, serialized_value, locked, created_at, updated_at)
             VALUES ($1, $2, false, now(), now())
             ON CONFLICT (name) DO UPDATE SET serialized_value = EXCLUDED.serialized_value, updated_at = now()",
        )
        .bind(k)
        .bind(json!({ "value": v }))
        .execute(&db)
        .await?;
    }

    app_config_show(State(db), sa, Query(q)).await
}

#[derive(Deserialize, Default)]
struct DiagQuery {
    user_query: Option<String>,
    user_id: Option<i64>,
}

async fn user_diagnostics_show(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Query(q): Query<DiagQuery>,
    body: Option<Json<Value>>,
) -> ApiResult {
    let body_val = body.map(|b| b.0).unwrap_or(Value::Null);
    let uid_param = q.user_id.or_else(|| body_val["user_id"].as_i64());
    let text_q = q
        .user_query
        .as_deref()
        .or_else(|| body_val["user_query"].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let resolved_id: Option<i64> = if let Some(id) = uid_param {
        Some(id)
    } else if let Some(tq) = text_q {
        if let Ok(id) = tq.parse::<i64>() {
            Some(id)
        } else {
            sqlx::query_scalar(
                "SELECT id::bigint FROM users WHERE lower(email) = $1 OR name ILIKE $2 LIMIT 1",
            )
            .bind(tq.to_ascii_lowercase())
            .bind(format!("%{tq}%"))
            .fetch_optional(&db)
            .await?
        }
    } else {
        None
    };

    let Some(uid) = resolved_id else {
        return Ok(Json(
            json!({ "user": null, "subscriptions": [], "sessions": [] }),
        ));
    };
    let user = user_summary_json(&db, uid).await?;

    let subs = sqlx::query(
        "SELECT id, subscription_type, identifier, EXTRACT(EPOCH FROM created_at)::bigint AS created_at
         FROM notification_subscriptions WHERE user_id = $1 ORDER BY id DESC",
    )
    .bind(uid)
    .fetch_all(&db)
    .await?
    .into_iter()
    .filter_map(|r| {
        Some(json!({
            "id": r.try_get::<i64, _>("id").ok()?,
            "subscription_type": r.try_get::<i32, _>("subscription_type").ok()?,
            "identifier": r.try_get::<Option<String>, _>("identifier").ok()?,
            "created_at": r.try_get::<i64, _>("created_at").ok()?,
        }))
    })
    .collect::<Vec<_>>();

    let sessions = sqlx::query(
        "SELECT id, client_id, browser_name, EXTRACT(EPOCH FROM last_activity_at)::bigint AS last_activity_at
         FROM user_sessions WHERE user_id = $1 ORDER BY id DESC LIMIT 25",
    )
    .bind(uid)
    .fetch_all(&db)
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|r| {
        Some(json!({
            "id": r.try_get::<i64, _>("id").ok()?,
            "client_id": r.try_get::<Option<String>, _>("client_id").ok()?,
            "browser_name": r.try_get::<Option<String>, _>("browser_name").ok()?,
            "last_activity_at": r.try_get::<Option<i64>, _>("last_activity_at").ok()?,
        }))
    })
    .collect::<Vec<_>>();

    Ok(Json(json!({
        "user": user,
        "subscriptions": subs,
        "sessions": sessions,
    })))
}

async fn user_diagnostics_destroy_subscriptions(
    State(db): State<PgPool>,
    _sa: SuperAdminUser,
    Json(body): Json<Value>,
) -> ApiResult {
    let uid = body["user_id"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("user_id is required"))?;
    let ids: Vec<i64> = body["subscription_ids"]
        .as_array()
        .map(|arr| arr.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    if ids.is_empty() {
        return Err(ApiError::bad("subscription_ids is required"));
    }
    let deleted =
        sqlx::query("DELETE FROM notification_subscriptions WHERE user_id = $1 AND id = ANY($2)")
            .bind(uid)
            .bind(&ids)
            .execute(&db)
            .await?
            .rows_affected();
    Ok(Json(json!({ "deleted_count": deleted })))
}
