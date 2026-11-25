//! Account Data Exports (contacts, conversations, messages CSV via ActiveStorage blobs).
//! Mirrors chatwoot/app/controllers/api/v1/accounts/data_exports_controller.rb.
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

use crate::{
    common::{ApiError, ApiResult, AppState, CurrentUser, require_account, ts},
    storage,
};

const PAGE_SIZE: i64 = 25;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/data_exports", get(index).post(create))
        .route("/data_exports/{id}", get(show))
        .route("/data_exports/{id}/download", get(download))
        .route("/data_exports/{id}/rerun", post(rerun))
}

fn csv_cell(s: &str) -> String {
    let clean = if s.starts_with(['=', '+', '-', '@']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    format!("\"{}\"", clean.replace('"', "\"\""))
}

fn csv_row(cells: &[&str]) -> String {
    cells
        .iter()
        .map(|c| csv_cell(c))
        .collect::<Vec<_>>()
        .join(",")
}

async fn export_json(db: &PgPool, account_id: i64, id: i64) -> Result<Value, ApiError> {
    let r = sqlx::query(
        "SELECT de.id, de.account_id, de.initiated_by_id, de.name, de.data_type, de.status,
                de.export_options, de.processed_records, de.total_records, de.error_message,
                de.started_at, de.completed_at, de.created_at, de.updated_at,
                u.name AS user_name,
                b.id::bigint AS blob_id, b.filename AS blob_filename
         FROM data_exports de
         LEFT JOIN users u ON u.id = de.initiated_by_id
         LEFT JOIN active_storage_attachments a ON a.record_type = 'DataExport' AND a.record_id = de.id AND a.name = 'export_file'
         LEFT JOIN active_storage_blobs b ON b.id = a.blob_id
         WHERE de.id = $1 AND de.account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let status_int: i32 = r.try_get("status")?;
    let status = match status_int {
        0 => "pending",
        1 => "processing",
        2 => "completed",
        3 => "failed",
        _ => "expired",
    };
    let blob_id: Option<i64> = r.try_get("blob_id")?;
    let blob_name: Option<String> = r.try_get("blob_filename")?;
    let file_url = match (blob_id, blob_name) {
        (Some(bid), Some(fname)) => Some(storage::blob_url(bid, &fname)),
        _ => None,
    };

    Ok(json!({
        "id": r.try_get::<i64, _>("id")?,
        "account_id": r.try_get::<i64, _>("account_id")?,
        "name": r.try_get::<String, _>("name")?,
        "data_type": r.try_get::<String, _>("data_type")?,
        "status": status,
        "export_options": r.try_get::<Value, _>("export_options")?,
        "processed_records": r.try_get::<i32, _>("processed_records")?,
        "total_records": r.try_get::<Option<i32>, _>("total_records")?,
        "error_message": r.try_get::<Option<String>, _>("error_message")?,
        "initiated_by": r.try_get::<Option<String>, _>("user_name")?.map(|n| json!({
            "id": r.try_get::<Option<i64>, _>("initiated_by_id").ok().flatten(),
            "name": n,
        })),
        "file_url": file_url,
        "started_at": ts(r.try_get::<Option<NaiveDateTime>, _>("started_at")?),
        "completed_at": ts(r.try_get::<Option<NaiveDateTime>, _>("completed_at")?),
        "created_at": ts(Some(r.try_get::<NaiveDateTime, _>("created_at")?)),
        "updated_at": ts(Some(r.try_get::<NaiveDateTime, _>("updated_at")?)),
    }))
}

async fn run_export(
    db: &PgPool,
    account_id: i64,
    initiated_by_id: i64,
    name: &str,
    data_type: &str,
    options: &Value,
) -> Result<Value, ApiError> {
    if !matches!(data_type, "contacts" | "conversations" | "messages") {
        return Err(ApiError::bad("Unsupported data_type for export"));
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO data_exports
            (account_id, initiated_by_id, name, data_type, status, export_options, processed_records, started_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 1, $5, 0, now(), now(), now()) RETURNING id",
    )
    .bind(account_id)
    .bind(initiated_by_id)
    .bind(name)
    .bind(data_type)
    .bind(options)
    .fetch_one(db)
    .await?;

    let mut lines = Vec::new();
    let count: i32 = match data_type {
        "contacts" => {
            lines.push("id,name,email,phone_number,identifier,created_at".to_string());
            let rows = sqlx::query(
                "SELECT id::bigint AS id, COALESCE(name, '') AS name, COALESCE(email, '') AS email,
                        COALESCE(phone_number, '') AS phone, COALESCE(identifier, '') AS identifier,
                        to_char(created_at, 'YYYY-MM-DD HH24:MI:SS') AS created_at
                 FROM contacts WHERE account_id = $1 ORDER BY id DESC LIMIT 10000",
            )
            .bind(account_id)
            .fetch_all(db)
            .await?;
            let n = rows.len() as i32;
            for r in rows {
                let cid = r.try_get::<i64, _>("id")?.to_string();
                let cname: String = r.try_get("name")?;
                let email: String = r.try_get("email")?;
                let phone: String = r.try_get("phone")?;
                let ident: String = r.try_get("identifier")?;
                let cat: String = r.try_get("created_at")?;
                lines.push(csv_row(&[&cid, &cname, &email, &phone, &ident, &cat]));
            }
            n
        }
        "conversations" => {
            lines.push("display_id,status,inbox_id,contact_id,created_at".to_string());
            let rows = sqlx::query(
                "SELECT display_id::bigint AS did, status, inbox_id::bigint AS iid, contact_id,
                        to_char(created_at, 'YYYY-MM-DD HH24:MI:SS') AS created_at
                 FROM conversations WHERE account_id = $1 ORDER BY id DESC LIMIT 10000",
            )
            .bind(account_id)
            .fetch_all(db)
            .await?;
            let n = rows.len() as i32;
            for r in rows {
                let did = r.try_get::<i64, _>("did")?.to_string();
                let st = r.try_get::<i32, _>("status")?.to_string();
                let iid = r.try_get::<i64, _>("iid")?.to_string();
                let ctid = r
                    .try_get::<Option<i64>, _>("contact_id")?
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let cat: String = r.try_get("created_at")?;
                lines.push(csv_row(&[&did, &st, &iid, &ctid, &cat]));
            }
            n
        }
        _ => {
            lines.push("id,conversation_id,message_type,private,content,created_at".to_string());
            let rows = sqlx::query(
                "SELECT id::bigint AS id, conversation_id::bigint AS cid, message_type, private,
                        COALESCE(content, '') AS content, to_char(created_at, 'YYYY-MM-DD HH24:MI:SS') AS created_at
                 FROM messages WHERE account_id = $1 ORDER BY id DESC LIMIT 10000",
            )
            .bind(account_id)
            .fetch_all(db)
            .await?;
            let n = rows.len() as i32;
            for r in rows {
                let mid = r.try_get::<i64, _>("id")?.to_string();
                let cid = r.try_get::<i64, _>("cid")?.to_string();
                let mt = r.try_get::<i32, _>("message_type")?.to_string();
                let priv_s = r.try_get::<bool, _>("private")?.to_string();
                let content: String = r.try_get("content")?;
                let cat: String = r.try_get("created_at")?;
                lines.push(csv_row(&[&mid, &cid, &mt, &priv_s, &content, &cat]));
            }
            n
        }
    };

    let csv_bytes = format!("\u{feff}{}\r\n", lines.join("\r\n"));
    let filename = format!("{data_type}_export_{id}.csv");
    let (blob_id, _) = storage::save_blob(db, csv_bytes.as_bytes(), &filename, "text/csv").await?;
    storage::attach(db, "DataExport", id, "export_file", blob_id).await?;

    sqlx::query(
        "UPDATE data_exports SET status = 2, processed_records = $1, total_records = $1, completed_at = now(), updated_at = now()
         WHERE id = $2 AND account_id = $3",
    )
    .bind(count)
    .bind(id)
    .bind(account_id)
    .execute(db)
    .await?;

    export_json(db, account_id, id).await
}

#[derive(Deserialize, Default)]
struct IndexQuery {
    before_id: Option<i64>,
}

async fn index(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(account_id): Path<i64>,
    Query(q): Query<IndexQuery>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM data_exports WHERE account_id = $1 AND ($2::bigint IS NULL OR id < $2)
         ORDER BY id DESC LIMIT $3",
    )
    .bind(account_id)
    .bind(q.before_id)
    .bind(PAGE_SIZE + 1)
    .fetch_all(&st.db)
    .await?;

    let has_more = ids.len() as i64 > PAGE_SIZE;
    let mut payload = Vec::new();
    for id in ids.into_iter().take(PAGE_SIZE as usize) {
        payload.push(export_json(&st.db, account_id, id).await?);
    }
    Ok(Json(json!({ "payload": payload, "has_more": has_more })))
}

async fn show(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    Ok(Json(export_json(&st.db, account_id, id).await?))
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
    let data_type = body["data_type"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("contacts");
    let name = body["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{data_type} export"));
    let options = body
        .get("export_options")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));

    Ok(Json(
        run_export(&st.db, account_id, user.id, &name, data_type, &options).await?,
    ))
}

async fn rerun(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let prev = export_json(&st.db, account_id, id).await?;
    let name = prev["name"].as_str().unwrap_or("export");
    let data_type = prev["data_type"].as_str().unwrap_or("contacts");
    let opts = &prev["export_options"];
    Ok(Json(
        run_export(&st.db, account_id, user.id, name, data_type, opts).await?,
    ))
}

async fn download(
    State(st): State<AppState>,
    user: CurrentUser,
    Path((account_id, id)): Path<(i64, i64)>,
) -> ApiResult {
    require_account(&st.db, &user, account_id)
        .await?
        .require_admin()?;
    let item = export_json(&st.db, account_id, id).await?;
    let url = item["file_url"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(json!({ "download_url": url })))
}
