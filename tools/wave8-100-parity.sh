#!/usr/bin/env bash
# Wave 8 100% Self-Hosted Parity Verification:
# - Derived VAPID public key in GET /api + /sw.js push service worker + endpoint-aware DELETE notification_subscriptions
# - Automation rule "add_sla" + PATCH sla_policy_id + applied_slas + conversation "sla_policy_id" & "muted" (single + batched list)
# - DELETE /contacts/{id}/avatar ActiveStorage cleanup + 404 unknown contact
# - Per-account widget transcript rate limit (429)
# - Paginated Reports v2 drilldown across conversation/message/event metrics
# - Headless Brave/Playwright UI smoke on /reports (drilldown + matrix/FRT/outgoing tabs) & /profile/settings
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}:$PATH"
export LC_ALL=C
unset PGHOST PGHOSTADDR PGPORT PGUSER PGPASSWORD PGSERVICE PGDATABASE PGOPTIONS

work=$(mktemp -d /tmp/pulsar-wave8.XXXXXXXX)
chmod 700 "$work"
app_pid=
cleanup() {
  rc=$?
  if [[ $rc -ne 0 && -f "$work/pulsar.log" ]]; then
    echo "=== pulsar.log ===" >&2
    tail -n 80 "$work/pulsar.log" >&2 || true
  fi
  if [[ -n "$app_pid" ]]; then kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true; fi
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

port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')

export FRONTEND_URL="http://127.0.0.1:$port"
export PULSAR_LOG_MAIL="1"
export PULSAR_WIDGET_TRANSCRIPT_LIMIT="2"
export VAPID_PRIVATE_KEY="ERERERERERERERERERERERERERERERERERERERERERE"
export VAPID_SUBJECT="mailto:ops@wave8.test"
BIND="127.0.0.1:$port" target/debug/pulsar >"$work/pulsar.log" 2>&1 &
app_pid=$!

for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then break; fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$port/health" >/dev/null

python3 - "$port" "$work" "$DATABASE_URL" <<'PY'
import json, pathlib, subprocess, sys, time, urllib.error, urllib.request

port, work_dir, db = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3]
base = f"http://127.0.0.1:{port}"

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

# 1. Service Worker /sw.js, derived VAPID public key & DELETE subscription by endpoint
st, info, _ = req("GET", "/api")
assert st == 200 and isinstance(info.get("vapid_public_key"), str) and len(info["vapid_public_key"]) >= 80, (st, info)
st, sw_js, _ = req("GET", "/sw.js")
assert st == 200 and "push" in sw_js and "notificationclick" in sw_js, st

st, created, hdrs = req("POST", "/installation/onboarding", {
    "user": {"company": "Wave8 Corp", "name": "Wave8 Admin", "email": "admin@wave8.test", "password": "Password123!"}
})
assert st == 200, (st, created)
aid = created["account"]["id"]
uid = created["user"]["id"]
sa_hdr = {"access-token": hdrs["access-token"], "client": hdrs["client"], "uid": hdrs["uid"]}

ep = "https://push.example.test/sub-wave8"
st, _, _ = req("POST", "/api/v1/notification_subscriptions", {
    "notification_subscription": {
        "subscription_type": "browser_push",
        "subscription_attributes": {"endpoint": ep, "keys": {"p256dh": "k", "auth": "a"}},
    }
}, sa_hdr)
assert st == 200, st
assert psql_scalar(f"SELECT count(*) FROM notification_subscriptions WHERE user_id = {uid}") == "1"
st, _, _ = req("DELETE", "/api/v1/notification_subscriptions", {"push_token": ep}, sa_hdr)
assert st == 200, st
assert psql_scalar(f"SELECT count(*) FROM notification_subscriptions WHERE user_id = {uid}") == "0"
print("  [ok] /sw.js, /api vapid_public_key & DELETE /notification_subscriptions by endpoint")

# 2. SLA policy + add_sla automation + conversation sla_policy_id & muted state
st, sla_res, _ = req("POST", f"/api/v1/accounts/{aid}/sla_policies", {
    "sla_policy": {
        "name": "Enterprise FRT SLA",
        "description": "10 min FRT",
        "first_response_time_threshold": 600,
    }
}, sa_hdr)
assert st in (200, 201), (st, sla_res)
sla_id = sla_res["payload"]["id"]

st, rule_res, _ = req("POST", f"/api/v1/accounts/{aid}/automation_rules", {
    "name": "Attach Enterprise SLA",
    "event_name": "conversation_created",
    "conditions": [{"attribute_key": "status", "filter_operator": "equal_to", "values": ["open"]}],
    "actions": [{"action_name": "add_sla", "action_params": [sla_id]}],
    "active": True,
}, sa_hdr)
assert st in (200, 201), (st, rule_res)

