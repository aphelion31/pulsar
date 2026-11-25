#!/usr/bin/env bash
# Verification script for Help Center & Search Articles endpoints (PLR-077).
# Usage: BASE=http://127.0.0.1:3163 bash tools/help-center-r4.sh
set -euo pipefail

B="${BASE:-http://127.0.0.1:3163}"
DB="${DATABASE_URL:-postgres://localhost/pulsar}"
TOKEN="${API_TOKEN:-$(psql -qtAX "$DB" -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")}"
[ -n "$TOKEN" ] || { echo "FAIL: missing api_access_token"; exit 1; }

A="$B/api/v1/accounts/1"
AUTH=(-H "api_access_token: $TOKEN" -H "content-type: application/json")
RAND="r4hc$$"
SLUG="hc-r4-$RAND"

cleanup() {
  curl -sS -X DELETE "${AUTH[@]}" "$A/portals/$SLUG" >/dev/null || true
}
trap cleanup EXIT

req() {
  local method="$1" path="$2" body="${3:-}"
  if [ -n "$body" ]; then
    curl -sS -w '\n%{http_code}' -X "$method" "${AUTH[@]}" -d "$body" "$A$path"
  else
    curl -sS -w '\n%{http_code}' -X "$method" "${AUTH[@]}" "$A$path"
  fi
}

assert_status() {
  local resp="$1" expected="$2" label="$3"
  local code body
  code="$(printf '%s\n' "$resp" | tail -n1)"
  body="$(printf '%s\n' "$resp" | sed '$d')"
  if [ "$code" != "$expected" ]; then
    echo "FAIL [$label]: expected HTTP $expected, got $code: $body"
    exit 1
  fi
}

echo "1. Create portal $SLUG"
r=$(req POST "/portals" "{\"portal\":{\"name\":\"HC R4 $RAND\",\"slug\":\"$SLUG\"}}")
assert_status "$r" 200 "create portal"

echo "2. Check ssl_status and send_instructions validation before custom_domain"
r=$(req GET "/portals/$SLUG/ssl_status")
assert_status "$r" 422 "ssl_status without domain"
printf '%s\n' "$r" | sed '$d' | jq -e '.error == "Custom domain is not configured"' >/dev/null

r=$(req POST "/portals/$SLUG/send_instructions" '{}')
assert_status "$r" 422 "send_instructions missing email"
printf '%s\n' "$r" | sed '$d' | jq -e '.error == "Email is required"' >/dev/null

r=$(req POST "/portals/$SLUG/send_instructions" '{"email":"not-an-email"}')
assert_status "$r" 422 "send_instructions invalid email"
printf '%s\n' "$r" | sed '$d' | jq -e '.error == "Invalid email format"' >/dev/null

r=$(req POST "/portals/$SLUG/send_instructions" '{"email":"ops@example.com"}')
assert_status "$r" 422 "send_instructions without domain"
printf '%s\n' "$r" | sed '$d' | jq -e '.error == "Custom domain is not configured"' >/dev/null

echo "3. Set custom_domain and re-test ssl_status + send_instructions"
r=$(req PATCH "/portals/$SLUG" "{\"portal\":{\"custom_domain\":\"help-$RAND.example.com\"}}")
assert_status "$r" 200 "set custom_domain"

r=$(req GET "/portals/$SLUG/ssl_status")
assert_status "$r" 200 "ssl_status with domain"
printf '%s\n' "$r" | sed '$d' | jq -e 'has("status") and has("verification_errors")' >/dev/null

r=$(req POST "/portals/$SLUG/send_instructions" '{"email":"ops@example.com"}')
assert_status "$r" 200 "send_instructions ok"
printf '%s\n' "$r" | sed '$d' | jq -e '.message == "Instructions sent successfully"' >/dev/null

echo "4. Test DELETE logo (light & dark)"
r=$(req DELETE "/portals/$SLUG/logo")
assert_status "$r" 200 "delete logo"
r=$(req DELETE "/portals/$SLUG/logo?dark=true")
assert_status "$r" 200 "delete dark logo"

