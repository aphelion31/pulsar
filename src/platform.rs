//! Platform API. All resource operations require an explicit platform permissible.
use crate::{
    auth,
    common::{ApiError, ApiResult, AppState},
};
use axum::{
    Json, Router,
    extract::{FromRequestParts, Path, State},
    http::{StatusCode, request::Parts},
    routing::get,
};
use serde_json::{Value, json};
use sqlx::PgPool;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", axum::routing::post(user_create))
        .route(
            "/users/{id}",
            get(user_show)
                .patch(user_update)
                .put(user_update)
                .delete(user_delete),
        )
        .route("/users/{id}/login", get(user_login))
        .route("/users/{id}/token", axum::routing::post(user_token))
        .route("/accounts", get(accounts).post(account_create))
        .route(
            "/accounts/{id}",
            get(account_show)
                .patch(account_update)
                .put(account_update)
                .delete(account_delete),
        )
        .route(
            "/accounts/{id}/account_users",
            get(account_users)
                .post(account_user_create)
                .delete(account_user_delete),
        )
        .route("/agent_bots", get(bots).post(bot_create))
        .route(
            "/agent_bots/{id}",
            get(bot_show)
                .patch(bot_update)
                .put(bot_update)
                .delete(bot_delete),
        )
        .route("/agent_bots/{id}/avatar", axum::routing::delete(bot_avatar))
}

