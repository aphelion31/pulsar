#!/usr/bin/env bash
set -euo pipefail
B=${BASE:-http://127.0.0.1:3123}
D=$(mktemp -d "${JCODE_SCRATCH_DIR:-$HOME/.jcode/scratch}/email-test.XXXXXX")
INBOX= CONTACT= CONV= CHANNEL= SINK=
cleanup() {
  [[ -z "$SINK" ]] || kill "$SINK" 2>/dev/null || true
  if [[ -n "$CONV" ]]; then psql postgres://localhost/pulsar -v ON_ERROR_STOP=1 -c "DELETE FROM messages WHERE conversation_id = $CONV; DELETE FROM conversations WHERE id = $CONV;" >/dev/null || true; fi
  if [[ -n "$INBOX" ]]; then psql postgres://localhost/pulsar -v ON_ERROR_STOP=1 -c "DELETE FROM inbox_members WHERE inbox_id = $INBOX; DELETE FROM contact_inboxes WHERE inbox_id = $INBOX; DELETE FROM inboxes WHERE id = $INBOX;" >/dev/null || true; fi
  if [[ -n "$CHANNEL" ]]; then psql postgres://localhost/pulsar -v ON_ERROR_STOP=1 -c "DELETE FROM channel_email WHERE id = $CHANNEL;" >/dev/null || true; fi
  if [[ -n "$CONTACT" ]]; then psql postgres://localhost/pulsar -v ON_ERROR_STOP=1 -c "DELETE FROM contacts WHERE id = $CONTACT;" >/dev/null || true; fi
  rm -rf "$D"
}
trap cleanup EXIT
cat > "$D/sink.py" <<'PY'
import asyncio, sys
async def client(r, w):
    w.write(b'220 localhost ESMTP\r\n'); await w.drain()
    data = False; message = []
    while line := await r.readline():
        if data:
            if line == b'.\r\n':
                open(sys.argv[1], 'wb').write(b''.join(message))
                data = False; response = b'250 accepted\r\n'
            else:
                message.append(line); continue
        else:
            cmd = line.split(maxsplit=1)[0].upper()
            if cmd in (b'EHLO', b'HELO'): response = b'250 localhost\r\n'
            elif cmd in (b'MAIL', b'RCPT', b'RSET'): response = b'250 OK\r\n'
            elif cmd == b'DATA': data = True; message = []; response = b'354 send data\r\n'
            elif cmd == b'QUIT': w.write(b'221 bye\r\n'); await w.drain(); break
            else: response = b'250 OK\r\n'
        w.write(response); await w.drain()
    w.close(); await w.wait_closed()
async def main():
    server = await asyncio.start_server(client, '127.0.0.1', 2525)
    async with server: await server.serve_forever()
asyncio.run(main())
PY
python3 "$D/sink.py" "$D/mail" >"$D/sink.log" 2>&1 & SINK=$!
sleep 1
kill -0 "$SINK"
curl -sf -D "$D/headers" -o /dev/null -H 'content-type: application/json' -d '{"email":"admin@example.com","password":"Password123!"}' "$B/auth/sign_in"
hdr() { grep -i "^$1:" "$D/headers" | cut -d' ' -f2 | tr -d '\r'; }
AUTH=(-H "access-token: $(hdr access-token)" -H "client: $(hdr client)" -H "uid: $(hdr uid)" -H 'content-type: application/json')
A="$B/api/v1/accounts/1"
j() { curl -fsS "${AUTH[@]}" "$@"; }
N="$(date +%s)-$$"
R=$(j -X POST "$A/inboxes" -d "{\"name\":\"Mail test $N\",\"channel\":{\"type\":\"email\",\"email\":\"test-$N@example.com\",\"smtp_enabled\":true,\"smtp_address\":\"127.0.0.1\",\"smtp_port\":2525,\"smtp_enable_ssl_tls\":false,\"smtp_enable_starttls_auto\":false,\"smtp_password\":\"sink-pass\"}}")
INBOX=$(echo "$R" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
CHANNEL=$(echo "$R" | python3 -c 'import json,sys; x=json.load(sys.stdin); assert "smtp_password" not in x and "imap_password" not in x; print(x["channel_id"])')
# With encryption keys set (ENCRYPTED=1), the stored password must be an encrypted payload, never plaintext.
if [[ ${ENCRYPTED:-0} == 1 ]]; then
  psql postgres://localhost/pulsar -tAc "SELECT smtp_password FROM channel_email WHERE id = $CHANNEL" | grep -q '^{"p":' || { echo "smtp_password stored in plaintext" >&2; exit 1; }
fi
CONTACT=$(j -X POST "$A/contacts" -d "{\"name\":\"Mail test\",\"email\":\"recipient-$N@example.com\"}" | python3 -c 'import sys,json;print(json.load(sys.stdin)["payload"]["contact"]["id"])')
CONV=$(j -X POST "$A/conversations" -d "{\"inbox_id\":$INBOX,\"contact_id\":$CONTACT}" | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
j -X POST "$A/conversations/$CONV/messages" -d '{"content":"incoming","message_type":"incoming","content_attributes":{"email":{"message_id":"incoming@example.com"}}}' >/dev/null
j -X POST "$A/conversations/$CONV/messages" -d '{"content":"outgoing reply","message_type":"outgoing"}' >/dev/null
for _ in {1..50}; do [[ -s "$D/mail" ]] && break; sleep .2; done
python3 - "$D/mail" "recipient-$N@example.com" <<'PY'
import email, sys
from email import policy
m=email.message_from_binary_file(open(sys.argv[1], 'rb'), policy=policy.default)
assert sys.argv[2] in m['To'], m
assert m['Subject'].endswith('New message'), m['Subject']
assert str(m['In-Reply-To']) == '<incoming@example.com>', m['In-Reply-To']
assert '<incoming@example.com>' in str(m['References'])
assert m['Message-ID'].startswith('<conversation/'), m['Message-ID']
print('Email outbound integration OK')
PY
