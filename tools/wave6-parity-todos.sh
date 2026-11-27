#!/usr/bin/env bash
# Wave 6 Parity Integration Test: P0, P1 & P3 Parity TODO Verification.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}:$PATH"
export LC_ALL=C
unset PGHOST PGHOSTADDR PGPORT PGUSER PGPASSWORD PGSERVICE PGDATABASE PGOPTIONS

work=$(mktemp -d /tmp/pulsar-wave6.XXXXXXXX)
chmod 700 "$work"
app_pid=
mock_pid=
cleanup() {
  rc=$?
  if [[ $rc -ne 0 && -f "$work/pulsar.log" ]]; then
    echo "=== pulsar.log ===" >&2
    tail -n 80 "$work/pulsar.log" >&2 || true
  fi
  if [[ -n "$app_pid" ]]; then kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true; fi
  if [[ -n "$mock_pid" ]]; then kill "$mock_pid" 2>/dev/null || true; wait "$mock_pid" 2>/dev/null || true; fi
  pg_ctl -D "$work/db" -m fast stop >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

initdb -D "$work/db" -U fixture -A trust --locale=C --encoding=UTF8 >/dev/null
pg_ctl -D "$work/db" -l "$work/postgres.log" -o "-h '' -k $work -c timezone=UTC" start >/dev/null
for f in migrations/0001_schema.sql migrations/0002_pulsar_indexes.sql migrations/0003_pulsar_widget_token_floor.sql migrations/0004_pulsar_draft_messages.sql migrations/0005_pulsar_events.sql migrations/0006_data_exports.sql migrations/0007_parity_extras.sql migrations/0008_chatwoot_4_20.sql; do
  psql -h "$work" -U fixture -d postgres -v ON_ERROR_STOP=1 -q -f "$f" >/dev/null
done

export DATABASE_URL="postgres://fixture@localhost/postgres?host=$work"
export STORAGE_DIR="$work/storage"
cargo build --offline --locked --quiet

mock_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')

cat >"$work/mock.py" <<'PY'
import http.server, json, pathlib, sys, threading

out_dir = pathlib.Path(sys.argv[2])
lock = threading.Lock()
events_file = out_dir / "mock_events.jsonl"

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass

    def _send(self, code, ct, data):
        if isinstance(data, str):
            data = data.encode()
        self.send_response(code)
        self.send_header("Content-Type", ct)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _record(self, kind, payload, headers=None):
        rec = {"kind": kind, "payload": payload, "headers": {k.lower(): v for k, v in (headers or {}).items()}}
        with lock:
            with open(events_file, "a") as fh:
                fh.write(json.dumps(rec) + "\n")

    def do_GET(self):
        if self.path == "/media/voice.ogg":
            self._send(200, "audio/ogg", b"OggS\x00\x02mock-audio-payload")
        else:
            self._send(404, "application/json", '{"error":"not found"}')

    def do_POST(self):
        ln = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(ln) if ln > 0 else b""
        if self.path == "/api/v1/oauth2/token":
            self._send(200, "application/json", json.dumps({"access_token": "bw-mock-token", "expires_in": 3600}))
            return
        if self.path == "/agentbot":
            body = json.loads(raw.decode())
            self._record("agentbot", body, self.headers)
            self._send(200, "application/json", '{"ok":true}')
            return
        if self.path == "/v1/chat/completions":
            body = json.loads(raw.decode())
            last = body.get("messages", [{}])[-1].get("content", "")
            reply = "[HANDOFF] Connecting you to a human agent." if "human" in last.lower() else "Hi from Captain mock AI!"
            self._record("llm", {"last": last, "reply": reply})
            self._send(200, "application/json", json.dumps({
                "choices": [{"message": {"content": reply}}],
                "usage": {"prompt_tokens": 10, "completion_tokens": 8, "total_tokens": 18}
            }))
            return
        if self.path == "/v1/audio/transcriptions":
            self._record("whisper", {"len": len(raw), "ct": self.headers.get("Content-Type", "")})
            self._send(200, "application/json", json.dumps({"text": "Hello from Whisper mock"}))
            return
        if self.path == "/fcm/send":
            body = json.loads(raw.decode())
            self._record("fcm", body)
            self._send(200, "application/json", '{"success":1}')
            return
        self._send(404, "application/json", '{"error":"unknown"}')

http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
PY

python3 "$work/mock.py" "$mock_port" "$work" >"$work/mock.log" 2>&1 &
mock_pid=$!

export FRONTEND_URL="http://127.0.0.1:$port"
export BANDWIDTH_API_URL="http://127.0.0.1:$mock_port"
export CAPTAIN_LLM_API_URL="http://127.0.0.1:$mock_port/v1"
export CAPTAIN_LLM_API_KEY="mock-key"
export OPENAI_API_URL="http://127.0.0.1:$mock_port/v1"
export OPENAI_API_KEY="mock-key"
export FCM_API_URL="http://127.0.0.1:$mock_port/fcm/send"
export FCM_SERVER_KEY="mock-fcm-key"
export PULSAR_WEBHOOK_ALLOW_PRIVATE="1"
export PULSAR_MULTI_NODE="1"
export PULSAR_CUSTOM_DOMAIN_FORCE_VERIFY="1"
export PULSAR_LOG_MAIL="1"
BIND="127.0.0.1:$port" target/debug/pulsar >"$work/pulsar.log" 2>&1 &
app_pid=$!

for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then break; fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$port/health" >/dev/null

python3 - "$port" "$mock_port" "$work" "$DATABASE_URL" <<'PY'
import hashlib, hmac, json, pathlib, subprocess, sys, time, urllib.error, urllib.parse, urllib.request, uuid

port, mock_port, work_dir, db = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3]), sys.argv[4]
base = f"http://127.0.0.1:{port}"
events_file = work_dir / "mock_events.jsonl"

