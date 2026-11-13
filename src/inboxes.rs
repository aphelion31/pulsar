//! OWNED BY A WORKER. See WORKERS.md for the contract.
//! Inboxes, inbox members, agents, assignable agents, teams. Relative to /api/v1/accounts/{account_id}.
use axum::{
    extract::{Path, RawQuery, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

use crate::{auth, cable, common::*};

// Chatwoot channel_web_widgets.feature_flags bit order (has_flags in channel/web_widget.rb).
const FEATURE_BITS: [(&str, i64); 4] = [
    ("attachments", 1),
    ("emoji_picker", 2),
    ("end_conversation", 4),
    ("use_inbox_avatar_for_bot", 8),
];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/inboxes", get(inbox_index).post(inbox_create))
        .route("/inboxes/{id}", get(inbox_show).patch(inbox_update).put(inbox_update).delete(inbox_destroy))
        .route("/inboxes/{id}/assignable_agents", get(inbox_assignable))
        .route("/assignable_agents", get(assignable_index))
        .route(
            "/inbox_members",
            post(members_replace).patch(members_replace).delete(members_remove),
        )
        .route("/inbox_members/destroy", delete(members_remove))
        .route("/inbox_members/{inbox_id}", get(members_show))
        .route("/agents", get(agents_index).post(agent_create))
        .route("/agents/{id}", patch(agent_update).delete(agent_destroy))
        .route("/teams", get(teams_index).post(team_create))
        .route("/teams/{id}", get(team_show).patch(team_update).put(team_update).delete(team_destroy))
        .route(
            "/teams/{id}/team_members",
            get(team_members_index)
                .post(team_members_add)
                .patch(team_members_replace)
                .delete(team_members_remove),
        )
}

// ---------- helpers ----------

fn token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

async fn hash_pw(pw: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || bcrypt::hash(pw, 10))
        .await
        .map_err(|_| ApiError::bad("hash"))?
        .map_err(|_| ApiError::bad("hash"))
}

/// Accept `{key: {...}}` (Rails nested params) or a flat body.
fn pick<'a>(b: &'a Value, key: &str) -> &'a Value {
    if b[key].is_object() { &b[key] } else { b }
}

fn ostr(b: &Value, k: &str) -> Option<String> {
    b[k].as_str().map(str::to_string)
}

fn obool(b: &Value, k: &str) -> Option<bool> {
    b[k].as_bool()
}

fn reply_time_to_int(v: &Value) -> Option<i64> {
    match v {
        Value::String(s) => match s.as_str() {
            "in_a_few_minutes" => Some(0),
            "in_a_few_hours" => Some(1),
            "in_a_day" => Some(2),
            _ => None,
        },
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

fn reply_time_name(i: i64) -> &'static str {
    match i {
        1 => "in_a_few_hours",
        2 => "in_a_day",
        _ => "in_a_few_minutes",
    }
}

fn flags_to_names(f: i64) -> Vec<&'static str> {
    FEATURE_BITS.iter().filter(|(_, b)| f & b != 0).map(|(n, _)| *n).collect()
}

fn names_to_flags(v: &Value) -> i64 {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter_map(|n| FEATURE_BITS.iter().find(|(k, _)| *k == n).map(|(_, b)| *b))
                .fold(0, |acc, b| acc | b)
        })
        .unwrap_or(0)
}

fn widget_script(base: &str, token: &str) -> String {
    format!("<script src=\"{base}/sdk.js\" data-website-token=\"{token}\" async></script>")
}

/// Rails `ids` array param: `user_ids` in body must be an array of integers.
fn user_ids_of(b: &Value, key: &str) -> Result<Vec<i64>, ApiError> {
    let arr = b[key].as_array().ok_or_else(|| ApiError::bad("Invalid User IDs"))?;
    let ids: Vec<i64> = arr.iter().map(|v| v.as_i64().ok_or_else(|| ApiError::bad("Invalid User IDs"))).collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Every id must be a member of this account.
async fn check_users(db: &PgPool, account_id: i64, ids: &[i64]) -> Result<(), ApiError> {
    let mut uniq = ids.to_vec();
    uniq.sort_unstable();
    uniq.dedup();
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM account_users WHERE account_id::bigint = $1 AND user_id::bigint = ANY($2::bigint[])",
    )
    .bind(account_id)
    .bind(&uniq)
    .fetch_one(db)
    .await?;
    if n as usize != uniq.len() {
        return Err(ApiError::bad("Invalid User IDs"));
    }
    Ok(())
}

/// Mirrors api/v1/models/_agent.json.jbuilder. `ids = None` lists all account agents.
async fn agents_for(db: &PgPool, account_id: i64, ids: Option<Vec<i64>>) -> Result<Vec<Value>, ApiError> {
    let rows = sqlx::query(
        "SELECT u.id::bigint AS id, u.email, u.name, u.display_name,
                u.confirmed_at IS NOT NULL AS confirmed, COALESCE(au.role, 0)::int AS role, au.auto_offline
         FROM account_users au JOIN users u ON u.id = au.user_id
         WHERE au.account_id::bigint = $1 AND ($2::bigint[] IS NULL OR u.id::bigint = ANY($2::bigint[]))
         ORDER BY u.name",
    )
    .bind(account_id)
    .bind(ids)
    .fetch_all(db)
    .await?;
    let snap = cable::presence_snapshot();
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let id: i64 = r.try_get("id")?;
        let name: String = r.try_get("name")?;
        let display: Option<String> = r.try_get("display_name")?;
        let role: i32 = r.try_get("role")?;
        out.push(json!({
            "id": id,
            "account_id": account_id,
            "availability_status": if snap.is_online(account_id, id) { "online" } else { "offline" },
            "auto_offline": r.try_get::<bool, _>("auto_offline")?,
            "confirmed": r.try_get::<bool, _>("confirmed")?,
            "email": r.try_get::<Option<String>, _>("email")?,
            "provider": "email",
            "available_name": display.unwrap_or_else(|| name.clone()),
            "name": name,
            "role": auth::role_name(role),
            "thumbnail": "",
            "custom_role_id": null,
        }));
    }
    Ok(out)
}