st, wg_inbox, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {
    "name": "Wave8 Widget",
    "channel": {"type": "web_widget", "website_url": "https://wave8.test", "hmac_mandatory": False}
}, sa_hdr)
assert st in (200, 201), (st, wg_inbox)
wg_token = wg_inbox["website_token"]

st, lbl_bug, _ = req("POST", f"/api/v1/accounts/{aid}/labels", {"title": "urgent", "color": "#ff0000"}, sa_hdr)
assert st in (200, 201), (st, lbl_bug)

st, c1, _ = req("POST", f"/api/v1/accounts/{aid}/contacts", {"name": "Dana Customer", "email": "dana@c.test"}, sa_hdr)
cid = c1["payload"]["contact"]["id"]
st, conv, _ = req("POST", f"/api/v1/accounts/{aid}/conversations", {"inbox_id": wg_inbox["id"], "contact_id": cid}, sa_hdr)
assert st in (200, 201), (st, conv)
did = conv["id"]

# Wait for async automation rule add_sla
wait_for("automation add_sla attaches sla_policy_id", lambda: (
    lambda s, c, _: c if s == 200 and c.get("sla_policy_id") == sla_id else None
)(*req("GET", f"/api/v1/accounts/{aid}/conversations/{did}", headers=sa_hdr)))
assert psql_scalar(f"SELECT count(*) FROM applied_slas WHERE sla_policy_id = {sla_id}") == "1"

# Mute -> verifies muted=true in single + batched list
st, _, _ = req("POST", f"/api/v1/accounts/{aid}/conversations/{did}/mute", {}, sa_hdr)
assert st == 200, st
st, single, _ = req("GET", f"/api/v1/accounts/{aid}/conversations/{did}", headers=sa_hdr)
assert st == 200 and single["muted"] is True and single["sla_policy_id"] == sla_id, single
st, batched, _ = req("GET", f"/api/v1/accounts/{aid}/conversations?status=all&assignee_type=all", headers=sa_hdr)
b_item = next(x for x in batched["data"]["payload"] if x["id"] == did)
assert b_item["muted"] is True and b_item["sla_policy_id"] == sla_id, b_item
req("POST", f"/api/v1/accounts/{aid}/conversations/{did}/unmute", {}, sa_hdr)
req("PATCH", f"/api/v1/accounts/{aid}/conversations/{did}", {"sla_policy_id": None}, sa_hdr)
st, single_clr, _ = req("GET", f"/api/v1/accounts/{aid}/conversations/{did}", headers=sa_hdr)
assert st == 200 and single_clr["muted"] is False and single_clr["sla_policy_id"] is None, single_clr
print("  [ok] Automation add_sla, PATCH sla_policy_id, and real muted state in single + batch")

# 3. Contact avatar delete + 404 on missing contact
subprocess.run(["psql", db, "-v", "ON_ERROR_STOP=1", "-q", "-c", f"""
    INSERT INTO active_storage_blobs (key, filename, content_type, metadata, byte_size, checksum, created_at, service_name)
    VALUES ('w8avblobkey1234567890123456', 'avatar.png', 'image/png', '{{}}', 4, 'AAAA', now(), 'local');
    INSERT INTO active_storage_attachments (name, record_type, record_id, blob_id, created_at)
    SELECT 'avatar', 'Contact', {cid}, id, now() FROM active_storage_blobs WHERE key = 'w8avblobkey1234567890123456';
"""], check=True)
st, c_before, _ = req("GET", f"/api/v1/accounts/{aid}/contacts/{cid}", headers=sa_hdr)
assert st == 200 and c_before["payload"]["thumbnail"] != "", c_before
st, _, _ = req("DELETE", f"/api/v1/accounts/{aid}/contacts/{cid}/avatar", headers=sa_hdr)
assert st == 200, st
st, c_after, _ = req("GET", f"/api/v1/accounts/{aid}/contacts/{cid}", headers=sa_hdr)
assert st == 200 and c_after["payload"]["thumbnail"] == "", c_after
st, _, _ = req("DELETE", f"/api/v1/accounts/{aid}/contacts/999999/avatar", headers=sa_hdr)
assert st == 404, st
print("  [ok] Contact DELETE /avatar clears ActiveStorage + 404 missing")

# 4. Widget transcript per-account rate limit (429 after PULSAR_WIDGET_TRANSCRIPT_LIMIT=2)
st, wcfg, _ = req("POST", f"/api/v1/widget/config?website_token={wg_token}", {})
wh = {"X-Auth-Token": wcfg["website_channel_config"]["auth_token"]}
st, wconv, _ = req("POST", f"/api/v1/widget/conversations?website_token={wg_token}", {
    "contact": {"name": "Visitor", "email": "visitor@wave8.test"},
    "message": {"content": "Need transcript"}
}, wh)
assert st == 200, (st, wconv)
assert req("POST", f"/api/v1/widget/conversations/transcript?website_token={wg_token}", {}, wh)[0] == 200
assert req("POST", f"/api/v1/widget/conversations/transcript?website_token={wg_token}", {}, wh)[0] == 200
assert req("POST", f"/api/v1/widget/conversations/transcript?website_token={wg_token}", {}, wh)[0] == 429
print("  [ok] Widget transcript per-account rate limit returns 429")

