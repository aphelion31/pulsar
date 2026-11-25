#!/usr/bin/env bash
# PLR-065: message update/retry/translate, request_contact_info, inbox avatar + campaigns,
# widget custom attributes, transcript, set_user HMAC, widget labels.
# Usage: BASE=http://127.0.0.1:3143 bash tools/messages-r2.sh
set -euo pipefail

B=${BASE:-http://127.0.0.1:3143}
DB=${DB_URL:-postgres://localhost/pulsar}
q() { psql -qtAX "$DB" -c "$1"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok: $*"; }

TOKEN=$(q "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
[ -n "$TOKEN" ] || fail "no api token for user 1"
H=(-H "api_access_token: $TOKEN" -H 'content-type: application/json')

API_INBOX=$(q "select id from inboxes where account_id=1 and channel_type='Channel::Api' order by id limit 1")
WEB_INBOX=$(q "select i.id from inboxes i where i.account_id=1 and i.channel_type='Channel::WebWidget' order by id limit 1")
WT=$(q "select w.website_token from channel_web_widgets w join inboxes i on i.channel_id=w.id and i.channel_type='Channel::WebWidget' where i.id=$WEB_INBOX")
[ -n "$API_INBOX" ] && [ -n "$WEB_INBOX" ] && [ -n "$WT" ] || fail "seed inboxes missing"

CREATED_CONV=""
CREATED_CONTACT=""
CREATED_LABEL=""
# hmac_token is changed by the set_user check. Keep the original value (NULL or not) and restore it on exit.
ORIG_HMAC_NULL=$(q "select hmac_token is null from channel_web_widgets where website_token='$WT'")
ORIG_HMAC=$(q "select coalesce(hmac_token,'') from channel_web_widgets where website_token='$WT'")
cleanup() {
  if [ "$ORIG_HMAC_NULL" = "t" ]; then
    q "update channel_web_widgets set hmac_token=null where website_token='$WT'" >/dev/null
  else
    q "update channel_web_widgets set hmac_token='$ORIG_HMAC' where website_token='$WT'" >/dev/null
  fi
  if [ -n "$CREATED_CONTACT" ]; then
    q "delete from messages where conversation_id in (select id from conversations where contact_inbox_id in (select id from contact_inboxes where contact_id=$CREATED_CONTACT))" >/dev/null
    q "delete from conversations where contact_inbox_id in (select id from contact_inboxes where contact_id=$CREATED_CONTACT)" >/dev/null
    q "delete from pulsar_widget_token_floors where contact_inbox_id in (select id from contact_inboxes where contact_id=$CREATED_CONTACT)" >/dev/null
    q "delete from contact_inboxes where contact_id=$CREATED_CONTACT" >/dev/null
    q "delete from contacts where id=$CREATED_CONTACT" >/dev/null
  fi
  [ -n "$CREATED_CONV" ] && q "delete from messages where conversation_id=$CREATED_CONV" >/dev/null
  [ -n "$CREATED_CONV" ] && q "delete from conversations where id=$CREATED_CONV" >/dev/null
  [ -n "$CREATED_LABEL" ] && q "delete from labels where id=$CREATED_LABEL" >/dev/null
  true
}
trap cleanup EXIT

# ---- API inbox conversation with one outgoing failed message ----
CONV_ID=$(curl -sf -X POST "$B/api/v1/accounts/1/conversations" "${H[@]}" \
  -d "{\"inbox_id\":$API_INBOX,\"contact_id\":null,\"status\":\"open\"}" 2>/dev/null | jq -r '.id // empty' || true)
if [ -z "$CONV_ID" ]; then
  CONV_ID=$(q "insert into conversations (account_id, inbox_id, status, contact_id, contact_inbox_id, additional_attributes, custom_attributes, created_at, updated_at, last_activity_at, display_id)
    values (1, $API_INBOX, 0, null, null, '{}', '{}', now(), now(), now(), (select coalesce(max(display_id),0)+1 from conversations where account_id=1))
    returning id")
  CREATED_CONV=$CONV_ID
  DISPLAY=$(q "select display_id from conversations where id=$CONV_ID")
else
  DISPLAY=$(q "select display_id from conversations where id=$CONV_ID")
  CREATED_CONV=$CONV_ID
fi
MSG_ID=$(q "insert into messages (content, account_id, inbox_id, conversation_id, message_type, private, status, content_type, created_at, updated_at, sender_type, sender_id, additional_attributes, content_attributes, external_source_ids)
  values ('hello', 1, $API_INBOX, $CONV_ID, 1, false, 3, 0, now(), now(), 'User', 1, '{}', '{}', '{}') returning id")

# ---- PATCH status: forward then backward ----
R=$(curl -sf -X PATCH "$B/api/v1/accounts/1/conversations/$DISPLAY/messages/$MSG_ID" "${H[@]}" -d '{"status":"delivered"}')
echo "$R" | jq -e '.status=="delivered"' >/dev/null || fail "patch status forward: $R"
ok "PATCH message status forward"
R=$(curl -sf -X PATCH "$B/api/v1/accounts/1/conversations/$DISPLAY/messages/$MSG_ID" "${H[@]}" -d '{"status":"sent"}')
echo "$R" | jq -e '.status=="delivered"' >/dev/null || fail "backward transition should be ignored: $R"
ok "PATCH message status backward ignored"

# inbox that is not API must be refused (web widget conversation)
WEB_CONV=$(q "select display_id from conversations where account_id=1 and inbox_id=$WEB_INBOX order by id desc limit 1")
if [ -n "$WEB_CONV" ]; then
  CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PATCH "$B/api/v1/accounts/1/conversations/$WEB_CONV/messages/1" "${H[@]}" -d '{"status":"read"}')
  [ "$CODE" = "403" ] || [ "$CODE" = "404" ] || fail "non-API inbox patch expected 403/404, got $CODE"
  ok "PATCH message refused on non-API inbox ($CODE)"
fi

# ---- retry: failed -> sent, re-emitted ----
q "update messages set status=3 where id=$MSG_ID" >/dev/null
R=$(curl -sf -X POST "$B/api/v1/accounts/1/conversations/$DISPLAY/messages/$MSG_ID/retry" "${H[@]}")
echo "$R" | jq -e '.status=="sent"' >/dev/null || fail "retry should reset to sent: $R"
ok "POST retry failed message"
R=$(curl -sf -X POST "$B/api/v1/accounts/1/conversations/$DISPLAY/messages/$MSG_ID/retry" "${H[@]}")
echo "$R" | jq -e '.status=="sent"' >/dev/null || fail "retry on non-failed should be a no-op: $R"
ok "POST retry on non-failed is a no-op"

# ---- translate: no Google key, clear 422 ----
CODE=$(curl -s -o /tmp/plr065-translate.$$ -w "%{http_code}" -X POST "$B/api/v1/accounts/1/conversations/$DISPLAY/messages/$MSG_ID/translate" "${H[@]}")
[ "$CODE" = "422" ] || fail "translate expected 422, got $CODE"
jq -e '.error=="translation not configured"' /tmp/plr065-translate.$$ >/dev/null || fail "translate error body"
rm -f /tmp/plr065-translate.$$
ok "POST translate -> 422 translation not configured"

# ---- request_contact_info: not WhatsApp -> refused ----
CODE=$(curl -s -o /tmp/plr065-cir.$$ -w "%{http_code}" -X POST "$B/api/v1/accounts/1/conversations/$DISPLAY/request_contact_info" "${H[@]}")
[ "$CODE" = "422" ] || fail "request_contact_info on Api inbox expected 422, got $CODE"
rm -f /tmp/plr065-cir.$$
ok "POST request_contact_info refused on non-WhatsApp"

# ---- inbox avatar delete + campaigns list ----
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$B/api/v1/accounts/1/inboxes/$API_INBOX/avatar" "${H[@]}")
[ "$CODE" = "200" ] || fail "inbox avatar delete expected 200, got $CODE"
ok "DELETE inbox avatar"
R=$(curl -sf "$B/api/v1/accounts/1/inboxes/$API_INBOX/campaigns" "${H[@]}")
echo "$R" | jq -e 'type=="array"' >/dev/null || fail "inbox campaigns not an array: $R"
ok "GET inbox campaigns"

# ---- widget: visitor session ----
VT=$(curl -sf -X POST "$B/api/v1/widget/config?website_token=$WT" | jq -r '.website_channel_config.auth_token')
[ -n "$VT" ] && [ "$VT" != "null" ] || fail "widget config token"
# POST /config creates an anonymous visitor contact. Its id is read from the widget contact endpoint and
# recorded before any other step so cleanup always removes it.
CONTACT_ID=$(curl -sf "$B/api/v1/widget/contact?website_token=$WT" -H "X-Auth-Token: $VT" | jq -r '.id')
[ -n "$CONTACT_ID" ] && [ "$CONTACT_ID" != "null" ] || fail "visitor contact id"
CREATED_CONTACT=$CONTACT_ID

# ---- widget: set_user without HMAC on hmac_mandatory inbox is refused; with bad hash too ----
q "update channel_web_widgets set hmac_token='plr065-secret' where website_token='$WT'" >/dev/null
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PATCH "$B/api/v1/widget/contact/set_user?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d '{"identifier":"plr065-user","identifier_hash":"deadbeef"}')
[ "$CODE" = "401" ] || fail "set_user bad hash expected 401, got $CODE"
ok "PATCH set_user rejects bad identifier_hash"

HASH=$(printf 'plr065-user' | openssl dgst -sha256 -hmac 'plr065-secret' | awk '{print $NF}')
R=$(curl -sf -X PATCH "$B/api/v1/widget/contact/set_user?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d "{\"identifier\":\"plr065-user\",\"identifier_hash\":\"$HASH\",\"name\":\"PLR065\"}")
echo "$R" | jq -e '.id and .widget_auth_token and .pubsub_token' >/dev/null || fail "set_user valid hash: $R"
VT=$(echo "$R" | jq -r '.widget_auth_token')
ok "PATCH set_user with valid identifier_hash"
q "update channel_web_widgets set hmac_token=null where website_token='$WT'" >/dev/null

# ---- widget: custom attributes on the contact ----
R=$(curl -sf -X POST "$B/api/v1/widget/contact/destroy_custom_attributes?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d '{"custom_attributes":["plr065_key"]}')
echo "$R" | jq -e '.id' >/dev/null || fail "contact destroy_custom_attributes: $R"
ok "POST widget contact destroy_custom_attributes"

# ---- widget: conversation custom attributes ----
curl -sf -X POST "$B/api/v1/widget/messages?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d '{"message":{"content":"plr065 hi"}}' >/dev/null || fail "widget message create"
W_CONV=$(q "select id from conversations where contact_inbox_id in (select id from contact_inboxes where contact_id=$CONTACT_ID) order by id desc limit 1")
if [ -n "$W_CONV" ]; then
  R=$(curl -sf -X POST "$B/api/v1/widget/conversations/set_custom_attributes?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d '{"custom_attributes":{"plr065_a":"1","plr065_b":"2"}}')
  echo "$R" | jq -e '.custom_attributes.plr065_a=="1"' >/dev/null || fail "conv set_custom_attributes: $R"
  ok "POST widget conversations set_custom_attributes"
  R=$(curl -sf -X POST "$B/api/v1/widget/conversations/destroy_custom_attributes?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d '{"custom_attribute":["plr065_a"]}')
  echo "$R" | jq -e '.custom_attributes | has("plr065_a")|not' >/dev/null || fail "conv destroy_custom_attributes: $R"
  ok "POST widget conversations destroy_custom_attributes"

  # ---- widget labels: only labels defined in the account ----
  CREATED_LABEL=$(q "insert into labels (title, description, color, show_on_sidebar, account_id, created_at, updated_at) values ('plr065-label','', '#1f93ff', true, 1, now(), now()) returning id")
  CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$B/api/v1/widget/labels?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d '{"label":"plr065-label"}')
  [ "$CODE" = "204" ] || fail "widget label add expected 204, got $CODE"
  q "select cached_label_list from conversations where id=$W_CONV" | grep -q "plr065-label" || fail "label not added to visitor conversation"
  ok "POST widget labels adds defined label"
  CODE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$B/api/v1/widget/labels?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d '{"label":"plr065-label"}')
  [ "$CODE" = "204" ] || fail "widget label remove expected 204, got $CODE"
  q "select coalesce(cached_label_list,'') from conversations where id=$W_CONV" | grep -qv "plr065-label" || fail "label not removed"
  ok "DELETE widget labels removes label"
  q "delete from labels where id=$CREATED_LABEL" >/dev/null; CREATED_LABEL=""
fi

# ---- widget transcript: no contact email -> nothing sent, 200 ----
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$B/api/v1/widget/conversations/transcript?website_token=$WT" -H "X-Auth-Token: $VT" -H 'content-type: application/json' -d '{"email":"x@example.com"}')
[ "$CODE" = "200" ] || fail "transcript expected 200, got $CODE"
ok "POST widget transcript (no contact email)"

echo "ALL PASS"