async fn inbox_in_account(db: &PgPool, account_id: i64, inbox_id: i64) -> Result<(), ApiError> {
    let found: Option<i32> = sqlx::query_scalar("SELECT 1 FROM inboxes WHERE id::bigint = $1 AND account_id::bigint = $2")
        .bind(inbox_id)
        .bind(account_id)
        .fetch_optional(db)
        .await?;
    found.map(|_| ()).ok_or_else(ApiError::not_found)
}

/// Admins see every inbox of the account; agents only inboxes they are members of.
async fn can_see_inbox(db: &PgPool, au: &AccountUser, inbox_id: i64) -> Result<(), ApiError> {
    inbox_in_account(db, au.account_id, inbox_id).await?;
    if au.is_admin() {
        return Ok(());
    }
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inbox_members WHERE user_id::bigint = $1 AND inbox_id::bigint = $2)",
    )
    .bind(au.user_id)
    .bind(inbox_id)
    .fetch_one(db)
    .await?;
    if member { Ok(()) } else { Err(ApiError::forbidden()) }
}

async fn member_ids(db: &PgPool, inbox_id: i64) -> Result<Vec<i64>, ApiError> {
    Ok(sqlx::query_scalar("SELECT user_id::bigint FROM inbox_members WHERE inbox_id::bigint = $1")
        .bind(inbox_id)
        .fetch_all(db)
        .await?)
}

/// Mirrors Inbox#assignable_agents: intersection of member sets, plus account administrators.
async fn assignable(db: &PgPool, account_id: i64, inbox_ids: &[i64]) -> Result<Vec<Value>, ApiError> {
    let mut uniq = inbox_ids.to_vec();
    uniq.sort_unstable();
    uniq.dedup();
    let n = uniq.len() as i64;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT user_id::bigint FROM account_users WHERE account_id::bigint = $1 AND role = 1
         UNION
         SELECT user_id::bigint FROM inbox_members
         WHERE $3 > 0 AND inbox_id::bigint = ANY($2::bigint[])
         GROUP BY user_id
         HAVING count(DISTINCT inbox_id) = $3",
    )
    .bind(account_id)
    .bind(&uniq)
    .bind(n)
    .fetch_all(db)
    .await?;
    agents_for(db, account_id, Some(ids)).await
}

/// `inbox_ids[]=1&inbox_ids[]=2` (Vue axios array encoding) or `inbox_ids=1`.
fn parse_inbox_ids(raw: Option<String>) -> Vec<i64> {
    raw.unwrap_or_default()
        .split('&')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            let k = k.replace("%5B", "[").replace("%5D", "]");
            (k == "inbox_ids[]" || k == "inbox_ids").then(|| v.parse::<i64>().ok()).flatten()
        })
        .collect()
}

// ---------- inboxes ----------

async fn update_email_channel(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, account_id: i64, id: i64, ch: &Value) -> Result<(), ApiError> {
    if let Some(email) = ch.get("email") {
        if !email.as_str().is_some_and(|v| v.parse::<lettre::Address>().is_ok()) {
            return Err(ApiError::bad("Email is invalid"));
        }
    }
    let keys = ["email", "imap_enabled", "imap_address", "imap_port", "imap_login", "imap_password", "imap_enable_ssl", "imap_authentication", "smtp_enabled", "smtp_address", "smtp_port", "smtp_login", "smtp_password", "smtp_domain", "smtp_enable_starttls_auto", "smtp_authentication", "smtp_openssl_verify_mode", "smtp_enable_ssl_tls"];
    // Passwords are never sent back to the client, so a blank password field means "keep the stored one".
    let fields: serde_json::Map<String, Value> = keys.iter().filter_map(|key| ch.get(*key).filter(|v| !v.is_null() && !(key.ends_with("_password") && v.as_str() == Some(""))).map(|v| ((*key).to_owned(), v.clone()))).collect();
    sqlx::query("UPDATE channel_email c SET (email, imap_enabled, imap_address, imap_port, imap_login, imap_password, imap_enable_ssl, imap_authentication, smtp_enabled, smtp_address, smtp_port, smtp_login, smtp_password, smtp_domain, smtp_enable_starttls_auto, smtp_authentication, smtp_openssl_verify_mode, smtp_enable_ssl_tls, updated_at) = (SELECT x.email, x.imap_enabled, x.imap_address, x.imap_port, x.imap_login, x.imap_password, x.imap_enable_ssl, x.imap_authentication, x.smtp_enabled, x.smtp_address, x.smtp_port, x.smtp_login, x.smtp_password, x.smtp_domain, x.smtp_enable_starttls_auto, x.smtp_authentication, x.smtp_openssl_verify_mode, x.smtp_enable_ssl_tls, now() FROM jsonb_populate_record(c, $3::jsonb) x) WHERE c.id = $1 AND c.account_id::bigint = $2")
        .bind(id).bind(account_id).bind(Value::Object(fields)).execute(&mut **tx).await?;
    Ok(())
}

