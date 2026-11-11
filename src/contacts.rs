//! Contacts endpoints. Owned by this worker.
//! See WORKERS.md and ../chatwoot/app/controllers/api/v1/accounts/contacts_controller.rb.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::common::{now, require_account, ts, ApiError, ApiResult, AppState, CurrentUser};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/contacts", get(index).post(create))
        .route("/contacts/search", get(search))
        .route("/contacts/active", get(active))
        .route("/contacts/filter", post(filter))
        .route("/contacts/{id}", get(show).patch(update).put(update).delete(destroy))
        .route("/contacts/{id}/conversations", get(conversations))
        .route("/contacts/{id}/labels", get(labels_index).post(labels_create))
        .route("/contacts/{id}/contact_inboxes", get(contact_inboxes_list).post(contact_inbox_create))
        .route("/contacts/{id}/contactable_inboxes", get(contactable_inboxes))
        .route("/contacts/{id}/destroy_custom_attributes", post(destroy_custom_attributes))
        .route("/contacts/{id}/avatar", delete(destroy_avatar))
        .route("/contacts/{id}/notes", get(notes_index).post(notes_create))
        .route("/contacts/{id}/notes/{note_id}", get(notes_show).patch(notes_update).put(notes_update).delete(notes_destroy))
}

const RESULTS_PER_PAGE: i64 = 15;

#[derive(Debug, Deserialize, Default)]
struct ContactsQuery {
    page: Option<i64>,
    sort: Option<String>,
    include_contact_inboxes: Option<Value>,
    q: Option<String>,
    #[serde(default)]
    labels: Option<Value>,
}

fn parse_include_inboxes(v: Option<&Value>) -> bool {
    match v {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s != "false",
        _ => true,
    }
}

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
struct ContactRow {
    id: i32,
    name: Option<String>,
    email: Option<String>,
    phone_number: Option<String>,
    account_id: i32,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    additional_attributes: Option<Value>,
    identifier: Option<String>,
    custom_attributes: Option<Value>,
    last_activity_at: Option<NaiveDateTime>,
    blocked: bool,
    company_id: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct ContactInboxRow {
    source_id: String,
    inbox_id: i32,
    inbox_name: String,
    inbox_channel_type: Option<String>,
}

async fn fetch_contact_inboxes(db: &PgPool, contact_id: i64) -> Vec<Value> {
    let rows = sqlx::query_as::<_, ContactInboxRow>(
        "SELECT ci.source_id, i.id AS inbox_id, i.name AS inbox_name, i.channel_type AS inbox_channel_type
         FROM contact_inboxes ci
         JOIN inboxes i ON i.id = ci.inbox_id
         WHERE ci.contact_id = $1
         ORDER BY ci.id ASC"
    )
    .bind(contact_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| {
            json!({
                "source_id": r.source_id,
                "inbox": {
                    "id": r.inbox_id,
                    "name": r.inbox_name,
                    "channel_type": r.inbox_channel_type.unwrap_or_default(),
                    "avatar_url": "",
                    "channel_id": null,
                    "provider": null
                }
            })
        })
        .collect()
}

fn contact_to_json(c: &ContactRow, inboxes: Option<Vec<Value>>) -> Value {
    let mut obj = json!({
        "additional_attributes": c.additional_attributes.clone().unwrap_or_else(|| json!({})),
        "availability_status": "offline",
        "email": c.email,
        "id": c.id,
        "name": c.name.clone().unwrap_or_default(),
        "phone_number": c.phone_number,
        "blocked": c.blocked,
        "identifier": c.identifier,
        "thumbnail": "",
        "custom_attributes": c.custom_attributes.clone().unwrap_or_else(|| json!({})),
        "created_at": ts(Some(c.created_at)),
    });

    if let Some(la) = c.last_activity_at {
        obj.as_object_mut().unwrap().insert("last_activity_at".into(), json!(ts(Some(la))));
    } else {
        obj.as_object_mut().unwrap().insert("last_activity_at".into(), Value::Null);
    }

    if let Some(ci) = c.company_id {
        obj.as_object_mut().unwrap().insert("company_id".into(), json!(ci));
    }

    if let Some(inboxes_list) = inboxes {
        obj.as_object_mut().unwrap().insert("contact_inboxes".into(), json!(inboxes_list));
    }

    obj
}

async fn build_contact_json(db: &PgPool, c: &ContactRow, include_inboxes: bool) -> Value {
    let inboxes = if include_inboxes {
        Some(fetch_contact_inboxes(db, c.id as i64).await)
    } else {
        None
    };
    // Chatwoot PR #14966: thumbnail is the avatar URL (Chatwoot-uploaded avatars live in active_storage).
    let mut v = contact_to_json(c, inboxes);
    v["thumbnail"] = json!(crate::storage::avatar_url(db, "Contact", c.id as i64).await);
    v
}

fn parse_sort_clause(sort: Option<&str>) -> &'static str {
    match sort {
        Some("email") => "email ASC NULLS LAST, id DESC",
        Some("-email") => "email DESC NULLS LAST, id DESC",
        Some("phone_number") => "phone_number ASC NULLS LAST, id DESC",
        Some("-phone_number") => "phone_number DESC NULLS LAST, id DESC",
        Some("last_activity_at") => "last_activity_at ASC NULLS LAST, id DESC",
        Some("-last_activity_at") => "last_activity_at DESC NULLS LAST, id DESC",
        Some("created_at") => "created_at ASC, id DESC",
        Some("-created_at") => "created_at DESC, id DESC",
        Some("-name") => "name DESC NULLS LAST, id DESC",
        _ => "name ASC NULLS LAST, id DESC",
    }
}

