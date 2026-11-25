#!/usr/bin/env bash
# Durable events: an event left in the pulsar_events outbox (as after a crash) is delivered on the next
# startup and the row is removed; normal events are cleaned up after delivery.
# Needs: built target/debug/pulsar, local DB. Starts its own server on 3124 and a webhook sink on 3998.
set -euo pipefail
DB=${DATABASE_URL:-postgres://localhost/pulsar}
q() { psql -qtAX "$DB" -c "$1"; }
cd "$(dirname "$0")/.."
SINK_OUT=$(mktemp); SERVER_LOG=$(mktemp)
python3 - "$SINK_OUT" <<'PY' &
import http.server, sys
out = open(sys.argv[1], 'a')
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(s):
        out.write(s.rfile.read(int(s.headers['content-length'])).decode() + "\n"); out.flush()
        s.send_response(200); s.end_headers()
    def log_message(s, *a): pass
http.server.HTTPServer(('127.0.0.1', 3998), H).serve_forever()
PY
SINK=$!
HOOK=$(q "insert into webhooks (account_id, url, subscriptions, webhook_type, created_at, updated_at) values (1, 'http://127.0.0.1:3998/h', '[\"contact_created\"]', 0, now(), now()) returning id")
MARK="durable-$(date +%s)"
q "insert into pulsar_events (name, account_id, data) values ('contact.created', 1, '{\"id\": 1, \"name\": \"$MARK\"}')" >/dev/null
cleanup() { kill $SINK ${SRV:-} 2>/dev/null || true; q "delete from webhooks where id=$HOOK" >/dev/null; q "delete from pulsar_events where data->>'name' = '$MARK'" >/dev/null; rm -f "$SINK_OUT" "$SERVER_LOG"; }
trap cleanup EXIT
BIND=127.0.0.1:3124 PULSAR_WEBHOOK_ALLOW_PRIVATE=1 PULSAR_SCHEDULER_INTERVAL_SECS=3600 ./target/debug/pulsar >"$SERVER_LOG" 2>&1 &
SRV=$!
for _ in $(seq 1 20); do grep -q "$MARK" "$SINK_OUT" 2>/dev/null && break; sleep 0.5; done
grep -q "$MARK" "$SINK_OUT" || { echo "FAIL replayed event not delivered"; tail -5 "$SERVER_LOG"; exit 1; }
sleep 1
[[ $(q "select count(*) from pulsar_events where data->>'name' = '$MARK'") == 0 ]] || { echo "FAIL outbox row not removed"; exit 1; }
grep -q "replaying undelivered events" "$SERVER_LOG" || { echo "FAIL no replay log"; exit 1; }
echo "durable events: replay after restart, delivery, cleanup OK"
