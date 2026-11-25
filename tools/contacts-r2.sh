#!/usr/bin/env bash
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:3142}
TOKEN=${TOKEN:-$(psql -qtAX postgres://localhost/pulsar -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")}
API="$BASE/api/v1/accounts/1"
key="plr064-$$"
file=$(mktemp "${JCODE_SCRATCH_DIR:-/tmp}/contacts-r2.XXXXXX")
cleanup() { printf '%s\n' "delete from contacts where account_id=1 and identifier=:'k';" | psql -q postgres://localhost/pulsar -v k="$key" >/dev/null; rm -f "$file"; }
trap cleanup EXIT
printf 'name,email,identifier,city\n"Test, Import",%s@example.invalid,%s,Testville\n' "$key" "$key" > "$file"
curl -fsS -H "api_access_token: $TOKEN" -F "import_file=@$file" "$API/contacts/import" | jq -e '.processed_records == 1' >/dev/null
curl -fsS -H "api_access_token: $TOKEN" -H 'Content-Type: application/json' -d "{\"payload\":[{\"attribute_key\":\"identifier\",\"filter_operator\":\"equal_to\",\"values\":[\"$key\"]}]}" "$API/contacts/filter" | jq -e '.meta.count == 1 and .payload[0].additional_attributes.city == "Testville"' >/dev/null
curl -fsS -H "api_access_token: $TOKEN" -H 'Content-Type: application/json' -d '{"payload":[{"attribute_key":"name","filter_operator":"equal_to","values":["x\u0027 OR 1=1 --"]}]}' "$API/contacts/filter" | jq -e '.meta.count == 0' >/dev/null
curl -fsS -X POST -H "api_access_token: $TOKEN" "$API/contacts/export" | grep -F "$key" >/dev/null
code=$(curl -sS -o /dev/null -w '%{http_code}' -H "api_access_token: $TOKEN" -H 'Content-Type: application/json' -d '{"inbox_id":-1,"source_id":"none"}' "$API/contact_inboxes/filter")
[ "$code" = 404 ]
echo 'contacts-r2: passed'
