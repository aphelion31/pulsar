#!/usr/bin/env bash
# Scheduler: snoozed conversations past snoozed_until reopen; inactive open conversations auto-resolve with
# the system activity message. Uses a server started with PULSAR_SCHEDULER_INTERVAL_SECS=2. Restores state.
set -euo pipefail
DB=${DATABASE_URL:-postgres://localhost/pulsar}
q() { PGTZ=UTC psql -qtAX "$DB" -c "$1"; }  # server sessions run in UTC (sqlx)
read -r SNZ RES < <(q "select (select id from conversations where account_id=1 and contact_id is not null order by id limit 1), (select id from conversations where account_id=1 and contact_id is not null order by id offset 1 limit 1)" | tr '|' ' ')
SAVE_SETTINGS=$(q "select coalesce(settings,'{}'::jsonb) from accounts where id=1")
SAVE=$(q "select string_agg(format('UPDATE conversations SET status=%s, snoozed_until=%L, last_activity_at=%L WHERE id=%s;', status, snoozed_until, last_activity_at, id), ' ') from conversations where id in ($SNZ,$RES)")
restore() {
  q "$SAVE" >/dev/null
  q "update accounts set settings='$SAVE_SETTINGS'::jsonb where id=1" >/dev/null
  q "delete from messages where conversation_id=$RES and content like 'Conversation was marked resolved by system%'" >/dev/null
}
trap restore EXIT
q "delete from messages where conversation_id=$RES and content like 'Conversation was marked resolved by system%'" >/dev/null
q "update conversations set status=3, snoozed_until=now()-interval '1 minute', last_activity_at=now() where id=$SNZ" >/dev/null
q "update conversations set status=0, last_activity_at=now()-interval '3 hours' where id=$RES" >/dev/null
q "update accounts set settings=coalesce(settings,'{}'::jsonb) || '{\"auto_resolve_after\": 120}' where id=1" >/dev/null
sleep 5
[[ $(q "select status from conversations where id=$SNZ") == 0 ]] || { echo "FAIL snoozed not reopened"; exit 1; }
[[ $(q "select status from conversations where id=$RES") == 1 ]] || { echo "FAIL not auto-resolved"; exit 1; }
q "select count(*) from messages where conversation_id=$RES and message_type=2 and content='Conversation was marked resolved by system due to 2 hours of inactivity'" | grep -qx 1 || { echo "FAIL activity message"; exit 1; }
echo "scheduler: snooze reopen + auto-resolve OK"
