#!/usr/bin/env bash
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:3141}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
TOKEN=${TOKEN:-$(psql -qtAX "$DB" -c "select token from access_tokens where owner_id=1 and owner_type='User' limit 1")}
AGENT=$(psql -qtAX "$DB" -c "select user_id from account_users where account_id=1 and role=0 limit 1")
TEST_TOKEN="hc-test-$(uuidgen)"
SLUG="hc-test-$(uuidgen | tr '[:upper:]' '[:lower:]')"
A="$BASE/api/v1/accounts/1/portals"
H=(-H "api_access_token: $TOKEN" -H 'content-type: application/json')
cleanup() {
  curl -s -X DELETE "${H[@]}" "$A/$SLUG" >/dev/null || true
  printf '%s\n' "delete from access_tokens where token=:'token'" | psql -q "$DB" -v token="$TEST_TOKEN" >/dev/null || true
}
trap cleanup EXIT
printf '%s\n' "insert into access_tokens(owner_type,owner_id,token,created_at,updated_at) values ('User', :'agent', :'token', now(), now())" | psql -q "$DB" -v ON_ERROR_STOP=1 -v token="$TEST_TOKEN" -v agent="$AGENT"
code=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "api_access_token: $TEST_TOKEN" -H 'content-type: application/json' -d "{\"portal\":{\"name\":\"Denied\",\"slug\":\"$SLUG\"}}" "$A")
[ "$code" = 403 ]
curl -fsS -X POST "${H[@]}" -d "{\"portal\":{\"name\":\"Test help\",\"slug\":\"$SLUG\"}}" "$A" | jq -e --arg s "$SLUG" '.slug == $s' >/dev/null
C=$(curl -fsS -X POST "${H[@]}" -d '{"category":{"name":"Basics","slug":"basics","locale":"en"}}' "$A/$SLUG/categories" | jq -er '.payload.id')
ARTICLE="article-$SLUG"
ID=$(curl -fsS -X POST "${H[@]}" -d "{\"article\":{\"title\":\"Hello\",\"slug\":\"$ARTICLE\",\"content\":\"World\",\"status\":\"published\",\"author_id\":1,\"category_id\":$C}}" "$A/$SLUG/articles" | jq -er '.payload.id')
curl -fsS "$BASE/hc/$SLUG/en/categories.json" | jq -e '.payload | any(.slug == "basics")' >/dev/null
curl -fsS "$BASE/hc/$SLUG/en/articles.json" | jq -e --arg s "$ARTICLE" '.payload | any(.slug == $s)' >/dev/null
curl -fsS "$BASE/hc/$SLUG/articles/$ARTICLE.json" | jq -e '.title == "Hello" and .status == "published"' >/dev/null
curl -fsS "${H[@]}" "$A/$SLUG/articles?status=published&category_slug=basics&locale=en&author_id=1&query=Hello" | jq -e '.payload | length == 1' >/dev/null
curl -fsS -X PATCH "${H[@]}" -d '{"article":{"title":"Updated"}}' "$A/$SLUG/articles/$ID" | jq -e '.payload.title == "Updated"' >/dev/null
curl -fsS -X DELETE "${H[@]}" "$A/$SLUG/articles/$ID" >/dev/null
curl -fsS -X DELETE "${H[@]}" "$A/$SLUG/categories/$C" >/dev/null
curl -fsS -X DELETE "${H[@]}" "$A/$SLUG" >/dev/null
echo 'help-center: PASS'
