# Chatwoot -> Pulsar Parity Audit & TODO

> Audit baseline: `pulsar` `develop` @ `5e73760` (`v1.0.0`) vs reference `/Users/beyondb/Halim/Projects/CEG/prjtss/chatwoot` (`192a2488f7`).
> Shipped frontend verification via REA CLI (`rea analyze-javascript-application web/build/client --json`).

---

## 1. Audit Evidence & Codebase Metrics

### REA Static Bundle Evidence (`web/build/client`)

- **Evidence ID**: `ev_4c9ebee020db319425de2f645d9a55be2241098d381ee574461c4470869af5aa`
- **Container SHA-256**: `e316afd2e109141ab76e1ce4bc1f9291566ddcb2f0e108fe39ccd5b72720b632`
- **Provider**: `rea-javascript-application` (`v1`)
- **Measured bundle statistics**:
  - `155` relevant files inspected (`154` JS chunks + `sdk.js`, plus `index.html`)
  - `2,546,626` text bytes read, `458,810` AST nodes visited
  - `0` parse failures, `0` skipped JS files, `0` invalid UTF-8 files, `0` integrity contradictions
- **Explicit REA limitations**: CommonJS/ESM bindings were recovered statically from inert text; bundle bootstrap code was never executed, and unavailable facts remain explicit rather than treated as absence.

### Codebase Size Breakdown (Raw vs Production Code)

| Layer | Chatwoot (Reference) | Pulsar (Rewrite) | Ratio & Notes |
| --- | --- | --- | --- |
| **Backend total** | `3,722` files · `301,161` LOC (`.rb`/`.erb`/`.jbuilder`/`.rake`) | `65` files · `48,298` LOC (`src/*.rs`) | **6.2x** raw LOC (`511` Axum `.route()` calls / `508` unique patterns / ~`1,411` verb bindings) |
| **Backend non-test, non-migration** | `2,282` files · `119,006` LOC (excl. `1,212` `spec/` files = `176,176` LOC & `228` `db/` files = `5,979` LOC) | `65` files · `48,298` LOC (`src/` includes unit tests + SQL + handlers + serializers) | **2.46x** production LOC |
| **Frontend total** | `2,541` files · `334,067` LOC (`.vue`/`.js`/`.ts`/`.scss`/`.css` in `app/javascript`) | `155` files · `25,797` LOC (`.ts`/`.tsx`/`.css` in `web/app`) | **12.9x** raw LOC |
| **Frontend non-test, non-storybook** | `1,813` files · `230,814` LOC (excl. `631` tests/specs = `96,092` LOC & `97` Storybook = `7,161` LOC; includes `183` legacy components + `226` Vuex/API clients) | `155` files · `25,797` LOC (`92` routes = `17,266` LOC, `53` components = `7,243` LOC, `6` lib = `435` LOC, `4` root = `853` LOC; `167` `route()` entries) | **8.9x** non-test LOC (single unified React UI vs Vue dual legacy + Next + Vuex SFCs) |
| **Locale JSON files** | `2,648` files (40+ languages) | `47` `en/` modules + `8` synced runtime locale bundles (`id`, `es`, `fr`, `de`, `pt_BR`, `ja`, `zh_CN`, `ar` incl. RTL) | Synced via `tools/sync-locales.mjs` |

---

## 2. Sudah Diconvert (Done Inventory)

### 2.1 Auth, Sessions, MFA, SAML SSO, Onboarding & Accounts

- **Rust (`src/`)**:
  - `src/auth.rs` (`1,210` LOC): `devise_token_auth` sessions (`/auth/sign_in`, `/auth/sign_out`, `/auth/validate_token`), `bcrypt` parity, `api_access_token`, feature flag bitmasks, `/api/v1/profile`, notifications & availability.
  - `src/mfa.rs` (`488` LOC): TOTP RFC-6238 enrollment/verification, encrypted secrets, YAML backup codes.
  - `src/saml.rs` (`405` LOC): IdP XML metadata parsing, `/api/v1/accounts/{id}/saml_settings`, `/auth/saml_login`.
  - `src/registration.rs` (`552` LOC): `/auth` signup, confirmation, resend confirmation, password reset (`/auth/password`).
  - `src/account_more.rs` (`469` LOC): account CRUD, branded email layout, `/api/v1/accounts/{id}` settings.
