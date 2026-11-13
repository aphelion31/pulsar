//! OWNED BY A WORKER. See WORKERS.md for the contract.
//! Routes here are mounted at /api/v1/accounts/{account_id}.
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::common::{now, require_account, ts, ApiError, ApiResult, AppState, CurrentUser};

pub fn routes() -> Router<AppState> {
    Router::new()
        // Companies
        .route("/companies", get(companies_index).post(companies_create))
        .route("/companies/search", get(companies_search))
        .route(
            "/companies/{id}",
            get(companies_show)
                .patch(companies_update)
                .put(companies_update)
                .delete(companies_destroy),
        )
        .route("/companies/{id}/avatar", delete(companies_avatar_destroy))
        .route("/companies/{id}/conversations", get(company_conversations))
        .route("/companies/{id}/conversations/filter", post(company_conversations_filter))
        .route("/companies/{id}/notes", get(company_notes))
        .route(
            "/companies/{id}/destroy_custom_attributes",
            post(companies_destroy_custom_attributes),
        )
        .route(
            "/companies/{id}/contacts",
            get(company_contacts_index).post(company_contacts_create),
        )
        .route(
            "/companies/{id}/contacts/search",
            get(company_contacts_search),
        )
        .route(
            "/companies/{id}/contacts/{contact_id}",
            delete(company_contacts_destroy),
        )
        // Audit Logs (admin only)
        .route("/audit_logs", get(audit_logs_index))
        // Custom Roles (admin only for mutations)
        .route(
            "/custom_roles",
            get(custom_roles_index).post(custom_roles_create),
        )
        .route(
            "/custom_roles/{id}",
            get(custom_roles_show)
                .patch(custom_roles_update)
                .put(custom_roles_update)
                .delete(custom_roles_destroy),
        )
}

// ============================================================================
// Companies Models & Helpers
// ============================================================================

const RESULTS_PER_PAGE: i64 = 50;
const CONTACTS_PER_PAGE: i64 = 25;
const AUDITS_PER_PAGE: i64 = 25;

async fn company_conversations_filter(
    path: Path<(i64, i64)>, user: CurrentUser, state: State<AppState>,
    query: Query<CompaniesQuery>, Json(body): Json<Value>,
) -> ApiResult {
    require_account(&state.0.db, &user, path.0.0).await?;
    if body.get("payload").and_then(Value::as_array).is_some_and(|v| !v.is_empty()) {
        return Err(ApiError::bad("Conversation filters are not supported"));
    }
    company_conversations(path, user, state, query).await
}

async fn company_conversations(
    Path((account_id, company_id)): Path<(i64, i64)>, user: CurrentUser,
    State(st): State<AppState>, Query(params): Query<CompaniesQuery>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM companies WHERE account_id=$1 AND id=$2)").bind(account_id).bind(company_id).fetch_one(&st.db).await?;
    if !exists { return Err(ApiError::not_found()); }
    let scope = au.custom_scope_sql("$3");
    let base = format!("FROM conversations c JOIN contacts ct ON ct.id=c.contact_id WHERE c.account_id=$1 AND ct.account_id=$1 AND ct.company_id=$2 AND ($4 OR c.inbox_id IN (SELECT inbox_id FROM inbox_members WHERE user_id=$3)) AND ({scope})");
    let (all_count, open_count): (i64, i64) = sqlx::query_as(&format!("SELECT count(*), count(*) FILTER (WHERE c.status=0) {base}"))
        .bind(account_id).bind(company_id).bind(user.id).bind(au.is_admin()).fetch_one(&st.db).await?;
    let page = params.page.unwrap_or(1).max(1);
    let rows: Vec<crate::conversations::Conv> = sqlx::query_as(&format!("SELECT {} {base} ORDER BY c.last_activity_at DESC, c.id DESC LIMIT 25 OFFSET $5", crate::conversations::CONV_COLS))
        .bind(account_id).bind(company_id).bind(user.id).bind(au.is_admin()).bind((page-1)*25).fetch_all(&st.db).await?;
    let mut payload = Vec::new();
    for row in &rows { payload.push(crate::conversations::conv_json(&st.db, row).await); }
    Ok(Json(json!({"meta":{"total_count":all_count,"all_count":all_count,"open_count":open_count,"page":page},"payload":payload})))
}

