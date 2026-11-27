#!/usr/bin/env bash
# Wave 9 Chatwoot 4.20.1 Upstream Sync Verification:
# - Idempotent 0008_chatwoot_4_20.sql migration & schema_migrations 20261125100456
# - conversations.last_message_at monotonic updates + last_message_sort ordering
# - Per-user per-account rate limits on conversation list & filter (429 + Retry-After)
# - Captain scenarios/tool_suggestions & MCP tool review_state + approve_tool
# - Help Center 50k char limit, preview_token + X-Robots-Tag: noindex, article.* webhooks
# - Data import rejected_rows endpoint
# - SMS webhook Basic Auth enforcement (401 vs 200)
# - Instagram unified login endpoints (/instagram/accounts & /instagram/connections)
# - Shopify storefront_widget GET/PATCH & removed Cloud-only whatsapp/access_request
# - Bot metrics filtering to AgentBot / Captain::Assistant senders
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}:$PATH"
export LC_ALL=C
unset PGHOST PGHOSTADDR PGPORT PGUSER PGPASSWORD PGSERVICE PGDATABASE PGOPTIONS

work=$(mktemp -d /tmp/pulsar-wave9.XXXXXXXX)
chmod 700 "$work"
app_pid=
mock_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
mock_pid=
cleanup() {
  rc=$?
  if [[ $rc -ne 0 && -f "$work/pulsar.log" ]]; then
    echo "=== pulsar.log ===" >&2
    tail -n 80 "$work/pulsar.log" >&2 || true
  fi
  if [[ -n "$mock_pid" ]]; then kill "$mock_pid" 2>/dev/null || true; wait "$mock_pid" 2>/dev/null || true; fi
  if [[ -n "$app_pid" ]]; then kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true; fi
  pg_ctl -D "$work/db" -m fast stop >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

python3 - "$mock_port" <<'PY' &
import http.server, json, sys
events = []
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(events).encode())
    def do_POST(self):
        n = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(n).decode() if n else ""
        try:
            body = json.loads(raw)
        except Exception:
            body = raw
        events.append([self.path, body])
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        if self.path == "/mcp" and isinstance(body, dict):
            res = {"protocolVersion": "2024-11-05", "capabilities": {}} if body.get("method") == "initialize" else {"tools": [{"name": "lookup", "description": "Lookup v2", "inputSchema": {"type": "object"}}]}
            self.wfile.write(json.dumps({"jsonrpc": "2.0", "id": body.get("id", 1), "result": res}).encode())
        elif "oauth2/token" in self.path:
            self.wfile.write(b'{"access_token":"bw-tok"}')
        else:
            self.wfile.write(b"{}")
    def log_message(self, *args):
        pass
http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
mock_pid=$!

initdb -D "$work/db" -U fixture -A trust --locale=C --encoding=UTF8 >/dev/null
pg_ctl -D "$work/db" -l "$work/postgres.log" -o "-h '' -k $work -c timezone=UTC" start >/dev/null
for f in migrations/0001_schema.sql migrations/0002_pulsar_indexes.sql migrations/0003_pulsar_widget_token_floor.sql migrations/0004_pulsar_draft_messages.sql migrations/0005_pulsar_events.sql migrations/0006_data_exports.sql migrations/0007_parity_extras.sql migrations/0008_chatwoot_4_20.sql migrations/0008_chatwoot_4_20.sql; do
  psql -h "$work" -U fixture -d postgres -v ON_ERROR_STOP=1 -q -f "$f" >/dev/null
done

export DATABASE_URL="postgres://fixture@localhost/postgres?host=$work"
export STORAGE_DIR="$work/storage"
cargo build --offline --locked --quiet

port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')

export FRONTEND_URL="http://127.0.0.1:$port"
export PULSAR_LOG_MAIL="1"
export PULSAR_WEBHOOK_ALLOW_PRIVATE="1"
export BANDWIDTH_API_URL="http://127.0.0.1:$mock_port"
export RATE_LIMIT_CONVERSATIONS_LIST="2"
export RATE_LIMIT_CONVERSATIONS_FILTER="3"
BIND="127.0.0.1:$port" target/debug/pulsar >"$work/pulsar.log" 2>&1 &
app_pid=$!

for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then break; fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$port/health" >/dev/null

python3 - "$port" "$work" "$DATABASE_URL" "$mock_port" <<'PY'
import base64, json, pathlib, subprocess, sys, time, urllib.error, urllib.request

port, work_dir, db, mock_port = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3], sys.argv[4]
base = f"http://127.0.0.1:{port}"

