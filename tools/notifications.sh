#!/usr/bin/env bash
# In-app notification check. Usage: BASE=http://127.0.0.1:3117 DATABASE_URL=postgres://localhost/pulsar tools/notifications.sh
# Creates agent B, assigns a new conversation to B, sends an incoming message, then checks B's notifications.
set -euo pipefail
B=${BASE:-http://127.0.0.1:3000}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
PW=Password123!
fail() { echo "FAIL: $*"; exit 1; }

H=$(mktemp)
login() { # $1 email, $2 password -> sets AUTH array for that user
  curl -sf -D "$H" -o /dev/null -H 'content-type: application/json' \
    -d "{\"email\":\"$1\",\"password\":\"$2\"}" "$B/auth/sign_in" || fail "sign_in $1"
  local hdr; hdr() { grep -i "^$1:" "$H" | cut -d' ' -f2 | tr -d '\r'; }
  AUTH=(-H "access-token: $(hdr access-token)" -H "client: $(hdr client)" -H "uid: $(hdr uid)" -H 'content-type: application/json')
}
j() { curl -sS "${AUTH[@]}" "$@"; }
A="$B/api/v1/accounts/1"

login admin@example.com "$PW"
ADMIN_AUTH=("${AUTH[@]}")
INBOX=$(j "$A/inboxes" | python3 -c 'import sys,json;print(json.load(sys.stdin)["payload"][0]["id"])')

EMAIL="agentb$RANDOM$RANDOM@ex.com"
AGENT_ID=$(j -X POST "$A/agents" -d "{\"agent\":{\"email\":\"$EMAIL\",\"name\":\"Agent B\",\"role\":\"agent\"}}" \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')

# Known password for agent B, so the script can sign in as B. Hash from htpasswd (bcrypt, $2y$ is accepted).
HASH=$(htpasswd -nbBC 10 x "$PW" | cut -d: -f2)
psql "$DB" -v ON_ERROR_STOP=1 -v h="$HASH" -v e="$EMAIL" -q <<'SQL'
UPDATE users SET encrypted_password = :'h' WHERE email = :'e';
SQL

# Append B to the inbox members, keeping the existing ones.
MEMBERS=$(j "$A/inbox_members/$INBOX" | python3 -c '
import sys,json; ids=[a["id"] for a in json.load(sys.stdin)["payload"]]
print(json.dumps(ids))')
NEW=$(python3 -c "import json,sys; ids=json.loads(sys.argv[1]); b=int(sys.argv[2]); print(json.dumps(sorted(set(ids+[b]))))" "$MEMBERS" "$AGENT_ID")
j -X POST "$A/inbox_members" -d "{\"inbox_id\":$INBOX,\"user_ids\":$NEW}" >/dev/null || fail "inbox members"

CONTACT=$(j -X POST "$A/contacts" -d "{\"name\":\"Notif $RANDOM\",\"email\":\"notif$RANDOM$RANDOM@ex.com\"}" \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["payload"]["contact"]["id"])')
CONV=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT,\"message\":{\"content\":\"hello from agent\"}}" \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')

login "$EMAIL" "$PW"
BEFORE=$(j "$A/notifications/unread_count" | python3 -c 'import sys,json;print(json.load(sys.stdin))')

ADMIN_AUTH_BACKUP=("${AUTH[@]}"); AUTH=("${ADMIN_AUTH[@]}")
j -X POST "$A/conversations/$CONV/assignments" -d "{\"assignee_id\":$AGENT_ID}" | grep -q "\"id\":$AGENT_ID" || fail assign
j -X POST "$A/conversations/$CONV/messages" -d '{"content":"incoming hi","message_type":"incoming"}' >/dev/null || fail incoming

# Listeners run async; poll B's notifications for up to 10 seconds.
login "$EMAIL" "$PW"
for _ in $(seq 1 20); do
  OUT=$(j "$A/notifications")
  if echo "$OUT" | python3 -c '
import sys,json
p=json.load(sys.stdin)["data"]["payload"]; kinds={n["notification_type"] for n in p}
sys.exit(0 if {"conversation_assignment","assigned_conversation_new_message"} <= kinds else 1)'; then break; fi
  sleep 0.5
done
echo "$OUT" | python3 -c '
import sys,json
p=json.load(sys.stdin)["data"]["payload"]; kinds={n["notification_type"] for n in p}
assert "conversation_assignment" in kinds, kinds
assert "assigned_conversation_new_message" in kinds, kinds
print("notifications ok", sorted(kinds))' || fail "notification types missing"

AFTER=$(j "$A/notifications/unread_count" | python3 -c 'import sys,json;print(json.load(sys.stdin))')
[ "$AFTER" -gt "$BEFORE" ] || fail "unread_count $BEFORE -> $AFTER"
echo "unread_count $BEFORE -> $AFTER"
echo "NOTIFICATIONS OK conv=$CONV agent=$AGENT_ID"
