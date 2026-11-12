#!/usr/bin/env bash
# Test automation rules listener:
# Creates a rule: message_created where content contains 'refund' -> add_label refund + assign_team
# Sends a non-matching message (must not trigger)
# Sends a matching message and polls until label appears and team is assigned (timeout 10s)
set -euo pipefail

B=${BASE:-http://127.0.0.1:3108}
DIR=$(mktemp -d "${JCODE_SCRATCH_DIR:-/tmp}/automation-test.XXXXXX")

cleanup() {
  if [ -n "${RULE:-}" ]; then
    curl -fsS "${AUTH[@]}" -X DELETE "$A/automation_rules/$RULE" >/dev/null 2>&1 || true
  fi
  if [ "${SUCCESS:-0}" -ne 1 ] && [ -f "$DIR/server.log" ]; then
    echo "=== SERVER LOG ==="
    cat "$DIR/server.log"
  fi
  [ -z "${SERVER:-}" ] || kill "$SERVER" 2>/dev/null || true
  rm -rf "$DIR"
}
trap cleanup EXIT

if [ -z "${BASE:-}" ]; then
  BIND=127.0.0.1:3108 "${PULSAR_BIN:-${CARGO_TARGET_DIR:-target}/debug/pulsar}" >"$DIR/server.log" 2>&1 &
  SERVER=$!
  for _ in {1..30}; do
    if grep -q 'pulsar listening on' "$DIR/server.log"; then break; fi
    kill -0 "$SERVER" 2>/dev/null || { cat "$DIR/server.log"; exit 1; }
    sleep 1
  done
fi

curl -sf -D "$DIR/headers" -o /dev/null -H 'content-type: application/json' \
  -d '{"email":"admin@example.com","password":"Password123!"}' "$B/auth/sign_in"

hdr() { awk -v key="$1" 'tolower($1)==tolower(key ":") {gsub("\r", "", $2); print $2}' "$DIR/headers"; }
AUTH=(-H "access-token: $(hdr access-token)" -H "client: $(hdr client)" -H "uid: $(hdr uid)" -H 'content-type: application/json')
A="$B/api/v1/accounts/1"
j() { curl -fsS "${AUTH[@]}" "$@"; }

# 1. Setup inbox and contact
INBOX=$(j "$A/inboxes" | python3 -c 'import json,sys; print(json.load(sys.stdin)["payload"][0]["id"])')
CONTACT=$(j -X POST "$A/contacts" -d "{\"name\":\"Automation Test $RANDOM\",\"email\":\"auto$RANDOM@ex.com\"}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["payload"]["contact"]["id"])')

# 2. Setup team and label
TEAM=$(j -X POST "$A/teams" -d "{\"name\":\"Refund Support $RANDOM\"}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
j -X POST "$A/labels" -d '{"title":"refund"}' >/dev/null 2>&1 || true

# 3. Create automation rule
RULE=$(j -X POST "$A/automation_rules" -d "{
  \"name\": \"Refund Auto-Triage $RANDOM\",
  \"event_name\": \"message_created\",
  \"conditions\": [
    {
      \"attribute_key\": \"content\",
      \"filter_operator\": \"contains\",
      \"values\": [\"refund\"],
      \"query_operator\": \"AND\"
    }
  ],
  \"actions\": [
    {
      \"action_name\": \"add_label\",
      \"action_params\": [\"refund\"]
    },
    {
      \"action_name\": \"assign_team\",
      \"action_params\": [$TEAM]
    }
  ],
  \"active\": true
}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')

echo "Created automation rule: $RULE (team $TEAM)"

# 4. Non-matching message test
CONV1=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
j -X POST "$A/conversations/$CONV1/messages" -d '{"content":"general inquiry about shipping","message_type":"incoming"}' >/dev/null

sleep 2

python3 -c "
import json, sys, urllib.request
headers = {'access-token': sys.argv[1], 'client': sys.argv[2], 'uid': sys.argv[3]}
req = urllib.request.Request('$A/conversations/$CONV1', headers=headers)
with urllib.request.urlopen(req) as resp:
    data = json.load(resp)
labels = data.get('labels', [])
meta = data.get('meta', {})
team = meta.get('team')
assert 'refund' not in labels, f'Rule triggered unexpectedly: labels={labels}'
assert team is None, f'Team assigned unexpectedly: {team}'
print('Non-matching message test passed: rule did not trigger')
" "$(hdr access-token)" "$(hdr client)" "$(hdr uid)"

# 5. Matching message test
CONV2=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
j -X POST "$A/conversations/$CONV2/messages" -d '{"content":"hello I need a refund for my order","message_type":"incoming"}' >/dev/null

# Poll conversation until label 'refund' and team appear (timeout 10s)
python3 -c "
import json, sys, time, urllib.request

headers = {'access-token': sys.argv[1], 'client': sys.argv[2], 'uid': sys.argv[3]}
req = urllib.request.Request('$A/conversations/$CONV2', headers=headers)
expected_team = int(sys.argv[4])

success = False
for i in range(20):
    time.sleep(0.5)
    with urllib.request.urlopen(req) as resp:
        data = json.load(resp)
    labels = data.get('labels', [])
    team = (data.get('meta') or {}).get('team')
    team_id = team.get('id') if isinstance(team, dict) else None
    if 'refund' in labels and team_id == expected_team:
        print(f'Matching message test passed after {(i+1)*0.5}s: labels={labels}, team_id={team_id}')
        success = True
        break

if not success:
    print(f'FAIL: Polling timed out (10s). Final state: labels={labels}, team={team}')
    sys.exit(1)
" "$(hdr access-token)" "$(hdr client)" "$(hdr uid)" "$TEAM"

echo "ALL AUTOMATION RULE TESTS PASSED"
SUCCESS=1
