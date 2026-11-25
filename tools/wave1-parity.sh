#!/usr/bin/env bash
# Wave 1 Parity Integration Test against synthetic PostgreSQL 18 + live pulsar binary + mock HTTP upstream.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}:$PATH"
export LC_ALL=C
unset PGHOST PGHOSTADDR PGPORT PGUSER PGPASSWORD PGSERVICE PGDATABASE PGOPTIONS

work=$(mktemp -d /tmp/pulsar-wave1.XXXXXXXX)
chmod 700 "$work"
app_pid=
mock_pid=
cleanup() {
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

target/debug/pulsar seed admin@example.com 'Password123!' 'Wave1 Org' >/dev/null
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -q <<'SQL'
INSERT INTO users (id, name, uid, email, provider, pubsub_token, confirmed_at, encrypted_password, created_at, updated_at)
VALUES (20, 'Agent User', 'agent@example.com', 'agent@example.com', 'email', 'agent-stream', now(),
        (SELECT encrypted_password FROM users WHERE id = 1), now(), now());
INSERT INTO account_users (account_id, user_id, role, created_at, updated_at)
VALUES (1, 20, 0, now(), now());
INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at)
VALUES ('User', 20, 'wave1-agent-token', now(), now());
INSERT INTO accounts (id, name, created_at, updated_at) VALUES (2, 'Tenant Two', now(), now());
INSERT INTO account_users (account_id, user_id, role, created_at, updated_at) VALUES (2, 1, 1, now(), now());
SELECT setval('users_id_seq', (SELECT max(id) FROM users));
SELECT setval('accounts_id_seq', (SELECT max(id) FROM accounts));
SQL

port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
mock_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')

python3 - "$mock_port" >"$work/mock.log" 2>&1 <<'PY' &
import http.server, json, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        if "/Accounts/" in self.path and "IncomingPhoneNumbers" in self.path:
            self.wfile.write(json.dumps({"incoming_phone_numbers":[{"sid":"PN123","friendly_name":"Test","sms_url":f"http://127.0.0.1:{self.server.server_port}/twilio/callback","capabilities":{"sms":True}}]}).encode())
        elif "/Accounts/" in self.path:
            self.wfile.write(json.dumps({"sid":"AC123","friendly_name":"MockAcc","status":"active","type":"Full"}).encode())
        else:
            self.wfile.write(b"{}")
    def do_POST(self):
        ln = int(self.headers.get("Content-Length", 0))
        _ = self.rfile.read(ln)
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps({"id":"tpl_123","name":"customer_satisfaction_survey_1","sid":"HX123","success":True}).encode())
    def log_message(self, *args): pass
http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
mock_pid=$!

export FRONTEND_URL="http://127.0.0.1:$port"
export TWILIO_API_URL="http://127.0.0.1:$mock_port"
export TWILIO_CONTENT_API_URL="http://127.0.0.1:$mock_port"
export WHATSAPP_CLOUD_BASE_URL="http://127.0.0.1:$mock_port"
export PULSAR_SCHEDULER_INTERVAL_SECS=3600

BIND="127.0.0.1:$port" target/debug/pulsar >"$work/pulsar.log" 2>&1 &
app_pid=$!

for ((i=0; i<100; i++)); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
curl -fsS "http://127.0.0.1:$port/health" >/dev/null

# Run base smoke test first
BASE="http://127.0.0.1:$port" tools/smoke.sh

# Run comprehensive Wave 1 API + RBAC + tenant-isolation checks
python3 - "$port" "$DATABASE_URL" <<'PY'
import json, subprocess, sys, urllib.error, urllib.request

base = f"http://127.0.0.1:{sys.argv[1]}"
db = sys.argv[2]

