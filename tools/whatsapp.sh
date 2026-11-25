#!/usr/bin/env bash
set -euo pipefail
B=${BASE:-http://127.0.0.1:3191}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
TOKEN=$(psql -qtAX "$DB" -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")
PHONE="+1999$(date +%s)"
INBOX=''; CHANNEL=''
cleanup() {
  if [[ $INBOX =~ ^[0-9]+$ && $CHANNEL =~ ^[0-9]+$ ]]; then
    psql -q "$DB" -v inbox="$INBOX" -v channel="$CHANNEL" <<'SQL'
DELETE FROM messages WHERE inbox_id = :inbox;
DELETE FROM conversations WHERE inbox_id = :inbox;
CREATE TEMP TABLE wa_test_contacts AS SELECT contact_id FROM contact_inboxes WHERE inbox_id = :inbox;
DELETE FROM contact_inboxes WHERE inbox_id = :inbox;
DELETE FROM contacts WHERE id IN (SELECT contact_id FROM wa_test_contacts) AND NOT EXISTS (SELECT 1 FROM contact_inboxes WHERE contact_id = contacts.id);
DELETE FROM inbox_members WHERE inbox_id = :inbox;
DELETE FROM inboxes WHERE id = :inbox;
DELETE FROM channel_whatsapp WHERE id = :channel;
SQL
  fi
}
trap cleanup EXIT
A="$B/api/v1/accounts/1"
AUTH=(-H "api_access_token: $TOKEN" -H 'content-type: application/json')
result=$(curl -fsS "${AUTH[@]}" -d "{\"name\":\"WA test\",\"phone_number\":\"$PHONE\",\"provider_config\":{\"api_key\":\"test-token\",\"app_secret\":\"test-secret\",\"phone_number_id\":\"123\",\"business_account_id\":\"456\",\"webhook_verify_token\":\"verify\"}}" "$A/channels/whatsapp_channel")
INBOX=$(jq -er '.id' <<<"$result"); CHANNEL=$(jq -er '.channel_id' <<<"$result")
[[ $(curl -fsS -G --data-urlencode 'hub.mode=subscribe' --data-urlencode 'hub.verify_token=verify' --data-urlencode 'hub.challenge=challenge' "$B/webhooks/whatsapp/$PHONE") == challenge ]]
[[ $(curl -s -o /dev/null -w '%{http_code}' -X POST -d '{}' "$B/webhooks/whatsapp/$PHONE") == 401 ]]
sender="${PHONE#+}"
payload=$(jq -nc --arg sender "$sender" '{object:"whatsapp_business_account",entry:[{changes:[{field:"messages",value:{metadata:{phone_number_id:"123"},contacts:[{wa_id:$sender,profile:{name:"Test"}}],messages:[{id:"wa-test-id",from:$sender,type:"text",text:{body:"hello whatsapp"}}]}}]}]}')
sig=$(printf %s "$payload" | openssl dgst -sha256 -hmac test-secret | awk '{print $NF}')
curl -fsS -H "X-Hub-Signature-256: sha256=$sig" -H 'content-type: application/json' -d "$payload" "$B/webhooks/whatsapp/$PHONE" >/dev/null
[[ $(psql -qtAX "$DB" -c "SELECT count(*) FROM messages WHERE inbox_id = $INBOX AND source_id = 'wa-test-id' AND content = 'hello whatsapp'") == 1 ]]
curl -fsS "${AUTH[@]}" -X POST "$A/inboxes/$INBOX/sync_templates" | jq -e '.message' >/dev/null
curl -fsS "${AUTH[@]}" "$A/inboxes/$INBOX/health" | jq -e '.id == "123" and .business_account_id == "456"' >/dev/null
echo 'WhatsApp endpoint checks passed'
