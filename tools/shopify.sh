#!/usr/bin/env bash
# Integration test for Shopify endpoints with a local Python mock of Shopify Admin/OAuth APIs.
set -euo pipefail

DB=${DATABASE_URL:-postgres://localhost/pulsar}
PORT=${PORT:-3184}
MOCK_PORT=${SHOPIFY_MOCK_PORT:-19283}
BIN=${PULSAR_BIN:-/Users/beyondb/.jcode/scratch/wt-r6-ds-target/debug/pulsar}
WEB_DIR=${WEB_DIR:-/Users/beyondb/.jcode/scratch/wt-int5/web/build/client}
B="http://127.0.0.1:$PORT"

MOCK_PID=""
SRV_PID=""
CONTACT_ID=""

cleanup() {
  [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null || true
  [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null || true
  if [ -n "$CONTACT_ID" ]; then
    psql -qtAX "$DB" -c "DELETE FROM contacts WHERE id = $CONTACT_ID;" >/dev/null 2>&1 || true
  fi
  psql -qtAX "$DB" -c "DELETE FROM integrations_hooks WHERE account_id = 1 AND app_id = 'shopify';" >/dev/null 2>&1 || true
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
        _ = self.rfile.read(n)
        if self.path == "/admin/oauth/access_token":
            self._json(200, {"access_token": "shpat_test_123", "scope": "read_customers,read_orders,read_fulfillments"})
        else:
            self._json(404, {"error": "unknown"})
    def do_GET(self):
        if self.headers.get("X-Shopify-Access-Token") != "shpat_test_123":
            self._json(401, {"errors": "unauthorized"})
            return
        if self.path.startswith("/admin/api/2025-01/customers/search.json"):
            self._json(200, {"customers": [{"id": 555, "email": "shopper@example.com", "phone": None}]})
        elif self.path.startswith("/admin/api/2025-01/orders.json"):
            self._json(200, {"orders": [{"id": 9991, "email": "shopper@example.com", "created_at": "2026-01-01T00:00:00Z", "total_price": "42.00", "currency": "USD", "fulfillment_status": "fulfilled", "financial_status": "paid"}]})
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

export SHOPIFY_CLIENT_ID="test-shopify-client-id"
export SHOPIFY_CLIENT_SECRET="test-shopify-client-secret"
export SHOPIFY_API_URL="http://127.0.0.1:$MOCK_PORT"
export FRONTEND_URL="http://127.0.0.1:$PORT"
# DET_KEY='' simulates prod (no deterministic key): CE stores the hook token as plaintext then.
export ACTIVE_RECORD_ENCRYPTION_PRIMARY_KEY=testprimary ACTIVE_RECORD_ENCRYPTION_KEY_DERIVATION_SALT=testsalt
export ACTIVE_RECORD_ENCRYPTION_DETERMINISTIC_KEY=${DET_KEY-testdeterministic}

BIND="127.0.0.1:$PORT" WEB_DIR="$WEB_DIR" "$BIN" >/tmp/pulsar-shopify-test.log 2>&1 &
SRV_PID=$!

for _ in $(seq 1 40); do
  curl -sf "$B/health" >/dev/null && break
  sleep 0.25
done
curl -sf "$B/health" >/dev/null

TOKEN=$(psql -qtAX "$DB" -c "SELECT token FROM access_tokens WHERE owner_id=1 AND owner_type='User' LIMIT 1")
[ -n "$TOKEN" ]
psql -qtAX "$DB" -c "DELETE FROM integrations_hooks WHERE account_id = 1 AND app_id = 'shopify';" >/dev/null

# 1. POST integrations/shopify/auth
AUTH_RESP=$(curl -sf -X POST "$B/api/v1/accounts/1/integrations/shopify/auth" \
  -H "api_access_token: $TOKEN" -H 'content-type: application/json' \
  -d '{"shop_domain":"demo-store.myshopify.com"}')
REDIRECT_URL=$(echo "$AUTH_RESP" | jq -r '.redirect_url')
echo "$REDIRECT_URL" | grep -q "^https://demo-store.myshopify.com/admin/oauth/authorize?"
STATE=$(python3 -c "import sys,urllib.parse as u; q=u.parse_qs(u.urlparse(sys.argv[1]).query); print(q['state'][0])" "$REDIRECT_URL")
[ -n "$STATE" ]
echo "shopify/auth OK"

# 2. GET /shopify/callback (valid HMAC + state)
CB_QS=$(python3 - "$STATE" "$SHOPIFY_CLIENT_SECRET" <<'PY'
import hashlib, hmac, sys, urllib.parse
state, secret = sys.argv[1], sys.argv[2]
params = [("code", "auth-code-1"), ("shop", "demo-store.myshopify.com"), ("state", state), ("timestamp", "1700000000")]
msg = urllib.parse.urlencode(sorted(params))
sig = hmac.new(secret.encode(), msg.encode(), hashlib.sha256).hexdigest()
print(msg + "&hmac=" + sig)
PY
)
CB_HDR=$(mktemp)
CB_CODE=$(curl -s -o /dev/null -D "$CB_HDR" -w '%{http_code}' "$B/shopify/callback?$CB_QS")
[ "$CB_CODE" = "303" ] || [ "$CB_CODE" = "302" ]
tr -d '\r' < "$CB_HDR" | grep -qi "location: .*settings/integrations/shopify$"
rm -f "$CB_HDR"
STORED=$(psql -qtAX "$DB" -c "SELECT access_token FROM integrations_hooks WHERE account_id=1 AND app_id='shopify'")
if [ -n "$ACTIVE_RECORD_ENCRYPTION_DETERMINISTIC_KEY" ]; then
  [ "$STORED" != "shpat_test_123" ] && echo "$STORED" | jq -e '.p and .h.iv' >/dev/null || { echo "FAIL: token not encrypted"; exit 1; }
else
  [ "$STORED" = "shpat_test_123" ] || { echo "FAIL: expected plaintext without deterministic key"; exit 1; }
fi
echo "shopify/callback OK"

# 3. GET integrations/shopify/orders?contact_id=
CONTACT_ID=$(psql -qtAX "$DB" -c "INSERT INTO contacts (account_id, name, email, created_at, updated_at) VALUES (1, 'Shopify Test', 'shopper@example.com', now(), now()) RETURNING id;")
ORDERS_RESP=$(curl -sf "$B/api/v1/accounts/1/integrations/shopify/orders?contact_id=$CONTACT_ID" \
  -H "api_access_token: $TOKEN")
echo "$ORDERS_RESP" | jq -e '.orders | length == 1 and .[0].id == 9991 and .[0].admin_url == "https://demo-store.myshopify.com/admin/orders/9991"' >/dev/null
echo "shopify/orders OK"

# 4. POST /webhooks/shopify (invalid HMAC -> 401, valid HMAC app/uninstalled -> 200 and deletes hook)
WH_BODY='{"myshopify_domain":"demo-store.myshopify.com"}'
BAD_CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/webhooks/shopify" \
  -H 'x-shopify-topic: app/uninstalled' -H 'x-shopify-hmac-sha256: invalid' -d "$WH_BODY")
[ "$BAD_CODE" = "401" ]

WH_HMAC=$(python3 - "$SHOPIFY_CLIENT_SECRET" "$WH_BODY" <<'PY'
import base64, hashlib, hmac, sys
sec, body = sys.argv[1].encode(), sys.argv[2].encode()
print(base64.b64encode(hmac.new(sec, body, hashlib.sha256).digest()).decode())
PY
)
WH_CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/webhooks/shopify" \
  -H 'x-shopify-topic: app/uninstalled' -H "x-shopify-hmac-sha256: $WH_HMAC" -d "$WH_BODY")
[ "$WH_CODE" = "200" ]
CNT=$(psql -qtAX "$DB" -c "SELECT count(*) FROM integrations_hooks WHERE account_id=1 AND app_id='shopify';")
[ "$CNT" = "0" ]
echo "webhooks/shopify OK"

# 5. Re-connect via callback and test DELETE integrations/shopify
CB_CODE2=$(curl -s -o /dev/null -w '%{http_code}' "$B/shopify/callback?$CB_QS")
[ "$CB_CODE2" = "303" ] || [ "$CB_CODE2" = "302" ]
DEL_CODE=$(curl -s -o /dev/null -w '%{http_code}' -X DELETE "$B/api/v1/accounts/1/integrations/shopify" -H "api_access_token: $TOKEN")
[ "$DEL_CODE" = "200" ]
CNT2=$(psql -qtAX "$DB" -c "SELECT count(*) FROM integrations_hooks WHERE account_id=1 AND app_id='shopify';")
[ "$CNT2" = "0" ]
echo "DELETE integrations/shopify OK"

echo "SHOPIFY ALL PASS"
