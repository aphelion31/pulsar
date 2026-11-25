#!/usr/bin/env bash
# Wave 2 Parity Integration Test: Facebook, Instagram, TikTok, Twitter, Google/MS OAuth,
# WhatsApp Manual Setup v2 + Embedded Signup + Access Request, Scheduled Campaigns + Analytics,
# and Help Center (sitemap.xml, .md, .png views, search, remove_locale, bulk translate).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}:$PATH"
export LC_ALL=C
unset PGHOST PGHOSTADDR PGPORT PGUSER PGPASSWORD PGSERVICE PGDATABASE PGOPTIONS

work=$(mktemp -d /tmp/pulsar-wave2.XXXXXXXX)
chmod 700 "$work"
app_pid=
mock_pid=
cleanup() {
  rc=$?
  if [[ $rc -ne 0 && -f "$work/pulsar.log" ]]; then
    echo "=== pulsar.log ===" >&2
    tail -n 60 "$work/pulsar.log" >&2 || true
    echo "=== mock.log ===" >&2
    tail -n 40 "$work/mock.log" >&2 || true
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

target/debug/pulsar seed admin@example.com 'Password123!' 'Wave2 Org' >/dev/null
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -q <<'SQL'
INSERT INTO users (id, name, uid, email, provider, pubsub_token, confirmed_at, encrypted_password, created_at, updated_at)
VALUES (20, 'Agent User', 'agent@example.com', 'agent@example.com', 'email', 'agent-stream', now(),
        (SELECT encrypted_password FROM users WHERE id = 1), now(), now());
INSERT INTO account_users (account_id, user_id, role, created_at, updated_at)
VALUES (1, 20, 0, now(), now());
INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at)
VALUES ('User', 20, 'wave2-agent-token', now(), now());
INSERT INTO accounts (id, name, created_at, updated_at) VALUES (2, 'Tenant Two', now(), now());
INSERT INTO account_users (account_id, user_id, role, created_at, updated_at) VALUES (2, 1, 1, now(), now());
SELECT setval('users_id_seq', (SELECT max(id) FROM users));
SELECT setval('accounts_id_seq', (SELECT max(id) FROM accounts));
SQL

port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
mock_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')

python3 - "$mock_port" "$port" >"$work/mock.log" 2>&1 <<'PY' &
import http.server, json, sys, urllib.parse
mock_port = int(sys.argv[1])
pulsar_port = int(sys.argv[2])

class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        p = urllib.parse.urlparse(self.path).path
        qs = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        if "/me/accounts" in p:
            self.wfile.write(json.dumps({"data": [{"id": "fb-page-1", "name": "Mock FB Page", "access_token": "fb-page-tok"}]}).encode())
        elif "/oauth/access_token" in p:
            self.wfile.write(json.dumps({"access_token": "wa-embedded-tok"}).encode())
        elif "/phone_numbers" in p:
            self.wfile.write(json.dumps({"data": [{"id": "pn-manual-1", "display_phone_number": "+1 555 000 1122", "verified_name": "Wave2 Biz"}, {"id": "pn-emb-1", "display_phone_number": "+1 555 000 3344", "verified_name": "Emb Biz"}]}).encode())
        elif p.endswith("/pn-manual-1"):
            fields = qs.get("fields", [""])[0]
            if "webhook_configuration" in fields:
                self.wfile.write(json.dumps({"webhook_configuration": {"override_callback_uri": f"http://127.0.0.1:{pulsar_port}/webhooks/whatsapp/+15550001122"}}).encode())
            else:
                self.wfile.write(json.dumps({"id": "pn-manual-1", "status": "CONNECTED", "code_verification_status": "VERIFIED"}).encode())
        elif "/message_templates" in p:
            self.wfile.write(json.dumps({"data": [{"name": "hello_world", "status": "APPROVED", "language": "en"}]}).encode())
        elif "/me/permissions" in p:
            self.wfile.write(json.dumps({"data": [{"permission": "whatsapp_business_messaging", "status": "granted"}]}).encode())
        elif "/subscribed_apps" in p:
            self.wfile.write(json.dumps({"data": [{"whatsapp_business_api_data": {"id": "app1"}}]}).encode())
        elif p.endswith("/me"):
            self.wfile.write(json.dumps({"user_id": "ig-biz-1", "username": "wave2_ig"}).encode())
        else:
            self.wfile.write(json.dumps({"id": "ok"}).encode())

    def do_POST(self):
        ln = int(self.headers.get("Content-Length", 0))
        _ = self.rfile.read(ln)
        p = urllib.parse.urlparse(self.path).path
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        if "oauth/access_token" in p or "oauth/token" in p:
            self.wfile.write(json.dumps({"access_token": "oauth-tok-123", "refresh_token": "ref-123", "user_id": "ig-biz-1", "open_id": "tt-biz-1", "email": "oauth@example.com"}).encode())
        elif p.endswith("/messages"):
            self.wfile.write(json.dumps({"messaging_product": "whatsapp", "messages": [{"id": "wamid.mock.1"}]}).encode())
        else:
            self.wfile.write(json.dumps({"success": True}).encode())

    def log_message(self, *args): pass

