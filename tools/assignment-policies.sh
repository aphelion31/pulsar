#!/usr/bin/env bash
set -euo pipefail
B=${BASE:-http://127.0.0.1:3151}/api/v1/accounts/1
DB=${DATABASE_URL:-postgres://localhost/pulsar}
T=$(psql -qtAX "$DB" -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
H=(-H "api_access_token: $T" -H 'content-type: application/json')
name="policy-test-$$-$(date +%s)"
id='' inbox='' agent_token="ap-test-$$-$(date +%s)"
cleanup() {
  [[ -z $id ]] || curl -s "${H[@]}" -X DELETE "$B/assignment_policies/$id" >/dev/null
  [[ -z $inbox ]] || curl -s "${H[@]}" -X DELETE "$B/inboxes/$inbox" >/dev/null
  printf "DELETE FROM access_tokens WHERE token = :'token';\n" | psql -q "$DB" -v token="$agent_token" >/dev/null
}
trap cleanup EXIT
agent=$(psql -qtAX "$DB" -c "select user_id from account_users where account_id=1 and role=0 limit 1")
printf "INSERT INTO access_tokens(owner_type,owner_id,token,created_at,updated_at) VALUES ('User', :agent, :'token', now(), now());\n" | psql -q "$DB" -v agent="$agent" -v token="$agent_token" >/dev/null
inbox=$(curl -sf "${H[@]}" -X POST "$B/inboxes" -d "{\"name\":\"$name\",\"channel\":{\"type\":\"api\"}}" | jq -er .id)
id=$(curl -sf "${H[@]}" -X POST "$B/assignment_policies" -d "{\"assignment_policy\":{\"name\":\"$name\"}}" | jq -er '.id')
[[ $(curl -s -o /dev/null -w '%{http_code}' "${H[@]}" -X POST "$B/assignment_policies" -d "{\"assignment_policy\":{\"name\":\"$name\"}}") == 422 ]]
curl -sf "${H[@]}" "$B/assignment_policies" | jq -e --argjson id "$id" 'any(.[]; .id == $id)' >/dev/null
curl -sf "${H[@]}" "$B/assignment_policies/$id" | jq -e '.assignment_order == "round_robin"' >/dev/null
curl -sf "${H[@]}" -X PATCH "$B/assignment_policies/$id" -d '{"assignment_policy":{"description":"updated"}}' | jq -e '.description == "updated"' >/dev/null
curl -sf "${H[@]}" -X POST "$B/inboxes/$inbox/assignment_policy" -d "{\"assignment_policy_id\":$id}" | jq -e --argjson id "$id" '.id == $id' >/dev/null
curl -sf "${H[@]}" "$B/inboxes/$inbox/assignment_policy" | jq -e --argjson id "$id" '.id == $id' >/dev/null
curl -sf "${H[@]}" "$B/assignment_policies/$id/inboxes" | jq -e --argjson id "$inbox" 'any(.inboxes[]; .id == $id)' >/dev/null
curl -sf "${H[@]}" -X DELETE "$B/inboxes/$inbox/assignment_policy" >/dev/null
[[ $(curl -s -o /dev/null -w '%{http_code}' "$B/assignment_policies") == 401 ]]
[[ $(curl -s -o /dev/null -w '%{http_code}' -H "api_access_token: $agent_token" "$B/assignment_policies") == 403 ]]
curl -sf "${H[@]}" -X DELETE "$B/assignment_policies/$id" >/dev/null
id=''
echo 'assignment policies: CRUD, uniqueness, inbox links, authorization OK'
