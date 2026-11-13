use crate::common::*;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::Redirect,
    routing::{get, post},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashMap;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/portals", get(portals_index).post(portal_create))
        .route(
            "/portals/{portal}",
            get(portal_show)
                .patch(portal_update)
                .put(portal_update)
                .delete(portal_delete),
        )
        .route(
            "/portals/{portal}/archive",
            axum::routing::patch(portal_archive),
        )
        .route(
            "/portals/{portal}/logo",
            axum::routing::delete(portal_logo_delete),
        )
        .route(
            "/portals/{portal}/send_instructions",
            post(portal_send_instructions),
        )
        .route("/portals/{portal}/ssl_status", get(portal_ssl_status))
        .route(
            "/portals/{portal}/categories",
            get(categories_index).post(category_create),
        )
        .route(
            "/portals/{portal}/categories/reorder",
            post(category_reorder),
        )
        .route(
            "/portals/{portal}/categories/{id}",
            get(category_show)
                .patch(category_update)
                .put(category_update)
                .delete(category_delete),
        )
        .route(
            "/portals/{portal}/articles",
            get(articles_index).post(article_create),
        )
        .route("/portals/{portal}/articles/reorder", post(article_reorder))
        .route(
            "/portals/{portal}/articles/bulk_actions/update_status",
            axum::routing::patch(articles_bulk_update_status),
        )
        .route(
            "/portals/{portal}/articles/bulk_actions/update_category",
            axum::routing::patch(articles_bulk_update_category),
        )
        .route(
            "/portals/{portal}/articles/bulk_actions/delete_articles",
            axum::routing::delete(articles_bulk_delete),
        )
        .route("/search/articles", get(search_articles))
        .route(
            "/portals/{portal}/articles/{id}",
            get(article_show)
                .patch(article_update)
                .put(article_update)
                .delete(article_delete),
        )
        .route("/portals/{portal}/articles/{id}/edit", get(article_show))
}
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/hc/{portal}", get(public_redirect))
        .route("/hc/{portal}/{locale}", get(public_portal))
        .route("/hc/{portal}/{locale}/categories", get(public_categories))
        .route("/hc/{portal}/{locale}/categories.json", get(public_categories))
        .route("/hc/{portal}/{locale}/articles", get(public_articles))
        .route("/hc/{portal}/{locale}/articles.json", get(public_articles))
        .route(
            "/hc/{portal}/{locale}/categories/{category}/articles",
            get(public_category_articles),
        )
        .route("/hc/{portal}/articles/{slug}", get(public_article))
}
type Params = HashMap<String, String>;
fn page(q: &Params) -> i64 {
    q.get("page")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(1)
        .clamp(1, 1_000_000)
}
fn data<'a>(body: &'a Value, key: &str) -> Result<&'a Value, ApiError> {
    body.get(key)
        .filter(|v| v.is_object())
        .ok_or_else(|| ApiError::bad(format!("{key} is required")))
}
fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
}
fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !["search", "articles", "categories"].contains(&s)
}
async fn portal(db: &PgPool, account: i64, slug: &str) -> Result<Value, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT to_jsonb(p) FROM portals p WHERE account_id=$1 AND slug=$2")
            .bind(account)
            .bind(slug)
            .fetch_one(db)
            .await?,
    )
}
async fn public_portal_row(db: &PgPool, slug: &str) -> Result<Value, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM portals p WHERE slug=$1 AND archived IS NOT TRUE",
    )
    .bind(slug)
    .fetch_one(db)
    .await?)
}
fn id(v: &Value) -> i64 {
    v["id"].as_i64().unwrap_or_default()
}
fn default_locale(p: &Value) -> &str {
    p["config"]["default_locale"].as_str().unwrap_or("en")
}
async fn portal_json(
    db: &PgPool,
    p: &Value,
    user: i64,
    count_articles: bool,
    locale: Option<&str>,
) -> Result<Value, ApiError> {
    let pid = id(p);
    let counts: (i64, i64, i64, i64, i64) = if count_articles {
        sqlx::query_as("SELECT count(*), count(*) FILTER (WHERE status=2), count(*) FILTER (WHERE status=1), count(*) FILTER (WHERE status=0), count(*) FILTER (WHERE author_id=$2) FROM articles WHERE portal_id=$1 AND ($3::text IS NULL OR locale=$3)").bind(pid).bind(user).bind(locale).fetch_one(db).await?
    } else {
        (0, 0, 0, 0, 0)
    };
    let categories: i64 = sqlx::query_scalar("SELECT count(*) FROM categories WHERE portal_id=$1")
        .bind(pid)
        .fetch_one(db)
        .await?;
    let config = &p["config"];
    let default = default_locale(p);
    let locales = config["allowed_locales"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![json!("en")]);
    let mut allowed = Vec::new();
    for l in locales {
        if let Some(l) = l.as_str() {
            let ac: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM articles WHERE portal_id=$1 AND locale=$2",
            )
            .bind(pid)
            .bind(l)
            .fetch_one(db)
            .await?;
            let cc: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM categories WHERE portal_id=$1 AND locale=$2",
            )
            .bind(pid)
            .bind(l)
            .fetch_one(db)
            .await?;
            allowed.push(json!({"code":l,"articles_count":ac,"categories_count":cc,"draft":config["draft_locales"].as_array().is_some_and(|a| a.contains(&json!(l)))}));
        }
    }
    let mut meta = json!({"all_articles_count":counts.0,"archived_articles_count":counts.1,"published_count":counts.2,"draft_articles_count":counts.3,"categories_count":categories,"default_locale":default});
    if counts.0 > 0 {
        meta["mine_articles_count"] = json!(counts.4);
    }
    Ok(
        json!({"id":pid,"color":p["color"],"custom_domain":p["custom_domain"],"header_text":p["header_text"],"homepage_link":p["homepage_link"],"name":p["name"],"page_title":p["page_title"],"slug":p["slug"],"archived":p["archived"],"account_id":p["account_id"],"config":{"allowed_locales":allowed,"default_locale":default,"layout":config["layout"].as_str().unwrap_or("classic"),"social_profiles":config["social_profiles"].as_object().cloned().unwrap_or_default(),"locale_translations":config["locale_translations"].as_object().cloned().unwrap_or_default(),"popular_content":config["popular_content"].as_object().cloned().unwrap_or_default(),"analytics":config["analytics"].as_object().cloned().unwrap_or_default(),"disable_search_indexing":config["disable_search_indexing"].as_bool().unwrap_or(false)},"meta":meta}),
    )
}
async fn category_json(
    db: &PgPool,
    c: &Value,
    locale: Option<&str>,
    public: bool,
) -> Result<Value, ApiError> {
    let cid = id(c);
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM articles WHERE category_id=$1 AND ($2::text IS NULL OR locale=$2) AND (NOT $3 OR status=1)").bind(cid).bind(locale).bind(public).fetch_one(db).await?;
    if public {
        return Ok(
            json!({"name":c["name"],"slug":c["slug"],"locale":c["locale"],"description":c["description"],"position":c["position"],"meta":{"articles_count":count}}),
        );
    }
    let related: Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',c.id,'name',c.name,'slug',c.slug,'locale',c.locale,'description',c.description,'position',c.position,'account_id',c.account_id) FROM categories c JOIN related_categories r ON r.related_category_id=c.id WHERE r.category_id=$1").bind(cid).fetch_all(db).await?;
    let mut out = json!({"id":cid,"name":c["name"],"slug":c["slug"],"locale":c["locale"],"description":c["description"],"position":c["position"],"account_id":c["account_id"],"icon":c["icon"],"icon_color":c["icon_color"],"related_categories":if related.is_empty(){Value::Null}else{json!(related)},"meta":{"articles_count":count}});
    for (key, col) in [
        ("parent_category", "parent_category_id"),
        ("root_category", "associated_category_id"),
    ] {
        if let Some(v) = c[col].as_i64() {
            let row:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name,'slug',slug,'locale',locale,'description',description,'position',position,'account_id',account_id) FROM categories WHERE id=$1 AND portal_id=$2").bind(v).bind(c["portal_id"].as_i64()).fetch_optional(db).await?;
            if let Some(row) = row {
                out[key] = row;
            }
        }
    }
    Ok(out)
}
async fn article_json(db: &PgPool, a: &Value, public: bool) -> Result<Value, ApiError> {
    let category: Option<Value> =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM categories c WHERE id=$1 AND portal_id=$2")
            .bind(a["category_id"].as_i64())
            .bind(a["portal_id"].as_i64())
            .fetch_optional(db)
            .await?;
    let c = category.unwrap_or(Value::Null);
    let status = match a["status"].as_i64() {
        Some(1) => "published",
        Some(2) => "archived",
        _ => "draft",
    };
    let author:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',u.id,'name',u.name,'available_name',COALESCE(NULLIF(u.display_name,''),u.name),'email',u.email,'provider',u.provider,'auto_offline',false,'confirmed',u.confirmed_at IS NOT NULL,'thumbnail','', 'account_id',$2::bigint, 'role',CASE WHEN au.role=1 THEN 'administrator' ELSE 'agent' END, 'availability_status','offline') FROM users u JOIN account_users au ON au.user_id=u.id AND au.account_id=$2 WHERE u.id=$1").bind(a["author_id"].as_i64()).bind(a["account_id"].as_i64()).fetch_optional(db).await?;
    if public {
        let p: Value = sqlx::query_scalar("SELECT to_jsonb(p) FROM portals p WHERE id=$1")
            .bind(a["portal_id"].as_i64())
            .fetch_one(db)
            .await?;
        let pj = public_portal_json(db, &p).await?;
        let mut out = json!({"id":a["id"],"category_id":a["category_id"],"title":a["title"],"content":a["content"],"description":a["description"],"status":status,"position":a["position"],"account_id":a["account_id"],"last_updated_at":a["updated_at"],"slug":a["slug"],"portal":pj,"views":a["views"],"associated_articles":null,"link":format!("hc/{}/articles/{}",p["slug"].as_str().unwrap_or(""),a["slug"].as_str().unwrap_or(""))});
        if !c.is_null() {
            out["category"] = json!({"id":c["id"],"slug":c["slug"],"locale":c["locale"]});
        }
        if let Some(u) = author {
            out["author"] = json!({"name":u["name"],"available_name":u["available_name"],"thumbnail":u["thumbnail"]});
        }
        return Ok(out);
    }
    let root:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'title',title,'locale',locale) FROM articles WHERE id=$1 AND portal_id=$2").bind(a["associated_article_id"].as_i64()).bind(a["portal_id"].as_i64()).fetch_optional(db).await?;
    let timestamp: i64 = sqlx::query_scalar(
        "SELECT extract(epoch from updated_at)::bigint FROM articles WHERE id=$1",
    )
    .bind(id(a))
    .fetch_one(db)
    .await?;
    let mut out = json!({"id":a["id"],"slug":a["slug"],"title":a["title"],"content":a["content"],"description":a["description"],"status":status,"draft_title":a["draft_title"],"draft_content":a["draft_content"],"position":a["position"],"account_id":a["account_id"],"updated_at":timestamp,"meta":a["meta"],"locale":a["locale"],"associated_article_id":a["associated_article_id"],"root_article":root,"category":{"id":a["category_id"],"name":c["name"],"slug":c["slug"],"locale":c["locale"],"icon":c["icon"],"icon_color":c["icon_color"]},"views":a["views"],"associated_articles":null});
    if let Some(author) = author {
        out["author"] = author;
    }
    Ok(out)
}
async fn public_portal_json(db: &PgPool, p: &Value) -> Result<Value, ApiError> {
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM categories c WHERE portal_id=$1 ORDER BY position",
    )
    .bind(id(p))
    .fetch_all(db)
    .await?;
    let mut categories = Vec::new();
    for c in &rows {
        categories.push(category_json(db, c, None, true).await?);
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM articles WHERE portal_id=$1 AND status=1")
            .bind(id(p))
            .fetch_one(db)
            .await?;
    Ok(
        json!({"custom_domain":p["custom_domain"],"header_text":p["header_text"],"homepage_link":p["homepage_link"],"name":p["name"],"page_title":p["page_title"],"slug":p["slug"],"categories":if categories.is_empty(){Value::Null}else{json!(categories)},"meta":{"articles_count":count,"categories_count":rows.len(),"default_locale":default_locale(p)}}),
    )
}

