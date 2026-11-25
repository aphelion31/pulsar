//! Account SAML SSO settings CRUD, metadata refresh, and SSO initiation endpoint.
//! Mirrors chatwoot/enterprise/app/controllers/api/v1/accounts/saml_settings_controller.rb
//! and enterprise/app/controllers/api/v1/auth_controller.rb.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::{get, post},
};
use base64::Engine;
use chrono::NaiveDateTime;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::common::{ApiError, ApiResult, AppState, CurrentUser, require_account};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/saml_settings",
            get(show)
                .post(create)
                .patch(update)
                .put(update)
                .delete(destroy),
        )
        .route("/saml_settings/refresh", post(refresh))
}

#[derive(sqlx::FromRow)]
struct SamlRow {
    id: i64,
    account_id: i64,
    sso_url: Option<String>,
    certificate: Option<String>,
    idp_entity_id: Option<String>,
    sp_entity_id: Option<String>,
    metadata_url: Option<String>,
    role_mappings: Option<Value>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

fn cert_fingerprint(cert: Option<&str>) -> Option<String> {
    let raw = cert?.trim();
    if raw.is_empty() {
        return None;
    }
    let body: String = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("-----"))
        .collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(body.as_bytes())
        .ok()?;
    let digest = Sha256::digest(&der);
    let hex_str = hex::encode_upper(digest);
    let pairs: Vec<&str> = hex_str
        .as_bytes()
        .chunks(2)
        .filter_map(|c| std::str::from_utf8(c).ok())
        .collect();
    Some(pairs.join(":"))
}

impl SamlRow {
    fn json(&self, frontend_url: &str) -> Value {
        let base = frontend_url.trim_end_matches('/');
        let sp = self
            .sp_entity_id
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{base}/saml/sp/{}", self.account_id));
        json!({
            "id": self.id,
            "account_id": self.account_id,
            "sso_url": self.sso_url,
            "certificate": self.certificate,
            "fingerprint": cert_fingerprint(self.certificate.as_deref()),
            "idp_entity_id": self.idp_entity_id,
            "sp_entity_id": sp,
            "metadata_url": self.metadata_url,
            "role_mappings": self.role_mappings.clone().unwrap_or_else(|| json!({})),
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        })
    }
}

fn pick(body: &Value) -> &Value {
    body.get("saml_settings")
        .filter(|v| v.is_object())
        .unwrap_or(body)
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

async fn fetch_row(st: &AppState, account_id: i64) -> Result<Option<SamlRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT id, account_id, sso_url, certificate, idp_entity_id, sp_entity_id,
                metadata_url, role_mappings, created_at, updated_at
         FROM account_saml_settings WHERE account_id = $1 LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&st.db)
    .await?)
}

async fn show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let Some(row) = fetch_row(&st, account_id).await? else {
        let base = st.frontend_url.trim_end_matches('/');
        return Ok(Json(json!({
            "id": null,
            "account_id": account_id,
            "sso_url": null,
            "certificate": null,
            "fingerprint": null,
            "idp_entity_id": null,
            "sp_entity_id": format!("{base}/saml/sp/{account_id}"),
            "metadata_url": null,
            "role_mappings": {},
        })));
    };
    Ok(Json(row.json(&st.frontend_url)))
}

async fn create(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let p = pick(&body);
    let sso_url = opt_str(p, "sso_url");
    let certificate = opt_str(p, "certificate");
    let idp_entity_id = opt_str(p, "idp_entity_id");
    let sp_entity_id = opt_str(p, "sp_entity_id");
    let metadata_url = opt_str(p, "metadata_url");
    let role_mappings = p
        .get("role_mappings")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));

    if sso_url.is_none() && metadata_url.is_none() {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Either sso_url or metadata_url is required".into(),
        ));
    }

    let row: SamlRow = if let Some(existing) = fetch_row(&st, account_id).await? {
        sqlx::query_as(
            "UPDATE account_saml_settings
             SET sso_url = COALESCE($2, sso_url),
                 certificate = COALESCE($3, certificate),
                 idp_entity_id = COALESCE($4, idp_entity_id),
                 sp_entity_id = COALESCE($5, sp_entity_id),
                 metadata_url = COALESCE($6, metadata_url),
                 role_mappings = $7,
                 updated_at = now()
             WHERE id = $1
             RETURNING id, account_id, sso_url, certificate, idp_entity_id, sp_entity_id,
                       metadata_url, role_mappings, created_at, updated_at",
        )
        .bind(existing.id)
        .bind(sso_url)
        .bind(certificate)
        .bind(idp_entity_id)
        .bind(sp_entity_id)
        .bind(metadata_url)
        .bind(role_mappings)
        .fetch_one(&st.db)
        .await?
    } else {
        sqlx::query_as(
            "INSERT INTO account_saml_settings
             (account_id, sso_url, certificate, idp_entity_id, sp_entity_id, metadata_url, role_mappings, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())
             RETURNING id, account_id, sso_url, certificate, idp_entity_id, sp_entity_id,
                       metadata_url, role_mappings, created_at, updated_at",
        )
        .bind(account_id)
        .bind(sso_url)
        .bind(certificate)
        .bind(idp_entity_id)
        .bind(sp_entity_id)
        .bind(metadata_url)
        .bind(role_mappings)
        .fetch_one(&st.db)
        .await?
    };

    Ok(Json(row.json(&st.frontend_url)))
}

