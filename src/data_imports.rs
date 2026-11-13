//! Data imports for CSV contacts. Mirrors chatwoot/app/controllers/api/v1/accounts/data_imports_controller.rb
//! and app/views/api/v1/accounts/data_imports/*.jbuilder. Admin only.
//! Only the CSV contacts import is supported. Any source provider or non-contacts data type returns
//! 422 `unsupported source`. Rows run in a background task. The raw CSV is kept in
//! `pulsar_data_import_files` until the import finishes, so failed imports can be retried.

use std::collections::HashMap;

use axum::{
    extract::{Multipart, Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{NaiveDateTime, SecondsFormat};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::PgPool;

use crate::common::{now, require_account, ApiError, ApiResult, AppState, CurrentUser};
use crate::contacts::parse_csv;

// Chatwoot DataImport status enum: pending 0, processing 1, completed 2, failed 3, completed_with_errors 6, abandoned 7.
const PENDING: i32 = 0;
const PROCESSING: i32 = 1;
const COMPLETED: i32 = 2;
const FAILED: i32 = 3;
const COMPLETED_WITH_ERRORS: i32 = 6;
const ABANDONED: i32 = 7;

const NON_SKIP: &str = "(details->>'kind' IS NULL OR details->>'kind' NOT IN ('failed','skipped'))";
const SKIP: &str = "details->>'kind' IN ('failed','skipped')";
const SOURCE_TYPES: [&str; 3] = ["contact", "conversation", "message"];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/data_imports", get(index).post(create))
        .route("/data_imports/{id}", get(show))
        .route("/data_imports/{id}/start", post(start))
        .route("/data_imports/{id}/retry", post(retry))
        .route("/data_imports/{id}/abandon", post(abandon))
        .route("/data_imports/{id}/error_logs", get(error_logs))
        .route("/data_imports/{id}/skip_logs", get(skip_logs))
}

const IMPORT_SELECT: &str = "SELECT d.id, d.name, d.data_type, d.source_type, d.source_provider, d.import_types, d.status,
    d.total_records, d.processed_records, d.processing_errors, d.stats, d.cursor, d.created_at, d.updated_at, d.started_at, d.completed_at,
    d.abandoned_at, u.id::bigint AS initiator_id, u.name AS initiator_name, u.email AS initiator_email
    FROM data_imports d LEFT JOIN users u ON u.id = d.initiated_by_id";

const LOG_SELECT: &str = "SELECT id, error_code, message, source_object_type, source_object_id, details, created_at FROM data_import_errors";

#[derive(sqlx::FromRow)]
struct ImportRow {
    id: i64,
    name: Option<String>,
    data_type: String,
    source_type: Option<String>,
    source_provider: Option<String>,
    import_types: Value,
    status: i32,
    total_records: Option<i32>,
    processed_records: Option<i32>,
    processing_errors: Option<String>,
    stats: Value,
    cursor: Value,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    started_at: Option<NaiveDateTime>,
    completed_at: Option<NaiveDateTime>,
    abandoned_at: Option<NaiveDateTime>,
    initiator_id: Option<i64>,
    initiator_name: Option<String>,
    initiator_email: Option<String>,
}

#[derive(sqlx::FromRow)]
struct LogRow {
    id: i64,
    error_code: String,
    message: Option<String>,
    source_object_type: Option<String>,
    source_object_id: Option<String>,
    details: Value,
    created_at: NaiveDateTime,
}

#[derive(Deserialize)]
struct ShowQuery {
    skip_logs_type: Option<String>,
}

/// Per-row result for the contacts CSV import.
enum Outcome {
    Imported,
    Skipped(&'static str, String),
    Failed(&'static str, String),
}

async fn admin(db: &PgPool, user: &CurrentUser, account_id: i64) -> Result<(), ApiError> {
    require_account(db, user, account_id).await?.require_admin()
}

fn status_name(status: i32) -> &'static str {
    match status {
        PENDING => "pending",
        PROCESSING => "processing",
        COMPLETED => "completed",
        FAILED => "failed",
        COMPLETED_WITH_ERRORS => "completed_with_errors",
        ABANDONED => "abandoned",
        _ => "unknown",
    }
}

fn iso(t: NaiveDateTime) -> Value {
    json!(t.and_utc().to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn iso_opt(t: Option<NaiveDateTime>) -> Value {
    t.map(iso).unwrap_or(Value::Null)
}

fn stats_json(total: i32, imported: i32, skipped: i32, failed: i32) -> Value {
    json!({
        "contacts": { "imported": imported, "skipped": skipped, "total": total },
        "conversations": { "imported": 0, "skipped": 0 },
        "messages": { "imported": 0, "skipped": 0 },
        "errors": { "count": failed }
    })
}

fn render(r: &ImportRow) -> Value {
    json!({
        "id": r.id,
        "name": r.name,
        "data_type": r.data_type,
        "source_type": r.source_type,
        "source_provider": r.source_provider,
        "import_types": r.import_types,
        "status": status_name(r.status),
        // Stalled detection only applies to integration imports, which are out of scope here.
        "stalled": false,
        "total_records": r.total_records,
        "processed_records": r.processed_records,
        "processing_errors": r.processing_errors,
        "stats": r.stats,
        "cursor": r.cursor,
        "created_at": iso(r.created_at),
        "updated_at": iso(r.updated_at),
        "started_at": iso_opt(r.started_at),
        "completed_at": iso_opt(r.completed_at),
        "abandoned_at": iso_opt(r.abandoned_at),
        "initiated_by": r.initiator_id.map(|id| json!({ "id": id, "name": r.initiator_name, "email": r.initiator_email })),
    })
}

fn log_json(l: &LogRow, with_kind: bool) -> Value {
    let mut v = json!({
        "id": l.id,
        "error_code": l.error_code,
        "message": l.message,
        "source_object_type": l.source_object_type,
        "source_object_id": l.source_object_id,
        "details": l.details,
        "created_at": iso(l.created_at),
    });
    if with_kind {
        v["kind"] = l.details.get("kind").cloned().unwrap_or(Value::Null);
    }
    v
}

/// (non-skip log count, skip log count) per import id.
async fn log_counts(db: &PgPool, ids: &[i64]) -> Result<HashMap<i64, (i64, i64)>, ApiError> {
    let rows: Vec<(i64, i64, i64)> = sqlx::query_as(&format!(
        "SELECT data_import_id, COUNT(*) FILTER (WHERE {NON_SKIP}), COUNT(*) FILTER (WHERE {SKIP})
         FROM data_import_errors WHERE data_import_id = ANY($1) GROUP BY data_import_id"
    ))
    .bind(ids.to_vec())
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(id, errs, skips)| (id, (errs, skips))).collect())
}

fn with_counts(mut v: Value, errs: i64, skips: i64) -> Value {
    v["import_errors_count"] = json!(errs);
    v["skip_logs_count"] = json!(skips);
    v
}

async fn find(db: &PgPool, account_id: i64, id: i64) -> Result<ImportRow, ApiError> {
    sqlx::query_as(&format!("{IMPORT_SELECT} WHERE d.account_id = $1 AND d.id = $2"))
        .bind(account_id)
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(ApiError::not_found)
}

/// Same payload as data_imports/show.json.jbuilder.
async fn detail(db: &PgPool, row: &ImportRow, skip_type: Option<&str>) -> Result<Value, ApiError> {
    let (errs, skips) = log_counts(db, &[row.id]).await?.get(&row.id).copied().unwrap_or((0, 0));
    let errors: Vec<LogRow> = sqlx::query_as(&format!("{LOG_SELECT} WHERE data_import_id = $1 AND {NON_SKIP} ORDER BY created_at DESC LIMIT 5"))
        .bind(row.id)
        .fetch_all(db)
        .await?;
    let selected = skip_type.filter(|t| SOURCE_TYPES.contains(t));
    let skip_rows: Vec<LogRow> = sqlx::query_as(&format!(
        "{LOG_SELECT} WHERE data_import_id = $1 AND {SKIP} AND ($2::text IS NULL OR source_object_type = $2) ORDER BY created_at DESC LIMIT 5"
    ))
    .bind(row.id)
    .bind(selected)
    .fetch_all(db)
    .await?;
    let by_type: Vec<(Option<String>, i64)> = sqlx::query_as(&format!(
        "SELECT source_object_type, COUNT(*) FROM data_import_errors WHERE data_import_id = $1 AND {SKIP} GROUP BY source_object_type"
    ))
    .bind(row.id)
    .fetch_all(db)
    .await?;
    let counts: Map<String, Value> = by_type.into_iter().map(|(t, c)| (t.unwrap_or_default(), json!(c))).collect();

    let mut v = with_counts(render(row), errs, skips);
    v["import_errors"] = json!(errors.iter().map(|l| log_json(l, false)).collect::<Vec<_>>());
    v["skip_logs"] = json!(skip_rows.iter().map(|l| log_json(l, true)).collect::<Vec<_>>());
    v["skip_logs_filters"] = json!({ "selected_source_object_type": selected, "counts_by_type": counts });
    Ok(v)
}

async fn index(Path(account_id): Path<i64>, user: CurrentUser, State(st): State<AppState>) -> ApiResult {
    admin(&st.db, &user, account_id).await?;
    let rows: Vec<ImportRow> = sqlx::query_as(&format!("{IMPORT_SELECT} WHERE d.account_id = $1 ORDER BY d.created_at DESC"))
        .bind(account_id)
        .fetch_all(&st.db)
        .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let counts = log_counts(&st.db, &ids).await?;
    let payload: Vec<Value> = rows
        .iter()
        .map(|r| {
            let (errs, skips) = counts.get(&r.id).copied().unwrap_or((0, 0));
            with_counts(render(r), errs, skips)
        })
        .collect();
    Ok(Json(json!({ "payload": payload })))
}

async fn show(Path((account_id, id)): Path<(i64, i64)>, Query(q): Query<ShowQuery>, user: CurrentUser, State(st): State<AppState>) -> ApiResult {
    admin(&st.db, &user, account_id).await?;
    let row = find(&st.db, account_id, id).await?;
    Ok(Json(detail(&st.db, &row, q.skip_logs_type.as_deref()).await?))
}

async fn ensure_file_table(db: &PgPool) -> Result<(), ApiError> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS pulsar_data_import_files (
            data_import_id bigint PRIMARY KEY REFERENCES data_imports(id) ON DELETE CASCADE,
            content text NOT NULL,
            created_at timestamp NOT NULL
        )",
    )
    .execute(db)
    .await?;
    Ok(())
}

