//! Captain AI Enterprise endpoints: preferences, assistants, scenarios, inboxes,
//! documents, assistant responses, FAQ suggestions, bulk actions, message reports,
//! agent sessions, copilot threads/messages/runs, custom tools, MCP servers,
//! tools manifest, and AI writing tasks.
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2_compat::{Digest, Sha256};
use sqlx::{PgPool, Row};

use crate::common::*;

const PAGE_SIZE: i64 = 25;

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/captain/preferences",
            get(preferences_show)
                .patch(preferences_update)
                .put(preferences_update),
        )
        // Assistants
        .route(
            "/captain/assistants",
            get(assistants_index).post(assistants_create),
        )
        .route("/captain/assistants/tools", get(assistants_tools))
        .route(
            "/captain/assistants/{id}",
            get(assistants_show)
                .patch(assistants_update)
                .put(assistants_update)
                .delete(assistants_destroy),
        )
        .route(
            "/captain/assistants/{id}/avatar",
            delete(assistants_avatar_destroy),
        )
        .route(
            "/captain/assistants/{id}/playground",
            post(assistants_playground),
        )
        .route("/captain/assistants/{id}/metrics", get(assistants_metrics))
        .route(
            "/captain/assistants/{id}/faq_stats",
            get(assistants_faq_stats),
        )
        .route("/captain/assistants/{id}/summary", get(assistants_summary))
        .route(
            "/captain/assistants/{id}/drilldown",
            get(assistants_drilldown),
        )
        .route(
            "/captain/assistants/{id}/stats/drilldown",
            get(stats_drilldown),
        )
        .route(
            "/captain/assistants/{id}/stats/overview",
            get(stats_overview),
        )
        .route(
            "/captain/assistants/{id}/stats/overview_summary",
            get(stats_overview_summary),
        )
        .route(
            "/captain/assistants/{id}/stats/resolution_flow",
            get(stats_resolution_flow),
        )
        .route(
            "/captain/assistants/{id}/stats/resolution_trend",
            get(stats_resolution_trend),
        )
        // Assistant inboxes
        .route(
            "/captain/assistants/{assistant_id}/inboxes",
            get(inboxes_index).post(inboxes_create),
        )
        .route(
            "/captain/assistants/{assistant_id}/inboxes/{inbox_id}",
            delete(inboxes_destroy),
        )
        // Scenarios
        .route(
            "/captain/assistants/{assistant_id}/scenarios",
            get(scenarios_index).post(scenarios_create),
        )
        .route(
            "/captain/assistants/{assistant_id}/scenarios/{id}",
            get(scenarios_show)
                .patch(scenarios_update)
                .put(scenarios_update)
                .delete(scenarios_destroy),
        )
        // Agent sessions & reports
        .route("/captain/agent_sessions/{id}", get(agent_session_show))
        .route("/captain/message_reports", post(message_reports_create))
        .route("/captain/bulk_actions", post(bulk_actions_create))
        // Assistant responses (FAQs)
        .route(
            "/captain/assistant_responses",
            get(responses_index).post(responses_create),
        )
        .route(
            "/captain/assistant_responses/{id}",
            get(responses_show)
                .patch(responses_update)
                .put(responses_update)
                .delete(responses_destroy),
        )
        .route(
            "/captain/assistant_responses/{id}/drilldown",
            get(responses_drilldown),
        )
        // FAQ suggestions
        .route("/captain/faq_suggestions", get(faq_suggestions_index))
        .route(
            "/captain/faq_suggestions/{id}",
            get(faq_suggestions_show)
                .patch(faq_suggestions_update)
                .put(faq_suggestions_update),
        )
        .route(
            "/captain/faq_suggestions/{id}/approve",
            post(faq_suggestions_approve),
        )
        .route(
            "/captain/faq_suggestions/{id}/dismiss",
            post(faq_suggestions_dismiss),
        )
        // Documents
        .route(
            "/captain/documents",
            get(documents_index).post(documents_create),
        )
        .route(
            "/captain/documents/{id}",
            get(documents_show).delete(documents_destroy),
        )
        .route("/captain/documents/{id}/sync", post(documents_sync))
        .route(
            "/captain/documents/{id}/drilldown",
            get(documents_drilldown),
        )
        // Copilot threads / messages / runs
        .route(
            "/captain/copilot_threads",
            get(copilot_threads_index).post(copilot_threads_create),
        )
        .route(
            "/captain/copilot_threads/{id}/stop",
            post(copilot_threads_stop),
        )
        .route(
            "/captain/copilot_threads/{copilot_thread_id}/copilot_messages",
            get(copilot_messages_index).post(copilot_messages_create),
        )
        .route(
            "/captain/copilot_runs/{id}/approve",
            post(copilot_runs_approve),
        )
        .route(
            "/captain/copilot_runs/{id}/reject",
            post(copilot_runs_reject),
        )
        // Custom tools
        .route(
            "/captain/custom_tools",
            get(custom_tools_index).post(custom_tools_create),
        )
        .route("/captain/custom_tools/test", post(custom_tools_test))
        .route(
            "/captain/custom_tools/{id}",
            get(custom_tools_show)
                .patch(custom_tools_update)
                .put(custom_tools_update)
                .delete(custom_tools_destroy),
        )
        // MCP servers
        .route(
            "/captain/mcp_servers",
            get(mcp_servers_index).post(mcp_servers_create),
        )
        .route("/captain/mcp_servers/discover", post(mcp_servers_discover))
        .route(
            "/captain/mcp_servers/{id}",
            get(mcp_servers_show)
                .patch(mcp_servers_update)
                .put(mcp_servers_update)
                .delete(mcp_servers_destroy),
        )
        .route(
            "/captain/mcp_servers/{id}/live_tools",
            get(mcp_servers_live_tools),
        )
        .route(
            "/captain/mcp_servers/{id}/authorization_url",
            get(mcp_servers_auth_url),
        )
        // Tools manifest
        .route("/captain/tools_manifest/installed", get(manifest_installed))
        .route("/captain/tools_manifest/preview", post(manifest_preview))
        .route("/captain/tools_manifest/install", post(manifest_install))
        // Tasks
        .route("/captain/tasks/rewrite", post(task_rewrite))
        .route("/captain/tasks/summarize", post(task_summarize))
        .route(
            "/captain/tasks/reply_suggestion",
            post(task_reply_suggestion),
        )
        .route("/captain/tasks/follow_up", post(task_follow_up))
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/captain/mcp/callback", get(mcp_oauth_callback))
        .route("/enterprise/webhooks/firecrawl", post(firecrawl_webhook))
}

// ============================================================================
// Shared helpers & LLM integration
// ============================================================================

fn pick<'a>(v: &'a Value, k: &str) -> &'a Value {
    if v[k].is_object() { &v[k] } else { v }
}

fn sopt(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn i64_param(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| {
        x.as_i64()
            .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
    })
}

fn slugify(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        format!("tool_{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
    } else {
        out
    }
}

fn parse_yaml_or_json_str(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        let trimmed = s.trim();
        let unquoted = trimmed
            .strip_prefix("--- ")
            .unwrap_or(trimmed)
            .trim()
            .trim_matches('\'')
            .trim_matches('"')
            .trim();
        if unquoted.is_empty() || unquoted == "---" || unquoted == "null" {
            None
        } else {
            Some(unquoted.to_string())
        }
    } else if let Some(s) = v.get("value").and_then(Value::as_str) {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    } else {
        None
    }
}

async fn installation_config_str(db: &PgPool, name: &str) -> Option<String> {
    let val: Option<Value> = sqlx::query_scalar(
        "SELECT serialized_value FROM installation_configs WHERE name = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(name)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    val.and_then(|v| parse_yaml_or_json_str(&v))
}

pub(crate) struct LlmConfig {
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
}

pub(crate) async fn resolve_llm_config(
    db: &PgPool,
    model_override: Option<&str>,
) -> Result<LlmConfig, ApiError> {
    let env_url = std::env::var("CAPTAIN_LLM_API_URL")
        .ok()
        .or_else(|| std::env::var("OPENAI_API_URL").ok())
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty());

    let db_endpoint = installation_config_str(db, "CAPTAIN_OPEN_AI_ENDPOINT").await;
    let base_url = env_url
        .clone()
        .or(db_endpoint)
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let api_key = std::env::var("CAPTAIN_LLM_API_KEY")
        .ok()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or(installation_config_str(db, "CAPTAIN_OPEN_AI_API_KEY").await);

    let api_key = match api_key {
        Some(k) => k,
        None if env_url.is_some() => "mock-key".to_string(),
        None => return Err(ApiError::bad("Captain AI not configured")),
    };

    let model = model_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("CAPTAIN_LLM_MODEL")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| "gpt-4o-mini".to_string());

    Ok(LlmConfig {
        base_url,
        api_key,
        model,
    })
}

pub(crate) async fn call_llm_chat(
    cfg: &LlmConfig,
    messages: Vec<Value>,
    temperature: f64,
) -> Result<(String, Value), ApiError> {
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let payload = json!({
        "model": cfg.model,
        "messages": messages,
        "temperature": temperature,
    });
    let bytes = serde_json::to_vec(&payload).map_err(|_| ApiError::bad("LLM serialize error"))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| ApiError::bad(format!("HTTP client error: {e}")))?;
    let mut req = client
        .post(&url)
        .header("content-type", "application/json")
        .body(bytes);
    if !cfg.api_key.is_empty() {
        req = req.header("authorization", format!("Bearer {}", cfg.api_key));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("Captain LLM request failed: {e}")))?;
    let status = resp.status();
    let raw = resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad(format!("Captain LLM read failed: {e}")))?;
    let parsed: Value = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
    if !status.is_success() {
        let msg = parsed["error"]["message"]
            .as_str()
            .or_else(|| parsed["error"].as_str())
            .unwrap_or("Captain LLM error");
        return Err(ApiError::bad(msg));
    }
    let content = parsed["choices"][0]["message"]["content"]
        .as_str()
        .or_else(|| parsed["message"]["content"].as_str())
        .or_else(|| parsed["content"].as_str())
        .unwrap_or("")
        .to_string();
    Ok((content, parsed))
}

async fn resolve_llm_model_for_feature(
    db: &PgPool,
    account_id: i64,
    feature: &str,
) -> Option<String> {
    let attrs: Option<Value> =
        sqlx::query_scalar("SELECT custom_attributes FROM accounts WHERE id = $1::bigint")
            .bind(account_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    attrs.and_then(|a| {
        a.get("captain_models")
            .and_then(|m| m.get(feature))
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

// ============================================================================
// Preferences
// ============================================================================

const FEATURE_KEYS: [&str; 6] = [
    "assistant",
    "copilot",
    "label_suggestion",
    "follow_up",
    "reply_suggestion",
    "summarize",
];

fn default_model_for(feature: &str) -> &'static str {
    match feature {
        "assistant" => "gpt-4o",
        "copilot" => "gpt-4o",
        _ => "gpt-4o-mini",
    }
}

fn feature_title(feature: &str) -> &'static str {
    match feature {
        "assistant" => "Captain Assistant",
        "copilot" => "Captain Copilot",
        "label_suggestion" => "Label Suggestion",
        "follow_up" => "Follow Up",
        "reply_suggestion" => "Reply Suggestion",
        "summarize" => "Conversation Summary",
        _ => "Captain Feature",
    }
}

async fn preferences_payload(db: &PgPool, account_id: i64) -> Result<Value, ApiError> {
    let attrs: Value = sqlx::query_scalar(
        "SELECT COALESCE(custom_attributes, '{}'::jsonb) FROM accounts WHERE id = $1::bigint",
    )
    .bind(account_id)
    .fetch_one(db)
    .await?;

    let captain_models = attrs
        .get("captain_models")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let captain_features = attrs
        .get("captain_features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut features_map = Map::new();
    for key in FEATURE_KEYS {
        let def = default_model_for(key);
        let selected = captain_models
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(def);
        let enabled = captain_features
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(true);
        features_map.insert(
            key.to_string(),
            json!({
                "name": feature_title(key),
                "default": def,
                "enabled": enabled,
                "model": selected,
                "selected": selected,
                "provider": "openai",
                "source": if captain_models.contains_key(key) { "account" } else { "default" },
            }),
        );
    }

    Ok(json!({
        "providers": {
            "openai": { "name": "OpenAI" },
            "anthropic": { "name": "Anthropic" },
            "gemini": { "name": "Google Gemini" },
        },
        "models": {
            "gpt-4o": { "id": "gpt-4o", "name": "GPT-4o", "provider": "openai" },
            "gpt-4o-mini": { "id": "gpt-4o-mini", "name": "GPT-4o mini", "provider": "openai" },
            "gpt-4.1": { "id": "gpt-4.1", "name": "GPT-4.1", "provider": "openai" },
            "claude-3-7-sonnet": { "id": "claude-3-7-sonnet", "name": "Claude 3.7 Sonnet", "provider": "anthropic" },
            "gemini-2.5-flash": { "id": "gemini-2.5-flash", "name": "Gemini 2.5 Flash", "provider": "gemini" },
        },
        "features": features_map,
    }))
}

async fn preferences_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(preferences_payload(&st.db, account_id).await?))
}

async fn preferences_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let mut attrs: Value = sqlx::query_scalar(
        "SELECT COALESCE(custom_attributes, '{}'::jsonb) FROM accounts WHERE id = $1::bigint",
    )
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    let root = attrs
        .as_object_mut()
        .ok_or_else(|| ApiError::bad("invalid account attributes"))?;

    if let Some(models) = body.get("captain_models").and_then(Value::as_object) {
        let mut merged = root
            .get("captain_models")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for key in FEATURE_KEYS {
            if let Some(val) = models.get(key) {
                if val.is_null() || val.as_str().is_some_and(|s| s.trim().is_empty()) {
                    merged.remove(key);
                } else if let Some(s) = val.as_str() {
                    merged.insert(key.to_string(), json!(s.trim()));
                }
            }
        }
        root.insert("captain_models".to_string(), Value::Object(merged));
    }

    if let Some(features) = body.get("captain_features").and_then(Value::as_object) {
        let mut merged = root
            .get("captain_features")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for key in FEATURE_KEYS {
            if let Some(val) = features.get(key).and_then(Value::as_bool) {
                merged.insert(key.to_string(), json!(val));
            }
        }
        root.insert("captain_features".to_string(), Value::Object(merged));
    }

    sqlx::query(
        "UPDATE accounts SET custom_attributes = $1, updated_at = now() WHERE id = $2::bigint",
    )
    .bind(&attrs)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    Ok(Json(preferences_payload(&st.db, account_id).await?))
}

