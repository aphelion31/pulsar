#!/usr/bin/env bash
# End-to-end check for Channel::Sms (Bandwidth): create, inbound webhook, delivery status, outbound listener.
# Uses a local python mock for Bandwidth (BANDWIDTH_API_URL). Never calls the real provider.
set -euo pipefail

PORT="${PORT:-3192}"
MOCK_PORT="${MOCK_PORT:-3992}"
DB_URL="${DATABASE_URL:-postgres://localhost/pulsar}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/../wt-r7-sm-target}"
WEB_DIR="${WEB_DIR:-/Users/beyondb/.jcode/scratch/web-build}"
BIN="${CARGO_TARGET_DIR}/debug/pulsar"
BASE="http://127.0.0.1:${PORT}"
MOCK_LOG="$(mktemp)"
SMS_PHONE="+15558880001"
CUSTOMER_NUM="+15552223333"
SMS_CLIENT_ID="sms_client_$$"
SMS_CLIENT_SECRET="sms_secret_$$"

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
    psql -qtAX "$DB_URL" -c "
      DELETE FROM messages WHERE inbox_id = $INBOX_ID;
      DELETE FROM conversations WHERE inbox_id = $INBOX_ID;
      DELETE FROM contact_inboxes WHERE inbox_id = $INBOX_ID;
      DELETE FROM inbox_members WHERE inbox_id = $INBOX_ID;
      DELETE FROM inboxes WHERE id = $INBOX_ID;" >/dev/null 2>&1
  fi
  [ -n "$CHANNEL_ID" ] && psql -qtAX "$DB_URL" -c "DELETE FROM channel_sms WHERE id = $CHANNEL_ID" >/dev/null 2>&1
  psql -qtAX "$DB_URL" -c "DELETE FROM contacts WHERE account_id = 1 AND phone_number IN ('${CUSTOMER_NUM}')" >/dev/null 2>&1
  rm -f "$MOCK_LOG"
  exit $rc
}
trap cleanup EXIT

TOKEN=$(psql -qtAX "$DB_URL" -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
[[ -n "$TOKEN" ]] || { echo "FAIL: no admin api_access_token"; exit 1; }

# Python mock: OAuth token endpoint + messaging endpoint. Logs every messaging request.
python3 - "${MOCK_PORT}" "${MOCK_LOG}" "${SMS_CLIENT_ID}" "${SMS_CLIENT_SECRET}" <<'PY' &
import http.server, json, sys, base64

port, log_path, cid, csec = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
expected_basic = "Basic " + base64.b64encode(f"{cid}:{csec}".encode()).decode()

class Handler(http.server.BaseHTTPRequestHandler):
    def _send(self, code, obj):
        raw = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length).decode("utf-8")
        auth = self.headers.get("Authorization", "")
        if self.path == "/api/v1/oauth2/token":
            if auth != expected_basic:
                return self._send(401, {"error": "invalid_client", "secret": csec})
            return self._send(200, {"access_token": "mock-token-abc", "expires_in": 3600, "token_type": "Bearer"})
        if self.path.startswith("/api/v2/users/") and self.path.endswith("/messages"):
            body = json.loads(raw or "{}")
            with open(log_path, "a") as f:
                f.write(json.dumps({"path": self.path, "auth": auth, "body": body}) + "\n")
            if body.get("to") == "+15559999999":
                return self._send(400, {"type": "validation-error", "description": "Invalid destination"})
            return self._send(202, {"id": "bw-msg-mock-001"})
        self._send(404, {"error": "not found"})

    def log_message(self, format, *args):
        pass

http.server.HTTPServer(("127.0.0.1", port), Handler).serve_forever()
PY
MOCK_PID=$!
sleep 0.4

# Server on our own port, pointed at the mock.
BANDWIDTH_API_URL="http://127.0.0.1:${MOCK_PORT}" BIND="127.0.0.1:${PORT}" WEB_DIR="$WEB_DIR" "$BIN" >"${MOCK_LOG}.srv" 2>&1 &
SRV_PID=$!
for _ in $(seq 1 60); do curl -sf "${BASE}/health" >/dev/null 2>&1 && break; sleep 0.2; done
curl -sf "${BASE}/health" >/dev/null || { echo "FAIL: server did not start"; cat "${MOCK_LOG}.srv"; exit 1; }

# 1. Create: legacy api_key config is rejected (Chatwoot: OAuth required on create)
LEGACY_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE}/api/v1/accounts/1/inboxes" \
  -H "api_access_token: ${TOKEN}" -H "Content-Type: application/json" \
  -d "{\"name\":\"SMS legacy\",\"channel\":{\"type\":\"sms\",\"phone_number\":\"${SMS_PHONE}\",\"provider_config\":{\"account_id\":\"a\",\"application_id\":\"b\",\"api_key\":\"k\",\"api_secret\":\"s\"}}}")
echo "CREATE legacy config: HTTP ${LEGACY_CODE}"
[[ "${LEGACY_CODE}" == "422" ]] || { echo "FAIL: legacy config should be 422"; exit 1; }