/// Mirrors api/v1/models/_inbox.json.jbuilder (+ channel-specific attrs).
async fn inbox_json(db: &PgPool, account_id: i64, inbox_id: i64, admin: bool) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT channel_id::bigint AS channel_id, name, channel_type, greeting_enabled, greeting_message,
                working_hours_enabled, enable_email_collect, csat_survey_enabled, csat_config,
                enable_auto_assignment, auto_assignment_config, out_of_office_message, timezone,
                allow_messages_after_resolved, lock_to_single_conversation, sender_name_type, business_name
         FROM inboxes WHERE id::bigint = $1 AND account_id::bigint = $2",
    )
    .bind(inbox_id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let channel_id: i64 = r.try_get("channel_id")?;
    let channel_type: Option<String> = r.try_get("channel_type")?;
    let ct = channel_type.clone().unwrap_or_default();
    let sender: i32 = r.try_get("sender_name_type")?;
    let sender_name = if sender == 1 { "external" } else { "friendly" };

    let mut out = json!({
        "id": inbox_id,
        "avatar_url": "",
        "channel_id": channel_id,
        "name": r.try_get::<String, _>("name")?,
        "channel_type": channel_type,
        "greeting_enabled": r.try_get::<Option<bool>, _>("greeting_enabled")?,
        "greeting_message": r.try_get::<Option<String>, _>("greeting_message")?,
        "working_hours_enabled": r.try_get::<Option<bool>, _>("working_hours_enabled")?,
        "enable_email_collect": r.try_get::<Option<bool>, _>("enable_email_collect")?,
        "csat_survey_enabled": r.try_get::<Option<bool>, _>("csat_survey_enabled")?,
        "csat_config": r.try_get::<Option<Value>, _>("csat_config")?,
        "enable_auto_assignment": r.try_get::<Option<bool>, _>("enable_auto_assignment")?,
        "auto_assignment_config": r.try_get::<Option<Value>, _>("auto_assignment_config")?,
        "out_of_office_message": r.try_get::<Option<String>, _>("out_of_office_message")?,
        "working_hours": [],
        "timezone": r.try_get::<Option<String>, _>("timezone")?,
        "callback_webhook_url": null,
        "allow_messages_after_resolved": r.try_get::<Option<bool>, _>("allow_messages_after_resolved")?,
        "lock_to_single_conversation": r.try_get::<bool, _>("lock_to_single_conversation")?,
        "sender_name_type": sender_name,
        "business_name": r.try_get::<Option<String>, _>("business_name")?,
    });
    let obj = out.as_object_mut().expect("object");

    match ct.as_str() {
        "Channel::Email" => {
            let c: Option<Value> = sqlx::query_scalar("SELECT to_jsonb(c) - 'imap_password' - 'smtp_password' - 'provider_config' FROM channel_email c WHERE id = $1 AND account_id = $2")
                .bind(channel_id).bind(account_id).fetch_optional(db).await?;
            if let Some(Value::Object(fields)) = c {
                for (key, value) in fields {
                    if key == "email" || key == "forward_to_email" || (admin && (key.starts_with("imap_") || key.starts_with("smtp_"))) {
                        obj.insert(key, value);
                    }
                }
                obj.insert("forwarding_enabled".into(), json!(std::env::var("MAILER_INBOUND_EMAIL_DOMAIN").is_ok_and(|v| !v.is_empty())));
            }
        }
        "Channel::Api" => {
            let c = sqlx::query(
                "SELECT webhook_url, identifier, hmac_token, hmac_mandatory, secret, additional_attributes
                 FROM channel_api WHERE id::bigint = $1 AND account_id::bigint = $2",
            )
            .bind(channel_id)
            .bind(account_id)
            .fetch_optional(db)
            .await?;
            if let Some(c) = c {
                obj.insert("webhook_url".into(), c.try_get::<Option<String>, _>("webhook_url")?.into());
                obj.insert("inbox_identifier".into(), c.try_get::<Option<String>, _>("identifier")?.into());
                obj.insert("hmac_mandatory".into(), c.try_get::<Option<bool>, _>("hmac_mandatory")?.into());
                obj.insert("additional_attributes".into(), c.try_get::<Option<Value>, _>("additional_attributes")?.into());
                if admin {
                    obj.insert("hmac_token".into(), c.try_get::<Option<String>, _>("hmac_token")?.into());
                    obj.insert("secret".into(), c.try_get::<Option<String>, _>("secret")?.into());
                }
            }
        }
        "Channel::WebWidget" => {
            let c = sqlx::query(
                "SELECT website_url, widget_color, website_token, welcome_title, welcome_tagline, hmac_token,
                        hmac_mandatory, pre_chat_form_enabled, pre_chat_form_options, reply_time,
                        feature_flags::bigint AS feature_flags, continuity_via_email, allowed_domains
                 FROM channel_web_widgets WHERE id::bigint = $1 AND account_id::bigint = $2",
            )
            .bind(channel_id)
            .bind(account_id)
            .fetch_optional(db)
            .await?;
            if let Some(c) = c {
                let token: Option<String> = c.try_get("website_token")?;
                let rt: Option<i32> = c.try_get("reply_time")?;
                let flags: i64 = c.try_get("feature_flags")?;
                obj.insert("website_url".into(), c.try_get::<Option<String>, _>("website_url")?.into());
                obj.insert("widget_color".into(), c.try_get::<Option<String>, _>("widget_color")?.into());
                obj.insert("website_token".into(), token.clone().into());
                obj.insert("welcome_title".into(), c.try_get::<Option<String>, _>("welcome_title")?.into());
                obj.insert("welcome_tagline".into(), c.try_get::<Option<String>, _>("welcome_tagline")?.into());
                let base = std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".into());
                obj.insert("web_widget_script".into(), json!(widget_script(&base, &token.clone().unwrap_or_default())));
                obj.insert("pre_chat_form_enabled".into(), c.try_get::<Option<bool>, _>("pre_chat_form_enabled")?.into());
                obj.insert("pre_chat_form_options".into(), c.try_get::<Option<Value>, _>("pre_chat_form_options")?.into());
                obj.insert("reply_time".into(), json!(reply_time_name(rt.unwrap_or(0) as i64)));
                obj.insert("selected_feature_flags".into(), json!(flags_to_names(flags)));
                obj.insert("continuity_via_email".into(), c.try_get::<Option<bool>, _>("continuity_via_email")?.into());
                obj.insert("allowed_domains".into(), c.try_get::<Option<String>, _>("allowed_domains")?.into());
                obj.insert("hmac_mandatory".into(), c.try_get::<Option<bool>, _>("hmac_mandatory")?.into());
                if admin {
                    obj.insert("hmac_token".into(), c.try_get::<Option<String>, _>("hmac_token")?.into());
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

async fn inbox_index(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM inboxes WHERE account_id::bigint = $1
           AND ($2::bool OR id::bigint IN (SELECT inbox_id::bigint FROM inbox_members WHERE user_id::bigint = $3))
         ORDER BY name",
    )
    .bind(account_id)
    .bind(au.is_admin())
    .bind(user.id)
    .fetch_all(&st.db)
    .await?;
    let mut payload = Vec::with_capacity(ids.len());
    for id in ids {
        payload.push(inbox_json(&st.db, account_id, id, au.is_admin()).await?);
    }
    Ok(Json(json!({ "payload": payload })))
}

async fn inbox_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    can_see_inbox(&st.db, &au, id).await?;
    Ok(Json(inbox_json(&st.db, account_id, id, au.is_admin()).await?))
}

async fn inbox_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    let name = ostr(&body, "name").filter(|s| !s.trim().is_empty()).ok_or_else(|| ApiError::bad("Name can't be blank"))?;
    let ch = &body["channel"];

    let mut tx = st.db.begin().await?;
    let (channel_id, channel_type): (i64, &str) = match ch["type"].as_str() {
        Some("email") => {
            let email = ostr(ch, "email").filter(|s| s.parse::<lettre::Address>().is_ok()).ok_or_else(|| ApiError::bad("Email is invalid"))?;
            let domain = std::env::var("MAILER_INBOUND_EMAIL_DOMAIN").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "localhost".into());
            let id: i64 = sqlx::query_scalar("INSERT INTO channel_email (account_id, email, forward_to_email, created_at, updated_at) VALUES ($1, $2, $3, now(), now()) RETURNING id")
                .bind(account_id as i32).bind(email).bind(format!("{}@{}", token(), domain)).fetch_one(&mut *tx).await?;
            update_email_channel(&mut tx, account_id, id, ch).await?;
            (id, "Channel::Email")
        }
        Some("api") => {
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO channel_api (account_id, webhook_url, identifier, hmac_token, secret, created_at, updated_at)
                 VALUES ($1::bigint, $2, $3, $4, $5, now(), now()) RETURNING id::bigint",
            )
            .bind(account_id)
            .bind(ostr(ch, "webhook_url"))
            .bind(token())
            .bind(token())
            .bind(token())
            .fetch_one(&mut *tx)
            .await?;
            (id, "Channel::Api")
        }
        Some("web_widget") => {
            let website_url = ostr(ch, "website_url").filter(|s| !s.is_empty()).ok_or_else(|| ApiError::bad("Website url can't be blank"))?;
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO channel_web_widgets (account_id, website_url, widget_color, welcome_title, welcome_tagline,
                    website_token, hmac_token, reply_time, created_at, updated_at)
                 VALUES ($1::bigint, $2, COALESCE($3::varchar, '#1f93ff'), $4, $5, $6, $7, 0, now(), now())
                 RETURNING id::bigint",
            )
            .bind(account_id)
            .bind(website_url)
            .bind(ostr(ch, "widget_color"))
            .bind(ostr(ch, "welcome_title"))
            .bind(ostr(ch, "welcome_tagline"))
            .bind(token())
            .bind(token())
            .fetch_one(&mut *tx)
            .await?;
            (id, "Channel::WebWidget")
        }
        _ => return Err(ApiError::bad("Invalid channel type")),
    };

    let inbox_id: i64 = sqlx::query_scalar(
        "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
         VALUES ($1::bigint, $2::bigint, $3, $4, now(), now()) RETURNING id::bigint",
    )
    .bind(channel_id)
    .bind(account_id)
    .bind(&name)
    .bind(channel_type)
    .fetch_one(&mut *tx)
    .await?;

    // Creator becomes a member so agents can see the inbox they just made.
    sqlx::query(
        "INSERT INTO inbox_members (user_id, inbox_id, created_at, updated_at) VALUES ($1::bigint, $2::bigint, now(), now())",
    )
    .bind(user.id)
    .bind(inbox_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    st.hub.invalidate_cache(account_id, "inbox");
    Ok(Json(inbox_json(&st.db, account_id, inbox_id, true).await?))
}

