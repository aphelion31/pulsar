# Pull Request Template

## Description

Summary of the change and the issue it fixes, with any context or dependencies.
Fixes # (issue)

## Type of change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality not to work as expected)
- [ ] This change requires a documentation update

## How has this been tested?

Commands run and what they showed (`cargo test`, `tools/smoke.sh`, `node tools/e2e.js`, `pnpm typecheck`).

## Checklist

- [ ] Branch cut from `develop` and named `<type>/<description>`
- [ ] PR title follows Conventional Commits (`feat: ...`, `fix: ...`)
- [ ] I have performed a self-review of my code
- [ ] I have updated the documentation where needed
- [ ] My changes generate no new warnings
- [ ] New and existing tests pass locally