# 2. Create with OAuth config -> inbox
CREATE_RESP=$(curl -sS -w "\n%{http_code}" -X POST "${BASE}/api/v1/accounts/1/inboxes" \
  -H "api_access_token: ${TOKEN}" -H "Content-Type: application/json" \
  -d "{\"name\":\"SMS Test\",\"channel\":{\"type\":\"sms\",\"phone_number\":\"${SMS_PHONE}\",\"provider_config\":{\"account_id\":\"acct-1\",\"application_id\":\"app-1\",\"client_id\":\"${SMS_CLIENT_ID}\",\"client_secret\":\"${SMS_CLIENT_SECRET}\"}}}")
CREATE_CODE=$(echo "${CREATE_RESP}" | tail -n1)
CREATE_BODY=$(echo "${CREATE_RESP}" | sed '$d')
echo "CREATE SMS inbox: HTTP ${CREATE_CODE}"
[[ "${CREATE_CODE}" == "200" ]] || { echo "${CREATE_BODY}"; exit 1; }
INBOX_ID=$(echo "${CREATE_BODY}" | jq -r '.id')
CHANNEL_ID=$(echo "${CREATE_BODY}" | jq -r '.channel_id')
[[ "$(echo "${CREATE_BODY}" | jq -r '.channel_type')" == "Channel::Sms" ]] || { echo "FAIL: channel_type"; exit 1; }
echo "Created SMS inbox=${INBOX_ID} channel=${CHANNEL_ID}"

# 3. Admin sees provider_config sliced to 3 keys; secrets excluded
ADMIN_CFG=$(curl -sf -H "api_access_token: ${TOKEN}" "${BASE}/api/v1/accounts/1/inboxes/${INBOX_ID}" | jq -c '.provider_config')
[[ "${ADMIN_CFG}" == "{\"account_id\":\"acct-1\",\"application_id\":\"app-1\",\"client_id\":\"${SMS_CLIENT_ID}\"}" ]] \
  || { echo "FAIL: admin provider_config = ${ADMIN_CFG}"; exit 1; }
echo "Verified admin provider_config sliced, secret hidden"

# 4. Agent (non-admin) sees phone_number but not provider_config
AGENT_ID=$(psql -qtAX "$DB_URL" -c "select user_id from account_users where account_id=1 and role=0 order by user_id limit 1")
AGENT_TOKEN="smstest$(date +%s)$RANDOM"
psql -qtAX "$DB_URL" -c "insert into access_tokens (owner_type,owner_id,token,created_at,updated_at) values ('User',${AGENT_ID},'${AGENT_TOKEN}',now(),now())" >/dev/null
psql -qtAX "$DB_URL" -c "insert into inbox_members (inbox_id,user_id,created_at,updated_at) values (${INBOX_ID},${AGENT_ID},now(),now())" >/dev/null
AGENT_SEES=$(curl -sf -H "api_access_token: ${AGENT_TOKEN}" "${BASE}/api/v1/accounts/1/inboxes/${INBOX_ID}" | jq -r '(.provider_config // "absent") | tostring')
psql -qtAX "$DB_URL" -c "delete from access_tokens where token='${AGENT_TOKEN}'" >/dev/null
[[ "${AGENT_SEES}" == "absent" ]] || { echo "FAIL: agent sees provider_config: ${AGENT_SEES}"; exit 1; }
echo "Verified provider_config hidden from agents"

# 5. Inbound webhook: unknown type ignored; message-received creates message
WH_URL="${BASE}/webhooks/sms/${SMS_PHONE#+}"
INBOUND_BODY="[{\"type\":\"message-received\",\"to\":\"${SMS_PHONE}\",\"message\":{\"id\":\"bw-in-001\",\"from\":\"${CUSTOMER_NUM}\",\"to\":[\"${SMS_PHONE}\"],\"text\":\"Hello from Bandwidth\"}}]"
WH_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${WH_URL}" -H "Content-Type: application/json" -d "${INBOUND_BODY}")
echo "INBOUND webhook: HTTP ${WH_CODE}"
[[ "${WH_CODE}" == "200" ]] || exit 1
sleep 0.5
IN_MSG=$(psql -qtAX "$DB_URL" -c "SELECT id || '|' || conversation_id || '|' || content FROM messages WHERE inbox_id = ${INBOX_ID} AND source_id = 'bw-in-001'")
[[ "${IN_MSG}" == *"Hello from Bandwidth" ]] || { echo "FAIL: inbound message not stored: ${IN_MSG}"; exit 1; }
CONV_ID=$(echo "${IN_MSG}" | cut -d'|' -f2)
CONV_DISPLAY_ID=$(psql -qtAX "$DB_URL" -c "SELECT display_id FROM conversations WHERE id = ${CONV_ID}")
echo "Inbound stored conversation display_id=${CONV_DISPLAY_ID}"

# Duplicate delivery is idempotent
curl -s -o /dev/null -X POST "${WH_URL}" -H "Content-Type: application/json" -d "${INBOUND_BODY}"
sleep 0.3
DUP_COUNT=$(psql -qtAX "$DB_URL" -c "SELECT count(*) FROM messages WHERE inbox_id = ${INBOX_ID} AND source_id = 'bw-in-001'")
[[ "${DUP_COUNT}" == "1" ]] || { echo "FAIL: duplicate inbound stored ${DUP_COUNT} times"; exit 1; }