async fn inbox_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    inbox_in_account(&st.db, account_id, id).await?;
    let b = pick(&body, "inbox");
    let ch = &body["channel"];

    let mut tx = st.db.begin().await?;
    sqlx::query(
        "UPDATE inboxes SET
            name = COALESCE($3, name),
            enable_auto_assignment = COALESCE($4, enable_auto_assignment),
            greeting_enabled = COALESCE($5, greeting_enabled),
            greeting_message = COALESCE($6, greeting_message),
            working_hours_enabled = COALESCE($7, working_hours_enabled),
            out_of_office_message = COALESCE($8, out_of_office_message),
            timezone = COALESCE($9, timezone),
            enable_email_collect = COALESCE($10, enable_email_collect),
            csat_survey_enabled = COALESCE($11, csat_survey_enabled),
            allow_messages_after_resolved = COALESCE($12, allow_messages_after_resolved),
            lock_to_single_conversation = COALESCE($13, lock_to_single_conversation),
            business_name = COALESCE($14, business_name),
            updated_at = now()
         WHERE id::bigint = $1 AND account_id::bigint = $2",
    )
    .bind(id)
    .bind(account_id)
    .bind(ostr(b, "name").filter(|s| !s.trim().is_empty()))
    .bind(obool(b, "enable_auto_assignment"))
    .bind(obool(b, "greeting_enabled"))
    .bind(ostr(b, "greeting_message"))
    .bind(obool(b, "working_hours_enabled"))
    .bind(ostr(b, "out_of_office_message"))
    .bind(ostr(b, "timezone"))
    .bind(obool(b, "enable_email_collect"))
    .bind(obool(b, "csat_survey_enabled"))
    .bind(obool(b, "allow_messages_after_resolved"))
    .bind(obool(b, "lock_to_single_conversation"))
    .bind(ostr(b, "business_name"))
    .execute(&mut *tx)
    .await?;

    if ch.is_object() {
        let row: Option<(i64, Option<String>)> = sqlx::query_as(
            "SELECT channel_id::bigint, channel_type FROM inboxes WHERE id::bigint = $1 AND account_id::bigint = $2",
        )
        .bind(id)
        .bind(account_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((channel_id, Some(ct))) = row {
            match ct.as_str() {
                "Channel::Email" => update_email_channel(&mut tx, account_id, channel_id, ch).await?,
                "Channel::Api" => {
                    sqlx::query(
                        "UPDATE channel_api SET webhook_url = COALESCE($3, webhook_url),
                            hmac_mandatory = COALESCE($4, hmac_mandatory), updated_at = now()
                         WHERE id::bigint = $1 AND account_id::bigint = $2",
                    )
                    .bind(channel_id)
                    .bind(account_id)
                    .bind(ostr(ch, "webhook_url"))
                    .bind(obool(ch, "hmac_mandatory"))
                    .execute(&mut *tx)
                    .await?;
                }
                "Channel::WebWidget" => {
                    let flags: Option<i64> = ch["selected_feature_flags"].is_array().then(|| names_to_flags(&ch["selected_feature_flags"]));
                    let reply: Option<i64> = reply_time_to_int(&ch["reply_time"]);
                    let pre_chat: Option<Value> = ch.get("pre_chat_form_options").filter(|v| v.is_object()).cloned();
                    sqlx::query(
                        "UPDATE channel_web_widgets SET
                            website_url = COALESCE($3, website_url),
                            widget_color = COALESCE($4, widget_color),
                            welcome_title = COALESCE($5, welcome_title),
                            welcome_tagline = COALESCE($6, welcome_tagline),
                            reply_time = COALESCE($7::bigint, reply_time),
                            pre_chat_form_enabled = COALESCE($8, pre_chat_form_enabled),
                            pre_chat_form_options = COALESCE($9::jsonb, pre_chat_form_options),
                            continuity_via_email = COALESCE($10, continuity_via_email),
                            feature_flags = COALESCE($11::bigint, feature_flags),
                            updated_at = now()
                         WHERE id::bigint = $1 AND account_id::bigint = $2",
                    )
                    .bind(channel_id)
                    .bind(account_id)
                    .bind(ostr(ch, "website_url"))
                    .bind(ostr(ch, "widget_color"))
                    .bind(ostr(ch, "welcome_title"))
                    .bind(ostr(ch, "welcome_tagline"))
                    .bind(reply)
                    .bind(obool(ch, "pre_chat_form_enabled"))
                    .bind(pre_chat)
                    .bind(obool(ch, "continuity_via_email"))
                    .bind(flags)
                    .execute(&mut *tx)
                    .await?;
                }
                _ => {}
            }
        }
    }
    tx.commit().await?;

    st.hub.invalidate_cache(account_id, "inbox");
    Ok(Json(inbox_json(&st.db, account_id, id, true).await?))
}

