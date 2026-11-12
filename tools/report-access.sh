#!/usr/bin/env bash
# Reports are admin-only (or custom role with report_manage), like Chatwoot's ReportPolicy#view?.
set -euo pipefail
B=${BASE:-http://127.0.0.1:3000}; DB=${DB:-postgres://localhost/pulsar}; A="$B/api/v1/accounts/1"; R="$B/api/v2/accounts/1"
q() { psql -qtAX "$DB" -c "$1"; }
login() { local h; h=$(mktemp); curl -s -D "$h" -o /dev/null -H 'content-type: application/json' -d "{\"email\":\"$1\",\"password\":\"$2\"}" "$B/auth/sign_in"
  AUTH=(-H "access-token: $(grep -i '^access-token:' "$h" | cut -d' ' -f2 | tr -d '\r')" -H "client: $(grep -i '^client:' "$h" | cut -d' ' -f2 | tr -d '\r')" -H "uid: $1"); }
code() { curl -s -o /dev/null -w '%{http_code}' "${AUTH[@]}" "$1"; }
login admin@example.com 'Password123!'; ADMIN=("${AUTH[@]}")
HASH=$(q "SELECT encrypted_password FROM users WHERE email='admin@example.com'")
R1=$RANDOM$RANDOM; E="rep$R1@example.com"
U=$(curl -s "${ADMIN[@]}" -H 'content-type: application/json' -X POST "$A/agents" -d "{\"name\":\"Rep $R1\",\"email\":\"$E\",\"role\":\"agent\"}" | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
q "UPDATE users SET encrypted_password='$HASH', confirmed_at=now() WHERE id=$U" >/dev/null
fail=0; chk() { [ "$2" = "$3" ] && echo "PASS $1 ($2)" || { echo "FAIL $1 got $2 want $3"; fail=1; }; }
P="/reports/summary?type=account&since=0&until=4102444800"
chk "admin summary" "$(code "$R$P")" 200
login "$E" 'Password123!'
chk "agent summary" "$(code "$R$P")" 401
chk "agent live" "$(code "$R/live_reports/conversation_metrics")" 401
chk "agent year_in_review" "$(code "$R/year_in_review")" 200
ROLE=$(q "INSERT INTO custom_roles (name, account_id, permissions, created_at, updated_at) VALUES ('rep $R1', 1, '{report_manage}', now(), now()) RETURNING id")
q "UPDATE account_users SET custom_role_id=$ROLE WHERE user_id=$U AND account_id=1" >/dev/null
chk "report_manage role summary" "$(code "$R$P")" 200
exit $fail
