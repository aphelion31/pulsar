#!/usr/bin/env bash
set -euo pipefail

PORT="${PORT:-3134}"
MOCK_PORT="${MOCK_PORT:-3998}"
DB="${DATABASE_URL:-postgres://localhost/pulsar}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
WEB_DIR="${WEB_DIR:-$ROOT/web/build/client}"
BIN="${CARGO_TARGET_DIR}/debug/pulsar"
BOT_TOKEN="test_tg_token_$$"

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
      DELETE FROM attachments WHERE message_id IN (SELECT id FROM messages WHERE inbox_id = $INBOX_ID);
      DELETE FROM messages WHERE inbox_id = $INBOX_ID;
      DELETE FROM conversations WHERE inbox_id = $INBOX_ID;
      DELETE FROM contact_inboxes WHERE inbox_id = $INBOX_ID;
      DELETE FROM contacts WHERE account_id = 1 AND additional_attributes->>'social_telegram_user_id' = '987654321';
      DELETE FROM inbox_members WHERE inbox_id = $INBOX_ID;
      DELETE FROM inboxes WHERE id = $INBOX_ID;
    " >/dev/null 2>&1
  fi
  if [ -n "$CHANNEL_ID" ]; then
    psql -qtAX "$DB" -c "DELETE FROM channel_telegram WHERE id = $CHANNEL_ID;" >/dev/null 2>&1
  fi
  psql -qtAX "$DB" -c "DELETE FROM channel_telegram WHERE bot_token = '$BOT_TOKEN';" >/dev/null 2>&1
  exit "$rc"
}
trap cleanup EXIT

# 1. Start tiny Python mock of Telegram Bot API on 127.0.0.1:3998
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
        REQUESTS.append({"method": "GET", "path": self.path, "body": None})
        if self.path.endswith("/getMe"):
            return self._send(200, {"ok": True, "result": {"id": 12345, "is_bot": True, "username": "pulsar_test_bot"}})
        if "/getFile" in self.path:
            return self._send(200, {"ok": True, "result": {"file_id": "PHOTO1", "file_path": "photos/file_1.jpg"}})
        if "/file/bot" in self.path and self.path.endswith("photos/file_1.jpg"):
            body = b"\xff\xd8\xff\xe0fakejpeg"
            self.send_response(200); self.send_header("Content-Type", "image/jpeg"); self.send_header("Content-Length", str(len(body))); self.end_headers()
            return self.wfile.write(body)
        self._send(404, {"ok": False})

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length > 0 else b""
        body = json.loads(raw.decode()) if raw else {}
        REQUESTS.append({"method": "POST", "path": self.path, "body": body})
        if self.path.endswith("/deleteWebhook"):
            return self._send(200, {"ok": True, "result": True})
        if self.path.endswith("/setWebhook"):
            return self._send(200, {"ok": True, "result": True})
        if self.path.endswith("/sendMessage"):
            return self._send(200, {"ok": True, "result": {"message_id": 888, "chat": {"id": body.get("chat_id")}, "text": body.get("text")}})
        self._send(404, {"ok": False})

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
TELEGRAM_API_URL="http://127.0.0.1:${MOCK_PORT}" \
WEB_DIR="$WEB_DIR" \
"$BIN" >/tmp/pulsar_tg_test.log 2>&1 &
SRV_PID=$!

for _ in $(seq 1 50); do
  if curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null

TOKEN=$(psql -qtAX "$DB" -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
[ -n "$TOKEN" ]

# 3. Create Telegram inbox via API
INBOX_JSON=$(curl -sf -X POST "http://127.0.0.1:${PORT}/api/v1/accounts/1/inboxes" \
  -H "api_access_token: ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"TG E2E Inbox\",\"channel\":{\"type\":\"telegram\",\"bot_token\":\"${BOT_TOKEN}\"}}")

INBOX_ID=$(echo "$INBOX_JSON" | jq -r '.id')
CHANNEL_ID=$(echo "$INBOX_JSON" | jq -r '.channel_id')
CHANNEL_TYPE=$(echo "$INBOX_JSON" | jq -r '.channel_type')
[ "$CHANNEL_TYPE" = "Channel::Telegram" ]
[ "$INBOX_ID" != "null" ]

# Verify mock got getMe and setWebhook
MOCK_REQS=$(curl -sf "http://127.0.0.1:${MOCK_PORT}/__requests")
echo "$MOCK_REQS" | jq -e ".[] | select(.path == \"/bot${BOT_TOKEN}/getMe\")" >/dev/null
echo "$MOCK_REQS" | jq -e ".[] | select(.path == \"/bot${BOT_TOKEN}/setWebhook\" and .body.url == \"http://127.0.0.1:${PORT}/webhooks/telegram/${BOT_TOKEN}\")" >/dev/null

# 4. POST a fake Telegram update to the webhook
curl -sf -X POST "http://127.0.0.1:${PORT}/webhooks/telegram/${BOT_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "update_id": 10001,
    "message": {
      "message_id": 777,
      "from": { "id": 987654321, "is_bot": false, "first_name": "Alice", "last_name": "Tg", "username": "alicetg", "language_code": "en" },
      "chat": { "id": 987654321, "first_name": "Alice", "last_name": "Tg", "username": "alicetg", "type": "private" },
      "date": 1700000000,
      "text": "Hello from Telegram!"
    }
  }' >/dev/null