def req(method, path, body=None, headers=None, raw_bytes=None):
    hdrs = dict(headers or {})
    data = raw_bytes
    if body is not None:
        data = json.dumps(body).encode()
        hdrs.setdefault("Content-Type", "application/json")
    r = urllib.request.Request(base + path, data=data, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(r, timeout=15) as resp:
            raw = resp.read().decode()
            parsed = None
            if raw and (resp.headers.get("Content-Type", "").startswith("application/json") or raw.startswith(("{", "["))):
                try:
                    parsed = json.loads(raw)
                except Exception:
                    parsed = raw
            else:
                parsed = raw
            return resp.status, parsed, {k.lower(): v for k, v in resp.headers.items()}
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        try:
            parsed = json.loads(raw) if raw else None
        except Exception:
            parsed = raw
        return e.code, parsed, {k.lower(): v for k, v in e.headers.items()}

def psql_scalar(sql):
    return subprocess.check_output(["psql", db, "-At", "-c", sql], text=True).strip()

def mock_events(kind=None):
    if not events_file.exists():
        return []
    out = []
    for line in events_file.read_text().splitlines():
        if not line.strip():
            continue
        item = json.loads(line)
        if kind is None or item["kind"] == kind:
            out.append(item)
    return out

def wait_for(desc, fn, timeout=6.0):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        res = fn()
        if res:
            return res
        last = res
        time.sleep(0.15)
    raise AssertionError(f"timed out waiting for: {desc} (last={last})")

# 0. Onboarding & Swagger/OpenAPI checks
st, created, sa_hdr_raw = req("POST", "/installation/onboarding", {
    "user": {"company": "Wave6 Corp", "name": "Wave6 Admin", "email": "admin@wave6.test", "password": "Password123!"}
})
assert st == 200 and created["user"]["type"] == "SuperAdmin", (st, created)
aid = created["account"]["id"]
uid = created["user"]["id"]
sa_hdr = {
    "access-token": sa_hdr_raw["access-token"],
    "client": sa_hdr_raw["client"],
    "uid": sa_hdr_raw["uid"],
}

st, sw_html, _ = req("GET", "/swagger")
assert st == 200 and "Pulsar API Documentation" in sw_html, st
st, spec, _ = req("GET", "/swagger/openapi.json")
assert st == 200 and spec["openapi"] == "3.0.3" and "/auth/sso_exchange" in spec["paths"], (st, spec)
st, info, _ = req("GET", "/api")
assert st == 200 and info["app"] == "pulsar", (st, info)
print("  [ok] Swagger, OpenAPI & onboarding")

# 1. AgentBot listener + WebWidget + ReportingEvents + Channel Summary
st, wg_inbox, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {
    "name": "Widget Support",
    "channel": {"type": "web_widget", "website_url": "https://shop.wave6.test", "hmac_mandatory": False}
}, sa_hdr)
assert st in (200, 201), (st, wg_inbox)
wg_inbox_id = wg_inbox["id"]
wg_token = wg_inbox["website_token"]