def req(path, method="GET", body=None, headers=None, expect=200):
    hdrs = dict(headers or {})
    data = None
    if body is not None:
        hdrs["Content-Type"] = "application/json"
        data = json.dumps(body).encode()
    r = urllib.request.Request(base + path, data=data, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(r, timeout=5) as res:
            assert res.status == expect, (path, res.status, expect)
            raw = res.read().decode()
            ctype = res.headers.get("Content-Type", "")
            return (json.loads(raw) if "json" in ctype and raw else raw), res.headers
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        assert e.code == expect, (path, method, e.code, expect, raw)
        return raw, e.headers

# 1. Public non-API routes
robots, _ = req("/robots.txt")
assert "User-agent:" in robots
api_info, _ = req("/api")
assert api_info["app"] == "pulsar"
req("/.well-known/assetlinks.json")
req("/.well-known/apple-app-site-association")
req("/.well-known/microsoft-identity-association.json")
challenge, _ = req("/.well-known/cf-custom-hostname-challenge/abc123")
assert challenge == "abc123"
req("/survey/responses/00000000-0000-0000-0000-000000000000")

# 2. Sign in as admin (2 sessions so we can revoke one)
def login(email, pw):
    r = urllib.request.Request(
        base + "/auth/sign_in",
        data=json.dumps({"email": email, "password": pw}).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(r, timeout=5) as res:
        return {
            "access-token": res.headers.get("access-token"),
            "client": res.headers.get("client"),
            "uid": res.headers.get("uid"),
        }

admin_h = login("admin@example.com", "Password123!")
admin_h2 = login("admin@example.com", "Password123!")
agent_h = {"api_access_token": "wave1-agent-token"}

# 3. Profile sessions + trusted devices + notification_subscriptions
sessions, _ = req("/api/v1/profile/sessions", headers=admin_h)
assert len(sessions) >= 2, sessions
other = [s for s in sessions if not s["current"]][0]
cur = [s for s in sessions if s["current"]][0]
req(f"/api/v1/profile/sessions/{cur['id']}", method="DELETE", headers=admin_h, expect=422)
req(f"/api/v1/profile/sessions/{other['id']}", method="DELETE", headers=admin_h, expect=200)
req("/api/v1/profile/trusted_devices", method="DELETE", headers=admin_h, expect=200)

sub, _ = req("/api/v1/notification_subscriptions", method="POST", body={
    "notification_subscription": {"subscription_type": 1, "subscription_attributes": {"push_token": "tok-1"}}
}, headers=admin_h)
assert sub["identifier"] == "tok-1"
req("/api/v1/notification_subscriptions?push_token=tok-1", method="DELETE", headers=admin_h, expect=200)

# 4. SLA policies + applied SLAs + metrics + CSV + RBAC
sla, _ = req("/api/v1/accounts/1/sla_policies", method="POST", body={
    "sla_policy": {
        "name": "Standard SLA",
        "description": "Fast response",
        "first_response_time_threshold": 60,
        "next_response_time_threshold": 120,
        "resolution_time_threshold": 300,
        "only_during_business_hours": True,
    }
}, headers=admin_h)
sla_id = sla["payload"]["id"]
req("/api/v1/accounts/1/sla_policies", method="POST", body={"sla_policy": {"name": "Blocked"}}, headers=agent_h, expect=403)
sla_list, _ = req("/api/v1/accounts/1/sla_policies", headers=agent_h)
assert len(sla_list["payload"]) == 1

# Seed an applied_sla + sla_event for metrics & CSV
subprocess.check_call([
    "psql", db, "-v", "ON_ERROR_STOP=1", "-q", "-c",
    f"INSERT INTO applied_slas (account_id, sla_policy_id, conversation_id, sla_status, created_at, updated_at) "
    f"SELECT 1, {sla_id}, id, 3, now(), now() FROM conversations WHERE account_id = 1 LIMIT 1;"
    f"INSERT INTO sla_events (applied_sla_id, conversation_id, account_id, sla_policy_id, inbox_id, event_type, meta, created_at, updated_at) "
    f"SELECT a.id, a.conversation_id, 1, {sla_id}, c.inbox_id, 0, '{{}}'::jsonb, now(), now() FROM applied_slas a JOIN conversations c ON c.id = a.conversation_id LIMIT 1;"
])
metrics, _ = req("/api/v1/accounts/1/applied_slas/metrics", headers=admin_h)
assert metrics["number_of_sla_misses"] == 1 and metrics["total_applied_slas"] == 1, metrics
applied, _ = req("/api/v1/accounts/1/applied_slas", headers=admin_h)
assert len(applied["payload"]) == 1 and applied["payload"][0]["sla_events"][0]["event_type"] == "frt"
csv_text, _ = req("/api/v1/accounts/1/applied_slas/download", headers=admin_h)
assert "Standard SLA" in csv_text
req("/api/v1/accounts/1/applied_slas", headers=agent_h, expect=403)
req("/api/v1/accounts/2/applied_slas", headers=admin_h, expect=200) # account 2 empty
req(f"/api/v1/accounts/2/sla_policies/{sla_id}", headers=admin_h, expect=404) # cross-tenant 404

# 5. Capacity policies + users + inbox limits
cap, _ = req("/api/v1/accounts/1/agent_capacity_policies", method="POST", body={
    "agent_capacity_policy": {
        "name": "Tier 1",
        "exclusion_rules": {"exclude_older_than_hours": 12, "excluded_labels": ["vip"]},
    }
}, headers=admin_h)
cap_id = cap["id"]
req("/api/v1/accounts/1/agent_capacity_policies", headers=agent_h, expect=403)
req(f"/api/v1/accounts/1/agent_capacity_policies/{cap_id}/users", method="POST", body={"user_id": 20}, headers=admin_h)
users, _ = req(f"/api/v1/accounts/1/agent_capacity_policies/{cap_id}/users", headers=admin_h)
assert len(users) == 1 and users[0]["id"] == 20
inboxes, _ = req("/api/v1/accounts/1/inboxes", headers=admin_h)
inbox_id = inboxes["payload"][0]["id"]
lim, _ = req(f"/api/v1/accounts/1/agent_capacity_policies/{cap_id}/inbox_limits", method="POST", body={
    "inbox_id": inbox_id, "conversation_limit": 8
}, headers=admin_h)
req(f"/api/v1/accounts/1/agent_capacity_policies/{cap_id}/inbox_limits/{lim['id']}", method="PATCH", body={"conversation_limit": 12}, headers=admin_h)
req(f"/api/v1/accounts/1/agent_capacity_policies/{cap_id}/inbox_limits/{lim['id']}", method="DELETE", headers=admin_h, expect=204)
req(f"/api/v1/accounts/1/agent_capacity_policies/{cap_id}/users/20", method="DELETE", headers=admin_h)
req(f"/api/v1/accounts/1/agent_capacity_policies/{cap_id}", method="DELETE", headers=admin_h)

# 6. Working hours, integrations/apps, branded email layout, reporting events, SAML
wh, _ = req("/api/v1/accounts/1/working_hours", headers=admin_h)
assert isinstance(wh, list)
apps, _ = req("/api/v1/accounts/1/integrations/apps", headers=admin_h)
assert any(a["id"] == "slack" and "action" in a for a in apps["payload"]), apps
slack_app, _ = req("/api/v1/accounts/1/integrations/apps/slack", headers=admin_h)
assert slack_app["id"] == "slack"

req("/api/v1/accounts/1/branded_email_layout", method="PUT", body={"branded_email_layout": "<div>{{ content_for_layout }}</div>"}, headers=admin_h)
layout, _ = req("/api/v1/accounts/1/branded_email_layout", headers=admin_h)
assert "content_for_layout" in layout["branded_email_layout"]
req("/api/v1/accounts/1/reporting_events", headers=admin_h)

saml, _ = req("/api/v1/accounts/1/saml_settings", method="POST", body={
    "saml_settings": {
        "sso_url": "https://idp.example.com/sso",
        "idp_entity_id": "https://idp.example.com",
        "certificate": "-----BEGIN CERTIFICATE-----\nQUJD\n-----END CERTIFICATE-----",
    }
}, headers=admin_h)
assert saml["fingerprint"]
req("/api/v1/accounts/1/saml_settings", method="DELETE", headers=admin_h, expect=204)

# 7. Inbox rotate_hmac_token, working_hours on create/update, Twilio health/webhook/csat_template
web_inbox, _ = req("/api/v1/accounts/1/inboxes", method="POST", body={
    "name": "Widget Wave1",
    "channel": {"type": "web_widget", "website_url": "https://example.com"},
}, headers=admin_h)
assert len(web_inbox["working_hours"]) == 7, web_inbox
old_hmac = web_inbox["hmac_token"]
rot, _ = req(f"/api/v1/accounts/1/inboxes/{web_inbox['id']}/rotate_hmac_token", method="POST", headers=admin_h)
assert rot["hmac_token"] != old_hmac

tw_inbox, _ = req("/api/v1/accounts/1/inboxes", method="POST", body={
    "name": "Twilio WA",
    "channel": {"type": "twilio", "account_sid": "AC123", "auth_token": "tok", "phone_number": "+15550001111", "medium": "whatsapp"},
}, headers=admin_h)
tw_health, _ = req(f"/api/v1/accounts/1/inboxes/{tw_inbox['id']}/health", headers=admin_h)
assert tw_health["medium"] == "whatsapp"
req(f"/api/v1/accounts/1/inboxes/{tw_inbox['id']}/register_webhook", method="POST", headers=admin_h)
csat_c, _ = req(f"/api/v1/accounts/1/inboxes/{tw_inbox['id']}/csat_template", method="POST", body={
    "template": {"message": "Rate our support!", "button_text": "Rate", "language": "en"}
}, headers=admin_h, expect=201)
assert csat_c["template"]["content_sid"] == "HX123"
an, _ = req(f"/api/v1/accounts/1/inboxes/{tw_inbox['id']}/csat_template/analyze", method="POST", body={
    "template": {"message": "Please rate your chat"}
}, headers=admin_h)
assert an["classification"] == "UTILITY"

print("WAVE 1 PARITY CHECKS PASSED")
PY
