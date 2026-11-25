#!/usr/bin/env bash
# Wave 4 Parity Integration Test: Captain AI, Conversation Monitors, Voice/Calls & Enterprise Helpers.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}:$PATH"
export LC_ALL=C
unset PGHOST PGHOSTADDR PGPORT PGUSER PGPASSWORD PGSERVICE PGDATABASE PGOPTIONS

work=$(mktemp -d /tmp/pulsar-wave4.XXXXXXXX)
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

target/debug/pulsar seed admin@example.com 'Password123!' 'Wave4 Org' >/dev/null
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -q <<'SQL'
INSERT INTO users (id, name, uid, email, provider, pubsub_token, confirmed_at, encrypted_password, created_at, updated_at)
VALUES (20, 'Agent User', 'agent@example.com', 'agent@example.com', 'email', 'agent-stream', now(),
        (SELECT encrypted_password FROM users WHERE id = 1), now(), now());
INSERT INTO account_users (account_id, user_id, role, created_at, updated_at)
VALUES (1, 20, 0, now(), now());
INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at)
VALUES ('User', 20, 'wave4-agent-token', now(), now());
SELECT setval('users_id_seq', (SELECT max(id) FROM users));

-- Seed contact, conversation, message, CSAT response, company, and campaign recipient
INSERT INTO inboxes (id, channel_id, account_id, name, channel_type, created_at, updated_at)
VALUES (10, 1, 1, 'Demo Web', 'Channel::WebWidget', now(), now());
SELECT setval('inboxes_id_seq', GREATEST(10, (SELECT COALESCE(MAX(id), 1) FROM inboxes)));
INSERT INTO contacts (id, account_id, name, email, phone_number, created_at, updated_at)
VALUES (10, 1, 'Caller Jane', 'jane@example.com', '+14155550199', now(), now());
SELECT setval('contacts_id_seq', GREATEST(10, (SELECT COALESCE(MAX(id), 1) FROM contacts)));
INSERT INTO contact_inboxes (id, contact_id, inbox_id, source_id, created_at, updated_at)
VALUES (10, 10, 10, 'src-10', now(), now());
SELECT setval('contact_inboxes_id_seq', GREATEST(10, (SELECT COALESCE(MAX(id), 1) FROM contact_inboxes)));
INSERT INTO conversations (id, account_id, inbox_id, contact_id, contact_inbox_id, status, display_id, uuid, last_activity_at, created_at, updated_at)
VALUES (10, 1, 10, 10, 10, 0, 10, gen_random_uuid(), now(), now(), now());
SELECT setval('conversations_id_seq', GREATEST(10, (SELECT COALESCE(MAX(id), 1) FROM conversations)));
INSERT INTO messages (id, account_id, inbox_id, conversation_id, message_type, content, content_type, sender_type, sender_id, created_at, updated_at)
VALUES (10, 1, 10, 10, 0, 'Hi, I need a refund for invoice 42 please!', 0, 'Contact', 10, now(), now());
SELECT setval('messages_id_seq', GREATEST(10, (SELECT COALESCE(MAX(id), 1) FROM messages)));
INSERT INTO csat_survey_responses (id, account_id, conversation_id, message_id, rating, feedback_message, contact_id, assigned_agent_id, created_at, updated_at)
VALUES (10, 1, 10, 10, 5, 'Great service', 10, 1, now(), now());
SELECT setval('csat_survey_responses_id_seq', GREATEST(10, (SELECT COALESCE(MAX(id), 1) FROM csat_survey_responses)));
INSERT INTO companies (id, account_id, name, domain, created_at, updated_at)
VALUES (10, 1, 'Acme Corp', 'acme.example', now(), now());
SELECT setval('companies_id_seq', GREATEST(10, (SELECT COALESCE(MAX(id), 1) FROM companies)));
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
        if p == "/doc/guide":
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"Refund policy: refunds available within 30 days.")
            return
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        if "/tool/ping" in p:
            self.wfile.write(json.dumps({"pong": True}).encode())
        elif "/companies/find" in p:
            self.wfile.write(json.dumps({"industry": "SaaS", "employees": 120}).encode())
        else:
            self.wfile.write(json.dumps({"ok": True}).encode())

    def do_POST(self):
        ln = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(ln).decode()
        p = urllib.parse.urlparse(self.path).path
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        if p.endswith("/chat/completions"):
            self.wfile.write(json.dumps({"choices": [{"message": {"content": "Happy to help with invoice 42!"}}]}).encode())
        elif p == "/mcp":
            self.wfile.write(json.dumps({"jsonrpc": "2.0", "id": 1, "result": {"tools": [{"name": "lookup_order", "description": "Find order"}]}}).encode())
        elif "/Calls.json" in p:
            self.wfile.write(json.dumps({"sid": "CA_MOCK_12345", "status": "queued"}).encode())
        elif p.endswith("/calls"):
            self.wfile.write(json.dumps({"calls": [{"id": "wacid.MOCK1"}]}).encode())
        else:
            self.wfile.write(json.dumps({"ok": True}).encode())

    def log_message(self, *args):
        pass