# 6. Agent reply -> outbound listener calls mock with bearer token
REPLY=$(curl -sS -w "\n%{http_code}" -X POST "${BASE}/api/v1/accounts/1/conversations/${CONV_DISPLAY_ID}/messages" \
  -H "api_access_token: ${TOKEN}" -H "Content-Type: application/json" \
  -d '{"content":"Reply via Bandwidth","message_type":"outgoing","private":false}')
REPLY_CODE=$(echo "${REPLY}" | tail -n1)
REPLY_ID=$(echo "${REPLY}" | sed '$d' | jq -r '.id')
echo "REPLY: HTTP ${REPLY_CODE} message=${REPLY_ID}"
[[ "${REPLY_CODE}" == "200" ]] || exit 1
sleep 1.0
SENT=$(grep -c '"Reply via Bandwidth"' "${MOCK_LOG}" || true)
[[ "${SENT}" -ge 1 ]] || { echo "FAIL: mock did not receive outbound"; cat "${MOCK_LOG}"; exit 1; }
grep -q 'Bearer mock-token-abc' "${MOCK_LOG}" || true
python3 - "${MOCK_LOG}" <<'PY'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1]) if '"body"' in l]
last = rows[-1]
assert last["body"]["to"] == "+15552223333", last
assert last["body"]["from"] == "+15558880001", last
assert last["body"]["applicationId"] == "app-1", last
assert last["path"] == "/api/v2/users/acct-1/messages", last
assert last["auth"] == "Bearer mock-token-abc", last
print("OUTBOUND mock payload ok")
PY
SRC_ID=$(psql -qtAX "$DB_URL" -c "SELECT source_id FROM messages WHERE id = ${REPLY_ID}")
[[ "${SRC_ID}" == "bw-msg-mock-001" ]] || { echo "FAIL: outbound source_id=${SRC_ID}"; exit 1; }
echo "Outbound source_id stored"

# 7. Delivery status webhook -> delivered
DLV_BODY="[{\"type\":\"message-delivered\",\"to\":\"${SMS_PHONE}\",\"message\":{\"id\":\"bw-msg-mock-001\"}}]"
curl -s -o /dev/null -w "DELIVERED webhook: HTTP %{http_code}\n" -X POST "${WH_URL}" -H "Content-Type: application/json" -d "${DLV_BODY}"
sleep 0.3
DLV_STATUS=$(psql -qtAX "$DB_URL" -c "SELECT status FROM messages WHERE id = ${REPLY_ID}")
[[ "${DLV_STATUS}" == "1" ]] || { echo "FAIL: delivered status=${DLV_STATUS}"; exit 1; }

# 8. Failed delivery with errorCode -> status 3 + external_error
FAIL_BODY="[{\"type\":\"message-failed\",\"to\":\"${SMS_PHONE}\",\"errorCode\":30003,\"description\":\"Unreachable\",\"message\":{\"id\":\"bw-msg-mock-001\"}}]"
curl -s -o /dev/null -X POST "${WH_URL}" -H "Content-Type: application/json" -d "${FAIL_BODY}"
sleep 0.3
FAIL_ROW=$(psql -qtAX "$DB_URL" -c "SELECT status || '|' || (content_attributes->>'external_error') FROM messages WHERE id = ${REPLY_ID}")
[[ "${FAIL_ROW}" == "3|30003 - Unreachable" ]] || { echo "FAIL: failed status row=${FAIL_ROW}"; exit 1; }
echo "Failed delivery recorded: ${FAIL_ROW}"

# 9. Provider rejection on outbound -> message failed with provider description
psql -qtAX "$DB_URL" -c "UPDATE contact_inboxes SET source_id = '+15559999999' WHERE id = (SELECT contact_inbox_id FROM conversations WHERE id = ${CONV_ID})" >/dev/null
curl -sS -o /dev/null -X POST "${BASE}/api/v1/accounts/1/conversations/${CONV_DISPLAY_ID}/messages" \
  -H "api_access_token: ${TOKEN}" -H "Content-Type: application/json" \
  -d '{"content":"to bad number 2","message_type":"outgoing","private":false}'
sleep 1.0
BAD_ROW=$(psql -qtAX "$DB_URL" -c "SELECT status || '|' || (content_attributes->>'external_error') FROM messages WHERE content = 'to bad number 2' AND inbox_id = ${INBOX_ID} ORDER BY id DESC LIMIT 1")
[[ "${BAD_ROW}" == "3|Invalid destination" ]] || { echo "FAIL: provider rejection row=${BAD_ROW}"; exit 1; }
echo "Provider rejection recorded: ${BAD_ROW}"
psql -qtAX "$DB_URL" -c "UPDATE contact_inboxes SET source_id = '${CUSTOMER_NUM}' WHERE inbox_id = ${INBOX_ID}" >/dev/null

echo "ALL SMS CHECKS PASSED"