http.server.HTTPServer(("127.0.0.1", mock_port), H).serve_forever()
PY
mock_pid=$!

export FRONTEND_URL="http://127.0.0.1:$port"
export META_GRAPH_API_URL="http://127.0.0.1:$mock_port"
export WHATSAPP_CLOUD_BASE_URL="http://127.0.0.1:$mock_port"
export INSTAGRAM_GRAPH_URL="http://127.0.0.1:$mock_port"
export INSTAGRAM_OAUTH_TOKEN_URL="http://127.0.0.1:$mock_port/oauth/access_token"
export TIKTOK_TOKEN_URL="http://127.0.0.1:$mock_port/oauth/token"
export TWITTER_API_URL="http://127.0.0.1:$mock_port"
export FB_VERIFY_TOKEN="fb-verify-secret"
export FB_APP_SECRET="fb-app-secret"
export INSTAGRAM_APP_ID="ig-app"
export INSTAGRAM_APP_SECRET="ig-secret"
export INSTAGRAM_VERIFY_TOKEN="ig-verify-secret"
export TIKTOK_APP_ID="tt-app"
export TIKTOK_APP_SECRET="tt-secret"
export TWITTER_CONSUMER_SECRET="tw-secret"
export WHATSAPP_APP_ID="wa-app"
export WHATSAPP_APP_SECRET="wa-secret"
export GOOGLE_OAUTH_CLIENT_ID="goog-id"
export AZURE_APP_ID="ms-id"
export PULSAR_SCHEDULER_INTERVAL_SECS=1

BIND="127.0.0.1:$port" target/debug/pulsar >"$work/pulsar.log" 2>&1 &
app_pid=$!

for ((i=0; i<100; i++)); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
curl -fsS "http://127.0.0.1:$port/health" >/dev/null

python3 - "$port" "$DATABASE_URL" <<'PY'
import base64, hashlib, hmac, json, subprocess, sys, time, urllib.error, urllib.parse, urllib.request

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
            if "json" in ctype and raw:
                return json.loads(raw.decode()), res.headers
            return raw, res.headers
    except urllib.error.HTTPError as e:
        raw = e.read()
        assert e.code == expect, (path, method, e.code, expect, raw)
        ctype = e.headers.get("Content-Type", "")
        if "json" in ctype and raw:
            return json.loads(raw.decode()), e.headers
        return raw, e.headers

# Login as admin
login, _ = req("/auth/sign_in", "POST", {"email": "admin@example.com", "password": "Password123!"})
token = login["data"]["access_token"]
H_ADMIN = {"api_access_token": token}
H_AGENT = {"api_access_token": "wave2-agent-token"}

# 1. Facebook callbacks + /bot webhook + channel_facebook_pages fields in inbox_json
pages, _ = req("/api/v1/accounts/1/callbacks/facebook_pages", "POST", {"omniauth_token": "u-tok"}, H_ADMIN)
assert pages["data"]["pages"][0]["id"] == "fb-page-1", pages
fb_inbox, _ = req("/api/v1/accounts/1/callbacks/register_facebook_page", "POST", {
    "page_id": "fb-page-1",
    "inbox_name": "FB Support",
    "page_access_token": "fb-page-tok",
    "user_access_token": "u-tok",
}, H_ADMIN)
assert fb_inbox["page_id"] == "fb-page-1", fb_inbox
assert fb_inbox["reauthorization_required"] is False

