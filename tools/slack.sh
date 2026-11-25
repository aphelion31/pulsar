#!/usr/bin/env bash
set -euo pipefail
# Starts private mock and app instances. Refuses to touch an existing Slack connection.
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DB=${DATABASE_URL:-postgres://localhost/pulsar}
PORT=${PORT:-3181}
MOCK_PORT=${MOCK_PORT:-3182}
BASE="http://127.0.0.1:$PORT/api/v1/accounts/1"
TOKEN=$(psql -qtAX "$DB" -c "SELECT token FROM access_tokens WHERE owner_id=1 AND owner_type='User' LIMIT 1")
[[ -n "$TOKEN" ]] || { echo 'Missing admin token' >&2; exit 1; }
[[ $(psql -qtAX "$DB" -c "SELECT count(*) FROM integrations_hooks WHERE account_id=1 AND app_id='slack'") == 0 ]] || { echo 'Existing Slack hook: refusing to modify it' >&2; exit 1; }
python3 - "$PORT" "$MOCK_PORT" <<'PY'
import socket, sys
for port in map(int, sys.argv[1:]):
    with socket.socket() as sock:
        try:
            sock.bind(('127.0.0.1', port))
        except OSError:
            sys.exit(f'Port {port} unavailable; refusing to use an existing service')
PY
mock_pid= app_pid= hook_id=
cleanup() {
  if [[ -n "$hook_id" ]]; then psql "$DB" -v ON_ERROR_STOP=1 -c "DELETE FROM integrations_hooks WHERE id=$hook_id AND account_id=1 AND app_id='slack'" >/dev/null; fi
  [[ -z "$app_pid" ]] || kill "$app_pid" 2>/dev/null || true
  [[ -z "$mock_pid" ]] || kill "$mock_pid" 2>/dev/null || true
}
trap cleanup EXIT
python3 -u - "$MOCK_PORT" <<'PY' &
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs
import json, sys
class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        data = parse_qs(self.rfile.read(int(self.headers.get('Content-Length', '0'))).decode())
        if self.path == '/oauth.v2.access':
            response = {'ok': data.get('code') == ['test-code'], 'access_token': 'mock-token'}
        elif self.headers.get('Authorization') != 'Bearer mock-token':
            response = {'ok': False, 'error': 'bad_token'}
        elif self.path == '/conversations.list':
            private = data.get('types') == ['private_channel']
            cursor = data.get('cursor', [''])[0]
            items = ([{'id':'P1','name':'private','is_private':True}] if private else
                     [{'id':'C2','name':'second','is_private':False}] if cursor else
                     [{'id':'C1','name':'general','is_private':False}])
            response = {'ok': True, 'channels': items, 'response_metadata': {'next_cursor': 'page2' if not private and not cursor else ''}}
        elif self.path == '/conversations.join':
            response = {'ok': data.get('channel') == ['C1']}
        else:
            response = {'ok': False}
        payload = json.dumps(response).encode()
        self.send_response(200); self.send_header('Content-Type', 'application/json'); self.end_headers(); self.wfile.write(payload)
    def log_message(self, *args): pass
HTTPServer(('127.0.0.1', int(sys.argv[1])), Handler).serve_forever()
PY
mock_pid=$!
BIND="127.0.0.1:$PORT" DATABASE_URL="$DB" SLACK_API_URL="http://127.0.0.1:$MOCK_PORT" SLACK_CLIENT_ID=test-id SLACK_CLIENT_SECRET=test-secret ACTIVE_RECORD_ENCRYPTION_PRIMARY_KEY=testprimary ACTIVE_RECORD_ENCRYPTION_DETERMINISTIC_KEY=${DET_KEY-testdeterministic} ACTIVE_RECORD_ENCRYPTION_KEY_DERIVATION_SALT=testsalt WEB_DIR="${WEB_DIR:-/Users/beyondb/.jcode/scratch/wt-int5/web/build/client}" "${CARGO_TARGET_DIR:-/Users/beyondb/.jcode/scratch/wt-r6-sl-target}/debug/pulsar" >"${JCODE_SCRATCH_DIR:-$ROOT}/slack-server.log" 2>&1 &
app_pid=$!
for _ in {1..100}; do curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && break; kill -0 "$app_pid" 2>/dev/null || { echo 'Server failed to start' >&2; exit 1; }; sleep .1; done
api() { curl -fsS -H "api_access_token: $TOKEN" -H 'Content-Type: application/json' "$@"; }
code() { curl -sS -o /dev/null -w '%{http_code}' -H "api_access_token: $TOKEN" -H 'Content-Type: application/json' "$@"; }
[[ $(curl -sS -o /dev/null -w '%{http_code}' "$BASE/integrations/apps/slack") == 401 ]]
api "$BASE/integrations/apps/slack" | jq -e '.id == "slack" and .enabled == false and .hooks == []' >/dev/null
created=$(api -X POST "$BASE/integrations/slack" -d '{"code":"test-code"}')
hook_id=$(jq -er '.hooks[0].id | select(type=="number")' <<<"$created")
expect_enc=t; [[ -z ${DET_KEY-x} ]] && expect_enc=f  # CE stores plaintext unless all 3 keys are set
[[ $(psql -qtAX "$DB" -c "SELECT access_token <> 'mock-token' FROM integrations_hooks WHERE id=$hook_id") == "$expect_enc" ]]
jq -e '.enabled and (.hooks[0].status == false) and (.hooks[0].settings == {})' <<<"$created" >/dev/null
api "$BASE/integrations/slack/list_all_channels" | jq -e 'length == 3 and .[0].id == "P1" and .[2].id == "C2"' >/dev/null
[[ $(code -X PATCH "$BASE/integrations/slack" -d '{"reference_id":"missing"}') == 422 ]]
api -X PATCH "$BASE/integrations/slack" -d '{"reference_id":"C1"}' | jq -e '.hooks[0] | .status and .reference_id == "C1" and .settings.channel_name == "general"' >/dev/null
api -X PATCH "$BASE/integrations/slack" -d '{"message_mode":"alert"}' | jq -e '.hooks[0].settings.message_mode == "alert"' >/dev/null
[[ $(code -X POST "$BASE/integrations/hooks/$hook_id/process_event" -d '{"event":"test"}') == 422 ]]
[[ $(code -X DELETE "$BASE/integrations/slack") == 200 ]]
hook_id=
api "$BASE/integrations/apps/slack" | jq -e '.enabled == false and .hooks == []' >/dev/null
echo 'Slack mock integration: PASS'