async fn company_notes(
    Path((account_id, company_id)): Path<(i64, i64)>, user: CurrentUser,
    State(st): State<AppState>, Query(params): Query<CompaniesQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM companies WHERE account_id=$1 AND id=$2)").bind(account_id).bind(company_id).fetch_one(&st.db).await?;
    if !exists { return Err(ApiError::not_found()); }
    let q = params.q.unwrap_or_default().trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let pattern = format!("%{q}%");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM notes n JOIN contacts c ON c.id=n.contact_id WHERE n.account_id=$1 AND c.account_id=$1 AND c.company_id=$2 AND n.content ILIKE $3")
        .bind(account_id).bind(company_id).bind(&pattern).fetch_one(&st.db).await?;
    let page = params.page.unwrap_or(1).max(1);
    let rows: Vec<(i64, String, i64, Option<i64>, NaiveDateTime, NaiveDateTime)> = sqlx::query_as("SELECT n.id,n.content,n.contact_id,n.user_id,n.created_at,n.updated_at FROM notes n JOIN contacts c ON c.id=n.contact_id WHERE n.account_id=$1 AND c.account_id=$1 AND c.company_id=$2 AND n.content ILIKE $3 ORDER BY n.created_at DESC LIMIT 20 OFFSET $4")
        .bind(account_id).bind(company_id).bind(pattern).bind((page-1)*20).fetch_all(&st.db).await?;
    let mut payload = Vec::new();
    for (id, content, contact_id, user_id, created_at, updated_at) in rows {
        let mut item = json!({"id":id,"content":content,"account_id":account_id,"contact_id":contact_id,"created_at":ts(Some(created_at)),"updated_at":ts(Some(updated_at)),"contact":crate::conversations::contact_json(&st.db, Some(contact_id)).await});
        if let Some(uid) = user_id { item["user"] = crate::conversations::user_json(&st.db, account_id, uid).await; }
        payload.push(item);
    }
    Ok(Json(json!({"meta":{"total_count":count,"page":page},"payload":payload})))
}

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
struct CompanyRow {
    id: i64,
    name: String,
    domain: Option<String>,
    description: Option<String>,
    account_id: i64,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    contacts_count: Option<i32>,
    additional_attributes: Option<Value>,
    custom_attributes: Option<Value>,
    last_activity_at: Option<NaiveDateTime>,
}

fn company_to_json(c: &CompanyRow) -> Value {
    let mut obj = json!({
        "id": c.id,
        "name": c.name,
        "contacts_count": c.contacts_count.unwrap_or(0),
        "domain": c.domain,
        "description": c.description,
        "additional_attributes": c.additional_attributes.clone().unwrap_or_else(|| json!({})),
        "custom_attributes": c.custom_attributes.clone().unwrap_or_else(|| json!({})),
        "avatar_url": "",
        "created_at": ts(Some(c.created_at)),
        "updated_at": ts(Some(c.updated_at)),
    });

    if let Some(la) = c.last_activity_at {
        obj.as_object_mut()
            .unwrap()
            .insert("last_activity_at".into(), json!(ts(Some(la))));
    }
    obj
}

#[derive(Debug, Deserialize, Default)]
struct CompaniesQuery {
    page: Option<i64>,
    sort: Option<String>,
    q: Option<String>,
}

fn parse_company_sort(sort: Option<&str>) -> &'static str {
    match sort {
        Some("-name") => "name DESC NULLS LAST",
        Some("name") => "name ASC NULLS LAST",
        Some("-domain") => "domain DESC NULLS LAST",
        Some("domain") => "domain ASC NULLS LAST",
        Some("-created_at") => "created_at DESC NULLS LAST",
        Some("created_at") => "created_at ASC NULLS LAST",
        Some("-last_activity_at") => "last_activity_at DESC NULLS LAST",
        Some("last_activity_at") => "last_activity_at ASC NULLS LAST",
        Some("-contacts_count") => "contacts_count DESC NULLS LAST",
        Some("contacts_count") => "contacts_count ASC NULLS LAST",
        _ => "name ASC NULLS LAST",
    }
}

// GET /companies?page=&sort=&q=
async fn companies_index(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<CompaniesQuery>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * RESULTS_PER_PAGE;
    let order_by = parse_company_sort(params.sort.as_deref());

    let (rows, total): (Vec<CompanyRow>, i64) = if let Some(ref q) = params.q {
        let q_trimmed = q.trim();
        if q_trimmed.is_empty() {
            let total: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM companies WHERE account_id = $1",
            )
            .bind(account_id)
            .fetch_one(&st.db)
            .await?;

            let sql = format!(
                "SELECT id, name, domain, description, account_id, created_at, updated_at,
                        contacts_count, additional_attributes, custom_attributes, last_activity_at
                 FROM companies
                 WHERE account_id = $1
                 ORDER BY {order_by}
                 LIMIT $2 OFFSET $3"
            );
            let rows: Vec<CompanyRow> = sqlx::query_as(&sql)
                .bind(account_id)
                .bind(RESULTS_PER_PAGE)
                .bind(offset)
                .fetch_all(&st.db)
                .await?;
            (rows, total.0)
        } else {
            let search = format!("%{}%", q_trimmed);
            let total: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM companies WHERE account_id = $1 AND (name ILIKE $2 OR domain ILIKE $2)",
            )
            .bind(account_id)
            .bind(&search)
            .fetch_one(&st.db)
            .await?;

            let sql = format!(
                "SELECT id, name, domain, description, account_id, created_at, updated_at,
                        contacts_count, additional_attributes, custom_attributes, last_activity_at
                 FROM companies
                 WHERE account_id = $1 AND (name ILIKE $2 OR domain ILIKE $2)
                 ORDER BY {order_by}
                 LIMIT $3 OFFSET $4"
            );
            let rows: Vec<CompanyRow> = sqlx::query_as(&sql)
                .bind(account_id)
                .bind(&search)
                .bind(RESULTS_PER_PAGE)
                .bind(offset)
                .fetch_all(&st.db)
                .await?;
            (rows, total.0)
        }
    } else {
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM companies WHERE account_id = $1",
        )
        .bind(account_id)
        .fetch_one(&st.db)
        .await?;

        let sql = format!(
            "SELECT id, name, domain, description, account_id, created_at, updated_at,
                    contacts_count, additional_attributes, custom_attributes, last_activity_at
             FROM companies
             WHERE account_id = $1
             ORDER BY {order_by}
             LIMIT $2 OFFSET $3"
        );
        let rows: Vec<CompanyRow> = sqlx::query_as(&sql)
            .bind(account_id)
            .bind(RESULTS_PER_PAGE)
            .bind(offset)
            .fetch_all(&st.db)
            .await?;
        (rows, total.0)
    };

    let payload: Vec<Value> = rows.iter().map(company_to_json).collect();
    Ok(Json(json!({
        "payload": payload,
        "meta": {
            "total_count": total,
            "page": page,
        }
    })))
}

