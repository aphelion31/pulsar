#!/usr/bin/env bash
# Data imports (contacts CSV) API check. Usage: BASE=http://127.0.0.1:3152 DATABASE_URL=postgres://localhost/pulsar tools/data-imports.sh
# Uploads a 3-row CSV with 1 bad row, polls to a terminal status, checks counts and error/skip logs,
# removes the contacts it created, and checks that an agent gets 403.
set -euo pipefail
B=${BASE:-http://127.0.0.1:3000}
DB=${DATABASE_URL:-postgres://localhost/pulsar}
PW='Password123!'
fail() { echo "FAIL: $*"; exit 1; }

H=$(mktemp)
TAG="dimp$RANDOM$RANDOM"
CSV=$(mktemp)
ERRCSV=$(mktemp)
AGENT_ID=""
cleanup() {
  psql "$DB" -qtAX -c "DELETE FROM contacts WHERE account_id=1 AND email LIKE '${TAG}%'" >/dev/null 2>&1 || true
  if [ -n "${IMPORT_ID:-}" ]; then
    psql "$DB" -qtAX -c "DELETE FROM data_import_errors WHERE data_import_id=$IMPORT_ID" >/dev/null 2>&1 || true
    psql "$DB" -qtAX -c "DELETE FROM pulsar_data_import_files WHERE data_import_id=$IMPORT_ID" >/dev/null 2>&1 || true
    psql "$DB" -qtAX -c "DELETE FROM data_imports WHERE id=$IMPORT_ID" >/dev/null 2>&1 || true
  fi
  if [ -n "$AGENT_ID" ]; then
    psql "$DB" -qtAX -c "DELETE FROM account_users WHERE account_id=1 AND user_id=$AGENT_ID" >/dev/null 2>&1 || true
    psql "$DB" -qtAX -c "DELETE FROM users WHERE id=$AGENT_ID" >/dev/null 2>&1 || true
  fi
  rm -f "$H" "$CSV" "$ERRCSV"
}
trap cleanup EXIT

login() { # $1 email, $2 password -> sets AUTH array
  curl -sf -D "$H" -o /dev/null -H 'content-type: application/json' \
    -d "{\"email\":\"$1\",\"password\":\"$2\"}" "$B/auth/sign_in" || fail "sign_in $1"
  local hdr; hdr() { grep -i "^$1:" "$H" | cut -d' ' -f2 | tr -d '\r'; }
  AUTH=(-H "access-token: $(hdr access-token)" -H "client: $(hdr client)" -H "uid: $(hdr uid)")
}
j() { curl -sS "${AUTH[@]}" -H 'content-type: application/json' "$@"; }
A="$B/api/v1/accounts/1"
field() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)"; }

login admin@example.com "$PW"

# Three data rows: row 1 imports, row 2 has no contact fields (skipped), row 3 has the wrong column count (failed).
cat > "$CSV" <<EOF
name,email,phone_number
Import One,${TAG}1@ex.com,
,,
Bad Row,${TAG}3@ex.com,+6200,extra
EOF

UP=$(curl -sS "${AUTH[@]}" -F "import_file=@$CSV;type=text/csv" -F "name=$TAG" "$A/data_imports")
IMPORT_ID=$(echo "$UP" | field 'd["id"]') || fail "upload: $UP"
[ "$(echo "$UP" | field 'd["data_type"]')" = "contacts" ] || fail "data_type"
[ "$(echo "$UP" | field 'd["name"]')" = "$TAG" ] || fail "name"
[ "$(echo "$UP" | field 'd["total_records"]')" = "3" ] || fail "total_records on create: $UP"
echo "created import $IMPORT_ID status=$(echo "$UP" | field 'd["status"]')"

# Poll to a terminal status.
STATUS=""
for _ in $(seq 1 30); do
  STATUS=$(j "$A/data_imports/$IMPORT_ID" | field 'd["status"]')
  case "$STATUS" in completed|completed_with_errors|failed|abandoned) break ;; esac
  sleep 1
done
echo "final status=$STATUS"
[ "$STATUS" = "completed_with_errors" ] || fail "expected completed_with_errors, got $STATUS"