st, bot, _ = req("POST", f"/api/v1/accounts/{aid}/agent_bots", {
    "name": "EchoBot",
    "outgoing_url": f"http://127.0.0.1:{mock_port}/agentbot",
    "bot_type": "webhook"
}, sa_hdr)
assert st in (200, 201), (st, bot)
st, _, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes/{wg_inbox_id}/set_agent_bot", {"agent_bot": bot["id"]}, sa_hdr)
assert st in (200, 204), st

st, wcfg, _ = req("POST", f"/api/v1/widget/config?website_token={wg_token}", {})
assert st == 200, (st, wcfg)
w_auth = wcfg["website_channel_config"]["auth_token"]
wh = {"X-Auth-Token": w_auth}
st, conv, _ = req("POST", f"/api/v1/widget/conversations?website_token={wg_token}", {
    "contact": {"name": "Alice Visitor", "email": "alice@visitor.test"},
    "message": {"content": "Where is my order?"}
}, wh)
assert st == 200, (st, conv)
did = conv["id"]

ab_ev = wait_for("agent_bot webhook delivery", lambda: [e for e in mock_events("agentbot") if e["payload"].get("event") == "message_created"])
assert "Where is my order?" in json.dumps(ab_ev[0]["payload"]), ab_ev[0]
assert ab_ev[0]["headers"].get("x-chatwoot-signature", "").startswith("sha256="), ab_ev[0]["headers"]

# Agent replies + resolves -> first_response + reply_time + conversation_resolved
st, agent_msg, _ = req("POST", f"/api/v1/accounts/{aid}/conversations/{did}/messages", {"content": "On its way!"}, sa_hdr)
assert st == 200, (st, agent_msg)
st, _, _ = req("POST", f"/api/v1/accounts/{aid}/conversations/{did}/toggle_status", {"status": "resolved"}, sa_hdr)
assert st == 200, st

wait_for(
    "reporting_events first_response + conversation_resolved",
    lambda: int(psql_scalar(f"SELECT count(*) FROM reporting_events WHERE account_id={aid} AND name IN ('first_response','conversation_resolved')")) >= 2,
)
st, ch_sum, _ = req("GET", f"/api/v2/accounts/{aid}/summary_reports/channel", headers=sa_hdr)
assert st == 200 and any(r["channel_type"] == "Channel::WebWidget" and r["resolved"] >= 1 for r in ch_sum), (st, ch_sum)
print("  [ok] agent_bots webhook, reporting_events & summary_reports/channel")

# 2. Captain listener auto-reply + [HANDOFF] + v2 bot metrics reports
st, cap_inbox, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {
    "name": "Captain API Inbox",
    "channel": {"type": "api"}
}, sa_hdr)
assert st in (200, 201), (st, cap_inbox)
cap_inbox_id = cap_inbox["id"]

st, asst, _ = req("POST", f"/api/v1/accounts/{aid}/captain/assistants", {
    "name": "Atlas",
    "description": "Helpful assistant"
}, sa_hdr)
assert st in (200, 201), (st, asst)
asst_id = asst["id"]
st, _, _ = req("POST", f"/api/v1/accounts/{aid}/captain/assistants/{asst_id}/inboxes", {"inbox_id": cap_inbox_id}, sa_hdr)
assert st in (200, 201), st

st, ct, _ = req("POST", f"/api/v1/accounts/{aid}/contacts", {"name": "Cap User", "email": "capuser@test.invalid"}, sa_hdr)
assert st in (200, 201), (st, ct)
cap_cid = ct["payload"]["contact"]["id"]
st, cap_conv, _ = req("POST", f"/api/v1/accounts/{aid}/conversations", {
    "inbox_id": cap_inbox_id,
    "contact_id": cap_cid,
    "status": "pending"
}, sa_hdr)
assert st in (200, 201), (st, cap_conv)
cap_did = cap_conv["id"]
st, cap_m1, _ = req("POST", f"/api/v1/accounts/{aid}/conversations/{cap_did}/messages", {
    "content": "Hello bot, what is status?",
    "message_type": "incoming"
}, sa_hdr)
assert st in (200, 201), (st, cap_m1)