// ============================================================================
// Assistants
// ============================================================================

async fn assistant_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let row = sqlx::query(
        "SELECT id, name, description, account_id, config, response_guidelines, guardrails,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at
         FROM captain_assistants WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let inbox_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM captain_inboxes WHERE captain_assistant_id = $1")
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap_or(0);

    let scenarios_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_scenarios WHERE assistant_id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    Ok(json!({
        "id": row.try_get::<i64, _>("id")?,
        "name": row.try_get::<String, _>("name")?,
        "description": row.try_get::<Option<String>, _>("description")?,
        "account_id": row.try_get::<i64, _>("account_id")?,
        "config": row.try_get::<Value, _>("config")?,
        "response_guidelines": row.try_get::<Value, _>("response_guidelines")?,
        "guardrails": row.try_get::<Value, _>("guardrails")?,
        "connected_inboxes_count": inbox_count,
        "scenarios_count": scenarios_count,
        "avatar_url": null,
        "created_at": row.try_get::<i64, _>("created_at")?,
        "updated_at": row.try_get::<i64, _>("updated_at")?,
    }))
}

async fn ensure_assistant(db: &PgPool, account_id: i64, assistant_id: i64) -> Result<(), ApiError> {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM captain_assistants WHERE id = $1 AND account_id = $2")
            .bind(assistant_id)
            .bind(account_id)
            .fetch_optional(db)
            .await?;
    if exists.is_none() {
        return Err(ApiError::not_found());
    }
    Ok(())
}

async fn assistants_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM captain_assistants WHERE account_id = $1 ORDER BY created_at DESC, id DESC",
    )
    .bind(account_id)
    .fetch_all(&st.db)
    .await?;
    let mut payload = Vec::with_capacity(ids.len());
    for id in &ids {
        payload.push(assistant_json(&st.db, account_id, *id).await?);
    }
    Ok(Json(json!({
        "payload": payload,
        "meta": { "total_count": ids.len(), "page": 1 }
    })))
}

async fn assistants_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(assistant_json(&st.db, account_id, id).await?))
}

async fn assistants_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "assistant");
    let name = sopt(b, "name").ok_or_else(|| ApiError::bad("Name can't be blank"))?;
    let description = b.get("description").and_then(Value::as_str);
    let config = b
        .get("config")
        .filter(|x| x.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let response_guidelines = b
        .get("response_guidelines")
        .filter(|x| x.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));
    let guardrails = b
        .get("guardrails")
        .filter(|x| x.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO captain_assistants (name, account_id, description, config, response_guidelines, guardrails, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, now(), now()) RETURNING id",
    )
    .bind(&name)
    .bind(account_id)
    .bind(description)
    .bind(config)
    .bind(response_guidelines)
    .bind(guardrails)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(assistant_json(&st.db, account_id, id).await?))
}

async fn assistants_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "assistant");

    let cur = sqlx::query(
        "SELECT name, description, config, response_guidelines, guardrails
         FROM captain_assistants WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let name = sopt(b, "name").unwrap_or_else(|| cur.try_get("name").unwrap_or_default());
    let description = if b.get("description").is_some() {
        b.get("description")
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        cur.try_get("description").ok().flatten()
    };

    let mut config = cur
        .try_get::<Value, _>("config")
        .unwrap_or_else(|_| json!({}));
    if let Some(patch_cfg) = b.get("config").and_then(Value::as_object)
        && let Some(obj) = config.as_object_mut()
    {
        for (k, v) in patch_cfg {
            obj.insert(k.clone(), v.clone());
        }
    }

    let response_guidelines = b
        .get("response_guidelines")
        .filter(|x| x.is_array())
        .cloned()
        .unwrap_or_else(|| {
            cur.try_get("response_guidelines")
                .unwrap_or_else(|_| json!([]))
        });
    let guardrails = b
        .get("guardrails")
        .filter(|x| x.is_array())
        .cloned()
        .unwrap_or_else(|| cur.try_get("guardrails").unwrap_or_else(|_| json!([])));

    sqlx::query(
        "UPDATE captain_assistants
         SET name = $1, description = $2, config = $3, response_guidelines = $4, guardrails = $5, updated_at = now()
         WHERE id = $6 AND account_id = $7",
    )
    .bind(&name)
    .bind(&description)
    .bind(config)
    .bind(response_guidelines)
    .bind(guardrails)
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    Ok(Json(assistant_json(&st.db, account_id, id).await?))
}

async fn assistants_avatar_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    ensure_assistant(&st.db, account_id, id).await?;
    sqlx::query(
        "DELETE FROM active_storage_attachments WHERE record_type = 'Captain::Assistant' AND record_id = $1 AND name = 'avatar'",
    )
    .bind(id)
    .execute(&st.db)
    .await?;
    Ok(Json(assistant_json(&st.db, account_id, id).await?))
}

async fn assistants_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query("DELETE FROM captain_assistants WHERE id = $1 AND account_id = $2")
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

#[derive(Deserialize)]
struct AssistantToolQuery {
    assistant_id: i64,
}

async fn assistants_tools(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<AssistantToolQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    ensure_assistant(&st.db, account_id, q.assistant_id).await?;

    let builtin = vec![
        json!({ "id": "handoff", "title": "Handoff to Human Agent", "description": "Transfers the conversation to a human agent.", "type": "builtin" }),
        json!({ "id": "resolve", "title": "Resolve Conversation", "description": "Marks the conversation as resolved.", "type": "builtin" }),
        json!({ "id": "add_private_note", "title": "Add Private Note", "description": "Adds an internal note for agents.", "type": "builtin" }),
        json!({ "id": "update_contact", "title": "Update Contact", "description": "Updates contact attributes.", "type": "builtin" }),
    ];

    let custom_rows = sqlx::query(
        "SELECT slug, title, description FROM captain_custom_tools WHERE account_id = $1 AND assistant_id = $2 AND enabled = true ORDER BY id DESC",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .fetch_all(&st.db)
    .await?;
    let mut payload = builtin;
    for r in custom_rows {
        payload.push(json!({
            "id": format!("custom_{}", r.try_get::<String, _>("slug")?),
            "title": r.try_get::<String, _>("title")?,
            "description": r.try_get::<Option<String>, _>("description")?,
            "type": "custom",
        }));
    }

    let mcp_rows = sqlx::query(
        "SELECT t.captain_name, t.name, COALESCE(t.approved_definition->>'description', '') AS description
         FROM captain_mcp_tools t
         JOIN captain_mcp_servers s ON s.id = t.mcp_server_id
         WHERE t.account_id = $1 AND s.assistant_id = $2 AND t.enabled = true AND s.enabled = true",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .fetch_all(&st.db)
    .await?;
    for r in mcp_rows {
        payload.push(json!({
            "id": r.try_get::<String, _>("captain_name")?,
            "title": r.try_get::<String, _>("name")?,
            "description": r.try_get::<String, _>("description")?,
            "type": "mcp",
        }));
    }

    Ok(Json(json!({ "payload": payload })))
}

async fn assistants_playground(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    require_account(&st.db, &user, account_id).await?;
    let assistant = assistant_json(&st.db, account_id, id).await?;
    let msg_content = sopt(&body, "message_content")
        .or_else(|| sopt(&body, "message"))
        .ok_or_else(|| ApiError::bad("message_content is required"))?;
    let request_id = sopt(&body, "request_id").unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let name = assistant["name"].as_str().unwrap_or("Captain");
    let desc = assistant["description"].as_str().unwrap_or("");
    let faqs: Vec<(String, String)> = sqlx::query_as(
        "SELECT question, answer FROM captain_assistant_responses
         WHERE account_id = $1 AND assistant_id = $2 AND status = 1
         ORDER BY updated_at DESC LIMIT 12",
    )
    .bind(account_id)
    .bind(id)
    .fetch_all(&st.db)
    .await
    .unwrap_or_default();

    let faq_ctx = faqs
        .iter()
        .map(|(q, a)| format!("Q: {q}\nA: {a}"))
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut messages = vec![json!({
        "role": "system",
        "content": format!(
            "You are {name}, a helpful support assistant. {desc}\n\nKnowledge base:\n{faq_ctx}"
        ),
    })];
    if let Some(hist) = body.get("message_history").and_then(Value::as_array) {
        for item in hist {
            let role = item["role"]
                .as_str()
                .or_else(|| item["sender"].as_str())
                .unwrap_or("user");
            let norm_role = if role == "assistant" || role == "agent" || role == "bot" {
                "assistant"
            } else {
                "user"
            };
            if let Some(c) = item["content"].as_str() {
                messages.push(json!({ "role": norm_role, "content": c }));
            }
        }
    }
    messages.push(json!({ "role": "user", "content": msg_content }));

    let model_override = resolve_llm_model_for_feature(&st.db, account_id, "assistant").await;
    let reply = match resolve_llm_config(&st.db, model_override.as_deref()).await {
        Ok(cfg) => call_llm_chat(&cfg, messages, 0.3).await?.0,
        Err(e) => return Err(e),
    };

    // Broadcast so ActionCable listeners also receive the playground completion.
    st.hub.to_account(
        account_id,
        "captain.playground.response",
        json!({
            "request_id": request_id,
            "assistant_id": id,
            "user_id": user.id,
            "message": reply,
        }),
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "request_id": request_id,
            "message": reply,
        })),
    ))
}

