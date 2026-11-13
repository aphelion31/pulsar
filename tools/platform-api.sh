#!/usr/bin/env bash
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:3171}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
TOKEN="plr079-$(uuidgen)"
APP=$(psql "$DB" -Atqc "INSERT INTO platform_apps(name,created_at,updated_at) VALUES ('PLR-079 test',now(),now()) RETURNING id")
A= U= B=
cleanup() {
  psql "$DB" -q -v ON_ERROR_STOP=1 -c "DELETE FROM account_users WHERE account_id IN (SELECT permissible_id FROM platform_app_permissibles WHERE platform_app_id=$APP AND permissible_type='Account'); DELETE FROM access_tokens WHERE (owner_type,owner_id) IN (SELECT permissible_type,permissible_id FROM platform_app_permissibles WHERE platform_app_id=$APP); DELETE FROM agent_bots WHERE id IN (SELECT permissible_id FROM platform_app_permissibles WHERE platform_app_id=$APP AND permissible_type='AgentBot'); DELETE FROM users WHERE id IN (SELECT permissible_id FROM platform_app_permissibles WHERE platform_app_id=$APP AND permissible_type='User'); DELETE FROM accounts WHERE id IN (SELECT permissible_id FROM platform_app_permissibles WHERE platform_app_id=$APP AND permissible_type='Account'); DELETE FROM access_tokens WHERE owner_type='PlatformApp' AND owner_id=$APP; DELETE FROM platform_app_permissibles WHERE platform_app_id=$APP; DELETE FROM platform_apps WHERE id=$APP"
  if [[ $A =~ ^[0-9]+$ ]]; then
    psql "$DB" -q -c "DROP SEQUENCE IF EXISTS conv_dpid_seq_$A, camp_dpid_seq_$A"
  fi
}
trap cleanup EXIT
psql "$DB" -q -c "INSERT INTO access_tokens(owner_type,owner_id,token,created_at,updated_at) VALUES ('PlatformApp',$APP,'$TOKEN',now(),now())"
request() { curl -fsS -H "api_access_token: $TOKEN" -H 'Content-Type: application/json' "$@"; }
status() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
[[ $(status -H 'api_access_token: invalid' "$BASE/platform/api/v1/accounts") == 401 ]]
code=$(status -H "api_access_token: $TOKEN" "$BASE/platform/api/v1/accounts/1")
[[ $code == 401 || $code == 404 ]]
A=$(request -d '{"name":"PLR079 account","locale":"en"}' "$BASE/platform/api/v1/accounts" | jq -er '.id')
request "$BASE/platform/api/v1/accounts" | jq -e --argjson id "$A" 'any(.[]; .id == $id)' >/dev/null
request "$BASE/platform/api/v1/accounts/$A" | jq -e '.name == "PLR079 account"' >/dev/null
request -X PATCH -d '{"name":"Updated account","limits":{"agents":4}}' "$BASE/platform/api/v1/accounts/$A" | jq -e '.name == "Updated account" and .limits.agents == 4' >/dev/null
U=$(request -d "{\"name\":\"PLR079 user\",\"email\":\"plr079-$APP@example.test\",\"password\":\"Example123!\"}" "$BASE/platform/api/v1/users" | jq -er '.id')
request "$BASE/platform/api/v1/users/$U" | jq -e '.name == "PLR079 user" and (.access_token | length > 0)' >/dev/null
request -X PATCH -d '{"display_name":"Updated user"}' "$BASE/platform/api/v1/users/$U" | jq -e '.display_name == "Updated user"' >/dev/null
request -X POST "$BASE/platform/api/v1/users/$U/token" | jq -e '.expiry == null and (.access_token | length > 0)' >/dev/null
# SSO is deliberately unavailable until one-time token consumption is implemented.
request "$BASE/platform/api/v1/users/$U/login" | jq -e 'has("url") and .url == null' >/dev/null
request -d "{\"user_id\":$U,\"role\":\"administrator\"}" "$BASE/platform/api/v1/accounts/$A/account_users" | jq -e '.role == "administrator"' >/dev/null
request "$BASE/platform/api/v1/accounts/$A/account_users" | jq -e --argjson id "$U" 'any(.[]; .user_id == $id)' >/dev/null
[[ $(status -X POST -H "api_access_token: $TOKEN" -H 'Content-Type: application/json' -d '{"user_id":1}' "$BASE/platform/api/v1/accounts/$A/account_users") == 401 ]]
B=$(request -d "{\"name\":\"PLR079 bot\",\"account_id\":$A,\"outgoing_url\":\"https://example.test/hook\"}" "$BASE/platform/api/v1/agent_bots" | jq -er '.id')
request "$BASE/platform/api/v1/agent_bots" | jq -e --argjson id "$B" 'any(.[]; .id == $id)' >/dev/null
request "$BASE/platform/api/v1/agent_bots/$B" | jq -e '.name == "PLR079 bot" and (.access_token | length > 0)' >/dev/null
request -X PATCH -d '{"description":"Updated bot"}' "$BASE/platform/api/v1/agent_bots/$B" | jq -e '.description == "Updated bot"' >/dev/null
request -X DELETE "$BASE/platform/api/v1/agent_bots/$B/avatar" | jq -e --argjson id "$B" '.id == $id' >/dev/null
request -X DELETE "$BASE/platform/api/v1/agent_bots/$B" >/dev/null
request -X DELETE -d "{\"user_id\":$U}" "$BASE/platform/api/v1/accounts/$A/account_users" >/dev/null
request -X DELETE "$BASE/platform/api/v1/users/$U" >/dev/null
request -X DELETE "$BASE/platform/api/v1/accounts/$A" >/dev/null
echo 'platform API endpoints and authorization passed (SSO URL unavailable)'
