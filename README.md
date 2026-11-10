# chatwoot-rs

Rust (axum + sqlx) backend for Chatwoot. Same Postgres schema, same Vue dashboard (unchanged UI).
One ~15 MB binary replaces Rails + Sidekiq + Redis for the core inbox workflow.

## Run

```bash
# 1. DB (schema generated from chatwoot db/schema.rb)
createdb chatwoot_rs && psql -d chatwoot_rs -f migrations/0001_schema.sql   # needs pgvector
# 2. Frontend (once): build the original Vue app
(cd ../chatwoot && pnpm install && npx vite build)                            # -> ../chatwoot/public/vite
# 3. Admin user + account + API inbox
cargo run --release -- seed admin@example.com 'Password123!' Acme
# 4. Serve
cargo run --release      # http://localhost:3000/app/login
```

Env: `DATABASE_URL` (default `postgres://localhost/chatwoot_rs`), `BIND` (`0.0.0.0:3000`), `PUBLIC_DIR` (`../chatwoot/public`), `FRONTEND_URL`.
Existing Chatwoot DB works too: point `DATABASE_URL` at it (bcrypt passwords and devise tokens are compatible).

## Scope

Ported: auth (devise_token_auth compatible), profile, accounts, ActionCable websocket (`/cable`, in-process),
conversations, messages, contacts, inboxes (API + web widget config), agents, teams, labels, canned responses,
custom attributes, custom filters, notifications, search.

Not ported (return 501 or empty): channel integrations (WhatsApp, FB, email IMAP/SMTP, etc), file attachments / avatars,
reports, campaigns, automation, help center, Captain AI, SLA, enterprise, web widget public API, mailers, MFA/SSO.
Unported `/api` routes answer `501` and log `unported endpoint`, so gaps are visible.

## Tests

```bash
cargo test                                  # unit checks (token auth, feature flags, config parse)
node tools/e2e.js                           # browser login + dashboard via playwright (BASE=http://127.0.0.1:3000)
```