#[derive(Deserialize, Default)]
struct StatsQuery {
    range: Option<String>,
    #[allow(dead_code)]
    timezone_offset: Option<f64>,
    metric: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

fn range_days(range: Option<&str>) -> i64 {
    match range.unwrap_or("7d") {
        "24h" | "1d" => 1,
        "30d" => 30,
        "90d" => 90,
        "365d" | "1y" => 365,
        _ => 7,
    }
}

async fn assistant_stats_payload(
    db: &PgPool,
    account_id: i64,
    id: i64,
    days: i64,
) -> Result<Value, ApiError> {
    ensure_assistant(db, account_id, id).await?;

    let sessions_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_sessions
         WHERE account_id = $1 AND assistant_id = $2
           AND created_at >= now() - ($3::bigint || ' days')::interval",
    )
    .bind(account_id)
    .bind(id)
    .bind(days)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let conversations_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT subject_id) FROM agent_sessions
         WHERE account_id = $1 AND assistant_id = $2 AND subject_type = 'Conversation'
           AND created_at >= now() - ($3::bigint || ' days')::interval",
    )
    .bind(account_id)
    .bind(id)
    .bind(days)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let faq_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_assistant_responses WHERE account_id = $1 AND assistant_id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let approved_faqs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_assistant_responses WHERE account_id = $1 AND assistant_id = $2 AND status = 1",
    )
    .bind(account_id)
    .bind(id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let docs_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_documents WHERE account_id = $1 AND assistant_id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let pending_suggestions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_faq_suggestions WHERE account_id = $1 AND assistant_id = $2 AND status = 0",
    )
    .bind(account_id)
    .bind(id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let resolved_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT s.subject_id)
         FROM agent_sessions s
         JOIN conversations c ON c.id = s.subject_id AND c.account_id = s.account_id
         WHERE s.account_id = $1 AND s.assistant_id = $2 AND s.subject_type = 'Conversation'
           AND c.status = 1
           AND s.created_at >= now() - ($3::bigint || ' days')::interval",
    )
    .bind(account_id)
    .bind(id)
    .bind(days)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let resolution_rate = if conversations_count > 0 {
        (resolved_count as f64 / conversations_count as f64) * 100.0
    } else {
        0.0
    };

    Ok(json!({
        "total_conversations": conversations_count,
        "resolved_conversations": resolved_count,
        "handoff_conversations": (conversations_count - resolved_count).max(0),
        "resolution_rate": resolution_rate,
        "responses_sent": sessions_count,
        "faq_count": faq_count,
        "approved_faqs_count": approved_faqs,
        "documents_count": docs_count,
        "pending_suggestions_count": pending_suggestions,
    }))
}

async fn assistants_metrics(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<StatsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(
        assistant_stats_payload(&st.db, account_id, id, range_days(q.range.as_deref())).await?,
    ))
}

async fn assistants_faq_stats(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    ensure_assistant(&st.db, account_id, id).await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_assistant_responses WHERE account_id = $1 AND assistant_id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);
    let approved: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_assistant_responses WHERE account_id = $1 AND assistant_id = $2 AND status = 1",
    )
    .bind(account_id)
    .bind(id)
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_faq_suggestions WHERE account_id = $1 AND assistant_id = $2 AND status = 0",
    )
    .bind(account_id)
    .bind(id)
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);

    Ok(Json(json!({
        "total_faqs": total,
        "approved_faqs": approved,
        "pending_faqs": total - approved,
        "pending_suggestions": pending,
    })))
}

async fn assistants_summary(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<StatsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let stats =
        assistant_stats_payload(&st.db, account_id, id, range_days(q.range.as_deref())).await?;
    let msg = format!(
        "Handled {} conversations with {} resolved ({:.1}% resolution rate) and {} approved FAQs.",
        stats["total_conversations"].as_i64().unwrap_or(0),
        stats["resolved_conversations"].as_i64().unwrap_or(0),
        stats["resolution_rate"].as_f64().unwrap_or(0.0),
        stats["approved_faqs_count"].as_i64().unwrap_or(0),
    );
    Ok(Json(json!({ "message": msg })))
}

async fn assistants_drilldown(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<StatsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    ensure_assistant(&st.db, account_id, id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let rows = sqlx::query(
        "SELECT DISTINCT c.id::bigint AS id, c.display_id::bigint AS display_id, c.status, c.created_at
         FROM agent_sessions s
         JOIN conversations c ON c.id = s.subject_id AND c.account_id = s.account_id
         WHERE s.account_id = $1 AND s.assistant_id = $2 AND s.subject_type = 'Conversation'
         ORDER BY c.id DESC LIMIT $3 OFFSET $4",
    )
    .bind(account_id)
    .bind(id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("display_id").ok()?,
                "conversation_id": r.try_get::<i64, _>("id").ok()?,
                "status": r.try_get::<i32, _>("status").ok()?,
            }))
        })
        .collect();

    Ok(Json(json!({
        "metric": q.metric.unwrap_or_else(|| "conversations".into()),
        "payload": payload,
        "meta": { "page": page, "per_page": per_page },
    })))
}

async fn stats_drilldown(
    st: State<AppState>,
    user: CurrentUser,
    path: Path<(i64, i64)>,
    q: Query<StatsQuery>,
) -> ApiResult {
    assistants_drilldown(st, user, path, q).await
}

async fn stats_overview(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<StatsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let mut stats =
        assistant_stats_payload(&st.db, account_id, id, range_days(q.range.as_deref())).await?;
    if let Some(obj) = stats.as_object_mut() {
        obj.insert("tracking_started_at".into(), json!(null));
    }
    Ok(Json(stats))
}

async fn stats_overview_summary(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<StatsQuery>,
) -> ApiResult {
    assistants_summary(State(st), user, Path((account_id, id)), Query(q)).await
}

async fn stats_resolution_flow(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<StatsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let stats =
        assistant_stats_payload(&st.db, account_id, id, range_days(q.range.as_deref())).await?;
    let total = stats["total_conversations"].as_i64().unwrap_or(0);
    let resolved = stats["resolved_conversations"].as_i64().unwrap_or(0);
    let handoff = (total - resolved).max(0);
    Ok(Json(json!({
        "total": total,
        "resolved_by_captain": resolved,
        "handed_off": handoff,
    })))
}

async fn stats_resolution_trend(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Query(q): Query<StatsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    ensure_assistant(&st.db, account_id, id).await?;
    let days = range_days(q.range.as_deref());
    let rows = sqlx::query(
        "SELECT date_trunc('day', s.created_at)::date::text AS day,
                COUNT(DISTINCT s.subject_id) AS total,
                COUNT(DISTINCT s.subject_id) FILTER (WHERE c.status = 1) AS resolved
         FROM agent_sessions s
         LEFT JOIN conversations c ON c.id = s.subject_id AND c.account_id = s.account_id
         WHERE s.account_id = $1 AND s.assistant_id = $2
           AND s.created_at >= now() - ($3::bigint || ' days')::interval
         GROUP BY 1 ORDER BY 1 ASC",
    )
    .bind(account_id)
    .bind(id)
    .bind(days)
    .fetch_all(&st.db)
    .await?;

    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "date": r.try_get::<String, _>("day").ok()?,
                "total": r.try_get::<i64, _>("total").ok()?,
                "resolved": r.try_get::<i64, _>("resolved").ok()?,
            }))
        })
        .collect();
    Ok(Json(json!({ "payload": payload })))
}

// ============================================================================
// Assistant Inboxes
// ============================================================================

async fn inboxes_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, assistant_id)): Path<(i64, i64)>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    ensure_assistant(&st.db, account_id, assistant_id).await?;
    let rows = sqlx::query(
        "SELECT ci.inbox_id FROM captain_inboxes ci
         JOIN inboxes i ON i.id = ci.inbox_id
         WHERE ci.captain_assistant_id = $1 AND i.account_id = $2
         ORDER BY ci.id DESC",
    )
    .bind(assistant_id)
    .bind(account_id)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::with_capacity(rows.len());
    for r in rows {
        let inbox_id: i64 = r.try_get("inbox_id")?;
        if let Ok(ij) =
            crate::inboxes::inbox_json(&st.db, account_id, inbox_id, au.is_admin()).await
        {
            payload.push(ij);
        }
    }
    Ok(Json(json!({
        "payload": payload,
        "meta": { "total_count": payload.len(), "page": 1 },
    })))
}

async fn inboxes_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, assistant_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;
    ensure_assistant(&st.db, account_id, assistant_id).await?;
    let inbox_id = i64_param(pick(&body, "inbox"), "inbox_id")
        .or_else(|| i64_param(&body, "inbox_id"))
        .ok_or_else(|| ApiError::bad("inbox_id is required"))?;

    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id::bigint FROM inboxes WHERE id = $1 AND account_id = $2")
            .bind(inbox_id)
            .bind(account_id)
            .fetch_optional(&st.db)
            .await?;
    if exists.is_none() {
        return Err(ApiError::not_found());
    }

    sqlx::query(
        "INSERT INTO captain_inboxes (captain_assistant_id, inbox_id, created_at, updated_at)
         VALUES ($1, $2, now(), now())
         ON CONFLICT (captain_assistant_id, inbox_id) DO NOTHING",
    )
    .bind(assistant_id)
    .bind(inbox_id)
    .execute(&st.db)
    .await?;

    Ok(Json(
        crate::inboxes::inbox_json(&st.db, account_id, inbox_id, au.is_admin()).await?,
    ))
}

async fn inboxes_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, assistant_id, inbox_id)): Path<(i64, i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    ensure_assistant(&st.db, account_id, assistant_id).await?;
    sqlx::query("DELETE FROM captain_inboxes WHERE captain_assistant_id = $1 AND inbox_id = $2")
        .bind(assistant_id)
        .bind(inbox_id)
        .execute(&st.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Scenarios
// ============================================================================

async fn scenario_json(
    db: &PgPool,
    account_id: i64,
    assistant_id: i64,
    id: i64,
) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT id, title, description, instruction, tools, enabled, assistant_id, account_id,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at
         FROM captain_scenarios WHERE id = $1 AND assistant_id = $2 AND account_id = $3",
    )
    .bind(id)
    .bind(assistant_id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "title": r.try_get::<Option<String>, _>("title")?,
        "description": r.try_get::<Option<String>, _>("description")?,
        "instruction": r.try_get::<Option<String>, _>("instruction")?,
        "tools": r.try_get::<Value, _>("tools")?,
        "enabled": r.try_get::<bool, _>("enabled")?,
        "assistant_id": r.try_get::<i64, _>("assistant_id")?,
        "account_id": r.try_get::<i64, _>("account_id")?,
        "created_at": r.try_get::<i64, _>("created_at")?,
        "updated_at": r.try_get::<i64, _>("updated_at")?,
    }))
}

async fn scenarios_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, assistant_id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    ensure_assistant(&st.db, account_id, assistant_id).await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM captain_scenarios WHERE account_id = $1 AND assistant_id = $2 ORDER BY id DESC",
    )
    .bind(account_id)
    .bind(assistant_id)
    .fetch_all(&st.db)
    .await?;
    let mut payload = Vec::with_capacity(ids.len());
    for id in &ids {
        payload.push(scenario_json(&st.db, account_id, assistant_id, *id).await?);
    }
    Ok(Json(json!({
        "payload": payload,
        "meta": { "total_count": ids.len(), "page": 1 }
    })))
}

async fn scenarios_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, assistant_id, id)): Path<(i64, i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(
        scenario_json(&st.db, account_id, assistant_id, id).await?,
    ))
}

async fn scenarios_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, assistant_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    ensure_assistant(&st.db, account_id, assistant_id).await?;
    let b = pick(&body, "scenario");
    let title = sopt(b, "title").ok_or_else(|| ApiError::bad("Title can't be blank"))?;
    let description = sopt(b, "description");
    let instruction = sopt(b, "instruction");
    let tools = b
        .get("tools")
        .filter(|x| x.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));
    let enabled = b.get("enabled").and_then(Value::as_bool).unwrap_or(true);

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO captain_scenarios (title, description, instruction, tools, enabled, assistant_id, account_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now()) RETURNING id",
    )
    .bind(&title)
    .bind(&description)
    .bind(&instruction)
    .bind(tools)
    .bind(enabled)
    .bind(assistant_id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(
        scenario_json(&st.db, account_id, assistant_id, id).await?,
    ))
}

async fn scenarios_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, assistant_id, id)): Path<(i64, i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "scenario");
    let cur = scenario_json(&st.db, account_id, assistant_id, id).await?;

    let title = sopt(b, "title").unwrap_or_else(|| cur["title"].as_str().unwrap_or("").into());
    let description = if b.get("description").is_some() {
        sopt(b, "description")
    } else {
        cur["description"].as_str().map(str::to_string)
    };
    let instruction = if b.get("instruction").is_some() {
        sopt(b, "instruction")
    } else {
        cur["instruction"].as_str().map(str::to_string)
    };
    let tools = b
        .get("tools")
        .filter(|x| x.is_array())
        .cloned()
        .unwrap_or_else(|| cur["tools"].clone());
    let enabled = b
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| cur["enabled"].as_bool().unwrap_or(true));

    sqlx::query(
        "UPDATE captain_scenarios SET title = $1, description = $2, instruction = $3, tools = $4, enabled = $5, updated_at = now()
         WHERE id = $6 AND assistant_id = $7 AND account_id = $8",
    )
    .bind(&title)
    .bind(&description)
    .bind(&instruction)
    .bind(tools)
    .bind(enabled)
    .bind(id)
    .bind(assistant_id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    Ok(Json(
        scenario_json(&st.db, account_id, assistant_id, id).await?,
    ))
}