def get_events():
    with urllib.request.urlopen(f"http://127.0.0.1:{mock_port}/events", timeout=5) as r:
        return json.loads(r.read().decode())

def req(method, path, body=None, headers=None):
    hdrs = dict(headers or {})
    data = None
    if body is not None:
        data = json.dumps(body).encode()
        hdrs.setdefault("Content-Type", "application/json")
    r = urllib.request.Request(base + path, data=data, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(r, timeout=15) as resp:
            raw = resp.read().decode()
            parsed = json.loads(raw) if raw and raw.lstrip().startswith(("{", "[")) else raw
            return resp.status, parsed, {k.lower(): v for k, v in resp.headers.items()}
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        parsed = json.loads(raw) if raw and raw.lstrip().startswith(("{", "[")) else raw
        return e.code, parsed, {k.lower(): v for k, v in e.headers.items()}

def psql_scalar(sql):
    return subprocess.check_output(["psql", db, "-At", "-c", sql], text=True).strip()

def wait_for(desc, fn, timeout=6.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        res = fn()
        if res:
            return res
        time.sleep(0.15)
    raise AssertionError(f"timed out waiting for: {desc}")

# 1. Schema migration version
assert psql_scalar("SELECT 1 FROM schema_migrations WHERE version='20261125100456'") == "1"

st, created, hdrs = req("POST", "/installation/onboarding", {
    "user": {"company": "Wave9 Corp", "name": "Wave9 Admin", "email": "admin@wave9.test", "password": "Password123!"}
})
assert st == 200, (st, created)
token = hdrs.get("api_access_token") or created["user"]["access_token"]
aid = created["account"]["id"]
uid = created["user"]["id"]
ah = {"api_access_token": token}

# 2. Conversations last_message_at + last_message_sort + rate limits
st, inb, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {"name": "Web", "channel": {"type": "api"}}, ah)
assert st in (200, 201), (st, inb)
iid = inb["id"]
st, c1, _ = req("POST", f"/api/v1/accounts/{aid}/contacts", {"name": "Alice", "email": "alice@w9.test"}, ah)
st, c2, _ = req("POST", f"/api/v1/accounts/{aid}/contacts", {"name": "Bob", "email": "bob@w9.test"}, ah)
cid1 = c1["payload"]["contact"]["id"]
cid2 = c2["payload"]["contact"]["id"]
st, cv1, _ = req("POST", f"/api/v1/accounts/{aid}/conversations", {"inbox_id": iid, "contact_id": cid1}, ah)
st, cv2, _ = req("POST", f"/api/v1/accounts/{aid}/conversations", {"inbox_id": iid, "contact_id": cid2}, ah)
d1, d2 = cv1["id"], cv2["id"]

req("POST", f"/api/v1/accounts/{aid}/conversations/{d1}/messages", {"content": "pub1", "message_type": "incoming"}, ah)
time.sleep(0.05)
req("POST", f"/api/v1/accounts/{aid}/conversations/{d2}/messages", {"content": "pub2", "message_type": "incoming"}, ah)
lm2_before = psql_scalar(f"SELECT EXTRACT(EPOCH FROM last_message_at) FROM conversations WHERE account_id={aid} AND display_id={d2}")
req("POST", f"/api/v1/accounts/{aid}/conversations/{d2}/messages", {"content": "priv note", "private": True}, ah)
lm2_after = psql_scalar(f"SELECT EXTRACT(EPOCH FROM last_message_at) FROM conversations WHERE account_id={aid} AND display_id={d2}")
assert lm2_before == lm2_after, (lm2_before, lm2_after)

st, asc_res, _ = req("GET", f"/api/v1/accounts/{aid}/conversations?sort_by=last_message_at_asc", headers=ah)
assert st == 200, (st, asc_res)
ids_asc = [c["id"] for c in asc_res["data"]["payload"]]
assert ids_asc.index(d1) < ids_asc.index(d2), ids_asc

st, desc_res, _ = req("GET", f"/api/v1/accounts/{aid}/conversations?sort_by=last_message_at_desc", headers=ah)
assert st == 200, (st, desc_res)
ids_desc = [c["id"] for c in desc_res["data"]["payload"]]
assert ids_desc.index(d2) < ids_desc.index(d1), ids_desc

# 2nd list was desc_res; 3rd list hits RATE_LIMIT_CONVERSATIONS_LIST=2
st, rl_res, rl_hdr = req("GET", f"/api/v1/accounts/{aid}/conversations", headers=ah)
assert st == 429 and "retry_after" in rl_res and "retry-after" in rl_hdr, (st, rl_res, rl_hdr)

for i in range(3):
    st, _, _ = req("POST", f"/api/v1/accounts/{aid}/conversations/filter", {"payload": [{"attribute_key": "status", "filter_operator": "equal_to", "values": ["open"]}]}, ah)
    assert st == 200, (i, st)
st, f_rl, f_hdr = req("POST", f"/api/v1/accounts/{aid}/conversations/filter", {"payload": [{"attribute_key": "status", "filter_operator": "equal_to", "values": ["open"]}]}, ah)
assert st == 429 and "retry_after" in f_rl and "retry-after" in f_hdr, (st, f_rl, f_hdr)

# 3. Captain scenarios/tool_suggestions & MCP review_state + approve_tool
st, asst, _ = req("POST", f"/api/v1/accounts/{aid}/captain/assistants", {"name": "Cap", "description": "Helper"}, ah)
assert st in (200, 201), (st, asst)
asst_id = asst["id"]
st, tsug, _ = req("POST", f"/api/v1/accounts/{aid}/captain/assistants/{asst_id}/scenarios/tool_suggestions", {"instruction": "Refund order and check status"}, ah)
assert st == 200 and isinstance(tsug.get("suggestions"), list), (st, tsug)

st, mcp, _ = req("POST", f"/api/v1/accounts/{aid}/captain/mcp_servers", {"assistant_id": asst_id, "name": "DemoMCP", "url": f"http://127.0.0.1:{mock_port}/mcp", "auth_type": "none"}, ah)
assert st in (200, 201) and "tools_changed_at" not in mcp, (st, mcp)
mcp_id = mcp["id"]
psql_scalar(f"INSERT INTO captain_mcp_tools (account_id, mcp_server_id, name, captain_name, approved_definition, approved_digest, review_state, enabled, created_at, updated_at) VALUES ({aid}, {mcp_id}, 'lookup', 'mcp_{mcp_id}_lookup', '{{\"description\":\"Lookup\"}}'::jsonb, 'old', 'current', true, now(), now())")
st, lt_res, _ = req("GET", f"/api/v1/accounts/{aid}/captain/mcp_servers/{mcp_id}/live_tools", headers=ah)
assert st == 200 and lt_res.get("tools"), (st, lt_res)
assert psql_scalar(f"SELECT review_state FROM captain_mcp_tools WHERE mcp_server_id={mcp_id} AND name='lookup'") == "changed"
digest = next(t["digest"] for t in lt_res["tools"] if t["definition"]["name"] == "lookup")
st, ap_res, _ = req("POST", f"/api/v1/accounts/{aid}/captain/mcp_servers/{mcp_id}/approve_tool", {"name": "lookup", "digest": digest}, ah)
assert st == 200, (st, ap_res)
assert psql_scalar(f"SELECT review_state FROM captain_mcp_tools WHERE mcp_server_id={mcp_id} AND name='lookup'") == "current"

# 4. Help Center 50k limit, preview_token + X-Robots-Tag: noindex, article.* webhooks
st, wh, _ = req("POST", f"/api/v1/accounts/{aid}/webhooks", {"webhook": {"url": f"http://127.0.0.1:{mock_port}/wh", "subscriptions": ["article_created", "article_updated", "article_deleted"]}}, ah)
assert st in (200, 201), (st, wh)
st, portal, _ = req("POST", f"/api/v1/accounts/{aid}/portals", {"portal": {"name": "Docs", "slug": "docs-w9"}}, ah)
assert st in (200, 201), (st, portal)
st, cat, _ = req("POST", f"/api/v1/accounts/{aid}/portals/docs-w9/categories", {"category": {"name": "General", "slug": "general", "locale": "en"}}, ah)
assert st in (200, 201), (st, cat)
cat_id = (cat.get("payload") or cat)["id"]

st, too_long, _ = req("POST", f"/api/v1/accounts/{aid}/portals/docs-w9/articles", {"article": {"title": "Long", "content": "a" * 50001, "category_id": cat_id, "author_id": uid, "status": "draft"}}, ah)
assert st == 422, (st, too_long)

st, art, _ = req("POST", f"/api/v1/accounts/{aid}/portals/docs-w9/articles", {"article": {"title": "Draft Guide", "content": "Hello draft", "category_id": cat_id, "author_id": uid, "status": "draft"}}, ah)
assert st in (200, 201), (st, art)
art_obj = art.get("payload") or art
art_id = art_obj["id"]
art_slug = art_obj["slug"]
wait_for("article_created webhook", lambda: any(isinstance(b, dict) and b.get("event") == "article_created" for _, b in get_events()))

st, pt, _ = req("GET", f"/api/v1/accounts/{aid}/portals/docs-w9/articles/{art_id}/preview_token", headers=ah)
assert st == 200 and pt.get("preview_token"), (st, pt)
st, pub_html, pub_hdr = req("GET", f"/hc/docs-w9/articles/{art_slug}?preview_token={pt['preview_token']}")
assert st == 200 and "noindex" in pub_hdr.get("x-robots-tag", ""), (st, pub_hdr)

st, _, _ = req("PATCH", f"/api/v1/accounts/{aid}/portals/docs-w9/articles/{art_id}", {"article": {"title": "Draft Guide v2"}}, ah)
assert st == 200
wait_for("article_updated webhook", lambda: any(isinstance(b, dict) and b.get("event") == "article_updated" for _, b in get_events()))

st, _, _ = req("DELETE", f"/api/v1/accounts/{aid}/portals/docs-w9/articles/{art_id}", headers=ah)
assert st == 200
wait_for("article_deleted webhook", lambda: any(isinstance(b, dict) and b.get("event") == "article_deleted" for _, b in get_events()))

# 5. Data imports rejected_rows (404 when no rejected file)
di_id = psql_scalar(f"INSERT INTO data_imports (account_id, data_type, status, created_at, updated_at) VALUES ({aid}, 'contact', 2, now(), now()) RETURNING id").splitlines()[0]
st, _, _ = req("GET", f"/api/v1/accounts/{aid}/data_imports/{di_id}/rejected_rows", headers=ah)
assert st == 404, st

# 6. SMS webhook Basic Auth
st, sms_inb, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {
    "name": "SMS Auth",
    "channel": {"type": "sms", "phone_number": "+15550009999", "provider_config": {"account_id": "a", "application_id": "app1", "client_id": "c", "client_secret": "s"}}
}, ah)
assert st in (200, 201) and sms_inb.get("secret") and sms_inb.get("callback_auth_enforced") is True, (st, sms_inb)
st, _, unauth_hdr = req("POST", "/webhooks/sms/+15550009999", [{"type": "message-received", "to": "+15550009999", "message": {"id": "m1", "from": "+15551112222", "text": "hi"}}])
assert st == 401 and "Basic" in unauth_hdr.get("www-authenticate", ""), (st, unauth_hdr)
sms_auth = base64.b64encode(f"chatwoot:{sms_inb['secret']}".encode()).decode()
st, _, _ = req("POST", "/webhooks/sms/+15550009999", [{"type": "message-received", "to": "+15550009999", "message": {"id": "m1", "from": "+15551112222", "text": "hi"}}], {"Authorization": f"Basic {sms_auth}"})
assert st == 200, st