async fn inbox_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    inbox_in_account(&st.db, account_id, id).await?;

    // Guard: refuse rather than silently destroy conversation history.
    let has_conv: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE inbox_id::bigint = $1 AND account_id::bigint = $2)",
    )
    .bind(id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;
    if has_conv {
        return Err(ApiError::bad("Inbox has conversations; delete them first"));
    }

    let mut tx = st.db.begin().await?;
    let (channel_id, channel_type): (i64, Option<String>) = sqlx::query_as(
        "SELECT channel_id::bigint, channel_type FROM inboxes WHERE id::bigint = $1 AND account_id::bigint = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM contact_inboxes WHERE inbox_id::bigint = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM inbox_members WHERE inbox_id::bigint = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM working_hours WHERE inbox_id::bigint = $1 AND account_id::bigint = $2")
        .bind(id)
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
    match channel_type.as_deref() {
        Some("Channel::Api") => {
            sqlx::query("DELETE FROM channel_api WHERE id::bigint = $1 AND account_id::bigint = $2")
                .bind(channel_id)
                .bind(account_id)
                .execute(&mut *tx)
                .await?;
        }
        Some("Channel::WebWidget") => {
            sqlx::query("DELETE FROM channel_web_widgets WHERE id::bigint = $1 AND account_id::bigint = $2")
                .bind(channel_id)
                .bind(account_id)
                .execute(&mut *tx)
                .await?;
        }
        _ => {}
    }
    sqlx::query("DELETE FROM inboxes WHERE id::bigint = $1 AND account_id::bigint = $2")
        .bind(id)
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    st.hub.invalidate_cache(account_id, "inbox");
    Ok(Json(json!({ "message": "Inbox deleted successfully" })))
}