- **React Router (`web/app/`)**:
  - `routes/login.tsx` (email/password + inline MFA + SAML SSO button), `routes/login.sso.tsx`, `routes/auth.signup.tsx`, `routes/auth.confirmation.tsx`, `routes/auth.verify-email.tsx`, `routes/auth.reset.tsx`, `routes/auth.password-edit.tsx`, `routes/installation.onboarding.tsx`, `routes/r2/onboarding.tsx`, `routes/r2/onboarding-inbox-setup.tsx`, `routes/settings/account.tsx`, `routes/settings/profile.tsx`, `routes/settings/r1/security*.tsx`, `routes/settings/r1/profile-mfa.tsx`, `routes/settings/r1/suspended.tsx`, `routes/settings/r1/no-accounts.tsx`, `components/next/AuthShell.tsx`.

### 2.2 Conversations, Messages, Drafts, Attachments, Search & Realtime

- **Rust (`src/`)**:
  - `src/conversations.rs` (`3,017` LOC): conversation list/meta/filters/search, labels, assignments, participants, snooze/resolve/mute, toggle priority/typing, transcript email, drafts (`migrations/0004_pulsar_draft_messages.sql`), messages/attachments, translate hooks.
  - `src/cable.rs` (`367` LOC): `/cable` ActionCable-compatible WebSocket hub & presence tracking.
  - `src/storage.rs` (`869` LOC): Rails ActiveStorage-compatible disk storage, signed IDs & direct uploads.
  - `src/events.rs` (`158` LOC): durable async dispatcher (`migrations/0005_pulsar_events.sql`).
- **React Router (`web/app/`)**:
  - `routes/inbox.tsx`, `routes/inbox-view.tsx`, `routes/r2/custom-view.tsx`, `routes/r2/search.tsx`.
  - `components/next/conversation/ReplyBox.tsx` (Markdown toolbar, `/` canned responses, `@` mentions, emoji picker, Help Center article insert, browser `MediaRecorder` audio note, Reply/Private Note tabs, AI Copilot), `components/next/conversation/ConversationSidebar.tsx` (Accordion panels, macro execute, integration cards), `components/next/CommandBar.tsx` (`⌘K`), `components/next/KeyboardShortcutsModal.tsx` (`?`), `components/next/sidebar/*` (`Sidebar.tsx`, `SidebarAccountSwitcher.tsx`, `SidebarProfileMenu.tsx`).

### 2.3 Contacts, Companies, Custom Attributes & Filters

- **Rust (`src/`)**:
  - `src/contacts.rs` (`2,110` LOC): CRUD, filter, search, active presence, merge (`actions/contact_merges`), labels, notes, attachments, contact inboxes, custom attributes.
  - `src/companies.rs` (`1,581` LOC): companies CRUD, notes, contacts/conversations links, enrichments.
- **React Router (`web/app/`)**:
  - `routes/contacts.tsx`, `routes/contacts.detail.tsx`, `routes/companies.tsx`, `routes/companies.detail.tsx`, `routes/settings/attributes.tsx`, `routes/settings/r1/custom-filters` views.

### 2.4 All 12 Messaging Channels & Inbound/Outbound Listeners

- **Rust (`src/`)**:
  - Website Widget: `src/widget.rs` (`1,704` LOC) + `/sdk.js` + `/widget` + `/api/v1/widget/*`.
  - API Inbox: `src/public_api.rs` (`519` LOC) + `src/inboxes.rs` (`2,010` LOC) + `src/inbox_actions.rs` (`864` LOC).
  - Email (IMAP + SMTP + OAuth): `src/email_inbound.rs` (`546` LOC), `src/listeners/email_outbound.rs` (`191` LOC), `src/mailer.rs` (`93` LOC), `src/channels/social_oauth.rs` (`/google/*`, `/microsoft/*`).
  - WhatsApp Cloud & Calling: `src/channels/whatsapp.rs` (`1,192` LOC) + embedded signup / manual setup / access requests / outbound listener + `src/voice.rs` WhatsApp calls.
  - Twilio SMS & WhatsApp: `src/channels/twilio.rs` (`796` LOC) + `src/listeners/twilio_outbound.rs` (`152` LOC).
  - Bandwidth SMS: `src/channels/sms.rs` (`482` LOC) + `src/listeners/sms_outbound.rs` (`94` LOC).
  - Facebook Messenger: `src/channels/facebook.rs` (`569` LOC) (`/bot` webhook + outbound listener).
  - Instagram: `src/channels/instagram.rs` (`463` LOC) (`/webhooks/instagram` + OAuth + outbound listener).
  - Telegram: `src/channels/telegram.rs` (`535` LOC) + `src/listeners/telegram_outbound.rs` (`130` LOC).
  - LINE: `src/channels/line.rs` (`368` LOC) + `src/listeners/line_outbound.rs` (`99` LOC).
  - TikTok & Twitter/X: `src/channels/social_oauth.rs` (`714` LOC) (`/webhooks/tiktok`, `/webhooks/twitter`, OAuth & outbound listeners).
  - Voice / Twilio Voice: `src/voice.rs` (`1,362` LOC) (`/calls`, `/conference`, TwiML, announcements, routing, WhatsApp call hours/permissions).
