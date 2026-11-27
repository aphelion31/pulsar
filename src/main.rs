mod account_more;
mod ar_encryption;
mod assignment_policies;
mod auth;
mod automation;
mod bots;
mod cable;
mod capacity_policies;
mod captain;
mod channels;
mod common;
mod companies;
mod contacts;
mod conversations;
mod custom_domains;
mod data_exports;
mod data_imports;
mod dyte;
mod email_inbound;
mod events;
mod help_center;
mod inbox_actions;
mod inboxes;
mod integrations;
mod listeners;
mod mailer;
mod mfa;
mod monitors;
mod platform;
mod public_api;
mod registration;
mod reports;
mod saml;
mod scheduler;
mod seed;
mod settings;
mod shopify;
mod sla;
mod slack;
mod storage;
mod super_admin;
mod voice;
mod web;
mod web_push;
mod widget;

use axum::{
    Router,
    routing::{any, get, post},
};
use common::AppState;
use tower_http::services::{ServeDir, ServeFile};

/// Account-scoped API: `/api/v1/accounts/{account_id}/...`. Each module owns its own routes.
fn account_routes() -> Router<AppState> {
    Router::new()
        .merge(conversations::routes())
        .merge(contacts::routes())
        .merge(inboxes::routes())
        .merge(inbox_actions::routes())
        .merge(assignment_policies::routes())
        .merge(capacity_policies::routes())
        .merge(sla::routes())
        .merge(saml::routes())
        .merge(slack::routes())
        .merge(settings::routes())
        .merge(help_center::routes())
        .merge(companies::routes())
        .merge(automation::routes())
        .merge(bots::routes())
        .merge(account_more::routes())
        .merge(custom_domains::routes())
        .merge(data_exports::routes())
        .merge(data_imports::routes())
        .merge(channels::twilio::account_routes())
        .merge(channels::whatsapp::account_routes())
        .merge(channels::facebook::account_routes())
        .merge(channels::instagram::account_routes())
        .merge(channels::social_oauth::account_routes())
        .merge(dyte::account_routes())
        .merge(shopify::account_routes())
        .merge(integrations::linear::account_routes())
        .merge(integrations::notion::account_routes())
        .merge(integrations::stripe::account_routes())
        .merge(captain::account_routes())
        .merge(monitors::account_routes())
        .merge(voice::account_routes())
        .fallback(web::not_implemented)
}

async fn enterprise_account_limits(
    axum::extract::State(st): axum::extract::State<AppState>,
    user: common::CurrentUser,
    axum::extract::Path(account_id): axum::extract::Path<i64>,
) -> common::ApiResult {
    common::require_account(&st.db, &user, account_id).await?;
    let convs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE account_id = $1 AND created_at >= date_trunc('month', now())",
    )
    .bind(account_id)
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);
    let agents: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM account_users WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&st.db)
            .await
            .unwrap_or(0);
    let non_web: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM inboxes WHERE account_id = $1 AND channel_type <> 'Channel::WebWidget'",
    )
    .bind(account_id)
    .fetch_one(&st.db)
    .await
    .unwrap_or(0);
    let stored_limits: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT limits FROM accounts WHERE id = $1::bigint")
            .bind(account_id)
            .fetch_optional(&st.db)
            .await
            .ok()
            .flatten();

    let agents_allowed = stored_limits
        .as_ref()
        .and_then(|v| v["agents"].as_i64())
        .unwrap_or(100_000);
    let inboxes_allowed = stored_limits
        .as_ref()
        .and_then(|v| v["inboxes"].as_i64())
        .unwrap_or(100_000);

    Ok(axum::Json(serde_json::json!({
        "id": account_id,
        "limits": {
            "conversation": { "allowed": 100_000, "consumed": convs },
            "non_web_inboxes": { "allowed": inboxes_allowed, "consumed": non_web },
            "agents": { "allowed": agents_allowed, "consumed": agents },
        }
    })))
}

async fn enterprise_billing_summary(
    axum::extract::State(st): axum::extract::State<AppState>,
    user: common::CurrentUser,
    axum::extract::Path(account_id): axum::extract::Path<i64>,
) -> common::ApiResult {
    common::require_account(&st.db, &user, account_id).await?;
    let attrs: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT custom_attributes FROM accounts WHERE id = $1::bigint")
            .bind(account_id)
            .fetch_optional(&st.db)
            .await?
            .flatten();
    let attrs = attrs.unwrap_or_else(|| serde_json::json!({}));
    Ok(axum::Json(serde_json::json!({
        "id": account_id,
        "plan": attrs.get("plan_name").and_then(|v| v.as_str()).unwrap_or("Open Source Enterprise"),
        "billing_currency": attrs.get("billing_currency").and_then(|v| v.as_str()).unwrap_or("usd"),
        "stripe_customer_id": attrs.get("stripe_customer_id"),
        "marked_for_deletion_at": attrs.get("marked_for_deletion_at"),
    })))
}

