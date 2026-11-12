#!/usr/bin/env bash
# Parity checks for team members, integration hooks and CSAT responses.
# Usage: BASE=http://127.0.0.1:3112 tools/parity-hooks-csat-teams.sh
# Prints PASS/FAIL per endpoint and exits non-zero if any check fails.
set -uo pipefail
B=${BASE:-http://127.0.0.1:3112}
H=$(mktemp)
trap 'rm -f "$H" "$H.csv"' EXIT
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
JSON=(-H 'content-type: application/json')

# ---- teams/{id}/team_members ----
req POST "$A/teams" "${JSON[@]}" -d "{\"name\":\"Parity Team $(date +%s%N)\",\"description\":\"parity\"}"
TEAM=$(printf '%s' "$BODY" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2)
check "POST teams" 200 "$CODE" '"name"' "$BODY"
req GET "$A/teams/$TEAM/team_members"
check "GET team_members" 200 "$CODE" '' "$BODY"
UIDS=$(curl -s "${AUTH[@]}" "$A/agents" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2)
req POST "$A/teams/$TEAM/team_members" "${JSON[@]}" -d "{\"user_ids\":[${UIDS:-1}]}"
check "POST team_members" 200 "$CODE" '' "$BODY"
req PATCH "$A/teams/$TEAM/team_members" "${JSON[@]}" -d "{\"user_ids\":[${UIDS:-1}]}"
check "PATCH team_members" 200 "$CODE" '' "$BODY"
req DELETE "$A/teams/$TEAM/team_members" "${JSON[@]}" -d "{\"user_ids\":[${UIDS:-1}]}"
check "DELETE team_members" 200 "$CODE" '' "$BODY"
req POST "$A/teams/$TEAM/team_members" "${JSON[@]}" -d '{"user_ids":[999999999]}'
check "POST team_members foreign user rejected" 422 "$CODE" '' "$BODY"
req DELETE "$A/teams/$TEAM" 
check "DELETE team cleanup" 200 "$CODE" '' "$BODY"

# ---- integrations/hooks ----
req POST "$A/integrations/hooks" "${JSON[@]}" -d '{"hook":{"app_id":"slack","settings":{"webhook_url":"https://example.com/hook"}}}'
HOOK=$(printf '%s' "$BODY" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2)
check "POST integrations/hooks" 200 "$CODE" '"app_id":"slack"' "$BODY"
req PATCH "$A/integrations/hooks/$HOOK" "${JSON[@]}" -d '{"hook":{"status":"disabled","settings":{"webhook_url":"https://example.com/v2"}}}'
check "PATCH integrations/hooks" 200 "$CODE" '"status":false' "$BODY"
req POST "$A/integrations/hooks" "${JSON[@]}" -d '{"hook":{"app_id":"stripe"}}'
check "POST stripe hook rejected" 404 "$CODE" '' "$BODY"
req DELETE "$A/integrations/hooks/$HOOK"
check "DELETE integrations/hooks" 200 "$CODE" '' "$BODY"
req DELETE "$A/integrations/hooks/$HOOK"
check "DELETE hook already gone" 404 "$CODE" '' "$BODY"

# ---- csat_survey_responses ----
req GET "$A/csat_survey_responses"
check "GET csat_survey_responses" 200 "$CODE" '' "$BODY"
req GET "$A/csat_survey_responses?rating=1,2&page=1"
check "GET csat_survey_responses filtered" 200 "$CODE" '' "$BODY"
req GET "$A/csat_survey_responses/metrics"
check "GET csat metrics" 200 "$CODE" '"total_count"' "$BODY"
check "GET csat metrics ratings" 200 "$CODE" '"ratings_count"' "$BODY"
req GET "$A/csat_survey_responses/download"
check "GET csat download" 200 "$CODE" 'Agent Name,Rating' "$BODY"
BOM=$(curl -s "${AUTH[@]}" "$A/csat_survey_responses/download" | head -c3 | od -An -tx1 | tr -d ' ')
if [ "$BOM" = "efbbbf" ]; then echo "PASS csat download BOM"; PASS=$((PASS + 1)); else echo "FAIL csat download BOM got=$BOM"; FAIL=$((FAIL + 1)); fi
CODE=$(curl -s -o /dev/null -w '%{http_code}' -H 'access-token: bogus' -H 'client: bogus' -H 'uid: bogus' "$A/csat_survey_responses")
check "CSAT bad token rejected" 401 "$CODE"

echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