async fn update(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    create(State(st), user, Path(account_id), Json(body)).await
}

async fn destroy(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult<StatusCode> {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    sqlx::query("DELETE FROM account_saml_settings WHERE account_id = $1")
        .bind(account_id)
        .execute(&st.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn extract_xml_attr<'a>(xml: &'a str, attr: &str) -> Option<&'a str> {
    for q in ['"', '\''] {
        let needle = format!("{attr}={q}");
        if let Some(pos) = xml.find(&needle) {
            let rest = &xml[pos + needle.len()..];
            if let Some(end) = rest.find(q) {
                return Some(&rest[..end]);
            }
        }
    }
    None
}

fn extract_xml_tag<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let start = xml.find(&open)?;
    let after_tag = &xml[start + open.len()..];
    let gt = after_tag.find('>')?;
    let body = &after_tag[gt + 1..];
    let close = format!("</{tag}>");
    let end = body.find(&close)?;
    Some(body[..end].trim())
}

fn parse_saml_metadata(xml: &str) -> (Option<String>, Option<String>, Option<String>) {
    let idp_entity_id = extract_xml_attr(xml, "entityID").map(str::to_string);
    let sso_url = xml
        .find("SingleSignOnService")
        .and_then(|idx| extract_xml_attr(&xml[idx..], "Location"))
        .map(str::to_string);
    let cert_raw = extract_xml_tag(xml, "ds:X509Certificate")
        .or_else(|| extract_xml_tag(xml, "X509Certificate"))
        .map(|b| format!("-----BEGIN CERTIFICATE-----\n{b}\n-----END CERTIFICATE-----"));
    (idp_entity_id, sso_url, cert_raw)
}

async fn refresh(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let existing = fetch_row(&st, account_id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let url = existing
        .metadata_url
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "metadata_url is not configured".into(),
            )
        })?;
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(ApiError::bad("Invalid metadata_url"));
    }

    let xml = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Failed to fetch SAML metadata".into(),
            )
        })?
        .text()
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Failed to read SAML metadata".into(),
            )
        })?;

    let (idp_entity_id, sso_url, certificate) = parse_saml_metadata(&xml);
    let row: SamlRow = sqlx::query_as(
        "UPDATE account_saml_settings
         SET idp_entity_id = COALESCE($2, idp_entity_id),
             sso_url = COALESCE($3, sso_url),
             certificate = COALESCE($4, certificate),
             metadata_refreshed_at = now(),
             updated_at = now()
         WHERE id = $1
         RETURNING id, account_id, sso_url, certificate, idp_entity_id, sp_entity_id,
                   metadata_url, role_mappings, created_at, updated_at",
    )
    .bind(existing.id)
    .bind(idp_entity_id)
    .bind(sso_url)
    .bind(certificate)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(row.json(&st.frontend_url)))
}

/// POST /api/v1/auth/saml_login
pub async fn saml_login(
    State(st): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    let email = body["email"]
        .as_str()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("Email is required"))?;

    let sso_account_id = body["sso_account_id"]
        .as_i64()
        .or_else(|| body["sso_account_id"].as_str().and_then(|s| s.parse().ok()));

    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT s.account_id, s.sso_url
         FROM users u
         JOIN account_users au ON au.user_id = u.id
         JOIN account_saml_settings s ON s.account_id = au.account_id
         WHERE lower(u.email) = $1
           AND COALESCE(s.sso_url, '') <> ''
           AND COALESCE(s.certificate, '') <> ''
           AND ($2::bigint IS NULL OR s.account_id = $2)
         ORDER BY au.id
         LIMIT 1",
    )
    .bind(&email)
    .bind(sso_account_id)
    .fetch_optional(&st.db)
    .await?;

    let target_url = if let Some((account_id, sso_url)) = row {
        let sep = if sso_url.contains('?') { '&' } else { '?' };
        let relay = body["redirect_url"].as_str().unwrap_or("web");
        let encoded_relay: String =
            url::form_urlencoded::byte_serialize(relay.as_bytes()).collect();
        format!("{sso_url}{sep}account_id={account_id}&RelayState={encoded_relay}")
    } else {
        let base = st.frontend_url.trim_end_matches('/');
        format!("{base}/app/login/sso?error=saml-authentication-failed")
    };

    let mut resp = Response::new(axum::body::Body::empty());
    *resp.status_mut() = StatusCode::TEMPORARY_REDIRECT;
    if let Ok(hv) = HeaderValue::from_str(&target_url) {
        resp.headers_mut().insert(header::LOCATION, hv);
    }
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_idp_metadata_xml() {
        let xml = r#"<EntityDescriptor entityID="https://idp.example.com/meta"><IDPSSODescriptor><KeyDescriptor><ds:X509Certificate>QUJD</ds:X509Certificate></KeyDescriptor><SingleSignOnService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect" Location="https://idp.example.com/sso"/></IDPSSODescriptor></EntityDescriptor>"#;
        let (idp, sso, cert) = parse_saml_metadata(xml);
        assert_eq!(idp.as_deref(), Some("https://idp.example.com/meta"));
        assert_eq!(sso.as_deref(), Some("https://idp.example.com/sso"));
        assert!(cert.unwrap().contains("QUJD"));
    }
}
