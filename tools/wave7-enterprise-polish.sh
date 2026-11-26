#!/usr/bin/env bash
# Wave 7 Enterprise & Polish Verification:
# Open Source Enterprise defaults, native RFC 8291/8292 Web Push encryption,
# batched conversation serialization, minor v2 reports, confirmation resend,
# all 57 locales (incl. 5 RTL), and Rich WYSIWYG + Markdown editor smoke.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}:$PATH"
export LC_ALL=C
unset PGHOST PGHOSTADDR PGPORT PGUSER PGPASSWORD PGSERVICE PGDATABASE PGOPTIONS

work=$(mktemp -d /tmp/pulsar-wave7.XXXXXXXX)
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
for f in migrations/0001_schema.sql migrations/0002_pulsar_indexes.sql migrations/0003_pulsar_widget_token_floor.sql migrations/0004_pulsar_draft_messages.sql migrations/0005_pulsar_events.sql migrations/0006_data_exports.sql migrations/0007_parity_extras.sql; do
  psql -h "$work" -U fixture -d postgres -v ON_ERROR_STOP=1 -q -f "$f" >/dev/null
done

export DATABASE_URL="postgres://fixture@localhost/postgres?host=$work"
export STORAGE_DIR="$work/storage"
cargo build --offline --locked --quiet

mock_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')

cat >"$work/mock.py" <<'PY'
import base64, http.server, json, pathlib, sys, threading

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

    def _record(self, kind, rec):
        with lock:
            with open(events_file, "a") as fh:
                fh.write(json.dumps({"kind": kind, **rec}) + "\n")

    def do_POST(self):
        ln = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(ln) if ln > 0 else b""
        hdrs = {k.lower(): v for k, v in self.headers.items()}
        if self.path == "/webpush/live":
            self._record("webpush_live", {"headers": hdrs, "body_b64": base64.b64encode(raw).decode(), "len": len(raw)})
            self._send(201, "text/plain", "Created")
            return
        if self.path == "/webpush/gone":
            self._record("webpush_gone", {"headers": hdrs, "len": len(raw)})
            self._send(410, "text/plain", "Gone")
            return
        self._send(404, "application/json", '{"error":"unknown"}')

http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
PY

python3 "$work/mock.py" "$mock_port" "$work" >"$work/mock.log" 2>&1 &
mock_pid=$!

# Valid P-256 test VAPID scalar & uncompressed UA public key (matches src/web_push.rs unit test vector keypair).
export FRONTEND_URL="http://127.0.0.1:$port"
export PULSAR_WEBHOOK_ALLOW_PRIVATE="1"
export PULSAR_LOG_MAIL="1"
export VAPID_PRIVATE_KEY="ERERERERERERERERERERERERERERERERERERERERERE"
export VAPID_SUBJECT="mailto:ops@wave7.test"
BIND="127.0.0.1:$port" target/debug/pulsar >"$work/pulsar.log" 2>&1 &
app_pid=$!

for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then break; fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$port/health" >/dev/null

python3 - "$port" "$mock_port" "$work" "$DATABASE_URL" <<'PY'
import base64, json, os, pathlib, subprocess, sys, time, urllib.error, urllib.request

port, mock_port, work_dir, db = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3]), sys.argv[4]
base = f"http://127.0.0.1:{port}"
events_file = work_dir / "mock_events.jsonl"

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

def mock_events(kind):
    if not events_file.exists():
        return []
    return [json.loads(l) for l in events_file.read_text().splitlines() if l.strip() and json.loads(l)["kind"] == kind]

