//! Installation config (Chatwoot installation_configs), health, and the 501 fallback.
//! The dashboard itself is the React Router SPA in web/, served from main.rs.
use axum::{
    Json,
    extract::OriginalUri,
    response::{Html, IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use sqlx::PgPool;

const GLOBAL_KEYS: &[&str] = &[
    "LOGO",
    "LOGO_DARK",
    "LOGO_THUMBNAIL",
    "INSTALLATION_NAME",
    "WIDGET_BRAND_URL",
    "TERMS_URL",
    "BRAND_URL",
    "BRAND_NAME",
    "PRIVACY_URL",
    "DISPLAY_MANIFEST",
    "CREATE_NEW_ACCOUNT_FROM_DASHBOARD",
    "CHATWOOT_INBOX_TOKEN",
    "API_CHANNEL_NAME",
    "API_CHANNEL_THUMBNAIL",
    "CLOUD_ANALYTICS_TOKEN",
    "DIRECT_UPLOADS_ENABLED",
    "MAXIMUM_FILE_UPLOAD_SIZE",
    "HCAPTCHA_SITE_KEY",
    "LOGOUT_REDIRECT_LINK",
    "DISABLE_USER_PROFILE_UPDATE",
    "DISABLE_META_INBOX_CREATION",
    "DISABLE_META_MESSAGE_SENDING",
    "DEPLOYMENT_ENV",
    "INSTALLATION_PRICING_PLAN",
    "CAPTAIN_TOOLS_MANIFEST_ENABLED",
    "CAPTAIN_TOOLS_CATALOG_URL",
    "ENABLE_ACCOUNT_SIGNUP",
];

/// installation_configs.serialized_value is YAML-serialized by Rails
/// (`--- !ruby/hash:ActiveSupport::HashWithIndifferentAccess\nvalue: X`). Extract the scalar.
fn parse_serialized(v: &Value) -> Value {
    if let Some(o) = v.as_object() {
        return o.get("value").cloned().unwrap_or(Value::Null);
    }
    let Some(s) = v.as_str() else {
        return Value::Null;
    };
    let Some(line) = s.lines().find_map(|l| l.strip_prefix("value: ")) else {
        return Value::Null;
    };
    let line = line.trim().trim_matches('\'').trim_matches('"');
    match line {
        "true" => json!(true),
        "false" => json!(false),
        _ => line
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| json!(line)),
    }
}

pub async fn global_config(db: &PgPool) -> Map<String, Value> {
    let defaults: Map<String, Value> =
        serde_json::from_str(include_str!("../installation_defaults.json")).unwrap();
    let rows: Vec<(String, Value)> =
        sqlx::query_as("SELECT name, serialized_value FROM installation_configs")
            .fetch_all(db)
            .await
            .unwrap_or_default();
    let stored: Map<String, Value> = rows
        .into_iter()
        .map(|(k, v)| (k, parse_serialized(&v)))
        .collect();
    let mut out = Map::new();
    for k in GLOBAL_KEYS {
        let v = stored
            .get(*k)
            .or_else(|| defaults.get(*k))
            .cloned()
            .unwrap_or(Value::Null);
        out.insert(k.to_string(), v);
    }
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    out.insert("APP_VERSION".into(), json!("4.20.1"));
    out.insert("IS_ENTERPRISE".into(), json!(true));
    if out
        .get("INSTALLATION_PRICING_PLAN")
        .is_none_or(|v| v.is_null())
    {
        out.insert(
            "INSTALLATION_PRICING_PLAN".into(),
            json!("open-source-enterprise"),
        );
    }
    out.insert(
        "FACEBOOK_API_VERSION".into(),
        json!(env("FACEBOOK_API_VERSION", "v18.0")),
    );
    let google_enabled = std::env::var("GOOGLE_OAUTH_CLIENT_ID")
        .ok()
        .is_some_and(|v| !v.trim().is_empty());
    let mut login_methods = vec!["email"];
    if google_enabled {
        login_methods.push("google_oauth2");
    }
    out.insert("ALLOWED_LOGIN_METHODS".into(), json!(login_methods));
    out.insert("GOOGLE_OAUTH_ENABLED".into(), json!(google_enabled));
    out.insert("ACTIVE_PLATFORM_BANNERS".into(), json!([]));
    out.insert("ACTIVE_FEATURE_ANNOUNCEMENTS".into(), json!([]));
    if out["ENABLE_ACCOUNT_SIGNUP"].is_null() {
        out.insert("ENABLE_ACCOUNT_SIGNUP".into(), json!("false"));
    }
    out
}

pub async fn api_info() -> Json<Value> {
    let google_enabled = std::env::var("GOOGLE_OAUTH_CLIENT_ID")
        .ok()
        .is_some_and(|v| !v.trim().is_empty());
    let mut login_methods = vec!["email"];
    if google_enabled {
        login_methods.push("google_oauth2");
    }
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "app": "pulsar",
        "is_enterprise": true,
        "edition": "open-source-enterprise",
        "vapid_public_key": crate::web_push::vapid_public_key(),
        "google_oauth_enabled": google_enabled,
        "allowed_login_methods": login_methods,
    }))
}