struct Platform(i64);
impl FromRequestParts<AppState> for Platform {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("api_access_token")
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty())
            .ok_or_else(ApiError::unauthorized)?;
        let id = sqlx::query_scalar::<_, i64>("SELECT p.id FROM platform_apps p JOIN access_tokens t ON t.owner_id=p.id AND t.owner_type='PlatformApp' WHERE t.token=$1")
            .bind(token).fetch_optional(&state.db).await?.ok_or_else(ApiError::unauthorized)?;
        Ok(Self(id))
    }
}
async fn permit(db: &PgPool, app: i64, kind: &str, id: i64) -> Result<(), ApiError> {
    let ok: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM platform_app_permissibles WHERE platform_app_id=$1 AND permissible_type=$2 AND permissible_id=$3)")
        .bind(app).bind(kind).bind(id).fetch_one(db).await?;
    if ok {
        Ok(())
    } else {
        Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Non permissible resource".into(),
        ))
    }
}
fn token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    v[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::bad(format!("{key} is required")))
}
async fn account_json(db: &PgPool, id: i64) -> Result<Value, ApiError> {
    let mut a = auth::account_json(db, id).await?;
    let limits: Value = sqlx::query_scalar("SELECT limits FROM accounts WHERE id=$1")
        .bind(id)
        .fetch_one(db)
        .await?;
    let o = a.as_object_mut().unwrap();
    for k in [
        "settings",
        "created_at",
        "auto_resolve_duration",
        "cache_keys",
        "latest_chatwoot_version",
    ] {
        o.remove(k);
    }
    o.insert("limits".into(), limits);
    Ok(a)
}
async fn user_json(db: &PgPool, id: i64, app: i64) -> Result<Value, ApiError> {
    let mut u: Value = sqlx::query_scalar("SELECT jsonb_build_object('id',u.id,'name',u.name,'display_name',u.display_name,'email',u.email,'provider',u.provider,'uid',u.uid,'pubsub_token',u.pubsub_token,'confirmed',u.confirmed_at IS NOT NULL,'message_signature',u.message_signature,'ui_settings',u.ui_settings,'custom_attributes',u.custom_attributes,'avatar_url','', 'available_name',COALESCE(NULLIF(u.display_name,''),u.name),'access_token',t.token,'account_id',au.account_id,'role',CASE au.role WHEN 0 THEN 'agent' WHEN 1 THEN 'administrator' END,'accounts',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',a.id,'name',a.name,'active_at',x.active_at,'role',CASE x.role WHEN 0 THEN 'agent' WHEN 1 THEN 'administrator' END)) FROM account_users x JOIN accounts a ON a.id=x.account_id WHERE x.user_id=u.id),'[]'::jsonb)) FROM users u LEFT JOIN access_tokens t ON t.owner_type='User' AND t.owner_id=u.id LEFT JOIN LATERAL (SELECT * FROM account_users WHERE user_id=u.id ORDER BY active_at DESC NULLS LAST LIMIT 1) au ON true WHERE u.id=$1 LIMIT 1").bind(id).fetch_one(db).await?;
    if u["custom_attributes"]
        .as_object()
        .is_some_and(|m| m.is_empty())
    {
        u.as_object_mut().unwrap().remove("custom_attributes");
    }
    let allowed = ids(db, app, "Account").await?;
    if let Some(accounts) = u["accounts"].as_array_mut() {
        accounts.retain(|a| a["id"].as_i64().is_some_and(|id| allowed.contains(&id)));
    }
    if !u["account_id"]
        .as_i64()
        .is_some_and(|id| allowed.contains(&id))
    {
        u["account_id"] = Value::Null;
        u["role"] = Value::Null;
    }
    Ok(u)
}
async fn bot_json(db: &PgPool, id: i64) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',b.id,'name',b.name,'description',b.description,'outgoing_url',b.outgoing_url,'account_id',b.account_id,'access_token',t.token) FROM agent_bots b LEFT JOIN access_tokens t ON t.owner_type='AgentBot' AND t.owner_id=b.id WHERE b.id=$1 LIMIT 1").bind(id).fetch_one(db).await?)
}
async fn ids(db: &PgPool, app: i64, kind: &str) -> Result<Vec<i64>, ApiError> {
    Ok(sqlx::query_scalar("SELECT permissible_id FROM platform_app_permissibles WHERE platform_app_id=$1 AND permissible_type=$2 ORDER BY id").bind(app).bind(kind).fetch_all(db).await?)
}
async fn accounts(State(db): State<PgPool>, Platform(app): Platform) -> ApiResult {
    let mut out = Vec::new();
    for id in ids(&db, app, "Account").await? {
        if let Ok(v) = account_json(&db, id).await {
            out.push(v);
        }
    }
    Ok(Json(json!(out)))
}
async fn account_show(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult {
    permit(&db, app, "Account", id).await?;
    Ok(Json(account_json(&db, id).await?))
}
async fn account_create(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Json(v): Json<Value>,
) -> ApiResult {
    let name = text(&v, "name")?;
    let locale = match v.get("locale") {
        Some(x) => auth::locale_index(x.as_str().unwrap_or(""))
            .ok_or_else(|| ApiError::bad("invalid locale"))?,
        None => 0,
    };
    let (f, g) = auth::default_feature_flags();
    let mut tx = db.begin().await?;
    let id: i64 = sqlx::query_scalar("INSERT INTO accounts(name,locale,feature_flags,feature_flags_ext_1,created_at,updated_at) VALUES($1,$2,$3,$4,now(),now()) RETURNING id::bigint").bind(name).bind(locale).bind(f).bind(g).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO platform_app_permissibles(platform_app_id,permissible_type,permissible_id,created_at,updated_at) VALUES($1,'Account',$2,now(),now())").bind(app).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    update_account(&db, id, &v).await?;
    Ok(Json(account_json(&db, id).await?))
}
async fn update_account(db: &PgPool, id: i64, v: &Value) -> Result<(), ApiError> {
    if v.get("name").is_some() {
        text(v, "name")?;
    }
    let locale = v
        .get("locale")
        .map(|x| {
            auth::locale_index(x.as_str().unwrap_or(""))
                .ok_or_else(|| ApiError::bad("invalid locale"))
        })
        .transpose()?;
    let status = v
        .get("status")
        .map(|x| match x.as_str() {
            Some("active") => Ok(0i32),
            Some("suspended") => Ok(1),
            _ => Err(ApiError::bad("invalid status")),
        })
        .transpose()?;
    for k in ["limits", "custom_attributes", "features"] {
        if v.get(k).is_some_and(|x| !x.is_object()) {
            return Err(ApiError::bad(format!("invalid {k}")));
        }
    }
    let (mut f, mut g): (i64, i64) =
        sqlx::query_as("SELECT feature_flags,feature_flags_ext_1 FROM accounts WHERE id=$1")
            .bind(id)
            .fetch_one(db)
            .await?;
    if let Some(features) = v["features"].as_object() {
        let all: Vec<Value> = serde_json::from_str(include_str!("../features.json")).unwrap();
        let (mut i, mut j) = (0, 0);
        for x in &all {
            let (flags, bit) = if x["column"] == "feature_flags" {
                let n = i;
                i += 1;
                (&mut f, n)
            } else {
                let n = j;
                j += 1;
                (&mut g, n)
            };
            if let Some(value) = features.get(x["name"].as_str().unwrap()) {
                if value == &Value::Null || value == &Value::Bool(false) {
                    *flags &= !(1i64 << bit);
                } else {
                    *flags |= 1i64 << bit;
                }
            }
        }
    }
    sqlx::query("UPDATE accounts SET name=COALESCE($2,name),locale=COALESCE($3,locale),domain=COALESCE($4,domain),support_email=COALESCE($5,support_email),status=COALESCE($6,status),limits=COALESCE($7,limits),custom_attributes=COALESCE($8,custom_attributes),feature_flags=$9,feature_flags_ext_1=$10,updated_at=now() WHERE id=$1")
        .bind(id).bind(v["name"].as_str()).bind(locale).bind(v["domain"].as_str()).bind(v["support_email"].as_str()).bind(status).bind(v.get("limits")).bind(v.get("custom_attributes")).bind(f).bind(g).execute(db).await?;
    Ok(())
}
async fn account_update(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
    Json(v): Json<Value>,
) -> ApiResult {
    permit(&db, app, "Account", id).await?;
    update_account(&db, id, &v).await?;
    Ok(Json(account_json(&db, id).await?))
}
async fn account_delete(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    permit(&db, app, "Account", id).await?;
    // ponytail: refuse deletion of populated accounts until the Rails dependent-destroy graph is implemented.
    let occupied: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM account_users WHERE account_id=$1) OR EXISTS(SELECT 1 FROM inboxes WHERE account_id=$1) OR EXISTS(SELECT 1 FROM contacts WHERE account_id=$1) OR EXISTS(SELECT 1 FROM agent_bots WHERE account_id=$1)").bind(id).fetch_one(&db).await?;
    if occupied {
        return Err(ApiError::bad("Cannot safely delete a populated account"));
    }
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM accounts WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM platform_app_permissibles WHERE permissible_type='Account' AND permissible_id=$1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::OK)
}
async fn user_create(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Json(v): Json<Value>,
) -> ApiResult {
    let email = text(&v, "email")?.trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::bad("invalid email"));
    }
    if let Some(id) =
        sqlx::query_scalar::<_, i64>("SELECT id::bigint FROM users WHERE lower(email)=$1 LIMIT 1")
            .bind(&email)
            .fetch_optional(&db)
            .await?
    {
        permit(&db, app, "User", id).await?;
        return Ok(Json(user_json(&db, id, app).await?));
    }
    let name = text(&v, "name")?;
    let password = v["password"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(token);
    if password.len() < 6 {
        return Err(ApiError::bad("password is too short"));
    }
    let hash = tokio::task::spawn_blocking(move || bcrypt::hash(password, 11))
        .await
        .map_err(|_| ApiError::bad("password hashing failed"))?
        .map_err(|_| ApiError::bad("password hashing failed"))?;
    let mut tx = db.begin().await?;
    let id:i64=sqlx::query_scalar("INSERT INTO users(uid,email,name,display_name,encrypted_password,confirmed_at,pubsub_token,tokens,custom_attributes,created_at,updated_at) VALUES($1,$1,$2,$3,$4,now(),$5,'{}',$6,now(),now()) RETURNING id::bigint").bind(&email).bind(name).bind(v["display_name"].as_str()).bind(hash).bind(token()).bind(v.get("custom_attributes").filter(|x| x.is_object()).cloned().unwrap_or(json!({}))).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO access_tokens(owner_type,owner_id,token,created_at,updated_at) VALUES('User',$1,$2,now(),now())").bind(id).bind(token()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO platform_app_permissibles(platform_app_id,permissible_type,permissible_id,created_at,updated_at) VALUES($1,'User',$2,now(),now())").bind(app).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(user_json(&db, id, app).await?))
}
async fn user_show(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult {
    permit(&db, app, "User", id).await?;
    Ok(Json(user_json(&db, id, app).await?))
}
async fn user_update(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
    Json(v): Json<Value>,
) -> ApiResult {
    permit(&db, app, "User", id).await?;
    if v.get("name").is_some() {
        text(&v, "name")?;
    }
    let email = v["email"].as_str().map(|s| s.trim().to_lowercase());
    if email.as_ref().is_some_and(|s| !s.contains('@')) {
        return Err(ApiError::bad("invalid email"));
    }
    let hash = if let Some(p) = v["password"].as_str() {
        if p.len() < 6 {
            return Err(ApiError::bad("password is too short"));
        }
        let p = p.to_owned();
        Some(
            tokio::task::spawn_blocking(move || bcrypt::hash(p, 11))
                .await
                .map_err(|_| ApiError::bad("password hashing failed"))?
                .map_err(|_| ApiError::bad("password hashing failed"))?,
        )
    } else {
        None
    };
    if v.get("custom_attributes").is_some_and(|x| !x.is_object()) {
        return Err(ApiError::bad("invalid custom_attributes"));
    }
    sqlx::query("UPDATE users SET name=COALESCE($2,name),display_name=COALESCE($3,display_name),email=COALESCE($4,email),uid=COALESCE($4,uid),encrypted_password=COALESCE($5,encrypted_password),custom_attributes=COALESCE(custom_attributes,'{}'::jsonb) || COALESCE($6,'{}'::jsonb),updated_at=now() WHERE id=$1").bind(id).bind(v["name"].as_str()).bind(v["display_name"].as_str()).bind(email).bind(hash).bind(v.get("custom_attributes")).execute(&db).await?;
    Ok(Json(user_json(&db, id, app).await?))
}
async fn user_delete(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    permit(&db, app, "User", id).await?;
    let occupied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM account_users WHERE user_id=$1) OR EXISTS(SELECT 1 FROM messages WHERE sender_type='User' AND sender_id=$1)").bind(id).fetch_one(&db).await?;
    if occupied {
        return Err(ApiError::bad(
            "Cannot safely delete a user with associated records",
        ));
    }
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM access_tokens WHERE owner_type='User' AND owner_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM platform_app_permissibles WHERE permissible_type='User' AND permissible_id=$1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::OK)
}
async fn user_login(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult {
    permit(&db, app, "User", id).await?;
    // ponytail: SSO requires Redis-backed one-time token consumption; fail closed until supported.
    Ok(Json(json!({"url": null})))
}
async fn user_token(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult {
    permit(&db, app, "User", id).await?;
    let u = user_json(&db, id, app).await?;
    Ok(Json(
        json!({"access_token":u["access_token"],"expiry":null,"user":{"id":u["id"],"name":u["name"],"display_name":u["display_name"],"email":u["email"],"pubsub_token":u["pubsub_token"]}}),
    ))
}
async fn account_users(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult {
    permit(&db, app, "Account", id).await?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(au) || jsonb_build_object('role',CASE role WHEN 0 THEN 'agent' WHEN 1 THEN 'administrator' END) FROM account_users au WHERE account_id=$1 AND EXISTS (SELECT 1 FROM platform_app_permissibles p WHERE p.platform_app_id=$2 AND p.permissible_type='User' AND p.permissible_id=au.user_id) ORDER BY id").bind(id).bind(app).fetch_all(&db).await?;
    Ok(Json(json!(rows)))
}
async fn account_user_create(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
    Json(v): Json<Value>,
) -> ApiResult {
    permit(&db, app, "Account", id).await?;
    let uid = v["user_id"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("user_id is required"))?;
    permit(&db, app, "User", uid).await?;
    let role = match v["role"].as_str().unwrap_or("agent") {
        "agent" => 0i32,
        "administrator" => 1,
        _ => return Err(ApiError::bad("invalid role")),
    };
    let result:Value=sqlx::query_scalar("INSERT INTO account_users(account_id,user_id,role,created_at,updated_at) VALUES($1,$2,$3,now(),now()) ON CONFLICT(account_id,user_id) DO UPDATE SET role=EXCLUDED.role,updated_at=now() RETURNING to_jsonb(account_users) || jsonb_build_object('role',CASE role WHEN 0 THEN 'agent' WHEN 1 THEN 'administrator' END)").bind(id).bind(uid).bind(role).fetch_one(&db).await?;
    Ok(Json(result))
}
async fn account_user_delete(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
    Json(v): Json<Value>,
) -> ApiResult<StatusCode> {
    permit(&db, app, "Account", id).await?;
    let uid = v["user_id"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("user_id is required"))?;
    permit(&db, app, "User", uid).await?;
    sqlx::query("DELETE FROM account_users WHERE account_id=$1 AND user_id=$2")
        .bind(id)
        .bind(uid)
        .execute(&db)
        .await?;
    Ok(StatusCode::OK)
}
async fn bots(State(db): State<PgPool>, Platform(app): Platform) -> ApiResult {
    let mut out = Vec::new();
    for id in ids(&db, app, "AgentBot").await? {
        if let Ok(v) = bot_json(&db, id).await {
            out.push(v);
        }
    }
    Ok(Json(json!(out)))
}
async fn bot_show(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult {
    permit(&db, app, "AgentBot", id).await?;
    Ok(Json(bot_json(&db, id).await?))
}
async fn bot_create(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Json(v): Json<Value>,
) -> ApiResult {
    let name = text(&v, "name")?;
    let aid = v["account_id"].as_i64();
    if let Some(a) = aid {
        permit(&db, app, "Account", a).await?;
    }
    if v.get("avatar").is_some() || v.get("avatar_url").is_some() {
        return Err(ApiError::bad("Avatar upload is not supported"));
    }
    let mut tx = db.begin().await?;
    let id:i64=sqlx::query_scalar("INSERT INTO agent_bots(name,description,outgoing_url,account_id,created_at,updated_at) VALUES($1,$2,$3,$4,now(),now()) RETURNING id").bind(name).bind(v["description"].as_str()).bind(v["outgoing_url"].as_str()).bind(aid).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO access_tokens(owner_type,owner_id,token,created_at,updated_at) VALUES('AgentBot',$1,$2,now(),now())").bind(id).bind(token()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO platform_app_permissibles(platform_app_id,permissible_type,permissible_id,created_at,updated_at) VALUES($1,'AgentBot',$2,now(),now())").bind(app).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(bot_json(&db, id).await?))
}
async fn bot_update(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
    Json(v): Json<Value>,
) -> ApiResult {
    permit(&db, app, "AgentBot", id).await?;
    if let Some(a) = v["account_id"].as_i64() {
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT account_id FROM agent_bots WHERE id=$1")
                .bind(id)
                .fetch_one(&db)
                .await?;
        if existing != Some(a) {
            permit(&db, app, "Account", a).await?;
        }
    }
    if v.get("avatar").is_some() || v.get("avatar_url").is_some() {
        return Err(ApiError::bad("Avatar upload is not supported"));
    }
    if v.get("name").is_some() {
        text(&v, "name")?;
    }
    sqlx::query("UPDATE agent_bots SET name=COALESCE($2,name),description=COALESCE($3,description),outgoing_url=COALESCE($4,outgoing_url),account_id=COALESCE($5,account_id),updated_at=now() WHERE id=$1").bind(id).bind(v["name"].as_str()).bind(v["description"].as_str()).bind(v["outgoing_url"].as_str()).bind(v["account_id"].as_i64()).execute(&db).await?;
    Ok(Json(bot_json(&db, id).await?))
}
async fn bot_delete(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    permit(&db, app, "AgentBot", id).await?;
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM agent_bots WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM access_tokens WHERE owner_type='AgentBot' AND owner_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM platform_app_permissibles WHERE permissible_type='AgentBot' AND permissible_id=$1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::OK)
}
async fn bot_avatar(
    State(db): State<PgPool>,
    Platform(app): Platform,
    Path(id): Path<i64>,
) -> ApiResult {
    permit(&db, app, "AgentBot", id).await?;
    // Remove the association only; blob storage may be shared, so do not delete blobs here.
    sqlx::query("DELETE FROM active_storage_attachments WHERE record_type='AgentBot' AND record_id=$1 AND name='avatar'").bind(id).execute(&db).await?;
    Ok(Json(bot_json(&db, id).await?))
}