async fn create(Path(account_id): Path<i64>, user: CurrentUser, State(st): State<AppState>, mut multipart: Multipart) -> ApiResult {
    admin(&st.db, &user, account_id).await?;
    let mut file: Option<Vec<u8>> = None;
    let mut name: Option<String> = None;
    while let Some(field) = multipart.next_field().await.map_err(|_| ApiError::bad("Invalid multipart"))? {
        let key = field.name().unwrap_or_default().to_string();
        match key.as_str() {
            "import_file" => file = Some(field.bytes().await.map_err(|_| ApiError::bad("Invalid CSV"))?.to_vec()),
            "name" => name = Some(field.text().await.map_err(|_| ApiError::bad("Invalid name"))?),
            "source_provider" => {
                if !field.text().await.map_err(|_| ApiError::bad("Invalid source"))?.trim().is_empty() {
                    return Err(ApiError::bad("unsupported source"));
                }
            }
            "data_type" => {
                let value = field.text().await.map_err(|_| ApiError::bad("Invalid data type"))?;
                if !value.trim().is_empty() && value.trim() != "contacts" {
                    return Err(ApiError::bad("unsupported source"));
                }
            }
            _ => {}
        }
    }
    let bytes = file.ok_or_else(|| ApiError::bad("import_file required"))?;
    let content = String::from_utf8(bytes).map_err(|_| ApiError::bad("Invalid CSV"))?;
    let rows = parse_csv(&content)?;
    let (_, data) = rows.split_first().ok_or_else(|| ApiError::bad("CSV header required"))?;
    let total = data.len() as i32;
    let name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("Contacts - {}", now().date()));
    ensure_file_table(&st.db).await?;

    let mut tx = st.db.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO data_imports (account_id, data_type, status, name, import_types, initiated_by_id, stats, cursor, source_metadata,
             total_records, processed_records, created_at, updated_at)
         VALUES ($1, 'contacts', $2, $3, '[]'::jsonb, $4, $5, '{}'::jsonb, '{}'::jsonb, $6, 0, $7, $7)
         RETURNING id::bigint",
    )
    .bind(account_id)
    .bind(PENDING)
    .bind(&name)
    .bind(user.id as i32)
    .bind(stats_json(total, 0, 0, 0))
    .bind(total)
    .bind(now())
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO pulsar_data_import_files (data_import_id, content, created_at) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(&content)
        .bind(now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    tokio::spawn(run_import(st.db.clone(), id));
    let row = find(&st.db, account_id, id).await?;
    Ok(Json(detail(&st.db, &row, None).await?))
}