async fn scenarios_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, assistant_id, id)): Path<(i64, i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query(
        "DELETE FROM captain_scenarios WHERE id = $1 AND assistant_id = $2 AND account_id = $3",
    )
    .bind(id)
    .bind(assistant_id)
    .bind(account_id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Documents
// ============================================================================

async fn document_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT d.id, d.name, d.external_link, d.content, d.assistant_id, d.account_id,
                d.status, d.metadata, d.sync_status,
                EXTRACT(EPOCH FROM d.last_synced_at)::bigint AS last_synced_at,
                EXTRACT(EPOCH FROM d.created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM d.updated_at)::bigint AS updated_at,
                a.name AS assistant_name,
                (SELECT COUNT(*) FROM captain_assistant_responses ar WHERE ar.documentable_type = 'Captain::Document' AND ar.documentable_id = d.id) AS responses_count
         FROM captain_documents d
         JOIN captain_assistants a ON a.id = d.assistant_id
         WHERE d.id = $1 AND d.account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let status_int = r.try_get::<i32, _>("status")?;
    let status_str = if status_int == 1 {
        "available"
    } else {
        "in_progress"
    };
    let sync_int: Option<i32> = r.try_get("sync_status")?;
    let sync_str = match sync_int {
        Some(0) => Some("syncing"),
        Some(1) => Some("synced"),
        Some(2) => Some("failed"),
        _ => None,
    };

    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "name": r.try_get::<Option<String>, _>("name")?,
        "external_link": r.try_get::<String, _>("external_link")?,
        "content": r.try_get::<Option<String>, _>("content")?,
        "assistant_id": r.try_get::<i64, _>("assistant_id")?,
        "assistant": { "id": r.try_get::<i64, _>("assistant_id")?, "name": r.try_get::<String, _>("assistant_name")? },
        "account_id": r.try_get::<i64, _>("account_id")?,
        "status": status_str,
        "sync_status": sync_str,
        "metadata": r.try_get::<Value, _>("metadata")?,
        "responses_count": r.try_get::<i64, _>("responses_count")?,
        "last_synced_at": r.try_get::<Option<i64>, _>("last_synced_at")?,
        "created_at": r.try_get::<i64, _>("created_at")?,
        "updated_at": r.try_get::<i64, _>("updated_at")?,
    }))
}

#[derive(Deserialize, Default)]
struct DocsQuery {
    assistant_id: Option<i64>,
    page: Option<i64>,
    search_key: Option<String>,
}

async fn documents_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<DocsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;
    let search = q
        .search_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_documents
         WHERE account_id = $1
           AND ($2::bigint IS NULL OR assistant_id = $2)
           AND ($3::text IS NULL OR name ILIKE $3 OR external_link ILIKE $3)",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .bind(&search)
    .fetch_one(&st.db)
    .await?;

    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM captain_documents
         WHERE account_id = $1
           AND ($2::bigint IS NULL OR assistant_id = $2)
           AND ($3::text IS NULL OR name ILIKE $3 OR external_link ILIKE $3)
         ORDER BY id DESC LIMIT $4 OFFSET $5",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .bind(&search)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::with_capacity(ids.len());
    for id in &ids {
        payload.push(document_json(&st.db, account_id, *id).await?);
    }
    Ok(Json(json!({
        "payload": payload,
        "meta": { "total_count": total, "page": page }
    })))
}

async fn documents_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(document_json(&st.db, account_id, id).await?))
}

async fn fetch_doc_content(url: &str) -> Option<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return None;
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let raw = resp.bytes().await.ok()?;
    let text = String::from_utf8_lossy(&raw).to_string();
    Some(text.chars().take(100_000).collect())
}

async fn documents_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "document");
    let assistant_id = i64_param(b, "assistant_id")
        .or_else(|| i64_param(&body, "assistant_id"))
        .ok_or_else(|| ApiError::bad("Missing Assistant"))?;
    ensure_assistant(&st.db, account_id, assistant_id).await?;

    let external_link =
        sopt(b, "external_link").ok_or_else(|| ApiError::bad("External link can't be blank"))?;
    let name = sopt(b, "name").or_else(|| Some(external_link.clone()));
    let mut content = sopt(b, "content");
    if content.is_none() {
        content = fetch_doc_content(&external_link).await;
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO captain_documents
            (name, external_link, content, assistant_id, account_id, status, sync_status, last_synced_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 1, 1, now(), now(), now())
         ON CONFLICT (assistant_id, md5(external_link)) DO UPDATE
            SET name = EXCLUDED.name, content = COALESCE(EXCLUDED.content, captain_documents.content),
                status = 1, sync_status = 1, last_synced_at = now(), updated_at = now()
         RETURNING id",
    )
    .bind(&name)
    .bind(&external_link)
    .bind(&content)
    .bind(assistant_id)
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(document_json(&st.db, account_id, id).await?))
}

async fn documents_sync(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let doc = document_json(&st.db, account_id, id).await?;
    let url = doc["external_link"].as_str().unwrap_or("");
    let fetched = fetch_doc_content(url).await;

    sqlx::query(
        "UPDATE captain_documents
         SET content = COALESCE($1, content), sync_status = 1, status = 1,
             last_synced_at = now(), last_sync_attempted_at = now(), updated_at = now()
         WHERE id = $2 AND account_id = $3",
    )
    .bind(&fetched)
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    Ok(StatusCode::ACCEPTED)
}

async fn documents_drilldown(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    document_json(&st.db, account_id, id).await?;
    let rows = sqlx::query(
        "SELECT id, display_id::bigint AS display_id, status FROM conversations
         WHERE account_id = $1 AND id IN (
             SELECT subject_id FROM agent_sessions
             WHERE account_id = $1 AND subject_type = 'Conversation' AND document_ids @> $2::jsonb
         ) ORDER BY id DESC LIMIT 25",
    )
    .bind(account_id)
    .bind(json!([id]))
    .fetch_all(&st.db)
    .await
    .unwrap_or_default();
    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("display_id").ok()?,
                "conversation_id": r.try_get::<i64, _>("id").ok()?,
                "status": r.try_get::<i32, _>("status").ok()?,
            }))
        })
        .collect();
    Ok(Json(
        json!({ "payload": payload, "meta": { "total_count": payload.len() } }),
    ))
}

async fn documents_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query("DELETE FROM captain_documents WHERE id = $1 AND account_id = $2")
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

// ============================================================================
// Assistant Responses (FAQs) & FAQ Suggestions
// ============================================================================

async fn response_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT ar.id, ar.question, ar.answer, ar.assistant_id, ar.account_id, ar.status,
                ar.documentable_type, ar.documentable_id, ar.edited,
                EXTRACT(EPOCH FROM ar.created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM ar.updated_at)::bigint AS updated_at,
                a.name AS assistant_name
         FROM captain_assistant_responses ar
         JOIN captain_assistants a ON a.id = ar.assistant_id
         WHERE ar.id = $1 AND ar.account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let status_int = r.try_get::<i32, _>("status")?;
    let status = if status_int == 1 {
        "approved"
    } else {
        "pending"
    };
    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "question": r.try_get::<String, _>("question")?,
        "answer": r.try_get::<String, _>("answer")?,
        "assistant_id": r.try_get::<i64, _>("assistant_id")?,
        "assistant": { "id": r.try_get::<i64, _>("assistant_id")?, "name": r.try_get::<String, _>("assistant_name")? },
        "account_id": r.try_get::<i64, _>("account_id")?,
        "status": status,
        "documentable_type": r.try_get::<Option<String>, _>("documentable_type")?,
        "documentable_id": r.try_get::<Option<i64>, _>("documentable_id")?,
        "edited": r.try_get::<bool, _>("edited")?,
        "created_at": r.try_get::<i64, _>("created_at")?,
        "updated_at": r.try_get::<i64, _>("updated_at")?,
    }))
}

#[derive(Deserialize, Default)]
struct ResponsesQuery {
    assistant_id: Option<i64>,
    document_id: Option<i64>,
    status: Option<String>,
    search: Option<String>,
    page: Option<i64>,
}

async fn responses_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<ResponsesQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;
    let status_filter: Option<i32> = match q.status.as_deref() {
        Some("pending") | Some("0") => Some(0),
        Some("approved") | Some("1") => Some(1),
        _ => None,
    };
    let search = q
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_assistant_responses
         WHERE account_id = $1
           AND ($2::bigint IS NULL OR assistant_id = $2)
           AND ($3::bigint IS NULL OR (documentable_type = 'Captain::Document' AND documentable_id = $3))
           AND ($4::int IS NULL OR status = $4)
           AND ($5::text IS NULL OR question ILIKE $5 OR answer ILIKE $5)",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .bind(q.document_id)
    .bind(status_filter)
    .bind(&search)
    .fetch_one(&st.db)
    .await?;

    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM captain_assistant_responses
         WHERE account_id = $1
           AND ($2::bigint IS NULL OR assistant_id = $2)
           AND ($3::bigint IS NULL OR (documentable_type = 'Captain::Document' AND documentable_id = $3))
           AND ($4::int IS NULL OR status = $4)
           AND ($5::text IS NULL OR question ILIKE $5 OR answer ILIKE $5)
         ORDER BY id DESC LIMIT $6 OFFSET $7",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .bind(q.document_id)
    .bind(status_filter)
    .bind(&search)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::with_capacity(ids.len());
    for id in &ids {
        payload.push(response_json(&st.db, account_id, *id).await?);
    }
    Ok(Json(json!({
        "payload": payload,
        "meta": { "total_count": total, "page": page }
    })))
}

async fn responses_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(response_json(&st.db, account_id, id).await?))
}

async fn responses_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "assistant_response");
    let assistant_id = i64_param(b, "assistant_id")
        .or_else(|| i64_param(&body, "assistant_id"))
        .ok_or_else(|| ApiError::bad("assistant_id is required"))?;
    ensure_assistant(&st.db, account_id, assistant_id).await?;
    let question = sopt(b, "question").ok_or_else(|| ApiError::bad("Question can't be blank"))?;
    let answer = sopt(b, "answer").ok_or_else(|| ApiError::bad("Answer can't be blank"))?;
    let status = match b.get("status").and_then(Value::as_str) {
        Some("pending") => 0i32,
        _ => 1i32,
    };

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO captain_assistant_responses
            (question, answer, assistant_id, documentable_id, documentable_type, account_id, status, edited, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'User', $5, $6, false, now(), now())
         RETURNING id",
    )
    .bind(&question)
    .bind(&answer)
    .bind(assistant_id)
    .bind(user.id)
    .bind(account_id)
    .bind(status)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(response_json(&st.db, account_id, id).await?))
}

async fn responses_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "assistant_response");
    let cur = response_json(&st.db, account_id, id).await?;

    let question =
        sopt(b, "question").unwrap_or_else(|| cur["question"].as_str().unwrap_or("").into());
    let answer = sopt(b, "answer").unwrap_or_else(|| cur["answer"].as_str().unwrap_or("").into());
    let status = match b.get("status").and_then(Value::as_str) {
        Some("pending") => 0i32,
        Some("approved") => 1i32,
        _ => {
            if cur["status"].as_str() == Some("approved") {
                1
            } else {
                0
            }
        }
    };

    sqlx::query(
        "UPDATE captain_assistant_responses
         SET question = $1, answer = $2, status = $3, edited = true, updated_at = now()
         WHERE id = $4 AND account_id = $5",
    )
    .bind(&question)
    .bind(&answer)
    .bind(status)
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    Ok(Json(response_json(&st.db, account_id, id).await?))
}

async fn responses_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n =
        sqlx::query("DELETE FROM captain_assistant_responses WHERE id = $1 AND account_id = $2")
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

