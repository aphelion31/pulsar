# AGENTS.md

Guidance for AI agents working in this repository.

## What this is

Pulsar is a lightweight rewrite of [Chatwoot](https://github.com/chatwoot/chatwoot): a Rust backend and a
React Router frontend shipped as one binary, with PostgreSQL as the only dependency. It keeps Chatwoot's
database schema and API shapes, so an existing Chatwoot database and API clients keep working.

| Path          | Role                                                                             |
| ------------- | -------------------------------------------------------------------------------- |
| `src/`        | Rust backend: axum HTTP + websocket, sqlx Postgres. One module per API area      |
| `web/`        | React Router v8 frontend in SPA mode (Vite, Tailwind v4, lucide icons)           |
| `migrations/` | PostgreSQL schema, generated from Chatwoot `db/schema.rb` by `tools/schema2sql.rb` |
| `tools/`      | Schema converter, API smoke test, browser E2E tests                              |
| `scripts/`    | Git hook checks and the branch helper                                            |
| `docs/`       | Architecture and process docs                                                    |

Read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) before making structural changes.

## Non-negotiables

**Every account-scoped handler calls `require_account` first and filters every query by `account_id`.**
Tenant isolation lives in the handler, not the database. A query without the account filter leaks one
customer's data to another.

**Bind every value.** Use `sqlx::query(...).bind(...)`. The only text ever formatted into SQL is a constant
(column lists like `CONV_COLS`) or a value picked from a fixed whitelist (`ORDER BY`, `date_trunc` units).
Never format request input into SQL.

**Agents (role 0) only see conversations in inboxes they are members of.** Admins (role 1) see the whole
account. Any new endpoint that touches conversations or messages enforces the same rule.

**Response shapes match Chatwoot's jbuilder views.** API clients built for Chatwoot must keep working. When
in doubt, read the matching view in the Chatwoot repository before writing a handler.

**The frontend talks to the backend over HTTP only.** `web/` never shares code or types with `src/`; the
contract is the JSON API.

## Adding a feature

Backend endpoint:

1. Pick the module that owns the area (`src/conversations.rs`, `src/contacts.rs`, ...) or add a new one.
2. Add the handler: `State(st): State<AppState>`, `user: CurrentUser`, then
   `let au = require_account(&st.db, &user, account_id).await?;` and `au.require_admin()?` for admin actions.
3. Register the route in the module's `routes()`. Paths are relative to `/api/v1/accounts/{account_id}`.
   A new module is merged in `account_routes()` in `src/main.rs`.
4. Broadcast realtime changes through `st.hub` (`to_inbox`, `to_account`, `invalidate_cache`), the same
   events Chatwoot's `action_cable_listener.rb` sends.
5. Add the endpoint to `tools/smoke.sh` and run it.

Frontend screen:

1. Add the route module in `web/app/routes/` and register it in `web/app/routes.ts`.
2. Read shared state with `useOutletContext<ShellCtx>()` (current user, account id, realtime `subscribe`).
3. Use `api()` and `acct()` from `web/app/lib/api.ts`, and the primitives in `web/app/components/ui.tsx`.
4. A `useEffect` must return nothing or a cleanup function. `useEffect(() => fn(), [])` that returns a value
   crashes the production build.

## Commands

```bash
cargo build                                  # backend
cargo test                                   # unit tests
cargo run -- seed admin@example.com 'pass' Acme
cargo run                                    # http://localhost:3000/app/login

cd web && pnpm install && pnpm build         # frontend into web/build/client
cd web && pnpm typecheck
cd web && PULSAR_URL=http://127.0.0.1:3000 pnpm dev   # hot reload, proxies API to the binary
```

```bash
BASE=http://127.0.0.1:3000 tools/smoke.sh                    # API flows over real HTTP
BASE=http://127.0.0.1:3000 node tools/e2e.js                 # sign in, reply, realtime
BASE=... WEBSITE_TOKEN=... node tools/e2e-widget.js          # website widget end to end
```

Verification before claiming work is done: `cargo build`, `cargo test`, `pnpm typecheck`, then exercise the
change over HTTP against a running binary. A passing build is not evidence a feature works.

## Conventions

Branches are `PLR-<number>/<type>-<description>` cut from `origin/develop` (`scripts/new-branch.sh`).
Commits are Conventional Commits in English. Both are enforced by the hooks in `.githooks/`, along with a
block on direct pushes to `develop` and `master`. See [`docs/github/`](docs/github/).

Rust: `cargo fmt` style. TypeScript: two-space indent, double quotes, as the existing files.

## Traps worth knowing

- **Chatwoot integer columns.** `serial`/`integer` columns decode as `i32`; select them as `id::bigint` when
  the Rust side uses `i64`. A mismatch is a runtime decode error, not a compile error.
- **axios array params.** The Chatwoot clients send `labels[]=a&labels[]=b`; `serde` query structs do not
  read that form. Parse `RawQuery` with `url::form_urlencoded` when a filter takes an array.
- **Realtime is in-process.** Presence and pub/sub live in memory, so one instance only. Add Postgres
  LISTEN/NOTIFY or Redis fan-out before running two.
- **`SECRET_KEY_BASE` must be set in production.** It signs file URLs and widget visitor tokens; the
  fallback is a public dev value.
- **The server needs a moment to boot.** Tests that start the binary and immediately call it get empty
  responses; wait for `pulsar listening on` in the log.
