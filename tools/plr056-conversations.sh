#!/usr/bin/env bash
set -euo pipefail
B=${BASE:-http://127.0.0.1:3131}
A="$B/api/v1/accounts/1"
TOKEN=$(psql -qtAX postgres://localhost/pulsar -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
H=(-H "api_access_token: $TOKEN" -H 'content-type: application/json')
j() { curl -fsS "${H[@]}" "$@"; }
INBOX=$(psql -qtAX postgres://localhost/pulsar -c 'select inbox_id from inbox_members where user_id=1 limit 1')
CONTACT=''; CONV=''
cleanup() {
  if [[ -n "$CONV" ]]; then j -X DELETE "$A/conversations/$CONV" >/dev/null || true; fi
  if [[ -n "$CONTACT" ]]; then j -X DELETE "$A/contacts/$CONTACT" >/dev/null || true; fi
}
trap cleanup EXIT
CONTACT=$(j -X POST "$A/contacts" -d "{\"name\":\"PLR056 test\",\"email\":\"plr056-$RANDOM-$RANDOM@example.org\"}" | jq -er '.payload.contact.id')
CONV=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT,\"message\":{\"content\":\"plr056uniquefind\"}}" | jq -er '.id')
C="$A/conversations/$CONV"
j -X PATCH "$C" -d '{"priority":"high"}' | jq -e '.priority == "high"' >/dev/null
j "$A/conversations/search?q=plr056uniquefind" | jq -e --argjson id "$CONV" 'any(.payload[]; .id == $id)' >/dev/null
j -X POST "$A/conversations/filter" -d "{\"payload\":[{\"attribute_key\":\"display_id\",\"filter_operator\":\"equal_to\",\"values\":[$CONV]}]}" | jq -e --argjson id "$CONV" 'any(.payload[]; .id == $id)' >/dev/null
j "$C/participants" | jq -e 'type == "array"' >/dev/null
j -X POST "$C/participants" -d '{"user_ids":[1]}' | jq -e 'any(.[]; .id == 1)' >/dev/null
j -X PATCH "$C/participants" -d '{"user_ids":[]}' | jq -e 'length == 0' >/dev/null
j -X DELETE "$C/participants" -d '{"user_ids":[1]}' >/dev/null
j -X POST "$C/custom_attributes" -d '{"custom_attributes":{"a":1,"b":2}}' >/dev/null
j -X POST "$C/destroy_custom_attributes" -d '{"custom_attributes":["a"]}' | jq -e '.custom_attributes == {"b":2}' >/dev/null
j "$C/attachments" | jq -e '.meta.total_count == 0 and .payload == []' >/dev/null
j -X POST "$C/mute" >/dev/null
j -X POST "$C/unmute" >/dev/null
j -X POST "$C/transcript" -d '{"email":"recipient@example.org"}' >/dev/null
j -X DELETE "$C" >/dev/null
CONV=''
echo 'conversation endpoints OK'
