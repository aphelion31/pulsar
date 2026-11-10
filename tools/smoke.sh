#!/usr/bin/env bash
# API smoke test for Pulsar. Usage: BASE=http://127.0.0.1:3098 tools/smoke.sh
set -euo pipefail
B=${BASE:-http://127.0.0.1:3000}
H=$(mktemp)
curl -sf -D "$H" -o /dev/null -H 'content-type: application/json' \
  -d '{"email":"admin@example.com","password":"Password123!"}' "$B/auth/sign_in"
hdr() { grep -i "^$1:" "$H" | cut -d' ' -f2 | tr -d '\r'; }
AUTH=(-H "access-token: $(hdr access-token)" -H "client: $(hdr client)" -H "uid: $(hdr uid)" -H 'content-type: application/json')
A="$B/api/v1/accounts/1"
j() { curl -sS "${AUTH[@]}" "$@"; }
fail() { echo "FAIL: $*"; exit 1; }

INBOX=$(j "$A/inboxes" | python3 -c 'import sys,json;print(json.load(sys.stdin)["payload"][0]["id"])')
CONTACT=$(j -X POST "$A/contacts" -d "{\"name\":\"Smoke $RANDOM\",\"email\":\"smoke$RANDOM$RANDOM@ex.com\"}" | python3 -c 'import sys,json;print(json.load(sys.stdin)["payload"]["contact"]["id"])')
CONV=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT,\"message\":{\"content\":\"hello from agent\"}}" | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
j -X POST "$A/conversations/$CONV/messages" -d '{"content":"incoming hi","message_type":"incoming"}' >/dev/null
j -X POST "$A/conversations/$CONV/messages" -d '{"content":"private note","private":true}' >/dev/null
j -X POST "$A/conversations/$CONV/labels" -d '{"labels":["vip","billing"]}' | grep -q vip || fail labels
j -X POST "$A/conversations/$CONV/assignments" -d '{"assignee_id":1}' | grep -q '"id":1' || fail assign
j -X POST "$A/conversations/$CONV/toggle_priority" -d '{"priority":"high"}' >/dev/null
j -X POST "$A/conversations/$CONV/toggle_status" -d '{"status":"resolved"}' | grep -q resolved || fail status
j "$A/conversations?status=resolved&assignee_type=me&labels[]=vip" | python3 -c '
import sys,json; d=json.load(sys.stdin)["data"]; assert d["meta"]["mine_count"]>=1, d["meta"]; c=d["payload"][0]
assert c["status"]=="resolved" and c["priority"]=="high" and "vip" in c["labels"], c
print("list ok", d["meta"])'
j "$A/conversations/$CONV/messages" | python3 -c '
import sys,json; p=json.load(sys.stdin)["payload"]; kinds=[m["message_type"] for m in p]
assert 0 in kinds and 1 in kinds and 2 in kinds, kinds; print("messages ok", len(p), kinds)'
j -X POST "$A/conversations/$CONV/toggle_status" -d '{"status":"open"}' >/dev/null
MSG=$(j "$A/conversations/$CONV/messages" | python3 -c 'import sys,json;print(json.load(sys.stdin)["payload"][-1]["id"])')
j -X DELETE "$A/conversations/$CONV/messages/$MSG" | grep -q deleted || fail delete
code=$(curl -s -o /dev/null -w '%{http_code}' "$A/conversations")
[ "$code" = 401 ] || fail "unauth $code"
for p in labels canned_responses agents teams notifications/unread_count cache_keys contacts custom_attribute_definitions; do
  code=$(curl -s -o /dev/null -w '%{http_code}' "${AUTH[@]}" "$A/$p"); echo "GET /$p $code"
done
echo "SMOKE OK conv=$CONV"
