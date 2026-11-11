# Pulsar

The lightweight customer support platform: a Rust rewrite of [Chatwoot](https://github.com/chatwoot/chatwoot),
the open source alternative to Intercom, Zendesk and Salesforce Service Cloud.

<p>
  <a href="https://github.com/aphelion31/pulsar/actions/workflows/ci.yml"><img src="https://github.com/aphelion31/pulsar/actions/workflows/ci.yml/badge.svg?branch=develop" alt="CI"></a>
  <img src="https://img.shields.io/github/commit-activity/m/aphelion31/pulsar" alt="Commits per month">
  <img src="https://img.shields.io/badge/backend-Rust-orange" alt="Rust">
  <img src="https://img.shields.io/badge/frontend-React%20Router-blue" alt="React Router">
  <img src="https://img.shields.io/github/license/aphelion31/pulsar" alt="MIT license">
</p>

> **Note:** Pulsar is a Rust rewrite of Chatwoot. The Ruby on Rails + Sidekiq + Redis backend is replaced by one
> small Rust binary (axum + sqlx), and the Vue dashboard by a React Router frontend. Pulsar keeps Chatwoot's
> PostgreSQL schema and API shapes, so an existing Chatwoot database and API clients keep working.
> Chatwoot is © Chatwoot Inc., MIT licensed (see `LICENSE-CHATWOOT`). Pulsar is not affiliated with Chatwoot Inc.

---

Pulsar is a self-hosted customer support platform that runs as a single process with PostgreSQL as its only
dependency. It gives you Chatwoot's inbox, contacts and live chat with a fraction of the memory, disk and CPU,
and full control over your customer data.

### ⚡ One binary, about 20 MB of RAM

No Rails, no Sidekiq, no Redis. The backend, the realtime websocket (ActionCable protocol) and the web app are
served by one 9 MB binary. See [Benchmark](#benchmark) for measured numbers against Chatwoot.

### 💬 Shared inbox

All conversations in one inbox: reply, private notes, attachments, assignment, labels, priority, snooze and
resolve, with live updates for every agent. Channels today are the website live chat widget and the API inbox.

### 🔌 Drop-in for Chatwoot

Point Pulsar at a Chatwoot database and users keep their passwords and login tokens (bcrypt and
devise_token_auth compatible). The account API (`/api/v1/accounts/...`), v2 reports and the widget API follow
Chatwoot's request and response shapes.

### 🗂️ Other features

#### Collaboration & Productivity

- Private notes for internal team discussions.
- Labels to organize and categorize conversations.
- Canned responses to reply faster to frequently asked questions.
- Teams, agents and inbox membership.
- Macros to run a set of actions on a conversation in one click.
- Custom filters and custom views.

#### Customer Data & Segmentation

- Contact management with profiles, notes and conversation history.
- Companies linked to contacts.
- Custom attributes for contacts and conversations.
- Contact labels and filters.

#### Integrations

- Website live chat widget, embedded with one `<script>` tag (`/sdk.js`).
- Agent bots and dashboard apps (stored, Chatwoot compatible).
- Webhooks and API inbox (stored. Delivery is on the roadmap).

#### Reports & Insights

- Conversation, agent, inbox, label and team reports (`/api/v2`).
- Live view of open conversation metrics.
- Audit logs and custom roles.

## Benchmark

Chatwoot `v4.15.1` (official Docker image, Puma with default settings, `ENABLE_RACK_ATTACK=false`) against Pulsar
on the **same machine and the same data**: 200 contacts, 200 conversations, 2,000 messages, one shared
PostgreSQL 16 (pgvector). Every app container was capped at 2 CPU and 2 GB. Host: 8 vCPU x86_64, Ubuntu 22.04.
Load generator: ApacheBench, 10 concurrent connections with keep-alive, authenticated with `api_access_token`,
after a warm-up run. Each number is the median of 3 runs.

### Size and memory

| | Chatwoot | Pulsar | |
| --- | --- | --- | --- |
| Docker image | 709 MB (2.84 GB on disk) | 9 MB binary + 0.5 MB web app | **~80x smaller** |
| Processes to run | Rails + Sidekiq + Redis | 1 | |
| Memory, web after load | 364 MB | 19 MB | **19x less** |
| Memory, background jobs | 591 MB (Sidekiq) | none needed | |
| Memory, Redis | 17 MB | none needed | |
| **Memory total** | **972 MB** | **19 MB** | **51x less** |
| Ready to serve after start | 27.9 s | 1.4 s | **20x faster** |

### Throughput and latency

| Endpoint | Chatwoot req/s | Pulsar req/s | Speedup | Chatwoot p50 / p95 | Pulsar p50 / p95 |
| --- | ---: | ---: | ---: | --- | --- |
| `GET /api/v1/profile` | 32 | 2,412 | **75x** | 311 / 365 ms | 3 / 5 ms |
| `GET /conversations/meta` | 44 | 2,089 | **48x** | 229 / 272 ms | 3 / 8 ms |
| `GET /contacts` | 21 | 421 | **20x** | 462 / 602 ms | 19 / 46 ms |
| `GET /conversations/{id}/messages` | 20 | 227 | **11x** | 498 / 624 ms | 39 / 63 ms |
| `GET /conversations` (25 per page) | 1.6 | 30 | **19x** | 5,017 / 7,829 ms | 317 / 366 ms |
| `POST /conversations/{id}/messages` | 17 | 191 | **11x** | 576 / 682 ms | 45 / 113 ms |

Both apps returned the same records with the same fields (Pulsar adds a few extra keys) and zero failed requests.
Pulsar's payloads were slightly larger, so the speedup is not from sending less data.

Read these numbers fairly:

- Pulsar does less work per request. It has no channel integrations, no Sidekiq jobs and no Captain AI yet
  (see [Scope](#scope)). Chatwoot's numbers include that machinery.
- Chatwoot was measured with default Puma settings (one process, five threads). More Puma workers raise
  its throughput and its memory together.
- Pulsar was built for glibc (`x86_64-unknown-linux-gnu`). A musl static build is the same speed but its
  allocator grew to 600 MB under the conversation list load, so use glibc or add jemalloc for musl.
- Both apps used the same database, so Postgres time is in both columns.

Reproduce: start both apps on one database and run `ab -k -c 10 -n 1000 -H "api_access_token: <token>" <url>`.

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): how the backend, realtime and frontend fit together.
- [`docs/architectures/`](docs/architectures/): structure, Chatwoot compatibility notes and deployment.
- [`AGENTS.md`](AGENTS.md): working rules for contributors and coding agents.

## Branching model

We use the [git-flow](https://nvie.com/posts/a-successful-git-branching-model/) branching model, same as Chatwoot.
The base branch is `develop`. If you are looking for a stable version, use `master` or tags labelled `vX.Y.Z`.

Work branches are `PLR-<number>/<type>-<description>`, for example `PLR-001/feat-boilerplate`. Create one with
`scripts/new-branch.sh feat-<description>`. Commits and PR titles are Conventional Commits in English. PRs go into
`develop` and are squash merged. Install the hooks once: `git config core.hooksPath .githooks`.

Details: [commit](docs/github/01.GIT-COMMIT.md), [branch](docs/github/02.GIT-BRANCH.md),
[pull request and release](docs/github/03.GIT-PR-FLOW.md).

## Deployment

You need PostgreSQL with the `pgvector` extension. Nothing else.

```bash
# 1. Database
createdb pulsar
psql -d pulsar -f migrations/0001_schema.sql
psql -d pulsar -f migrations/0002_pulsar_indexes.sql

# 2. Build the frontend once (served by the binary at /app)
(cd web && pnpm install --frozen-lockfile && pnpm build)

# 3. First admin user, account and API inbox
cargo run --release -- seed admin@example.com 'change-me-now' Acme

# 4. Serve
SECRET_KEY_BASE=$(openssl rand -hex 64) cargo run --release   # http://localhost:3000/app/login
```

| Env | Default | Purpose |
| --- | --- | --- |
| `DATABASE_URL` | `postgres://localhost/pulsar` | Postgres connection |
| `BIND` | `0.0.0.0:3000` | Listen address |
| `WEB_DIR` | `web/build/client` | Built React frontend |
| `STORAGE_DIR` | `./storage` | Uploaded files (same layout as Rails ActiveStorage disk service) |
| `SECRET_KEY_BASE` | dev value | Signs file URLs and widget tokens. **Set it in production** and keep it stable |
| `FRONTEND_URL` | `http://localhost:3000` | Public base URL |

Put a TLS reverse proxy (Caddy, Traefik, nginx) in front and pass websocket upgrades on `/cable`.
Embed the live chat on a website with `<script src="https://your-pulsar/sdk.js" data-website-token="..."></script>`.
More in [docs/architectures/04.DEPLOY.md](docs/architectures/04.DEPLOY.md).

### Existing Chatwoot database

Pulsar targets the Chatwoot 4.19 schema and logs a warning on start if the database is older. Upgrade Chatwoot
first, test on a copy, and stop Chatwoot's Sidekiq before switching so two systems do not process the same data.

### Frontend development

Run the binary, then `cd web && PULSAR_URL=http://127.0.0.1:3000 pnpm dev`. Vite proxies `/api`, `/auth`, `/rails`
and the `/cable` websocket to the binary.

## Scope

**Ported:** auth (devise_token_auth compatible), profile, accounts, realtime websocket, conversations, messages,
attachments and avatars, contacts, companies, inboxes (API and website widget), agents, teams, labels, canned
responses, custom attributes, custom filters, notifications, search, macros, automation rules (stored), webhooks
(stored), campaigns (stored), agent bots, dashboard apps, audit logs, custom roles, v2 reports, website widget.

**Not ported yet:** channel integrations (WhatsApp, Facebook, Instagram, email, Telegram, SMS and others), webhook
delivery and the public API, running automation rules and campaigns, mailers and password reset, help center,
Captain AI, SLA, CSAT, MFA and SSO, super admin. Unported `/api` routes answer `501`.

**Single node only:** realtime pub/sub and presence are in memory. Add Redis or Postgres `LISTEN/NOTIFY` fan-out
before running more than one instance.

## Tests

```bash
cargo test                                            # unit tests
BASE=http://127.0.0.1:3000 tools/smoke.sh             # API smoke test against a running server
BASE=http://127.0.0.1:3000 tools/chatwoot-issues.sh   # regression checks for bugs reported on Chatwoot
node tools/e2e.js                                     # browser: sign in, reply, realtime (needs Playwright)
node tools/e2e-widget.js                              # browser: website widget round trip
(cd web && pnpm typecheck)                            # frontend types
```

## Security

Found a vulnerability? Please do not open a public issue. Report it privately through
[GitHub security advisories](https://github.com/aphelion31/pulsar/security/advisories/new).

## Contributors

Thanks goes to everyone who [contributes](https://github.com/aphelion31/pulsar/graphs/contributors), and to the
Chatwoot team and community, whose product, schema and API Pulsar is built on.

*Pulsar* is released under the MIT License (see `LICENSE`). It includes schema and API compatibility derived from
Chatwoot (MIT, © Chatwoot Inc., see `LICENSE-CHATWOOT`). Chatwoot's `enterprise/` code is under a separate license
and is not used.