http.server.HTTPServer(("127.0.0.1", mock_port), H).serve_forever()
PY
mock_pid=$!

export FRONTEND_URL="http://127.0.0.1:$port"
export CAPTAIN_LLM_API_URL="http://127.0.0.1:$mock_port/v1"
export CAPTAIN_LLM_API_KEY="mock-openai-key"
export TWILIO_API_URL="http://127.0.0.1:$mock_port"
export WHATSAPP_CLOUD_BASE_URL="http://127.0.0.1:$mock_port"
export COMPANY_ENRICHMENT_API_URL="http://127.0.0.1:$mock_port"
BIND="127.0.0.1:$port" target/debug/pulsar >"$work/pulsar.log" 2>&1 &
app_pid=$!

for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then break; fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$port/health" >/dev/null

python3 - "$port" "$mock_port" <<'PY'
import json, sys, urllib.request, urllib.error, urllib.parse
port = sys.argv[1]
mock_port = sys.argv[2]
base = f"http://127.0.0.1:{port}"
mock_base = f"http://127.0.0.1:{mock_port}"

def req(method, path, body=None, headers=None, form=False):
    hdrs = dict(headers or {})
    data = None
    if body is not None:
        if form:
            data = urllib.parse.urlencode(body).encode()
            hdrs.setdefault("Content-Type", "application/x-www-form-urlencoded")
        else:
            data = json.dumps(body).encode()
            hdrs.setdefault("Content-Type", "application/json")
    r = urllib.request.Request(base + path, data=data, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(r, timeout=10) as resp:
            raw = resp.read().decode()
            try:
                return resp.status, json.loads(raw) if raw else None
            except Exception:
                return resp.status, raw
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        try:
            return e.code, json.loads(raw) if raw else None
        except Exception:
            return e.code, raw

# Sign in admin
sign_req = urllib.request.Request(
    base + "/auth/sign_in",
    data=json.dumps({"email": "admin@example.com", "password": "Password123!"}).encode(),
    headers={"Content-Type": "application/json"},
    method="POST",
)
with urllib.request.urlopen(sign_req) as r:
    admin_hdr = {
        "access-token": r.headers["access-token"],
        "client": r.headers["client"],
        "uid": r.headers["uid"],
    }
agent_hdr = {"api_access_token": "wave4-agent-token"}

# 1. Captain preferences + assistant CRUD + playground + scenarios + inboxes
st, prefs = req("GET", "/api/v1/accounts/1/captain/preferences", headers=admin_hdr)
assert st == 200 and "features" in prefs, (st, prefs)
st, prefs = req("PATCH", "/api/v1/accounts/1/captain/preferences", {"captain_models": {"assistant": "gpt-4o-mini"}}, admin_hdr)
assert st == 200 and prefs["features"]["assistant"]["selected"] == "gpt-4o-mini", prefs

st, asst = req("POST", "/api/v1/accounts/1/captain/assistants", {"assistant": {"name": "Cap Bot", "description": "Helpful"}}, admin_hdr)
assert st == 200 and asst["name"] == "Cap Bot", (st, asst)
aid = asst["id"]

st, _ = req("POST", f"/api/v1/accounts/1/captain/assistants/{aid}/inboxes", {"inbox_id": 10}, admin_hdr)
assert st == 200, st
st, _ = req("POST", f"/api/v1/accounts/1/captain/assistants/{aid}/scenarios", {"scenario": {"title": "Refunds", "instruction": "Offer help"}}, admin_hdr)
assert st == 200, st
st, pg = req("POST", f"/api/v1/accounts/1/captain/assistants/{aid}/playground", {"message_content": "Refund?"}, admin_hdr)
assert st in (200, 202) and "Happy to help" in pg["message"], (st, pg)
st, ov = req("GET", f"/api/v1/accounts/1/captain/assistants/{aid}/stats/overview", headers=admin_hdr)
assert st == 200 and "resolution_rate" in ov, (st, ov)

# 2. Documents + sync + Assistant responses + FAQ suggestions + Copilot + Custom Tool + MCP + Tasks
st, doc = req("POST", "/api/v1/accounts/1/captain/documents", {"document": {"assistant_id": aid, "name": "Refunds", "external_link": "https://docs.example/refund", "content": "30-day refunds"}}, admin_hdr)
assert st == 200 and doc["status"] == "available", (st, doc)
st, _ = req("POST", f"/api/v1/accounts/1/captain/documents/{doc['id']}/sync", headers=admin_hdr)
assert st == 202, st

st, resp = req("POST", "/api/v1/accounts/1/captain/assistant_responses", {"assistant_response": {"assistant_id": aid, "question": "Refund?", "answer": "30 days"}}, admin_hdr)
assert st == 200 and resp["status"] == "approved", (st, resp)

st, thr = req("POST", "/api/v1/accounts/1/captain/copilot_threads", {"assistant_id": aid, "conversation_id": 1, "message": "Draft a reply"}, admin_hdr)
assert st == 200 and thr["assistant_message"]["message"]["content"], (st, thr)
st, msgs = req("GET", f"/api/v1/accounts/1/captain/copilot_threads/{thr['id']}/copilot_messages", headers=admin_hdr)
assert st == 200 and len(msgs["payload"]) >= 2, (st, msgs)

st, ctool = req("POST", "/api/v1/accounts/1/captain/custom_tools", {"custom_tool": {"assistant_id": aid, "title": "Ping Tool", "endpoint_url": f"{mock_base}/tool/ping"}}, admin_hdr)
assert st == 200 and ctool["slug"] == "ping_tool", (st, ctool)
st, ctest = req("POST", "/api/v1/accounts/1/captain/custom_tools/test", {"custom_tool": {"http_method": "GET", "endpoint_url": f"{mock_base}/tool/ping"}}, admin_hdr)
assert st == 200 and ctest["status"] == 200 and "pong" in ctest["body"], (st, ctest)
st, mcp = req("POST", "/api/v1/accounts/1/captain/mcp_servers/discover", {"url": f"{mock_base}/mcp"}, admin_hdr)
assert st == 200 and mcp["tools"][0]["name"] == "lookup_order", (st, mcp)

st, rw = req("POST", "/api/v1/accounts/1/captain/tasks/rewrite", {"content": "hello", "operation": "improve"}, admin_hdr)
assert st == 200 and rw["message"] == "Happy to help with invoice 42!", (st, rw)
st, sm = req("POST", "/api/v1/accounts/1/captain/tasks/summarize", {"conversation_display_id": 1}, admin_hdr)
assert st == 200 and sm["message"] == "Happy to help with invoice 42!", (st, sm)
st, rs = req("POST", "/api/v1/accounts/1/captain/tasks/reply_suggestion", {"conversation_display_id": 1}, admin_hdr)
assert st == 200 and rs["message"] == "Happy to help with invoice 42!", (st, rs)

# 3. Monitors CRUD + preview + timeseries + conversations + pause/resume (and RBAC)
st, _ = req("POST", "/api/v1/accounts/1/monitors", {"name": "Blocked", "condition": "refund"}, agent_hdr)
assert st == 403, f"expected 403 for non-admin monitor create, got {st}"

st, prev = req("POST", "/api/v1/accounts/1/monitors/preview", {"condition": "refund"}, admin_hdr)
assert st == 202 and prev["matched_count"] >= 1, (st, prev)
st, pstat = req("GET", f"/api/v1/accounts/1/monitors/preview/{prev['token']}", headers=admin_hdr)
assert st == 200 and pstat["status"] == "complete", (st, pstat)

st, mon = req("POST", "/api/v1/accounts/1/monitors", {"name": "Refunds", "condition": "refund"}, admin_hdr)
assert st == 201 and mon["recent_count"] >= 1, (st, mon)
mid = mon["id"]
st, mlist = req("GET", "/api/v1/accounts/1/monitors", headers=admin_hdr)
assert st == 200 and len(mlist["payload"]) == 1, (st, mlist)
st, mconvs = req("GET", f"/api/v1/accounts/1/monitors/{mid}/conversations", headers=admin_hdr)
assert st == 200 and len(mconvs["payload"]) >= 1, (st, mconvs)
st, mon_p = req("PATCH", f"/api/v1/accounts/1/monitors/{mid}", {"paused": True}, admin_hdr)
assert st == 200 and mon_p["paused_at"] is not None, (st, mon_p)
st, mon_r = req("POST", f"/api/v1/accounts/1/monitors/{mid}/resume", {"mode": "continue", "collection_version": 0}, admin_hdr)
assert st == 200 and mon_r["paused_at"] is None, (st, mon_r)

# 4. Voice inbox + outbound call + conference token/join/end + Twilio TwiML + status
st, vinbox = req("POST", "/api/v1/accounts/1/inboxes", {
    "name": "Support Voice",
    "channel": {
        "type": "voice",
        "phone_number": "+14155550100",
        "account_sid": "AC_VOICE_1",
        "auth_token": "tok_voice_1",
        "api_key_sid": "SK_1",
        "api_key_secret": "secret_1",
        "twiml_app_sid": "AP_1",
        "voice_enabled": True,
    },
}, admin_hdr)
assert st == 200 and vinbox["voice_enabled"] is True, (st, vinbox)
vid = vinbox["id"]

st, tok = req("GET", f"/api/v1/accounts/1/inboxes/{vid}/conference/token", headers=admin_hdr)
assert st == 200 and tok["token"].count(".") == 2, (st, tok)

st, ccall = req("POST", "/api/v1/accounts/1/contacts/10/call", {"inbox_id": vid}, admin_hdr)
assert st == 200 and ccall["call"]["provider_call_id"] == "CA_MOCK_12345", (st, ccall)

st, calls_list = req("GET", "/api/v1/accounts/1/calls", headers=admin_hdr)
assert st == 200 and len(calls_list["payload"]) >= 1, (st, calls_list)

st, twiml = req("POST", "/twilio/voice/call/+14155550100", {"CallSid": "CA_INBOUND_1", "From": "+14155550199", "To": "+14155550100"}, form=True)
assert st == 200 and "<Conference" in str(twiml), (st, twiml)
st, _ = req("POST", "/twilio/voice/status/+14155550100", {"CallSid": "CA_INBOUND_1", "CallStatus": "completed", "CallDuration": "42"}, form=True)
assert st == 204, st

st, _ = req("PUT", f"/api/v1/accounts/1/inboxes/{vid}/call_announcements/greeting", {"enabled": True, "message": "Welcome"}, admin_hdr)
assert st == 200, st
st, _ = req("PUT", f"/api/v1/accounts/1/inboxes/{vid}/call_routing", {"ring_strategy": "sequential", "ring_step_seconds": 15, "prefer_free_agents": True, "call_cooldown_seconds": 5}, admin_hdr)
assert st == 200, st

# 5. Enterprise helpers: companies/enrichment, conversations/campaign_history, csat update, enterprise limits
st, comp = req("POST", "/api/v1/accounts/1/companies/10/enrichment", headers=admin_hdr)
assert st == 200 and comp["additional_attributes"]["enrichment"]["industry"] == "SaaS", (st, comp)

st, chist = req("GET", "/api/v1/accounts/1/conversations/1/campaign_history", headers=admin_hdr)
assert st == 200 and "payload" in chist, (st, chist)

st, csat = req("PATCH", "/api/v1/accounts/1/csat_survey_responses/10", {"csat_review_notes": "Reviewed by QA"}, admin_hdr)
assert st == 200 and csat["csat_review_notes"] == "Reviewed by QA", (st, csat)

st, lim = req("GET", "/enterprise/api/v1/accounts/1/limits", headers=admin_hdr)
assert st == 200 and lim["limits"]["conversation"]["consumed"] >= 1, (st, lim)

print("WAVE 4 CAPTAIN + MONITORS + VOICE PARITY CHECKS PASSED")
PY
