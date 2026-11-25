#!/usr/bin/env bash
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:3113}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
identifier=$(psql "$DB" -Atc "SELECT a.identifier FROM channel_api a JOIN inboxes i ON i.channel_id=a.id AND i.channel_type='Channel::Api' AND i.account_id=a.account_id WHERE a.identifier IS NOT NULL AND coalesce(a.hmac_mandatory,false)=false LIMIT 1")
[[ -n "$identifier" ]] || { echo 'FAIL: no non-HMAC API inbox'; exit 1; }
root="$BASE/public/api/v1/inboxes/$identifier"
request() { curl -fsS -H 'Content-Type: application/json' "$@"; }
request "$root" >/dev/null; echo 'PASS inbox'
[[ $(curl -s -o /dev/null -w '%{http_code}' "$BASE/public/api/v1/inboxes/not-a-real-identifier") == 404 ]]; echo 'PASS wrong inbox'
c1=$(request -X POST -d '{"name":"Public API smoke"}' "$root/contacts" | jq -er .source_id)
c2=$(request -X POST -d '{"name":"Public API smoke other"}' "$root/contacts" | jq -er .source_id)
echo 'PASS contacts'
conv=$(request -X POST -d '{}' "$root/contacts/$c1/conversations" | jq -er .id); echo 'PASS conversation'
[[ $(curl -s -o /dev/null -w '%{http_code}' "$root/contacts/$c2/conversations/$conv") == 404 ]]; echo 'PASS contact isolation'
msgs="$root/contacts/$c1/conversations/$conv/messages"
request -X POST -d '{"content":"Public API smoke message"}' "$msgs" | jq -e '.message_type == 0 and .content == "Public API smoke message"' >/dev/null; echo 'PASS message create'
request "$msgs" | jq -e 'any(.[]; .content == "Public API smoke message")' >/dev/null; echo 'PASS messages list'
