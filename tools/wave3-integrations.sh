#!/usr/bin/env bash
# Wave 3 Parity Integration Test: Linear, Notion, Stripe, Shopify Custom Apps,
# Slack webhooks, Google Translate + auto_translate, and Dialogflow listener.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}:$PATH"
export LC_ALL=C
unset PGHOST PGHOSTADDR PGPORT PGUSER PGPASSWORD PGSERVICE PGDATABASE PGOPTIONS

work=$(mktemp -d /tmp/pulsar-wave3.XXXXXXXX)
chmod 700 "$work"
app_pid=
mock_pid=
cleanup() {
  rc=$?
  if [[ $rc -ne 0 && -f "$work/pulsar.log" ]]; then
    echo "=== pulsar.log ===" >&2
    tail -n 60 "$work/pulsar.log" >&2 || true
  fi
  if [[ -n "$app_pid" ]]; then kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true; fi
  if [[ -n "$mock_pid" ]]; then kill "$mock_pid" 2>/dev/null || true; wait "$mock_pid" 2>/dev/null || true; fi
  pg_ctl -D "$work/db" -m fast stop >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

initdb -D "$work/db" -U fixture -A trust --locale=C --encoding=UTF8 >/dev/null
pg_ctl -D "$work/db" -l "$work/postgres.log" -o "-h '' -k $work -c timezone=UTC" start >/dev/null
for f in migrations/0001_schema.sql migrations/0002_pulsar_indexes.sql migrations/0003_pulsar_widget_token_floor.sql migrations/0004_pulsar_draft_messages.sql migrations/0005_pulsar_events.sql; do
  psql -h "$work" -U fixture -d postgres -v ON_ERROR_STOP=1 -q -f "$f" >/dev/null
done

export DATABASE_URL="postgres://fixture@localhost/postgres?host=$work"
export STORAGE_DIR="$work/storage"
cargo build --offline --locked --quiet

target/debug/pulsar seed admin@example.com 'Password123!' 'Wave3 Org' >/dev/null
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -q <<'SQL'
INSERT INTO users (id, name, uid, email, provider, pubsub_token, confirmed_at, encrypted_password, created_at, updated_at)
VALUES (20, 'Agent User', 'agent@example.com', 'agent@example.com', 'email', 'agent-stream', now(),
        (SELECT encrypted_password FROM users WHERE id = 1), now(), now());
INSERT INTO account_users (account_id, user_id, role, created_at, updated_at)
VALUES (1, 20, 0, now(), now());
INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at)
VALUES ('User', 20, 'wave3-agent-token', now(), now());
SELECT setval('users_id_seq', (SELECT max(id) FROM users));
SQL

port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
mock_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')

python3 - "$mock_port" >"$work/mock.log" 2>&1 <<'PY' &
import http.server, json, sys, urllib.parse
mock_port = int(sys.argv[1])

