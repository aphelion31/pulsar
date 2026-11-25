# Chatwoot -> Pulsar Full Parity Map

Reference: `/Users/beyondb/Halim/Projects/CEG/prjtss/chatwoot` (`192a2488f7`)
Target: `/Users/beyondb/Halim/Projects/CEG/prjtss/pulsar` (branch `PLR-090/feat-chatwoot-full-parity`)
ADRs: [DECISIONS.md](DECISIONS.md) | Terms: [GLOSSARY.md](GLOSSARY.md)

## Baseline (Rounds 1-6 + PLR-087 + PLR-088)
- `done`: Core auth, conversations, messages, contacts, companies, inboxes, teams, labels, canned responses, custom attributes/filters, macros, automation rules, webhooks, notifications, reports v2, public API, widget, Telegram, LINE, Email IMAP/SMTP, Twilio SMS/WhatsApp, WhatsApp Cloud (`PLR-087`), Bandwidth SMS (`PLR-088`), Slack/Dyte/Shopify basics, Assignment Policies, Data Imports, MFA, Platform API.

## Wave 1 — Core Stubs & Core UI Placeholders
- `done` | Backend `src/sla.rs` + scheduler SLA eval (replace `/sla_policies` stub)
- `done` | Backend `src/capacity_policies.rs` (agent capacity policies + users + inbox limits)
- `done` | Backend `src/settings.rs` & `src/main.rs` (real `/integrations/apps`, `/working_hours`, `/profile/sessions`, `/profile/trusted_devices`, `/notification_subscriptions`, `/branded_email_layout`, `/reporting_events`)
- `done` | Backend `src/saml.rs` (`saml_settings` + `auth/saml_login`)
- `done` | Backend `src/contacts.rs` (`contact_merge` already present), `src/conversations.rs`, `src/inboxes.rs`, `src/inbox_actions.rs` (`contact_info_request`, `reporting_events`, `rotate_hmac_token`, `message_templates`, `whatsapp_business_management_token`, health/webhook/csat_template)
- `done` | Backend `src/main.rs` public routes (`/survey/responses/{id}`, `/robots.txt`, `/api`, `/.well-known/*`)
- `done` | UI `inbox-view.tsx`, `survey.responses.tsx`, `auth.signup.tsx`, `auth.confirmation.tsx`, `auth.verify-email.tsx`, `login.sso.tsx`, `reports.sla.tsx`, `settings/capacity-policy.tsx`, `settings/templates.tsx`, SAML/sessions/MFA in `security.tsx`
- Gate: `cargo test` 46/46 passed, `pnpm typecheck && pnpm build` passed, `tools/wave1-parity.sh` (incl. `tools/smoke.sh`) passed against PostgreSQL 18.

## Wave 2 — Full Help Center + Remaining Messaging Channels
- `done` | Backend `src/channels/facebook.rs`, `src/channels/instagram.rs` + outbound listeners + `inbox_json` channel attributes
- `done` | Backend `src/channels/social_oauth.rs` (TikTok, Twitter/X, Google & Microsoft email OAuth) + shared HMAC OAuth state in `src/common.rs`
- `done` | Backend `src/channels/whatsapp.rs` & `src/bots.rs` & `src/scheduler.rs` (WA embedded signup/access request/manual setup, campaign analytics & scheduled dispatch)
- `done` | Backend `src/help_center.rs` (sitemap.xml, `.md`, `.png` view tracking, public categories/articles/search, `remove_locale`, bulk `translate`)
- `done` | UI Help Center dashboard (`help-center/{index,article-editor}.tsx`), `campaigns.whatsapp.tsx`, channel setup wizard (`inbox-new.tsx`), Sidebar link
- Gate: `cargo test` 48/48 passed, `pnpm typecheck && pnpm build` passed, `tools/wave2-channels-hc.sh` passed against PostgreSQL 18 + mock social/Meta Graph APIs.

## Wave 3 — All Remaining Integrations
- `done` | Backend `src/integrations/{mod,linear,notion,stripe}.rs` (OAuth/callbacks/webhooks + GraphQL Linear/Stripe customer lookup)
- `done` | Backend `src/shopify.rs` (`shopify_custom_app` CRUD, custom callback/webhook, `complete_install`), `src/slack.rs` (`/api/v1/integrations/webhooks`)
- `done` | Backend `src/conversations.rs` (Google Translate + `auto_translate`) + `src/listeners/dialogflow.rs`
- `done` | UI `shopify.select-account.tsx`, `settings/integrations/{slack,linear,stripe,notion,shopify,google-translate,hook-detail}.tsx`, `ConversationSidebar.tsx` cards
- Gate: `cargo test` 48/48 passed, `pnpm typecheck && pnpm build` passed, `tools/wave3-integrations.sh` passed against PostgreSQL 18 + mock integration APIs.

## Wave 4 — Captain AI, Conversation Monitors, Voice/Calls
- `done` | Backend `src/captain.rs` (preferences, assistants, inboxes, scenarios, documents, responses, FAQ suggestions, bulk actions, message reports, agent sessions, copilot, custom tools, MCP servers, tools manifest, writing tasks)
- `done` | Backend `src/monitors.rs`, `src/voice.rs` (calls, WhatsApp calling, Twilio Voice TwiML/conference/announcements/routing), and enterprise helpers (`companies/enrichment`, `conversations/campaign_history`, `csat_survey_responses` review notes, `enterprise/api/v1/accounts/{id}/limits`, `"voice"` inbox creation)
- `done` | UI `captain/*`, `reports.monitors.tsx`, `reports.monitor-detail.tsx`, `calls.tsx`, `inbox.tsx` AI composer actions, `inbox-new.tsx` (`voice` + `whatsapp_call`), Sidebar links
- Gate: `cargo test --offline --locked` 48/48 passed, `pnpm typecheck && pnpm build` passed, `tools/wave4-captain-monitors-voice.sh` passed against PostgreSQL 18 + mock OpenAI/Twilio/MCP APIs.

## Wave 5 — Super Admin, Onboarding & Final Release Gate
- `open` | Backend `src/super_admin.rs`, `src/platform.rs` (`email_channel_migrations`)
- `open` | UI `installation.onboarding.tsx`, `super-admin/*`, delete `placeholder.tsx` (zero `ph()` routes)
- `open` | `rust-anything` pure-Ruby unit verification + full HTTP/Playwright acceptance + `cutover.sh`