wait_for(
    "captain auto-reply",
    lambda: any(m["message_type"] == 1 and "Captain mock AI" in (m["content"] or "") for m in req("GET", f"/api/v1/accounts/{aid}/conversations/{cap_did}/messages", headers=sa_hdr)[1]["payload"]),
)
st, _, _ = req("POST", f"/api/v1/accounts/{aid}/conversations/{cap_did}/messages", {
    "content": "Please transfer to a human agent",
    "message_type": "incoming"
}, sa_hdr)
assert st == 200, st
wait_for(
    "captain handoff to open + bot_handoff metric",
    lambda: int(psql_scalar(f"SELECT count(*) FROM reporting_events WHERE account_id={aid} AND name='conversation_bot_handoff'")) >= 1,
)
st, bot_rep, _ = req("GET", f"/api/v2/accounts/{aid}/reports?metric=bot_handoffs_count&type=account", headers=sa_hdr)
assert st == 200 and len(bot_rep) >= 1 and bot_rep[0]["value"] >= 1, (st, bot_rep)
print("  [ok] captain auto-reply, handoff & bot_handoffs_count report")

# 3. Multi-node rate limiter (pulsar_rate_limits) + PG notify channel check
for i in range(11):
    st, _, _ = req("POST", "/auth/sign_in", {"email": "throttle@wave6.test", "password": "wrong"})
assert st == 429, f"expected 429 after 10 failures under PULSAR_MULTI_NODE=1, got {st}"
assert int(psql_scalar("SELECT hits FROM pulsar_rate_limits WHERE bucket='throttle@wave6.test'")) >= 10
print("  [ok] PULSAR_MULTI_NODE postgres rate limiter")

# 4. ActionMailbox webhooks (Postmark JSON & Mailgun multipart)
st, em_inbox, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {
    "name": "Email Webhook Inbox",
    "channel": {"type": "email", "email": "inbound-hook@wave6.test"}
}, sa_hdr)
assert st in (200, 201), (st, em_inbox)
em_inbox_id = em_inbox["id"]

st, _, _ = req("POST", "/rails/action_mailbox/postmark/inbound_emails", {
    "FromFull": {"Email": "sender1@customer.test", "Name": "Postmark Sender"},
    "OriginalRecipient": "inbound-hook@wave6.test",
    "Subject": "Postmark Question",
    "TextBody": "Hello via Postmark HTTP webhook!",
    "MessageID": "pm-msg-001@customer.test"
})
assert st == 204, st

bnd = "----Wave6MailgunBoundary"
mp_body = (
    f"--{bnd}\r\nContent-Disposition: form-data; name=\"recipient\"\r\n\r\ninbound-hook@wave6.test\r\n"
    f"--{bnd}\r\nContent-Disposition: form-data; name=\"sender\"\r\n\r\nsender2@customer.test\r\n"
    f"--{bnd}\r\nContent-Disposition: form-data; name=\"subject\"\r\n\r\nMailgun Question\r\n"
    f"--{bnd}\r\nContent-Disposition: form-data; name=\"body-plain\"\r\n\r\nHello via Mailgun!\r\n"
    f"--{bnd}--\r\n"
).encode()
st, _, _ = req("POST", "/rails/action_mailbox/mailgun/inbound_emails/mime", headers={"Content-Type": f"multipart/form-data; boundary={bnd}"}, raw_bytes=mp_body)
assert st == 204, st
assert int(psql_scalar(f"SELECT count(*) FROM messages WHERE account_id={aid} AND inbox_id={em_inbox_id}")) == 2
print("  [ok] ActionMailbox inbound email webhooks")

# 5. Platform SSO one-time exchange & populated account cascade delete
st, acc2, _ = req("POST", "/super_admin/api/accounts", {"account": {"name": "Disposable Org"}}, sa_hdr)
assert st == 200, (st, acc2)
aid2 = acc2["id"]
st, papp, _ = req("POST", "/super_admin/api/platform_apps", {"name": "SSO App"}, sa_hdr)
assert st == 200, (st, papp)
subprocess.run(["psql", db, "-v", "ON_ERROR_STOP=1", "-q", "-c",
    f"INSERT INTO platform_app_permissibles (platform_app_id, permissible_type, permissible_id, created_at, updated_at) VALUES ({papp['id']}, 'User', {uid}, now(), now()), ({papp['id']}, 'Account', {aid2}, now(), now());"], check=True)