async fn responses_drilldown(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    response_json(&st.db, account_id, id).await?;
    let rows = sqlx::query(
        "SELECT id, display_id::bigint AS display_id, status FROM conversations
         WHERE account_id = $1 AND id IN (
             SELECT subject_id FROM agent_sessions
             WHERE account_id = $1 AND subject_type = 'Conversation' AND used_faq_ids @> $2::jsonb
         ) ORDER BY id DESC LIMIT 25",
    )
    .bind(account_id)
    .bind(json!([id]))
    .fetch_all(&st.db)
    .await
    .unwrap_or_default();
    let payload: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("display_id").ok()?,
                "conversation_id": r.try_get::<i64, _>("id").ok()?,
                "status": r.try_get::<i32, _>("status").ok()?,
            }))
        })
        .collect();
    Ok(Json(
        json!({ "payload": payload, "meta": { "total_count": payload.len() } }),
    ))
}

async fn suggestion_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT s.id, s.question, s.answer, s.assistant_id, s.account_id, s.language, s.source_count, s.status,
                EXTRACT(EPOCH FROM s.created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM s.updated_at)::bigint AS updated_at,
                a.name AS assistant_name
         FROM captain_faq_suggestions s
         JOIN captain_assistants a ON a.id = s.assistant_id
         WHERE s.id = $1 AND s.account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let status = match r.try_get::<i32, _>("status")? {
        1 => "approved",
        2 => "dismissed",
        _ => "open",
    };
    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "question": r.try_get::<String, _>("question")?,
        "answer": r.try_get::<String, _>("answer")?,
        "assistant_id": r.try_get::<i64, _>("assistant_id")?,
        "assistant": { "id": r.try_get::<i64, _>("assistant_id")?, "name": r.try_get::<String, _>("assistant_name")? },
        "account_id": r.try_get::<i64, _>("account_id")?,
        "language": r.try_get::<String, _>("language")?,
        "source_count": r.try_get::<i32, _>("source_count")?,
        "status": status,
        "created_at": r.try_get::<i64, _>("created_at")?,
        "updated_at": r.try_get::<i64, _>("updated_at")?,
    }))
}

#[derive(Deserialize, Default)]
struct SuggestionsQuery {
    assistant_id: Option<i64>,
    status: Option<String>,
    search: Option<String>,
    page: Option<i64>,
}

async fn faq_suggestions_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<SuggestionsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;
    let status_filter: Option<i32> = match q.status.as_deref() {
        Some("open") | Some("0") => Some(0),
        Some("approved") | Some("1") => Some(1),
        Some("dismissed") | Some("2") => Some(2),
        _ => None,
    };
    let search = q
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_faq_suggestions
         WHERE account_id = $1
           AND ($2::bigint IS NULL OR assistant_id = $2)
           AND ($3::int IS NULL OR status = $3)
           AND ($4::text IS NULL OR question ILIKE $4 OR answer ILIKE $4)",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .bind(status_filter)
    .bind(&search)
    .fetch_one(&st.db)
    .await?;

    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM captain_faq_suggestions
         WHERE account_id = $1
           AND ($2::bigint IS NULL OR assistant_id = $2)
           AND ($3::int IS NULL OR status = $3)
           AND ($4::text IS NULL OR question ILIKE $4 OR answer ILIKE $4)
         ORDER BY id DESC LIMIT $5 OFFSET $6",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .bind(status_filter)
    .bind(&search)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::with_capacity(ids.len());
    for id in &ids {
        payload.push(suggestion_json(&st.db, account_id, *id).await?);
    }
    Ok(Json(json!({
        "payload": payload,
        "meta": { "total_count": total, "page": page }
    })))
}

async fn faq_suggestions_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let mut s = suggestion_json(&st.db, account_id, id).await?;
    let obs = sqlx::query(
        "SELECT id, conversation_id, generated_question, generated_answer, language, status,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at
         FROM captain_faq_observations
         WHERE account_id = $1 AND faq_suggestion_id = $2
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(account_id)
    .bind(id)
    .fetch_all(&st.db)
    .await
    .unwrap_or_default();

    let obs_json: Vec<Value> = obs
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("id").ok()?,
                "conversation_id": r.try_get::<i64, _>("conversation_id").ok()?,
                "generated_question": r.try_get::<String, _>("generated_question").ok()?,
                "generated_answer": r.try_get::<String, _>("generated_answer").ok()?,
                "language": r.try_get::<String, _>("language").ok()?,
                "created_at": r.try_get::<i64, _>("created_at").ok()?,
            }))
        })
        .collect();
    if let Some(obj) = s.as_object_mut() {
        obj.insert("observations".into(), Value::Array(obs_json));
    }
    Ok(Json(s))
}

async fn faq_suggestions_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "faq_suggestion");
    let cur = suggestion_json(&st.db, account_id, id).await?;
    if cur["status"].as_str() != Some("open") {
        return Err(ApiError::not_found());
    }
    let question =
        sopt(b, "question").unwrap_or_else(|| cur["question"].as_str().unwrap_or("").into());
    let answer = sopt(b, "answer").unwrap_or_else(|| cur["answer"].as_str().unwrap_or("").into());

    sqlx::query(
        "UPDATE captain_faq_suggestions SET question = $1, answer = $2, updated_at = now()
         WHERE id = $3 AND account_id = $4 AND status = 0",
    )
    .bind(&question)
    .bind(&answer)
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    Ok(Json(suggestion_json(&st.db, account_id, id).await?))
}

async fn faq_suggestions_approve(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    body: Option<Json<Value>>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let cur = suggestion_json(&st.db, account_id, id).await?;
    if cur["status"].as_str() != Some("open") {
        return Err(ApiError::bad("FAQ suggestion already processed"));
    }
    let b = body
        .as_ref()
        .map(|j| pick(&j.0, "faq_suggestion").clone())
        .unwrap_or_else(|| json!({}));
    let question =
        sopt(&b, "question").unwrap_or_else(|| cur["question"].as_str().unwrap_or("").into());
    let answer = sopt(&b, "answer").unwrap_or_else(|| cur["answer"].as_str().unwrap_or("").into());
    let assistant_id = cur["assistant_id"].as_i64().unwrap_or(0);

    let mut tx = st.db.begin().await?;
    sqlx::query(
        "UPDATE captain_faq_suggestions SET question = $1, answer = $2, status = 1, updated_at = now()
         WHERE id = $3 AND account_id = $4",
    )
    .bind(&question)
    .bind(&answer)
    .bind(id)
    .bind(account_id)
    .execute(&mut *tx)
    .await?;

    let resp_id: i64 = sqlx::query_scalar(
        "INSERT INTO captain_assistant_responses
            (question, answer, assistant_id, documentable_id, documentable_type, account_id, status, edited, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'User', $5, 1, false, now(), now()) RETURNING id",
    )
    .bind(&question)
    .bind(&answer)
    .bind(assistant_id)
    .bind(user.id)
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(response_json(&st.db, account_id, resp_id).await?))
}

async fn faq_suggestions_dismiss(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query(
        "UPDATE captain_faq_suggestions SET status = 2, updated_at = now()
         WHERE id = $1 AND account_id = $2 AND status = 0",
    )
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(suggestion_json(&st.db, account_id, id).await?))
}

// ============================================================================
// Bulk actions, Message reports, Agent sessions
// ============================================================================

async fn bulk_actions_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;

    let action_type = body["type"].as_str().unwrap_or("");
    let ids: Vec<i64> = body["ids"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_i64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
                .collect()
        })
        .unwrap_or_default();
    let status_cmd = body["fields"]["status"].as_str().unwrap_or("");
    if action_type.is_empty() || ids.is_empty() || status_cmd.is_empty() {
        return Err(ApiError::bad("type, ids and fields.status are required"));
    }

    match action_type {
        "AssistantResponse" => {
            if status_cmd != "delete" {
                return Err(ApiError::bad("Unsupported AssistantResponse status action"));
            }
            let n = sqlx::query(
                "DELETE FROM captain_assistant_responses WHERE account_id = $1 AND id = ANY($2)",
            )
            .bind(account_id)
            .bind(&ids)
            .execute(&st.db)
            .await?
            .rows_affected();
            Ok(Json(json!({ "count": n, "payload": [] })))
        }
        "AssistantDocument" => match status_cmd {
            "delete" => {
                let n = sqlx::query(
                    "DELETE FROM captain_documents WHERE account_id = $1 AND id = ANY($2)",
                )
                .bind(account_id)
                .bind(&ids)
                .execute(&st.db)
                .await?
                .rows_affected();
                Ok(Json(json!({ "count": n })))
            }
            "sync" => {
                sqlx::query(
                    "UPDATE captain_documents
                     SET sync_status = 1, status = 1, last_synced_at = now(), last_sync_attempted_at = now(), updated_at = now()
                     WHERE account_id = $1 AND id = ANY($2)",
                )
                .bind(account_id)
                .bind(&ids)
                .execute(&st.db)
                .await?;
                Ok(Json(json!({ "ids": ids, "count": ids.len() })))
            }
            _ => Err(ApiError::bad("Unsupported AssistantDocument status action")),
        },
        _ => Err(ApiError::bad("Invalid bulk action type")),
    }
}

async fn message_reports_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let message_id =
        i64_param(&body, "message_id").ok_or_else(|| ApiError::bad("message_id is required"))?;
    let reason =
        sopt(&body, "report_reason").ok_or_else(|| ApiError::bad("report_reason is required"))?;
    let description = sopt(&body, "description");

    let conv_id: Option<i64> = sqlx::query_scalar(
        "SELECT conversation_id::bigint FROM messages WHERE id = $1 AND account_id = $2",
    )
    .bind(message_id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?;
    let conv_id = conv_id.ok_or_else(ApiError::not_found)?;

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO captain_message_reports
            (account_id, conversation_id, message_id, user_id, report_reason, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, now(), now()) RETURNING id",
    )
    .bind(account_id)
    .bind(conv_id)
    .bind(message_id)
    .bind(user.id)
    .bind(&reason)
    .bind(&description)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(json!({
        "id": id,
        "account_id": account_id,
        "conversation_id": conv_id,
        "message_id": message_id,
        "user_id": user.id,
        "report_reason": reason,
        "description": description,
    })))
}

async fn agent_session_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, message_id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let row = sqlx::query(
        "SELECT id, assistant_id, llm_model, cited_document_ids, used_faq_ids, scenario_ids,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at
         FROM agent_sessions
         WHERE account_id = $1 AND result_type = 'Message' AND result_id = $2
         ORDER BY id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(message_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let cited_ids: Vec<i64> = row
        .try_get::<Value, _>("cited_document_ids")?
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let used_faq_ids: Vec<i64> = row
        .try_get::<Value, _>("used_faq_ids")?
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let scenario_ids: Vec<i64> = row
        .try_get::<Value, _>("scenario_ids")?
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();

    let citations: Vec<Value> = if cited_ids.is_empty() {
        vec![]
    } else {
        sqlx::query("SELECT id, name, external_link FROM captain_documents WHERE account_id = $1 AND id = ANY($2)")
            .bind(account_id)
            .bind(&cited_ids)
            .fetch_all(&st.db)
            .await?
            .into_iter()
            .filter_map(|r| {
                Some(json!({
                    "id": r.try_get::<i64, _>("id").ok()?,
                    "name": r.try_get::<Option<String>, _>("name").ok()?,
                    "external_link": r.try_get::<String, _>("external_link").ok()?,
                }))
            })
            .collect()
    };

    let used_faqs: Vec<Value> = if used_faq_ids.is_empty() {
        vec![]
    } else {
        sqlx::query("SELECT id, question, answer FROM captain_assistant_responses WHERE account_id = $1 AND id = ANY($2)")
            .bind(account_id)
            .bind(&used_faq_ids)
            .fetch_all(&st.db)
            .await?
            .into_iter()
            .filter_map(|r| {
                Some(json!({
                    "id": r.try_get::<i64, _>("id").ok()?,
                    "question": r.try_get::<String, _>("question").ok()?,
                    "answer": r.try_get::<String, _>("answer").ok()?,
                }))
            })
            .collect()
    };

    let scenarios: Vec<Value> = if scenario_ids.is_empty() {
        vec![]
    } else {
        sqlx::query(
            "SELECT id, title FROM captain_scenarios WHERE account_id = $1 AND id = ANY($2)",
        )
        .bind(account_id)
        .bind(&scenario_ids)
        .fetch_all(&st.db)
        .await?
        .into_iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<i64, _>("id").ok()?,
                "title": r.try_get::<Option<String>, _>("title").ok()?,
            }))
        })
        .collect()
    };

    Ok(Json(json!({
        "id": row.try_get::<i64, _>("id")?,
        "assistant_id": row.try_get::<i64, _>("assistant_id")?,
        "llm_model": row.try_get::<Option<String>, _>("llm_model")?,
        "citations": citations,
        "used_faqs": used_faqs,
        "scenarios": scenarios,
        "created_at": row.try_get::<i64, _>("created_at")?,
    })))
}

