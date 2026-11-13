#!/usr/bin/env bash
# Round 1 misc: notifications destroy_all/snooze/unread, agents bulk_create, public csat_survey show/update.
# Usage: BASE=http://127.0.0.1:3133 DATABASE_URL=postgres://localhost/pulsar tools/misc-r1.sh
set -euo pipefail
B=${BASE:-http://127.0.0.1:3133}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
q() { psql -qtAX "$DB" -c "$1"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok: $*"; }

TOKEN=$(q "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
A=(-H "api_access_token: $TOKEN" -H 'content-type: application/json')
ACC=1
USER_ID=$(q "select id from users where email='admin@example.com' limit 1")
[[ -n $USER_ID ]] || fail "admin user not found"
TAG="misc-r1-$RANDOM$RANDOM"

INBOX_ID= CONV_ID= CONV_UUID= CSAT_MSG= SAVE_CSAT=
NOTIF_IDS=()
EMAILS=("${TAG}-a@ex.com" "${TAG}-b@ex.com" "${TAG}-a@ex.com")
cleanup() {
  [[ -n $CONV_ID ]] && q "delete from csat_survey_responses where conversation_id=$CONV_ID; delete from messages where conversation_id=$CONV_ID; delete from conversations where id=$CONV_ID" >/dev/null || true
  [[ -n $INBOX_ID && -n $SAVE_CSAT ]] && q "update inboxes set csat_survey_enabled=$SAVE_CSAT where id=$INBOX_ID" >/dev/null || true
  q "delete from notifications where account_id=$ACC and user_id=$USER_ID and meta->>'tag'='$TAG'" >/dev/null || true
  q "delete from account_users where account_id=$ACC and user_id in (select id from users where email like '${TAG}%')" >/dev/null || true
  q "delete from users where email like '${TAG}%'" >/dev/null || true
}
trap cleanup EXIT

# ---------- notifications ----------
for i in 1 2 3; do
  read -r NID < <(q "insert into notifications (account_id,user_id,notification_type,primary_actor_type,primary_actor_id,read_at,created_at,updated_at,last_activity_at,meta)
    values ($ACC,$USER_ID,1,'Conversation',1, $([[ $i == 1 ]] && echo "now()" || echo NULL), now(), now(), now(), jsonb_build_object('tag','$TAG','last_snoozed_at','2020-01-01'))
    returning id" | tr -d ' ')
  NOTIF_IDS+=("$NID")
done
N_READ=${NOTIF_IDS[0]} N_UNREAD=${NOTIF_IDS[1]} N_OTHER=${NOTIF_IDS[2]}

# unread: clears read_at, returns notification
R=$(curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/notifications/$N_READ/unread")
[[ $(echo "$R" | jq -r .read_at) == null ]] || fail "unread did not clear read_at: $R"
[[ $(q "select read_at is null from notifications where id=$N_READ") == t ]] || fail "unread not persisted"
ok "notifications/{id}/unread"

# snooze: unix timestamp stored, meta.last_snoozed_at cleared
UNTIL=$(( $(date +%s) + 3600 ))
R=$(curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/notifications/$N_UNREAD/snooze" -d "{\"snoozed_until\":$UNTIL}")
[[ $(echo "$R" | jq -r .id) == "$N_UNREAD" ]] || fail "snooze response: $R"
[[ $(q "select extract(epoch from snoozed_until)::bigint from notifications where id=$N_UNREAD") == "$UNTIL" ]] || fail "snooze not persisted"
[[ $(q "select meta->>'last_snoozed_at' is null from notifications where id=$N_UNREAD") == t ]] || fail "last_snoozed_at not cleared"
ok "notifications/{id}/snooze"
curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/notifications/$N_OTHER/snooze" -d '{"snoozed_until":null}' >/dev/null
[[ $(q "select snoozed_until is null from notifications where id=$N_OTHER") == t ]] || fail "snooze null did not clear"
ok "notifications/{id}/snooze null clears"

# unknown id: 404
CODE=$(curl -s -o /dev/null -w '%{http_code}' "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/notifications/999999999/unread")
[[ $CODE == 404 ]] || fail "unknown notification expected 404 got $CODE"
ok "unknown notification -> 404"

# destroy_all type=read: only read rows go
q "update notifications set read_at=now() where id=$N_READ" >/dev/null
curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/notifications/destroy_all?type=read" >/dev/null
[[ $(q "select count(*) from notifications where id=$N_READ") == 0 ]] || fail "read row survived type=read"
[[ $(q "select count(*) from notifications where id=$N_UNREAD") == 1 ]] || fail "unread row removed by type=read"
ok "notifications/destroy_all?type=read"

# DELETE /notifications/{id}
curl -sf "${A[@]}" -X DELETE "$B/api/v1/accounts/$ACC/notifications/$N_OTHER" >/dev/null
[[ $(q "select count(*) from notifications where id=$N_OTHER") == 0 ]] || fail "delete failed"
ok "notifications/{id} DELETE"

# destroy_all (default: all)
curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/notifications/destroy_all" >/dev/null
[[ $(q "select count(*) from notifications where account_id=$ACC and user_id=$USER_ID and meta->>'tag'='$TAG'") == 0 ]] || fail "destroy_all left rows"
ok "notifications/destroy_all (all)"

# ---------- agents bulk_create ----------
EX=$(q "select email from users where provider='email' and id=$USER_ID")
BODY=$(jq -nc --arg a "${EMAILS[0]}" --arg b "${EMAILS[1]}" --arg ex "$EX" '{emails: [$a, $b, $ex, $a]}')
curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/agents/bulk_create" -d "$BODY" >/dev/null
[[ $(q "select count(*) from users where lower(email)='${EMAILS[0]}'") == 1 ]] || fail "duplicate-in-batch created twice or not created"
[[ $(q "select count(*) from account_users au join users u on u.id=au.user_id where au.account_id=$ACC and lower(u.email)='${EMAILS[1]}'") == 1 ]] || fail "second agent not added"
[[ $(q "select count(*) from account_users au join users u on u.id=au.user_id where au.account_id=$ACC and u.id=$USER_ID") == 1 ]] || fail "existing account member changed"
ok "agents/bulk_create creates new, skips existing and duplicates"

curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/agents/bulk_create" -d "$BODY" >/dev/null
[[ $(q "select count(*) from account_users au join users u on u.id=au.user_id where au.account_id=$ACC and lower(u.email) like '${TAG}%'") == 2 ]] || fail "re-run changed agent count"
ok "agents/bulk_create idempotent"

CODE=$(curl -s -o /dev/null -w '%{http_code}' "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/agents/bulk_create" -d '{"emails":"nope"}')
[[ $CODE == 422 ]] || fail "non-array emails expected 422 got $CODE"
ok "agents/bulk_create rejects non-array"

# ---------- public csat_survey ----------
read -r INBOX_ID WT < <(q "select i.id, w.website_token from inboxes i join channel_web_widgets w on w.id=i.channel_id where i.channel_type='Channel::WebWidget' and i.account_id=$ACC order by i.id limit 1" | tr '|' ' ')
SAVE_CSAT=$(q "select coalesce(csat_survey_enabled::text,'null') from inboxes where id=$INBOX_ID")
q "update inboxes set csat_survey_enabled=true where id=$INBOX_ID" >/dev/null
VT=$(curl -sf -X POST "$B/api/v1/widget/config?website_token=$WT" | jq -r .website_channel_config.auth_token)
W=(-H "X-Auth-Token: $VT" -H 'content-type: application/json')
DISPLAY=$(curl -sf "${W[@]}" -X POST "$B/api/v1/widget/conversations?website_token=$WT" -d "{\"message\":{\"content\":\"$TAG\"}}" | jq .id)
CONV_ID=$(q "select id from conversations where account_id=$ACC and display_id=$DISPLAY")
curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/$ACC/conversations/$DISPLAY/toggle_status" -d '{"status":"resolved"}' >/dev/null
for _ in 1 2 3 4 5 6; do
  CSAT_MSG=$(q "select id from messages where conversation_id=$CONV_ID and content_type=9 limit 1")
  [[ -n $CSAT_MSG ]] && break
  sleep 1
done
[[ $CSAT_MSG =~ ^[0-9]+$ ]] || fail "csat survey message not created"
CONV_UUID=$(q "select uuid from conversations where id=$CONV_ID")

P=(-H 'content-type: application/json')
R=$(curl -sf "${P[@]}" "$B/public/api/v1/csat_survey/$CONV_UUID")
[[ $(echo "$R" | jq -r .id) == "$CSAT_MSG" ]] || fail "show id: $R"
[[ $(echo "$R" | jq -r .csat_survey_response) == null ]] || fail "response should be null before rating"
[[ $(echo "$R" | jq -r .conversation_id) == "$DISPLAY" ]] || fail "show conversation_id: $R"
ok "GET public/api/v1/csat_survey/{uuid}"

CODE=$(curl -s -o /dev/null -w '%{http_code}' "${P[@]}" "$B/public/api/v1/csat_survey/00000000-0000-0000-0000-000000000000")
[[ $CODE == 404 ]] || fail "unknown uuid expected 404 got $CODE"
ok "unknown uuid -> 404"

curl -sf "${P[@]}" -X PATCH "$B/public/api/v1/csat_survey/$CONV_UUID" -d '{"message":{"submitted_values":{"csat_survey_response":{"rating":4,"feedback_message":"nice"}}}}' >/dev/null
ok "PATCH public/api/v1/csat_survey/{uuid} rating"

# csat listener writes the row asynchronously (events are not inline in Pulsar)
for _ in 1 2 3 4 5 6; do
  RATING=$(q "select rating from csat_survey_responses where message_id=$CSAT_MSG" || true)
  [[ $RATING == 4 ]] && break
  sleep 1
done
[[ $RATING == 4 ]] || fail "csat_survey_responses not recorded (rating='$RATING')"
ok "csat_survey_responses row stored by listener"
R=$(curl -sf "${P[@]}" "$B/public/api/v1/csat_survey/$CONV_UUID")
[[ $(echo "$R" | jq -r .csat_survey_response.rating) == 4 ]] || fail "show after rating: $R"
ok "GET reflects stored rating"

CODE=$(curl -s -o /dev/null -w '%{http_code}' "${P[@]}" -X PATCH "$B/public/api/v1/csat_survey/$CONV_UUID" -d '{"message":{"submitted_values":{"csat_survey_response":{"rating":9}}}}')
[[ $CODE == 422 ]] || fail "rating 9 expected 422 got $CODE"
ok "out-of-range rating -> 422"

q "update messages set created_at=now()-interval '15 days' where id=$CSAT_MSG" >/dev/null
CODE=$(curl -s -o /dev/null -w '%{http_code}' "${P[@]}" -X PATCH "$B/public/api/v1/csat_survey/$CONV_UUID" -d '{"message":{"submitted_values":{"csat_survey_response":{"rating":1}}}}')
[[ $CODE == 422 ]] || fail "locked survey expected 422 got $CODE"
ok "survey older than 14 days is locked"

echo "ALL OK"