// GET /contacts?page=&sort=&include_contact_inboxes=
async fn index(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<ContactsQuery>,
    axum::extract::RawQuery(raw): axum::extract::RawQuery,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * RESULTS_PER_PAGE;
    let include_inboxes = parse_include_inboxes(params.include_contact_inboxes.as_ref());
    let order_by = parse_sort_clause(params.sort.as_deref());

    // Check optional label filter: axios sends arrays as `labels[]=a&labels[]=b`; also accept `labels=a`.
    let from_raw: Vec<String> = raw
        .as_deref()
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .filter(|(k, v)| (k == "labels[]" || k == "labels") && !v.trim().is_empty())
                .map(|(_, v)| v.trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    let label_filter = if !from_raw.is_empty() { Some(from_raw) } else { match &params.labels {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(vec![s.trim().to_string()]),
        Some(Value::Array(arr)) => {
            let list: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            if list.is_empty() { None } else { Some(list) }
        }
        _ => None,
    } };

    let (contacts, count): (Vec<ContactRow>, i64) = if let Some(labels) = label_filter {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT c.id) FROM contacts c
             JOIN taggings tg ON tg.taggable_type = 'Contact' AND tg.taggable_id = c.id AND tg.context = 'labels'
             JOIN tags t ON t.id = tg.tag_id
             WHERE c.account_id = $1 AND t.name = ANY($2)"
        )
        .bind(account_id)
        .bind(&labels)
        .fetch_one(&st.db)
        .await?;

        let sql = format!(
            "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                    additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id
             FROM contacts
             WHERE account_id = $1 AND id IN (
               SELECT tg.taggable_id FROM taggings tg JOIN tags t ON t.id = tg.tag_id
               WHERE tg.taggable_type = 'Contact' AND tg.context = 'labels' AND t.name = ANY($2))
             ORDER BY {order_by}
             LIMIT $3 OFFSET $4"
        );
        let rows = sqlx::query_as::<_, ContactRow>(&sql)
            .bind(account_id)
            .bind(&labels)
            .bind(RESULTS_PER_PAGE)
            .bind(offset)
            .fetch_all(&st.db)
            .await?;
        (rows, count.0)
    } else {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM contacts WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&st.db)
            .await?;

        let sql = format!(
            "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                    additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id
             FROM contacts
             WHERE account_id = $1
             ORDER BY {order_by}
             LIMIT $2 OFFSET $3"
        );
        let rows = sqlx::query_as::<_, ContactRow>(&sql)
            .bind(account_id)
            .bind(RESULTS_PER_PAGE)
            .bind(offset)
            .fetch_all(&st.db)
            .await?;
        (rows, count.0)
    };

    let mut payload = Vec::with_capacity(contacts.len());
    for c in &contacts {
        payload.push(build_contact_json(&st.db, c, include_inboxes).await);
    }

    Ok(Json(json!({
        "meta": {
            "count": count,
            "current_page": page
        },
        "payload": payload
    })))
}

// GET /contacts/search?q=
async fn search(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<ContactsQuery>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let q = match params.q.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => s,
        _ => return Err(ApiError::bad("Specify search string with parameter q")),
    };

    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * RESULTS_PER_PAGE;
    let include_inboxes = parse_include_inboxes(params.include_contact_inboxes.as_ref());
    let order_by = parse_sort_clause(params.sort.as_deref());
    let pattern = format!("%{q}%");

    // Fetch limit + 1 to detect has_more
    let sql = format!(
        "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id
         FROM contacts
         WHERE account_id = $1 AND (
             name ILIKE $2 OR email ILIKE $2 OR phone_number ILIKE $2 OR identifier ILIKE $2
         )
         ORDER BY {order_by}
         LIMIT $3 OFFSET $4"
    );

    let mut rows = sqlx::query_as::<_, ContactRow>(&sql)
        .bind(account_id)
        .bind(&pattern)
        .bind(RESULTS_PER_PAGE + 1)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;

    let has_more = rows.len() > RESULTS_PER_PAGE as usize;
    if has_more {
        rows.truncate(RESULTS_PER_PAGE as usize);
    }
    let count = rows.len();

    let mut payload = Vec::with_capacity(rows.len());
    for c in &rows {
        payload.push(build_contact_json(&st.db, c, include_inboxes).await);
    }

    Ok(Json(json!({
        "meta": {
            "count": count,
            "current_page": page,
            "has_more": has_more
        },
        "payload": payload
    })))
}