// ============================================================================
// Copilot Threads, Messages, Runs
// ============================================================================

async fn copilot_thread_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT id, title, user_id, account_id, assistant_id,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at
         FROM copilot_threads WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "title": r.try_get::<String, _>("title")?,
        "user_id": r.try_get::<i64, _>("user_id")?,
        "account_id": r.try_get::<i64, _>("account_id")?,
        "assistant_id": r.try_get::<Option<i32>, _>("assistant_id")?,
        "created_at": r.try_get::<i64, _>("created_at")?,
        "updated_at": r.try_get::<i64, _>("updated_at")?,
    }))
}

async fn copilot_message_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT id, copilot_thread_id, account_id, message, message_type,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at
         FROM copilot_messages WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let mtype = match r.try_get::<Option<i32>, _>("message_type")?.unwrap_or(0) {
        1 => "assistant",
        _ => "user",
    };

    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "copilot_thread_id": r.try_get::<i64, _>("copilot_thread_id")?,
        "account_id": r.try_get::<i64, _>("account_id")?,
        "message": r.try_get::<Value, _>("message")?,
        "message_type": mtype,
        "created_at": r.try_get::<i64, _>("created_at")?,
    }))
}

#[derive(Deserialize, Default)]
struct PageOnly {
    page: Option<i64>,
}

async fn copilot_threads_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<PageOnly>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * 20;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM copilot_threads WHERE account_id = $1 AND user_id = $2",
    )
    .bind(account_id)
    .bind(user.id)
    .fetch_one(&st.db)
    .await?;

    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM copilot_threads WHERE account_id = $1 AND user_id = $2
         ORDER BY created_at DESC, id DESC LIMIT 20 OFFSET $3",
    )
    .bind(account_id)
    .bind(user.id)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::with_capacity(ids.len());
    for id in &ids {
        payload.push(copilot_thread_json(&st.db, account_id, *id).await?);
    }
    Ok(Json(json!({
        "payload": payload,
        "meta": { "total_count": total, "page": page }
    })))
}

async fn generate_copilot_reply(
    db: &PgPool,
    account_id: i64,
    thread_id: i64,
    assistant_id: Option<i64>,
    prompt: &str,
    conv_display_id: Option<i64>,
) -> Result<Value, ApiError> {
    let model = resolve_llm_model_for_feature(db, account_id, "copilot").await;
    let cfg = resolve_llm_config(db, model.as_deref()).await?;

    let mut ctx = String::new();
    if let Some(cid) = conv_display_id {
        let msgs = sqlx::query(
            "SELECT m.message_type, m.content FROM messages m
             JOIN conversations c ON c.id = m.conversation_id
             WHERE c.account_id = $1 AND c.display_id = $2 AND m.content IS NOT NULL
             ORDER BY m.created_at DESC LIMIT 12",
        )
        .bind(account_id)
        .bind(cid)
        .fetch_all(db)
        .await
        .unwrap_or_default();
        for r in msgs.into_iter().rev() {
            let mt: i32 = r.try_get("message_type").unwrap_or(0);
            let role = if mt == 0 { "Contact" } else { "Agent" };
            let txt: String = r.try_get("content").unwrap_or_default();
            ctx.push_str(&format!("{role}: {txt}\n"));
        }
    }

    let faqs: Vec<(String, String)> = if let Some(aid) = assistant_id {
        sqlx::query_as(
            "SELECT question, answer FROM captain_assistant_responses
             WHERE account_id = $1 AND assistant_id = $2 AND status = 1 LIMIT 8",
        )
        .bind(account_id)
        .bind(aid)
        .fetch_all(db)
        .await
        .unwrap_or_default()
    } else {
        vec![]
    };
    let faq_block = faqs
        .iter()
        .map(|(q, a)| format!("FAQ: {q} -> {a}"))
        .collect::<Vec<_>>()
        .join("\n");

    let messages = vec![
        json!({
            "role": "system",
            "content": format!("You are Captain Copilot assisting a support agent.\n{faq_block}\n\nConversation:\n{ctx}"),
        }),
        json!({ "role": "user", "content": prompt }),
    ];
    let (reply, _) = call_llm_chat(&cfg, messages, 0.3).await?;

    let msg_id: i64 = sqlx::query_scalar(
        "INSERT INTO copilot_messages (copilot_thread_id, account_id, message, message_type, created_at, updated_at)
         VALUES ($1, $2, $3, 1, now(), now()) RETURNING id",
    )
    .bind(thread_id)
    .bind(account_id)
    .bind(json!({ "content": reply }))
    .fetch_one(db)
    .await?;

    copilot_message_json(db, account_id, msg_id).await
}

async fn copilot_threads_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let msg = sopt(&body, "message").ok_or_else(|| ApiError::bad("message is required"))?;
    let assistant_id = i64_param(&body, "assistant_id");
    if let Some(aid) = assistant_id {
        ensure_assistant(&st.db, account_id, aid).await?;
    }
    let title: String = msg.chars().take(120).collect();

    let thread_id: i64 = sqlx::query_scalar(
        "INSERT INTO copilot_threads (title, user_id, account_id, assistant_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, now(), now()) RETURNING id",
    )
    .bind(&title)
    .bind(user.id)
    .bind(account_id)
    .bind(assistant_id.map(|v| v as i32))
    .fetch_one(&st.db)
    .await?;

    let user_msg_id: i64 = sqlx::query_scalar(
        "INSERT INTO copilot_messages (copilot_thread_id, account_id, message, message_type, created_at, updated_at)
         VALUES ($1, $2, $3, 0, now(), now()) RETURNING id",
    )
    .bind(thread_id)
    .bind(account_id)
    .bind(json!({ "content": msg }))
    .fetch_one(&st.db)
    .await?;

    let mut reply_msg = None;
    if resolve_llm_config(&st.db, None).await.is_ok() {
        reply_msg = generate_copilot_reply(
            &st.db,
            account_id,
            thread_id,
            assistant_id,
            &msg,
            i64_param(&body, "conversation_id"),
        )
        .await
        .ok();
    }

    let mut thread = copilot_thread_json(&st.db, account_id, thread_id).await?;
    if let Some(obj) = thread.as_object_mut() {
        obj.insert(
            "user_message".into(),
            copilot_message_json(&st.db, account_id, user_msg_id).await?,
        );
        if let Some(r) = reply_msg {
            obj.insert("assistant_message".into(), r);
        }
    }
    Ok(Json(thread))
}

async fn copilot_threads_stop(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id).await?;
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM copilot_threads WHERE id = $1 AND account_id = $2 AND user_id = $3",
    )
    .bind(id)
    .bind(account_id)
    .bind(user.id)
    .fetch_optional(&st.db)
    .await?;
    if exists.is_none() {
        return Err(ApiError::not_found());
    }
    sqlx::query(
        "UPDATE copilot_runs SET status = 'cancelled', updated_at = now()
         WHERE copilot_thread_id = $1 AND account_id = $2 AND status IN ('queued', 'running', 'awaiting_approval')",
    )
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;
    Ok(StatusCode::OK)
}

async fn copilot_messages_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, copilot_thread_id)): Path<(i64, i64)>,
    Query(q): Query<PageOnly>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let _ = copilot_thread_json(&st.db, account_id, copilot_thread_id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * 1000;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM copilot_messages WHERE account_id = $1 AND copilot_thread_id = $2",
    )
    .bind(account_id)
    .bind(copilot_thread_id)
    .fetch_one(&st.db)
    .await?;

    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM copilot_messages WHERE account_id = $1 AND copilot_thread_id = $2
         ORDER BY created_at ASC, id ASC LIMIT 1000 OFFSET $3",
    )
    .bind(account_id)
    .bind(copilot_thread_id)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::with_capacity(ids.len());
    for id in &ids {
        payload.push(copilot_message_json(&st.db, account_id, *id).await?);
    }
    Ok(Json(json!({
        "payload": payload,
        "meta": { "total_count": total, "page": page }
    })))
}

async fn copilot_messages_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, copilot_thread_id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let thread = copilot_thread_json(&st.db, account_id, copilot_thread_id).await?;
    if thread["user_id"].as_i64() != Some(user.id) {
        return Err(ApiError::not_found());
    }
    let msg = sopt(&body, "message").ok_or_else(|| ApiError::bad("message is required"))?;

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO copilot_messages (copilot_thread_id, account_id, message, message_type, created_at, updated_at)
         VALUES ($1, $2, $3, 0, now(), now()) RETURNING id",
    )
    .bind(copilot_thread_id)
    .bind(account_id)
    .bind(json!({ "content": msg }))
    .fetch_one(&st.db)
    .await?;

    if resolve_llm_config(&st.db, None).await.is_ok() {
        let _ = generate_copilot_reply(
            &st.db,
            account_id,
            copilot_thread_id,
            thread["assistant_id"].as_i64(),
            &msg,
            i64_param(&body, "conversation_id"),
        )
        .await;
    }

    Ok(Json(copilot_message_json(&st.db, account_id, id).await?))
}

async fn copilot_runs_approve(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let row = sqlx::query(
        "UPDATE copilot_runs SET status = 'approved', updated_at = now()
         WHERE id = $1 AND account_id = $2 AND user_id = $3 AND status = 'awaiting_approval'
         RETURNING id, copilot_message_id, status",
    )
    .bind(id)
    .bind(account_id)
    .bind(user.id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(|| ApiError::bad("Approval already decided or not found"))?;

    Ok(Json(json!({
        "id": row.try_get::<i64, _>("id")?,
        "copilot_message_id": row.try_get::<Option<i64>, _>("copilot_message_id")?,
        "status": row.try_get::<String, _>("status")?,
    })))
}

async fn copilot_runs_reject(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let row = sqlx::query(
        "UPDATE copilot_runs SET status = 'rejected', updated_at = now()
         WHERE id = $1 AND account_id = $2 AND user_id = $3 AND status = 'awaiting_approval'
         RETURNING id, copilot_message_id, status",
    )
    .bind(id)
    .bind(account_id)
    .bind(user.id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(|| ApiError::bad("Approval already decided or not found"))?;

    Ok(Json(json!({
        "id": row.try_get::<i64, _>("id")?,
        "copilot_message_id": row.try_get::<Option<i64>, _>("copilot_message_id")?,
        "status": row.try_get::<String, _>("status")?,
    })))
}

// ============================================================================
// Custom Tools
// ============================================================================

async fn custom_tool_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT id, account_id, assistant_id, slug, title, description, http_method, endpoint_url,
                request_template, response_template, auth_type, param_schema, enabled, headers,
                source_metadata, settings,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at
         FROM captain_custom_tools WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "account_id": r.try_get::<i64, _>("account_id")?,
        "assistant_id": r.try_get::<Option<i64>, _>("assistant_id")?,
        "slug": r.try_get::<String, _>("slug")?,
        "title": r.try_get::<String, _>("title")?,
        "description": r.try_get::<Option<String>, _>("description")?,
        "http_method": r.try_get::<String, _>("http_method")?,
        "endpoint_url": r.try_get::<String, _>("endpoint_url")?,
        "request_template": r.try_get::<Option<String>, _>("request_template")?,
        "response_template": r.try_get::<Option<String>, _>("response_template")?,
        "auth_type": r.try_get::<Option<String>, _>("auth_type")?,
        "param_schema": r.try_get::<Value, _>("param_schema")?,
        "enabled": r.try_get::<bool, _>("enabled")?,
        "headers": r.try_get::<Value, _>("headers")?,
        "source_metadata": r.try_get::<Option<Value>, _>("source_metadata")?,
        "settings": r.try_get::<Value, _>("settings")?,
        "created_at": r.try_get::<i64, _>("created_at")?,
        "updated_at": r.try_get::<i64, _>("updated_at")?,
    }))
}

#[derive(Deserialize, Default)]
struct ToolsQuery {
    assistant_id: Option<i64>,
    page: Option<i64>,
}