- **React Router (`web/app/`)**:
  - `routes/settings/inboxes.tsx`, `routes/settings/inbox-new.tsx` (13 channel setup flows: website, facebook, whatsapp, sms, email, api, telegram, line, instagram, tiktok, twitter, voice, whatsapp_call), `routes/calls.tsx`.

### 2.5 Automation, Macros, SLA, Policies, CSAT, Campaigns, Webhooks, Notifications & Scheduler

- **Rust (`src/`)**:
  - `src/automation.rs` (`568` LOC) + `src/listeners/automation.rs` (`1,466` LOC): CRUD & async condition/action evaluator.
  - `src/sla.rs` (`1,037` LOC), `src/assignment_policies.rs` (`237` LOC), `src/capacity_policies.rs` (`619` LOC).
  - `src/bots.rs` (`710` LOC): campaigns (incl. analytics metrics/contacts), agent bots, dashboard apps.
  - `src/listeners/csat.rs` (`162` LOC), `src/listeners/webhooks.rs` (`244` LOC), `src/listeners/notifications.rs` (`547` LOC, in-app notifications + `conversation_participants` + agent email notifications), `src/listeners/dialogflow.rs` (`111` LOC).
  - `src/scheduler.rs` (`315` LOC): snooze reopen, auto-resolve, SLA evaluation, scheduled campaigns.
  - `src/settings.rs` (`1,791` LOC): canned responses, teams, labels, custom roles, audit logs, working hours, notification settings, integrations apps.
- **React Router (`web/app/`)**:
  - `routes/settings/{automation,macros,macro-editor,sla,capacity-policy,webhooks,agent-bots,canned,labels,teams,team-editor,agents,custom-roles,auditlogs,templates}.tsx`, `routes/r3/assignment-policy.tsx`, `routes/campaigns.tsx`, `routes/campaigns.whatsapp.tsx`, `routes/r2/campaigns-one-off.tsx`, `routes/survey.responses.tsx`.

### 2.6 Help Center Portals

- **Rust (`src/`)**: `src/help_center.rs` (`1,747` LOC) — portals, categories, locales, articles, `.md`, `/hc/{portal}/sitemap.xml`, public `/hc/{portal}/{locale}/*`, public API `/public/api/v1/portals/*`, bulk translate & article actions.
- **React Router (`web/app/`)**: `routes/help-center/index.tsx`, `routes/help-center/article-editor.tsx`.

### 2.7 Captain AI & Conversation Monitors

- **Rust (`src/`)**: `src/captain.rs` (`4,024` LOC) — assistants, inboxes, scenarios, documents (`pgvector`), responses, FAQ suggestions, bulk actions, message reports, agent sessions, copilot threads/messages/runs, custom tools, MCP servers, tools manifests, writing tasks, Firecrawl webhook; `src/monitors.rs` (`590` LOC).
- **React Router (`web/app/`)**: `routes/captain/{assistants,assistant-detail,documents,responses,tools,preferences,nav}.tsx`, `routes/reports.monitors.tsx`, `routes/reports.monitor-detail.tsx`.

### 2.8 Third-Party Integrations

- **Rust (`src/`)**: `src/integrations/{mod,linear,notion,stripe}.rs`, `src/slack.rs` (`480` LOC, incl. `/slack_uploads`), `src/shopify.rs` (`1,110` LOC), `src/dyte.rs` (`631` LOC), `src/conversations.rs` (Google Translate).
- **React Router (`web/app/`)**: `routes/settings/integrations/{slack,linear,stripe,notion,shopify,google-translate,hook-detail}.tsx`, `routes/settings/r1/integrations.tsx`, `routes/settings/r1/dashboard-apps.tsx`, `routes/shopify.select-account.tsx`.

### 2.9 Reports v2 & Live Reports

