# Architecture Decisions (Chatwoot -> Pulsar)

## ADR-001: Single-binary Rust backend + React Router v8 SPA
Status: accepted (2026-11-25)
Context: Chatwoot runs Rails + Sidekiq + Redis + ActionCable + Vue/Vite across multiple processes. Pulsar already replaces the backend with one Axum 0.8 + sqlx 0.8 + Tokio binary and PostgreSQL as the only external dependency.
Decision: Complete the whole-app parity port directly inside `pulsar/` (`src/` Axum backend + `web/` React Router v8 SPA), keeping `chatwoot/` read-only as ground-truth reference.
Consequences: No Redis, Sidekiq, or Rails runtime required; async jobs and event listeners run in-process backed by PostgreSQL (`pulsar_events` and `scheduler`).

## ADR-002: Consolidated baseline from origin/develop + PLR-087 + PLR-088
Status: accepted (2026-11-25)
Context: Local `develop` was at `93601e4` (#31) while `origin/develop` (`413f67c`, #55) already implemented Rounds 1-6, and `PLR-087`/`PLR-088` implemented WhatsApp Cloud and Bandwidth SMS. Local WIP on `93601e4` is preserved in git stash `wip-backup-on-93601e4-2026-11-25`.
Decision: Branch `PLR-090/feat-chatwoot-full-parity` from `origin/develop` and merge `PLR-087` and `PLR-088` before porting Waves 1-5.
Consequences: Zero rework of Rounds 1-6 and WhatsApp/SMS channels; WIP My Inbox / runtime harness patterns salvaged from stash.

## ADR-003: PostgreSQL schema, auth, API JSON, and ActionCable parity bar
Status: accepted (2026-11-25)
Context: Existing Chatwoot databases, API clients, and widgets must work seamlessly against Pulsar.
Decision: Keep `migrations/0001_schema.sql` compatible with Chatwoot `db/schema.rb`, `devise_token_auth` (`users.tokens`), `api_access_token`, HS256 widget tokens, jbuilder JSON shapes (string enums, unix-second timestamps, `display_id`), and ActionCable `RoomChannel` event payloads.
Consequences: Handlers filter every query by `account_id` after `require_account`, cast `integer` IDs as `::bigint`, and bind all SQL values.

## ADR-004: React Router v8 SPA frontend over HTTP JSON only
Status: accepted (2026-11-25)
Context: All Chatwoot Vue interfaces (dashboard, portal, widget, survey, onboarding, super_admin) are being migrated to React Router v8 in `web/`.
Decision: Replace all `placeholder.tsx` (`ph(...)`) routes with working React Router v8 routes using `api()`/`acct()` in `web/app/lib/api.ts`, Tailwind v4, and existing `ui.tsx` primitives.
Consequences: No shared Rust/TS codegen; HTTP JSON + `/cable` WebSocket are the sole contract.

## ADR-005: Scope — OSS + self-hosted Enterprise + Super Admin; drop SaaS/Ruby-only tooling
Status: accepted (2026-11-25)
Context: Chatwoot includes Ruby runtime machinery (`bootsnap`, `puma`, `sidekiq*`), APM/SaaS billing (`datadog`, `newrelic*`, `scout_apm`, `elastic-apm`, `judoscale*`, `stripe` SaaS billing webhooks), and `administrate`.
Decision: Implement 100% of OSS + self-hosted Enterprise features (SLA, Capacity Policies, SAML, Captain AI, Monitors, Voice/Calls, all Channels & Integrations, Help Center) plus Super Admin & Installation Onboarding; drop Ruby/SaaS-billing/APM machinery and provide self-hosted unlimited defaults for enterprise limits.
Consequences: Every user-facing dashboard, public, widget, platform, and super-admin flow works standalone.