async fn custom_tools_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<ToolsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captain_custom_tools
         WHERE account_id = $1 AND ($2::bigint IS NULL OR assistant_id = $2)",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .fetch_one(&st.db)
    .await?;

    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM captain_custom_tools
         WHERE account_id = $1 AND ($2::bigint IS NULL OR assistant_id = $2)
         ORDER BY id DESC LIMIT $3 OFFSET $4",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::with_capacity(ids.len());
    for id in &ids {
        payload.push(custom_tool_json(&st.db, account_id, *id).await?);
    }
    Ok(Json(json!({
        "payload": payload,
        "meta": { "total_count": total, "page": page }
    })))
}

async fn custom_tools_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(custom_tool_json(&st.db, account_id, id).await?))
}

async fn custom_tools_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "custom_tool");
    let assistant_id = i64_param(b, "assistant_id")
        .or_else(|| i64_param(&body, "assistant_id"))
        .ok_or_else(|| ApiError::bad("assistant_id is required"))?;
    ensure_assistant(&st.db, account_id, assistant_id).await?;

    let title = sopt(b, "title").ok_or_else(|| ApiError::bad("Title can't be blank"))?;
    let slug = sopt(b, "slug").unwrap_or_else(|| slugify(&title));
    let endpoint_url =
        sopt(b, "endpoint_url").ok_or_else(|| ApiError::bad("Endpoint url can't be blank"))?;
    let http_method = sopt(b, "http_method")
        .unwrap_or_else(|| "GET".into())
        .to_ascii_uppercase();
    let description = sopt(b, "description");
    let request_template = sopt(b, "request_template");
    let response_template = sopt(b, "response_template");
    let auth_type = sopt(b, "auth_type").unwrap_or_else(|| "none".into());
    let auth_config = b
        .get("auth_config")
        .filter(|x| x.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let param_schema = b
        .get("param_schema")
        .filter(|x| x.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));
    let headers = b
        .get("headers")
        .filter(|x| x.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let settings = b
        .get("settings")
        .filter(|x| x.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let enabled = b.get("enabled").and_then(Value::as_bool).unwrap_or(true);

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO captain_custom_tools
            (account_id, assistant_id, slug, title, description, http_method, endpoint_url,
             request_template, response_template, auth_type, auth_config, param_schema,
             enabled, headers, settings, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now(), now())
         RETURNING id",
    )
    .bind(account_id)
    .bind(assistant_id)
    .bind(&slug)
    .bind(&title)
    .bind(&description)
    .bind(&http_method)
    .bind(&endpoint_url)
    .bind(&request_template)
    .bind(&response_template)
    .bind(&auth_type)
    .bind(auth_config)
    .bind(param_schema)
    .bind(enabled)
    .bind(headers)
    .bind(settings)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(custom_tool_json(&st.db, account_id, id).await?))
}

async fn custom_tools_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "custom_tool");
    let cur = custom_tool_json(&st.db, account_id, id).await?;

    let title = sopt(b, "title").unwrap_or_else(|| cur["title"].as_str().unwrap_or("").into());
    let description = if b.get("description").is_some() {
        sopt(b, "description")
    } else {
        cur["description"].as_str().map(str::to_string)
    };
    let http_method = sopt(b, "http_method")
        .unwrap_or_else(|| cur["http_method"].as_str().unwrap_or("GET").into())
        .to_ascii_uppercase();
    let endpoint_url = sopt(b, "endpoint_url")
        .unwrap_or_else(|| cur["endpoint_url"].as_str().unwrap_or("").into());
    let request_template = if b.get("request_template").is_some() {
        sopt(b, "request_template")
    } else {
        cur["request_template"].as_str().map(str::to_string)
    };
    let response_template = if b.get("response_template").is_some() {
        sopt(b, "response_template")
    } else {
        cur["response_template"].as_str().map(str::to_string)
    };
    let auth_type =
        sopt(b, "auth_type").unwrap_or_else(|| cur["auth_type"].as_str().unwrap_or("none").into());
    let param_schema = b
        .get("param_schema")
        .filter(|x| x.is_array())
        .cloned()
        .unwrap_or_else(|| cur["param_schema"].clone());
    let headers = b
        .get("headers")
        .filter(|x| x.is_object())
        .cloned()
        .unwrap_or_else(|| cur["headers"].clone());
    let settings = b
        .get("settings")
        .filter(|x| x.is_object())
        .cloned()
        .unwrap_or_else(|| cur["settings"].clone());
    let enabled = b
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| cur["enabled"].as_bool().unwrap_or(true));

    sqlx::query(
        "UPDATE captain_custom_tools
         SET title = $1, description = $2, http_method = $3, endpoint_url = $4,
             request_template = $5, response_template = $6, auth_type = $7,
             param_schema = $8, headers = $9, settings = $10, enabled = $11, updated_at = now()
         WHERE id = $12 AND account_id = $13",
    )
    .bind(&title)
    .bind(&description)
    .bind(&http_method)
    .bind(&endpoint_url)
    .bind(&request_template)
    .bind(&response_template)
    .bind(&auth_type)
    .bind(param_schema)
    .bind(headers)
    .bind(settings)
    .bind(enabled)
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    Ok(Json(custom_tool_json(&st.db, account_id, id).await?))
}

async fn custom_tools_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query("DELETE FROM captain_custom_tools WHERE id = $1 AND account_id = $2")
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

async fn custom_tools_test(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "custom_tool");
    let url = sopt(b, "endpoint_url").ok_or_else(|| ApiError::bad("endpoint_url is required"))?;
    let method_str = sopt(b, "http_method")
        .unwrap_or_else(|| "GET".into())
        .to_ascii_uppercase();
    let method = reqwest::Method::from_bytes(method_str.as_bytes())
        .map_err(|_| ApiError::bad("Invalid http_method"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| ApiError::bad(format!("Client error: {e}")))?;
    let mut req = client.request(method, &url);
    if let Some(hdrs) = b.get("headers").and_then(Value::as_object) {
        for (k, v) in hdrs {
            if let Some(vs) = v.as_str() {
                req = req.header(k.as_str(), vs);
            }
        }
    }
    if let Some(auth_cfg) = b.get("auth_config").and_then(Value::as_object)
        && let Some(token) = auth_cfg.get("token").and_then(Value::as_str)
        && !token.is_empty()
    {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    if let Some(tpl) = sopt(b, "request_template") {
        req = req.header("content-type", "application/json").body(tpl);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("Custom tool request failed: {e}")))?;
    let status = resp.status().as_u16();
    let raw = resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad(format!("Custom tool read failed: {e}")))?;
    let body_text: String = String::from_utf8_lossy(&raw).chars().take(500).collect();

    Ok(Json(json!({
        "status": status,
        "body": body_text,
    })))
}

// ============================================================================
// MCP Servers & Tools Manifest
// ============================================================================

async fn mcp_server_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT id, account_id, assistant_id, url, name, auth_type, auth_header, status,
                last_error, enabled,
                EXTRACT(EPOCH FROM tools_changed_at)::bigint AS tools_changed_at,
                EXTRACT(EPOCH FROM created_at)::bigint AS created_at,
                EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at
         FROM captain_mcp_servers WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let tools_rows = sqlx::query(
        "SELECT id, name, captain_name, approved_definition, enabled
         FROM captain_mcp_tools WHERE mcp_server_id = $1 AND account_id = $2 ORDER BY id ASC",
    )
    .bind(id)
    .bind(account_id)
    .fetch_all(db)
    .await?;

    let tools: Vec<Value> = tools_rows
        .into_iter()
        .filter_map(|t| {
            Some(json!({
                "id": t.try_get::<i64, _>("id").ok()?,
                "name": t.try_get::<String, _>("name").ok()?,
                "captain_name": t.try_get::<String, _>("captain_name").ok()?,
                "approved_definition": t.try_get::<Value, _>("approved_definition").ok()?,
                "enabled": t.try_get::<bool, _>("enabled").ok()?,
            }))
        })
        .collect();

    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "account_id": r.try_get::<i64, _>("account_id")?,
        "assistant_id": r.try_get::<i64, _>("assistant_id")?,
        "url": r.try_get::<String, _>("url")?,
        "name": r.try_get::<String, _>("name")?,
        "auth_type": r.try_get::<String, _>("auth_type")?,
        "auth_header": r.try_get::<Option<String>, _>("auth_header")?,
        "status": r.try_get::<String, _>("status")?,
        "last_error": r.try_get::<Option<String>, _>("last_error")?,
        "enabled": r.try_get::<bool, _>("enabled")?,
        "tools": tools,
        "tools_changed_at": r.try_get::<Option<i64>, _>("tools_changed_at")?,
        "created_at": r.try_get::<i64, _>("created_at")?,
        "updated_at": r.try_get::<i64, _>("updated_at")?,
    }))
}

async fn mcp_discover_tools(
    url: &str,
    token: Option<&str>,
    auth_header: Option<&str>,
) -> Result<Vec<Value>, ApiError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| ApiError::bad(format!("Client error: {e}")))?;
    let body = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {},
    }))
    .map_err(|_| ApiError::bad("JSON serialize error"))?;

    let mut req = client
        .post(url)
        .header("content-type", "application/json")
        .body(body);
    if let Some(tok) = token.filter(|s| !s.is_empty()) {
        let hdr = auth_header
            .filter(|s| !s.is_empty())
            .unwrap_or("Authorization");
        let val = if hdr.eq_ignore_ascii_case("authorization") && !tok.starts_with("Bearer ") {
            format!("Bearer {tok}")
        } else {
            tok.to_string()
        };
        req = req.header(hdr, val);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("MCP server unreachable: {e}")))?;
    let status = resp.status();
    let raw = resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad(format!("MCP response error: {e}")))?;
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ApiError::bad("MCP token rejected"));
    }
    if !status.is_success() {
        return Err(ApiError::bad(format!("MCP server error: HTTP {status}")));
    }
    let parsed: Value = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
    let list = parsed["result"]["tools"]
        .as_array()
        .or_else(|| parsed["tools"].as_array())
        .cloned()
        .unwrap_or_default();
    Ok(list)
}

async fn mcp_servers_index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<ToolsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM captain_mcp_servers
         WHERE account_id = $1 AND ($2::bigint IS NULL OR assistant_id = $2)
         ORDER BY id DESC",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .fetch_all(&st.db)
    .await?;
    let mut payload = Vec::with_capacity(ids.len());
    for id in &ids {
        payload.push(mcp_server_json(&st.db, account_id, *id).await?);
    }
    Ok(Json(json!({ "payload": payload })))
}

async fn mcp_servers_show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    Ok(Json(mcp_server_json(&st.db, account_id, id).await?))
}

async fn mcp_servers_discover(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let url = sopt(&body, "url").ok_or_else(|| ApiError::bad("url is required"))?;
    let tools = mcp_discover_tools(
        &url,
        sopt(&body, "token").as_deref(),
        sopt(&body, "auth_header").as_deref(),
    )
    .await?;
    Ok(Json(json!({ "tools": tools, "auth_type": "none" })))
}

async fn mcp_servers_create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "mcp_server");
    let assistant_id = i64_param(b, "assistant_id")
        .or_else(|| i64_param(&body, "assistant_id"))
        .ok_or_else(|| ApiError::bad("assistant_id is required"))?;
    ensure_assistant(&st.db, account_id, assistant_id).await?;
    let url = sopt(b, "url").ok_or_else(|| ApiError::bad("url is required"))?;
    let name = sopt(b, "name").ok_or_else(|| ApiError::bad("name is required"))?;
    let auth_type = sopt(b, "auth_type").unwrap_or_else(|| "none".into());
    let auth_header = sopt(b, "auth_header");
    let auth_config = b
        .get("auth_config")
        .filter(|x| x.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let enabled = b.get("enabled").and_then(Value::as_bool).unwrap_or(true);

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO captain_mcp_servers
            (account_id, assistant_id, url, name, auth_type, auth_header, auth_config, enabled, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now()) RETURNING id",
    )
    .bind(account_id)
    .bind(assistant_id)
    .bind(&url)
    .bind(&name)
    .bind(&auth_type)
    .bind(&auth_header)
    .bind(auth_config)
    .bind(enabled)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(mcp_server_json(&st.db, account_id, id).await?))
}

