# chatwoot-rs worker contract

Goal: Rust (axum 0.8 + sqlx 0.8 + Postgres) backend that serves the UNCHANGED Chatwoot Vue dashboard.
Original Rails app (read-only reference): `../chatwoot`. Never edit `../chatwoot`.
DB: `postgres://localhost/chatwoot_rs` (schema = chatwoot `db/schema.rb`, see `migrations/0001_schema.sql`).

## Rules
- Only edit the module file(s) you own. Do not touch `main.rs`, `common.rs`, `auth.rs`, `cable.rs`, `web.rs`, `Cargo.toml`.
  If you need something from those, message the coordinator.
- Your module exposes `pub fn routes() -> Router<AppState>` with paths RELATIVE to `/api/v1/accounts/{account_id}`,
  e.g. `.route("/conversations", get(index))`. Extract account id with `Path(account_id): Path<i64>` (or a tuple
  `Path((account_id, id)): Path<(i64, i64)>`).
- Every handler: `user: CurrentUser` extractor, then `let au = require_account(&st.db, &user, account_id).await?;`.
  Admin-only actions: `au.require_admin()?`. ALWAYS filter queries by `account_id` (tenant isolation). Bind params, never format user input into SQL.
- State: `State(st): State<AppState>` gives `st.db` (PgPool) and `st.hub` (realtime). Errors: return `ApiResult` (`Result<Json<Value>, ApiError>`); `?` on sqlx works.
- Use runtime `sqlx::query`/`query_as`/`query_scalar` (NOT the `query!` macros, no DATABASE_URL at compile time).
  Column types: `serial`/`integer` = i32 (cast `id::bigint` in SELECT if you want i64), `bigint` = i64, `timestamp` = `chrono::NaiveDateTime`,
  `json`/`jsonb` = `serde_json::Value` (cast `json` cols to `::jsonb` when comparing).
- Timestamps in JSON: most Chatwoot payloads use unix seconds (`common::ts(opt)`), match the jbuilder.
- JSON shape MUST match the jbuilder views in `../chatwoot/app/views/api/v1/...` and what the Vue store expects
  (`../chatwoot/app/javascript/dashboard/api/*.js`, `store/modules/*.js`). Read them before writing a handler.
- Realtime: after mutations, broadcast like `app/listeners/action_cable_listener.rb` does:
  `st.hub.to_inbox(&st.db, account_id, inbox_id, contact_inbox_id, "message.created", data).await` or
  `st.hub.to_account(account_id, "contact.created", data)`. Cacheable models (label, inbox, team): call `st.hub.invalidate_cache(account_id, "label")` after create/update/delete.
- Ponytail: smallest correct implementation. Skip enterprise/captain/SLA/integrations. Mark skipped bits with `// ponytail: ...`.
- Verify: `cargo build` clean (no errors), then run server `./target/debug/chatwoot-rs` (port 3000; kill your own instance after,
  or use `BIND=127.0.0.1:30xx` to avoid clashes) and curl your endpoints. Login: 
  `curl -s -D - -H 'content-type: application/json' -d '{"email":"admin@example.com","password":"Password123!"}' localhost:3000/auth/sign_in`
  then pass headers `access-token`, `client`, `uid`. Or use header `api_access_token: <data.access_token from login>`.
- Commit only your files: `git add src/<yourmodule>.rs && git commit -m "..."`. Others commit concurrently, do not `git add -A`.

## Helpers in common.rs / auth.rs / cable.rs
- `common::{AppState, ApiError, ApiResult, CurrentUser, AccountUser, require_account, ts, now}`
- `ApiError::{not_found(), unauthorized(), forbidden(), bad(msg)}`
- `auth::{role_name, availability_name}`; `cable::presence_snapshot().is_online(account_id, user_id)`