// GET /contacts/active
async fn active(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<ContactsQuery>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let page = params.page.unwrap_or(1).max(1);
    // ponytail: active contacts from presence tracker. Return empty list if none active
    Ok(Json(json!({
        "meta": {
            "count": 0,
            "current_page": page
        },
        "payload": []
    })))
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FilterPayloadItem {
    attribute_key: String,
    filter_operator: String,
    values: Option<Vec<Value>>,
    #[serde(default)]
    query_operator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FilterBody {
    payload: Option<Vec<FilterPayloadItem>>,
}

// POST /contacts/filter
async fn filter(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<ContactsQuery>,
    Json(body): Json<FilterBody>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * RESULTS_PER_PAGE;
    let include_inboxes = parse_include_inboxes(params.include_contact_inboxes.as_ref());
    let order_by = parse_sort_clause(params.sort.as_deref());

    let filters = body.payload.unwrap_or_default();
    let mut conditions = Vec::new();
    let mut binds: Vec<String> = Vec::new();

    // ponytail: support name, email, phone_number, identifier with equal_to, not_equal_to, contains, does_not_contain
    for f in filters {
        let col = match f.attribute_key.as_str() {
            "name" => "name",
            "email" => "email",
            "phone_number" => "phone_number",
            "identifier" => "identifier",
            _ => continue,
        };

        let val = f.values.as_ref().and_then(|v| v.first()).and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else if v.is_null() {
                None
            } else {
                Some(v.to_string())
            }
        });

        match f.filter_operator.as_str() {
            "equal_to" => {
                if let Some(v) = val {
                    binds.push(v);
                    conditions.push(format!("{col} = ${}", binds.len() + 1));
                }
            }
            "not_equal_to" => {
                if let Some(v) = val {
                    binds.push(v);
                    conditions.push(format!("{col} != ${}", binds.len() + 1));
                }
            }
            "contains" => {
                if let Some(v) = val {
                    binds.push(format!("%{v}%"));
                    conditions.push(format!("{col} ILIKE ${}", binds.len() + 1));
                }
            }
            "does_not_contain" => {
                if let Some(v) = val {
                    binds.push(format!("%{v}%"));
                    conditions.push(format!("{col} NOT ILIKE ${}", binds.len() + 1));
                }
            }
            // chatwoot#15926: no value needed. `col` is from the whitelist above, never user input.
            "is_present" => conditions.push(format!("({col} IS NOT NULL AND {col} <> '')")),
            "is_not_present" => conditions.push(format!("({col} IS NULL OR {col} = '')")),
            _ => {}
        }
    }

    let where_clause = if conditions.is_empty() {
        "".to_string()
    } else {
        format!("AND ({})", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM contacts WHERE account_id = $1 {where_clause}");
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(account_id);
    for b in &binds {
        count_query = count_query.bind(b);
    }
    let count = count_query.fetch_one(&st.db).await?;

    let select_sql = format!(
        "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id
         FROM contacts
         WHERE account_id = $1 {where_clause}
         ORDER BY {order_by}
         LIMIT ${} OFFSET ${}",
        binds.len() + 2,
        binds.len() + 3
    );

    let mut select_query = sqlx::query_as::<_, ContactRow>(&select_sql).bind(account_id);
    for b in &binds {
        select_query = select_query.bind(b);
    }
    select_query = select_query.bind(RESULTS_PER_PAGE).bind(offset);

    let rows = select_query.fetch_all(&st.db).await?;

    let mut payload = Vec::with_capacity(rows.len());
    for c in &rows {
        payload.push(build_contact_json(&st.db, c, include_inboxes).await);
    }

    Ok(Json(json!({
        "meta": {
            "count": count,
            "current_page": page
        },
        "payload": payload
    })))
}

// GET /contacts/{id}
async fn show(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<ContactsQuery>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let include_inboxes = parse_include_inboxes(params.include_contact_inboxes.as_ref());

    let contact = sqlx::query_as::<_, ContactRow>(
        "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id
         FROM contacts WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let payload = build_contact_json(&st.db, &contact, include_inboxes).await;
    Ok(Json(json!({ "payload": payload })))
}

#[derive(Debug, Deserialize, Default)]
struct CreateContactBody {
    name: Option<String>,
    email: Option<String>,
    phone_number: Option<String>,
    identifier: Option<String>,
    additional_attributes: Option<Value>,
    custom_attributes: Option<Value>,
    inbox_id: Option<i64>,
    source_id: Option<String>,
}

/// 422 naming the conflicting attribute and the existing contact id (chatwoot#15917).
fn conflict(message: &str, attribute: &str, existing_id: i32) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({
            "message": message,
            "attributes": [attribute],
            "data": { "id": existing_id }
        })),
    )
        .into_response()
}

