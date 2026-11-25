#!/usr/bin/env bash
# Checks Pulsar against reported Chatwoot issues. Each line: ISSUE STATUS detail.
# Usage: BASE=http://127.0.0.1:3000 tools/chatwoot-issues.sh   (needs seeded admin@example.com / Password123!)
set -uo pipefail
B=${BASE:-http://127.0.0.1:3000}
A="$B/api/v1/accounts/1"
H=$(mktemp)
curl -s -D "$H" -o /dev/null -H 'content-type: application/json' \
  -d '{"email":"admin@example.com","password":"Password123!"}' "$B/auth/sign_in"
hv() { grep -i "^$1:" "$H" | cut -d' ' -f2 | tr -d '\r'; }
AUTH=(-H "access-token: $(hv access-token)" -H "client: $(hv client)" -H "uid: $(hv uid)" -H 'content-type: application/json')
j() { curl -s "${AUTH[@]}" "$@"; }
code() { curl -s -o /dev/null -w '%{http_code}' "${AUTH[@]}" "$@"; }
py() { python3 -c "import sys,json;d=json.load(sys.stdin);$1"; }
report() { printf '%-7s %-5s %s\n' "$1" "$2" "$3"; }

INBOX=$(j "$A/inboxes" | py 'print([i for i in d["payload"] if i["channel_type"]=="Channel::Api"][0]["id"])')
CONTACT=$(j -X POST "$A/contacts" -d "{\"name\":\"Issue check $RANDOM\"}" | py 'print(d["payload"]["contact"]["id"])')
CONV=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT}" | py 'print(d["id"])')

# #16261 re-snooze "until next reply" must clear the old deadline
j -X POST "$A/conversations/$CONV/toggle_status" -d '{"status":"snoozed","snoozed_until":4102444800}' >/dev/null
j -X POST "$A/conversations/$CONV/toggle_status" -d '{"status":"snoozed","snoozed_until":null}' >/dev/null
S=$(j "$A/conversations/$CONV" | py 'print(d["snoozed_until"])')
[ "$S" = "None" ] && report 16261 PASS "snoozed_until cleared" || report 16261 FAIL "snoozed_until kept: $S"

# #16151 labels accepted when creating a conversation
j -X POST "$A/labels" -d '{"title":"created-with-label"}' >/dev/null
L=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT,\"labels\":[\"created-with-label\"]}" | py 'print(",".join(d["labels"]))')
[ "$L" = "created-with-label" ] && report 16151 PASS "labels set on create" || report 16151 FAIL "labels on create: '$L'"

# #13918 / PR #14159 labels not defined in the account are not attached
L=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT,\"labels\":[\"created-with-label\",\"no-such-label-$RANDOM\"]}" | py 'print(",".join(d["labels"]))')
[ "$L" = "created-with-label" ] && report 13918 PASS "undefined label dropped" || report 13918 FAIL "labels: '$L'"

# #15453 / PR #15496 concurrent creates with one phone number make one contact
P="+1555$RANDOM$RANDOM"
for _ in 1 2 3 4 5 6; do j -o /dev/null -X POST "$A/contacts" -d "{\"name\":\"race\",\"phone_number\":\"$P\"}" & done; wait
N=$(j "$A/contacts/search?q=${P#+}" | py 'print(sum(1 for c in d["payload"] if c["phone_number"]=="'"$P"'"))')
[ "$N" = "1" ] && report 15453 PASS "one contact per phone" || report 15453 FAIL "contacts with same phone: $N"

# #15802 missing record is 404, not 500
c=$(code "$A/conversations/999999"); [ "$c" = 404 ] && report 15802 PASS "missing show 404" || report 15802 FAIL "missing show $c"

# #16368 numeric custom attribute 0 survives a round trip (API side)
j -X PATCH "$A/contacts/$CONTACT" -d '{"custom_attributes":{"score":0}}' >/dev/null
V=$(j "$A/contacts/$CONTACT" | py 'print(d["payload"]["custom_attributes"].get("score"))')
[ "$V" = "0" ] && report 16368 PASS "custom attribute 0 kept" || report 16368 FAIL "custom attribute 0 -> $V"

