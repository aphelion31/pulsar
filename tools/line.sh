#!/usr/bin/env bash
set -euo pipefail

PORT="${PORT:-3144}"
MOCK_PORT="${MOCK_PORT:-3997}"
DB="${DATABASE_URL:-postgres://localhost/pulsar}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
WEB_DIR="${WEB_DIR:-$ROOT/web/build/client}"
BIN="${CARGO_TARGET_DIR}/debug/pulsar"
LINE_CHANNEL_ID="test_line_ch_$$"
LINE_CHANNEL_SECRET="test_line_secret_$$"
LINE_CHANNEL_TOKEN="test_line_token_$$"
LINE_USER_ID="U_line_user_12345"

MOCK_PID=""
SRV_PID=""
INBOX_ID=""
CHANNEL_ID=""

cleanup() {
  local rc=$?
  set +e
  [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && wait "$SRV_PID" 2>/dev/null
  [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null && wait "$MOCK_PID" 2>/dev/null
  if [ -n "$INBOX_ID" ]; then
    psql -qtAX "$DB" -c "
      DELETE FROM messages WHERE inbox_id = $INBOX_ID;
      DELETE FROM conversations WHERE inbox_id = $INBOX_ID;
      DELETE FROM contact_inboxes WHERE inbox_id = $INBOX_ID;
      DELETE FROM contacts WHERE account_id = 1 AND additional_attributes->>'social_line_user_id' = '$LINE_USER_ID';
      DELETE FROM inbox_members WHERE inbox_id = $INBOX_ID;
      DELETE FROM inboxes WHERE id = $INBOX_ID;
    " >/dev/null 2>&1
  fi
  if [ -n "$CHANNEL_ID" ]; then
    psql -qtAX "$DB" -c "DELETE FROM channel_line WHERE id = $CHANNEL_ID;" >/dev/null 2>&1
  fi
  psql -qtAX "$DB" -c "DELETE FROM channel_line WHERE line_channel_id = '$LINE_CHANNEL_ID';" >/dev/null 2>&1
  exit "$rc"
}
trap cleanup EXIT

# 1. Start tiny Python mock of LINE Messaging API on 127.0.0.1:3997
python3 - "$MOCK_PORT" <<'PY' &
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

PORT = int(sys.argv[1])
REQUESTS = []

class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

    def _send(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/__requests":
            return self._send(200, REQUESTS)
        REQUESTS.append({
            "method": "GET",
            "path": self.path,
            "auth": self.headers.get("Authorization"),
            "body": None,
        })
        if self.path.startswith("/v2/bot/profile/"):
            uid = self.path.split("/")[-1]
            return self._send(200, {
                "userId": uid,
                "displayName": "Alice Line",
                "pictureUrl": "https://example.com/alice.png",
            })
        self._send(404, {"message": "not found"})

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length > 0 else b""
        body = json.loads(raw.decode()) if raw else {}
        REQUESTS.append({
            "method": "POST",
            "path": self.path,
            "auth": self.headers.get("Authorization"),
            "body": body,
        })
        if self.path == "/v2/bot/message/push":
            return self._send(200, {})
        self._send(404, {"message": "not found"})

HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
PY
MOCK_PID=$!

for _ in $(seq 1 30); do
  if curl -sf "http://127.0.0.1:${MOCK_PORT}/__requests" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

# 2. Start pulsar server on PORT
BIND="127.0.0.1:${PORT}" \
FRONTEND_URL="http://127.0.0.1:${PORT}" \
LINE_API_URL="http://127.0.0.1:${MOCK_PORT}" \
WEB_DIR="$WEB_DIR" \
"$BIN" >/tmp/pulsar_line_test.log 2>&1 &
SRV_PID=$!

for _ in $(seq 1 50); do
  if curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null

TOKEN=$(psql -qtAX "$DB" -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
[ -n "$TOKEN" ]

# 3. Create LINE inbox via API
INBOX_JSON=$(curl -sf -X POST "http://127.0.0.1:${PORT}/api/v1/accounts/1/inboxes" \
  -H "api_access_token: ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"LINE E2E Inbox\",\"channel\":{\"type\":\"line\",\"line_channel_id\":\"${LINE_CHANNEL_ID}\",\"line_channel_secret\":\"${LINE_CHANNEL_SECRET}\",\"line_channel_token\":\"${LINE_CHANNEL_TOKEN}\"}}")

INBOX_ID=$(echo "$INBOX_JSON" | jq -r '.id')
CHANNEL_ID=$(echo "$INBOX_JSON" | jq -r '.channel_id')
CHANNEL_TYPE=$(echo "$INBOX_JSON" | jq -r '.channel_type')
[ "$CHANNEL_TYPE" = "Channel::Line" ]
[ "$INBOX_ID" != "null" ]

# 4. Bad-signature webhook request must return 401
PAYLOAD='{"destination":"'"${LINE_CHANNEL_ID}"'","events":[{"type":"message","message":{"type":"text","id":"line_msg_777","text":"Hello from LINE!"},"source":{"type":"user","userId":"'"${LINE_USER_ID}"'"}}]}'

BAD_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:${PORT}/webhooks/line/${LINE_CHANNEL_ID}" \
  -H "Content-Type: application/json" \
  -H "X-Line-Signature: invalid_sig_123" \
  -d "$PAYLOAD")
[ "$BAD_CODE" = "401" ]

# 5. Compute valid X-Line-Signature with openssl and POST signed webhook
SIG=$(printf '%s' "$PAYLOAD" | openssl dgst -sha256 -hmac "$LINE_CHANNEL_SECRET" -binary | openssl base64 -A)
GOOD_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:${PORT}/webhooks/line/${LINE_CHANNEL_ID}" \
  -H "Content-Type: application/json" \
  -H "X-Line-Signature: ${SIG}" \
  -d "$PAYLOAD")
[ "$GOOD_CODE" = "200" ]

# Verify mock got profile fetch with Bearer token
MOCK_REQS=$(curl -sf "http://127.0.0.1:${MOCK_PORT}/__requests")
echo "$MOCK_REQS" | jq -e ".[] | select(.method == \"GET\" and .path == \"/v2/bot/profile/${LINE_USER_ID}\" and .auth == \"Bearer ${LINE_CHANNEL_TOKEN}\")" >/dev/null

# 6. Assert conversation + contact + incoming message
CONVS_JSON=$(curl -sf "http://127.0.0.1:${PORT}/api/v1/accounts/1/conversations?inbox_id=${INBOX_ID}" \
  -H "api_access_token: ${TOKEN}")
DISPLAY_ID=$(echo "$CONVS_JSON" | jq -r ".data.payload[] | select(.inbox_id == ${INBOX_ID}) | .id" | head -n1)
[ -n "$DISPLAY_ID" ] && [ "$DISPLAY_ID" != "null" ]

MSGS_JSON=$(curl -sf "http://127.0.0.1:${PORT}/api/v1/accounts/1/conversations/${DISPLAY_ID}/messages" \
  -H "api_access_token: ${TOKEN}")
echo "$MSGS_JSON" | jq -e '.payload[] | select(.content == "Hello from LINE!" and .message_type == 0)' >/dev/null
IN_SRC=$(psql -qtAX "$DB" -c "SELECT source_id FROM messages WHERE inbox_id = $INBOX_ID AND message_type = 0 LIMIT 1")
[ "$IN_SRC" = "line_msg_777" ]
CONTACT_NAME=$(psql -qtAX "$DB" -c "SELECT name FROM contacts WHERE account_id = 1 AND additional_attributes->>'social_line_user_id' = '$LINE_USER_ID' LIMIT 1")
[ "$CONTACT_NAME" = "Alice Line" ]

# 7. Post an agent reply and assert mock received push with right auth/to/text
REPLY_JSON=$(curl -sf -X POST "http://127.0.0.1:${PORT}/api/v1/accounts/1/conversations/${DISPLAY_ID}/messages" \
  -H "api_access_token: ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Hi Alice from Pulsar LINE!","message_type":"outgoing","private":false}')
REPLY_ID=$(echo "$REPLY_JSON" | jq -r '.id')
[ "$REPLY_ID" != "null" ]

FOUND_PUSH=0
for _ in $(seq 1 30); do
  MOCK_REQS=$(curl -sf "http://127.0.0.1:${MOCK_PORT}/__requests")
  if echo "$MOCK_REQS" | jq -e ".[] | select(.method == \"POST\" and .path == \"/v2/bot/message/push\" and .auth == \"Bearer ${LINE_CHANNEL_TOKEN}\" and .body.to == \"${LINE_USER_ID}\" and .body.messages[0].type == \"text\" and .body.messages[0].text == \"Hi Alice from Pulsar LINE!\")" >/dev/null 2>&1; then
    FOUND_PUSH=1
    break
  fi
  sleep 0.1
done
[ "$FOUND_PUSH" = "1" ]

echo "line.sh: ALL CHECKS PASSED (inbox=$INBOX_ID conv=$DISPLAY_ID in_src=$IN_SRC contact=$CONTACT_NAME bad_sig=$BAD_CODE)"