async fn start(Path((account_id, id)): Path<(i64, i64)>, user: CurrentUser, State(st): State<AppState>) -> ApiResult {
    admin(&st.db, &user, account_id).await?;
    let row = find(&st.db, account_id, id).await?;
    // Only a pending import needs kicking. The claim in `process` stops a second worker from running it twice.
    if row.status == PENDING {
        tokio::spawn(run_import(st.db.clone(), id));
    }
    Ok(Json(detail(&st.db, &row, None).await?))
}

async fn retry(Path((account_id, id)): Path<(i64, i64)>, user: CurrentUser, State(st): State<AppState>) -> ApiResult {
    admin(&st.db, &user, account_id).await?;
    let row = find(&st.db, account_id, id).await?;
    let has_file: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pulsar_data_import_files WHERE data_import_id = $1)")
        .bind(id)
        .fetch_one(&st.db)
        .await?;
    if !(row.status == FAILED || row.status == ABANDONED) || !has_file {
        return Err(ApiError::bad("This import cannot be retried."));
    }
    let mut tx = st.db.begin().await?;
    sqlx::query("DELETE FROM data_import_errors WHERE data_import_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE data_imports SET status = $3, processing_errors = NULL, abandoned_at = NULL, processed_records = 0, updated_at = $4
         WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .bind(PENDING)
    .bind(now())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tokio::spawn(run_import(st.db.clone(), id));
    let row = find(&st.db, account_id, id).await?;
    Ok(Json(detail(&st.db, &row, None).await?))
}

