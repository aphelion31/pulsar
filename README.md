# Pulsar

> **Note:** Pulsar is a Rust rewrite of [Chatwoot](https://github.com/chatwoot/chatwoot), the open source customer support platform.
> The Ruby on Rails + Sidekiq + Redis backend is replaced by one small Rust binary (axum + sqlx), and the Vue dashboard
> is replaced by a React Router frontend. Pulsar keeps Chatwoot's PostgreSQL schema and API shapes, so an existing
> Chatwoot database and API clients keep working.
> Chatwoot is © Chatwoot Inc., MIT licensed (see `LICENSE-CHATWOOT`). Pulsar is not affiliated with Chatwoot Inc.

## Why

Chatwoot needs Rails, Sidekiq, Redis and several GB of RAM. Pulsar runs the core inbox workflow in one process
of about 12 MB resident memory, with Postgres as the only dependency. Realtime (ActionCable protocol) runs in-process.

## Layout

```
src/          Rust backend (axum + sqlx), one module per Chatwoot API area
web/          React Router v8 frontend (SPA mode, Vite, Tailwind v4, lucide icons)
migrations/   PostgreSQL schema, generated from Chatwoot db/schema.rb by tools/schema2sql.rb
tools/        schema converter, API smoke test, browser E2E test
```

## Run

```bash
# 1. Database (needs PostgreSQL with pgvector)
createdb pulsar && psql -d pulsar -f migrations/0001_schema.sql

# 2. Frontend: build once into web/build/client (served by the binary at /app)
(cd web && pnpm install && pnpm build)

# 3. First admin user, account and API inbox
cargo run --release -- seed admin@example.com 'change-me-now' Acme

# 4. Serve
cargo run --release      # http://localhost:3000/app/login
```

Frontend development with hot reload: run the binary, then `cd web && PULSAR_URL=http://127.0.0.1:3000 pnpm dev`.
Vite proxies `/api`, `/auth`, `/rails` and the `/cable` websocket to the binary.

| Env | Default | Purpose |
| --- | --- | --- |
| `DATABASE_URL` | `postgres://localhost/pulsar` | Postgres connection |
| `BIND` | `0.0.0.0:3000` | Listen address |
| `WEB_DIR` | `web/build/client` | Built React frontend |
| `STORAGE_DIR` | `./storage` | Uploaded files (same layout as Rails ActiveStorage disk service) |
| `SECRET_KEY_BASE` | dev value | Signs file URLs and widget tokens. Set it in production |
| `FRONTEND_URL` | `http://localhost:3000` | Public base URL |

An existing Chatwoot database works too: point `DATABASE_URL` at it. Passwords (bcrypt) and login tokens
(devise_token_auth format) are compatible, so users keep their credentials. Test on a copy first.

## Scope

Ported: auth (devise_token_auth compatible), profile, accounts, realtime websocket (`/cable`, ActionCable protocol),
conversations, messages, file attachments and avatars, contacts, inboxes (API and web widget config), agents, teams,
labels, canned responses, custom attributes, custom filters, notifications, search, macros, automation rules (stored,
not executed yet), webhooks (stored), campaigns (stored, not sent), agent bots, dashboard apps,
companies, audit logs, custom roles, v2 reports, and the public website widget (`/sdk.js` embed).

Frontend screens: sign in, conversations (list, thread, reply, private note, attachments, assign, resolve, live
updates), contacts, settings, website chat widget.

Not ported: channel integrations (WhatsApp, Facebook, email IMAP/SMTP, etc.), help center, Captain AI, SLA,
enterprise features, mailers, MFA/SSO. Unported `/api` routes answer `501` and log `unported endpoint`.

Single node only: realtime pub/sub and presence are in-memory. Add Redis or Postgres LISTEN/NOTIFY fan-out before
running more than one instance.

## Contributing

Read [`AGENTS.md`](AGENTS.md) and [`docs/`](docs/ARCHITECTURE.md) first. Short version:

- `develop` is the default branch; `master` holds released code. Same model as Chatwoot.
- Work branches are `PLR-<number>/<type>-<description>`, cut from `develop`, for example
  `PLR-001/feat-boilerplate`. Create one with `scripts/new-branch.sh feat-<description>`.
- Commits and PR titles are Conventional Commits in English. No AI co-author lines.
- Install the hooks once: `git config core.hooksPath .githooks`.

Details: [commit](docs/github/01.GIT-COMMIT.md), [branch](docs/github/02.GIT-BRANCH.md),
[pull request and release](docs/github/03.GIT-PR-FLOW.md).

## Tests

```bash
cargo test                                  # unit tests
BASE=http://127.0.0.1:3000 tools/smoke.sh   # API smoke test against a running server
node tools/e2e.js                           # browser: sign in, reply, realtime delivery (needs playwright)
(cd web && pnpm typecheck)                  # frontend types
```

## License

MIT (see `LICENSE`). Includes schema and API compatibility derived from Chatwoot (MIT, © Chatwoot Inc., see
`LICENSE-CHATWOOT`). Chatwoot's `enterprise/` code is under a separate license and is not used.