// GET /companies/search?q=
async fn companies_search(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<CompaniesQuery>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let q = params.q.unwrap_or_default();
    let q_trimmed = q.trim();
    if q_trimmed.is_empty() {
        return Err(ApiError::bad("Specify search string with parameter q"));
    }

    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * RESULTS_PER_PAGE;
    let order_by = parse_company_sort(params.sort.as_deref());
    let search = format!("%{}%", q_trimmed);

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM companies WHERE account_id = $1 AND (name ILIKE $2 OR domain ILIKE $2)",
    )
    .bind(account_id)
    .bind(&search)
    .fetch_one(&st.db)
    .await?;

    let sql = format!(
        "SELECT id, name, domain, description, account_id, created_at, updated_at,
                contacts_count, additional_attributes, custom_attributes, last_activity_at
         FROM companies
         WHERE account_id = $1 AND (name ILIKE $2 OR domain ILIKE $2)
         ORDER BY {order_by}
         LIMIT $3 OFFSET $4"
    );
    let rows: Vec<CompanyRow> = sqlx::query_as(&sql)
        .bind(account_id)
        .bind(&search)
        .bind(RESULTS_PER_PAGE)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;

    let payload: Vec<Value> = rows.iter().map(company_to_json).collect();
    Ok(Json(json!({
        "payload": payload,
        "meta": {
            "total_count": total.0,
            "page": page,
        }
    })))
}

// GET /companies/{id}
async fn companies_show(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let row: Option<CompanyRow> = sqlx::query_as(
        "SELECT id, name, domain, description, account_id, created_at, updated_at,
                contacts_count, additional_attributes, custom_attributes, last_activity_at
         FROM companies
         WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?;

    let row = row.ok_or_else(ApiError::not_found)?;
    Ok(Json(json!({ "payload": company_to_json(&row) })))
}

#[derive(Debug, Deserialize, Default)]
struct CompanyInput {
    name: Option<String>,
    domain: Option<String>,
    description: Option<String>,
    additional_attributes: Option<Value>,
    custom_attributes: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct CompanyPayloadWrapper {
    company: Option<CompanyInput>,
    name: Option<String>,
    domain: Option<String>,
    description: Option<String>,
    additional_attributes: Option<Value>,
    custom_attributes: Option<Value>,
}

impl CompanyPayloadWrapper {
    fn extract(self) -> CompanyInput {
        if let Some(c) = self.company {
            c
        } else {
            CompanyInput {
                name: self.name,
                domain: self.domain,
                description: self.description,
                additional_attributes: self.additional_attributes,
                custom_attributes: self.custom_attributes,
            }
        }
    }
}

// POST /companies
async fn companies_create(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<CompanyPayloadWrapper>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let input = body.extract();
    let name = input.name.map(|s| s.trim().to_string()).unwrap_or_default();
    if name.is_empty() {
        return Err(ApiError::bad("Name is required"));
    }

    let domain = input
        .domain
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());

    if let Some(ref d) = domain {
        let exists: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM companies WHERE account_id = $1 AND domain = $2",
        )
        .bind(account_id)
        .bind(d)
        .fetch_optional(&st.db)
        .await?;
        if exists.is_some() {
            return Err(ApiError::bad("Domain has already been taken"));
        }
    }

    let description = input.description.map(|s| s.trim().to_string());
    let additional_attributes = input.additional_attributes.unwrap_or_else(|| json!({}));
    let custom_attributes = input.custom_attributes.unwrap_or_else(|| json!({}));
    let current_time = now();

    let row: CompanyRow = sqlx::query_as(
        "INSERT INTO companies (
            name, domain, description, account_id, created_at, updated_at,
            contacts_count, additional_attributes, custom_attributes
         )
         VALUES ($1, $2, $3, $4, $5, $5, 0, $6, $7)
         RETURNING id, name, domain, description, account_id, created_at, updated_at,
                   contacts_count, additional_attributes, custom_attributes, last_activity_at",
    )
    .bind(name)
    .bind(domain)
    .bind(description)
    .bind(account_id)
    .bind(current_time)
    .bind(additional_attributes)
    .bind(custom_attributes)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(json!({ "payload": company_to_json(&row) })))
}