plat_hdr = {"api_access_token": papp["access_token"]}

st, sso_res, _ = req("GET", f"/platform/api/v1/users/{uid}/login", headers=plat_hdr)
assert st == 200 and sso_res["url"] and "sso_auth_token=" in sso_res["url"], (st, sso_res)
parsed_qs = urllib.parse.parse_qs(urllib.parse.urlparse(sso_res["url"]).query)
sso_tok = parsed_qs["sso_auth_token"][0]
st, ex1, ex_hdr = req("POST", "/auth/sso_exchange", {"email": "admin@wave6.test", "sso_auth_token": sso_tok})
assert st == 200 and ex_hdr.get("access-token"), (st, ex1)
st, _, _ = req("POST", "/auth/sso_exchange", {"email": "admin@wave6.test", "sso_auth_token": sso_tok})
assert st == 401, f"expected 401 replay rejection for one-time SSO token, got {st}"

# Populate aid2 with inbox + contact + conversation and cascade delete
subprocess.run(["psql", db, "-v", "ON_ERROR_STOP=1", "-q", "-c",
    f"INSERT INTO inboxes (account_id, name, channel_type, channel_id, created_at, updated_at) VALUES ({aid2}, 'Temp API', 'Channel::Api', 999999, now(), now());"], check=True)
st, _, _ = req("DELETE", f"/platform/api/v1/accounts/{aid2}", headers=plat_hdr)
assert st == 200, st
assert psql_scalar(f"SELECT count(*) FROM accounts WHERE id={aid2}") == "0"
print("  [ok] Platform SSO single-use exchange & cascade account_delete")

# 6. Widget HMAC identifier_hash contact binding
subprocess.run(["psql", db, "-v", "ON_ERROR_STOP=1", "-q", "-c",
    f"UPDATE channel_web_widgets SET hmac_mandatory = true WHERE website_token = '{wg_token}';"], check=True)
hmac_key = psql_scalar(f"SELECT hmac_token FROM channel_web_widgets WHERE website_token = '{wg_token}'")
st, _, _ = req("PATCH", f"/api/v1/widget/contact?website_token={wg_token}", {"identifier": "cust-42", "identifier_hash": "deadbeef"}, wh)
assert st == 401, f"expected 401 on bad widget HMAC, got {st}"
good_sig = hmac.new(hmac_key.encode(), b"cust-42", hashlib.sha256).hexdigest()
st, cu, _ = req("PATCH", f"/api/v1/widget/contact?website_token={wg_token}", {"identifier": "cust-42", "identifier_hash": good_sig, "name": "Alice HMAC"}, wh)
assert st == 200 and cu["has_name"] is True, (st, cu)
assert psql_scalar(f"SELECT name FROM contacts WHERE account_id={aid} AND identifier='cust-42'") == "Alice HMAC"
assert psql_scalar(f"SELECT count(*) FROM contact_inboxes WHERE inbox_id={wg_inbox_id} AND hmac_verified=true") == "1"
print("  [ok] Widget contact HMAC verification")

# 7. Bandwidth SMS inbound media download + external_error + Whisper transcription
st, sms_inbox, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {
    "name": "BW SMS",
    "channel": {
        "type": "sms",
        "phone_number": "+15550001111",
        "provider_config": {"account_id": "bw-acct", "application_id": "bw-app", "client_id": "bw-cid", "client_secret": "bw-sec"}
    }
}, sa_hdr)
assert st in (200, 201), (st, sms_inbox)
sms_inbox_id = sms_inbox["id"]
import base64
sms_auth = base64.b64encode(f"chatwoot:{sms_inbox['secret']}".encode()).decode()

