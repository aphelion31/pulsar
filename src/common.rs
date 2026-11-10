//! Shared state, errors, auth extractor. Every handler module uses these.
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::cable::Hub;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub hub: Hub,
    pub public_dir: std::path::PathBuf,
    pub frontend_url: String,
}

impl FromRef<AppState> for PgPool {
    fn from_ref(s: &AppState) -> PgPool {
        s.db.clone()
    }
}

/// Uniform error type. `?` on sqlx errors gives 500, `ApiError::not_found()` etc for others.
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);

impl ApiError {
    pub fn not_found() -> Self {
        Self(StatusCode::NOT_FOUND, "Resource could not be found".into())
    }
    pub fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "You need to sign in or sign up before continuing.".into())
    }
    pub fn forbidden() -> Self {
        Self(StatusCode::FORBIDDEN, "You are not authorized to do this action".into())
    }
    pub fn bad(msg: impl Into<String>) -> Self {
        Self(StatusCode::UNPROCESSABLE_ENTITY, msg.into())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Self::not_found(),
            e => {
                tracing::error!("db error: {e}");
                Self(StatusCode::INTERNAL_SERVER_ERROR, "Internal error".into())
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = if self.0 == StatusCode::UNAUTHORIZED {
            json!({ "errors": [self.1] })
        } else {
            json!({ "error": self.1, "message": self.1 })
        };
        (self.0, Json(body)).into_response()
    }
}

pub type ApiResult<T = Json<Value>> = Result<T, ApiError>;

/// Authenticated user. Accepts devise_token_auth headers (access-token, client, uid)
/// or `api_access_token` header (access_tokens table).
#[derive(Clone, Debug)]
pub struct CurrentUser {
    pub id: i64,
    pub email: String,
    pub name: String,
    pub pubsub_token: Option<String>,
    /// client id of the devise token used, for sign_out
    pub client: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for CurrentUser
where
    PgPool: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let db = PgPool::from_ref(state);
        let h = |k: &str| parts.headers.get(k).and_then(|v| v.to_str().ok()).map(str::to_string);

        let api_token = h("api_access_token").or_else(|| {
            h("authorization").and_then(|a| a.strip_prefix("Bearer ").map(str::to_string))
        });
        if let (None, Some(tok)) = (h("access-token"), api_token) {
            let row: Option<(i64, String, String, Option<String>)> = sqlx::query_as(
                "SELECT u.id::bigint, COALESCE(u.email,''), u.name, u.pubsub_token FROM access_tokens t
                 JOIN users u ON u.id = t.owner_id AND t.owner_type = 'User' WHERE t.token = $1",
            )
            .bind(&tok)
            .fetch_optional(&db)
            .await?;
            let (id, email, name, pubsub_token) = row.ok_or_else(ApiError::unauthorized)?;
            return Ok(Self { id, email, name, pubsub_token, client: None });
        }

        let (Some(token), Some(client), Some(uid)) = (h("access-token"), h("client"), h("uid")) else {
            return Err(ApiError::unauthorized());
        };
        let row: Option<(i64, String, String, Option<String>, Option<Value>)> = sqlx::query_as(
            "SELECT id::bigint, COALESCE(email,''), name, pubsub_token, tokens::jsonb FROM users WHERE uid = $1 AND provider = 'email'",
        )
        .bind(&uid)
        .fetch_optional(&db)
        .await?;
        let (id, email, name, pubsub_token, tokens) = row.ok_or_else(ApiError::unauthorized)?;
        if !crate::auth::token_valid(tokens.as_ref(), &client, &token) {
            return Err(ApiError::unauthorized());
        }
        Ok(Self { id, email, name, pubsub_token, client: Some(client) })
    }
}

/// Membership of the current user in the account from the URL. Use via `require_account`.
#[derive(Clone, Debug)]
pub struct AccountUser {
    pub account_id: i64,
    pub user_id: i64,
    pub role: i32, // 0 agent, 1 administrator
}

impl AccountUser {
    pub fn is_admin(&self) -> bool {
        self.role == 1
    }
    pub fn require_admin(&self) -> Result<(), ApiError> {
        if self.is_admin() { Ok(()) } else { Err(ApiError::forbidden()) }
    }
}

/// Verify `user` belongs to `account_id`. Every `/api/v1/accounts/{account_id}/...` handler must call this.
pub async fn require_account(db: &PgPool, user: &CurrentUser, account_id: i64) -> Result<AccountUser, ApiError> {
    let role: Option<(i32,)> =
        sqlx::query_as("SELECT COALESCE(role,0) FROM account_users WHERE account_id = $1 AND user_id = $2")
            .bind(account_id)
            .bind(user.id)
            .fetch_optional(db)
            .await?;
    let (role,) = role.ok_or_else(ApiError::unauthorized)?;
    Ok(AccountUser { account_id, user_id: user.id, role })
}

/// Rails-style `to_i` for optional timestamps (nil -> 0).
pub fn ts(t: Option<chrono::NaiveDateTime>) -> i64 {
    t.map(|t| t.and_utc().timestamp()).unwrap_or(0)
}

pub fn now() -> chrono::NaiveDateTime {
    chrono::Utc::now().naive_utc()
}