async fn enterprise_select_billing_currency(
    axum::extract::State(st): axum::extract::State<AppState>,
    user: common::CurrentUser,
    axum::extract::Path(account_id): axum::extract::Path<i64>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<axum::http::StatusCode, common::ApiError> {
    common::require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let cur = body["currency"]
        .as_str()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| matches!(s.as_str(), "usd" | "eur" | "gbp" | "inr"))
        .ok_or_else(|| common::ApiError::bad("Invalid billing currency"))?;
    sqlx::query(
        "UPDATE accounts SET custom_attributes = COALESCE(custom_attributes, '{}'::jsonb) || $1, updated_at = now() WHERE id = $2::bigint",
    )
    .bind(serde_json::json!({ "billing_currency": cur }))
    .bind(account_id)
    .execute(&st.db)
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn enterprise_toggle_deletion(
    axum::extract::State(st): axum::extract::State<AppState>,
    user: common::CurrentUser,
    axum::extract::Path(account_id): axum::extract::Path<i64>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> common::ApiResult {
    common::require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let action = body["action_type"].as_str().unwrap_or("");
    let patch = match action {
        "delete" => {
            serde_json::json!({ "marked_for_deletion_at": chrono::Utc::now().to_rfc3339() })
        }
        "undelete" => serde_json::json!({ "marked_for_deletion_at": null }),
        _ => {
            return Err(common::ApiError::bad(
                "Invalid action_type. Must be either \"delete\" or \"undelete\"",
            ));
        }
    };
    sqlx::query(
        "UPDATE accounts SET custom_attributes = COALESCE(custom_attributes, '{}'::jsonb) || $1, updated_at = now() WHERE id = $2::bigint",
    )
    .bind(&patch)
    .bind(account_id)
    .execute(&st.db)
    .await?;
    Ok(axum::Json(
        serde_json::json!({ "id": account_id, "action_type": action }),
    ))
}

async fn enterprise_topup_options(
    axum::extract::State(st): axum::extract::State<AppState>,
    user: common::CurrentUser,
    axum::extract::Path(account_id): axum::extract::Path<i64>,
) -> common::ApiResult {
    common::require_account(&st.db, &user, account_id).await?;
    let attrs: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT custom_attributes FROM accounts WHERE id = $1::bigint")
            .bind(account_id)
            .fetch_optional(&st.db)
            .await?
            .flatten();
    let currency = attrs
        .as_ref()
        .and_then(|v| v["billing_currency"].as_str())
        .unwrap_or("usd");
    Ok(axum::Json(serde_json::json!({
        "id": account_id,
        "currency": currency,
        "options": [
            { "credits": 500, "amount_cents": 2000 },
            { "credits": 2500, "amount_cents": 8000 },
            { "credits": 10000, "amount_cents": 25000 }
        ]
    })))
}

async fn enterprise_legal_compliance(
    axum::extract::State(st): axum::extract::State<AppState>,
    user: common::CurrentUser,
    axum::extract::Path(account_id): axum::extract::Path<i64>,
) -> common::ApiResult {
    common::require_account(&st.db, &user, account_id).await?;
    let val: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT serialized_value FROM installation_configs WHERE name = 'CHATWOOT_DPA_SIGNING_URL' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&st.db)
    .await
    .ok()
    .flatten();
    let dpa_url = val.and_then(|v| {
        v.get("value")
            .and_then(|x| x.as_str())
            .or_else(|| v.as_str())
            .filter(|s| !s.is_empty() && *s != "null")
            .map(str::to_string)
    });
    Ok(axum::Json(
        serde_json::json!({ "dpa_signing_url": dpa_url }),
    ))
}

async fn enterprise_checkout_stub(
    axum::extract::State(st): axum::extract::State<AppState>,
    user: common::CurrentUser,
    axum::extract::Path(account_id): axum::extract::Path<i64>,
) -> common::ApiResult {
    common::require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    Ok(axum::Json(serde_json::json!({ "redirect_url": null })))
}

async fn enterprise_subscription_noop(
    axum::extract::State(st): axum::extract::State<AppState>,
    user: common::CurrentUser,
    axum::extract::Path(account_id): axum::extract::Path<i64>,
) -> Result<axum::http::StatusCode, common::ApiError> {
    common::require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub fn app(state: AppState) -> Router {
    // React Router SPA: real files under /app/* (assets), everything else under /app falls back to index.html
    let web_dir = state.public_dir.clone();
    let spa = ServeDir::new(&web_dir).fallback(ServeFile::new(web_dir.join("index.html")));
    Router::new()
        .route("/health", get(web::health))
        .route("/auth/sign_in", post(auth::sign_in))
        .route("/auth/sso_exchange", post(auth::sso_exchange))
        .route("/omniauth/google_oauth2", get(auth::google_omniauth_start))
        .route(
            "/omniauth/google_oauth2/callback",
            get(auth::google_omniauth_callback),
        )
        .route("/auth/confirmation", post(registration::confirm))
        .route(
            "/resend_confirmation",
            post(registration::resend_confirmation),
        )
        .route(
            "/auth/password",
            post(auth::request_password_reset).put(auth::reset_password),
        )
        .route("/auth/validate_token", get(auth::validate_token))
        .route("/auth/sign_out", axum::routing::delete(auth::sign_out))
        .route(
            "/api/v1/profile",
            get(auth::profile_show)
                .put(auth::profile_update)
                .patch(auth::profile_update)
                .layer(axum::extract::DefaultBodyLimit::max(
                    storage::MAX_BYTES + 1024 * 1024,
                )),
        )
        .route(
            "/api/v1/profile/avatar",
            axum::routing::delete(auth::profile_avatar_destroy),
        )
        .route(
            "/api/v1/profile/resend_confirmation",
            post(auth::resend_confirmation),
        )
        .route(
            "/api/v1/profile/reset_access_token",
            post(auth::reset_access_token),
        )
        .route(
            "/api/v1/profile/set_active_account",
            axum::routing::put(auth::set_active_account),
        )
        .route("/api/v1/profile/availability", post(auth::availability))
        .route("/api/v1/profile/auto_offline", post(auth::auto_offline))
        .route(
            "/api/v1/profile/work_preferences",
            post(auth::auto_offline).patch(auth::auto_offline),
        )
        .route("/api/v1/accounts", post(registration::v1_create))
        .route("/api/v2/accounts", post(registration::v2_create))
        .route(
            "/api/v1/accounts/{account_id}",
            get(auth::account_show)
                .patch(auth::account_update)
                .put(auth::account_update),
        )
        .route(
            "/api/v1/accounts/{account_id}/onboarding",
            axum::routing::put(auth::onboarding_update).patch(auth::onboarding_update),
        )
        .route(
            "/api/v1/accounts/{account_id}/onboarding/help_center_generation",
            get(auth::help_center_generation),
        )
        .route(
            "/api/v1/accounts/{account_id}/",
            get(auth::account_show)
                .patch(auth::account_update)
                .put(auth::account_update),
        )
        .route(
            "/api/v1/accounts/{account_id}/update_active_at",
            post(auth::update_active_at),
        )
        .route("/api/v1/profile/sessions", get(auth::sessions_index))
        .route(
            "/api/v1/profile/sessions/{id}",
            axum::routing::delete(mfa::session_destroy),
        )
        .route(
            "/api/v1/profile/trusted_devices",
            axum::routing::delete(auth::trusted_devices_destroy),
        )
        .route(
            "/api/v1/profile/mfa",
            get(mfa::show).post(mfa::create).delete(mfa::destroy),
        )
        .route("/api/v1/profile/mfa/verify", post(mfa::verify))
        .route("/api/v1/profile/mfa/backup_codes", post(mfa::backup_codes))
        .route("/api/v1/auth/saml_login", post(saml::saml_login))
        .route(
            "/api/v1/notification_subscriptions",
            post(auth::notification_subscription_create)
                .delete(auth::notification_subscription_destroy),
        )
        .route(
            "/api/v1/changelog",
            get(|| async { axum::Json(serde_json::json!({ "posts": [] })) }),
        )
        .route("/api", get(web::api_info))
        .route("/swagger", get(web::swagger_ui))
        .route("/swagger/openapi.json", get(web::openapi_json))
        .route(
            "/robots.txt",
            get(|| async { "User-agent: *\nDisallow: /app\n" }),
        )
        .route(
            "/favicon.ico",
            get(|| async { axum::http::StatusCode::NO_CONTENT }),
        )
        .route(
            "/.well-known/assetlinks.json",
            get(|| async { axum::Json(serde_json::json!([])) }),
        )
        .route(
            "/.well-known/apple-app-site-association",
            get(|| async {
                axum::Json(serde_json::json!({ "applinks": { "apps": [], "details": [] } }))
            }),
        )
        .route(
            "/.well-known/microsoft-identity-association.json",
            get(|| async { axum::Json(serde_json::json!({ "associatedApplications": [] })) }),
        )
        .route(
            "/.well-known/cf-custom-hostname-challenge/{id}",
            get(|axum::extract::Path(id): axum::extract::Path<String>| async move { id }),
        )
        .route_service(
            "/survey/responses/{id}",
            ServeFile::new(web_dir.join("index.html")),
        )
        .nest("/api/v1/accounts/{account_id}", account_routes())
        .nest(
            "/api/v2/accounts/{account_id}",
            reports::routes().fallback(web::not_implemented),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/limits",
            get(enterprise_account_limits),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/billing_summary",
            get(enterprise_billing_summary),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/checkout",
            post(enterprise_checkout_stub),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/subscription",
            post(enterprise_subscription_noop),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/select_billing_currency",
            post(enterprise_select_billing_currency),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/toggle_deletion",
            post(enterprise_toggle_deletion),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/topup_checkout",
            post(enterprise_checkout_stub),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/topup_options",
            get(enterprise_topup_options),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/legal_compliance",
            get(enterprise_legal_compliance),
        )
        .route(
            "/enterprise/api/v1/accounts/{account_id}/reconnect_shopify",
            post(enterprise_checkout_stub),
        )
        .merge(captain::public_routes())
        .merge(email_inbound::webhook_routes())
        .merge(voice::public_routes())
        .merge(widget::routes())
        .merge(dyte::widget_routes())
        .merge(shopify::public_routes())
        .merge(slack::public_routes())
        .merge(integrations::linear::public_routes())
        .merge(integrations::notion::public_routes())
        .merge(integrations::stripe::public_routes())
        .merge(channels::telegram::routes())
        .merge(channels::line::routes())
        .merge(channels::twilio::routes())
        .merge(channels::whatsapp::routes())
        .merge(channels::sms::routes())
        .merge(channels::facebook::routes())
        .merge(channels::instagram::routes())
        .merge(channels::social_oauth::routes())
        .nest("/platform/api/v1", platform::routes())
        .merge(public_api::routes())
        .merge(help_center::public_routes())
        .merge(storage::routes())
        .merge(super_admin::routes())
        .route("/api/{*rest}", any(web::not_implemented))
        .route("/cable", get(cable::ws_handler))
        .route("/", get(|| async { axum::response::Redirect::to("/app/") }))
        .route(
            "/app",
            get(|| async { axum::response::Redirect::to("/app/") }),
        )
        .nest_service("/app/", spa)
        .route_service("/sdk.js", ServeFile::new(web_dir.join("sdk.js")))
        .route_service("/sw.js", ServeFile::new(web_dir.join("sw.js")))
        // Negotiated via Accept-Encoding; skips tiny bodies, images and SSE/ws upgrades by default.
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();
    let url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/pulsar".into());
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await?;

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("seed") {
        return seed::run(&db, &args[2..]).await;
    }

    // Pulsar's queries target this Chatwoot schema version. An older Chatwoot database misses columns and some endpoints 500.
    const SCHEMA: &str = "20261125100456";
    let db_schema: Option<String> =
        sqlx::query_scalar("SELECT max(version) FROM schema_migrations")
            .fetch_one(&db)
            .await
            .ok()
            .flatten();
    if db_schema.as_deref().is_none_or(|v| v < SCHEMA) {
        tracing::warn!(
            "database schema {} is older than {SCHEMA} (Chatwoot 4.20). Upgrade Chatwoot first, or apply migrations/0001_schema.sql column additions",
            db_schema.as_deref().unwrap_or("unknown")
        );
    }

    // Released Chatwoot 4.19.1 lacks account_users.accepts_conversations/accepts_calls (added later on main).
    let has_wp: bool = sqlx::query_scalar(
        "SELECT count(*) = 2 FROM information_schema.columns
         WHERE table_schema = current_schema() AND table_name = 'account_users'
           AND column_name IN ('accepts_conversations', 'accepts_calls')",
    )
    .fetch_one(&db)
    .await?;
    let _ = common::HAS_WORK_PREFERENCES.set(has_wp);
    if !has_wp {
        tracing::info!(
            "account_users work preferences not in this schema; using Chatwoot defaults (accept all)"
        );
    }

    // Pulsar-owned table (same as migrations/0003). Created here too so an install that skipped the
    // migration keeps the widget working instead of failing every request. Idempotent; Chatwoot
    // tables are not touched.
    sqlx::query(include_str!(
        "../migrations/0003_pulsar_widget_token_floor.sql"
    ))
    .execute(&db)
    .await
    .map_err(|e| anyhow::anyhow!("creating pulsar_widget_token_floors: {e}"))?;
    sqlx::query(include_str!("../migrations/0004_pulsar_draft_messages.sql"))
        .execute(&db)
        .await
        .map_err(|e| anyhow::anyhow!("creating pulsar_draft_messages: {e}"))?;
    sqlx::query(include_str!("../migrations/0005_pulsar_events.sql"))
        .execute(&db)
        .await
        .map_err(|e| anyhow::anyhow!("creating pulsar_events: {e}"))?;
    sqlx::raw_sql(include_str!("../migrations/0006_data_exports.sql"))
        .execute(&db)
        .await
        .map_err(|e| anyhow::anyhow!("creating data_exports: {e}"))?;
    sqlx::raw_sql(include_str!("../migrations/0007_parity_extras.sql"))
        .execute(&db)
        .await
        .map_err(|e| anyhow::anyhow!("creating parity extras: {e}"))?;
    sqlx::raw_sql(include_str!("../migrations/0008_chatwoot_4_20.sql"))
        .execute(&db)
        .await
        .map_err(|e| anyhow::anyhow!("applying chatwoot 4.20 schema delta: {e}"))?;

    let hub = cable::Hub::with_db(db.clone());
    let state = AppState {
        db,
        hub,
        public_dir: std::env::var("WEB_DIR")
            .unwrap_or_else(|_| "web/build/client".into())
            .into(),
        frontend_url: std::env::var("FRONTEND_URL")
            .unwrap_or_else(|_| "http://localhost:3000".into()),
    };
    cable::spawn_pg_fanout(state.hub.clone(), state.db.clone());
    // Async listeners (Chatwoot AsyncDispatcher). Each lane registers its listener here.
    events::start(
        state.clone(),
        vec![
            (
                "notifications",
                events::listener(listeners::notifications::handle),
            ),
            ("webhooks", events::listener(listeners::webhooks::handle)),
            (
                "automation",
                events::listener(listeners::automation::handle),
            ),
            ("csat", events::listener(listeners::csat::handle)),
            (
                "reporting_events",
                events::listener(listeners::reporting_events::handle),
            ),
            ("agent_bots", events::listener(listeners::agent_bots::handle)),
            ("captain", events::listener(listeners::captain::handle)),
            (
                "email_outbound",
                events::listener(listeners::email_outbound::handle),
            ),
            (
                "telegram_outbound",
                events::listener(listeners::telegram_outbound::handle),
            ),
            (
                "line_outbound",
                events::listener(listeners::line_outbound::handle),
            ),
            (
                "twilio_outbound",
                events::listener(listeners::twilio_outbound::handle),
            ),
            (
                "whatsapp_outbound",
                events::listener(channels::whatsapp::outbound),
            ),
            (
                "sms_outbound",
                events::listener(listeners::sms_outbound::handle),
            ),
            (
                "facebook_outbound",
                events::listener(channels::facebook::outbound),
            ),
            (
                "instagram_outbound",
                events::listener(channels::instagram::outbound),
            ),
            (
                "tiktok_outbound",
                events::listener(channels::social_oauth::tiktok_outbound),
            ),
            (
                "twitter_outbound",
                events::listener(channels::social_oauth::twitter_outbound),
            ),
            (
                "dialogflow",
                events::listener(listeners::dialogflow::handle),
            ),
        ],
    );
    // Chatwoot FetchImapEmailInboxesJob: poll IMAP for Channel::Email inboxes. Errors are logged per inbox.
    email_inbound::spawn(state.clone());
    scheduler::spawn(state.clone());
    let addr = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:3000".into());
    tracing::info!("pulsar listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn router_builds_without_overlapping_routes() {
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/pulsar")
            .expect("lazy pool");
        let state = AppState {
            db,
            hub: cable::Hub::default(),
            public_dir: std::path::PathBuf::from("web/build/client"),
            frontend_url: "http://localhost:3000".into(),
        };
        let _router = app(state);
    }
}
