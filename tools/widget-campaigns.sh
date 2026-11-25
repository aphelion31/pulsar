#!/usr/bin/env bash
# Live chat campaigns: widget lists enabled campaigns; campaign.triggered opens one conversation with
# the campaign message, only for a contact with no conversation yet. Cleans up after itself.
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:3110}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
q() { psql -qtAX "$DB" -c "$1"; }
TOKEN=$(q "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
read -r INBOX WT < <(q "select i.id, w.website_token from inboxes i join channel_web_widgets w on w.id=i.channel_id where i.channel_type='Channel::WebWidget' and i.account_id=1 limit 1" | tr '|' ' ')
H=(-H "api_access_token: $TOKEN" -H 'content-type: application/json')
CID=$(curl -sf "${H[@]}" -X POST "$BASE/api/v1/accounts/1/campaigns" -d "{\"title\":\"t\",\"message\":\"Hello from campaign\",\"inbox_id\":$INBOX,\"sender_id\":1,\"enabled\":true,\"trigger_rules\":{\"url\":\"*\",\"time_on_page\":3}}" | jq .id)
cleanup() { curl -s "${H[@]}" -X DELETE "$BASE/api/v1/accounts/1/campaigns/$CID" >/dev/null; }
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

curl -sf "$BASE/api/v1/widget/campaigns?website_token=$WT" | jq -e "map(select(.id==$CID and .message==\"Hello from campaign\" and .sender.id==1)) | length == 1" >/dev/null || fail "campaign not listed"

VT=$(curl -sf -X POST "$BASE/api/v1/widget/config?website_token=$WT" | jq -r '.website_channel_config.auth_token')
[[ -n $VT && $VT != null ]] || fail "no visitor token"
ev() { curl -sf -X POST "$BASE/api/v1/widget/events?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d "{\"name\":\"campaign.triggered\",\"event_info\":{\"campaign_id\":$CID,\"referer\":\"https://x.test/p\"}}" >/dev/null; }
ev; ev  # second trigger must not create another conversation
DB_CID=$(q "select id from campaigns where display_id=$CID and account_id=1")
N=$(q "select count(*) from conversations where campaign_id=$DB_CID")
[[ $N == 1 ]] || fail "expected 1 campaign conversation, got $N"
q "select count(*) from messages m join conversations c on c.id=m.conversation_id where c.campaign_id=$DB_CID and m.message_type=1 and m.content='Hello from campaign' and m.additional_attributes->>'campaign_id'='$DB_CID' and m.sender_type='User'" | grep -qx 1 || fail "campaign message missing"
q "select additional_attributes->>'referer' from conversations where campaign_id=$DB_CID" | grep -qx 'https://x.test/p' || fail "additional attributes missing"
q "delete from messages where conversation_id in (select id from conversations where campaign_id=$DB_CID); delete from conversations where campaign_id=$DB_CID;" >/dev/null
echo "widget campaigns: list, trigger, single conversation, message attrs OK"