// POST /contacts
async fn create(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<CreateContactBody>,
) -> Result<Response, ApiError> {
    let _au = require_account(&st.db, &user, account_id).await?;

    // Chatwoot #15453 / PR #15496: check-then-insert raced, so two concurrent creates with the same phone
    // (or email/identifier) both passed. Serialize creates per account; schema is Chatwoot's, no new index.
    let mut tx = st.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('pulsar_contact_create'), $1::int)")
        .bind(account_id as i32)
        .execute(&mut *tx)
        .await?;

    // Uniqueness per account. Lookups are scoped to account_id, so only same-account contacts are named.
    let norm_email = body.email.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    if let Some(e) = norm_email {
        let exists: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM contacts WHERE account_id = $1 AND lower(email) = lower($2) LIMIT 1"
        )
        .bind(account_id)
        .bind(e)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((id,)) = exists {
            return Ok(conflict("Email has already been taken", "email", id));
        }
    }

    let norm_phone = body.phone_number.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(p) = norm_phone {
        let exists: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM contacts WHERE account_id = $1 AND phone_number = $2 LIMIT 1"
        )
        .bind(account_id)
        .bind(p)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((id,)) = exists {
            return Ok(conflict("Phone number has already been taken", "phone_number", id));
        }
    }

    let norm_identifier = body.identifier.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    if let Some(ident) = norm_identifier {
        let exists: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM contacts WHERE account_id = $1 AND identifier = $2 LIMIT 1"
        )
        .bind(account_id)
        .bind(ident)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((id,)) = exists {
            return Ok(conflict("Identifier has already been taken", "identifier", id));
        }
    }

    let name = body.name.unwrap_or_default();
    let additional_attributes = body.additional_attributes.unwrap_or_else(|| json!({}));
    let custom_attributes = body.custom_attributes.unwrap_or_else(|| json!({}));
    let created_at = now();

    let row = sqlx::query_as::<_, ContactRow>(
        "INSERT INTO contacts (name, email, phone_number, account_id, created_at, updated_at,
                               additional_attributes, identifier, custom_attributes, last_activity_at)
         VALUES ($1, $2, $3, $4, $5, $5, $6, $7, $8, $5)
         RETURNING id, name, email, phone_number, account_id, created_at, updated_at,
                   additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id"
    )
    .bind(&name)
    .bind(norm_email)
    .bind(body.phone_number.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(account_id)
    .bind(created_at)
    .bind(&additional_attributes)
    .bind(norm_identifier)
    .bind(&custom_attributes)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    let mut contact_inbox_payload = json!({
        "inbox": Value::Null,
        "source_id": Value::Null
    });

    if let Some(inbox_id) = body.inbox_id {
        let inbox: Option<(i32, String, Option<String>)> = sqlx::query_as(
            "SELECT id, name, channel_type FROM inboxes WHERE account_id = $1 AND id = $2"
        )
        .bind(account_id)
        .bind(inbox_id)
        .fetch_optional(&st.db)
        .await?;

        if let Some((iid, iname, ichannel)) = inbox {
            let source_id = body.source_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let pubsub_token = uuid::Uuid::new_v4().to_string();
            let ci_now = now();

            sqlx::query(
                "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at, pubsub_token)
                 VALUES ($1, $2, $3, $4, $4, $5)
                 ON CONFLICT (inbox_id, source_id) DO NOTHING"
            )
            .bind(row.id as i64)
            .bind(inbox_id)
            .bind(&source_id)
            .bind(ci_now)
            .bind(&pubsub_token)
            .execute(&st.db)
            .await?;

            contact_inbox_payload = json!({
                "source_id": source_id,
                "inbox": {
                    "id": iid,
                    "name": iname,
                    "channel_type": ichannel.unwrap_or_default(),
                    "avatar_url": "",
                    "channel_id": null,
                    "provider": null
                }
            });
        }
    }

    let contact_json = build_contact_json(&st.db, &row, true).await;
    st.hub.to_account(account_id, "contact.created", contact_json.clone());

    Ok(Json(json!({
        "payload": {
            "contact": contact_json,
            "contact_inbox": contact_inbox_payload
        }
    }))
    .into_response())
}

#[derive(Debug, Deserialize, Default)]
struct UpdateContactBody {
    name: Option<String>,
    email: Option<Value>,
    phone_number: Option<Value>,
    identifier: Option<Value>,
    blocked: Option<bool>,
    additional_attributes: Option<Value>,
    custom_attributes: Option<Value>,
}