SHOW=$(j "$A/data_imports/$IMPORT_ID")
[ "$(echo "$SHOW" | field 'd["processed_records"]')" = "3" ] || fail "processed_records: $SHOW"
[ "$(echo "$SHOW" | field 'd["stats"]["contacts"]["imported"]')" = "1" ] || fail "imported count"
[ "$(echo "$SHOW" | field 'd["stats"]["contacts"]["skipped"]')" = "1" ] || fail "skipped count"
[ "$(echo "$SHOW" | field 'd["import_errors_count"]')" = "0" ] || fail "import_errors_count"
[ "$(echo "$SHOW" | field 'd["skip_logs_count"]')" = "2" ] || fail "skip_logs_count"
[ "$(echo "$SHOW" | field 'd["stats"]["errors"]["count"]')" = "1" ] || fail "errors.count"
echo "$SHOW" | field 'd["skip_logs"][0]["error_code"]' | grep -q . || fail "skip log missing error_code"
echo "$SHOW" | field 'd["skip_logs_filters"]["counts_by_type"]["contact"]' | grep -q 2 || fail "counts_by_type"
echo "show ok"

# Contacts created by the import exist.
CNT=$(psql "$DB" -qtAX -c "SELECT count(*) FROM contacts WHERE account_id=1 AND email LIKE '${TAG}%'")
[ "$CNT" = "1" ] || fail "expected 1 contact created, got $CNT"

# Index lists it with counts.
j "$A/data_imports" | field "[i for i in d['payload'] if i['id']==$IMPORT_ID][0]['skip_logs_count']" | grep -q 2 || fail "index"
echo "index ok"

# CSV exports.
curl -sS "${AUTH[@]}" "$A/data_imports/$IMPORT_ID/skip_logs" -o "$ERRCSV" -D "$H.skip"
grep -qi 'content-type: text/csv' "$H.skip" || fail "skip_logs content-type"
[ "$(wc -l < "$ERRCSV" | tr -d ' ')" -ge 3 ] || fail "skip_logs csv rows"
curl -sS "${AUTH[@]}" "$A/data_imports/$IMPORT_ID/error_logs" -o "$ERRCSV"
head -1 "$ERRCSV" | grep -q 'created_at,kind,source_object_type' || fail "error_logs header"
rm -f "$H.skip"
echo "csv ok"

# Retry is refused for a completed import.
CODE=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "${AUTH[@]}" "$A/data_imports/$IMPORT_ID/retry")
[ "$CODE" = "422" ] || fail "retry on completed should be 422, got $CODE"

# Unsupported source is refused.
CODE=$(curl -sS -o /dev/null -w '%{http_code}' "${AUTH[@]}" -F "import_file=@$CSV" -F "source_provider=intercom" "$A/data_imports")
[ "$CODE" = "422" ] || fail "unsupported source should be 422, got $CODE"

# Agent is forbidden.
AGENT_EMAIL="agentdi$RANDOM$RANDOM@ex.com"
AGENT_ID=$(j -X POST "$A/agents" -d "{\"agent\":{\"email\":\"$AGENT_EMAIL\",\"name\":\"DI Agent\",\"role\":\"agent\"}}" | field 'd["id"]')
HASH=$(htpasswd -nbBC 10 x "$PW" | cut -d: -f2)
psql "$DB" -qtAX -c "UPDATE users SET encrypted_password='$HASH', provider='email', uid='$AGENT_EMAIL' WHERE id=$AGENT_ID" >/dev/null
login "$AGENT_EMAIL" "$PW"
CODE=$(curl -sS -o /dev/null -w '%{http_code}' "${AUTH[@]}" "$A/data_imports")
[ "$CODE" = "403" ] || fail "agent index should be 403, got $CODE"
CODE=$(curl -sS -o /dev/null -w '%{http_code}' "${AUTH[@]}" "$A/data_imports/$IMPORT_ID")
[ "$CODE" = "403" ] || fail "agent show should be 403, got $CODE"
echo "agent forbidden ok"

echo "PASS data-imports"
