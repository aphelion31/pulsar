#!/usr/bin/env bash
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:3161}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
TOKEN=${TOKEN:-$(psql "$DB" -qtAX -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")}
[[ -n $TOKEN ]] || { echo 'Missing account 1 user token' >&2; exit 1; }
check() {
  local method=$1 path=$2 code
  code=$(curl -sS -o /dev/null -w '%{http_code}' -X "$method" -H "api_access_token: $TOKEN" -H 'Content-Type: application/json' --data '{}' "$BASE$path")
  echo "$method $path $code"
  [[ $code != 405 && $code != 501 ]] || exit 1
}
a=/api/v1/accounts/1
for path in conversations/999999999 conversations/999999999/messages/999999999 conversations/999999999/participants agents/999999999 notifications/999999999 integrations/hooks/999999999; do check PUT "$a/$path"; done
check GET "$a/integrations/hooks/999999999"
check PATCH "$a/onboarding"
check PATCH /api/v1/profile
check DELETE /api/v1/notification_subscriptions
check PUT /api/v1/widget/messages/999999999
check PUT /api/v1/widget/contact
check DELETE /api/v1/widget/labels/verb-parity-missing
p=/public/api/v1/inboxes/verb-parity-missing/contacts/verb-parity-missing
check PUT "$p"
check PUT "$p/conversations/999999999/messages/999999999"
# Only our unique test subscription is touched, even if the test fails.
marker="verb-parity-$(uuidgen)"
cleanup() { echo "DELETE FROM notification_subscriptions WHERE subscription_attributes->>'push_token' = :'marker'" | psql "$DB" -q -v ON_ERROR_STOP=1 -v marker="$marker"; }
trap cleanup EXIT
echo "INSERT INTO notification_subscriptions (user_id, subscription_type, subscription_attributes, created_at, updated_at) VALUES (1, 0, jsonb_build_object('push_token', :'marker'), now(), now())" | psql "$DB" -q -v ON_ERROR_STOP=1 -v marker="$marker"
code=$(curl -sS -o /dev/null -w '%{http_code}' -X DELETE -H "api_access_token: $TOKEN" -H 'Content-Type: application/json' --data "{\"push_token\":\"$marker\"}" "$BASE/api/v1/notification_subscriptions")
[[ $code == 200 ]]
[[ $(echo "SELECT count(*) FROM notification_subscriptions WHERE subscription_attributes->>'push_token' = :'marker'" | psql "$DB" -qtAX -v marker="$marker") == 0 ]]
echo 'subscription deletion: ok'