def wait_for(desc, fn, timeout=6.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        res = fn()
        if res:
            return res
        time.sleep(0.15)
    raise AssertionError(f"timed out waiting for: {desc}")

# 1. Open Source Enterprise defaults
st, info, _ = req("GET", "/api")
assert st == 200 and info["is_enterprise"] is True and info["edition"] == "open-source-enterprise", (st, info)

st, created, hdrs = req("POST", "/installation/onboarding", {
    "user": {"company": "OSS Enterprise", "name": "Admin User", "email": "admin@wave7.test", "password": "Password123!"}
})
assert st == 200, (st, created)
aid = created["account"]["id"]
uid = created["user"]["id"]
sa_hdr = {"access-token": hdrs["access-token"], "client": hdrs["client"], "uid": hdrs["uid"]}

st, acct_res, _ = req("GET", f"/api/v1/accounts/{aid}", headers=sa_hdr)
assert st == 200, (st, acct_res)
features = acct_res["features"]
for flag in ["custom_roles", "sla", "audit_logs", "saml", "captain_integration", "conversation_monitors", "inbox_view"]:
    assert features.get(flag) is True, f"expected {flag}=True in Open Source Enterprise, got {features.get(flag)}"
assert "conversation_unread_counts" not in features
assert "stripe_integration" not in features

st, bill, _ = req("GET", f"/enterprise/api/v1/accounts/{aid}/billing_summary", headers=sa_hdr)
assert st == 200 and bill["plan"] == "Open Source Enterprise", (st, bill)

st, spa_html, _ = req("GET", "/app/")
assert st == 200 and "<html" in spa_html.lower(), st
print("  [ok] Open Source Enterprise defaults & feature flags")

# 2. Profile resend_confirmation generates fresh token on unconfirmed agent
st, unconf_ag, _ = req("POST", f"/api/v1/accounts/{aid}/agents", {
    "email": "unconf@wave7.test",
    "name": "Unconfirmed Agent",
    "role": "agent"
}, sa_hdr)
assert st in (200, 201), (st, unconf_ag)
unconf_uid = unconf_ag["id"]
subprocess.run(["psql", db, "-v", "ON_ERROR_STOP=1", "-q", "-c",
    f"UPDATE users SET confirmed_at = NULL, confirmation_token = NULL, confirmation_sent_at = NULL WHERE id = {unconf_uid}; "
    f"INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at) VALUES ('User', {unconf_uid}, 'wave7-unconf-tok', now(), now());"], check=True)
u_hdr = {"api_access_token": "wave7-unconf-tok"}
st, _, _ = req("POST", "/api/v1/profile/resend_confirmation", {}, u_hdr)
assert st == 200, st
tok = psql_scalar("SELECT confirmation_token FROM users WHERE lower(email) = 'unconf@wave7.test'")
sent_at = psql_scalar("SELECT confirmation_sent_at IS NOT NULL FROM users WHERE lower(email) = 'unconf@wave7.test'")
assert len(tok) >= 20 and sent_at == "t", (tok, sent_at)
log_txt = (work_dir / "pulsar.log").read_text()
assert f"confirmation_token={tok}" in log_txt
print("  [ok] POST /api/v1/profile/resend_confirmation")

# 3. Inbox, Labels, Team, Conversations, Batch Listing & Reports (matrix, FRT distribution, outgoing counts)
st, inbox1, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {"name": "Alpha API", "channel": {"type": "api"}}, sa_hdr)
st, inbox2, _ = req("POST", f"/api/v1/accounts/{aid}/inboxes", {"name": "Beta API", "channel": {"type": "api"}}, sa_hdr)
assert inbox1["id"] and inbox2["id"]
st, lbl_bug, _ = req("POST", f"/api/v1/accounts/{aid}/labels", {"title": "bug", "color": "#ff0000"}, sa_hdr)
st, lbl_vip, _ = req("POST", f"/api/v1/accounts/{aid}/labels", {"title": "vip", "color": "#00ff00"}, sa_hdr)
assert lbl_bug["id"] and lbl_vip["id"]
st, team, _ = req("POST", f"/api/v1/accounts/{aid}/teams", {"name": "Tier2"}, sa_hdr)
tid = team["id"]

