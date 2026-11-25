#!/usr/bin/env bash
# Integration test for Dyte endpoints with a local Python mock of Cloudflare RealtimeKit.
set -euo pipefail

DB=${DATABASE_URL:-postgres://localhost/pulsar}
PORT=${PORT:-3183}
MOCK_PORT=${DYTE_MOCK_PORT:-19183}
BIN=${PULSAR_BIN:-/Users/beyondb/.jcode/scratch/wt-r6-ds-target/debug/pulsar}
WEB_DIR=${WEB_DIR:-/Users/beyondb/.jcode/scratch/wt-int5/web/build/client}
B="http://127.0.0.1:$PORT"

MOCK_PID=""
SRV_PID=""
HOOK_ID=""
MSG_ID=""
CONV_ID=""
CONTACT_ID=""

cleanup() {
  [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null || true
  [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null || true
  if [ -n "$MSG_ID" ]; then
    psql -qtAX "$DB" -c "DELETE FROM messages WHERE id = $MSG_ID;" >/dev/null 2>&1 || true
  fi
  if [ -n "$CONV_ID" ]; then
    psql -qtAX "$DB" -c "DELETE FROM messages WHERE conversation_id = $CONV_ID; DELETE FROM conversations WHERE id = $CONV_ID;" >/dev/null 2>&1 || true
  fi
  if [ -n "$CONTACT_ID" ]; then
    psql -qtAX "$DB" -c "DELETE FROM contact_inboxes WHERE contact_id = $CONTACT_ID; DELETE FROM contacts WHERE id = $CONTACT_ID;" >/dev/null 2>&1 || true
  fi
  if [ -n "$HOOK_ID" ]; then
    psql -qtAX "$DB" -c "DELETE FROM integrations_hooks WHERE id = $HOOK_ID;" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

python3 -u - "$MOCK_PORT" <<'PY' &
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

port = int(sys.argv[1])
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        n = int(self.headers.get("Content-Length", "0"))
        body = json.loads(self.rfile.read(n) or b"{}") if n else {}
        if self.path.endswith("/meetings"):
            self._json(200, {"success": True, "data": {"id": "dyte-mtg-123", "title": body.get("title", "")}})
        elif self.path.endswith("/participants"):
            if body.get("preset_name") == "group-call-host":
                self._json(200, {"success": True, "data": {"id": "part-abc", "token": "tok-agent-1", "custom_participant_id": body.get("custom_participant_id")}})
            else:
                self._json(400, {"success": False, "error": {"message": "bad preset"}})
        elif self.path.endswith("/token"):
            self._json(200, {"success": True, "data": {"id": "part-abc", "token": "tok-refreshed-2"}})
        else:
            self._json(404, {"error": "unknown"})
    def do_GET(self):
        if self.path.endswith("/participants"):
            self._json(200, {"success": True, "data": [{"id": "part-abc", "custom_participant_id": "User:1"}]})
        else:
            self._json(404, {"error": "unknown"})
    def _json(self, code, obj):
        b = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

HTTPServer(("127.0.0.1", port), H).serve_forever()
PY
MOCK_PID=$!

DYTE_API_URL="http://127.0.0.1:$MOCK_PORT" BIND="127.0.0.1:$PORT" WEB_DIR="$WEB_DIR" "$BIN" >/tmp/pulsar-dyte-test.log 2>&1 &
SRV_PID=$!

for _ in $(seq 1 40); do
  curl -sf "$B/health" >/dev/null && break
  sleep 0.25
done
curl -sf "$B/health" >/dev/null

TOKEN=$(psql -qtAX "$DB" -c "SELECT token FROM access_tokens WHERE owner_id=1 AND owner_type='User' LIMIT 1")
[ -n "$TOKEN" ]

psql -qtAX "$DB" -c "DELETE FROM integrations_hooks WHERE account_id=1 AND app_id='dyte';" >/dev/null
HOOK_ID=$(psql -qtAX "$DB" -c "INSERT INTO integrations_hooks (account_id, app_id, hook_type, status, settings, created_at, updated_at) VALUES (1, 'dyte', 0, 1, '{\"account_id\":\"cf-acct\",\"app_id\":\"cf-app\",\"api_token\":\"cf-tok\"}'::jsonb, now(), now()) RETURNING id;")

# Find a WebWidget inbox + create a contact & conversation
INBOX_ROW=$(psql -qtAX "$DB" -c "SELECT i.id, w.website_token FROM inboxes i JOIN channel_web_widgets w ON w.id = i.channel_id AND i.channel_type='Channel::WebWidget' WHERE i.account_id=1 LIMIT 1;")
INBOX_ID=$(echo "$INBOX_ROW" | cut -d'|' -f1)
WEBSITE_TOKEN=$(echo "$INBOX_ROW" | cut -d'|' -f2)

# Create widget session via /api/v1/widget/config
WIDGET_RESP=$(curl -sf -X POST "$B/api/v1/widget/config?website_token=$WEBSITE_TOKEN")
X_AUTH_TOKEN=$(echo "$WIDGET_RESP" | jq -r '.website_channel_config.auth_token // empty')
[ -n "$X_AUTH_TOKEN" ]

# Create widget conversation so contact + conversation exist for widget test
CONV_RESP=$(curl -sf -X POST "$B/api/v1/widget/conversations?website_token=$WEBSITE_TOKEN" \
  -H "x-auth-token: $X_AUTH_TOKEN" -H 'content-type: application/json' \
  -d '{"message":{"content":"hi dyte"}}')
CONV_DISPLAY_ID=$(echo "$CONV_RESP" | jq -r '.id')
CONV_ID=$(psql -qtAX "$DB" -c "SELECT id FROM conversations WHERE account_id=1 AND display_id=$CONV_DISPLAY_ID;")
CONTACT_ID=$(psql -qtAX "$DB" -c "SELECT contact_id FROM conversations WHERE id=$CONV_ID;")

# 1. POST create_a_meeting
CREATE_RESP=$(curl -sf -X POST "$B/api/v1/accounts/1/integrations/dyte/create_a_meeting" \
  -H "api_access_token: $TOKEN" -H 'content-type: application/json' \
  -d "{\"conversation_id\":$CONV_DISPLAY_ID}")
echo "$CREATE_RESP" | jq -e '.content_type == "integrations" and .content_attributes.type == "dyte" and .content_attributes.data.meeting_id == "dyte-mtg-123"' >/dev/null
MSG_ID=$(echo "$CREATE_RESP" | jq -r '.id')
echo "create_a_meeting OK: msg_id=$MSG_ID"

# 2. POST add_participant_to_meeting (first time adds participant, second time refreshes token)
ADD_RESP=$(curl -sf -X POST "$B/api/v1/accounts/1/integrations/dyte/add_participant_to_meeting" \
  -H "api_access_token: $TOKEN" -H 'content-type: application/json' \
  -d "{\"message_id\":$MSG_ID}")
echo "$ADD_RESP" | jq -e '.token == "tok-agent-1" and .id == "part-abc"' >/dev/null
echo "add_participant_to_meeting (create) OK"

REFRESH_RESP=$(curl -sf -X POST "$B/api/v1/accounts/1/integrations/dyte/add_participant_to_meeting" \
  -H "api_access_token: $TOKEN" -H 'content-type: application/json' \
  -d "{\"message_id\":$MSG_ID}")
echo "$REFRESH_RESP" | jq -e '.token == "tok-refreshed-2"' >/dev/null
echo "add_participant_to_meeting (refresh) OK"

# 3. POST /api/v1/widget/integrations/dyte/add_participant_to_meeting
WIDGET_ADD=$(curl -sf -X POST "$B/api/v1/widget/integrations/dyte/add_participant_to_meeting?website_token=$WEBSITE_TOKEN" \
  -H "x-auth-token: $X_AUTH_TOKEN" -H 'content-type: application/json' \
  -d "{\"message_id\":$MSG_ID}")
echo "$WIDGET_ADD" | jq -e '.token == "tok-agent-1"' >/dev/null
echo "widget add_participant_to_meeting OK"

echo "DYTE ALL PASS"