async fn portals_index(
    State(st): State<AppState>,
    u: CurrentUser,
    Path(account): Path<i64>,
    Query(q): Query<Params>,
) -> ApiResult {
    require_account(&st.db, &u, account).await?;
    let rows: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM portals p WHERE account_id=$1 ORDER BY id")
            .bind(account)
            .fetch_all(&st.db)
            .await?;
    let mut payload = Vec::new();
    for p in &rows {
        payload.push(portal_json(&st.db, p, u.id, false, None).await?);
    }
    Ok(Json(
        json!({"payload":payload,"meta":{"current_page":q.get("page").cloned().unwrap_or("1".into()),"portals_count":rows.len()}}),
    ))
}
async fn portal_show(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Query(q): Query<Params>,
) -> ApiResult {
    require_account(&st.db, &u, account).await?;
    let p = portal(&st.db, account, &slug).await?;
    Ok(Json(
        portal_json(&st.db, &p, u.id, true, q.get("locale").map(String::as_str)).await?,
    ))
}
async fn portal_create(
    State(st): State<AppState>,
    u: CurrentUser,
    Path(account): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let b = data(&body, "portal")?;
    let name = text(b, "name").ok_or_else(|| ApiError::bad("Name can't be blank"))?;
    let slug = text(b, "slug")
        .filter(|s| valid_slug(s))
        .ok_or_else(|| ApiError::bad("Slug is invalid"))?;
    let p:Value=sqlx::query_scalar("INSERT INTO portals (account_id,name,slug,color,custom_domain,homepage_link,page_title,header_text,config,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,now(),now()) RETURNING to_jsonb(portals.*)").bind(account).bind(name).bind(slug).bind(text(b,"color")).bind(text(b,"custom_domain")).bind(text(b,"homepage_link")).bind(text(b,"page_title")).bind(text(b,"header_text")).bind(b.get("config").cloned().unwrap_or(json!({"allowed_locales":["en"]}))).fetch_one(&st.db).await?;
    Ok(Json(portal_json(&st.db, &p, u.id, false, None).await?))
}
// Only static, allowlisted identifiers enter these statements; values remain bound JSON.
async fn update(
    db: &PgPool,
    table: &str,
    pid: i64,
    row_id: i64,
    b: &Value,
    fields: &[&str],
    touch: bool,
) -> Result<Value, ApiError> {
    let mut set = Vec::new();
    for field in fields {
        if b.get(*field).is_some() {
            set.push(format!(
                "{field}=(jsonb_populate_record(NULL::{table}, $1)).{field}"
            ));
        }
    }
    if touch {
        set.push("updated_at=now()".into());
    }
    if set.is_empty() {
        return Ok(sqlx::query_scalar(&format!(
            "SELECT to_jsonb(t) FROM {table} t WHERE id=$2 AND {}=$3",
            if table == "portals" {
                "account_id"
            } else {
                "portal_id"
            }
        ))
        .bind(b)
        .bind(row_id)
        .bind(pid)
        .fetch_one(db)
        .await?);
    }
    let sql = format!(
        "UPDATE {table} SET {} WHERE id=$2 AND {}=$3 RETURNING to_jsonb({table}.*)",
        set.join(","),
        if table == "portals" {
            "account_id"
        } else {
            "portal_id"
        }
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(b)
        .bind(row_id)
        .bind(pid)
        .fetch_one(db)
        .await?)
}
const PORTAL_FIELDS: &[&str] = &[
    "name",
    "slug",
    "color",
    "custom_domain",
    "header_text",
    "homepage_link",
    "page_title",
    "archived",
    "config",
];
const CATEGORY_FIELDS: &[&str] = &[
    "name",
    "description",
    "position",
    "slug",
    "locale",
    "icon",
    "icon_color",
    "parent_category_id",
    "associated_category_id",
];
const ARTICLE_FIELDS: &[&str] = &[
    "title",
    "slug",
    "position",
    "content",
    "description",
    "category_id",
    "author_id",
    "associated_article_id",
    "status",
    "locale",
    "draft_title",
    "draft_content",
    "meta",
];
async fn portal_update(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let mut b = data(&body, "portal")?.clone();
    if let Some(s) = b.get("slug") {
        if !s.as_str().is_some_and(valid_slug) {
            return Err(ApiError::bad("Slug is invalid"));
        }
    }
    if let Some(cfg) = b.get_mut("config") {
        if let (Some(old), Some(new)) = (p["config"].as_object(), cfg.as_object_mut()) {
            for (k, v) in old {
                new.entry(k.clone()).or_insert(v.clone());
            }
        }
    }
    let p = update(&st.db, "portals", account, id(&p), &b, PORTAL_FIELDS, true).await?;
    Ok(Json(portal_json(&st.db, &p, u.id, false, None).await?))
}
async fn portal_archive(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    update(
        &st.db,
        "portals",
        account,
        id(&p),
        &json!({"archived":true}),
        PORTAL_FIELDS,
        true,
    )
    .await?;
    Ok(Json(json!({})))
}
async fn portal_delete(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let mut tx = st.db.begin().await?;
    let pid = id(&p);
    sqlx::query("DELETE FROM related_categories WHERE category_id IN (SELECT id FROM categories WHERE portal_id=$1) OR related_category_id IN (SELECT id FROM categories WHERE portal_id=$1)").bind(pid).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM articles WHERE portal_id=$1")
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM categories WHERE portal_id=$1")
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM portals_members WHERE portal_id=$1")
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE inboxes SET portal_id=NULL WHERE portal_id=$1")
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM portals WHERE id=$1 AND account_id=$2")
        .bind(pid)
        .bind(account)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({})))
}
async fn portal_logo_delete(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Query(q): Query<Params>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let dark = matches!(
        q.get("dark").map(String::as_str),
        Some("true" | "1" | "t" | "yes")
    );
    let name = if dark { "dark_logo" } else { "logo" };
    let old: Vec<i64> = sqlx::query_scalar(
        "DELETE FROM active_storage_attachments WHERE record_type='Portal' AND record_id=$1 AND name=$2 RETURNING blob_id",
    )
    .bind(id(&p))
    .bind(name)
    .fetch_all(&st.db)
    .await?;
    if !old.is_empty() {
        sqlx::query(
            "DELETE FROM active_storage_blobs b WHERE b.id = ANY($1) AND NOT EXISTS (SELECT 1 FROM active_storage_attachments a WHERE a.blob_id = b.id)",
        )
        .bind(&old)
        .execute(&st.db)
        .await?;
    }
    Ok(Json(json!({})))
}
fn valid_email(s: &str) -> bool {
    let s = s.trim();
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !s.contains(char::is_whitespace)
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}
async fn portal_send_instructions(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let email = text(&body, "email")
        .map(str::trim)
        .ok_or_else(|| ApiError::bad("Email is required"))?;
    if !valid_email(email) {
        return Err(ApiError::bad("Invalid email format"));
    }
    let custom_domain = text(&p, "custom_domain")
        .map(str::trim)
        .ok_or_else(|| ApiError::bad("Custom domain is not configured"))?;
    let raw_target = std::env::var("HELPCENTER_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("FRONTEND_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| st.frontend_url.clone());
    let target_host = raw_target
        .trim()
        .strip_prefix("https://")
        .or_else(|| raw_target.trim().strip_prefix("http://"))
        .unwrap_or(raw_target.trim())
        .split('/')
        .next()
        .unwrap_or("localhost");
    let cname_record = format!("{custom_domain} CNAME {target_host}");
    let subject = format!("Finish setting up {custom_domain}");
    let html = format!(
        "<p>Add the following CNAME record to your DNS provider to finish setting up <strong>{custom_domain}</strong>:</p><pre>{cname_record}</pre>"
    );
    tracing::info!(to = email, custom_domain, cname_record, "portal CNAME instructions");
    if let Err(e) = crate::mailer::send_html(email, &subject, html).await {
        tracing::warn!(to = email, error = %e, "failed to send portal CNAME instructions");
    }
    Ok(Json(json!({ "message": "Instructions sent successfully" })))
}
async fn portal_ssl_status(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
) -> ApiResult {
    require_account(&st.db, &u, account).await?;
    let p = portal(&st.db, account, &slug).await?;
    if text(&p, "custom_domain").is_none() {
        return Err(ApiError::bad("Custom domain is not configured"));
    }
    let ssl = &p["ssl_settings"];
    Ok(Json(json!({
        "status": ssl.get("cf_status").cloned().unwrap_or(Value::Null),
        "verification_errors": ssl.get("cf_verification_errors").cloned().unwrap_or(Value::Null),
    })))
}
async fn categories_index(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Query(q): Query<Params>,
) -> ApiResult {
    require_account(&st.db, &u, account).await?;
    let p = portal(&st.db, account, &slug).await?;
    let loc = q.get("locale").map(String::as_str);
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(c) FROM categories c WHERE portal_id=$1 AND ($2::text IS NULL OR locale=$2) ORDER BY position LIMIT 1000 OFFSET $3").bind(id(&p)).bind(loc).bind((page(&q)-1)*1000).fetch_all(&st.db).await?;
    let mut payload = Vec::new();
    for c in &rows {
        payload.push(category_json(&st.db, c, loc, false).await?);
    }
    Ok(Json(
        json!({"payload":payload,"meta":{"current_page":q.get("page").cloned().unwrap_or("1".into()),"categories_count":rows.len()}}),
    ))
}
async fn category_create(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let b = data(&body, "category")?;
    let name = text(b, "name").ok_or_else(|| ApiError::bad("Name can't be blank"))?;
    let s = text(b, "slug").ok_or_else(|| ApiError::bad("Slug can't be blank"))?;
    let locale = text(b, "locale").unwrap_or("en");
    validate_locale(&p, locale)?;
    let c:Value=sqlx::query_scalar("INSERT INTO categories (account_id,portal_id,name,slug,description,locale,position,icon,icon_color,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,now(),now()) RETURNING to_jsonb(categories.*)").bind(account).bind(id(&p)).bind(name).bind(s).bind(text(b,"description")).bind(locale).bind(b["position"].as_i64().map(|x|x as i32)).bind(text(b,"icon").unwrap_or("")).bind(text(b,"icon_color").unwrap_or("")).fetch_one(&st.db).await?;
    let c = update_category_extras(&st.db, &p, &c, b).await?;
    Ok(Json(
        json!({"payload":category_json(&st.db,&c,None,false).await?}),
    ))
}
fn validate_locale(p: &Value, locale: &str) -> Result<(), ApiError> {
    let ok = p["config"]["allowed_locales"]
        .as_array()
        .map(|a| a.contains(&json!(locale)))
        .unwrap_or(locale == "en");
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad(
            "Locale is not part of portal's allowed locales",
        ))
    }
}
async fn validate_category_refs(
    db: &PgPool,
    pid: i64,
    b: &Value,
    fields: &[&str],
) -> Result<(), ApiError> {
    for k in fields {
        if let Some(v) = b[*k].as_i64() {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE id=$1 AND portal_id=$2)",
            )
            .bind(v)
            .bind(pid)
            .fetch_one(db)
            .await?;
            if !exists {
                return Err(ApiError::bad("Invalid category"));
            }
        }
    }
    Ok(())
}
async fn update_category_extras(
    db: &PgPool,
    p: &Value,
    c: &Value,
    b: &Value,
) -> Result<Value, ApiError> {
    validate_category_refs(
        db,
        id(p),
        b,
        &["parent_category_id", "associated_category_id"],
    )
    .await?;
    let c = update(db, "categories", id(p), id(c), b, CATEGORY_FIELDS, true).await?;
    if let Some(ids) = b["related_category_ids"].as_array() {
        if !ids.is_empty() {
            let mut tx = db.begin().await?;
            sqlx::query("DELETE FROM related_categories WHERE category_id=$1")
                .bind(id(&c))
                .execute(&mut *tx)
                .await?;
            for related in ids {
                let rid = related
                    .as_i64()
                    .ok_or_else(|| ApiError::bad("Invalid category"))?;
                sqlx::query("INSERT INTO related_categories(category_id,related_category_id,created_at,updated_at) SELECT $1,id,now(),now() FROM categories WHERE id=$2 AND portal_id=$3 ON CONFLICT DO NOTHING").bind(id(&c)).bind(rid).bind(id(p)).execute(&mut *tx).await?;
            }
            tx.commit().await?;
        }
    }
    Ok(c)
}
async fn category_show(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug, cid)): Path<(i64, String, i64)>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let c: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM categories c WHERE id=$1 AND portal_id=$2")
            .bind(cid)
            .bind(id(&p))
            .fetch_one(&st.db)
            .await?;
    Ok(Json(
        json!({"payload":category_json(&st.db,&c,None,false).await?}),
    ))
}
async fn category_update(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug, cid)): Path<(i64, String, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let b = data(&body, "category")?;
    if let Some(l) = text(b, "locale") {
        validate_locale(&p, l)?;
    }
    let c: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM categories c WHERE id=$1 AND portal_id=$2")
            .bind(cid)
            .bind(id(&p))
            .fetch_one(&st.db)
            .await?;
    let c = update_category_extras(&st.db, &p, &c, b).await?;
    Ok(Json(
        json!({"payload":category_json(&st.db,&c,None,false).await?}),
    ))
}
async fn category_delete(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug, cid)): Path<(i64, String, i64)>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let mut tx = st.db.begin().await?;
    let found: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM categories WHERE id=$1 AND portal_id=$2)")
            .bind(cid)
            .bind(id(&p))
            .fetch_one(&mut *tx)
            .await?;
    if !found {
        return Err(ApiError::not_found());
    }
    sqlx::query("UPDATE articles SET category_id=NULL WHERE category_id=$1 AND portal_id=$2")
        .bind(cid)
        .bind(id(&p))
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE categories SET parent_category_id=NULL WHERE parent_category_id=$1 AND portal_id=$2").bind(cid).bind(id(&p)).execute(&mut *tx).await?;
    sqlx::query("UPDATE categories SET associated_category_id=NULL WHERE associated_category_id=$1 AND portal_id=$2").bind(cid).bind(id(&p)).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM related_categories WHERE category_id=$1 OR related_category_id=$1")
        .bind(cid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM categories WHERE id=$1 AND portal_id=$2")
        .bind(cid)
        .bind(id(&p))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({})))
}
async fn reorder(db: &PgPool, pid: i64, table: &str, body: &Value) -> Result<Value, ApiError> {
    let mut tx = db.begin().await?;
    let mut positions = serde_json::Map::new();
    if let Some(m) = body["positions_hash"].as_object() {
        for (key, val) in m {
            let row: i64 = key.parse().map_err(|_| ApiError::bad("Invalid id"))?;
            let pos = val
                .as_i64()
                .or_else(|| val.as_str().and_then(|s| s.parse().ok()))
                .filter(|n| i32::try_from(*n).is_ok())
                .ok_or_else(|| ApiError::bad("Invalid position"))?;
            let sql = format!(
                "UPDATE {table} SET position=$1,updated_at=now() WHERE id=$2 AND portal_id=$3"
            );
            let result = sqlx::query(&sql)
                .bind(pos as i32)
                .bind(row)
                .bind(pid)
                .execute(&mut *tx)
                .await?;
            if result.rows_affected() == 0 {
                return Err(ApiError::not_found());
            }
            positions.insert(key.clone(), json!(pos));
        }
    }
    tx.commit().await?;
    Ok(json!(positions))
}
async fn category_reorder(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    reorder(&st.db, id(&p), "categories", &body).await?;
    Ok(Json(json!({})))
}
async fn article_reorder(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    Ok(Json(
        json!({"positions":reorder(&st.db,id(&p),"articles",&body).await?}),
    ))
}