# Verify /bot GET challenge
chall, _ = req("/bot?hub.mode=subscribe&hub.verify_token=fb-verify-secret&hub.challenge=CHALL123")
assert chall.decode() == "CHALL123", chall
req("/bot?hub.mode=subscribe&hub.verify_token=wrong&hub.challenge=X", expect=403)

# Signed /bot webhook POST creates conversation & message
fb_ev = json.dumps({
    "object": "page",
    "entry": [{
        "id": "fb-page-1",
        "messaging": [{
            "sender": {"id": "fb-sender-1"},
            "recipient": {"id": "fb-page-1"},
            "message": {"mid": "fb-mid-1", "text": "Hello from FB"}
        }]
    }]
}).encode()
sig = "sha256=" + hmac.new(b"fb-app-secret", fb_ev, hashlib.sha256).hexdigest()
req("/bot", "POST", raw_bytes=fb_ev, headers={"Content-Type": "application/json", "X-Hub-Signature-256": "sha256=deadbeef"}, expect=401)
req("/bot", "POST", raw_bytes=fb_ev, headers={"Content-Type": "application/json", "X-Hub-Signature-256": sig}, expect=200)

# 2. Instagram OAuth authorization -> callback -> signed webhook
ig_auth, _ = req("/api/v1/accounts/1/instagram/authorization", "POST", {}, H_ADMIN)
ig_state = urllib.parse.parse_qs(urllib.parse.urlparse(ig_auth["url"]).query)["state"][0]
_, ig_hdrs = req(f"/instagram/callback?code=igcode&state={urllib.parse.quote(ig_state)}", expect=307)
assert "/agents" in ig_hdrs["Location"], ig_hdrs["Location"]
ig_ch, _ = req("/webhooks/instagram?hub.mode=subscribe&hub.verify_token=ig-verify-secret&hub.challenge=IG99")
assert ig_ch.decode() == "IG99"
ig_ev = json.dumps({
    "object": "instagram",
    "entry": [{
        "id": "ig-biz-1",
        "messaging": [{
            "sender": {"id": "ig-user-9"},
            "recipient": {"id": "ig-biz-1"},
            "message": {"mid": "ig-mid-1", "text": "Hi IG"}
        }]
    }]
}).encode()
ig_sig = "sha256=" + hmac.new(b"ig-app-secret" if False else b"ig-secret", ig_ev, hashlib.sha256).hexdigest()
req("/webhooks/instagram", "POST", raw_bytes=ig_ev, headers={"Content-Type": "application/json", "X-Hub-Signature-256": ig_sig}, expect=200)

# 3. TikTok, Twitter, Google, Microsoft OAuth & webhooks
tt_auth, _ = req("/api/v1/accounts/1/tiktok/authorization", "POST", {}, H_ADMIN)
tt_state = urllib.parse.parse_qs(urllib.parse.urlparse(tt_auth["url"]).query)["state"][0]
_, tt_hdrs = req(f"/tiktok/callback?code=ttcode&state={urllib.parse.quote(tt_state)}", expect=307)
assert "/agents" in tt_hdrs["Location"]

tw_auth, _ = req("/api/v1/accounts/1/twitter/authorization", "POST", {}, H_ADMIN)
tw_state = urllib.parse.parse_qs(urllib.parse.urlparse(tw_auth["url"]).query)["oauth_token"][0]
_, tw_hdrs = req(f"/twitter/callback?user_id=tw-prof-1&oauth_token={urllib.parse.quote(tw_state)}&oauth_verifier=v1", expect=307)
assert "/agents" in tw_hdrs["Location"]

crc, _ = req("/webhooks/twitter?crc_token=crc_abc")
exp_crc = "sha256=" + base64.b64encode(hmac.new(b"tw-secret", b"crc_abc", hashlib.sha256).digest()).decode()
assert crc["response_token"] == exp_crc, crc