- **Rust (`src/`)**: `src/reports.rs` (`1,608` LOC) — `/api/v2/accounts/{id}/reports/*`, summary reports, live reports, `year_in_review`.
- **React Router (`web/app/`)**: `routes/reports.tsx`, `routes/reports.sla.tsx`, `components/next/reports/Live.tsx` (+ siblings), `components/next/charts/BarChart.tsx`.

### 2.10 Public API, Platform API, Widget, Data Imports/Exports, Super Admin

- **Rust (`src/`)**: `src/public_api.rs`, `src/platform.rs` (`647` LOC, incl. `email_channel_migrations`), `src/data_imports.rs` (`774` LOC), `src/data_exports.rs` (`322` LOC + `migrations/0006_data_exports.sql`), `src/super_admin.rs` (`1,855` LOC), `src/web.rs` (`126` LOC), `src/ar_encryption.rs` (`284` LOC), `src/main.rs` (`679` LOC, incl. `.well-known` Android/Apple links & enterprise limits).
- **React Router (`web/app/`)**: `routes/widget.tsx` (`641` LOC), `routes/r3/data-imports.tsx`, `routes/r3/data-import-detail.tsx`, `routes/super-admin/{sign-in,layout,dashboard,accounts,users,configs,resources,instance-status}.tsx`, `routes/settings/billing.tsx`.

---

## 3. TODO — Parity Status

### P0 — Core Runtime Listeners & Horizontal Scaling

- [x] **Port `reporting_event_listener` (`INSERT INTO reporting_events`)**: Implemented in `src/listeners/reporting_events.rs` recording `first_response`, `reply_time`, `conversation_resolved`, `conversation_bot_resolved`, and `conversation_bot_handoff` on live `message.created` and `conversation.status_changed` events.
- [x] **Port `agent_bot_listener` webhook dispatcher**: Implemented in `src/listeners/agent_bots.rs` reusing SSRF-safe signed webhook delivery (`src/listeners/webhooks.rs`) for active bots in `agent_bot_inboxes`.
- [x] **Port `captain_listener` auto-reply dispatcher**: Implemented in `src/listeners/captain.rs` answering incoming unassigned messages via linked Captain assistants & Knowledge Base FAQs, with `[HANDOFF]` escalation to `open`.
- [x] **Multi-node WebSocket, presence & rate-limit backend**: Implemented PostgreSQL `LISTEN/NOTIFY` ActionCable fan-out (`pulsar_cable_events` in `src/cable.rs`) and atomic `pulsar_rate_limits` window counter (`PULSAR_MULTI_NODE=1` in `src/auth.rs`, `migrations/0007_parity_extras.sql`).

### P1 — Channel, Delivery & Auth Parity

- [x] **Inbound Email HTTP webhooks (`app/mailboxes/*`)**: Implemented ActionMailbox webhook routes for Mailgun, SendGrid, Postmark, Mandrill, generic relay, and Amazon SES/SNS in `src/email_inbound.rs`.
- [x] **Web Push + FCM push delivery**: Populated `push_message_title` / `push_message_body` and wired FCM + Web Push relay delivery with 404/410 stale subscription pruning in `src/listeners/notifications.rs`.
- [x] **Agent Google / OmniAuth login (`devise_overrides/omniauth_callbacks`)**: Implemented `/omniauth/google_oauth2` and `/omniauth/google_oauth2/callback` (`src/auth.rs`), `ALLOWED_LOGIN_METHODS`/`google_oauth_enabled` (`src/web.rs`), and Google sign-in button (`web/app/routes/login.tsx`).
- [x] **Bandwidth SMS inbound media download**: Wired `webhooks::safe_fetch_bytes` into `src/channels/sms.rs` downloading MMS `media` URLs via `storage::attach_to_message`.
- [x] **24-hour messaging window (`can_reply?`) enforcement**: Implemented `conversations::within_messaging_window` gating automated CSAT survey (`src/listeners/csat.rs`) and auto-resolve messages (`src/scheduler.rs`).
- [x] **Agent invite onboarding email**: Updated `src/inboxes.rs` + `src/mailer.rs` to generate password-setup tokens (`auth::reset_digest`) and send invitation emails on new agent creation and bulk onboarding.
- [x] **Platform API SSO link & populated account deletion**: Implemented single-use `pulsar_sso_tokens` (`GET /platform/api/v1/users/{id}/login` -> `POST /auth/sso_exchange`) and transaction-safe cascading `account_delete` in `src/platform.rs` & `src/auth.rs`.
- [x] **Widget `identifier_hash` HMAC contact identity binding**: Unified `PATCH /api/v1/widget/contact` and `PATCH /api/v1/widget/contact/set_user` in `src/widget.rs` to enforce HMAC-SHA256 verification and persist `hmac_verified` & `contacts.identifier`.

