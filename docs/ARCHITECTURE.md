# Architecture

Short index. The details live in numbered documents under [`architectures/`](architectures/):

| Doc                                                              | Covers                                                        |
| ---------------------------------------------------------------- | ------------------------------------------------------------- |
| [01.OVERVIEW.md](architectures/01.OVERVIEW.md)                   | What Pulsar replaces, request paths, realtime, auth           |
| [02.STRUCTURE-PROJECT.md](architectures/02.STRUCTURE-PROJECT.md) | Folder tree, Rust modules, frontend routes, env, scripts      |
| [03.CHATWOOT-COMPAT.md](architectures/03.CHATWOOT-COMPAT.md)     | Schema, auth tokens, API shapes, what is and is not ported    |
| [04.DEPLOY.md](architectures/04.DEPLOY.md)                       | Building the release binary and frontend, running in production |

Process docs (commit, branch, PR, release) live in [`github/`](github/):

| Doc                                           | Covers                                                        |
| --------------------------------------------- | ------------------------------------------------------------- |
| [01.GIT-COMMIT.md](github/01.GIT-COMMIT.md)   | Conventional Commit format and hook checks                    |
| [02.GIT-BRANCH.md](github/02.GIT-BRANCH.md)   | `develop`, `master`, `PLR-NNN/<type>-*` branches, release and hotfix |
| [03.GIT-PR-FLOW.md](github/03.GIT-PR-FLOW.md) | PR titles, review, merge, versioning, release                 |

## Rules that must not break

- **Tenant isolation in every handler.** Account-scoped handlers call `require_account` first and filter every
  query by `account_id`.
- **Values are bound, never formatted into SQL.** Only constants and whitelisted identifiers are interpolated.
- **Agents see only their inboxes.** Role 0 users are limited to conversations in inboxes they belong to.
- **Chatwoot API shapes are kept.** Existing Chatwoot API clients must keep working against Pulsar.

## Request paths

```
dashboard:  browser (React SPA at /app) → fetch /api/v1/accounts/{id}/... with devise token headers
            → axum handler → require_account → sqlx → Postgres
realtime:   browser → WebSocket /cable (ActionCable protocol) → Hub (in-memory streams)
            handler mutation → st.hub.to_inbox / to_account → subscribed browsers
widget:     customer site → /sdk.js → iframe /widget → /app/widget (React)
            → /api/v1/widget/* with X-Auth-Token (HS256 visitor token) → Postgres
```
