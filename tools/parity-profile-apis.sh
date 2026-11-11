#!/usr/bin/env bash
# Parity checks for profile, widget config, direct uploads, year in review, onboarding.
# Usage: BASE=http://127.0.0.1:3103 tools/parity-profile-apis.sh
# Prints PASS/FAIL per endpoint and exits non-zero if any check fails.
set -uo pipefail
B=${BASE:-http://127.0.0.1:3000}
H=$(mktemp)
trap 'rm -f "$H" "$H.txt" "$H.png"' EXIT
PASS=0
FAIL=0

check() { # name, expected-http-code, actual-http-code, body-grep-pattern(optional), body
  local name=$1 want=$2 got=$3 pat=${4:-} body=${5:-}
  if [ "$got" = "$want" ] && { [ -z "$pat" ] || printf '%s' "$body" | grep -q -- "$pat"; }; then
    echo "PASS $name ($got)"; PASS=$((PASS + 1))
  else
    echo "FAIL $name want=$want got=$got pattern=$pat"; FAIL=$((FAIL + 1))
  fi
}

curl -sf -D "$H" -o /dev/null -H 'content-type: application/json' \
  -d '{"email":"admin@example.com","password":"Password123!"}' "$B/auth/sign_in" || { echo "FAIL sign_in"; exit 1; }
hdr() { grep -i "^$1:" "$H" | cut -d' ' -f2 | tr -d '\r'; }
AT=$(hdr access-token); CL=$(hdr client); UID_H=$(hdr uid)
AUTH=(-H "access-token: $AT" -H "client: $CL" -H "uid: $UID_H")
A="$B/api/v1/accounts/1"

req() { # method, url, extra-curl-args...; sets CODE and BODY
  local m=$1 u=$2; shift 2
  local out
  out=$(curl -sS -w '\n%{http_code}' -X "$m" "${AUTH[@]}" "$@" "$u")
  CODE=$(printf '%s' "$out" | tail -n1)
  BODY=$(printf '%s' "$out" | sed '$d')
}

# ---- profile ----
req GET "$B/api/v1/profile"
check "GET /api/v1/profile" 200 "$CODE" '"available_name"' "$BODY"

req PUT "$B/api/v1/profile" -H 'content-type: application/json' -d '{"profile":{"display_name":"Parity Admin"}}'
check "PUT /api/v1/profile (json)" 200 "$CODE" '"display_name":"Parity Admin"' "$BODY"

req PUT "$B/api/v1/profile" -H 'content-type: application/json' -d '{"profile":{"password":"x","current_password":"wrong"}}'
check "PUT /api/v1/profile wrong current password" 422 "$CODE" 'incorrect' "$BODY"

printf '\x89PNG\r\n\x1a\n' > "$H.png"
req PUT "$B/api/v1/profile" -F 'profile[display_name]=Parity Admin' -F "profile[avatar]=@$H.png;type=image/png;filename=a.png"
check "PUT /api/v1/profile multipart avatar" 200 "$CODE" '"avatar_url":"/rails' "$BODY"

req DELETE "$B/api/v1/profile/avatar"
check "DELETE /api/v1/profile/avatar" 200 "$CODE" '"avatar_url":""' "$BODY"

req POST "$B/api/v1/profile/resend_confirmation"
check "POST /api/v1/profile/resend_confirmation" 200 "$CODE"

req POST "$B/api/v1/profile/reset_access_token"
check "POST /api/v1/profile/reset_access_token" 200 "$CODE" '"access_token"' "$BODY"

# ---- widget config ----
req GET "$A/inboxes"
WIDGET_INBOX_TOKEN=$(printf '%s' "$BODY" | python3 -c 'import sys,json
d=json.load(sys.stdin)["payload"]
w=[i for i in d if i.get("channel_type")=="Channel::WebWidget"]
print(w[0]["website_token"] if w else "")' 2>/dev/null)
if [ -z "$WIDGET_INBOX_TOKEN" ]; then
  req POST "$A/inboxes" -H 'content-type: application/json' -d '{"name":"Parity Web","channel":{"type":"web_widget","website_url":"https://parity.example.com"}}'
  WIDGET_INBOX_TOKEN=$(printf '%s' "$BODY" | python3 -c 'import sys,json;print(json.load(sys.stdin)["channel"]["website_token"])' 2>/dev/null)
fi
TOKEN_Q="website_token=$WIDGET_INBOX_TOKEN"

req_plain() { local out; out=$(curl -sS -w '\n%{http_code}' -X POST -H 'content-type: application/json' "$@"); CODE=$(printf '%s' "$out" | tail -n1); BODY=$(printf '%s' "$out" | sed '$d'); }
req_plain "$B/api/v1/widget/config?$TOKEN_Q" -d '{}'
check "POST /api/v1/widget/config" 200 "$CODE" '"website_channel_config"' "$BODY"
check "widget config enabled_languages has 44" 200 "$CODE" '"iso_639_1_code":"sl"' "$BODY"
check "widget config utc_off_set present" 200 "$CODE" '"utc_off_set":"[+-][0-9][0-9]:[0-9][0-9]"' "$BODY"
check "widget config global_config present" 200 "$CODE" '"global_config"' "$BODY"

# ---- widget direct upload ----
printf 'hello' > "$H.txt"
req POST "$B/api/v1/widget/direct_uploads?$TOKEN_Q" -H 'content-type: application/json' -d '{"blob":{"filename":"a.txt","content_type":"text/plain","byte_size":5,"checksum":"x"}}'
check "POST widget direct_uploads without visitor token" 401 "$CODE"

VISITOR=$(curl -sS -X POST -H 'content-type: application/json' "$B/api/v1/widget/config?$TOKEN_Q" -d '{}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["website_channel_config"]["auth_token"])' 2>/dev/null)
DU=$(curl -sS -w '\n%{http_code}' -X POST -H 'content-type: application/json' -H "X-Auth-Token: $VISITOR" "$B/api/v1/widget/direct_uploads?$TOKEN_Q" -d '{"blob":{"filename":"a.txt","content_type":"text/plain","byte_size":5,"checksum":"x"}}')
CODE=$(printf '%s' "$DU" | tail -n1); BODY=$(printf '%s' "$DU" | sed '$d')
check "POST widget direct_uploads with visitor token" 200 "$CODE" '"direct_upload"' "$BODY"
UPLOAD_URL=$(printf '%s' "$BODY" | python3 -c 'import sys,json;print(json.load(sys.stdin)["direct_upload"]["url"])' 2>/dev/null)
SIGNED=$(printf '%s' "$BODY" | python3 -c 'import sys,json;print(json.load(sys.stdin)["signed_id"])' 2>/dev/null)
PUT_CODE=$(curl -sS -o /dev/null -w '%{http_code}' -X PUT -H 'content-type: text/plain' --data-binary @"$H.txt" "$B$UPLOAD_URL")
check "PUT direct upload bytes" 204 "$PUT_CODE"
BAD_CODE=$(curl -sS -o /dev/null -w '%{http_code}' -X PUT --data-binary @"$H.txt" "$B${UPLOAD_URL%%\?*}?token=bad")
check "PUT direct upload bad token rejected" 403 "$BAD_CODE"
[ -n "$SIGNED" ] && SIGNED_OK=1 || SIGNED_OK=0
check "direct upload returned signed_id" 1 "$SIGNED_OK"

# ---- year in review ----
req GET "$B/api/v2/accounts/1/year_in_review"
check "GET /api/v2/accounts/1/year_in_review" 200 "$CODE" '"support_personality"' "$BODY"
check "year_in_review default year 2025" 200 "$CODE" '"year":2025' "$BODY"

req GET "$B/api/v2/accounts/1/year_in_review?year=2024"
check "year_in_review year=2024" 200 "$CODE" '"year":2024' "$BODY"

# ---- onboarding (self-host) ----
req PUT "$A/onboarding" -H 'content-type: application/json' -d '{"onboarding_step":"bogus"}'
check "PUT onboarding invalid step" 422 "$CODE" 'Invalid onboarding step' "$BODY"

req GET "$A/onboarding/help_center_generation"
check "GET onboarding/help_center_generation" 200 "$CODE" '"generation_id":null' "$BODY"

req GET "$A"
NAME_BEFORE=$(printf '%s' "$BODY" | python3 -c 'import sys,json;print(json.load(sys.stdin)["name"])' 2>/dev/null)
req PUT "$A/onboarding" -H 'content-type: application/json' -d '{"onboarding_step":"account_details","name":"Parity Acme"}'
NAME_AFTER=$(printf '%s' "$BODY" | python3 -c 'import sys,json;print(json.load(sys.stdin)["name"])' 2>/dev/null)
check "PUT onboarding account_details without cursor returns account" 200 "$CODE" '"id":1' "$BODY"
[ -n "$NAME_BEFORE" ] && [ "$NAME_BEFORE" = "$NAME_AFTER" ] && NOOP=1 || NOOP=0
check "PUT onboarding account_details without cursor leaves name unchanged" 1 "$NOOP"

echo "---"
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