st, _, _ = req("POST", "/webhooks/sms/+15550001111", [{
    "type": "message-received",
    "to": "+15550001111",
    "message": {
        "id": "bw-msg-1",
        "from": "+15559990000",
        "text": "Voice note attached",
        "media": [f"http://127.0.0.1:{mock_port}/media/voice.ogg"]
    }
}], {"Authorization": f"Basic {sms_auth}"})
assert st == 200, st
sms_mid = int(psql_scalar(f"SELECT id FROM messages WHERE account_id={aid} AND inbox_id={sms_inbox_id} ORDER BY id DESC LIMIT 1"))
sms_conv_did = int(psql_scalar(f"SELECT c.display_id FROM messages m JOIN conversations c ON c.id=m.conversation_id WHERE m.id={sms_mid}"))
assert int(psql_scalar(f"SELECT count(*) FROM attachments WHERE message_id={sms_mid}")) == 1

cap_api_mid = cap_m1["id"]
st, _, _ = req("PATCH", f"/api/v1/accounts/{aid}/conversations/{cap_did}/messages/{cap_api_mid}", {"status": "failed", "external_error": "Carrier rejected"}, sa_hdr)
assert st == 200, st
st, msgs_list, _ = req("GET", f"/api/v1/accounts/{aid}/conversations/{cap_did}/messages", headers=sa_hdr)
assert st == 200 and any(m["id"] == cap_api_mid and m.get("external_error") == "Carrier rejected" for m in msgs_list["payload"]), msgs_list

st, tr, _ = req("POST", f"/api/v1/accounts/{aid}/conversations/{sms_conv_did}/messages/{sms_mid}/transcribe", {}, sa_hdr)
assert st == 200 and tr["transcribed_text"] == "Hello from Whisper mock", (st, tr)
print("  [ok] Bandwidth SMS media SafeFetch, external_error & Whisper transcription")

# 8. Custom Domains create + verify + delete & Push Notification FCM check
st, cd_create, _ = req("POST", f"/api/v1/accounts/{aid}/custom_domains", {"domain": "support.wave6.test"}, sa_hdr)
assert st == 200 and cd_create["payload"]["status"] == "pending", (st, cd_create)
cd_id = cd_create["payload"]["id"]
st, cd_ver, _ = req("POST", f"/api/v1/accounts/{aid}/custom_domains/{cd_id}/verify", {}, sa_hdr)
assert st == 200 and cd_ver["payload"]["verified"] is True, (st, cd_ver)
st, cd_list, _ = req("GET", f"/api/v1/accounts/{aid}/custom_domains", headers=sa_hdr)
assert st == 200 and len(cd_list["payload"]) == 1, (st, cd_list)
st, _, _ = req("DELETE", f"/api/v1/accounts/{aid}/custom_domains/{cd_id}", headers=sa_hdr)
assert st == 204, st

# Agent invite email reset_password_token + self-assignment skip + FCM push delivery
st, inv_ag, _ = req("POST", f"/api/v1/accounts/{aid}/agents", {
    "email": "invited@wave6.test",
    "name": "Invited Agent",
    "role": "agent"
}, sa_hdr)
assert st in (200, 201), (st, inv_ag)
assert int(psql_scalar("SELECT count(*) FROM users WHERE lower(email)='invited@wave6.test' AND reset_password_token IS NOT NULL")) == 1

st, _, _ = req("POST", f"/api/v1/accounts/{aid}/conversations/{sms_conv_did}/assignments", {"assignee_id": uid}, sa_hdr)
assert st == 200, st
time.sleep(0.3)
assert int(psql_scalar(f"SELECT count(*) FROM notifications WHERE user_id={uid} AND notification_type=2")) == 0

st, _, _ = req("POST", "/api/v1/notification_subscriptions", {
    "notification_subscription": {
        "subscription_type": "fcm",
        "subscription_attributes": {"push_token": "fcm-device-tok-1"}
    }
}, sa_hdr)
assert st == 200, st
st, _, _ = req("POST", f"/api/v1/accounts/{aid}/conversations/{sms_conv_did}/messages", {
    "content": "Customer reply triggers FCM push",
    "message_type": "incoming"
}, sa_hdr)
assert st == 200, st
fcm_ev = wait_for("FCM push notification delivery", lambda: mock_events("fcm"))
assert fcm_ev[0]["payload"]["to"] == "fcm-device-tok-1" and fcm_ev[0]["payload"]["notification"]["title"], fcm_ev[0]
print("  [ok] Custom Domains verify, agent invite token, self-assign skip & FCM push delivery")

print("ALL WAVE6 PARITY TESTS PASSED!")
PY