echo "5. Create categories and articles"
r=$(req POST "/portals/$SLUG/categories" '{"category":{"name":"Cat One","slug":"cat-one","locale":"en"}}')
assert_status "$r" 200 "create cat1"
CAT1=$(printf '%s\n' "$r" | sed '$d' | jq -r '.payload.id')

r=$(req POST "/portals/$SLUG/categories" '{"category":{"name":"Cat Two","slug":"cat-two","locale":"en"}}')
assert_status "$r" 200 "create cat2"
CAT2=$(printf '%s\n' "$r" | sed '$d' | jq -r '.payload.id')

r=$(req POST "/portals/$SLUG/articles" "{\"article\":{\"title\":\"Alpha $RAND\",\"content\":\"Body alpha $RAND\",\"author_id\":1,\"category_id\":$CAT1,\"status\":\"draft\"}}")
assert_status "$r" 200 "create art1"
ART1=$(printf '%s\n' "$r" | sed '$d' | jq -r '.payload.id')

r=$(req POST "/portals/$SLUG/articles" "{\"article\":{\"title\":\"Beta $RAND\",\"content\":\"Body beta $RAND\",\"author_id\":1,\"category_id\":$CAT1,\"status\":\"draft\"}}")
assert_status "$r" 200 "create art2"
ART2=$(printf '%s\n' "$r" | sed '$d' | jq -r '.payload.id')

echo "6. Test bulk update_status and update_category"
r=$(req PATCH "/portals/$SLUG/articles/bulk_actions/update_status" "{\"ids\":[$ART1,$ART2],\"status\":\"published\"}")
assert_status "$r" 200 "bulk update_status"

r=$(req GET "/portals/$SLUG/articles/$ART1")
assert_status "$r" 200 "show art1"
printf '%s\n' "$r" | sed '$d' | jq -e '.payload.status == "published"' >/dev/null

r=$(req PATCH "/portals/$SLUG/articles/bulk_actions/update_status" '{"ids":[999999999],"status":"published"}')
assert_status "$r" 422 "bulk update_status missing ids"

r=$(req PATCH "/portals/$SLUG/articles/bulk_actions/update_category" "{\"ids\":[$ART1,$ART2],\"category_id\":$CAT2}")
assert_status "$r" 200 "bulk update_category"

r=$(req GET "/portals/$SLUG/articles/$ART2")
assert_status "$r" 200 "show art2"
printf '%s\n' "$r" | sed '$d' | jq -e ".payload.category.id == $CAT2" >/dev/null

echo "7. Test GET /search/articles"
r=$(req GET "/search/articles?q=$RAND")
assert_status "$r" 200 "search articles"
printf '%s\n' "$r" | sed '$d' | jq -e "
  (.payload.articles | length) == 2 and
  (.payload.articles[0] | has(\"id\") and has(\"title\") and has(\"locale\") and has(\"content\") and has(\"slug\") and has(\"portal_slug\") and has(\"account_id\") and has(\"category_name\") and has(\"status\") and has(\"updated_at\")) and
  (.payload.articles[0].portal_slug == \"$SLUG\") and
  (.payload.articles[0].category_name == \"Cat Two\") and
  (.payload.articles[0].status == \"published\")
" >/dev/null

r=$(req GET "/search/articles?q=")
assert_status "$r" 200 "search articles empty q"
printf '%s\n' "$r" | sed '$d' | jq -e '(.payload.articles | length) == 0' >/dev/null

echo "8. Test DELETE bulk_actions/delete_articles"
r=$(req DELETE "/portals/$SLUG/articles/bulk_actions/delete_articles" "{\"ids\":[$ART1,$ART2]}")
assert_status "$r" 200 "bulk delete_articles"

r=$(req GET "/search/articles?q=$RAND")
assert_status "$r" 200 "search articles after bulk delete"
printf '%s\n' "$r" | sed '$d' | jq -e '(.payload.articles | length) == 0' >/dev/null

echo "HELP CENTER R4 OK"
