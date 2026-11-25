# Domain Glossary (Chatwoot -> Pulsar)

| Chatwoot Term | Meaning | Pulsar Rust Module / Type |
| --- | --- | --- |
| Account | Tenant workspace (`accounts` table) | `common::AccountUser`, `require_account` |
| AccountUser | User membership & role (`0=agent`, `1=admin`) + custom role | `common::AccountUser` |
| Inbox | Communication channel endpoint (`inboxes` + `channel_*`) | `inboxes.rs`, `channels/*` |
| Conversation | Thread identified publicly by `display_id` per account | `conversations.rs` |
| Contact / ContactInbox | End-user record + per-inbox `source_id` binding | `contacts.rs`, `widget.rs` |
| ActionCable (`RoomChannel`) | Realtime WebSocket protocol over `/cable` | `cable::Hub`, `cable.rs` |
| AsyncDispatcher / Sidekiq | Durable PostgreSQL-backed async event bus + cron loop | `events.rs` (`pulsar_events`), `scheduler.rs` |
| Help Center (`Portal`) | Knowledge base (`portals`, `categories`, `articles`) | `help_center.rs` |
| CSAT Survey | Post-resolution rating (`csat_survey_responses`) | `account_more.rs`, `public_api.rs`, `listeners/csat.rs` |
| SLA (`SlaPolicy` / `AppliedSla`) | Response/resolution thresholds & breach tracking | `sla.rs`, `applied_slas`, `sla_events` |
| Capacity Policy | Agent max open conversation limits per inbox + exclusions | `capacity_policies.rs` |
| Integrations Hook | Account/inbox app credential & settings (`integrations_hooks`) | `slack.rs`, `dyte.rs`, `shopify.rs`, `integrations/*` |
| Captain AI | Assistants, FAQs, documents, scenarios, copilot, AI tasks | `captain.rs` |
| Conversation Monitor | Automated QA/scan monitors over conversations | `monitors.rs` |
| Voice / Calls | Twilio / WhatsApp voice call & conference management | `voice.rs` |
| SuperAdmin | Instance-wide operator (`users.type = 'SuperAdmin'`) | `super_admin.rs` |