class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        p = urllib.parse.urlparse(self.path).path
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        if "/v1/customers/search" in p:
            self.wfile.write(json.dumps({"data": [{"id": "cus_1", "name": "Alice Stripe", "email": "cust@example.com", "phone": "+1555"}], "has_more": False}).encode())
        elif "/v1/subscriptions" in p:
            self.wfile.write(json.dumps({"data": [{"id": "sub_1", "status": "active"}]}).encode())
        elif "/v1/invoices" in p:
            self.wfile.write(json.dumps({"data": [{"id": "in_1", "number": "INV-1", "status": "paid", "total": 2500, "currency": "usd", "created": 1700000000}]}).encode())
        else:
            self.wfile.write(json.dumps({"ok": True}).encode())

    def do_POST(self):
        ln = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(ln).decode()
        p = urllib.parse.urlparse(self.path).path
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        if p == "/linear/oauth/token":
            self.wfile.write(json.dumps({"access_token": "lin_tok_1", "token_type": "Bearer", "expires_in": 3600}).encode())
        elif p == "/linear/graphql":
            body = json.loads(raw)
            q = body.get("query", "")
            if "teams" in q:
                self.wfile.write(json.dumps({"data": {"teams": {"nodes": [{"id": "team-1", "name": "Engineering"}]}}}).encode())
            elif "team(" in q:
                self.wfile.write(json.dumps({"data": {"team": {"states": {"nodes": []}, "labels": {"nodes": []}, "members": {"nodes": []}, "projects": {"nodes": []}}}}).encode())
            elif "issueCreate" in q:
                self.wfile.write(json.dumps({"data": {"issueCreate": {"success": True, "issue": {"id": "iss-1", "identifier": "ENG-42", "title": "Bug", "url": "https://linear.app/eng/ENG-42"}}}}).encode())
            elif "attachmentCreate" in q:
                self.wfile.write(json.dumps({"data": {"attachmentCreate": {"success": True, "attachment": {"id": "att-1", "url": "http://x", "title": "Conv", "issue": {"id": "iss-1", "identifier": "ENG-42", "title": "Bug"}}}}}).encode())
            elif "attachmentDelete" in q:
                self.wfile.write(json.dumps({"data": {"attachmentDelete": {"success": True}}}).encode())
            elif "searchIssues" in q:
                self.wfile.write(json.dumps({"data": {"searchIssues": {"nodes": [{"id": "iss-1", "identifier": "ENG-42", "title": "Bug"}]}}}).encode())
            elif "attachmentsForURL" in q:
                self.wfile.write(json.dumps({"data": {"attachmentsForURL": {"nodes": [{"id": "att-1", "issue": {"id": "iss-1", "identifier": "ENG-42", "title": "Bug"}}]}}}).encode())
            else:
                self.wfile.write(json.dumps({"data": {}}).encode())
        elif p == "/notion/oauth/token":
            self.wfile.write(json.dumps({"access_token": "not_tok_1", "token_type": "bearer", "workspace_id": "ws-1", "workspace_name": "Wave3 Wiki", "bot_id": "bot-1"}).encode())
        elif p == "/stripe/oauth/token":
            self.wfile.write(json.dumps({"access_token": "sk_test_1", "livemode": False, "stripe_user_id": "acct_123", "refresh_token": "rt_1"}).encode())
        elif "/admin/oauth/access_token" in p:
            self.wfile.write(json.dumps({"access_token": "shpat_custom_1", "scope": "read_customers,read_orders"}).encode())
        elif p == "/translate":
            self.wfile.write(json.dumps({"data": {"translations": [{"translatedText": "Hola mundo"}]}}).encode())
        elif p == "/dialogflow":
            self.wfile.write(json.dumps({"queryResult": {"fulfillmentText": "Dialogflow says hi!"}}).encode())
        else:
            self.wfile.write(json.dumps({"ok": True}).encode())

    def log_message(self, *args): pass

http.server.HTTPServer(("127.0.0.1", mock_port), H).serve_forever()
PY
mock_pid=$!

export FRONTEND_URL="http://127.0.0.1:$port"
export LINEAR_CLIENT_ID="lin-cid"
export LINEAR_CLIENT_SECRET="lin-sec"
export LINEAR_API_URL="http://127.0.0.1:$mock_port/linear/graphql"
export LINEAR_OAUTH_TOKEN_URL="http://127.0.0.1:$mock_port/linear/oauth/token"
export NOTION_CLIENT_ID="not-cid"
export NOTION_CLIENT_SECRET="not-sec"
export NOTION_OAUTH_TOKEN_URL="http://127.0.0.1:$mock_port/notion/oauth/token"
export STRIPE_APP_CLIENT_ID="ca_test"
export STRIPE_APP_SECRET_KEY="sk_test_app"
export STRIPE_APP_WEBHOOK_SECRET="whsec_wave3"
export STRIPE_API_URL="http://127.0.0.1:$mock_port"
export STRIPE_OAUTH_TOKEN_URL="http://127.0.0.1:$mock_port/stripe/oauth/token"
export GOOGLE_TRANSLATE_API_URL="http://127.0.0.1:$mock_port/translate"
export DIALOGFLOW_API_URL="http://127.0.0.1:$mock_port/dialogflow"
export SLACK_SIGNING_SECRET="slack-sec-wave3"
export SHOPIFY_API_URL="http://127.0.0.1:$mock_port"
export PULSAR_SCHEDULER_INTERVAL_SECS=3600