# #15743 idempotency: same echo_id twice must not create two messages
E="idem-$RANDOM"
j -X POST "$A/conversations/$CONV/messages" -d "{\"content\":\"once\",\"echo_id\":\"$E\"}" >/dev/null
j -X POST "$A/conversations/$CONV/messages" -d "{\"content\":\"once\",\"echo_id\":\"$E\"}" >/dev/null
N=$(j "$A/conversations/$CONV/messages" | py 'print(sum(1 for m in d["payload"] if m["content"]=="once"))')
[ "$N" = 1 ] && report 15743 PASS "echo_id dedup" || report 15743 FAIL "echo_id retry created $N messages"

# #15919 macro assign_agent to a non-member must not silently succeed
AG=$(j -X POST "$A/agents" -d "{\"name\":\"Outsider $RANDOM\",\"email\":\"outsider$RANDOM@example.com\",\"role\":\"agent\"}" | py 'print(d["id"])')
MID=$(j -X POST "$A/macros" -d "{\"name\":\"assign outsider\",\"visibility\":\"global\",\"actions\":[{\"action_name\":\"assign_agent\",\"action_params\":[$AG]}]}" | py 'print(d["payload"]["id"])')
R=$(code -X POST "$A/macros/$MID/execute" -d "{\"conversation_ids\":[$CONV]}")
ASG=$(j "$A/conversations/$CONV" | py 'print((d["meta"].get("assignee") or {}).get("id"))')
if [ "$R" -ge 400 ] || [ "$ASG" = "$AG" ]; then report 15919 PASS "non-member assign is reported or applied ($R, assignee=$ASG)"; else report 15919 FAIL "silent no-op ($R, assignee=$ASG)"; fi

# #16354 deleting an agent never deletes the user row
U=$(psql -d "${PGDB:-pulsar}" -Atc "select count(*) from users where id=$AG")
code -X DELETE "$A/agents/$AG" >/dev/null
U2=$(psql -d "${PGDB:-pulsar}" -Atc "select count(*) from users where id=$AG")
[ "$U" = 1 ] && [ "$U2" = 1 ] && report 16354 PASS "user row kept after agent removal" || report 16354 FAIL "user rows $U -> $U2"

# #16149 notifications index survives a notification whose conversation is gone
psql -d "${PGDB:-pulsar}" -qc "insert into notifications (account_id,user_id,notification_type,primary_actor_type,primary_actor_id,created_at,updated_at) values (1,1,1,'Conversation',99999999,now(),now())"
c=$(code "$A/notifications"); [ "$c" = 200 ] && report 16149 PASS "orphan notification -> 200" || report 16149 FAIL "orphan notification -> $c"
psql -d "${PGDB:-pulsar}" -qc "delete from notifications where primary_actor_id=99999999"

# #16049 index for the default conversation sort
I=$(psql -d "${PGDB:-pulsar}" -Atc "select count(*) from pg_indexes where tablename='conversations' and indexdef ilike '%account_id%last_activity_at%'")
[ "$I" -ge 1 ] && report 16049 PASS "composite (account_id, last_activity_at) index" || report 16049 FAIL "no (account_id, last_activity_at) index"

# #10859 in_reply_to_external_id resolves to the message with that source_id
SID="ext-$RANDOM$RANDOM"
ORIG=$(j -X POST "$A/conversations/$CONV/messages" -d "{\"content\":\"original\",\"source_id\":\"$SID\"}" | py 'print(d["id"])')
R=$(j -X POST "$A/conversations/$CONV/messages" -d "{\"content\":\"reply\",\"content_attributes\":{\"in_reply_to_external_id\":\"$SID\"}}" | py 'a=d["content_attributes"];print(a.get("in_reply_to"),a.get("in_reply_to_external_id"))')
[ "$R" = "$ORIG $SID" ] && report 10859 PASS "reply linked by external id" || report 10859 FAIL "got '$R' want '$ORIG $SID'"

# #5171 failed logins for one email are throttled (10 per 15 min), like Chatwoot's rack_attack login/email
E="nobody$RANDOM$RANDOM@example.com"; last=""
for _ in $(seq 1 11); do last=$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' -d "{\"email\":\"$E\",\"password\":\"x\"}" "$B/auth/sign_in"); done
[ "$last" = 429 ] && report 5171 PASS "11th failed login -> 429" || report 5171 FAIL "11th failed login -> $last"
