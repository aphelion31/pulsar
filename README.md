# Pulsar

> **Note:** Pulsar is a Rust rewrite of the [Chatwoot](https://github.com/chatwoot/chatwoot) backend.
> It reuses Chatwoot's PostgreSQL schema and serves Chatwoot's original Vue dashboard unchanged, so the UI is the same.
> The Ruby on Rails + Sidekiq + Redis stack is replaced by one small Rust binary (axum + sqlx).
> Chatwoot is © Chatwoot Inc., MIT licensed (see `LICENSE-CHATWOOT`). Pulsar is not affiliated with Chatwoot Inc.

## Why

Chatwoot needs Rails, Sidekiq, Redis and several GB of RAM. Pulsar runs the core inbox workflow in one ~15 MB process
with Postgres as the only dependency. Websocket realtime (ActionCable protocol) is in-process.

## Run

```bash
# 1. DB (schema generated from Chatwoot db/schema.rb by tools/schema2sql.rb)
createdb pulsar && psql -d pulsar -f migrations/0001_schema.sql   # needs pgvector
# 2. Frontend (once): build Chatwoot's Vue app
(cd ../chatwoot && pnpm install && npx vite build)                 # -> ../chatwoot/public/vite
# 3. Admin user + account + API inbox
cargo run --release -- seed admin@example.com 'Password123!' Acme
# 4. Serve
cargo run --release      # http://localhost:3000/app/login
```

Env: `DATABASE_URL` (default `postgres://localhost/pulsar`), `BIND` (`0.0.0.0:3000`), `PUBLIC_DIR` (`../chatwoot/public`), `FRONTEND_URL`.

An existing Chatwoot database works too: point `DATABASE_URL` at it. Passwords (bcrypt) and login tokens
(devise_token_auth format) are compatible, so users keep their credentials. Test on a copy first.

## Scope

Ported: auth (devise_token_auth compatible), profile, accounts, realtime websocket (`/cable`, ActionCable protocol),
conversations, messages, contacts, inboxes (API + web widget config), agents, teams, labels, canned responses,
custom attributes, custom filters, notifications, search, reports (in progress), companies (in progress).

Not ported (return `501` or empty data): channel integrations (WhatsApp, Facebook, email IMAP/SMTP, etc.),
file attachments / avatars, campaigns, automation, macros, help center, Captain AI, SLA, enterprise features,
public web widget API, mailers, MFA/SSO. Unported `/api` routes answer `501` and log `unported endpoint`, so gaps are visible.

Single node only: realtime pub/sub and presence are in-memory. Add Redis or Postgres LISTEN/NOTIFY fan-out before running more than one instance.

## Tests

```bash
cargo test                                  # unit checks (token auth, feature flags, config parse, filters)
BASE=http://127.0.0.1:3000 tools/smoke.sh   # API smoke test against a running server
node tools/e2e.js                           # browser: login, send reply, realtime delivery (needs playwright)
```

## License

MIT. Includes schema and protocol compatibility derived from Chatwoot (MIT, © Chatwoot Inc., see `LICENSE-CHATWOOT`).
Chatwoot's `enterprise/` code is under a separate license and is not used.