BIND="127.0.0.1:$port" target/debug/pulsar >"$work/pulsar.log" 2>&1 &
app_pid=$!

for ((i=0; i<100; i++)); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
curl -fsS "http://127.0.0.1:$port/health" >/dev/null

python3 - "$port" "$DATABASE_URL" <<'PY'
import hashlib, hmac, json, subprocess, sys, time, urllib.error, urllib.parse, urllib.request

base = f"http://127.0.0.1:{sys.argv[1]}"
db = sys.argv[2]

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None
opener = urllib.request.build_opener(NoRedirect)

def req(path, method="GET", body=None, headers=None, expect=200, raw_bytes=None):
    hdrs = dict(headers or {})
    data = raw_bytes
    if body is not None:
        hdrs["Content-Type"] = "application/json"
        data = json.dumps(body).encode()
    r = urllib.request.Request(base + path, data=data, headers=hdrs, method=method)
    try:
        with opener.open(r, timeout=5) as res:
            assert res.status == expect, (path, res.status, expect)
            raw = res.read()
            ctype = res.headers.get("Content-Type", "")
            return (json.loads(raw.decode()) if "json" in ctype and raw else raw), res.headers
    except urllib.error.HTTPError as e:
        raw = e.read()
        assert e.code == expect, (path, method, e.code, expect, raw)
        ctype = e.headers.get("Content-Type", "")
        return (json.loads(raw.decode()) if "json" in ctype and raw else raw), e.headers

login, _ = req("/auth/sign_in", "POST", {"email": "admin@example.com", "password": "Password123!"})
H_ADMIN = {"api_access_token": login["data"]["access_token"]}
H_AGENT = {"api_access_token": "wave3-agent-token"}

# Seed inbox, contact, conversation #1, initial message
inbox_res, _ = req("/api/v1/accounts/1/inboxes", "POST", {
    "name": "Wave3 API", "channel": {"type": "api"}
}, H_ADMIN)
inbox_id = inbox_res["id"]
ct, _ = req("/api/v1/accounts/1/contacts", "POST", {
    "name": "Cust Stripe", "email": "cust@example.com"
}, H_ADMIN)
contact_id = ct["payload"]["contact"]["id"]
conv, _ = req("/api/v1/accounts/1/conversations", "POST", {
    "source_id": "src-w3", "inbox_id": inbox_id, "contact_id": contact_id
}, H_ADMIN)
conv_id = conv["id"]
msg_res, _ = req(f"/api/v1/accounts/1/conversations/{conv_id}/messages", "POST", {
    "content": "Hello world", "message_type": "outgoing"
}, H_ADMIN)
msg_id = msg_res["id"]

# RBAC checks for non-admin
req("/api/v1/accounts/1/integrations/linear/auth", "POST", {}, H_AGENT, expect=403)
req("/api/v1/accounts/1/integrations/stripe/auth", "POST", {}, H_AGENT, expect=403)
req("/api/v1/accounts/1/notion/authorization", "POST", {}, H_AGENT, expect=403)