async fn abandon(Path((account_id, id)): Path<(i64, i64)>, user: CurrentUser, State(st): State<AppState>) -> ApiResult {
    admin(&st.db, &user, account_id).await?;
    sqlx::query("UPDATE data_imports SET status = $3, abandoned_at = $4, updated_at = $4 WHERE id = $1 AND account_id = $2 AND status IN ($5, $6)")
        .bind(id)
        .bind(account_id)
        .bind(ABANDONED)
        .bind(now())
        .bind(PENDING)
        .bind(PROCESSING)
        .execute(&st.db)
        .await?;
    let row = find(&st.db, account_id, id).await?;
    Ok(Json(detail(&st.db, &row, None).await?))
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_response(body: String, filename: String) -> Response {
    (
        [(header::CONTENT_TYPE, "text/csv".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\""))],
        body,
    )
        .into_response()
}

async fn logs_csv(db: &PgPool, id: i64, filter: &str, filename: String) -> Result<Response, ApiError> {
    let logs: Vec<(i64, String, Option<String>, Option<String>, Option<String>, Value, NaiveDateTime)> = sqlx::query_as(&format!(
        "SELECT id, error_code, message, source_object_type, source_object_id, details, created_at FROM data_import_errors
         WHERE data_import_id = $1 AND {filter} ORDER BY created_at ASC"
    ))
    .bind(id)
    .fetch_all(db)
    .await?;
    let mut out = String::from("created_at,kind,source_object_type,source_object_id,error_code,message,details\r\n");
    for (_, error_code, message, object_type, object_id, details, created_at) in logs {
        let kind = details.get("kind").and_then(Value::as_str).unwrap_or_default();
        let fields = [
            created_at.and_utc().to_rfc3339_opts(SecondsFormat::Secs, true),
            kind.to_string(),
            object_type.unwrap_or_default(),
            object_id.unwrap_or_default(),
            error_code,
            message.unwrap_or_default(),
            details.to_string(),
        ];
        out.push_str(&fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    Ok(csv_response(out, filename))
}

async fn error_logs(Path((account_id, id)): Path<(i64, i64)>, user: CurrentUser, State(st): State<AppState>) -> Result<Response, ApiError> {
    admin(&st.db, &user, account_id).await?;
    find(&st.db, account_id, id).await?;
    logs_csv(&st.db, id, NON_SKIP, format!("data-import-{id}-error-logs.csv")).await
}

async fn skip_logs(Path((account_id, id)): Path<(i64, i64)>, user: CurrentUser, State(st): State<AppState>) -> Result<Response, ApiError> {
    admin(&st.db, &user, account_id).await?;
    find(&st.db, account_id, id).await?;
    logs_csv(&st.db, id, SKIP, format!("data-import-{id}-skip-logs.csv")).await
}

/// Background entry point. Failures are recorded on the import row instead of being dropped.
async fn run_import(db: PgPool, id: i64) {
    if let Err(e) = process(&db, id).await {
        let message = e.1;
        tracing::error!(import_id = id, error = %message, "data import failed");
        let _ = sqlx::query(
            "UPDATE data_imports SET status = $2, processing_errors = $3, last_error_at = $4, updated_at = $4
             WHERE id = $1 AND status IN ($5, $6)",
        )
        .bind(id)
        .bind(FAILED)
        .bind(message)
        .bind(now())
        .bind(PENDING)
        .bind(PROCESSING)
        .execute(&db)
        .await;
    }
}

async fn process(db: &PgPool, id: i64) -> Result<(), ApiError> {
    // Atomic claim: only one worker moves pending -> processing.
    let claimed = sqlx::query("UPDATE data_imports SET status = $2, started_at = $3, updated_at = $3 WHERE id = $1 AND status = $4")
        .bind(id)
        .bind(PROCESSING)
        .bind(now())
        .bind(PENDING)
        .execute(db)
        .await?
        .rows_affected();
    if claimed == 0 {
        return Ok(());
    }

    let (account_id, content): (i64, String) = sqlx::query_as(
        "SELECT d.account_id::bigint, f.content FROM data_imports d
         JOIN pulsar_data_import_files f ON f.data_import_id = d.id WHERE d.id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| ApiError::bad("Import file is missing"))?;

    let rows = parse_csv(&content)?;
    let (headers, data) = rows.split_first().ok_or_else(|| ApiError::bad("CSV header required"))?;
    let total = data.len() as i32;
    let (mut imported, mut skipped, mut failed) = (0, 0, 0);

    for (index, row) in data.iter().enumerate() {
        // Line number in the file: header is line 1.
        let line = (index + 2).to_string();
        let outcome = if row.len() != headers.len() {
            Outcome::Failed("column_count_mismatch", format!("Expected {} columns, found {}", headers.len(), row.len()))
        } else {
            match upsert_contact(db, account_id, headers, row).await {
                Ok(outcome) => outcome,
                Err(e) => {
                    tracing::warn!(import_id = id, line = %line, error = %e, "contact row failed");
                    Outcome::Failed("contact_save_failed", "Contact could not be saved".into())
                }
            }
        };
        match outcome {
            Outcome::Imported => imported += 1,
            Outcome::Skipped(code, message) => {
                skipped += 1;
                log_row(db, id, &line, "skipped", code, &message).await?;
            }
            Outcome::Failed(code, message) => {
                failed += 1;
                log_row(db, id, &line, "failed", code, &message).await?;
            }
        }

        let progress = sqlx::query("UPDATE data_imports SET processed_records = $2, stats = $3, updated_at = $4 WHERE id = $1 AND status = $5")
            .bind(id)
            .bind(index as i32 + 1)
            .bind(stats_json(total, imported, skipped, failed))
            .bind(now())
            .bind(PROCESSING)
            .execute(db)
            .await?
            .rows_affected();
        // Zero rows means the import was abandoned or restarted meanwhile. Stop here.
        if progress == 0 {
            return Ok(());
        }
    }

    let status = if failed > 0 { COMPLETED_WITH_ERRORS } else { COMPLETED };
    let done = sqlx::query(
        "UPDATE data_imports SET status = $2, processed_records = $3, stats = $4, completed_at = $5, updated_at = $5,
             last_error_at = CASE WHEN $6 THEN $5 ELSE last_error_at END
         WHERE id = $1 AND status = $7",
    )
    .bind(id)
    .bind(status)
    .bind(total)
    .bind(stats_json(total, imported, skipped, failed))
    .bind(now())
    .bind(failed > 0)
    .bind(PROCESSING)
    .execute(db)
    .await?
    .rows_affected();
    if done > 0 {
        // Raw file is only needed for retry, and retry is not offered after completion.
        sqlx::query("DELETE FROM pulsar_data_import_files WHERE data_import_id = $1").bind(id).execute(db).await?;
    }
    Ok(())
}

async fn log_row(db: &PgPool, id: i64, line: &str, kind: &str, code: &str, message: &str) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO data_import_errors (data_import_id, source_object_type, source_object_id, error_code, message, details, created_at, updated_at)
         VALUES ($1, 'contact', $2, $3, $4, $5, $6, $6)",
    )
    .bind(id)
    .bind(line)
    .bind(code)
    .bind(message)
    .bind(json!({ "kind": kind }))
    .bind(now())
    .execute(db)
    .await?;
    Ok(())
}

