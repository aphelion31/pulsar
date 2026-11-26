#!/usr/bin/env bash
# Wave 5 Parity Integration Test: Installation Onboarding, Super Admin API & Platform Email Channel Migrations.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}:$PATH"
export LC_ALL=C
unset PGHOST PGHOSTADDR PGPORT PGUSER PGPASSWORD PGSERVICE PGDATABASE PGOPTIONS

work=$(mktemp -d /tmp/pulsar-wave5.XXXXXXXX)
chmod 700 "$work"
app_pid=
cleanup() {
  rc=$?
  if [[ $rc -ne 0 && -f "$work/pulsar.log" ]]; then
    echo "=== pulsar.log ===" >&2
    tail -n 60 "$work/pulsar.log" >&2 || true
  fi
  if [[ -n "$app_pid" ]]; then kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true; fi
  pg_ctl -D "$work/db" -m fast stop >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

initdb -D "$work/db" -U fixture -A trust --locale=C --encoding=UTF8 >/dev/null
pg_ctl -D "$work/db" -l "$work/postgres.log" -o "-h '' -k $work -c timezone=UTC" start >/dev/null
for f in migrations/0001_schema.sql migrations/0002_pulsar_indexes.sql migrations/0003_pulsar_widget_token_floor.sql migrations/0004_pulsar_draft_messages.sql migrations/0005_pulsar_events.sql migrations/0006_data_exports.sql; do
  psql -h "$work" -U fixture -d postgres -v ON_ERROR_STOP=1 -q -f "$f" >/dev/null
done

export DATABASE_URL="postgres://fixture@localhost/postgres?host=$work"
export STORAGE_DIR="$work/storage"
cargo build --offline --locked --quiet

port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
export FRONTEND_URL="http://127.0.0.1:$port"
export EMAIL_CHANNEL_MIGRATION="true"
BIND="127.0.0.1:$port" target/debug/pulsar >"$work/pulsar.log" 2>&1 &
app_pid=$!

for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then break; fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$port/health" >/dev/null

python3 - "$port" "$DATABASE_URL" <<'PY'
import json, subprocess, sys, urllib.error, urllib.request
port, db = sys.argv[1], sys.argv[2]
base = f"http://127.0.0.1:{port}"

def req(method, path, body=None, headers=None):
    hdrs = dict(headers or {})
    data = None
    if body is not None:
        data = json.dumps(body).encode()
        hdrs.setdefault("Content-Type", "application/json")
    r = urllib.request.Request(base + path, data=data, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(r, timeout=10) as resp:
            raw = resp.read().decode()
            return resp.status, json.loads(raw) if raw else None, dict(resp.headers)
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        return e.code, json.loads(raw) if raw else None, dict(e.headers)

# 1. Onboarding gate on empty DB -> create SuperAdmin -> locked afterward
st, ob, _ = req("GET", "/installation/onboarding", headers={"Accept": "application/json"})
assert st == 200 and ob["onboarding_needed"] is True, (st, ob)

st, created, hdrs = req("POST", "/installation/onboarding", {
    "user": {
        "company": "Wave5 HQ",
        "name": "Super Boss",
        "email": "super@example.com",
        "password": "Password123!",
    }
})
assert st == 200 and created["user"]["type"] == "SuperAdmin", (st, created)
aid = created["account"]["id"]

st, ob2, _ = req("GET", "/installation/onboarding", headers={"Accept": "application/json"})
assert st == 200 and ob2["onboarding_needed"] is False, (st, ob2)
st, _, _ = req("POST", "/installation/onboarding", {"user": {"company": "X", "name": "Y", "email": "x@y.z", "password": "Password123!"}})
assert st == 403, f"expected 403 once onboarded, got {st}"

# 2. Auth enforcement: unauth -> 401; regular user -> 403; SuperAdmin sign_in -> 200
st, _, _ = req("GET", "/super_admin/api/dashboard")
assert st == 401, st

st, signin_data, sa_hdr_raw = req("POST", "/super_admin/sign_in", {"email": "super@example.com", "password": "Password123!"})
assert st == 200 and signin_data["data"]["type"] == "SuperAdmin", (st, signin_data)
sa_hdr = {
    "access-token": sa_hdr_raw["access-token"],
    "client": sa_hdr_raw["client"],
    "uid": sa_hdr_raw["uid"],
}

st, regular_u, _ = req("POST", "/super_admin/api/users", {"user": {"name": "Plain Agent", "email": "agent@example.com", "password": "Password123!"}}, sa_hdr)
assert st == 200 and regular_u["type"] is None, (st, regular_u)
st, _, _ = req("POST", "/super_admin/api/account_users", {"account_id": aid, "user_id": regular_u["id"], "role": "agent"}, sa_hdr)
assert st == 200, st

ag_sign = urllib.request.Request(base + "/auth/sign_in", data=json.dumps({"email": "agent@example.com", "password": "Password123!"}).encode(), headers={"Content-Type": "application/json"}, method="POST")
with urllib.request.urlopen(ag_sign) as r:
    ag_hdr = {"access-token": r.headers["access-token"], "client": r.headers["client"], "uid": r.headers["uid"]}

st, _, _ = req("GET", "/super_admin/api/dashboard", headers=ag_hdr)
assert st == 403, f"expected 403 for non-SuperAdmin, got {st}"

# 3. SuperAdmin CRUD: accounts, seed, reset_cache, impersonate, app_config, installation_configs, bots, apps, banners, instance_status
st, dash, _ = req("GET", "/super_admin/api/dashboard", headers=sa_hdr)
assert st == 200 and dash["accountsCount"] >= 1, (st, dash)

st, acc2, _ = req("POST", "/super_admin/api/accounts", {"account": {"name": "Second Org", "domain": "second.example"}}, sa_hdr)
assert st == 200 and acc2["name"] == "Second Org", (st, acc2)
st, seed_r, _ = req("POST", f"/super_admin/api/accounts/{acc2['id']}/seed", headers=sa_hdr)
assert st == 200 and seed_r["ok"] is True, (st, seed_r)
st, _, _ = req("POST", f"/super_admin/api/accounts/{acc2['id']}/reset_cache", headers=sa_hdr)
assert st == 200, st

st, imp, _ = req("POST", f"/super_admin/api/users/{regular_u['id']}/impersonate", headers=sa_hdr)
assert st == 200 and imp["session"]["uid"] == "agent@example.com", (st, imp)

st, cfg, _ = req("POST", "/super_admin/api/app_config?config=captain", {"app_config": {"CAPTAIN_OPEN_AI_MODEL": "gpt-4o"}}, sa_hdr)
assert st == 200 and cfg["values"]["CAPTAIN_OPEN_AI_MODEL"] == "gpt-4o", (st, cfg)

st, ic, _ = req("POST", "/super_admin/api/installation_configs", {"name": "CUSTOM_BANNER", "value": "enabled"}, sa_hdr)
assert st == 200 and ic["value"] == "enabled", (st, ic)

st, papp, _ = req("POST", "/super_admin/api/platform_apps", {"name": "Migrator App"}, sa_hdr)
assert st == 200 and papp["access_token"], (st, papp)
st, bot, _ = req("POST", "/super_admin/api/agent_bots", {"name": "SA Bot", "outgoing_url": "https://bot.example/hook"}, sa_hdr)
assert st == 200 and bot["name"] == "SA Bot", (st, bot)
st, ban, _ = req("POST", "/super_admin/api/platform_banners", {"title": "Notice", "banner_message": "Maintenance Sunday"}, sa_hdr)
assert st == 200 and ban["banner_message"] == "Maintenance Sunday", (st, ban)
st, ist, _ = req("GET", "/super_admin/api/instance_status", headers=sa_hdr)
assert st == 200 and ist["metrics"]["Postgres alive"] == "true", (st, ist)

# 4. Platform API email_channel_migrations
subprocess.run(
    ["psql", db, "-v", "ON_ERROR_STOP=1", "-q", "-c",
     f"INSERT INTO platform_app_permissibles (platform_app_id, permissible_type, permissible_id, created_at, updated_at) VALUES ({papp['id']}, 'Account', {aid}, now(), now());"],
    check=True,
)
plat_hdr = {"api_access_token": papp["access_token"]}
st, mig, _ = req("POST", f"/platform/api/v1/accounts/{aid}/email_channel_migrations", {
    "migrations": [
        {"email": "support@gmail.example", "provider": "google", "inbox_name": "Gmail Migrated"},
        {"email": "help@outlook.example", "provider": "microsoft"},
    ]
}, plat_hdr)
assert st == 200 and len(mig["results"]) == 2 and mig["results"][0]["status"] == "success" and mig["results"][1]["status"] == "success", (st, mig)

# 5. Final Gap Parity: Data Exports, Billing/Enterprise Webhooks, Firecrawl Webhook, Slack Uploads, WA Call Hours, User Diagnostics
st, exp, _ = req("POST", f"/api/v1/accounts/{aid}/data_exports", {"data_type": "contacts", "name": "All Contacts"}, sa_hdr)
assert st == 200 and exp["status"] == "completed", (st, exp)
eid = exp["id"]
st, exp_dl, _ = req("GET", f"/api/v1/accounts/{aid}/data_exports/{eid}/download", headers=sa_hdr)
assert st == 200 and exp_dl["download_url"], (st, exp_dl)
st, exp_re, _ = req("POST", f"/api/v1/accounts/{aid}/data_exports/{eid}/rerun", headers=sa_hdr)
assert st == 200 and exp_re["id"] != eid, (st, exp_re)

st, bs, _ = req("GET", f"/enterprise/api/v1/accounts/{aid}/billing_summary", headers=sa_hdr)
assert st == 200 and bs["plan"] == "Open Source Enterprise", (st, bs)
st, leg, _ = req("GET", f"/enterprise/api/v1/accounts/{aid}/legal_compliance", headers=sa_hdr)
assert st == 200 and "dpa_signing_url" in leg, (st, leg)
st, top, _ = req("GET", f"/enterprise/api/v1/accounts/{aid}/topup_options", headers=sa_hdr)
assert st == 200 and len(top["options"]) >= 1, (st, top)
st, sw, _ = req("POST", "/enterprise/webhooks/stripe", {"type": "customer.subscription.updated", "data": {"object": {"metadata": {"account_id": str(aid)}}}})
assert st == 200, st

st, cap_asst, _ = req("POST", f"/api/v1/accounts/{aid}/captain/assistants", {"assistant": {"name": "Firecrawl Cap"}}, sa_hdr)
assert st == 200, (st, cap_asst)
cap_id = cap_asst["id"]
import hashlib
fc_tok = hashlib.sha256(f"pulsar-dev-secret-change-me:firecrawl:{cap_id}:{aid}".encode()).hexdigest()
st, _, _ = req("POST", "/enterprise/webhooks/firecrawl", {"assistant_id": cap_id, "token": "wrong", "type": "crawl.page"})
assert st == 401, st
st, _, _ = req("POST", "/enterprise/webhooks/firecrawl", {
    "assistant_id": cap_id,
    "token": fc_tok,
    "type": "crawl.page",
    "data": [{"url": "https://docs.example/fc", "markdown": "# Firecrawl Guide", "metadata": {"title": "FC Guide"}}],
})
assert st == 200, st

st, diag, _ = req("GET", f"/super_admin/api/user_diagnostics?user_query=agent@example.com", headers=sa_hdr)
assert st == 200 and diag["user"]["email"] == "agent@example.com", (st, diag)

# Slack uploads redirect (no-redirect check via opener)
class NoRedir(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl): return None
op = urllib.request.build_opener(NoRedir())
try:
    op.open(base + "/slack_uploads?sender_type=agent")
except urllib.error.HTTPError as e:
    assert e.code in (302, 307) and "/integrations/slack/agent.png" in e.headers.get("Location", ""), (e.code, dict(e.headers))

# WhatsApp call hours + callback permission on seeded WhatsApp inbox
subprocess.run(
    ["psql", db, "-v", "ON_ERROR_STOP=1", "-q", "-c",
     f"INSERT INTO channel_whatsapp (id, account_id, phone_number, provider, provider_config, created_at, updated_at) VALUES (99, {aid}, '+14155559999', 'whatsapp_cloud', '{{\"voice_enabled\": true}}'::jsonb, now(), now()); INSERT INTO inboxes (id, channel_id, account_id, name, channel_type, created_at, updated_at) VALUES (99, 99, {aid}, 'WA Voice', 'Channel::Whatsapp', now(), now());"],
    check=True,
)
st, wch, _ = req("GET", f"/api/v1/accounts/{aid}/inboxes/99/whatsapp_call_hours", headers=sa_hdr)
assert st == 200 and wch["matches_whatsapp"] is True, (st, wch)
st, _, _ = req("PUT", f"/api/v1/accounts/{aid}/inboxes/99/whatsapp_call_hours", {"call_hours": {"timezone": "UTC"}}, sa_hdr)
assert st == 200, st
st, _, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes/99/set_whatsapp_callback_permission", {"callback_permission_enabled": True}, sa_hdr)
assert st == 200, st

print("WAVE 5 SUPER ADMIN + ONBOARDING + PLATFORM MIGRATIONS PASSED")
PY