async fn inbox_assignable(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    can_see_inbox(&st.db, &au, id).await?;
    let agents = assignable(&st.db, account_id, &[id]).await?;
    Ok(Json(json!({ "payload": agents })))
}

async fn assignable_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    RawQuery(raw): RawQuery,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let ids = parse_inbox_ids(raw);
    for id in &ids {
        can_see_inbox(&st.db, &au, *id).await?;
    }
    let agents = assignable(&st.db, account_id, &ids).await?;
    Ok(Json(json!({ "payload": agents })))
}

// ---------- inbox members ----------

async fn members_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, inbox_id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    can_see_inbox(&st.db, &au, inbox_id).await?;
    let ids = member_ids(&st.db, inbox_id).await?;
    Ok(Json(json!({ "payload": agents_for(&st.db, account_id, Some(ids)).await? })))
}

/// POST/PATCH /inbox_members {inbox_id, user_ids}: replace the member set.
async fn members_replace(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    let inbox_id = body["inbox_id"].as_i64().ok_or_else(|| ApiError::bad("Invalid inbox"))?;
    inbox_in_account(&st.db, account_id, inbox_id).await?;
    let ids = user_ids_of(&body, "user_ids")?;
    check_users(&st.db, account_id, &ids).await?;

    let mut tx = st.db.begin().await?;
    sqlx::query("DELETE FROM inbox_members WHERE inbox_id::bigint = $1 AND NOT (user_id::bigint = ANY($2::bigint[]))")
        .bind(inbox_id)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    for uid in &ids {
        sqlx::query(
            "INSERT INTO inbox_members (user_id, inbox_id, created_at, updated_at)
             VALUES ($1::bigint, $2::bigint, now(), now()) ON CONFLICT DO NOTHING",
        )
        .bind(uid)
        .bind(inbox_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    st.hub.invalidate_cache(account_id, "inbox");

    let agents = agents_for(&st.db, account_id, Some(member_ids(&st.db, inbox_id).await?)).await?;
    Ok(Json(json!({ "payload": agents })))
}

/// DELETE /inbox_members and /inbox_members/destroy {inbox_id, user_ids}.
async fn members_remove(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    let inbox_id = body["inbox_id"].as_i64().ok_or_else(|| ApiError::bad("Invalid inbox"))?;
    inbox_in_account(&st.db, account_id, inbox_id).await?;
    let ids = user_ids_of(&body, "user_ids")?;

    sqlx::query("DELETE FROM inbox_members WHERE inbox_id::bigint = $1 AND user_id::bigint = ANY($2::bigint[])")
        .bind(inbox_id)
        .bind(&ids)
        .execute(&st.db)
        .await?;
    st.hub.invalidate_cache(account_id, "inbox");

    let agents = agents_for(&st.db, account_id, Some(member_ids(&st.db, inbox_id).await?)).await?;
    Ok(Json(json!({ "payload": agents })))
}

// ---------- agents ----------

async fn agents_index(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(json!(agents_for(&st.db, account_id, None).await?)))
}

async fn agent_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    let b = pick(&body, "agent");
    let email = ostr(b, "email").unwrap_or_default().trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::bad("Email is invalid"));
    }
    let name = ostr(b, "name")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| email.split('@').next().unwrap_or("agent").to_string());
    let role: i64 = if b["role"].as_str() == Some("administrator") { 1 } else { 0 };

    let mut tx = st.db.begin().await?;
    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM users WHERE lower(email) = $1 AND provider = 'email'",
    )
    .bind(&email)
    .fetch_optional(&mut *tx)
    .await?;
    let uid = match existing {
        Some(u) => u,
        None => {
            // ponytail: no invite email; account is confirmed with a random password the agent must reset.
            let hash = hash_pw(token()).await?;
            sqlx::query_scalar(
                "INSERT INTO users (provider, uid, encrypted_password, name, email, confirmed_at, pubsub_token,
                    created_at, updated_at, tokens)
                 VALUES ('email', $1, $2, $3, $1, now(), $4, now(), now(), '{}') RETURNING id::bigint",
            )
            .bind(&email)
            .bind(hash)
            .bind(&name)
            .bind(token())
            .fetch_one(&mut *tx)
            .await?
        }
    };
    let already: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM account_users WHERE account_id::bigint = $1 AND user_id::bigint = $2",
    )
    .bind(account_id)
    .bind(uid)
    .fetch_optional(&mut *tx)
    .await?;
    if already.is_some() {
        return Err(ApiError::bad("Agent already exists in this account"));
    }
    sqlx::query(
        "INSERT INTO account_users (account_id, user_id, role, created_at, updated_at)
         VALUES ($1::bigint, $2::bigint, $3::bigint, now(), now())",
    )
    .bind(account_id)
    .bind(uid)
    .bind(role)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let agent = agents_for(&st.db, account_id, Some(vec![uid])).await?.pop().ok_or_else(ApiError::not_found)?;
    Ok(Json(agent))
}

