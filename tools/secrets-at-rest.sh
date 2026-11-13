#!/usr/bin/env bash
# Secrets at rest: with encryption keys, webhook / agent bot / API-channel secrets are stored encrypted,
# returned in plaintext to admins, and webhook signatures use the plaintext secret.
# Starts its own server on 3126 with test keys and a webhook receiver on 3996.
set -euo pipefail
DB=${DATABASE_URL:-postgres://localhost/pulsar}
q() { psql -qtAX "$DB" -c "$1"; }
cd "$(dirname "$0")/.."
D=$(mktemp -d)
python3 - "$D/recv" <<'PY' &
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(s):
        body = s.rfile.read(int(s.headers['content-length']))
        open(sys.argv[1], 'a').write(s.headers.get('X-Chatwoot-Timestamp','') + '\t' + s.headers.get('X-Chatwoot-Signature','') + '\t' + body.decode() + '\n')
        s.send_response(200); s.end_headers()
    def log_message(s, *a): pass
http.server.HTTPServer(('127.0.0.1', 3996), H).serve_forever()
PY
RECV=$!
ACTIVE_RECORD_ENCRYPTION_PRIMARY_KEY=secretstest ACTIVE_RECORD_ENCRYPTION_KEY_DERIVATION_SALT=secretssalt PULSAR_WEBHOOK_ALLOW_PRIVATE=1 \
  BIND=127.0.0.1:3126 PULSAR_SCHEDULER_INTERVAL_SECS=3600 ./target/debug/pulsar >"$D/server.log" 2>&1 & SRV=$!
T=$(q "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
H=(-H "api_access_token: $T" -H 'content-type: application/json')
B=http://127.0.0.1:3126/api/v1/accounts/1
WH= BOT= INBOX=
cleanup() {
  kill $RECV $SRV 2>/dev/null || true
  [[ -n $WH ]] && q "delete from webhooks where id=$WH" >/dev/null
  [[ -n $BOT ]] && q "delete from access_tokens where owner_type='AgentBot' and owner_id=$BOT; delete from agent_bots where id=$BOT" >/dev/null
  if [[ -n $INBOX ]]; then CH=$(q "select channel_id from inboxes where id=$INBOX"); q "delete from inbox_members where inbox_id=$INBOX; delete from inboxes where id=$INBOX; delete from channel_api where id=$CH" >/dev/null; fi
  rm -rf "$D"
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
sleep 4
R=$(curl -sf "${H[@]}" -X POST "$B/webhooks" -d '{"webhook":{"url":"http://127.0.0.1:3996/h","subscriptions":["contact_created"]}}')
WH=$(jq '.payload.webhook.id' <<<"$R"); SECRET=$(jq -r '.payload.webhook.secret' <<<"$R")
[[ ${#SECRET} == 24 ]] || fail "webhook secret not returned as plaintext: $SECRET"
q "select secret from webhooks where id=$WH" | grep -q '^{"p":' || fail "webhook secret stored in plaintext"
BOT=$(curl -sf "${H[@]}" -X POST "$B/agent_bots" -d '{"name":"secbot"}' | jq .id)
BS=$(curl -sf "${H[@]}" "$B/agent_bots/$BOT" | jq -r .secret)
[[ ${#BS} == 24 ]] || fail "bot secret not plaintext in API"
q "select secret from agent_bots where id=$BOT" | grep -q '^{"p":' || fail "bot secret stored in plaintext"
INBOX=$(curl -sf "${H[@]}" -X POST "$B/inboxes" -d '{"name":"sec api","channel":{"type":"api"}}' | jq .id)
AS=$(curl -sf "${H[@]}" "$B/inboxes/$INBOX" | jq -r .secret)
[[ ${#AS} == 24 ]] || fail "api channel secret not plaintext in API: $AS"
q "select c.secret from channel_api c join inboxes i on i.channel_id=c.id and i.channel_type='Channel::Api' where i.id=$INBOX" | grep -q '^{"p":' || fail "api secret stored in plaintext"
# signature must be computed with the plaintext secret
curl -sf "${H[@]}" -X POST "$B/contacts" -d "{\"name\":\"sig $(date +%s)\"}" >/dev/null
for _ in $(seq 1 20); do [[ -s $D/recv ]] && break; sleep 0.5; done
IFS=$'\t' read -r TS SIG BODY < "$D/recv"
EXP="sha256=$(printf '%s.%s' "$TS" "$BODY" | openssl dgst -sha256 -hmac "$SECRET" -hex | awk '{print $NF}')"
[[ $SIG == "$EXP" ]] || fail "signature mismatch"
q "delete from contacts where name like 'sig %' and created_at > now() - interval '5 minutes'" >/dev/null
echo "secrets: encrypted at rest, plaintext to admins, webhook signature OK"
