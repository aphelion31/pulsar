#!/usr/bin/env bash
set -euo pipefail
B=${BASE:-http://127.0.0.1:3106}
PORT=${WEBHOOK_TEST_PORT:-3107}
DIR=$(mktemp -d "${JCODE_SCRATCH_DIR:-/tmp}/webhook-test.XXXXXX")
cleanup() { if [ -n "${HOOK:-}" ]; then curl -fsS "${AUTH[@]}" -X DELETE "$A/webhooks/$HOOK" >/dev/null || true; fi; [ -z "${RECEIVER:-}" ] || kill "$RECEIVER" 2>/dev/null || true; [ -z "${SERVER:-}" ] || kill "$SERVER" 2>/dev/null || true; rm -rf "$DIR"; }
trap cleanup EXIT
if [ -z "${BASE:-}" ]; then
  BIND=127.0.0.1:3106 PULSAR_WEBHOOK_ALLOW_PRIVATE=1 "${PULSAR_BIN:-${CARGO_TARGET_DIR:-target}/debug/pulsar}" >"$DIR/server.log" 2>&1 &
  SERVER=$!
  for _ in {1..30}; do
    if grep -q 'pulsar listening on' "$DIR/server.log"; then break; fi
    kill -0 "$SERVER" 2>/dev/null || { cat "$DIR/server.log"; exit 1; }
    sleep 1
  done
fi
python3 -u - "$PORT" "$DIR/received" <<'PY' &
import http.server, json, sys
class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers['Content-Length'])).decode()
        with open(sys.argv[2], 'a') as f:
            f.write(json.dumps({'path': self.path, 'headers': dict(self.headers), 'body': body}) + '\n')
        self.send_response(200)
        self.end_headers()
http.server.HTTPServer(('127.0.0.1', int(sys.argv[1])), Handler).serve_forever()
PY
RECEIVER=$!
sleep 1
curl -sf -D "$DIR/headers" -o /dev/null -H 'content-type: application/json' -d '{"email":"admin@example.com","password":"Password123!"}' "$B/auth/sign_in"
hdr() { awk -v key="$1" 'tolower($1)==tolower(key ":") {gsub("\r", "", $2); print $2}' "$DIR/headers"; }
AUTH=(-H "access-token: $(hdr access-token)" -H "client: $(hdr client)" -H "uid: $(hdr uid)" -H 'content-type: application/json')
A="$B/api/v1/accounts/1"
j() { curl -fsS "${AUTH[@]}" "$@"; }
INBOX=$(j -X POST "$A/inboxes" -d "{\"name\":\"Webhook test $RANDOM\",\"channel\":{\"type\":\"api\",\"webhook_url\":\"http://127.0.0.1:$PORT/inbox\"}}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')
HOOK=$(j -X POST "$A/webhooks" -d "{\"webhook\":{\"url\":\"http://127.0.0.1:$PORT/account?$RANDOM$RANDOM\",\"subscriptions\":[\"message_created\"]}}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["payload"]["webhook"]["id"])')
psql "${DATABASE_URL:-postgres://localhost/pulsar}" -v ON_ERROR_STOP=1 -q -c "UPDATE webhooks SET secret='test-secret' WHERE id=$HOOK AND account_id=1; UPDATE channel_api SET secret='test-secret' WHERE id=(SELECT channel_id FROM inboxes WHERE id=$INBOX AND account_id=1);"
CONTACT=$(j -X POST "$A/contacts" -d "{\"name\":\"Webhook test $RANDOM\"}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["payload"]["contact"]["id"])')
CONV=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')
MSG=$(j -X POST "$A/conversations/$CONV/messages" -d '{"content":"webhook delivery test","message_type":"outgoing"}' | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')
for _ in {1..30}; do
  if [ -f "$DIR/received" ] && [ "$(wc -l < "$DIR/received")" -ge 2 ]; then break; fi
  sleep 1
done
python3 - "$DIR/received" "$MSG" <<'PY'
import hashlib, hmac, json, sys, uuid
with open(sys.argv[1]) as f: rows = [json.loads(line) for line in f]
rows = [r for r in rows if json.loads(r['body']).get('id') == int(sys.argv[2]) and json.loads(r['body']).get('event') == 'message_created']
assert {r['path'].split('?')[0] for r in rows} == {'/account', '/inbox'}, rows
for r in rows:
    headers = {k.lower():v for k,v in r['headers'].items()}
    assert headers['content-type'] == 'application/json' and headers['accept'] == 'application/json'
    uuid.UUID(headers['x-chatwoot-delivery'])
    expected = 'sha256=' + hmac.new(b'test-secret', (headers['x-chatwoot-timestamp'] + '.' + r['body']).encode(), hashlib.sha256).hexdigest()
    assert hmac.compare_digest(headers['x-chatwoot-signature'], expected)
print('Webhook delivery and signatures OK')
PY
