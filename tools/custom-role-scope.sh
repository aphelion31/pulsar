#!/usr/bin/env bash
# Chatwoot #11885/#13193: an agent with a "participating" custom role sees only conversations assigned to
# them or where they participate, in list, show and search. Needs a running Pulsar on $BASE and psql to $DB.
set -euo pipefail
B=${BASE:-http://127.0.0.1:3000}; DB=${DB:-postgres://localhost/pulsar}; A="$B/api/v1/accounts/1"
q() { psql -qtAX "$DB" -c "$1"; }
login() { local h; h=$(mktemp); curl -s -D "$h" -o /dev/null -H 'content-type: application/json' -d "{\"email\":\"$1\",\"password\":\"$2\"}" "$B/auth/sign_in"
  AUTH=(-H "access-token: $(grep -i '^access-token:' "$h" | cut -d' ' -f2 | tr -d '\r')" -H "client: $(grep -i '^client:' "$h" | cut -d' ' -f2 | tr -d '\r')" -H "uid: $1" -H 'content-type: application/json'); }
py() { python3 -c "import sys,json;d=json.load(sys.stdin);$1"; }

login admin@example.com 'Password123!'
ADMIN=("${AUTH[@]}")
R=$RANDOM$RANDOM; EMAIL="scoped$R@example.com"
AGENT=$(curl -s "${ADMIN[@]}" -X POST "$A/agents" -d "{\"name\":\"Scoped $R\",\"email\":\"$EMAIL\",\"role\":\"agent\"}" | py 'print(d["id"])')
HASH=$(q "SELECT encrypted_password FROM users WHERE email='admin@example.com'")
q "UPDATE users SET encrypted_password='$HASH', confirmed_at=now() WHERE id=$AGENT"
ROLE=$(q "INSERT INTO custom_roles (name, account_id, permissions, created_at, updated_at) VALUES ('participating $R', 1, '{conversation_participating_manage}', now(), now()) RETURNING id")
q "UPDATE account_users SET custom_role_id=$ROLE WHERE user_id=$AGENT AND account_id=1"
INBOX=$(curl -s "${ADMIN[@]}" "$A/inboxes" | py 'print(d["payload"][0]["id"])')
q "INSERT INTO inbox_members (user_id, inbox_id, created_at, updated_at) VALUES ($AGENT, $INBOX, now(), now()) ON CONFLICT DO NOTHING" >/dev/null
CONTACT=$(curl -s "${ADMIN[@]}" -X POST "$A/contacts" -d "{\"name\":\"Scope $R\"}" | py 'print(d["payload"]["contact"]["id"])')
mk() { curl -s "${ADMIN[@]}" -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT,\"assignee_id\":$1,\"message\":{\"content\":\"secret$R $2\"}}" | py 'print(d["id"])'; }
OTHER=$(mk 1 other); MINE=$(mk "$AGENT" mine)

login "$EMAIL" 'Password123!'
fail=0
ids=$(curl -s "${AUTH[@]}" "$A/conversations?status=all&assignee_type=all" | py 'print(" ".join(str(c["id"]) for c in d["data"]["payload"]))')
[[ " $ids " == *" $MINE "* && " $ids " != *" $OTHER "* ]] && echo "PASS list scoped" || { echo "FAIL list: $ids (mine=$MINE other=$OTHER)"; fail=1; }
c=$(curl -s -o /dev/null -w '%{http_code}' "${AUTH[@]}" "$A/conversations/$OTHER"); [ "$c" = 404 ] && echo "PASS show other 404" || { echo "FAIL show other $c"; fail=1; }
c=$(curl -s -o /dev/null -w '%{http_code}' "${AUTH[@]}" "$A/conversations/$MINE"); [ "$c" = 200 ] && echo "PASS show mine 200" || { echo "FAIL show mine $c"; fail=1; }
n=$(curl -s "${AUTH[@]}" "$A/search?q=secret$R" | py 'print(len(d["payload"]["messages"]))'); [ "$n" = 1 ] && echo "PASS search scoped" || { echo "FAIL search found $n"; fail=1; }
exit $fail
