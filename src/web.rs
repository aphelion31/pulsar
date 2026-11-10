//! Serves the prebuilt Vue dashboard (chatwoot `vite build` output) with the same
//! window.chatwootConfig / globalConfig that Rails' vueapp.html.erb injects.
use axum::{
    extract::{OriginalUri, State},
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use serde_json::{json, Map, Value};
use sqlx::PgPool;

use crate::common::AppState;

const GLOBAL_KEYS: &[&str] = &[
    "LOGO", "LOGO_DARK", "LOGO_THUMBNAIL", "INSTALLATION_NAME", "WIDGET_BRAND_URL", "TERMS_URL", "BRAND_URL",
    "BRAND_NAME", "PRIVACY_URL", "DISPLAY_MANIFEST", "CREATE_NEW_ACCOUNT_FROM_DASHBOARD", "CHATWOOT_INBOX_TOKEN",
    "API_CHANNEL_NAME", "API_CHANNEL_THUMBNAIL", "CLOUD_ANALYTICS_TOKEN", "DIRECT_UPLOADS_ENABLED",
    "MAXIMUM_FILE_UPLOAD_SIZE", "HCAPTCHA_SITE_KEY", "LOGOUT_REDIRECT_LINK", "DISABLE_USER_PROFILE_UPDATE",
    "DISABLE_META_INBOX_CREATION", "DISABLE_META_MESSAGE_SENDING", "DEPLOYMENT_ENV", "INSTALLATION_PRICING_PLAN",
    "CAPTAIN_TOOLS_MANIFEST_ENABLED", "CAPTAIN_TOOLS_CATALOG_URL", "ENABLE_ACCOUNT_SIGNUP",
];

/// installation_configs.serialized_value is YAML-serialized by Rails
/// (`--- !ruby/hash:ActiveSupport::HashWithIndifferentAccess\nvalue: X`). Extract the scalar.
fn parse_serialized(v: &Value) -> Value {
    if let Some(o) = v.as_object() {
        return o.get("value").cloned().unwrap_or(Value::Null);
    }
    let Some(s) = v.as_str() else { return Value::Null };
    let Some(line) = s.lines().find_map(|l| l.strip_prefix("value: ")) else { return Value::Null };
    let line = line.trim().trim_matches('\'').trim_matches('"');
    match line {
        "true" => json!(true),
        "false" => json!(false),
        _ => line.parse::<i64>().map(Value::from).unwrap_or_else(|_| json!(line)),
    }
}

pub async fn global_config(db: &PgPool) -> Map<String, Value> {
    let defaults: Map<String, Value> = serde_json::from_str(include_str!("../installation_defaults.json")).unwrap();
    let rows: Vec<(String, Value)> = sqlx::query_as("SELECT name, serialized_value FROM installation_configs")
        .fetch_all(db)
        .await
        .unwrap_or_default();
    let stored: Map<String, Value> = rows.into_iter().map(|(k, v)| (k, parse_serialized(&v))).collect();
    let mut out = Map::new();
    for k in GLOBAL_KEYS {
        let v = stored.get(*k).or_else(|| defaults.get(*k)).cloned().unwrap_or(Value::Null);
        out.insert(k.to_string(), v);
    }
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    out.insert("APP_VERSION".into(), json!("4.19.1"));
    out.insert("IS_ENTERPRISE".into(), json!(false));
    out.insert("FACEBOOK_API_VERSION".into(), json!(env("FACEBOOK_API_VERSION", "v18.0")));
    out.insert("ALLOWED_LOGIN_METHODS".into(), json!(["email"]));
    out.insert("ACTIVE_PLATFORM_BANNERS".into(), json!([]));
    out.insert("ACTIVE_FEATURE_ANNOUNCEMENTS".into(), json!([]));
    if out["ENABLE_ACCOUNT_SIGNUP"].is_null() {
        out.insert("ENABLE_ACCOUNT_SIGNUP".into(), json!("false"));
    }
    out
}

fn vite_tags(public_dir: &std::path::Path, entry: &str) -> String {
    let manifest: Value = std::fs::read_to_string(public_dir.join("vite/.vite/manifest.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let e = &manifest[format!("entrypoints/{entry}.js")];
    let mut css: Vec<String> = Vec::new();
    // collect css of entry + static imports (vite_ruby does the same)
    fn walk(m: &Value, key: &str, css: &mut Vec<String>, seen: &mut std::collections::HashSet<String>) {
        if !seen.insert(key.to_string()) {
            return;
        }
        let n = &m[key];
        for c in n["css"].as_array().into_iter().flatten() {
            css.push(c.as_str().unwrap_or_default().to_string());
        }
        for i in n["imports"].as_array().into_iter().flatten() {
            walk(m, i.as_str().unwrap_or_default(), css, seen);
        }
    }
    walk(&manifest, &format!("entrypoints/{entry}.js"), &mut css, &mut Default::default());
    css.dedup();
    let mut s = String::new();
    for c in css {
        s += &format!(r#"<link rel="stylesheet" href="/vite/{c}" media="screen">"#);
    }
    s += &format!(r#"<script src="/vite/{}" crossorigin="anonymous" type="module"></script>"#, e["file"].as_str().unwrap_or(""));
    s
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'").replace('<', "\\u003c")
}

/// GET /, /app, /app/*  (and login pages use the v3app pack)
pub async fn dashboard(State(st): State<AppState>, OriginalUri(uri): OriginalUri) -> Response {
    let path = uri.path();
    if path == "/" {
        return Redirect::to("/app").into_response();
    }
    let pack = if path.contains("/auth") || path.contains("/login") { "v3app" } else { "dashboard" };
    let gc = global_config(&st.db).await;
    let s = |k: &str| gc.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let config = json!({
        "hostURL": st.frontend_url, "helpCenterURL": env("HELPCENTER_URL"),
        "fbAppId": "", "instagramAppId": "", "tiktokAppId": "",
        "allowedLoginMethods": ["email"], "fbApiVersion": s("FACEBOOK_API_VERSION"),
        "whatsappAppId": "", "whatsappConfigurationId": "", "whatsappApiVersion": "",
        "signupEnabled": s("ENABLE_ACCOUNT_SIGNUP"), "isEnterprise": "false", "isMfaEnabled": "false",
        "enabledLanguages": [{"name": "English (en)", "iso_639_1_code": "en"}, {"name": "Bahasa Indonesia (id)", "iso_639_1_code": "id"}],
        "helpUrls": {}, "inboxEventsEnabled": "false", "searchTimeRangeLimitDays": 90,
        "autoResolveMinMinutes": 10, "selectedLocale": "en",
    });
    let title = s("INSTALLATION_NAME");
    let html = format!(
        r#"<!DOCTYPE html>
<html>
  <head>
    <title>{title}</title>
    <meta name="viewport" content="width=device-width, initial-scale=1, shrink-to-fit=no, user-scalable=0"/>
    <meta name="robots" content="noindex">
    <link rel="icon" type="image/png" sizes="512x512" href="{logo}">
    <script>
      window.chatwootConfig = {config};
      window.globalConfig = {gc};
      window.browserConfig = {{ browser_name: '' }};
      window.errorLoggingConfig = '{sentry}';
    </script>
    {tags}
  </head>
  <body class="text-slate-600">
    <div id="app"></div>
    <noscript id="noscript">This app works best with JavaScript enabled.</noscript>
  </body>
</html>"#,
        title = html_escape(&title),
        logo = html_escape(&s("LOGO_THUMBNAIL")),
        config = config.to_string().replace("</", "<\\/"),
        gc = Value::Object(gc.clone()).to_string().replace("</", "<\\/"),
        sentry = esc(&env("SENTRY_FRONTEND_DSN")),
        tags = vite_tags(&st.public_dir, pack),
    );
    Html(html).into_response()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "woot" }))
}

/// Fallback for unported API routes: explicit 501 so gaps are visible, not silent.
pub async fn not_implemented(OriginalUri(uri): OriginalUri) -> Response {
    tracing::warn!("unported endpoint: {}", uri.path());
    (axum::http::StatusCode::NOT_IMPLEMENTED, Json(json!({ "error": "Not implemented in Pulsar", "path": uri.path() })))
        .into_response()
}

#[cfg(test)]
mod tests {
    #[test]
    fn yaml_value() {
        let v = serde_json::json!("--- !ruby/hash:ActiveSupport::HashWithIndifferentAccess\nvalue: Acme\n");
        assert_eq!(super::parse_serialized(&v), serde_json::json!("Acme"));
        let v = serde_json::json!("---\nvalue: false\n");
        assert_eq!(super::parse_serialized(&v), serde_json::json!(false));
    }
}