async fn mcp_servers_update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let b = pick(&body, "mcp_server");
    let cur = mcp_server_json(&st.db, account_id, id).await?;
    let name = sopt(b, "name").unwrap_or_else(|| cur["name"].as_str().unwrap_or("").into());
    let auth_header = if b.get("auth_header").is_some() {
        sopt(b, "auth_header")
    } else {
        cur["auth_header"].as_str().map(str::to_string)
    };
    let enabled = b
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| cur["enabled"].as_bool().unwrap_or(true));

    sqlx::query(
        "UPDATE captain_mcp_servers SET name = $1, auth_header = $2, enabled = $3, updated_at = now()
         WHERE id = $4 AND account_id = $5",
    )
    .bind(&name)
    .bind(&auth_header)
    .bind(enabled)
    .bind(id)
    .bind(account_id)
    .execute(&st.db)
    .await?;

    if let Some(tools) = b.get("tools").and_then(Value::as_array) {
        for t in tools {
            let Some(tname) = t.get("name").and_then(Value::as_str) else {
                continue;
            };
            let t_enabled = t.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            let cname = format!("mcp_{id}_{}", slugify(tname));
            let digest = hex::encode(Sha256::digest(t.to_string().as_bytes()));
            sqlx::query(
                "INSERT INTO captain_mcp_tools
                    (account_id, mcp_server_id, name, captain_name, approved_definition, approved_digest, enabled, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())
                 ON CONFLICT (mcp_server_id, name) DO UPDATE
                    SET approved_definition = EXCLUDED.approved_definition,
                        approved_digest = EXCLUDED.approved_digest,
                        enabled = EXCLUDED.enabled,
                        updated_at = now()",
            )
            .bind(account_id)
            .bind(id)
            .bind(tname)
            .bind(&cname)
            .bind(t)
            .bind(digest)
            .bind(t_enabled)
            .execute(&st.db)
            .await?;
        }
    }

    Ok(Json(mcp_server_json(&st.db, account_id, id).await?))
}

async fn mcp_servers_destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let n = sqlx::query("DELETE FROM captain_mcp_servers WHERE id = $1 AND account_id = $2")
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

async fn mcp_servers_live_tools(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let r = sqlx::query(
        "SELECT url, auth_header, auth_config FROM captain_mcp_servers WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let url: String = r.try_get("url")?;
    let auth_header: Option<String> = r.try_get("auth_header")?;
    let auth_cfg: Value = r.try_get("auth_config")?;
    let tools =
        mcp_discover_tools(&url, auth_cfg["token"].as_str(), auth_header.as_deref()).await?;
    Ok(Json(json!({ "payload": tools })))
}

async fn mcp_servers_auth_url(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let srv = mcp_server_json(&st.db, account_id, id).await?;
    let url = srv["url"].as_str().unwrap_or("");
    let state = sign_oauth_claims(&json!({
        "account_id": account_id,
        "mcp_server_id": id,
        "exp": chrono::Utc::now().timestamp() + 600,
    }));
    let callback = format!(
        "{}/captain/mcp/callback",
        st.frontend_url.trim_end_matches('/')
    );
    let auth_url = {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        ser.append_pair("response_type", "code");
        ser.append_pair("state", &state);
        ser.append_pair("redirect_uri", &callback);
        format!("{url}?{}", ser.finish())
    };
    Ok(Json(json!({ "url": auth_url })))
}

#[derive(Deserialize, Default)]
struct McpCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn mcp_oauth_callback(
    State(st): State<AppState>,
    Query(q): Query<McpCallbackQuery>,
) -> Response {
    let Some(claims) = q.state.as_deref().and_then(verify_oauth_claims) else {
        return Redirect::to("/app").into_response();
    };
    let Some(account_id) = claims["account_id"].as_i64() else {
        return Redirect::to("/app").into_response();
    };
    let Some(server_id) = claims["mcp_server_id"].as_i64() else {
        return Redirect::to("/app").into_response();
    };
    let assistant_id: Option<i64> = sqlx::query_scalar(
        "SELECT assistant_id FROM captain_mcp_servers WHERE id = $1 AND account_id = $2",
    )
    .bind(server_id)
    .bind(account_id)
    .fetch_optional(&st.db)
    .await
    .ok()
    .flatten();

    let status = if q.error.is_none() && q.code.as_deref().is_some_and(|c| !c.is_empty()) {
        let _ = sqlx::query(
            "UPDATE captain_mcp_servers SET status = 'active', oauth_credentials = $1, updated_at = now()
             WHERE id = $2 AND account_id = $3",
        )
        .bind(json!({ "code": q.code }))
        .bind(server_id)
        .bind(account_id)
        .execute(&st.db)
        .await;
        "success"
    } else {
        "failed"
    };

    let dest = if let Some(aid) = assistant_id {
        format!(
            "/app/accounts/{account_id}/captain/{aid}/tools?mcp_server_id={server_id}&sign_in={status}"
        )
    } else {
        format!(
            "/app/accounts/{account_id}/captain/tools?mcp_server_id={server_id}&sign_in={status}"
        )
    };
    Redirect::to(&dest).into_response()
}

async fn manifest_installed(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<ToolsQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let rows = sqlx::query(
        "SELECT DISTINCT source_metadata->>'repository' AS repo, source_metadata->>'path' AS path, source_metadata->>'version' AS version
         FROM captain_custom_tools
         WHERE account_id = $1 AND ($2::bigint IS NULL OR assistant_id = $2)
           AND source_metadata->>'source' = 'github'",
    )
    .bind(account_id)
    .bind(q.assistant_id)
    .fetch_all(&st.db)
    .await?;

    let installed: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!([
                r.try_get::<Option<String>, _>("repo").ok().flatten(),
                r.try_get::<Option<String>, _>("path").ok().flatten(),
                r.try_get::<Option<String>, _>("version").ok().flatten(),
            ])
        })
        .collect();
    Ok(Json(json!({ "installed_toolsets": installed })))
}

async fn manifest_preview(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let source = sopt(&body, "source").ok_or_else(|| ApiError::bad("source is required"))?;
    Ok(Json(json!({
        "source": source,
        "tools": [],
        "auth_fields": [],
        "up_to_date": true,
    })))
}

async fn manifest_install(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let source = sopt(&body, "source").ok_or_else(|| ApiError::bad("source is required"))?;
    Ok(Json(json!({
        "source": source,
        "payload": [],
    })))
}

// ============================================================================
// Writing Tasks: rewrite, summarize, reply_suggestion, follow_up
// ============================================================================

async fn load_conversation_transcript(
    db: &PgPool,
    account_id: i64,
    display_id: i64,
) -> Result<(i64, String), ApiError> {
    let conv_id: Option<i64> = sqlx::query_scalar(
        "SELECT id::bigint FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(db)
    .await?;
    let conv_id = conv_id.ok_or_else(ApiError::not_found)?;
    let rows = sqlx::query(
        "SELECT message_type, content FROM messages
         WHERE conversation_id = $1 AND account_id = $2 AND content IS NOT NULL AND private = false
         ORDER BY created_at DESC LIMIT 20",
    )
    .bind(conv_id)
    .bind(account_id)
    .fetch_all(db)
    .await?;
    let mut lines = Vec::with_capacity(rows.len());
    for r in rows.into_iter().rev() {
        let mt: i32 = r.try_get("message_type").unwrap_or(0);
        let who = if mt == 0 { "Customer" } else { "Agent" };
        let c: String = r.try_get("content").unwrap_or_default();
        lines.push(format!("{who}: {c}"));
    }
    Ok((conv_id, lines.join("\n")))
}

async fn run_task(
    st: &AppState,
    user: &CurrentUser,
    account_id: i64,
    feature: &str,
    system_prompt: &str,
    user_prompt: String,
    follow_up_context: Option<Value>,
) -> ApiResult {
    require_account(&st.db, user, account_id).await?;
    let model = resolve_llm_model_for_feature(&st.db, account_id, feature).await;
    let cfg = resolve_llm_config(&st.db, model.as_deref()).await?;
    let (message, _) = call_llm_chat(
        &cfg,
        vec![
            json!({ "role": "system", "content": system_prompt }),
            json!({ "role": "user", "content": user_prompt }),
        ],
        0.3,
    )
    .await?;

    let mut out = json!({ "message": message });
    if let Some(ctx) = follow_up_context
        && let Some(obj) = out.as_object_mut()
    {
        obj.insert("follow_up_context".into(), ctx);
    }
    Ok(Json(out))
}

async fn task_rewrite(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let content = sopt(&body, "content").ok_or_else(|| ApiError::bad("content is required"))?;
    let operation = sopt(&body, "operation").unwrap_or_else(|| "improve".into());
    let prompt = format!("Operation: {operation}\n\nText:\n{content}");
    run_task(
        &st,
        &user,
        account_id,
        "editor",
        "Rewrite the agent draft cleanly without changing factual details. Return only the rewritten text.",
        prompt,
        None,
    )
    .await
}

async fn task_summarize(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let display_id = i64_param(&body, "conversation_display_id")
        .or_else(|| i64_param(&body, "conversation_id"))
        .ok_or_else(|| ApiError::bad("conversation_display_id is required"))?;
    let (_, transcript) = load_conversation_transcript(&st.db, account_id, display_id).await?;
    run_task(
        &st,
        &user,
        account_id,
        "summarize",
        "Summarize the support conversation concisely with key issues and resolution status.",
        transcript,
        None,
    )
    .await
}

async fn task_reply_suggestion(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let display_id = i64_param(&body, "conversation_display_id")
        .or_else(|| i64_param(&body, "conversation_id"))
        .ok_or_else(|| ApiError::bad("conversation_display_id is required"))?;
    let (_, transcript) = load_conversation_transcript(&st.db, account_id, display_id).await?;
    run_task(
        &st,
        &user,
        account_id,
        "reply_suggestion",
        "Draft a polite, direct support reply to the latest customer message. Return only the suggested reply.",
        transcript,
        None,
    )
    .await
}

async fn task_follow_up(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    let follow_up_context = body
        .get("follow_up_context")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let msg = sopt(&body, "message")
        .or_else(|| sopt(&body, "event_name"))
        .unwrap_or_else(|| "Refine the previous response".into());
    run_task(
        &st,
        &user,
        account_id,
        "follow_up",
        "Apply the follow-up instruction to refine the previous AI response.",
        format!("Context: {follow_up_context}\nInstruction: {msg}"),
        Some(follow_up_context),
    )
    .await
}

fn verify_firecrawl_token(assistant_id: i64, account_id: i64, token: &str) -> bool {
    if let Ok(env_secret) = std::env::var("FIRECRAWL_WEBHOOK_SECRET")
        && !env_secret.is_empty()
        && token == env_secret
    {
        return true;
    }
    let secret =
        std::env::var("SECRET_KEY_BASE").unwrap_or_else(|_| "pulsar-dev-secret-change-me".into());
    let expected = hex::encode(Sha256::digest(
        format!("{secret}:firecrawl:{assistant_id}:{account_id}").as_bytes(),
    ));
    token == expected
}

async fn firecrawl_webhook(
    State(st): State<AppState>,
    Json(body): Json<Value>,
) -> Result<StatusCode, ApiError> {
    let assistant_id = i64_param(&body, "assistant_id")
        .ok_or_else(|| ApiError::bad("assistant_id is required"))?;
    let token = sopt(&body, "token").unwrap_or_default();

    let account_id: Option<i64> =
        sqlx::query_scalar("SELECT account_id FROM captain_assistants WHERE id = $1")
            .bind(assistant_id)
            .fetch_optional(&st.db)
            .await?;
    let Some(account_id) = account_id else {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Invalid access_token".into(),
        ));
    };
    if !verify_firecrawl_token(assistant_id, account_id, &token) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Invalid access_token".into(),
        ));
    }

    if body["type"].as_str() == Some("crawl.page")
        && let Some(page) = body["data"].as_array().and_then(|a| a.first())
    {
        let link = page["metadata"]["sourceURL"]
            .as_str()
            .or_else(|| page["url"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if !link.is_empty() {
            let title = page["metadata"]["title"].as_str().unwrap_or(&link);
            let content = page["markdown"]
                .as_str()
                .or_else(|| page["content"].as_str())
                .unwrap_or("");
            sqlx::query(
                "INSERT INTO captain_documents
                    (name, external_link, content, assistant_id, account_id, status, sync_status, last_synced_at, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, 1, 1, now(), now(), now())
                 ON CONFLICT (assistant_id, md5(external_link)) DO UPDATE
                    SET name = EXCLUDED.name, content = EXCLUDED.content, status = 1, sync_status = 1,
                        last_synced_at = now(), updated_at = now()",
            )
            .bind(title)
            .bind(&link)
            .bind(content)
            .bind(assistant_id)
            .bind(account_id)
            .execute(&st.db)
            .await?;
        }
    }
    Ok(StatusCode::OK)
}