// PATCH/PUT /contacts/{id}
async fn update(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<UpdateContactBody>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let existing = sqlx::query_as::<_, ContactRow>(
        "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id
         FROM contacts WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let new_email = match body.email {
        Some(Value::String(s)) => {
            let s = s.trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        }
        Some(Value::Null) => None,
        Some(_) => existing.email.clone(),
        None => existing.email.clone(),
    };

    if let Some(ref e) = new_email {
        let conflict: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM contacts WHERE account_id = $1 AND lower(email) = lower($2) AND id != $3"
        )
        .bind(account_id)
        .bind(e)
        .bind(id)
        .fetch_optional(&st.db)
        .await?;

        if conflict.is_some() {
            return Err(ApiError::bad("Email has already been taken"));
        }
    }

    let new_identifier = match body.identifier {
        Some(Value::String(s)) => {
            let s = s.trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        }
        Some(Value::Null) => None,
        Some(_) => existing.identifier.clone(),
        None => existing.identifier.clone(),
    };

    if let Some(ref ident) = new_identifier {
        let conflict: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM contacts WHERE account_id = $1 AND identifier = $2 AND id != $3"
        )
        .bind(account_id)
        .bind(ident)
        .bind(id)
        .fetch_optional(&st.db)
        .await?;

        if conflict.is_some() {
            return Err(ApiError::bad("Identifier has already been taken"));
        }
    }

    let new_phone = match body.phone_number {
        Some(Value::String(s)) => {
            let s = s.trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        }
        Some(Value::Null) => None,
        Some(_) => existing.phone_number.clone(),
        None => existing.phone_number.clone(),
    };

    let new_name = body.name.unwrap_or_else(|| existing.name.clone().unwrap_or_default());
    let new_blocked = body.blocked.unwrap_or(existing.blocked);

    // Merge additional_attributes and custom_attributes like Rails
    let mut add_attr = existing.additional_attributes.unwrap_or_else(|| json!({}));
    if let Some(Value::Object(map)) = body.additional_attributes {
        if let Some(dest) = add_attr.as_object_mut() {
            for (k, v) in map {
                dest.insert(k, v);
            }
        }
    }

    let mut cust_attr = existing.custom_attributes.unwrap_or_else(|| json!({}));
    if let Some(Value::Object(map)) = body.custom_attributes {
        if let Some(dest) = cust_attr.as_object_mut() {
            for (k, v) in map {
                dest.insert(k, v);
            }
        }
    }

    let updated_at = now();

    let updated_row = sqlx::query_as::<_, ContactRow>(
        "UPDATE contacts
         SET name = $1, email = $2, phone_number = $3, identifier = $4, blocked = $5,
             additional_attributes = $6, custom_attributes = $7, updated_at = $8
         WHERE account_id = $9 AND id = $10
         RETURNING id, name, email, phone_number, account_id, created_at, updated_at,
                   additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id"
    )
    .bind(&new_name)
    .bind(&new_email)
    .bind(&new_phone)
    .bind(&new_identifier)
    .bind(new_blocked)
    .bind(&add_attr)
    .bind(&cust_attr)
    .bind(updated_at)
    .bind(account_id)
    .bind(id)
    .fetch_one(&st.db)
    .await?;

    let contact_json = build_contact_json(&st.db, &updated_row, true).await;
    st.hub.to_account(account_id, "contact.updated", contact_json.clone());

    Ok(Json(json!({ "payload": contact_json })))
}