goog, _ = req("/api/v1/accounts/1/google/authorization", "POST", {"email": "g@example.com"}, H_ADMIN)
assert "accounts.google.com" in goog["url"]
ms, _ = req("/api/v1/accounts/1/microsoft/authorization", "POST", {"email": "m@example.com"}, H_ADMIN)
assert "login.microsoftonline.com" in ms["url"]

# 4. WhatsApp access_request, manual preview + connect + webhook_status + embedded signup
acc_req, _ = req("/api/v1/accounts/1/whatsapp/access_request", "POST", {"use_case": "support"}, H_ADMIN)
assert acc_req["features"].get("whatsapp_embedded_signup_inbox_creation") is True, acc_req
req("/api/v1/accounts/1/whatsapp/access_request", "POST", {"use_case": "support"}, H_AGENT, expect=403)

prev, _ = req("/api/v1/accounts/1/whatsapp/manual/preview", "POST", {
    "waba_id": "waba-1",
    "phone_number_id": "pn-manual-1",
    "access_token": "perm-tok",
}, H_ADMIN)
assert prev["display_phone_number"] == "+15550001122", prev

conn, _ = req("/api/v1/accounts/1/whatsapp/manual/connect", "POST", {
    "waba_id": "waba-1",
    "phone_number_id": "pn-manual-1",
    "access_token": "perm-tok",
    "app_secret": "app-sec",
    "inbox_name": "WA Manual Inbox",
}, H_ADMIN, expect=201)
wa_inbox_id = conn["id"]
assert conn["webhook_setup"] is True, conn

wh_st, _ = req(f"/api/v1/accounts/1/whatsapp/manual/{wa_inbox_id}/webhook_status", "GET", None, H_ADMIN)
assert wh_st["callback_configured"] is True and wh_st["subscription_verified"] is True, wh_st
wh_setup, _ = req(f"/api/v1/accounts/1/whatsapp/manual/{wa_inbox_id}/setup_webhook", "POST", {}, H_ADMIN)
assert wh_setup["callback_verified"] is True

emb, _ = req("/api/v1/accounts/1/whatsapp/authorization", "POST", {
    "code": "emb-code",
    "waba_id": "waba-emb",
    "phone_number_id": "pn-emb-1",
}, H_ADMIN)
assert emb["success"] is True and emb["channel_type"] == "whatsapp", emb

# 5. Scheduled Campaign dispatch + Analytics metrics & contacts
lbl, _ = req("/api/v1/accounts/1/labels", "POST", {"title": "vip-wave2"}, H_ADMIN)
lbl_id = lbl["id"]
c1, _ = req("/api/v1/accounts/1/contacts", "POST", {"name": "Alice WA", "phone_number": "+15552223333"}, H_ADMIN)
req(f"/api/v1/accounts/1/contacts/{c1['payload']['contact']['id']}/labels", "POST", {"labels": ["vip-wave2"]}, H_ADMIN)
c2, _ = req("/api/v1/accounts/1/contacts", "POST", {"name": "Bob NoPhone"}, H_ADMIN)
req(f"/api/v1/accounts/1/contacts/{c2['payload']['contact']['id']}/labels", "POST", {"labels": ["vip-wave2"]}, H_ADMIN)

camp, _ = req("/api/v1/accounts/1/campaigns", "POST", {
    "campaign": {
        "title": "Wave2 WA Promo",
        "inbox_id": wa_inbox_id,
        "message": "Hi promo!",
        "campaign_type": "one_off",
        "enabled": True,
        "scheduled_at": int(time.time()) - 5,
        "audience": [{"type": "Label", "id": lbl_id}],
        "template_params": {"name": "hello_world", "language": "en", "parameters": []},
    }
}, H_ADMIN)
camp_id = camp["id"]

# Wait up to 6s for 1s scheduler loop to dispatch campaign
for _ in range(12):
    cur, _ = req(f"/api/v1/accounts/1/campaigns/{camp_id}", "GET", None, H_ADMIN)
    if cur["campaign_status"] == "completed":
        break
    time.sleep(0.5)
assert cur["campaign_status"] == "completed", cur