# 5. Assert conversation + incoming message
CONVS_JSON=$(curl -sf "http://127.0.0.1:${PORT}/api/v1/accounts/1/conversations?inbox_id=${INBOX_ID}" \
  -H "api_access_token: ${TOKEN}")
DISPLAY_ID=$(echo "$CONVS_JSON" | jq -r ".data.payload[] | select(.inbox_id == ${INBOX_ID}) | .id" | head -n1)
[ -n "$DISPLAY_ID" ] && [ "$DISPLAY_ID" != "null" ]

MSGS_JSON=$(curl -sf "http://127.0.0.1:${PORT}/api/v1/accounts/1/conversations/${DISPLAY_ID}/messages" \
  -H "api_access_token: ${TOKEN}")
echo "$MSGS_JSON" | jq -e '.payload[] | select(.content == "Hello from Telegram!" and .message_type == 0)' >/dev/null
IN_SRC=$(psql -qtAX "$DB" -c "SELECT source_id FROM messages WHERE inbox_id = $INBOX_ID AND message_type = 0 LIMIT 1")
[ "$IN_SRC" = "777" ]

# 6. Post an agent reply and assert mock received sendMessage with right chat_id/text
REPLY_JSON=$(curl -sf -X POST "http://127.0.0.1:${PORT}/api/v1/accounts/1/conversations/${DISPLAY_ID}/messages" \
  -H "api_access_token: ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Hi Alice from Pulsar!","message_type":"outgoing","private":false}')
REPLY_ID=$(echo "$REPLY_JSON" | jq -r '.id')
[ "$REPLY_ID" != "null" ]

FOUND_SEND=0
for _ in $(seq 1 30); do
  MOCK_REQS=$(curl -sf "http://127.0.0.1:${MOCK_PORT}/__requests")
  if echo "$MOCK_REQS" | jq -e ".[] | select(.path == \"/bot${BOT_TOKEN}/sendMessage\" and (.body.chat_id | tostring) == \"987654321\" and .body.text == \"Hi Alice from Pulsar!\")" >/dev/null 2>&1; then
    FOUND_SEND=1
    break
  fi
  sleep 0.1
done
[ "$FOUND_SEND" = "1" ]

OUT_SRC=""
for _ in $(seq 1 20); do
  OUT_SRC=$(psql -qtAX "$DB" -c "SELECT COALESCE(source_id, '') FROM messages WHERE id = $REPLY_ID")
  [ "$OUT_SRC" = "888" ] && break
  sleep 0.1
done
[ "$OUT_SRC" = "888" ]

# 7. Inbound photo (no text) is stored as an image attachment downloaded via getFile
curl -sf -X POST "http://127.0.0.1:${PORT}/webhooks/telegram/${BOT_TOKEN}" -H "Content-Type: application/json" -d '{
  "update_id": 10002,
  "message": { "message_id": 779, "from": { "id": 987654321, "first_name": "Alice" }, "chat": { "id": 987654321, "type": "private" },
    "date": 1700000001, "photo": [ { "file_id": "SMALL", "width": 90 }, { "file_id": "PHOTO1", "width": 800 } ] } }' >/dev/null
ATT=""
for _ in $(seq 1 30); do
  ATT=$(psql -qtAX "$DB" -c "SELECT a.file_type || ':' || COALESCE(a.extension,'') FROM attachments a JOIN messages m ON m.id = a.message_id WHERE m.inbox_id = $INBOX_ID AND m.source_id = '779'")
  [ -n "$ATT" ] && break
  sleep 0.2
done
[ "$ATT" = "0:jpg" ] || { echo "photo attachment missing: '$ATT'"; exit 1; }

echo "telegram.sh: ALL CHECKS PASSED (inbox=$INBOX_ID conv=$DISPLAY_ID in_src=$IN_SRC out_src=$OUT_SRC)"