async fn agent_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    let b = pick(&body, "agent");

    let avail: Option<i32> = match ostr(b, "availability").as_deref() {
        None => None,
        Some("online") => Some(0),
        Some("offline") => Some(1),
        Some("busy") => Some(2),
        Some(_) => return Err(ApiError::bad("Invalid availability")),
    };
    let role: Option<i64> = match ostr(b, "role").as_deref() {
        None => None,
        Some("administrator") => Some(1),
        Some("agent") => Some(0),
        Some(_) => return Err(ApiError::bad("Invalid role")),
    };

    let mut tx = st.db.begin().await?;
    let n = sqlx::query(
        "UPDATE account_users SET
            role = COALESCE($3::bigint, role),
            availability = COALESCE($4::int, availability),
            auto_offline = COALESCE($5, auto_offline),
            updated_at = now()
         WHERE account_id::bigint = $1 AND user_id::bigint = $2",
    )
    .bind(account_id)
    .bind(id)
    .bind(role)
    .bind(avail)
    .bind(obool(b, "auto_offline"))
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    if let Some(name) = ostr(b, "name").filter(|s| !s.trim().is_empty()) {
        sqlx::query("UPDATE users SET name = $2, updated_at = now() WHERE id::bigint = $1")
            .bind(id)
            .bind(name)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    if role.is_some() {
        st.hub.invalidate_cache(account_id, "inbox");
    }
    let agent = agents_for(&st.db, account_id, Some(vec![id])).await?.pop().ok_or_else(ApiError::not_found)?;
    Ok(Json(agent))
}

/// Removes the account_user and cleans up account-scoped associations.
async fn agent_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    if id == user.id {
        return Err(ApiError::bad("You cannot remove yourself from the account"));
    }
    let mut tx = st.db.begin().await?;
    let n = sqlx::query("DELETE FROM account_users WHERE account_id::bigint = $1 AND user_id::bigint = $2")
        .bind(account_id)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    sqlx::query(
        "DELETE FROM inbox_members im USING inboxes i
         WHERE im.inbox_id = i.id AND i.account_id::bigint = $1 AND im.user_id::bigint = $2",
    )
    .bind(account_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM team_members tm USING teams t
         WHERE tm.team_id = t.id AND t.account_id::bigint = $1 AND tm.user_id::bigint = $2",
    )
    .bind(account_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM conversation_participants cp USING conversations c
         WHERE cp.conversation_id = c.id AND c.account_id::bigint = $1 AND cp.user_id::bigint = $2",
    )
    .bind(account_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE conversations SET assignee_id = NULL, updated_at = now()
         WHERE account_id::bigint = $1 AND assignee_id::bigint = $2",
    )
    .bind(account_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    st.hub.invalidate_cache(account_id, "inbox");
    st.hub.invalidate_cache(account_id, "team");
    Ok(Json(json!({})))
}

// ---------- teams ----------

async fn teams_json(db: &PgPool, account_id: i64, user_id: i64, team_id: Option<i64>) -> Result<Vec<Value>, ApiError> {
    let rows = sqlx::query(
        "SELECT t.id::bigint AS id, t.name, t.description, t.allow_auto_assign, t.icon, t.icon_color,
                t.account_id::bigint AS account_id,
                EXISTS(SELECT 1 FROM team_members tm WHERE tm.team_id = t.id AND tm.user_id::bigint = $3::bigint) AS is_member
         FROM teams t
         WHERE t.account_id::bigint = $1::bigint AND ($2::bigint IS NULL OR t.id = $2::bigint)
         ORDER BY t.name",
    )
    .bind(account_id)
    .bind(team_id)
    .bind(user_id)
    .fetch_all(db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        out.push(json!({
            "id": r.try_get::<i64, _>("id")?,
            "name": r.try_get::<String, _>("name")?,
            "description": r.try_get::<Option<String>, _>("description")?,
            "allow_auto_assign": r.try_get::<Option<bool>, _>("allow_auto_assign")?,
            "icon": r.try_get::<Option<String>, _>("icon")?,
            "icon_color": r.try_get::<Option<String>, _>("icon_color")?,
            "account_id": r.try_get::<i64, _>("account_id")?,
            "is_member": r.try_get::<bool, _>("is_member")?,
        }));
    }
    Ok(out)
}

async fn team_exists(db: &PgPool, account_id: i64, team_id: i64) -> Result<(), ApiError> {
    let ok: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM teams WHERE id::bigint = $1 AND account_id::bigint = $2)")
        .bind(team_id)
        .bind(account_id)
        .fetch_one(db)
        .await?;
    if ok { Ok(()) } else { Err(ApiError::not_found()) }
}

