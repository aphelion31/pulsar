#!/usr/bin/env bash
# Agent bot member routes: reset_access_token / reset_secret rotate values, avatar delete, admin only.
set -euo pipefail
B=${BASE:-http://127.0.0.1:3110}/api/v1/accounts/1
DB=${DATABASE_URL:-postgres://localhost/pulsar}
T=$(psql -qtAX "$DB" -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
AG=$(psql -qtAX "$DB" -c "select t.token from access_tokens t join account_users au on au.user_id=t.owner_id and t.owner_type='User' and au.account_id=1 and au.role=0 limit 1")
H=(-H "api_access_token: $T" -H 'content-type: application/json')
ID=$(curl -sf "${H[@]}" -X POST "$B/agent_bots" -d '{"name":"tokbot","outgoing_url":"https://example.com/h"}' | jq .id)
trap 'curl -s "${H[@]}" -X DELETE "$B/agent_bots/$ID" >/dev/null' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
A=$(curl -sf "${H[@]}" "$B/agent_bots/$ID" | jq -r .access_token)
S=$(curl -sf "${H[@]}" "$B/agent_bots/$ID" | jq -r .secret)
A2=$(curl -sf "${H[@]}" -X POST "$B/agent_bots/$ID/reset_access_token" | jq -r .access_token)
S2=$(curl -sf "${H[@]}" -X POST "$B/agent_bots/$ID/reset_secret" | jq -r .secret)
[[ $A != "$A2" && ${#A2} == 24 ]] || fail "access token not rotated"
[[ $S != "$S2" && ${#S2} == 24 ]] || fail "secret not rotated"
[[ $(curl -s -o /dev/null -w '%{http_code}' "${H[@]}" -X DELETE "$B/agent_bots/$ID/avatar") == 200 ]] || fail "avatar delete"
[[ $(curl -s -o /dev/null -w '%{http_code}' -H "api_access_token: $AG" -X POST "$B/agent_bots/$ID/reset_access_token") =~ ^40[13]$ ]] || fail "agent allowed to reset token"
echo "agent bot tokens: rotate token/secret, avatar delete, admin-only OK"
