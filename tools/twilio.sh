#!/usr/bin/env bash
set -euo pipefail

BASE="${BASE:-http://127.0.0.1:3153}"
DB_URL="${DATABASE_URL:-postgres://localhost/pulsar}"
MOCK_PORT=3995
MOCK_LOG="$(mktemp)"
MOCK_PID=""

cleanup() {
  if [[ -n "${MOCK_PID}" ]]; then
    kill "${MOCK_PID}" 2>/dev/null || true
    wait "${MOCK_PID}" 2>/dev/null || true
  fi
  rm -f "${MOCK_LOG}"
  if [[ -n "${INBOX_ID:-}" ]]; then
    psql "${DB_URL}" -q -c "
      DELETE FROM messages WHERE inbox_id = ${INBOX_ID};
      DELETE FROM conversations WHERE inbox_id = ${INBOX_ID};
      DELETE FROM contact_inboxes WHERE inbox_id = ${INBOX_ID};
      DELETE FROM contacts WHERE account_id = 1 AND phone_number IN ('+15551112222', '+15553334444');
      DELETE FROM inbox_members WHERE inbox_id = ${INBOX_ID};
      DELETE FROM inboxes WHERE id = ${INBOX_ID};
    " || true
  fi
  if [[ -n "${CHANNEL_ID:-}" ]]; then
    psql "${DB_URL}" -q -c "DELETE FROM channel_twilio_sms WHERE id = ${CHANNEL_ID};" || true
  fi
  if [[ -n "${WA_INBOX_ID:-}" ]]; then
    psql "${DB_URL}" -q -c "
      DELETE FROM messages WHERE inbox_id = ${WA_INBOX_ID};
      DELETE FROM conversations WHERE inbox_id = ${WA_INBOX_ID};
      DELETE FROM contact_inboxes WHERE inbox_id = ${WA_INBOX_ID};
      DELETE FROM inbox_members WHERE inbox_id = ${WA_INBOX_ID};
      DELETE FROM inboxes WHERE id = ${WA_INBOX_ID};
    " || true
  fi
  if [[ -n "${WA_CHANNEL_ID:-}" ]]; then
    psql "${DB_URL}" -q -c "DELETE FROM channel_twilio_sms WHERE id = ${WA_CHANNEL_ID};" || true
  fi
}
trap cleanup EXIT

TOKEN=$(psql -qtAX "${DB_URL}" -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
if [[ -z "${TOKEN}" ]]; then
  echo "FAIL: no admin api_access_token found" >&2
  exit 1
fi

# Start Python mock Twilio Messages API
python3 - "${MOCK_PORT}" "${MOCK_LOG}" <<'PY' &
import base64, http.server, json, sys, urllib.parse

port = int(sys.argv[1])
log_path = sys.argv[2]

class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length).decode("utf-8")
        params = dict(urllib.parse.parse_qsl(raw))
        auth = self.headers.get("Authorization", "")
        with open(log_path, "a") as f:
            f.write(json.dumps({
                "path": self.path,
                "auth": auth,
                "params": params,
            }) + "\n")
        resp = json.dumps({
            "sid": "SM_MOCK_OUTBOUND_001",
            "status": "queued",
            "to": params.get("To"),
            "from": params.get("From"),
            "body": params.get("Body"),
        }).encode("utf-8")
        self.send_response(201)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(resp)))
        self.end_headers()
        self.wfile.write(resp)

    def log_message(self, format, *args):
        pass

http.server.HTTPServer(("127.0.0.1", port), Handler).serve_forever()
PY
MOCK_PID=$!
sleep 0.4

ACCOUNT_SID="ACtest00000000000000000000000001"
AUTH_TOKEN="twilio_test_secret_token_123"
TWILIO_NUM="+15559990001"
CUSTOMER_NUM="+15551112222"

# 1. Create SMS channel
CREATE_RESP=$(curl -sS -w "\n%{http_code}" -X POST "${BASE}/api/v1/accounts/1/channels/twilio_channel" \
  -H "api_access_token: ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"twilio_channel\": {
      \"name\": \"Twilio SMS Test\",
      \"account_sid\": \"${ACCOUNT_SID}\",
      \"auth_token\": \"${AUTH_TOKEN}\",
      \"phone_number\": \"${TWILIO_NUM}\",
      \"medium\": \"sms\"
    }
  }")
