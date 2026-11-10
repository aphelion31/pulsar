//! Installation config (Chatwoot installation_configs), health, and the 501 fallback.
//! The dashboard itself is the React Router SPA in web/, served from main.rs.
use axum::{
    extract::OriginalUri,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Map, Value};
use sqlx::PgPool;


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