pub async fn openapi_json() -> Json<Value> {
    Json(json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Pulsar API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Chatwoot-compatible REST & Platform API served by Pulsar"
        },
        "servers": [{ "url": "/" }],
        "paths": {
            "/health": { "get": { "summary": "Health check" } },
            "/api": { "get": { "summary": "Server metadata & login capabilities" } },
            "/auth/sign_in": { "post": { "summary": "Authenticate agent (email/password or MFA)" } },
            "/auth/sso_exchange": { "post": { "summary": "Exchange one-time SSO token for session headers" } },
            "/omniauth/google_oauth2": { "get": { "summary": "Initiate Google OAuth2 sign-in" } },
            "/api/v1/accounts/{account_id}/conversations": {
                "get": { "summary": "List conversations" },
                "post": { "summary": "Create conversation" }
            },
            "/api/v1/accounts/{account_id}/conversations/{id}/messages": {
                "post": { "summary": "Send message or attachment" }
            },
            "/api/v1/accounts/{account_id}/conversations/{id}/messages/{message_id}/transcribe": {
                "post": { "summary": "Transcribe audio attachment with Whisper" }
            },
            "/api/v1/accounts/{account_id}/calls/{id}/transcribe": {
                "post": { "summary": "Transcribe recorded voice call" }
            },
            "/api/v1/accounts/{account_id}/contacts": { "get": { "summary": "List contacts" }, "post": { "summary": "Create contact" } },
            "/api/v1/accounts/{account_id}/inboxes": { "get": { "summary": "List inboxes" }, "post": { "summary": "Create inbox" } },
            "/api/v1/accounts/{account_id}/campaigns": { "get": { "summary": "List campaigns" }, "post": { "summary": "Create SMS/widget campaign" } },
            "/api/v1/accounts/{account_id}/custom_domains": { "get": { "summary": "List custom domains" }, "post": { "summary": "Register custom domain" } },
            "/api/v1/accounts/{account_id}/custom_domains/{id}/verify": { "post": { "summary": "Verify DNS/Cloudflare custom domain ownership" } },
            "/api/v2/accounts/{account_id}/reports": { "get": { "summary": "V2 timeseries metrics (including bot_resolutions_count & bot_handoffs_count)" } },
            "/api/v2/accounts/{account_id}/summary_reports/channel": { "get": { "summary": "Channel summary metrics" } },
            "/platform/api/v1/users/{id}/login": { "get": { "summary": "Generate single-use SSO login URL" } }
        }
    }))
}

pub async fn swagger_ui() -> Html<&'static str> {
    Html(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1" />
<title>Pulsar API Explorer (OpenAPI 3.0)</title>
<style>
  body { font-family: system-ui, -apple-system, sans-serif; margin: 2rem auto; max-width: 960px; padding: 0 1rem; color: #111827; background: #f9fafb; }
  h1 { margin-bottom: .25rem; }
  code, pre { background: #111827; color: #e5e7eb; padding: .15rem .4rem; border-radius: 6px; font-size: 13px; }
  pre { padding: 1rem; overflow: auto; }
  .endpoint { background: #fff; border: 1px solid #e5e7eb; border-radius: 10px; padding: .85rem 1rem; margin: .6rem 0; display: flex; justify-content: space-between; align-items: center; }
  .method { display: inline-block; font-size: 11px; font-weight: 700; padding: 2px 8px; border-radius: 6px; background: #dbeafe; color: #1d4ed8; margin-right: 8px; text-transform: uppercase; }
</style>
</head>
<body>
  <h1>Pulsar API Documentation</h1>
  <p>OpenAPI JSON spec: <a href="/swagger/openapi.json"><code>/swagger/openapi.json</code></a></p>
  <div id="list">Loading specification...</div>
  <script>
    fetch('/swagger/openapi.json').then(r => r.json()).then(spec => {
      const root = document.getElementById('list');
      root.innerHTML = '';
      for (const [path, ops] of Object.entries(spec.paths || {})) {
        for (const [method, meta] of Object.entries(ops)) {
          const el = document.createElement('div');
          el.className = 'endpoint';
          el.innerHTML = '<div><span class="method">' + method + '</span><code>' + path + '</code></div><span>' + (meta.summary || '') + '</span>';
          root.appendChild(el);
        }
      }
    });
  </script>
</body>
</html>"#)
}

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "woot" }))
}

/// Fallback for unported API routes: explicit 501 so gaps are visible, not silent.
pub async fn not_implemented(OriginalUri(uri): OriginalUri) -> Response {
    tracing::warn!("unported endpoint: {}", uri.path());
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        Json(json!({ "error": "Not implemented in Pulsar", "path": uri.path() })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    #[test]
    fn yaml_value() {
        let v = serde_json::json!(
            "--- !ruby/hash:ActiveSupport::HashWithIndifferentAccess\nvalue: Acme\n"
        );
        assert_eq!(super::parse_serialized(&v), serde_json::json!("Acme"));
        let v = serde_json::json!("---\nvalue: false\n");
        assert_eq!(super::parse_serialized(&v), serde_json::json!(false));
    }

    #[test]
    fn open_source_enterprise_defaults() {
        let defaults: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(include_str!("../installation_defaults.json")).unwrap();
        assert_eq!(
            defaults["INSTALLATION_PRICING_PLAN"],
            serde_json::json!("open-source-enterprise")
        );
    }
}