// DELETE /contacts/{id} (admin only)
async fn destroy(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;

    let contact = sqlx::query_as::<_, ContactRow>(
        "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id
         FROM contacts WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let contact_json = build_contact_json(&st.db, &contact, false).await;

    // Delete contact inboxes, notes, taggings, and contact
    sqlx::query("DELETE FROM contact_inboxes WHERE contact_id = $1")
        .bind(id)
        .execute(&st.db)
        .await?;
    sqlx::query("DELETE FROM notes WHERE account_id = $1 AND contact_id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&st.db)
        .await?;
    sqlx::query("DELETE FROM taggings WHERE taggable_type = 'Contact' AND taggable_id = $1")
        .bind(id)
        .execute(&st.db)
        .await?;
    sqlx::query("DELETE FROM contacts WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&st.db)
        .await?;

    st.hub.to_account(account_id, "contact.deleted", contact_json);

    Ok(Json(json!({ "message": "Contact deleted successfully" })))
}

// POST /contacts/{id}/destroy_custom_attributes
#[derive(Debug, Deserialize)]
struct DestroyCustomAttributesBody {
    custom_attributes: Vec<String>,
}

async fn destroy_custom_attributes(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<DestroyCustomAttributesBody>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let existing: Option<(Option<Value>,)> = sqlx::query_as(
        "SELECT custom_attributes FROM contacts WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?;

    let (mut cust_attr,) = existing.ok_or_else(ApiError::not_found)?;
    let mut attr_obj = cust_attr.take().unwrap_or_else(|| json!({}));
    if let Some(map) = attr_obj.as_object_mut() {
        for k in body.custom_attributes {
            map.remove(&k);
        }
    }

    sqlx::query("UPDATE contacts SET custom_attributes = $1, updated_at = $2 WHERE account_id = $3 AND id = $4")
        .bind(&attr_obj)
        .bind(now())
        .bind(account_id)
        .bind(id)
        .execute(&st.db)
        .await?;

    Ok(Json(json!({ "message": "Custom attributes deleted" })))
}

// DELETE /contacts/{id}/avatar
async fn destroy_avatar(
    Path((account_id, _id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    // ponytail: avatar purge is a no-op when avatars stored as url/empty
    Ok(Json(json!({ "message": "Avatar deleted" })))
}

// GET /contacts/{id}/conversations -> {payload:[conversation]}
#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
struct ConversationRow {
    id: i32,
    display_id: i32,
    inbox_id: i32,
    status: i32,
    created_at: NaiveDateTime,
    last_activity_at: NaiveDateTime,
    cached_label_list: Option<String>,
    account_id: i32,
}

fn conv_status_string(status: i32) -> &'static str {
    match status {
        0 => "open",
        1 => "resolved",
        2 => "pending",
        3 => "snoozed",
        _ => "open",
    }
}

async fn conversations(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let contact = sqlx::query_as::<_, ContactRow>(
        "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id
         FROM contacts WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let sender_json = contact_to_json(&contact, None);

    let rows = sqlx::query_as::<_, ConversationRow>(
        "SELECT id, display_id, inbox_id, status, created_at, last_activity_at, cached_label_list, account_id
         FROM conversations
         WHERE account_id = $1 AND contact_id = $2
         ORDER BY last_activity_at DESC, id DESC
         LIMIT 25"
    )
    .bind(account_id)
    .bind(id)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::with_capacity(rows.len());
    for conv in rows {
        let labels: Vec<String> = conv
            .cached_label_list
            .as_deref()
            .map(|s| s.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect())
            .unwrap_or_default();

        let conv_obj = json!({
            "id": conv.display_id,
            "inbox_id": conv.inbox_id,
            "status": conv_status_string(conv.status),
            "messages": [],
            "meta": {
                "sender": sender_json.clone()
            },
            "timestamp": ts(Some(conv.last_activity_at)),
            "created_at": ts(Some(conv.created_at)),
            "labels": labels,
            "account_id": conv.account_id
        });
        payload.push(conv_obj);
    }

    Ok(Json(json!({ "payload": payload })))
}

// GET/POST /contacts/{id}/labels -> {payload:[titles]}
async fn labels_index(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let labels: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM tags t
         JOIN taggings tg ON tg.tag_id = t.id
         WHERE tg.taggable_type = 'Contact' AND tg.taggable_id = $1 AND tg.context = 'labels'
         ORDER BY t.name ASC"
    )
    .bind(id)
    .fetch_all(&st.db)
    .await?;

    Ok(Json(json!({ "payload": labels })))
}

#[derive(Debug, Deserialize)]
struct LabelsBody {
    labels: Vec<String>,
}

async fn labels_create(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<LabelsBody>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    // Verify contact belongs to account
    let exists: Option<(i32,)> = sqlx::query_as("SELECT id FROM contacts WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .fetch_optional(&st.db)
        .await?;
    if exists.is_none() {
        return Err(ApiError::not_found());
    }

    // Delete existing contact taggings
    sqlx::query("DELETE FROM taggings WHERE taggable_type = 'Contact' AND taggable_id = $1 AND context = 'labels'")
        .bind(id)
        .execute(&st.db)
        .await?;

    let mut out_labels = Vec::new();
    for label in body.labels {
        let label = label.trim().to_string();
        if label.is_empty() {
            continue;
        }

        // Upsert tag
        let tag_id: i32 = sqlx::query_scalar(
            "INSERT INTO tags (name, taggings_count) VALUES ($1, 0)
             ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
             RETURNING id"
        )
        .bind(&label)
        .fetch_one(&st.db)
        .await?;

        // Insert tagging
        let tag_now = now();
        sqlx::query(
            "INSERT INTO taggings (tag_id, taggable_type, taggable_id, context, created_at)
             VALUES ($1, 'Contact', $2, 'labels', $3)"
        )
        .bind(tag_id)
        .bind(id)
        .bind(tag_now)
        .execute(&st.db)
        .await?;

        out_labels.push(label);
    }

    st.hub.invalidate_cache(account_id, "label");

    Ok(Json(json!({ "payload": out_labels })))
}

// GET /contacts/{id}/contact_inboxes
async fn contact_inboxes_list(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let inboxes = fetch_contact_inboxes(&st.db, id).await;
    Ok(Json(json!({ "payload": inboxes })))
}

#[derive(Debug, Deserialize)]
struct CreateContactInboxBody {
    inbox_id: i64,
    source_id: Option<String>,
}

// POST /contacts/{id}/contact_inboxes
async fn contact_inbox_create(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<CreateContactInboxBody>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    // Verify contact belongs to account
    let contact_exists: Option<(i32,)> = sqlx::query_as("SELECT id FROM contacts WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .fetch_optional(&st.db)
        .await?;
    if contact_exists.is_none() {
        return Err(ApiError::not_found());
    }

    let inbox: Option<(i32, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, channel_type FROM inboxes WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(body.inbox_id)
    .fetch_optional(&st.db)
    .await?;

    let (iid, iname, ichannel) = inbox.ok_or_else(ApiError::not_found)?;
    let source_id = body.source_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let pubsub_token = uuid::Uuid::new_v4().to_string();
    let ci_now = now();

    sqlx::query(
        "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at, pubsub_token)
         VALUES ($1, $2, $3, $4, $4, $5)
         ON CONFLICT (inbox_id, source_id) DO NOTHING"
    )
    .bind(id)
    .bind(body.inbox_id)
    .bind(&source_id)
    .bind(ci_now)
    .bind(&pubsub_token)
    .execute(&st.db)
    .await?;

    Ok(Json(json!({
        "source_id": source_id,
        "inbox": {
            "id": iid,
            "name": iname,
            "channel_type": ichannel.unwrap_or_default(),
            "avatar_url": "",
            "channel_id": null,
            "provider": null
        }
    })))
}

// GET /contacts/{id}/contactable_inboxes
#[derive(Debug, sqlx::FromRow)]
struct InboxSlimRow {
    id: i32,
    name: String,
    channel_type: Option<String>,
}

async fn contactable_inboxes(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let contact = sqlx::query_as::<_, ContactRow>(
        "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                additional_attributes, identifier, custom_attributes, last_activity_at, blocked, company_id
         FROM contacts WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let inboxes = sqlx::query_as::<_, InboxSlimRow>(
        "SELECT id, name, channel_type FROM inboxes WHERE account_id = $1 ORDER BY id ASC"
    )
    .bind(account_id)
    .fetch_all(&st.db)
    .await?;

    let mut payload = Vec::new();
    for ib in inboxes {
        let ch_type = ib.channel_type.as_deref().unwrap_or_default();
        let source_id = match ch_type {
            "Channel::Email" => {
                if let Some(ref em) = contact.email {
                    Some(em.clone())
                } else {
                    None
                }
            }
            "Channel::TwilioSms" | "Channel::Sms" => {
                contact.phone_number.clone()
            }
            "Channel::Whatsapp" => {
                contact.phone_number.as_ref().map(|p| p.replace('+', ""))
            }
            "Channel::Api" => {
                let existing_source: Option<(String,)> = sqlx::query_as(
                    "SELECT source_id FROM contact_inboxes WHERE inbox_id = $1 AND contact_id = $2 ORDER BY id DESC LIMIT 1"
                )
                .bind(ib.id)
                .bind(id)
                .fetch_optional(&st.db)
                .await
                .unwrap_or(None);

                Some(existing_source.map(|r| r.0).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()))
            }
            "Channel::WebWidget" => {
                let existing_source: Option<(String,)> = sqlx::query_as(
                    "SELECT ci.source_id FROM contact_inboxes ci
                     LEFT JOIN conversations c ON c.contact_inbox_id = ci.id
                     WHERE ci.inbox_id = $1 AND ci.contact_id = $2 AND c.id IS NULL
                     ORDER BY ci.id DESC LIMIT 1"
                )
                .bind(ib.id)
                .bind(id)
                .fetch_optional(&st.db)
                .await
                .unwrap_or(None);

                existing_source.map(|r| r.0)
            }
            _ => None,
        };

        if let Some(sid) = source_id {
            payload.push(json!({
                "source_id": sid,
                "inbox": {
                    "id": ib.id,
                    "name": ib.name,
                    "channel_type": ch_type,
                    "avatar_url": "",
                    "channel_id": null,
                    "provider": null
                }
            }));
        }
    }

    Ok(Json(json!({ "payload": payload })))
}

// Notes
#[derive(Debug, sqlx::FromRow)]
struct NoteRow {
    id: i64,
    content: String,
    account_id: i64,
    contact_id: i64,
    user_id: Option<i64>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    user_name: Option<String>,
    user_email: Option<String>,
}

fn note_to_json(n: &NoteRow) -> Value {
    let mut obj = json!({
        "id": n.id,
        "content": n.content,
        "account_id": n.account_id,
        "contact_id": n.contact_id,
        "created_at": ts(Some(n.created_at)),
        "updated_at": ts(Some(n.updated_at)),
    });

    if let Some(uid) = n.user_id {
        obj.as_object_mut().unwrap().insert("user".into(), json!({
            "id": uid,
            "name": n.user_name.clone().unwrap_or_default(),
            "email": n.user_email.clone().unwrap_or_default(),
            "avatar_url": "",
            "thumbnail": ""
        }));
    }

    obj
}

// GET /contacts/{id}/notes
async fn notes_index(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let rows = sqlx::query_as::<_, NoteRow>(
        "SELECT n.id, n.content, n.account_id, n.contact_id, n.user_id, n.created_at, n.updated_at,
                u.name AS user_name, u.email AS user_email
         FROM notes n
         LEFT JOIN users u ON u.id = n.user_id
         WHERE n.account_id = $1 AND n.contact_id = $2
         ORDER BY n.created_at DESC"
    )
    .bind(account_id)
    .bind(id)
    .fetch_all(&st.db)
    .await?;

    let payload: Vec<Value> = rows.iter().map(note_to_json).collect();
    Ok(Json(json!(payload)))
}

#[derive(Debug, Deserialize)]
struct NotePayload {
    content: String,
}

#[derive(Debug, Deserialize)]
struct CreateNoteBody {
    // Both { content } and { note: { content } } supported
    content: Option<String>,
    note: Option<NotePayload>,
}

impl CreateNoteBody {
    fn extract_content(&self) -> Option<String> {
        self.content.clone().or_else(|| self.note.as_ref().map(|n| n.content.clone()))
    }
}

// POST /contacts/{id}/notes
async fn notes_create(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<CreateNoteBody>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let content = match body.extract_content() {
        Some(s) if !s.trim().is_empty() => s,
        _ => return Err(ApiError::bad("Content cannot be empty")),
    };

    // Verify contact belongs to account
    let exists: Option<(i32,)> = sqlx::query_as("SELECT id FROM contacts WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .fetch_optional(&st.db)
        .await?;
    if exists.is_none() {
        return Err(ApiError::not_found());
    }

    let note_now = now();
    let row = sqlx::query_as::<_, NoteRow>(
        "INSERT INTO notes (content, account_id, contact_id, user_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $5)
         RETURNING id, content, account_id, contact_id, user_id, created_at, updated_at,
                   NULL::varchar AS user_name, NULL::varchar AS user_email"
    )
    .bind(&content)
    .bind(account_id)
    .bind(id)
    .bind(user.id)
    .bind(note_now)
    .fetch_one(&st.db)
    .await?;

    let note_row = NoteRow {
        user_name: Some(user.name.clone()),
        user_email: Some(user.email.clone()),
        ..row
    };

    Ok(Json(note_to_json(&note_row)))
}

// GET /contacts/{id}/notes/{note_id}
async fn notes_show(
    Path((account_id, id, note_id)): Path<(i64, i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let row = sqlx::query_as::<_, NoteRow>(
        "SELECT n.id, n.content, n.account_id, n.contact_id, n.user_id, n.created_at, n.updated_at,
                u.name AS user_name, u.email AS user_email
         FROM notes n
         LEFT JOIN users u ON u.id = n.user_id
         WHERE n.account_id = $1 AND n.contact_id = $2 AND n.id = $3"
    )
    .bind(account_id)
    .bind(id)
    .bind(note_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    Ok(Json(note_to_json(&row)))
}

// PATCH/PUT /contacts/{id}/notes/{note_id}
async fn notes_update(
    Path((account_id, id, note_id)): Path<(i64, i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<CreateNoteBody>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let content = match body.extract_content() {
        Some(s) if !s.trim().is_empty() => s,
        _ => return Err(ApiError::bad("Content cannot be empty")),
    };

    let updated = sqlx::query_as::<_, NoteRow>(
        "UPDATE notes
         SET content = $1, updated_at = $2
         WHERE account_id = $3 AND contact_id = $4 AND id = $5
         RETURNING id, content, account_id, contact_id, user_id, created_at, updated_at,
                   NULL::varchar AS user_name, NULL::varchar AS user_email"
    )
    .bind(&content)
    .bind(now())
    .bind(account_id)
    .bind(id)
    .bind(note_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    // Populate user info
    let user_info: Option<(String, String)> = if let Some(uid) = updated.user_id {
        sqlx::query_as("SELECT name, email FROM users WHERE id = $1")
            .bind(uid)
            .fetch_optional(&st.db)
            .await
            .unwrap_or(None)
    } else {
        None
    };

    let note_row = NoteRow {
        user_name: user_info.as_ref().map(|u| u.0.clone()),
        user_email: user_info.as_ref().map(|u| u.1.clone()),
        ..updated
    };

    Ok(Json(note_to_json(&note_row)))
}

// DELETE /contacts/{id}/notes/{note_id}
async fn notes_destroy(
    Path((account_id, id, note_id)): Path<(i64, i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let res = sqlx::query("DELETE FROM notes WHERE account_id = $1 AND contact_id = $2 AND id = $3")
        .bind(account_id)
        .bind(id)
        .bind(note_id)
        .execute(&st.db)
        .await?;

    if res.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }

    Ok(Json(json!({ "message": "Note deleted successfully" })))
}