st, c1_contact, _ = req("POST", f"/api/v1/accounts/{aid}/contacts", {"name": "Alice Contact", "email": "alice@c.test"}, sa_hdr)
cid = c1_contact["payload"]["contact"]["id"]
st, conv1, _ = req("POST", f"/api/v1/accounts/{aid}/conversations", {"inbox_id": inbox1["id"], "contact_id": cid, "team_id": tid, "assignee_id": uid}, sa_hdr)
st, conv2, _ = req("POST", f"/api/v1/accounts/{aid}/conversations", {"inbox_id": inbox1["id"], "contact_id": cid}, sa_hdr)
st, conv3, _ = req("POST", f"/api/v1/accounts/{aid}/conversations", {"inbox_id": inbox2["id"], "contact_id": cid}, sa_hdr)
did1, did2, did3 = conv1["id"], conv2["id"], conv3["id"]

req("POST", f"/api/v1/accounts/{aid}/conversations/{did1}/labels", {"labels": ["bug", "vip"]}, sa_hdr)
req("POST", f"/api/v1/accounts/{aid}/conversations/{did2}/labels", {"labels": ["bug"]}, sa_hdr)
req("POST", f"/api/v1/accounts/{aid}/conversations/{did3}/labels", {"labels": ["vip"]}, sa_hdr)

# Incoming + outgoing messages across convs -> triggers first_response + outgoing count + batch list check
req("POST", f"/api/v1/accounts/{aid}/conversations/{did1}/messages", {"content": "Need help", "message_type": "incoming"}, sa_hdr)
req("POST", f"/api/v1/accounts/{aid}/conversations/{did1}/messages", {"content": "Reply 1", "message_type": "outgoing"}, sa_hdr)
req("POST", f"/api/v1/accounts/{aid}/conversations/{did1}/messages", {"content": "Reply 2", "message_type": "outgoing"}, sa_hdr)
req("POST", f"/api/v1/accounts/{aid}/conversations/{did3}/messages", {"content": "Reply 3", "message_type": "outgoing"}, sa_hdr)

# Batched list parity vs single show
st, list_res, _ = req("GET", f"/api/v1/accounts/{aid}/conversations?assignee_type=all&status=all", headers=sa_hdr)
assert st == 200 and len(list_res["data"]["payload"]) == 3
listed_1 = next(x for x in list_res["data"]["payload"] if x["id"] == did1)
st, single_1, _ = req("GET", f"/api/v1/accounts/{aid}/conversations/{did1}", headers=sa_hdr)
assert st == 200
for k in ["id", "channel", "labels", "status", "unread_count"]:
    assert listed_1[k] == single_1[k], (k, listed_1[k], single_1[k])
assert listed_1["meta"]["team"]["id"] == tid and listed_1["meta"]["assignee"]["id"] == uid
assert listed_1["last_non_activity_message"]["content"] == single_1["last_non_activity_message"]["content"]
print("  [ok] Set-based conversation batch listing matches single show")

# Report: inbox_label_matrix
st, mat, _ = req("GET", f"/api/v2/accounts/{aid}/reports/inbox_label_matrix", headers=sa_hdr)
assert st == 200
assert [i["name"] for i in mat["inboxes"]] == ["Alpha API", "Beta API"], mat
assert [l["title"] for l in mat["labels"]] == ["bug", "vip"], mat
assert mat["matrix"] == [[2, 1], [0, 1]], mat

# Report: first_response_time_distribution (ensure first_response event exists via wait_for)
frt = wait_for("first_response reporting_event + distribution", lambda: (lambda s, d, _: d if s == 200 and "Channel::Api" in d else None)(*req("GET", f"/api/v2/accounts/{aid}/reports/first_response_time_distribution", headers=sa_hdr)))
assert frt["Channel::Api"]["0-1h"] >= 1, frt

# Report: outgoing_messages_count across all groupings
for grp, check in [
    ("agent", lambda r: len(r) == 1 and r[0]["outgoing_messages_count"] == 3),
    ("inbox", lambda r: sum(x["outgoing_messages_count"] for x in r) == 3 and len(r) == 2),
    ("team", lambda r: len(r) == 1 and r[0]["name"] == "Tier2" and r[0]["outgoing_messages_count"] == 2),
    ("label", lambda r: {x["name"]: x["outgoing_messages_count"] for x in r} == {"bug": 2, "vip": 3}),
]:
    st, oc, _ = req("GET", f"/api/v2/accounts/{aid}/reports/outgoing_messages_count?group_by={grp}", headers=sa_hdr)
    assert st == 200 and check(oc), (grp, st, oc)