CREATE_CODE=$(echo "${CREATE_RESP}" | tail -n1)
CREATE_BODY=$(echo "${CREATE_RESP}" | sed '$d')
echo "CREATE SMS: HTTP ${CREATE_CODE}"
[[ "${CREATE_CODE}" == "200" ]] || { echo "${CREATE_BODY}"; exit 1; }

INBOX_ID=$(echo "${CREATE_BODY}" | jq -r '.id')
CHANNEL_ID=$(echo "${CREATE_BODY}" | jq -r '.channel_id')
[[ -n "${INBOX_ID}" && "${INBOX_ID}" != "null" ]] || exit 1
echo "Created SMS inbox=${INBOX_ID} channel=${CHANNEL_ID}"

# Check auth_token is encrypted in DB when encryption keys are configured
DB_TOKEN=$(psql -qtAX "${DB_URL}" -c "SELECT auth_token FROM channel_twilio_sms WHERE id = ${CHANNEL_ID}")
if [[ -n "${ACTIVE_RECORD_ENCRYPTION_PRIMARY_KEY:-}" ]]; then
  [[ "${DB_TOKEN}" != "${AUTH_TOKEN}" ]] || { echo "FAIL: auth_token stored plaintext"; exit 1; }
  echo "Verified auth_token encrypted in DB"
fi

# Credentials are visible to admins only (_inbox.json.jbuilder)
ADMIN_SEES=$(curl -sf -H "api_access_token: ${TOKEN}" "${BASE}/api/v1/accounts/1/inboxes/${INBOX_ID}" | jq -r '.auth_token')
[[ "${ADMIN_SEES}" == "${AUTH_TOKEN}" ]] || { echo "FAIL: admin should see auth_token, got ${ADMIN_SEES}"; exit 1; }
AGENT_ID=$(psql -qtAX "${DB_URL}" -c "select user_id from account_users where account_id=1 and role=0 order by user_id limit 1")
AGENT_TOKEN="twtest$(date +%s)$RANDOM"
psql -qtAX "${DB_URL}" -c "insert into access_tokens (owner_type,owner_id,token,created_at,updated_at) values ('User',${AGENT_ID},'${AGENT_TOKEN}',now(),now())" >/dev/null
psql -qtAX "${DB_URL}" -c "insert into inbox_members (inbox_id,user_id,created_at,updated_at) values (${INBOX_ID},${AGENT_ID},now(),now())" >/dev/null
AGENT_SEES=$(curl -sf -H "api_access_token: ${AGENT_TOKEN}" "${BASE}/api/v1/accounts/1/inboxes" | jq -r ".payload[] | select(.id==${INBOX_ID}) | (.auth_token // \"absent\") + \"|\" + (.account_sid // \"absent\") + \"|\" + .medium")
psql -qtAX "${DB_URL}" -c "delete from access_tokens where token='${AGENT_TOKEN}'" >/dev/null
[[ "${AGENT_SEES}" == "absent|absent|sms" ]] || { echo "FAIL: agent sees credentials: ${AGENT_SEES}"; exit 1; }
echo "Verified credentials hidden from agents"

# 2. Bad signature -> 401
BAD_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE}/twilio/callback" \
  -H "X-Twilio-Signature: invalid_sig" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data-urlencode "AccountSid=${ACCOUNT_SID}" \
  --data-urlencode "From=${CUSTOMER_NUM}" \
  --data-urlencode "To=${TWILIO_NUM}" \
  --data-urlencode "Body=Should reject" \
  --data-urlencode "MessageSid=SM_BAD_001")
echo "BAD SIG callback: HTTP ${BAD_CODE}"
[[ "${BAD_CODE}" == "401" ]] || exit 1

# 3. Valid signed callback -> 204
python3 - "${BASE}/twilio/callback" "${AUTH_TOKEN}" "${ACCOUNT_SID}" "${CUSTOMER_NUM}" "${TWILIO_NUM}" <<'PY'
import base64, hashlib, hmac, sys, urllib.parse, urllib.request