async fn articles_index(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Query(q): Query<Params>,
) -> ApiResult {
    require_account(&st.db, &u, account).await?;
    let p = portal(&st.db, account, &slug).await?;
    let locale = q.get("locale").filter(|s| !s.is_empty());
    let category = q.get("category_slug").filter(|s| !s.is_empty());
    let query = q.get("query").filter(|s| !s.is_empty());
    let author = q.get("author_id").and_then(|s| s.parse::<i64>().ok());
    let status = q.get("status").map(|s| status_number(s)).transpose()?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a) FROM articles a LEFT JOIN categories c ON c.id=a.category_id WHERE a.portal_id=$1 AND ($2::text IS NULL OR a.locale=$2) AND ($3::text IS NULL OR c.slug=$3) AND ($4::text IS NULL OR (a.title ILIKE '%'||$4||'%' OR a.description ILIKE '%'||$4||'%' OR a.content ILIKE '%'||$4||'%')) AND ($5::bigint IS NULL OR a.author_id=$5) ORDER BY CASE WHEN $3::text IS NOT NULL THEN a.position END ASC, a.updated_at DESC").bind(id(&p)).bind(locale).bind(category).bind(query).bind(author).fetch_all(&st.db).await?;
    let all: i64 = sqlx::query_scalar("SELECT count(*) FROM articles WHERE portal_id=$1")
        .bind(id(&p))
        .fetch_one(&st.db)
        .await?;
    let mut payload = Vec::new();
    for a in rows
        .iter()
        .filter(|a| status.is_none_or(|s| a["status"].as_i64() == Some(s as i64)))
        .skip(((page(&q) - 1) * 25) as usize)
        .take(25)
    {
        payload.push(article_json(&st.db, a, false).await?);
    }
    let count = |n: i64| {
        rows.iter()
            .filter(|a| a["status"].as_i64() == Some(n))
            .count()
    };
    let mine = rows
        .iter()
        .filter(|a| a["author_id"].as_i64() == Some(u.id))
        .count();
    Ok(Json(
        json!({"payload":payload,"meta":{"all_articles_count":all,"archived_articles_count":count(2),"articles_count":rows.len(),"current_page":q.get("page").cloned().unwrap_or("1".into()),"draft_articles_count":count(0),"mine_articles_count":mine,"published_count":count(1)}}),
    ))
}
fn status_number(s: &str) -> Result<i32, ApiError> {
    match s {
        "draft" => Ok(0),
        "published" => Ok(1),
        "archived" => Ok(2),
        _ => Err(ApiError::bad("Invalid status")),
    }
}
async fn validate_article(
    db: &PgPool,
    p: &Value,
    b: &Value,
    old: Option<&Value>,
    account: i64,
) -> Result<Value, ApiError> {
    let mut value = old.cloned().unwrap_or(json!({}));
    let o = value
        .as_object_mut()
        .ok_or_else(|| ApiError::bad("Invalid article"))?;
    for (k, v) in b
        .as_object()
        .ok_or_else(|| ApiError::bad("Invalid article"))?
    {
        if ARTICLE_FIELDS.contains(&k.as_str()) {
            o.insert(k.clone(), v.clone());
        }
    }
    if let Some(s) = value["status"].as_str() {
        value["status"] = json!(status_number(s)?);
    } else if value["status"].is_null() {
        value["status"] = json!(0);
    } else if ![0, 1, 2].contains(&value["status"].as_i64().unwrap_or(-1)) {
        return Err(ApiError::bad("Invalid status"));
    }
    if text(&value, "title").is_none() {
        return Err(ApiError::bad("Title can't be blank"));
    }
    if value["status"] == 1 && text(&value, "content").is_none() {
        return Err(ApiError::bad("Content can't be blank"));
    }
    if let Some(s) = text(&value, "slug") {
        if !valid_slug(s) {
            return Err(ApiError::bad("Slug is invalid"));
        }
    } else {
        value["slug"] = json!(uuid::Uuid::new_v4().simple().to_string());
    }
    let author = value["author_id"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("Author can't be blank"))?;
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM account_users WHERE account_id=$1 AND user_id=$2)",
    )
    .bind(account)
    .bind(author)
    .fetch_one(db)
    .await?;
    if !ok {
        return Err(ApiError::bad("Invalid author ID"));
    }
    validate_category_refs(db, id(p), &value, &["category_id"]).await?;
    if let Some(cid) = value["category_id"].as_i64() {
        let locale: String =
            sqlx::query_scalar("SELECT locale FROM categories WHERE id=$1 AND portal_id=$2")
                .bind(cid)
                .bind(id(p))
                .fetch_one(db)
                .await?;
        value["locale"] = json!(locale);
    } else if text(&value, "locale").is_none() {
        value["locale"] = json!(default_locale(p));
    }
    if let Some(root) = value["associated_article_id"].as_i64() {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM articles WHERE id=$1 AND portal_id=$2)",
        )
        .bind(root)
        .bind(id(p))
        .fetch_one(db)
        .await?;
        if !exists {
            return Err(ApiError::bad("Invalid associated article"));
        }
    }
    if !value["meta"]["noindex"].is_null() && !value["meta"]["noindex"].is_boolean() {
        return Err(ApiError::bad("Invalid noindex"));
    }
    Ok(value)
}
async fn article_create(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let b = validate_article(&st.db, &p, data(&body, "article")?, None, account).await?;
    let a:Value=sqlx::query_scalar("INSERT INTO articles (account_id,portal_id,title,slug,content,description,status,category_id,author_id,locale,position,views,meta,draft_title,draft_content,associated_article_id,created_at,updated_at) SELECT $2,$3,x.title,x.slug,x.content,x.description,x.status,x.category_id,x.author_id,x.locale,COALESCE(x.position,(SELECT COALESCE(max(position),0)+10 FROM articles WHERE account_id=$2 AND category_id IS NOT DISTINCT FROM x.category_id)),0,COALESCE(x.meta,'{}'::jsonb),x.draft_title,x.draft_content,x.associated_article_id,now(),now() FROM jsonb_populate_record(NULL::articles,$1) x RETURNING to_jsonb(articles.*)").bind(&b).bind(account).bind(id(&p)).fetch_one(&st.db).await?;
    Ok(Json(
        json!({"payload":article_json(&st.db,&a,false).await?}),
    ))
}
async fn article_row(db: &PgPool, pid: i64, aid: i64) -> Result<Value, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT to_jsonb(a) FROM articles a WHERE id=$1 AND portal_id=$2")
            .bind(aid)
            .bind(pid)
            .fetch_one(db)
            .await?,
    )
}
async fn article_show(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug, aid)): Path<(i64, String, i64)>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let a = article_row(&st.db, id(&p), aid).await?;
    Ok(Json(
        json!({"payload":article_json(&st.db,&a,false).await?}),
    ))
}
async fn article_update(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug, aid)): Path<(i64, String, i64)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let old = article_row(&st.db, id(&p), aid).await?;
    let mut input = data(&body, "article")?.clone();
    if let Some(author) = input["author_id"].as_i64() {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM account_users WHERE account_id=$1 AND user_id=$2)",
        )
        .bind(account)
        .bind(author)
        .fetch_one(&st.db)
        .await?;
        if !ok {
            input.as_object_mut().unwrap().remove("author_id");
        }
    }
    let b = validate_article(&st.db, &p, &input, Some(&old), account).await?;
    let touch = input
        .as_object()
        .unwrap()
        .keys()
        .any(|k| k != "draft_title" && k != "draft_content");
    let a = update(&st.db, "articles", id(&p), aid, &b, ARTICLE_FIELDS, touch).await?;
    Ok(Json(
        json!({"payload":article_json(&st.db,&a,false).await?}),
    ))
}
async fn article_delete(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug, aid)): Path<(i64, String, i64)>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let mut tx = st.db.begin().await?;
    sqlx::query("UPDATE articles SET associated_article_id=NULL WHERE associated_article_id=$1 AND portal_id=$2").bind(aid).bind(id(&p)).execute(&mut *tx).await?;
    let n = sqlx::query("DELETE FROM articles WHERE id=$1 AND portal_id=$2")
        .bind(aid)
        .bind(id(&p))
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    tx.commit().await?;
    Ok(Json(json!({})))
}
fn article_ids(body: &Value) -> Vec<i64> {
    body.get("ids")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                .collect()
        })
        .unwrap_or_default()
}
async fn articles_bulk_update_status(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let ids = article_ids(&body);
    let matched: Vec<(i64, Option<String>)> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as("SELECT id, content FROM articles WHERE portal_id=$1 AND id = ANY($2)")
            .bind(id(&p))
            .bind(&ids)
            .fetch_all(&st.db)
            .await?
    };
    if matched.is_empty() {
        return Err(ApiError::bad("No articles found to process"));
    }
    let status_str = body
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad("Invalid status value"))?;
    let status_num = status_number(status_str).map_err(|_| ApiError::bad("Invalid status value"))?;
    if status_num == 1
        && matched
            .iter()
            .any(|(_, c)| c.as_deref().map(str::trim).is_none_or(str::is_empty))
    {
        return Err(ApiError::bad("Content can't be blank"));
    }
    let matched_ids: Vec<i64> = matched.into_iter().map(|(aid, _)| aid).collect();
    sqlx::query("UPDATE articles SET status=$1, updated_at=now() WHERE portal_id=$2 AND id = ANY($3)")
        .bind(status_num)
        .bind(id(&p))
        .bind(&matched_ids)
        .execute(&st.db)
        .await?;
    Ok(Json(json!({})))
}
async fn articles_bulk_update_category(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let ids = article_ids(&body);
    let matched_ids: Vec<i64> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_scalar("SELECT id FROM articles WHERE portal_id=$1 AND id = ANY($2)")
            .bind(id(&p))
            .bind(&ids)
            .fetch_all(&st.db)
            .await?
    };
    if matched_ids.is_empty() {
        return Err(ApiError::bad("No articles found to process"));
    }
    let cid = body
        .get("category_id")
        .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .ok_or_else(|| ApiError::bad("Category not found in this portal"))?;
    let cat_ok: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM categories WHERE id=$1 AND portal_id=$2)")
            .bind(cid)
            .bind(id(&p))
            .fetch_one(&st.db)
            .await?;
    if !cat_ok {
        return Err(ApiError::bad("Category not found in this portal"));
    }
    sqlx::query(
        "UPDATE articles SET category_id=$1, updated_at=now() WHERE portal_id=$2 AND id = ANY($3)",
    )
    .bind(cid)
    .bind(id(&p))
    .bind(&matched_ids)
    .execute(&st.db)
    .await?;
    Ok(Json(json!({})))
}
async fn articles_bulk_delete(
    State(st): State<AppState>,
    u: CurrentUser,
    Path((account, slug)): Path<(i64, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    require_account(&st.db, &u, account)
        .await?
        .require_admin()?;
    let p = portal(&st.db, account, &slug).await?;
    let ids = article_ids(&body);
    let matched_ids: Vec<i64> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_scalar("SELECT id FROM articles WHERE portal_id=$1 AND id = ANY($2)")
            .bind(id(&p))
            .bind(&ids)
            .fetch_all(&st.db)
            .await?
    };
    if matched_ids.is_empty() {
        return Err(ApiError::bad("No articles found to process"));
    }
    let mut tx = st.db.begin().await?;
    sqlx::query(
        "UPDATE articles SET associated_article_id=NULL WHERE portal_id=$1 AND associated_article_id = ANY($2)",
    )
    .bind(id(&p))
    .bind(&matched_ids)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM articles WHERE portal_id=$1 AND id = ANY($2)")
        .bind(id(&p))
        .bind(&matched_ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({})))
}
// mirrors chatwoot/app/controllers/api/v1/accounts/search_controller.rb#articles
async fn search_articles(
    State(st): State<AppState>,
    u: CurrentUser,
    Path(account): Path<i64>,
    Query(q): Query<Params>,
) -> ApiResult {
    require_account(&st.db, &u, account).await?;
    let Some(term) = q.get("q").map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Ok(Json(json!({ "payload": { "articles": [] } })));
    };
    let pat = format!("%{term}%");
    let offset = (page(&q) - 1) * 15;
    let since = q.get("since").and_then(|s| s.parse::<i64>().ok());
    let until = q.get("until").and_then(|s| s.parse::<i64>().ok());
    let rows: Vec<(
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        i32,
        Option<String>,
        i32,
        i64,
    )> = sqlx::query_as(
        "SELECT a.id::bigint, a.title, a.locale, a.content, a.slug, p.slug, a.account_id, c.name, a.status, extract(epoch from a.updated_at)::bigint
         FROM articles a
         JOIN portals p ON p.id = a.portal_id
         LEFT JOIN categories c ON c.id = a.category_id
         WHERE a.account_id = $1
           AND (a.title ILIKE $2 OR a.description ILIKE $2 OR a.content ILIKE $2)
           AND ($3::bigint IS NULL OR a.updated_at >= to_timestamp($3))
           AND ($4::bigint IS NULL OR a.updated_at <= to_timestamp($4))
         ORDER BY a.updated_at DESC
         LIMIT 15 OFFSET $5",
    )
    .bind(account)
    .bind(&pat)
    .bind(since)
    .bind(until)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;
    let articles: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let status = match r.8 {
                1 => "published",
                2 => "archived",
                _ => "draft",
            };
            json!({
                "id": r.0,
                "title": r.1,
                "locale": r.2,
                "content": r.3,
                "slug": r.4,
                "portal_slug": r.5,
                "account_id": r.6,
                "category_name": r.7,
                "status": status,
                "updated_at": r.9,
            })
        })
        .collect();
    Ok(Json(json!({ "payload": { "articles": articles } })))
}
async fn public_redirect(
    State(st): State<AppState>,
    Path(slug): Path<String>,
) -> ApiResult<Redirect> {
    let p = public_portal_row(&st.db, &slug).await?;
    Ok(Redirect::temporary(&format!(
        "/hc/{}/{}",
        slug,
        default_locale(&p)
    )))
}
async fn public_portal(
    State(st): State<AppState>,
    Path((slug, _locale)): Path<(String, String)>,
) -> ApiResult {
    let p = public_portal_row(&st.db, &slug).await?;
    Ok(Json(public_portal_json(&st.db, &p).await?))
}
async fn public_categories(
    State(st): State<AppState>,
    Path((slug, _locale)): Path<(String, String)>,
) -> ApiResult {
    let p = public_portal_row(&st.db, &slug).await?;
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM categories c WHERE portal_id=$1 ORDER BY position",
    )
    .bind(id(&p))
    .fetch_all(&st.db)
    .await?;
    let mut out = Vec::new();
    for c in rows {
        out.push(category_json(&st.db, &c, None, true).await?);
    }
    Ok(Json(json!({"payload":out})))
}
async fn public_articles_impl(
    db: &PgPool,
    slug: &str,
    locale: &str,
    q: &Params,
    category: Option<&str>,
) -> ApiResult {
    let p = public_portal_row(db, slug).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM articles WHERE portal_id=$1 AND locale=$2 AND status=1",
    )
    .bind(id(&p))
    .bind(locale)
    .fetch_one(db)
    .await?;
    let query = q.get("query").filter(|s| !s.trim().is_empty());
    let limit = q
        .get("per_page")
        .map(|s| s.parse::<i64>().unwrap_or(25).clamp(1, 100))
        .unwrap_or(i64::MAX);
    let offset = if limit == i64::MAX {
        0
    } else {
        (page(q) - 1) * limit
    };
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a) FROM articles a LEFT JOIN categories c ON c.id=a.category_id WHERE a.portal_id=$1 AND a.status=1 AND a.locale=$2 AND ($3::text IS NULL OR a.title ILIKE '%'||$3||'%' OR a.content ILIKE '%'||$3||'%' OR a.description ILIKE '%'||$3||'%') AND ($4::text IS NULL OR c.slug=$4) ORDER BY CASE WHEN $5 THEN a.views END DESC, a.position ASC LIMIT $6 OFFSET $7").bind(id(&p)).bind(locale).bind(query).bind(category).bind(q.get("sort").is_some_and(|s|s=="views")).bind(limit).bind(offset).fetch_all(db).await?;
    let mut out = Vec::new();
    for a in rows {
        out.push(article_json(db, &a, true).await?);
    }
    Ok(Json(json!({"payload":out,"meta":{"articles_count":count}})))
}
async fn public_articles(
    State(st): State<AppState>,
    Path((slug, locale)): Path<(String, String)>,
    Query(q): Query<Params>,
) -> ApiResult {
    public_articles_impl(&st.db, &slug, &locale, &q, None).await
}
async fn public_category_articles(
    State(st): State<AppState>,
    Path((slug, locale, category)): Path<(String, String, String)>,
    Query(q): Query<Params>,
) -> ApiResult {
    public_articles_impl(&st.db, &slug, &locale, &q, Some(&category)).await
}
async fn public_article(
    State(st): State<AppState>,
    Path((slug, article)): Path<(String, String)>,
) -> ApiResult {
    let p = public_portal_row(&st.db, &slug).await?;
    let article = article.strip_suffix(".json").unwrap_or(&article);
    let a: Value = sqlx::query_scalar(
        "SELECT to_jsonb(a) FROM articles a WHERE portal_id=$1 AND slug=$2 AND status=1",
    )
    .bind(id(&p))
    .bind(article)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(article_json(&st.db, &a, true).await?))
}