# 5. Paginated Reports v2 drilldown across conversation/message/event metrics
req("POST", f"/api/v1/accounts/{aid}/conversations/{did}/labels", {"labels": ["urgent"]}, sa_hdr)
req("POST", f"/api/v1/accounts/{aid}/conversations/{did}/messages", {"content": "Inbound question", "message_type": "incoming"}, sa_hdr)
req("POST", f"/api/v1/accounts/{aid}/conversations/{did}/messages", {"content": "Agent reply", "message_type": "outgoing"}, sa_hdr)

st, dd_conv, _ = req("GET", f"/api/v2/accounts/{aid}/reports/drilldown?metric=conversations_count&per_page=10&page=1", headers=sa_hdr)
assert st == 200 and dd_conv["meta"]["total_count"] >= 2 and dd_conv["payload"][0]["conversation"]["display_id"] > 0, (st, dd_conv)
st, dd_msg, _ = req("GET", f"/api/v2/accounts/{aid}/reports/drilldown?metric=outgoing_messages_count&per_page=10&page=1", headers=sa_hdr)
assert st == 200 and dd_msg["meta"]["total_count"] >= 1 and dd_msg["payload"][0]["message"]["content"] == "Agent reply", (st, dd_msg)
dd_frt = wait_for("drilldown avg_first_response_time", lambda: (
    lambda s, d, _: d if s == 200 and d["meta"]["total_count"] >= 1 else None
)(*req("GET", f"/api/v2/accounts/{aid}/reports/drilldown?metric=avg_first_response_time", headers=sa_hdr)))
assert dd_frt["payload"][0]["record_type"] == "message" and dd_frt["payload"][0]["metric_value"] is not None, dd_frt
print("  [ok] Reports v2 drilldown across conversation, message & event metrics")

(work_dir / "session.json").write_text(json.dumps({
    "base": base,
    "accountId": aid,
    "email": "admin@wave8.test",
    "password": "Password123!",
}))
print("ALL WAVE8 API TESTS PASSED!")
PY

if [[ -d /Users/beyondb/.jcode/scratch/node_modules/playwright && -d "/Applications/Brave Browser.app" ]]; then
  echo "Running headless Brave/Playwright UI smoke on /reports and /profile/settings..."
  NODE_PATH=/Users/beyondb/.jcode/scratch/node_modules node - "$work/session.json" <<'JS'
const { chromium } = require("playwright");
const fs = require("node:fs");
const sess = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));

(async () => {
  const browser = await chromium.launch({
    executablePath: "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
    headless: true,
    args: ["--no-sandbox"],
  });
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(`console.error: ${m.text()}`);
  });

  await page.goto(`${sess.base}/app/login`, { waitUntil: "networkidle" });
  await page.getByLabel(/email/i).fill(sess.email);
  await page.getByLabel(/password/i).fill(sess.password);
  await page.getByRole("button", { name: /login|sign in/i }).click();
  await page.waitForURL(new RegExp(`/app/accounts/${sess.accountId}`), { timeout: 10000 });

  await page.goto(`${sess.base}/app/accounts/${sess.accountId}/reports`, { waitUntil: "networkidle" });
  await page.getByText("Metric Drilldown Records").waitFor({ state: "visible", timeout: 10000 });
  await page.getByText("Dana Customer").waitFor({ state: "visible", timeout: 10000 });
  await page.getByRole("button", { name: /FRT Distribution/i }).click();
  await page.getByText("Channel::WebWidget").waitFor({ state: "visible", timeout: 10000 });
  await page.getByRole("button", { name: /Outgoing Messages/i }).click();
  await page.getByText("Wave8 Admin").waitFor({ state: "visible", timeout: 10000 });

  await page.goto(`${sess.base}/app/accounts/${sess.accountId}/profile/settings`, { waitUntil: "networkidle" });
  await page.getByText("Access Token").first().waitFor({ state: "visible", timeout: 10000 });

  if (errors.length > 0) {
    throw new Error(`UI console/page errors detected:\n${errors.join("\n")}`);
  }
  await browser.close();
  console.log("  [ok] Headless Brave/Playwright UI smoke: /reports drilldown/tabs + /profile/settings, 0 errors");
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
JS
fi

echo "ALL WAVE8 100% PARITY CHECKS PASSED!"
