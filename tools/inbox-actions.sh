#!/usr/bin/env bash
# Inbox actions: reset_secret, health, sync_templates, register_webhook, csat_template (GET/POST).
# Creates its own Api inbox and removes it on exit. Asserts status codes and JSON shape.
set -euo pipefail
B=${BASE:-http://127.0.0.1:3162}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
q() { psql -qtAX "$DB" -c "$1"; }
TOKEN=$(q "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
A=(-H "api_access_token: $TOKEN" -H 'content-type: application/json')
ACC=1
CH=
INBOX=
cleanup() {
  if [[ -n $INBOX ]]; then
    q "delete from inbox_members where inbox_id=$INBOX; delete from inboxes where id=$INBOX" >/dev/null || true
  fi
  if [[ -n $CH ]]; then q "delete from channel_api where id=$CH" >/dev/null || true; fi
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok: $*"; }

# status CODE_EXPECTED METHOD PATH [BODY]
expect_status() {
  local want=$1 method=$2 path=$3 body=${4:-}
  local out code
  if [[ -n $body ]]; then
    out=$(curl -s -o /tmp/ia-body.$$ -w '%{http_code}' "${A[@]}" -X "$method" "$B$path" -d "$body")
  else
    out=$(curl -s -o /tmp/ia-body.$$ -w '%{http_code}' "${A[@]}" -X "$method" "$B$path")
  fi
  code=$out
  [[ $code == "$want" ]] || fail "$method $path expected $want got $code: $(cat /tmp/ia-body.$$)"
  pass "$method $path -> $code"
}

# Api channel inbox for reset_secret.
CH=$(q "insert into channel_api (account_id, webhook_url, identifier, hmac_token, secret, created_at, updated_at)
        values ($ACC, '', 'ia-test-$$', 'ia-hmac-$$', 'plain-old', now(), now()) returning id")
INBOX=$(q "insert into inboxes (account_id, channel_id, channel_type, name, created_at, updated_at)
           values ($ACC, $CH, 'Channel::Api', 'ia-test-$$', now(), now()) returning id")

# reset_secret: returns new plaintext secret, DB row changes, old secret gone.
OLD=$(q "select secret from channel_api where id=$CH")
RESP=$(curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/inboxes/$INBOX/reset_secret")
NEW=$(echo "$RESP" | jq -r .secret)
[[ -n $NEW && $NEW != null ]] || fail "reset_secret returned no secret: $RESP"
[[ $NEW != plain-old ]] || fail "reset_secret kept old secret"
[[ $(echo "$RESP" | jq -r .channel_type) == "Channel::Api" ]] || fail "reset_secret response missing inbox fields"
STORED=$(q "select secret from channel_api where id=$CH")
[[ $STORED != "$OLD" ]] || fail "stored secret unchanged"
pass "POST reset_secret rotates secret and returns plaintext"

# reset_secret on a non-Api inbox: 404 (Chatwoot head :not_found). Use the seeded web widget inbox.
WIDGET=$(q "select id from inboxes where account_id=$ACC and channel_type='Channel::WebWidget' order by id limit 1")
if [[ -n $WIDGET ]]; then expect_status 404 POST "/api/v1/accounts/$ACC/inboxes/$WIDGET/reset_secret"; fi

# health: web widget is not a health channel -> 400 (validate_health_supported_channel).
if [[ -n $WIDGET ]]; then expect_status 400 GET "/api/v1/accounts/$ACC/inboxes/$WIDGET/health"; fi

# sync_templates on a non-WhatsApp inbox -> 422.
expect_status 422 POST "/api/v1/accounts/$ACC/inboxes/$INBOX/sync_templates"

# register_webhook on a non-supported inbox -> 400.
expect_status 400 POST "/api/v1/accounts/$ACC/inboxes/$INBOX/register_webhook"

# csat_template on a non-WhatsApp inbox -> 400.
expect_status 400 GET "/api/v1/accounts/$ACC/inboxes/$INBOX/csat_template"
expect_status 400 POST "/api/v1/accounts/$ACC/inboxes/$INBOX/csat_template" '{"template":{"message":"hi"}}'

# Unknown inbox -> 404 everywhere.
expect_status 404 GET "/api/v1/accounts/$ACC/inboxes/999999999/health"
expect_status 404 POST "/api/v1/accounts/$ACC/inboxes/999999999/reset_secret"

# Unauthenticated -> 401 (no token).
code=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/api/v1/accounts/$ACC/inboxes/$INBOX/reset_secret")
[[ $code == 401 ]] || fail "unauthenticated reset_secret expected 401 got $code"
pass "unauthenticated -> 401"

rm -f /tmp/ia-body.$$
echo "ALL PASS"