url, token, acct, frm, to = sys.argv[1:6]
params = {
    "AccountSid": acct,
    "Body": "Hello from Twilio SMS",
    "From": frm,
    "MessageSid": "SM_INBOUND_001",
    "To": to,
}
s = url + "".join(k + params[k] for k in sorted(params))
sig = base64.b64encode(hmac.new(token.encode(), s.encode(), hashlib.sha1).digest()).decode()
data = urllib.parse.urlencode(params).encode()
req = urllib.request.Request(url, data=data, method="POST")
req.add_header("Content-Type", "application/x-www-form-urlencoded")
req.add_header("X-Twilio-Signature", sig)
with urllib.request.urlopen(req) as resp:
    assert resp.status == 204, f"Expected 204, got {resp.status}"
print("SIGNED callback: HTTP 204")
PY

# Verify incoming message in DB
IN_MSG_ID=$(psql -qtAX "${DB_URL}" -c "SELECT id FROM messages WHERE inbox_id = ${INBOX_ID} AND source_id = 'SM_INBOUND_001'")
[[ -n "${IN_MSG_ID}" ]] || { echo "FAIL: incoming message not created"; exit 1; }
CONV_DISPLAY_ID=$(psql -qtAX "${DB_URL}" -c "SELECT c.display_id FROM conversations c JOIN messages m ON m.conversation_id = c.id WHERE m.id = ${IN_MSG_ID}")
echo "Incoming message id=${IN_MSG_ID} conversation display_id=${CONV_DISPLAY_ID}"

# 4. Agent reply -> outbound listener hits mock
REPLY_RESP=$(curl -sS -w "\n%{http_code}" -X POST "${BASE}/api/v1/accounts/1/conversations/${CONV_DISPLAY_ID}/messages" \
  -H "api_access_token: ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"content": "Reply from Pulsar agent", "message_type": "outgoing", "private": false}')
REPLY_CODE=$(echo "${REPLY_RESP}" | tail -n1)
REPLY_BODY=$(echo "${REPLY_RESP}" | sed '$d')
echo "AGENT REPLY: HTTP ${REPLY_CODE}"
[[ "${REPLY_CODE}" == "200" ]] || { echo "${REPLY_BODY}"; exit 1; }
OUT_MSG_ID=$(echo "${REPLY_BODY}" | jq -r '.id')

# Wait briefly for async listener
for _ in $(seq 1 20); do
  if [[ -s "${MOCK_LOG}" ]]; then
    break
  fi
  sleep 0.1
done
[[ -s "${MOCK_LOG}" ]] || { echo "FAIL: outbound mock did not receive request"; exit 1; }

python3 - "${MOCK_LOG}" "${ACCOUNT_SID}" "${AUTH_TOKEN}" "${CUSTOMER_NUM}" "${TWILIO_NUM}" <<'PY'
import base64, json, sys

log_path, acct, token, to_num, from_num = sys.argv[1:6]
with open(log_path) as f:
    rec = json.loads(f.readline())
assert rec["path"] == f"/2010-04-01/Accounts/{acct}/Messages.json", rec
expected_auth = "Basic " + base64.b64encode(f"{acct}:{token}".encode()).decode()
assert rec["auth"] == expected_auth, rec["auth"]
assert rec["params"]["To"] == to_num, rec["params"]
assert rec["params"]["From"] == from_num, rec["params"]
assert rec["params"]["Body"] == "Reply from Pulsar agent", rec["params"]
print("Outbound mock verified:", rec["params"])
PY

# Verify source_id saved on outgoing message
sleep 0.2
OUT_SOURCE_ID=$(psql -qtAX "${DB_URL}" -c "SELECT source_id FROM messages WHERE id = ${OUT_MSG_ID}")
[[ "${OUT_SOURCE_ID}" == "SM_MOCK_OUTBOUND_001" ]] || { echo "FAIL: expected source_id SM_MOCK_OUTBOUND_001, got '${OUT_SOURCE_ID}'"; exit 1; }
echo "Outgoing message source_id=${OUT_SOURCE_ID}"

# 5. Delivery status -> bad sig 401, valid sig updates status
BAD_DS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE}/twilio/delivery_status" \
  -H "X-Twilio-Signature: wrong" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data-urlencode "AccountSid=${ACCOUNT_SID}" \
  --data-urlencode "From=${TWILIO_NUM}" \
  --data-urlencode "MessageSid=SM_MOCK_OUTBOUND_001" \
  --data-urlencode "MessageStatus=delivered")
