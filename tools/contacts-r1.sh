#!/usr/bin/env bash
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:3132}
TOKEN=${TOKEN:-$(psql -qtAX postgres://localhost/pulsar -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")}
CONTACT=$(psql -qtAX postgres://localhost/pulsar -c 'select id from contacts where account_id=1 limit 1')
[[ -n "$TOKEN" && -n "$CONTACT" ]]
curl -fsS -H "api_access_token: $TOKEN" "$BASE/api/v1/accounts/1/contacts/$CONTACT/attachments" | jq -e '.meta.total_count >= 0 and (.payload | type == "array")' >/dev/null
[[ $(curl -s -o /dev/null -w '%{http_code}' -H "api_access_token: $TOKEN" "$BASE/api/v1/accounts/1/contacts/2147483647/attachments") == 404 ]]
echo 'contact attachments: PASS'
