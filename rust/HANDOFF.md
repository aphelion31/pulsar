# Pulsar Parity Handoff

- **Branch**: `PLR-090/feat-chatwoot-full-parity`
- **Plan & Status**: [MAP.md](MAP.md) | **ADRs**: [DECISIONS.md](DECISIONS.md) | **Terms**: [GLOSSARY.md](GLOSSARY.md)
- **Completed**: Step 0 + Wave 1 + Wave 2 (Facebook/Instagram/TikTok/Twitter/Google/MS channels & OAuth, WhatsApp manual/embedded signup/access request, scheduled campaigns + analytics, Help Center dashboard & public sitemap/.md/.png/search/translate/remove_locale).
- **Last Verified**: `cargo test` (48 passed, 1 ignored), `pnpm typecheck && pnpm build`, `tools/wave2-channels-hc.sh` (PostgreSQL 18 + live Axum + mock HTTP).
- **Next**: Wave 3 — Remaining Integrations (Linear, Notion, Stripe, Shopify custom app, Slack webhook, Google Translate, Dialogflow + UI).