# 1. Linear OAuth + GraphQL + issue create/link/search/unlink + delete
lin_auth, _ = req("/api/v1/accounts/1/integrations/linear/auth", "POST", {}, H_ADMIN)
lin_state = urllib.parse.parse_qs(urllib.parse.urlparse(lin_auth["url"]).query)["state"][0]
_, lin_hdrs = req(f"/linear/callback?code=lincode&state={urllib.parse.quote(lin_state)}", expect=307)
assert "/settings/integrations/linear" in lin_hdrs["Location"]
teams, _ = req("/api/v1/accounts/1/integrations/linear/teams", "GET", None, H_ADMIN)
assert teams[0]["name"] == "Engineering", teams
ent, _ = req("/api/v1/accounts/1/integrations/linear/team_entities?team_id=team-1", "GET", None, H_ADMIN)
assert "states" in ent
created_iss, _ = req("/api/v1/accounts/1/integrations/linear/create_issue", "POST", {
    "conversation_id": conv_id, "team_id": "team-1", "title": "Bug"
}, H_ADMIN)
assert created_iss["identifier"] == "ENG-42"
linked, _ = req(f"/api/v1/accounts/1/integrations/linear/linked_issues?conversation_id={conv_id}", "GET", None, H_ADMIN)
assert len(linked) == 1
searched, _ = req("/api/v1/accounts/1/integrations/linear/search_issue?q=Bug", "GET", None, H_ADMIN)
assert searched[0]["identifier"] == "ENG-42"
req("/api/v1/accounts/1/integrations/linear/unlink_issue", "POST", {
    "conversation_id": conv_id, "link_id": "att-1"
}, H_ADMIN)
req("/api/v1/accounts/1/integrations/linear", "DELETE", None, H_ADMIN)

# 2. Notion authorization + callback + delete
not_auth, _ = req("/api/v1/accounts/1/notion/authorization", "POST", {}, H_ADMIN)
not_state = urllib.parse.parse_qs(urllib.parse.urlparse(not_auth["url"]).query)["state"][0]
_, not_hdrs = req(f"/notion/callback?code=notcode&state={urllib.parse.quote(not_state)}", expect=307)
assert "/settings/integrations/notion" in not_hdrs["Location"]
not_app, _ = req("/api/v1/accounts/1/integrations/apps/notion", "GET", None, H_ADMIN)
assert len(not_app["hooks"]) == 1 and not_app["hooks"][0]["settings"]["workspace_name"] == "Wave3 Wiki", not_app
req("/api/v1/accounts/1/integrations/notion", "DELETE", None, H_ADMIN)

# 3. Stripe auth + callback + show + customer + webhook deauth
st_auth, _ = req("/api/v1/accounts/1/integrations/stripe/auth", "POST", {}, H_ADMIN)
st_state = urllib.parse.parse_qs(urllib.parse.urlparse(st_auth["url"]).query)["state"][0]
_, st_hdrs = req(f"/stripe/callback?code=stcode&state={urllib.parse.quote(st_state)}", expect=307)
assert "/settings/integrations/stripe" in st_hdrs["Location"]
st_show, _ = req("/api/v1/accounts/1/integrations/stripe", "GET", None, H_ADMIN)
assert st_show["account_id"] == "acct_123" and st_show["mode"] == "sandbox", st_show
st_cust, _ = req(f"/api/v1/accounts/1/integrations/stripe/customer?conversation_id={conv_id}", "GET", None, H_ADMIN)
assert st_cust["customer"]["id"] == "cus_1" and len(st_cust["subscriptions"]) == 1 and len(st_cust["invoices"]) == 1, st_cust

deauth_ev = json.dumps({"type": "account.application.deauthorized", "account": "acct_123"}).encode()
req("/webhooks/stripe_app", "POST", raw_bytes=deauth_ev, headers={"Content-Type": "application/json", "Stripe-Signature": "t=1,v1=dead"}, expect=400)
st_sig = hmac.new(b"whsec_wave3", b"1700000000." + deauth_ev, hashlib.sha256).hexdigest()
req("/webhooks/stripe_app", "POST", raw_bytes=deauth_ev, headers={"Content-Type": "application/json", "Stripe-Signature": f"t=1700000000,v1={st_sig}"}, expect=200)
req("/api/v1/accounts/1/integrations/stripe", "GET", None, H_ADMIN, expect=404)