/// One CSV row. Same matching and attribute rules as `import_contacts` in contacts.rs, per row instead of all-or-nothing.
async fn upsert_contact(db: &PgPool, account_id: i64, headers: &[String], row: &[String]) -> Result<Outcome, sqlx::Error> {
    let mut attrs = Map::new();
    for (key, value) in headers.iter().zip(row) {
        if !value.trim().is_empty() {
            attrs.insert(key.clone(), json!(value.trim()));
        }
    }
    let get = |k: &str| attrs.get(k).and_then(Value::as_str).map(str::to_string);
    let name = get("name");
    let email = get("email");
    let identifier = get("identifier");
    let phone = get("phone_number").map(|p| if p.starts_with('+') { p } else { format!("+{p}") });
    if name.is_none() && email.is_none() && identifier.is_none() && phone.is_none() {
        return Ok(Outcome::Skipped("missing_contact_fields", "Row has no name, email, phone number or identifier".into()));
    }

    let existing: Option<i32> = sqlx::query_scalar(
        "SELECT id FROM contacts WHERE account_id=$1 AND (($2::text IS NOT NULL AND identifier=$2) OR ($3::text IS NOT NULL AND lower(email)=lower($3)) OR ($4::text IS NOT NULL AND phone_number=$4))
         ORDER BY CASE WHEN identifier=$2 THEN 0 WHEN lower(email)=lower($3) THEN 1 ELSE 2 END LIMIT 1",
    )
    .bind(account_id)
    .bind(&identifier)
    .bind(&email)
    .bind(&phone)
    .fetch_optional(db)
    .await?;

    let mut additional = Map::new();
    for key in ["company_name", "city", "country", "country_code"] {
        if let Some(v) = attrs.get(key) {
            additional.insert(key.into(), v.clone());
        }
    }
    for key in ["name", "email", "identifier", "phone_number", "labels"] {
        attrs.remove(key);
    }

    if let Some(id) = existing {
        sqlx::query("UPDATE contacts SET name=COALESCE($1,name),email=COALESCE($2,email),phone_number=COALESCE($3,phone_number),identifier=COALESCE($4,identifier),additional_attributes=COALESCE(additional_attributes,'{}'::jsonb)||$5,custom_attributes=COALESCE(custom_attributes,'{}'::jsonb)||$6,updated_at=$7 WHERE account_id=$8 AND id=$9")
            .bind(name).bind(email).bind(phone).bind(identifier).bind(json!(additional)).bind(json!(attrs)).bind(now()).bind(account_id).bind(id).execute(db).await?;
    } else {
        sqlx::query("INSERT INTO contacts (name,email,phone_number,identifier,account_id,additional_attributes,custom_attributes,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8)")
            .bind(name.unwrap_or_default()).bind(email).bind(phone).bind(identifier).bind(account_id).bind(json!(additional)).bind(json!(attrs)).bind(now()).execute(db).await?;
    }
    Ok(Outcome::Imported)
}
