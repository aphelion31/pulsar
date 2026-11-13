#!/usr/bin/env bash
# Agent email notifications: with global SMTP pointing at a local sink, assigning a conversation to an agent
# who has the email_conversation_assignment flag sends "…has been assigned to you." to that agent.
# Starts its own server on 3125 and an SMTP sink on 2526. Restores the agent's notification flags.
set -euo pipefail
DB=${DATABASE_URL:-postgres://localhost/pulsar}
q() { psql -qtAX "$DB" -c "$1"; }
cd "$(dirname "$0")/.."
D=$(mktemp -d)
cat > "$D/sink.py" <<'PY'
import asyncio, sys
async def client(r, w):
    w.write(b'220 sink\r\n'); await w.drain(); data = False; msg = []
    while line := await r.readline():
        if data:
            if line == b'.\r\n':
                open(sys.argv[1], 'ab').write(b''.join(msg) + b'\n=====\n'); data = False; resp = b'250 ok\r\n'
            else: msg.append(line); continue
        else:
            c = line.split(maxsplit=1)[0].upper()
            if c == b'DATA': data = True; msg = []; resp = b'354 go\r\n'
            elif c == b'QUIT': w.write(b'221 bye\r\n'); await w.drain(); break
            else: resp = b'250 ok\r\n'
        w.write(resp); await w.drain()
    w.close()
async def main():
    s = await asyncio.start_server(client, '127.0.0.1', 2526)
    async with s: await s.serve_forever()
asyncio.run(main())
PY
python3 "$D/sink.py" "$D/mail" & SINK=$!
AGENT=$(q "select u.id from users u join account_users au on au.user_id=u.id and au.account_id=1 where u.id<>1 and u.confirmed_at is not null order by u.id limit 1")
AGENT_EMAIL=$(q "select email from users where id=$AGENT")
SAVE=$(q "select coalesce((select email_flags::text from notification_settings where account_id=1 and user_id=$AGENT),'none')")
q "insert into notification_settings (account_id,user_id,email_flags,push_flags,created_at,updated_at) values (1,$AGENT,2,0,now(),now()) on conflict (account_id,user_id) do update set email_flags=2" >/dev/null
CONV=$(q "select c.display_id from conversations c join inboxes i on i.id=c.inbox_id where c.account_id=1 and c.contact_id is not null and c.status=0 order by c.id desc limit 1")
INBOX=$(q "select inbox_id from conversations where account_id=1 and display_id=$CONV")
MEMBER=$(q "select count(*) from inbox_members where inbox_id=$INBOX and user_id=$AGENT")
[[ $MEMBER == 0 ]] && q "insert into inbox_members (inbox_id,user_id,created_at,updated_at) values ($INBOX,$AGENT,now(),now())" >/dev/null
PREV=$(q "select coalesce(assignee_id::text,'null') from conversations where account_id=1 and display_id=$CONV")
cleanup() {
  kill $SINK ${SRV:-} 2>/dev/null || true
  if [[ $SAVE == none ]]; then q "delete from notification_settings where account_id=1 and user_id=$AGENT" >/dev/null; else q "update notification_settings set email_flags=$SAVE where account_id=1 and user_id=$AGENT" >/dev/null; fi
  [[ $MEMBER == 0 ]] && q "delete from inbox_members where inbox_id=$INBOX and user_id=$AGENT" >/dev/null
  q "update conversations set assignee_id=$PREV where account_id=1 and display_id=$CONV" >/dev/null
  cp "$D/server.log" /tmp/agent-email-server.log 2>/dev/null; cp "$D/mail" /tmp/agent-email.mail 2>/dev/null; rm -rf "$D"
}
trap cleanup EXIT
SMTP_ADDRESS=127.0.0.1 SMTP_PORT=2526 SMTP_ENABLE_STARTTLS_AUTO=false MAILER_SENDER_EMAIL=noreply@pulsar.test BIND=127.0.0.1:3125 \
  PULSAR_SCHEDULER_INTERVAL_SECS=3600 ./target/debug/pulsar >"$D/server.log" 2>&1 & SRV=$!
sleep 4
TOKEN=$(q "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
q "update conversations set assignee_id=null where account_id=1 and display_id=$CONV" >/dev/null
curl -sf -H "api_access_token: $TOKEN" -H 'content-type: application/json' -X POST "http://127.0.0.1:3125/api/v1/accounts/1/conversations/$CONV/assignments" -d "{\"assignee_id\":$AGENT}" >/dev/null
for _ in $(seq 1 20); do [[ -s $D/mail ]] && break; sleep 0.5; done
grep -q "has been assigned to you" "$D/mail" || { echo "FAIL no assignment email"; grep -iE "notification|mail" "$D/server.log" | tail -3; exit 1; }
grep -qi "^To: .*$AGENT_EMAIL" "$D/mail" || { echo "FAIL wrong recipient"; exit 1; }
python3 -c 'import quopri,sys; sys.stdout.write(quopri.decodestring(open(sys.argv[1],"rb").read()).decode())' "$D/mail" | grep -q "/app/accounts/1/conversations/$CONV\"" || { echo "FAIL missing link"; exit 1; }
echo "agent email: assignment mail to $AGENT_EMAIL OK"