### P2 — Frontend React Router Parity

- [x] **One-off / SMS campaign creation UI**: Implemented SMS creation modal (SMS/Twilio inbox selector, label audience checkboxes, optional schedule time) and shared channel tabs (`Live Chat`, `SMS`, `WhatsApp`) across `web/app/routes/r2/campaigns-one-off.tsx`, `campaigns.tsx`, and `campaigns.whatsapp.tsx`.
- [x] **Remaining Web Widget sub-views (`web/app/routes/widget.tsx`)**: Implemented `?view=unread-messages` drawer, proactive campaign banner preview/trigger, inline Help Center portal article search & Markdown reader, and Dyte video call join action.
- [x] **Year in Review UI**: Implemented `web/app/routes/reports.year-in-review.tsx` consuming `GET /api/v2/accounts/{id}/year_in_review` and wired into `routes.ts`, `reports.tsx`, and `Sidebar.tsx`.
- [x] **Dedicated Data Export detail route**: Implemented `web/app/routes/r3/data-export-detail.tsx` (`/settings/data/exports/:dataExportId`) and linked export rows in `web/app/routes/r3/data-imports.tsx`.
- [x] **Multi-language i18n translations & locale switcher**: Synced 8 runtime locales (`id`, `es`, `fr`, `de`, `pt_BR`, `ja`, `zh_CN`, `ar`) via `tools/sync-locales.mjs`, added on-demand loader, English fallback, Arabic RTL (`dir="rtl"`), `useLocale()`, and profile menu switcher in `SidebarProfileMenu.tsx`.
- [x] **Rich Markdown composer preview**: Implemented zero-dependency XSS-safe `renderSafeMarkdown` (`web/app/lib/markdown.ts`) with Write/Preview toggles in `ReplyBox.tsx` and formatted Markdown rendering in `article-editor.tsx` (kept lightweight without TipTap/ProseMirror npm bloat).

### P3 — Enterprise Cloud, Audio AI, Hardening & Test Suite

- [x] **Audio & Voice Call AI transcription (`audio_transcription_job`, `call_transcription_job`)**: Implemented `POST /conversations/{id}/messages/{message_id}/transcribe` and `POST /calls/{id}/transcribe` in `src/voice.rs` + `storage::read_first_audio_blob` via Whisper `/audio/transcriptions`.
- [x] **Custom domains & Cloudflare SSL checks (`ee/custom_domains`, `cloudflare_verification_job`)**: Implemented `/custom_domains` CRUD + `/verify` in `src/custom_domains.rs` (`pulsar_custom_domains` table, Cloudflare API & DNS-over-HTTPS TXT verification).
- [x] **Bot resolution time-series & channel report summary**: Implemented `bot_resolutions_count`, `bot_handoffs_count`, and `summary_reports_channel` queries in `src/reports.rs`.
- [x] **Encrypt webhook & agent bot secrets (`ar_encryption`)**: Verified `src/bots.rs` and `src/automation.rs` store secrets via `ar_encryption::new_secret()` and reveal via `ar_encryption::reveal_secret()`, and cleaned stale comment.
- [x] **Backend `ponytail:` cleanups & optimizations**: Implemented `pulsar_message_errors` (`external_error` persistence in `src/conversations.rs`), self-assignment skip + recent message snippets in `src/listeners/notifications.rs`, `auto_resolve_label` in `src/scheduler.rs`, IMAP CRAM-MD5 in `src/email_inbound.rs`, and Bandwidth OAuth token cache in `src/channels/sms.rs`.
- [x] **OpenAPI `/swagger` endpoint & automated frontend component tests**: Added `/swagger` & `/swagger/openapi.json` (`src/web.rs`), Bun test suite `web/app/lib/{i18n,plural,theme,markdown}.test.ts`, CI `bun test` gate, and live end-to-end `tools/wave6-parity-todos.sh`.
- [ ] **Chatwoot Cloud SaaS checkout (`enterprise_checkout_stub`)**: `src/main.rs` keeps hosted-billing stub responses (Cloud SaaS Stripe billing only, N/A for self-hosted Pulsar deployments).