metrics, _ = req(f"/api/v1/accounts/1/campaigns/{camp_id}/analytics/metrics", "GET", None, H_ADMIN)
assert metrics["audience"] == 2 and metrics["sent"] >= 1 and metrics["failed"] >= 1, metrics
contacts_res, _ = req(f"/api/v1/accounts/1/campaigns/{camp_id}/analytics/contacts", "GET", None, H_ADMIN)
assert contacts_res["meta"]["total_count"] == 2 and len(contacts_res["payload"]) == 2, contacts_res

# 6. Help Center: sitemap.xml, .md, .png view increment, search, category show, translate, remove_locale
portal_res, _ = req("/api/v1/accounts/1/portals", "POST", {
    "portal": {
        "name": "Wave2 Help",
        "slug": "wave2-help",
        "config": {"allowed_locales": ["en", "es"], "default_locale": "en"},
    }
}, H_ADMIN)
cat_res, _ = req("/api/v1/accounts/1/portals/wave2-help/categories", "POST", {
    "category": {"name": "Getting Started", "slug": "getting-started", "locale": "en", "position": 1}
}, H_ADMIN)
cat_id = cat_res["payload"]["id"]
art_res, _ = req("/api/v1/accounts/1/portals/wave2-help/articles", "POST", {
    "article": {
        "title": "Welcome Guide",
        "slug": "welcome-guide",
        "content": "# Welcome\nHello **Pulsar**!",
        "description": "Quick intro",
        "category_id": cat_id,
        "author_id": 1,
        "status": 1,
        "locale": "en",
    }
}, H_ADMIN)
art_id = art_res["payload"]["id"]

sitemap, sm_hdrs = req("/hc/wave2-help/sitemap.xml")
assert "application/xml" in sm_hdrs["Content-Type"] and b"/hc/wave2-help/articles/welcome-guide" in sitemap, sitemap

md, md_hdrs = req("/hc/wave2-help/articles/welcome-guide.md")
assert "text/markdown" in md_hdrs["Content-Type"] and b"Hello **Pulsar**" in md, md

png, png_hdrs = req("/hc/wave2-help/articles/welcome-guide.png")
assert png_hdrs["Content-Type"] == "image/png" and png[:4] == b"\x89PNG"
art_after, _ = req(f"/api/v1/accounts/1/portals/wave2-help/articles/{art_id}", "GET", None, H_ADMIN)
assert art_after["payload"]["views"] == 1, art_after

hc_search, _ = req("/hc/wave2-help/en/search?query=Pulsar")
assert hc_search["meta"]["articles_count"] == 1, hc_search
hc_cat, _ = req("/hc/wave2-help/en/categories/getting-started")
assert hc_cat["slug"] == "getting-started" and len(hc_cat["articles"]) == 1, hc_cat

# Translate to 'es', verify 409 conflict on repeat without force, 200 with force=true
req("/api/v1/accounts/1/portals/wave2-help/articles/bulk_actions/translate", "POST", {
    "ids": [art_id],
    "locale": "es",
}, H_ADMIN, expect=200)
dup_res, _ = req("/api/v1/accounts/1/portals/wave2-help/articles/bulk_actions/translate", "POST", {
    "ids": [art_id],
    "locale": "es",
}, H_ADMIN, expect=409)
assert len(dup_res["duplicate_articles"]) == 1, dup_res
req("/api/v1/accounts/1/portals/wave2-help/articles/bulk_actions/translate", "POST", {
    "ids": [art_id],
    "locale": "es",
    "force": True,
}, H_ADMIN, expect=200)

# Remove locale 'es', fail on default 'en'
req("/api/v1/accounts/1/portals/wave2-help/remove_locale?locale=en", "DELETE", None, H_ADMIN, expect=422)
rem_loc, _ = req("/api/v1/accounts/1/portals/wave2-help/remove_locale?locale=es", "DELETE", None, H_ADMIN, expect=200)
assert [l["code"] for l in rem_loc["config"]["allowed_locales"]] == ["en"], rem_loc

print("WAVE 2 CHANNELS + CAMPAIGNS + HELP CENTER PARITY CHECKS PASSED")
PY