async fn teams_index(State(st): State<AppState>, user: CurrentUser, Path(account_id): Path<i64>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(json!(teams_json(&st.db, account_id, user.id, None).await?)))
}

async fn team_show(State(st): State<AppState>, user: CurrentUser, Path((account_id, id)): Path<(i64, i64)>) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let t = teams_json(&st.db, account_id, user.id, Some(id)).await?.pop().ok_or_else(ApiError::not_found)?;
    Ok(Json(t))
}

async fn team_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    let b = pick(&body, "team");
    let name = ostr(b, "name").filter(|s| !s.trim().is_empty()).ok_or_else(|| ApiError::bad("Name can't be blank"))?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO teams (name, description, allow_auto_assign, icon, icon_color, account_id, created_at, updated_at)
         VALUES ($1, $2, COALESCE($3::bool, true), COALESCE($4::varchar, ''), COALESCE($5::varchar, ''), $6::bigint, now(), now())
         RETURNING id::bigint",
    )
    .bind(name)
    .bind(ostr(b, "description"))
    .bind(obool(b, "allow_auto_assign"))
    .bind(ostr(b, "icon"))
    .bind(ostr(b, "icon_color"))
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;
    st.hub.invalidate_cache(account_id, "team");
    Ok(Json(teams_json(&st.db, account_id, user.id, Some(id)).await?.pop().ok_or_else(ApiError::not_found)?))
}

async fn team_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    team_exists(&st.db, account_id, id).await?;
    let b = pick(&body, "team");
    sqlx::query(
        "UPDATE teams SET name = COALESCE($3, name), description = COALESCE($4, description),
            allow_auto_assign = COALESCE($5, allow_auto_assign), icon = COALESCE($6, icon),
            icon_color = COALESCE($7, icon_color), updated_at = now()
         WHERE id::bigint = $1 AND account_id::bigint = $2",
    )
    .bind(id)
    .bind(account_id)
    .bind(ostr(b, "name").filter(|s| !s.trim().is_empty()))
    .bind(ostr(b, "description"))
    .bind(obool(b, "allow_auto_assign"))
    .bind(ostr(b, "icon"))
    .bind(ostr(b, "icon_color"))
    .execute(&st.db)
    .await?;
    st.hub.invalidate_cache(account_id, "team");
    Ok(Json(teams_json(&st.db, account_id, user.id, Some(id)).await?.pop().ok_or_else(ApiError::not_found)?))
}

async fn team_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    team_exists(&st.db, account_id, id).await?;

    // Guard: do not orphan or rewrite conversation assignments silently.
    let has_conv: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE team_id::bigint = $1 AND account_id::bigint = $2)",
    )
    .bind(id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;
    if has_conv {
        return Err(ApiError::bad("Team has conversations; reassign them first"));
    }

    let mut tx = st.db.begin().await?;
    sqlx::query("DELETE FROM team_members WHERE team_id::bigint = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM teams WHERE id::bigint = $1 AND account_id::bigint = $2")
        .bind(id)
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    st.hub.invalidate_cache(account_id, "team");
    Ok(Json(json!({})))
}

async fn team_member_list(db: &PgPool, account_id: i64, team_id: i64) -> Result<Value, ApiError> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT user_id::bigint FROM team_members WHERE team_id::bigint = $1")
        .bind(team_id)
        .fetch_all(db)
        .await?;
    Ok(json!(agents_for(db, account_id, Some(ids)).await?))
}

async fn team_members_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    team_exists(&st.db, account_id, id).await?;
    Ok(Json(team_member_list(&st.db, account_id, id).await?))
}

async fn team_members_add(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    team_exists(&st.db, account_id, id).await?;
    let ids = user_ids_of(&body, "user_ids")?;
    check_users(&st.db, account_id, &ids).await?;
    for uid in &ids {
        sqlx::query(
            "INSERT INTO team_members (team_id, user_id, created_at, updated_at)
             VALUES ($1::bigint, $2::bigint, now(), now()) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(uid)
        .execute(&st.db)
        .await?;
    }
    st.hub.invalidate_cache(account_id, "team");
    Ok(Json(team_member_list(&st.db, account_id, id).await?))
}

async fn team_members_replace(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    team_exists(&st.db, account_id, id).await?;
    let ids = user_ids_of(&body, "user_ids")?;
    check_users(&st.db, account_id, &ids).await?;
    let mut tx = st.db.begin().await?;
    sqlx::query("DELETE FROM team_members WHERE team_id::bigint = $1 AND NOT (user_id::bigint = ANY($2::bigint[]))")
        .bind(id)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    for uid in &ids {
        sqlx::query(
            "INSERT INTO team_members (team_id, user_id, created_at, updated_at)
             VALUES ($1::bigint, $2::bigint, now(), now()) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    st.hub.invalidate_cache(account_id, "team");
    Ok(Json(team_member_list(&st.db, account_id, id).await?))
}

async fn team_members_remove(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    team_exists(&st.db, account_id, id).await?;
    let ids = user_ids_of(&body, "user_ids")?;
    sqlx::query("DELETE FROM team_members WHERE team_id::bigint = $1 AND user_id::bigint = ANY($2::bigint[])")
        .bind(id)
        .bind(&ids)
        .execute(&st.db)
        .await?;
    st.hub.invalidate_cache(account_id, "team");
    Ok(Json(team_member_list(&st.db, account_id, id).await?))
}
