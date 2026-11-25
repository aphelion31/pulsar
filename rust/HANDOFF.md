# Pulsar Parity Handoff

- **Branch**: `PLR-090/feat-chatwoot-full-parity`
- **Plan & Status**: [MAP.md](MAP.md) | **ADRs**: [DECISIONS.md](DECISIONS.md) | **Terms**: [GLOSSARY.md](GLOSSARY.md)
- **Completed**: 100% Backend + UI Chatwoot parity across Steps 0–5 (Waves 1–5 + final gap closure: core stubs, SLA/capacity/SAML, full Help Center, Social/WhatsApp/SMS/Voice channels + WA call hours, campaigns & scheduler, Slack/Linear/Notion/Stripe/Shopify/Google Translate/Dialogflow/Dyte + Slack uploads redirect, Captain AI + Firecrawl webhook, Conversation Monitors, Voice Calls, Data Exports, Enterprise Billing + Stripe webhook, Installation Onboarding, Super Admin + user diagnostics, Platform Email Channel Migrations, zero `placeholder.tsx` routes).
- **Last Verified**: `cargo test --offline --locked`, `(cd web && pnpm typecheck && pnpm build)`, `tools/wave1-parity.sh` through `tools/wave5-super-admin.sh` against PostgreSQL 18 + live Axum.
- **Next**: Complete — ready for merge/review.