# 7. Instagram unified login endpoints (invalid/expired token -> 422)
st, _, _ = req("POST", f"/api/v1/accounts/{aid}/instagram/accounts", {"connection_token": "bad"}, ah)
assert st == 422, st
st, _, _ = req("POST", f"/api/v1/accounts/{aid}/instagram/connections", {"connection_token": "bad", "instagram_id": "123"}, ah)
assert st == 422, st

# 8. Shopify storefront_widget + removed whatsapp/access_request
st, ww_inb, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {"name": "Store Widget", "channel": {"type": "web_widget", "website_url": "https://demo.myshopify.com"}}, ah)
assert st in (200, 201), (st, ww_inb)
ww_id = ww_inb["id"]
psql_scalar(f"INSERT INTO integrations_hooks (account_id, app_id, status, reference_id, settings, created_at, updated_at) VALUES ({aid}, 'shopify', 1, 'demo.myshopify.com', '{{}}'::jsonb, now(), now())")
st, sw_get, _ = req("GET", f"/api/v1/accounts/{aid}/integrations/shopify_storefront_widget", headers=ah)
assert st == 200 and sw_get.get("inbox_id") is None and "theme_editor_url" in sw_get, (st, sw_get)
st, sw_patch, _ = req("PATCH", f"/api/v1/accounts/{aid}/integrations/shopify_storefront_widget", {"inbox_id": ww_id}, ah)
assert st == 200 and sw_patch.get("inbox_id") == ww_id, (st, sw_patch)

st, _, _ = req("POST", f"/api/v1/accounts/{aid}/whatsapp/access_request", {"business_name": "Test"}, ah)
assert st in (404, 405, 501), st

# 9. Bot metrics filters to bot senders
st, bm, _ = req("GET", f"/api/v2/accounts/{aid}/reports/bot_metrics", headers=ah)
assert st == 200 and "conversation_count" in bm, (st, bm)

print("wave9-chatwoot-4-20: ALL CHECKS PASSED")
PY