# 4. Shopify custom app CRUD + callback + webhook
ca0, _ = req("/api/v1/accounts/1/integrations/shopify_custom_app", "GET", None, H_ADMIN)
assert ca0["custom_app"] is None
ca1, _ = req("/api/v1/accounts/1/integrations/shopify_custom_app", "POST", {
    "shop_domain": "mystore.myshopify.com",
    "client_id": "shp_cid",
    "client_secret": "shp_csec",
    "install_url": "https://mystore.myshopify.com/install"
}, H_ADMIN)
assert ca1["custom_app"]["callback_url"].endswith("/shopify/accounts/1/callback"), ca1
ca2, _ = req("/api/v1/accounts/1/integrations/shopify_custom_app", "PATCH", {"enabled": True}, H_ADMIN)
assert ca2["custom_app"]["enabled"] is True

pairs = [("code", "shp_code"), ("shop", "mystore.myshopify.com"), ("timestamp", "1700000000")]
msg_qs = "&".join(f"{k}={v}" for k, v in sorted(pairs))
shp_hmac = hmac.new(b"shp_csec", msg_qs.encode(), hashlib.sha256).hexdigest()
_, ca_cb = req(f"/shopify/accounts/1/callback?{msg_qs}&hmac={shp_hmac}", expect=303)
assert "/settings/integrations/shopify" in ca_cb["Location"] and "error" not in ca_cb["Location"]
req("/api/v1/accounts/1/integrations/shopify_custom_app", "DELETE", None, H_ADMIN, expect=409)
req("/api/v1/accounts/1/integrations/shopify", "DELETE", None, H_ADMIN)
req("/api/v1/accounts/1/integrations/shopify_custom_app", "DELETE", None, H_ADMIN)

# 5. Slack webhook url_verification
sl_raw = json.dumps({"type": "url_verification", "challenge": "chal_xyz"}).encode()
ts = str(int(time.time()))
req("/api/v1/integrations/webhooks", "POST", raw_bytes=sl_raw, headers={"Content-Type": "application/json", "X-Slack-Request-Timestamp": ts, "X-Slack-Signature": "v0=bad"}, expect=401)
sl_sig = "v0=" + hmac.new(b"slack-sec-wave3", f"v0:{ts}:".encode() + sl_raw, hashlib.sha256).hexdigest()
sl_res, _ = req("/api/v1/integrations/webhooks", "POST", raw_bytes=sl_raw, headers={"Content-Type": "application/json", "X-Slack-Request-Timestamp": ts, "X-Slack-Signature": sl_sig}, expect=200)
assert sl_res["challenge"] == "chal_xyz"

# 6. Google Translate + auto_translate + Dialogflow listener
req("/api/v1/accounts/1/integrations/hooks", "POST", {
    "hook": {"app_id": "google_translate", "settings": {"project_id": "p1", "api_key": "k1"}}
}, H_ADMIN)
tr, _ = req(f"/api/v1/accounts/1/conversations/{conv_id}/messages/{msg_id}/translate", "POST", {"target_language": "es"}, H_ADMIN)
assert tr["content"] == "Hola mundo", tr
atr, _ = req(f"/api/v1/accounts/1/conversations/{conv_id}/messages/auto_translate", "POST", {"message_ids": [msg_id], "target_language": "es"}, H_ADMIN, expect=202)
assert atr["translations"][str(msg_id)] == "Hola mundo", atr

req("/api/v1/accounts/1/integrations/hooks", "POST", {
    "hook": {"app_id": "dialogflow", "inbox_id": inbox_id, "settings": {"project_id": "df1", "region": "en"}}
}, H_ADMIN)
req(f"/api/v1/accounts/1/conversations/{conv_id}/messages", "POST", {
    "content": "Hi bot!", "message_type": "incoming"
}, H_ADMIN)
found_bot = False
for _ in range(15):
    msgs, _ = req(f"/api/v1/accounts/1/conversations/{conv_id}/messages", "GET", None, H_ADMIN)
    if any(m.get("content") == "Dialogflow says hi!" for m in msgs.get("payload", [])):
        found_bot = True
        break
    time.sleep(0.2)
assert found_bot, "Dialogflow reply not received"

print("WAVE 3 INTEGRATIONS PARITY CHECKS PASSED")
PY