print("  [ok] V2 reports: inbox_label_matrix, first_response_time_distribution, outgoing_messages_count")

# 4. Native RFC 8291 (aes128gcm) + RFC 8292 (VAPID ES256) Web Push + 410 pruning
pem = subprocess.check_output(["openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout"], text=True)
ec_txt = subprocess.check_output(["openssl", "ec", "-text", "-noout"], input=pem, text=True)
pub_hex = "".join(ec_txt.split("pub:")[1].split("ASN1 OID:")[0].split()).replace(":", "")
ua_pub_b64 = base64.urlsafe_b64encode(bytes.fromhex(pub_hex)).rstrip(b"=").decode()
auth_b64 = base64.urlsafe_b64encode(os.urandom(16)).rstrip(b"=").decode()

for ep in [f"http://127.0.0.1:{mock_port}/webpush/live", f"http://127.0.0.1:{mock_port}/webpush/gone"]:
    st, _, _ = req("POST", "/api/v1/notification_subscriptions", {
        "notification_subscription": {
            "subscription_type": "browser_push",
            "subscription_attributes": {
                "endpoint": ep,
                "keys": {"p256dh": ua_pub_b64, "auth": auth_b64},
            },
        }
    }, sa_hdr)
    assert st == 200, st

req("POST", f"/api/v1/accounts/{aid}/conversations/{did1}/messages", {"content": "Trigger encrypted web push", "message_type": "incoming"}, sa_hdr)
wp_ev = wait_for("RFC 8291 web push delivery", lambda: mock_events("webpush_live"))
ev = wp_ev[0]
assert ev["headers"].get("content-encoding") == "aes128gcm", ev["headers"]
assert ev["headers"].get("content-type") == "application/octet-stream", ev["headers"]
auth_hdr = ev["headers"].get("authorization", "")
assert auth_hdr.startswith("vapid t=") and ", k=" in auth_hdr, auth_hdr
jwt_tok = auth_hdr[len("vapid t="):].split(", k=")[0]
jwt_hdr = json.loads(base64.urlsafe_b64decode(jwt_tok.split(".")[0] + "=="))
jwt_claims = json.loads(base64.urlsafe_b64decode(jwt_tok.split(".")[1] + "=="))
assert jwt_hdr == {"typ": "JWT", "alg": "ES256"}
assert jwt_claims["aud"] == f"http://127.0.0.1:{mock_port}" and jwt_claims["sub"] == "mailto:ops@wave7.test"
raw_packet = base64.b64decode(ev["body_b64"])
assert len(raw_packet) > 86 + 17
assert int.from_bytes(raw_packet[16:20], "big") == 4096
assert raw_packet[20] == 65 and raw_packet[21] == 0x04

wait_for("410 Gone subscription deletion", lambda: psql_scalar("SELECT count(*) FROM notification_subscriptions WHERE subscription_attributes->>'endpoint' LIKE '%/webpush/gone'") == "0")
print("  [ok] Native RFC 8291/8292 Web Push + 410 subscription pruning")

# 5. Create Help Center portal for Article Editor Playwright smoke
st, portal, _ = req("POST", f"/api/v1/accounts/{aid}/portals", {
    "portal": {"name": "Wave7 Help", "slug": "wave7-help", "config": {"allowed_locales": ["en"], "default_locale": "en"}}
}, sa_hdr)
assert st in (200, 201), (st, portal)
portal_slug = portal.get("slug") or portal.get("payload", {}).get("slug") or "wave7-help"

# Write info for optional Playwright headless UI smoke
(work_dir / "session.json").write_text(json.dumps({
    "base": base,
    "accountId": aid,
    "conversationId": did1,
    "portalSlug": portal_slug,
    "email": "admin@wave7.test",
    "password": "Password123!",
}))
print("ALL WAVE7 API & CRYPTO TESTS PASSED!")
PY

