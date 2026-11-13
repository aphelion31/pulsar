#!/usr/bin/env bash
# CSAT: resolving a conversation in a CSAT-enabled inbox sends one input_csat survey message; the visitor's
# rating (widget PATCH /messages/:id) is stored in csat_survey_responses. Restores the inbox flag.
set -euo pipefail
B=${BASE:-http://127.0.0.1:3110}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
q() { psql -qtAX "$DB" -c "$1"; }
read -r INBOX WT < <(q "select i.id, w.website_token from inboxes i join channel_web_widgets w on w.id=i.channel_id where i.channel_type='Channel::WebWidget' and i.account_id=1 order by i.id limit 1" | tr '|' ' ')
SAVE=$(q "select coalesce(csat_survey_enabled::text,'null') from inboxes where id=$INBOX")
TOKEN=$(q "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
CONV=
cleanup() {
  q "update inboxes set csat_survey_enabled=$SAVE where id=$INBOX" >/dev/null
  if [[ -n $CONV ]]; then q "delete from csat_survey_responses where conversation_id=$CONV; delete from messages where conversation_id=$CONV; delete from conversations where id=$CONV" >/dev/null; fi
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
q "update inboxes set csat_survey_enabled=true where id=$INBOX" >/dev/null

VT=$(curl -sf -X POST "$B/api/v1/widget/config?website_token=$WT" | jq -r .website_channel_config.auth_token)
W=(-H "X-Auth-Token: $VT" -H 'content-type: application/json')
DISPLAY=$(curl -sf "${W[@]}" -X POST "$B/api/v1/widget/conversations?website_token=$WT" -d '{"message":{"content":"csat test"}}' | jq .id)
CONV=$(q "select id from conversations where account_id=1 and display_id=$DISPLAY")
A=(-H "api_access_token: $TOKEN" -H 'content-type: application/json')
curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/1/conversations/$DISPLAY/toggle_status" -d '{"status":"resolved"}' >/dev/null
sleep 1.5
MID=$(q "select id from messages where conversation_id=$CONV and content_type=9")
[[ $MID =~ ^[0-9]+$ ]] || fail "expected one survey message, got '$MID'"
q "select message_type from messages where id=$MID" | grep -qx 3 || fail "survey is not a template message"

# resolving again (reopen + resolve) must not send a second survey
curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/1/conversations/$DISPLAY/toggle_status" -d '{"status":"open"}' >/dev/null
curl -sf "${A[@]}" -X POST "$B/api/v1/accounts/1/conversations/$DISPLAY/toggle_status" -d '{"status":"resolved"}' >/dev/null
sleep 1.5
[[ $(q "select count(*) from messages where conversation_id=$CONV and content_type=9") == 1 ]] || fail "survey sent twice"

curl -sf "${W[@]}" -X PATCH "$B/api/v1/widget/messages/$MID?website_token=$WT" -d '{"message":{"submitted_values":{"csat_survey_response":{"rating":4,"feedback_message":"good"}}}}' >/dev/null
sleep 1.5
q "select rating||'|'||feedback_message from csat_survey_responses where message_id=$MID" | grep -qx '4|good' || fail "rating not stored"
echo "csat: survey on resolve, no duplicate, rating stored OK"