// PATCH/PUT /companies/{id}
async fn companies_update(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<CompanyPayloadWrapper>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let input = body.extract();

    let existing: Option<CompanyRow> = sqlx::query_as(
        "SELECT id, name, domain, description, account_id, created_at, updated_at,
                contacts_count, additional_attributes, custom_attributes, last_activity_at
         FROM companies
         WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?;
    let existing = existing.ok_or_else(ApiError::not_found)?;

    let name = input
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(existing.name);

    let domain = if let Some(d) = input.domain {
        let trimmed = d.trim().to_lowercase();
        if trimmed.is_empty() {
            None
        } else {
            let exists: Option<(i64,)> = sqlx::query_as(
                "SELECT id FROM companies WHERE account_id = $1 AND domain = $2 AND id <> $3",
            )
            .bind(account_id)
            .bind(&trimmed)
            .bind(id)
            .fetch_optional(&st.db)
            .await?;
            if exists.is_some() {
                return Err(ApiError::bad("Domain has already been taken"));
            }
            Some(trimmed)
        }
    } else {
        existing.domain
    };

    let description = input.description.or(existing.description);

    let mut add_attr = existing.additional_attributes.unwrap_or_else(|| json!({}));
    if let Some(new_add) = input.additional_attributes {
        if let (Some(base_obj), Some(new_obj)) = (add_attr.as_object_mut(), new_add.as_object()) {
            for (k, v) in new_obj {
                base_obj.insert(k.clone(), v.clone());
            }
        } else {
            add_attr = new_add;
        }
    }

    let mut cust_attr = existing.custom_attributes.unwrap_or_else(|| json!({}));
    if let Some(new_cust) = input.custom_attributes {
        if let (Some(base_obj), Some(new_obj)) = (cust_attr.as_object_mut(), new_cust.as_object()) {
            for (k, v) in new_obj {
                base_obj.insert(k.clone(), v.clone());
            }
        } else {
            cust_attr = new_cust;
        }
    }

    let current_time = now();
    let row: CompanyRow = sqlx::query_as(
        "UPDATE companies
         SET name = $1, domain = $2, description = $3, additional_attributes = $4,
             custom_attributes = $5, updated_at = $6
         WHERE account_id = $7 AND id = $8
         RETURNING id, name, domain, description, account_id, created_at, updated_at,
                   contacts_count, additional_attributes, custom_attributes, last_activity_at",
    )
    .bind(name)
    .bind(domain)
    .bind(description)
    .bind(add_attr)
    .bind(cust_attr)
    .bind(current_time)
    .bind(account_id)
    .bind(id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(json!({ "payload": company_to_json(&row) })))
}

// DELETE /companies/{id} (Admin only)
async fn companies_destroy(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;

    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM companies WHERE account_id = $1 AND id = $2")
            .bind(account_id)
            .bind(id)
            .fetch_optional(&st.db)
            .await?;
    if existing.is_none() {
        return Err(ApiError::not_found());
    }

    // nullify company_id in contacts
    sqlx::query("UPDATE contacts SET company_id = NULL WHERE account_id = $1 AND company_id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&st.db)
        .await?;

    sqlx::query("DELETE FROM companies WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&st.db)
        .await?;

    Ok(Json(json!({ "ok": true })))
}

// DELETE /companies/{id}/avatar
async fn companies_avatar_destroy(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;
    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM companies WHERE account_id = $1 AND id = $2")
            .bind(account_id)
            .bind(id)
            .fetch_optional(&st.db)
            .await?;
    if existing.is_none() {
        return Err(ApiError::not_found());
    }
    crate::storage::remove_avatar(&st.db, "Company", id).await?;
    Ok(Json(json!({ "message": "Avatar deleted" })))
}

#[derive(Debug, Deserialize)]
struct DestroyCustomAttributesBody {
    custom_attributes: Vec<String>,
}

// POST /companies/{id}/destroy_custom_attributes
async fn companies_destroy_custom_attributes(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<DestroyCustomAttributesBody>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let row: Option<CompanyRow> = sqlx::query_as(
        "SELECT id, name, domain, description, account_id, created_at, updated_at,
                contacts_count, additional_attributes, custom_attributes, last_activity_at
         FROM companies
         WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?;
    let row = row.ok_or_else(ApiError::not_found)?;

    let mut cust = row.custom_attributes.unwrap_or_else(|| json!({}));
    if let Some(obj) = cust.as_object_mut() {
        for key in &body.custom_attributes {
            obj.remove(key);
        }
    }

    let updated: CompanyRow = sqlx::query_as(
        "UPDATE companies
         SET custom_attributes = $1, updated_at = $2
         WHERE account_id = $3 AND id = $4
         RETURNING id, name, domain, description, account_id, created_at, updated_at,
                   contacts_count, additional_attributes, custom_attributes, last_activity_at",
    )
    .bind(cust)
    .bind(now())
    .bind(account_id)
    .bind(id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(json!({ "payload": company_to_json(&updated) })))
}

// ============================================================================
// Company Contacts
// ============================================================================

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
struct ContactItemRow {
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

fn contact_item_to_json(
    c: &ContactItemRow,
    current_company_id: i64,
    company: Option<&CompanyRow>,
) -> Value {
    let mut obj = json!({
        "id": c.id,
        "name": c.name.clone().unwrap_or_default(),
        "email": c.email,
        "phone_number": c.phone_number,
        "blocked": c.blocked,
        "identifier": c.identifier,
        "thumbnail": "",
        "additional_attributes": c.additional_attributes.clone().unwrap_or_else(|| json!({})),
        "custom_attributes": c.custom_attributes.clone().unwrap_or_else(|| json!({})),
        "created_at": ts(Some(c.created_at)),
        "company_id": c.company_id,
        "linked_to_current_company": c.company_id == Some(current_company_id),
        "company": company.map(company_to_json).unwrap_or(Value::Null),
    });

    if let Some(la) = c.last_activity_at {
        obj.as_object_mut()
            .unwrap()
            .insert("last_activity_at".into(), json!(ts(Some(la))));
    } else {
        obj.as_object_mut()
            .unwrap()
            .insert("last_activity_at".into(), Value::Null);
    }

    obj
}

#[derive(Debug, Deserialize, Default)]
struct CompanyContactsQuery {
    page: Option<i64>,
    q: Option<String>,
}

// GET /companies/{id}/contacts?page=&q=
async fn company_contacts_index(
    Path((account_id, company_id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<CompanyContactsQuery>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let company: Option<CompanyRow> = sqlx::query_as(
        "SELECT id, name, domain, description, account_id, created_at, updated_at,
                contacts_count, additional_attributes, custom_attributes, last_activity_at
         FROM companies
         WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(company_id)
    .fetch_optional(&st.db)
    .await?;
    let company = company.ok_or_else(ApiError::not_found)?;

    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * CONTACTS_PER_PAGE;

    let (rows, total): (Vec<ContactItemRow>, i64) = if let Some(ref q) = params.q {
        let q_trimmed = q.trim();
        if q_trimmed.is_empty() {
            let total: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM contacts WHERE account_id = $1 AND company_id = $2",
            )
            .bind(account_id)
            .bind(company_id)
            .fetch_one(&st.db)
            .await?;

            let rows: Vec<ContactItemRow> = sqlx::query_as(
                "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                        additional_attributes, identifier, custom_attributes, last_activity_at,
                        blocked, company_id
                 FROM contacts
                 WHERE account_id = $1 AND company_id = $2
                 ORDER BY name ASC NULLS LAST, id ASC
                 LIMIT $3 OFFSET $4",
            )
            .bind(account_id)
            .bind(company_id)
            .bind(CONTACTS_PER_PAGE)
            .bind(offset)
            .fetch_all(&st.db)
            .await?;
            (rows, total.0)
        } else {
            let search = format!("%{}%", q_trimmed);
            let total: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM contacts
                 WHERE account_id = $1 AND company_id = $2
                   AND (name ILIKE $3 OR email ILIKE $3 OR phone_number ILIKE $3 OR identifier ILIKE $3)",
            )
            .bind(account_id)
            .bind(company_id)
            .bind(&search)
            .fetch_one(&st.db)
            .await?;

            let rows: Vec<ContactItemRow> = sqlx::query_as(
                "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                        additional_attributes, identifier, custom_attributes, last_activity_at,
                        blocked, company_id
                 FROM contacts
                 WHERE account_id = $1 AND company_id = $2
                   AND (name ILIKE $3 OR email ILIKE $3 OR phone_number ILIKE $3 OR identifier ILIKE $3)
                 ORDER BY name ASC NULLS LAST, id ASC
                 LIMIT $4 OFFSET $5",
            )
            .bind(account_id)
            .bind(company_id)
            .bind(&search)
            .bind(CONTACTS_PER_PAGE)
            .bind(offset)
            .fetch_all(&st.db)
            .await?;
            (rows, total.0)
        }
    } else {
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM contacts WHERE account_id = $1 AND company_id = $2",
        )
        .bind(account_id)
        .bind(company_id)
        .fetch_one(&st.db)
        .await?;

        let rows: Vec<ContactItemRow> = sqlx::query_as(
            "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                    additional_attributes, identifier, custom_attributes, last_activity_at,
                    blocked, company_id
             FROM contacts
             WHERE account_id = $1 AND company_id = $2
             ORDER BY name ASC NULLS LAST, id ASC
             LIMIT $3 OFFSET $4",
        )
        .bind(account_id)
        .bind(company_id)
        .bind(CONTACTS_PER_PAGE)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;
        (rows, total.0)
    };

    let payload: Vec<Value> = rows
        .iter()
        .map(|r| contact_item_to_json(r, company_id, Some(&company)))
        .collect();

    Ok(Json(json!({
        "payload": payload,
        "meta": {
            "total_count": total,
            "page": page,
        }
    })))
}

// GET /companies/{id}/contacts/search?q=&page=
// Searches contacts eligible to link to this company (company_id IS NULL OR company_id != company_id)
async fn company_contacts_search(
    Path((account_id, company_id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<CompanyContactsQuery>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let company: Option<CompanyRow> = sqlx::query_as(
        "SELECT id, name, domain, description, account_id, created_at, updated_at,
                contacts_count, additional_attributes, custom_attributes, last_activity_at
         FROM companies
         WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(company_id)
    .fetch_optional(&st.db)
    .await?;
    let company = company.ok_or_else(ApiError::not_found)?;

    let q = params.q.unwrap_or_default();
    let q_trimmed = q.trim();
    if q_trimmed.is_empty() {
        return Err(ApiError::bad("Specify search string with parameter q"));
    }

    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * CONTACTS_PER_PAGE;
    let search = format!("%{}%", q_trimmed);

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM contacts
         WHERE account_id = $1 AND (company_id IS NULL OR company_id <> $2)
           AND (name ILIKE $3 OR email ILIKE $3 OR phone_number ILIKE $3 OR identifier ILIKE $3)",
    )
    .bind(account_id)
    .bind(company_id)
    .bind(&search)
    .fetch_one(&st.db)
    .await?;

    let rows: Vec<ContactItemRow> = sqlx::query_as(
        "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                additional_attributes, identifier, custom_attributes, last_activity_at,
                blocked, company_id
         FROM contacts
         WHERE account_id = $1 AND (company_id IS NULL OR company_id <> $2)
           AND (name ILIKE $3 OR email ILIKE $3 OR phone_number ILIKE $3 OR identifier ILIKE $3)
         ORDER BY name ASC NULLS LAST, id ASC
         LIMIT $4 OFFSET $5",
    )
    .bind(account_id)
    .bind(company_id)
    .bind(&search)
    .bind(CONTACTS_PER_PAGE)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let payload: Vec<Value> = rows
        .iter()
        .map(|r| contact_item_to_json(r, company_id, Some(&company)))
        .collect();

    Ok(Json(json!({
        "payload": payload,
        "meta": {
            "total_count": total.0,
            "page": page,
        }
    })))
}

#[derive(Debug, Deserialize)]
struct AssignContactBody {
    contact_id: i64,
}

// POST /companies/{id}/contacts { contact_id }
async fn company_contacts_create(
    Path((account_id, company_id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<AssignContactBody>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let company: Option<CompanyRow> = sqlx::query_as(
        "SELECT id, name, domain, description, account_id, created_at, updated_at,
                contacts_count, additional_attributes, custom_attributes, last_activity_at
         FROM companies
         WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(company_id)
    .fetch_optional(&st.db)
    .await?;
    let mut company = company.ok_or_else(ApiError::not_found)?;

    let contact: Option<ContactItemRow> = sqlx::query_as(
        "SELECT id, name, email, phone_number, account_id, created_at, updated_at,
                additional_attributes, identifier, custom_attributes, last_activity_at,
                blocked, company_id
         FROM contacts
         WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(body.contact_id)
    .fetch_optional(&st.db)
    .await?;
    let contact = contact.ok_or_else(ApiError::not_found)?;

    let old_company_id = contact.company_id;

    // assign contact to new company
    let updated_contact: ContactItemRow = sqlx::query_as(
        "UPDATE contacts
         SET company_id = $1, updated_at = $2
         WHERE account_id = $3 AND id = $4
         RETURNING id, name, email, phone_number, account_id, created_at, updated_at,
                   additional_attributes, identifier, custom_attributes, last_activity_at,
                   blocked, company_id",
    )
    .bind(company_id)
    .bind(now())
    .bind(account_id)
    .bind(body.contact_id)
    .fetch_one(&st.db)
    .await?;

    // update activity at on company if contact has last_activity_at
    if let Some(la) = updated_contact.last_activity_at {
        let should_update = match company.last_activity_at {
            Some(curr) => la > curr,
            None => true,
        };
        if should_update {
            sqlx::query("UPDATE companies SET last_activity_at = $1 WHERE account_id = $2 AND id = $3")
                .bind(la)
                .bind(account_id)
                .bind(company_id)
                .execute(&st.db)
                .await?;
            company.last_activity_at = Some(la);
        }
    }

    // refresh contacts_count on new company
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM contacts WHERE account_id = $1 AND company_id = $2",
    )
    .bind(account_id)
    .bind(company_id)
    .fetch_one(&st.db)
    .await?;
    sqlx::query("UPDATE companies SET contacts_count = $1 WHERE account_id = $2 AND id = $3")
        .bind(count.0 as i32)
        .bind(account_id)
        .bind(company_id)
        .execute(&st.db)
        .await?;
    company.contacts_count = Some(count.0 as i32);

    // refresh contacts_count on old company if any
    if let Some(old_cid) = old_company_id {
        if old_cid != company_id {
            let old_count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM contacts WHERE account_id = $1 AND company_id = $2",
            )
            .bind(account_id)
            .bind(old_cid)
            .fetch_one(&st.db)
            .await?;
            let _ = sqlx::query(
                "UPDATE companies SET contacts_count = $1 WHERE account_id = $2 AND id = $3",
            )
            .bind(old_count.0 as i32)
            .bind(account_id)
            .bind(old_cid)
            .execute(&st.db)
            .await;
        }
    }

    Ok(Json(json!({
        "payload": contact_item_to_json(&updated_contact, company_id, Some(&company))
    })))
}

// DELETE /companies/{id}/contacts/{contact_id}
async fn company_contacts_destroy(
    Path((account_id, company_id, contact_id)): Path<(i64, i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let company_exists: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM companies WHERE account_id = $1 AND id = $2")
            .bind(account_id)
            .bind(company_id)
            .fetch_optional(&st.db)
            .await?;
    if company_exists.is_none() {
        return Err(ApiError::not_found());
    }

    let contact: Option<(i64,)> = sqlx::query_as(
        "SELECT id::bigint FROM contacts WHERE account_id = $1 AND id = $2 AND company_id = $3",
    )
    .bind(account_id)
    .bind(contact_id)
    .bind(company_id)
    .fetch_optional(&st.db)
    .await?;
    if contact.is_none() {
        return Err(ApiError::not_found());
    }

    sqlx::query("UPDATE contacts SET company_id = NULL WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(contact_id)
        .execute(&st.db)
        .await?;

    // recalculate contacts_count
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM contacts WHERE account_id = $1 AND company_id = $2",
    )
    .bind(account_id)
    .bind(company_id)
    .fetch_one(&st.db)
    .await?;
    sqlx::query("UPDATE companies SET contacts_count = $1 WHERE account_id = $2 AND id = $3")
        .bind(count.0 as i32)
        .bind(account_id)
        .bind(company_id)
        .execute(&st.db)
        .await?;

    Ok(Json(json!({ "ok": true })))
}

// ============================================================================
// Audit Logs (admin only)
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
struct AuditRow {
    id: i64,
    auditable_id: Option<i64>,
    auditable_type: Option<String>,
    associated_id: Option<i64>,
    associated_type: Option<String>,
    user_id: Option<i64>,
    user_type: Option<String>,
    username: Option<String>,
    action: Option<String>,
    audited_changes: Option<Value>,
    version: Option<i32>,
    comment: Option<String>,
    remote_address: Option<String>,
    request_uuid: Option<String>,
    created_at: Option<NaiveDateTime>,
    city: Option<String>,
    country: Option<String>,
}

fn mask_remote_address(ip_str: Option<&str>) -> Option<String> {
    let ip = ip_str?;
    if ip.is_empty() {
        return None;
    }
    if let Ok(ipv4) = ip.parse::<std::net::Ipv4Addr>() {
        let oct = ipv4.octets();
        Some(format!("{}.{}.{}.x", oct[0], oct[1], oct[2]))
    } else if let Ok(ipv6) = ip.parse::<std::net::Ipv6Addr>() {
        let seg = ipv6.segments();
        Some(format!("{:x}:{:x}:{:x}:{:x}::", seg[0], seg[1], seg[2], seg[3]))
    } else {
        None
    }
}

fn audit_to_json(a: &AuditRow) -> Value {
    let location = match (&a.city, &a.country) {
        (Some(c), Some(co)) if !c.is_empty() && !co.is_empty() => Some(format!("{}, {}", c, co)),
        (Some(c), _) if !c.is_empty() => Some(c.clone()),
        (_, Some(co)) if !co.is_empty() => Some(co.clone()),
        _ => None,
    };

    let masked_ip = mask_remote_address(a.remote_address.as_deref());

    let mut audited_changes = a.audited_changes.clone().unwrap_or_else(|| json!({}));
    if a.auditable_type.as_deref() == Some("Message") {
        if let Some(obj) = audited_changes.as_object_mut() {
            obj.remove("content");
        }
    }

    json!({
        "id": a.id,
        "auditable_id": a.auditable_id,
        "auditable_type": a.auditable_type,
        "auditable": Value::Null,
        "associated_id": a.associated_id,
        "associated_type": a.associated_type,
        "user_id": a.user_id,
        "user_type": a.user_type,
        "username": a.username,
        "action": a.action,
        "audited_changes": audited_changes,
        "version": a.version.unwrap_or(0),
        "comment": a.comment,
        "request_uuid": a.request_uuid,
        "created_at": ts(a.created_at),
        "location": location,
        "remote_address": masked_ip,
    })
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Default)]
struct AuditLogsQuery {
    page: Option<i64>,
    sort: Option<String>,
    q: Option<String>,
    since: Option<String>,
    #[serde(rename = "until")]
    until_ts: Option<String>,
}

// GET /audit_logs?page=&sort=&q=&since=&until=
async fn audit_logs_index(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Query(params): Query<AuditLogsQuery>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;

    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * AUDITS_PER_PAGE;
    let order_dir = if params.sort.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };

    // Filter audits associated with Account, or auditable as Account
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audits
         WHERE (associated_type = 'Account' AND associated_id = $1)
            OR (auditable_type = 'Account' AND auditable_id = $1)",
    )
    .bind(account_id)
    .fetch_one(&st.db)
    .await?;

    let sql = format!(
        "SELECT id, auditable_id, auditable_type, associated_id, associated_type,
                user_id, user_type, username, action, audited_changes, version,
                comment, remote_address, request_uuid, created_at, city, country
         FROM audits
         WHERE (associated_type = 'Account' AND associated_id = $1)
            OR (auditable_type = 'Account' AND auditable_id = $1)
         ORDER BY created_at {order_dir} NULLS LAST, id {order_dir}
         LIMIT $2 OFFSET $3"
    );
    let rows: Vec<AuditRow> = sqlx::query_as(&sql)
        .bind(account_id)
        .bind(AUDITS_PER_PAGE)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;

    let payload: Vec<Value> = rows.iter().map(audit_to_json).collect();
    Ok(Json(json!({
        "audit_logs": payload,
        "current_page": page,
        "per_page": AUDITS_PER_PAGE,
        "total_entries": total.0,
    })))
}

// ============================================================================
// Custom Roles
// ============================================================================

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
struct CustomRoleRow {
    id: i64,
    name: Option<String>,
    description: Option<String>,
    account_id: i64,
    permissions: Vec<String>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

fn custom_role_to_json(r: &CustomRoleRow) -> Value {
    json!({
        "id": r.id,
        "name": r.name.clone().unwrap_or_default(),
        "description": r.description,
        "permissions": r.permissions,
        "created_at": ts(Some(r.created_at)),
        "updated_at": ts(Some(r.updated_at)),
    })
}

// GET /custom_roles
async fn custom_roles_index(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let rows: Vec<CustomRoleRow> = sqlx::query_as(
        "SELECT id, name, description, account_id, permissions, created_at, updated_at
         FROM custom_roles
         WHERE account_id = $1
         ORDER BY id ASC",
    )
    .bind(account_id)
    .fetch_all(&st.db)
    .await?;

    let list: Vec<Value> = rows.iter().map(custom_role_to_json).collect();
    Ok(Json(json!(list)))
}

// GET /custom_roles/{id}
async fn custom_roles_show(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let _au = require_account(&st.db, &user, account_id).await?;

    let row: Option<CustomRoleRow> = sqlx::query_as(
        "SELECT id, name, description, account_id, permissions, created_at, updated_at
         FROM custom_roles
         WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?;

    let row = row.ok_or_else(ApiError::not_found)?;
    Ok(Json(custom_role_to_json(&row)))
}

#[derive(Debug, Deserialize, Default)]
struct CustomRoleInput {
    name: Option<String>,
    description: Option<String>,
    permissions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct CustomRoleWrapper {
    custom_role: Option<CustomRoleInput>,
    name: Option<String>,
    description: Option<String>,
    permissions: Option<Vec<String>>,
}

impl CustomRoleWrapper {
    fn extract(self) -> CustomRoleInput {
        if let Some(cr) = self.custom_role {
            cr
        } else {
            CustomRoleInput {
                name: self.name,
                description: self.description,
                permissions: self.permissions,
            }
        }
    }
}

// POST /custom_roles (Admin only)
async fn custom_roles_create(
    Path(account_id): Path<i64>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<CustomRoleWrapper>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;

    let input = body.extract();
    let name = input.name.map(|s| s.trim().to_string()).unwrap_or_default();
    if name.is_empty() {
        return Err(ApiError::bad("Name is required"));
    }

    let description = input.description.map(|s| s.trim().to_string());
    let permissions = input.permissions.unwrap_or_default();
    let current_time = now();

    let row: CustomRoleRow = sqlx::query_as(
        "INSERT INTO custom_roles (name, description, account_id, permissions, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $5)
         RETURNING id, name, description, account_id, permissions, created_at, updated_at",
    )
    .bind(name)
    .bind(description)
    .bind(account_id)
    .bind(&permissions)
    .bind(current_time)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(custom_role_to_json(&row)))
}

// PATCH/PUT /custom_roles/{id} (Admin only)
async fn custom_roles_update(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
    Json(body): Json<CustomRoleWrapper>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;

    let existing: Option<CustomRoleRow> = sqlx::query_as(
        "SELECT id, name, description, account_id, permissions, created_at, updated_at
         FROM custom_roles
         WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?;
    let existing = existing.ok_or_else(ApiError::not_found)?;

    let input = body.extract();
    let name = input
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(existing.name.unwrap_or_default());
    let description = input.description.or(existing.description);
    let permissions = input.permissions.unwrap_or(existing.permissions);
    let current_time = now();

    let row: CustomRoleRow = sqlx::query_as(
        "UPDATE custom_roles
         SET name = $1, description = $2, permissions = $3, updated_at = $4
         WHERE account_id = $5 AND id = $6
         RETURNING id, name, description, account_id, permissions, created_at, updated_at",
    )
    .bind(name)
    .bind(description)
    .bind(&permissions)
    .bind(current_time)
    .bind(account_id)
    .bind(id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(custom_role_to_json(&row)))
}

// DELETE /custom_roles/{id} (Admin only)
async fn custom_roles_destroy(
    Path((account_id, id)): Path<(i64, i64)>,
    user: CurrentUser,
    State(st): State<AppState>,
) -> ApiResult {
    let au = require_account(&st.db, &user, account_id).await?;
    au.require_admin()?;

    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM custom_roles WHERE account_id = $1 AND id = $2")
            .bind(account_id)
            .bind(id)
            .fetch_optional(&st.db)
            .await?;
    if existing.is_none() {
        return Err(ApiError::not_found());
    }

    // Nullify custom_role_id in account_users
    sqlx::query("UPDATE account_users SET custom_role_id = NULL WHERE account_id = $1 AND custom_role_id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&st.db)
        .await?;

    sqlx::query("DELETE FROM custom_roles WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&st.db)
        .await?;

    Ok(Json(json!({ "ok": true })))
}