if [[ -d /Users/beyondb/.jcode/scratch/node_modules/playwright && -d "/Applications/Brave Browser.app" ]]; then
  echo "Running headless Brave/Playwright UI smoke for 57 locales, RTL & RichTextEditor..."
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

  // Verify 57 locales & RTL switching in profile menu
  const profileBtn = page.locator("aside button").filter({ hasText: /Admin User/i }).first();
  await profileBtn.click();
  const langSelect = page.getByLabel("Language");
  const count = await langSelect.locator("option").count();
  if (count !== 57) throw new Error(`expected 57 locales, got ${count}`);

  await langSelect.selectOption("fa");
  await page.waitForFunction(() => document.documentElement.dir === "rtl" && document.documentElement.lang === "fa");
  // Reopen profile menu after <Outlet key={locale} /> cleanly re-renders the shell in Arabic/Persian RTL.
  await profileBtn.click();
  const respId = page.waitForResponse((r) => r.url().includes("/app/i18n/id.json") && r.status() === 200);
  await page.getByLabel("Language").selectOption("id");
  await respId;
  await page.waitForFunction(() => document.documentElement.dir === "ltr" && document.documentElement.lang === "id");
  await profileBtn.click();
  await page.getByLabel("Language").selectOption("en");
  await page.waitForFunction(() => document.documentElement.lang === "en");

  // Verify ReplyBox dual-mode Rich WYSIWYG -> Markdown sync -> Split -> Send
  await page.goto(`${sess.base}/app/accounts/${sess.accountId}/conversations/${sess.conversationId}`, { waitUntil: "networkidle" });
  const composer = page.locator('[data-editor-mode]').first();
  await composer.waitFor({ state: "visible", timeout: 10000 });
  if ((await composer.getAttribute("data-editor-mode")) !== "visual") {
    throw new Error("expected ReplyBox to default to visual WYSIWYG mode");
  }
  await page.getByRole("textbox", { name: "Reply" }).click();
  await page.keyboard.type("Visual WYSIWYG smoke");
  await page.getByRole("button", { name: "Markdown", exact: true }).click();
  if ((await composer.getAttribute("data-editor-mode")) !== "markdown") {
    throw new Error("expected switch to markdown mode");
  }
  const mdVal = await page.getByRole("textbox", { name: "Reply" }).inputValue();
  if (!mdVal.includes("Visual WYSIWYG smoke")) {
    throw new Error(`expected Visual text synced into Markdown textarea, got: ${JSON.stringify(mdVal)}`);
  }
  await page.getByRole("button", { name: "Split", exact: true }).click();
  if ((await composer.getAttribute("data-editor-mode")) !== "split") {
    throw new Error("expected switch to split mode");
  }
  await page.getByRole("button", { name: /Send/i }).first().click();
  await page.waitForFunction(() => document.body.innerText.includes("Visual WYSIWYG smoke"), { timeout: 10000 });

  // Verify Help Center Article Editor RichTextEditor
  await page.goto(`${sess.base}/app/accounts/${sess.accountId}/portals/${sess.portalSlug}/en/articles/new`, { waitUntil: "networkidle" });
  const artComposer = page.locator('[data-editor-mode]').first();
  await artComposer.waitFor({ state: "visible", timeout: 10000 });
  if ((await artComposer.getAttribute("data-editor-mode")) !== "visual") {
    throw new Error("expected article editor to default to visual mode");
  }
  await page.getByRole("button", { name: "Markdown", exact: true }).click();
  if ((await artComposer.getAttribute("data-editor-mode")) !== "markdown") {
    throw new Error("expected article editor switch to markdown mode");
  }
  await page.getByRole("button", { name: "Split", exact: true }).click();
  if ((await artComposer.getAttribute("data-editor-mode")) !== "split") {
    throw new Error("expected article editor switch to split mode");
  }
  if (errors.length > 0) {
    throw new Error(`UI console/page errors detected:\n${errors.join("\n")}`);
  }
  await browser.close();
  console.log("  [ok] Headless Brave/Playwright UI smoke: 57 locales, RTL, ReplyBox & Article RichTextEditor (Visual/Markdown/Split), 0 errors");
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
JS
fi

echo "ALL WAVE7 ENTERPRISE & POLISH CHECKS PASSED!"
