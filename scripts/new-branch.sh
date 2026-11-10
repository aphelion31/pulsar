#!/usr/bin/env bash
# Cut a work branch from the latest origin/develop.
#   scripts/new-branch.sh feat-reports-v2            -> PLR-<next>/feat-reports-v2
#   scripts/new-branch.sh PLR-020/fix-login-loop     -> as given
set -euo pipefail
arg=${1:?usage: scripts/new-branch.sh <type>-<description> | PLR-NNN/<type>-<description>}
if [[ -n "$(git status --porcelain)" ]]; then
  echo "✖ Working tree is not clean. Commit or stash first." >&2
  exit 1
fi
git fetch --prune origin
if [[ "$arg" == PLR-* ]]; then
  name=$arg
else
  # Merged branches get deleted, so also count PR history (needs gh; falls back to remote branches only).
  prs=$(command -v gh >/dev/null && gh pr list --state all --limit 1000 --json headRefName -q '.[].headRefName' 2>/dev/null || true)
  last=$({ git branch -r; echo "$prs"; } | grep -oE 'PLR-[0-9]+' | sed 's/PLR-//' | sort -n | tail -1 || true)
  next=$(printf '%03d' $(( 10#${last:-0} + 1 )))
  name="PLR-$next/$arg"
fi
"$(dirname "$0")/check-branch.sh" "$name"
git switch -c "$name" origin/develop
echo "✔ On $name (from origin/develop)"