echo "BAD SIG delivery_status: HTTP ${BAD_DS}"
[[ "${BAD_DS}" == "401" ]] || exit 1

python3 - "${BASE}/twilio/delivery_status" "${AUTH_TOKEN}" "${ACCOUNT_SID}" "${TWILIO_NUM}" <<'PY'
import base64, hashlib, hmac, sys, urllib.parse, urllib.request

url, token, acct, frm = sys.argv[1:5]
for st in ("delivered", "read"):
    params = {
        "AccountSid": acct,
        "From": frm,
        "MessageSid": "SM_MOCK_OUTBOUND_001",
        "MessageStatus": st,
    }
    s = url + "".join(k + params[k] for k in sorted(params))
    sig = base64.b64encode(hmac.new(token.encode(), s.encode(), hashlib.sha1).digest()).decode()
    data = urllib.parse.urlencode(params).encode()
    req = urllib.request.Request(url, data=data, method="POST")
    req.add_header("Content-Type", "application/x-www-form-urlencoded")
    req.add_header("X-Twilio-Signature", sig)
    with urllib.request.urlopen(req) as resp:
        assert resp.status == 204, f"Expected 204, got {resp.status}"
print("SIGNED delivery_status: HTTP 204")
PY

MSG_STATUS=$(psql -qtAX "${DB_URL}" -c "SELECT status FROM messages WHERE id = ${OUT_MSG_ID}")
[[ "${MSG_STATUS}" == "2" ]] || { echo "FAIL: expected status=2 (read), got ${MSG_STATUS}"; exit 1; }
echo "Delivery status updated message status=${MSG_STATUS} (read)"

# 6. WhatsApp medium channel + callback with whatsapp: prefix
WA_RESP=$(curl -sS -w "\n%{http_code}" -X POST "${BASE}/api/v1/accounts/1/channels/twilio_channel" \
  -H "api_access_token: ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"twilio_channel\": {
      \"name\": \"Twilio WA Test\",
      \"account_sid\": \"${ACCOUNT_SID}\",
      \"auth_token\": \"${AUTH_TOKEN}\",
      \"phone_number\": \"+15559990002\",
      \"medium\": \"whatsapp\"
    }
  }")
WA_CODE=$(echo "${WA_RESP}" | tail -n1)
WA_BODY=$(echo "${WA_RESP}" | sed '$d')
[[ "${WA_CODE}" == "200" ]] || { echo "${WA_BODY}"; exit 1; }
WA_INBOX_ID=$(echo "${WA_BODY}" | jq -r '.id')
WA_CHANNEL_ID=$(echo "${WA_BODY}" | jq -r '.channel_id')

python3 - "${BASE}/twilio/callback" "${AUTH_TOKEN}" "${ACCOUNT_SID}" <<'PY'
import base64, hashlib, hmac, sys, urllib.parse, urllib.request

url, token, acct = sys.argv[1:4]
params = {
    "AccountSid": acct,
    "Body": "Hello via WhatsApp",
    "From": "whatsapp:+15553334444",
    "MessageSid": "SM_WA_001",
    "To": "whatsapp:+15559990002",
}
s = url + "".join(k + params[k] for k in sorted(params))
sig = base64.b64encode(hmac.new(token.encode(), s.encode(), hashlib.sha1).digest()).decode()
data = urllib.parse.urlencode(params).encode()
req = urllib.request.Request(url, data=data, method="POST")
req.add_header("Content-Type", "application/x-www-form-urlencoded")
req.add_header("X-Twilio-Signature", sig)
with urllib.request.urlopen(req) as resp:
    assert resp.status == 204
print("SIGNED WhatsApp callback: HTTP 204")
PY

WA_PHONE=$(psql -qtAX "${DB_URL}" -c "SELECT c.phone_number FROM contacts c JOIN contact_inboxes ci ON ci.contact_id = c.id WHERE ci.inbox_id = ${WA_INBOX_ID} LIMIT 1")
[[ "${WA_PHONE}" == "+15553334444" ]] || { echo "FAIL: expected stripped phone +15553334444, got '${WA_PHONE}'"; exit 1; }
echo "WhatsApp contact phone stripped cleanly: ${WA_PHONE}"

echo "ALL TWILIO CHECKS PASSED"
