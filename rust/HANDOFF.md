# Pulsar Parity Handoff

- **Branch**: `PLR-090/feat-chatwoot-full-parity`
- **Plan & Status**: [MAP.md](MAP.md) | **ADRs**: [DECISIONS.md](DECISIONS.md) | **Terms**: [GLOSSARY.md](GLOSSARY.md)
- **Completed**: Step 0 + Wave 1 + Wave 2 + Wave 3 + Wave 4 (Captain AI, Conversation Monitors, Voice/Calls, enterprise helpers + UI).
- **Last Verified**: `cargo test --offline --locked` (48 passed, 1 ignored), `pnpm typecheck && pnpm build`, `tools/wave4-captain-monitors-voice.sh` (PostgreSQL 18 + live Axum + mock HTTP).
- **Next**: Wave 5 — Super Admin, Installation Onboarding, final cleanup & release gate.
