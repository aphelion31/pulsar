#!/usr/bin/env bash
set -euo pipefail

DB=${DATABASE_URL:-postgres://localhost/pulsar}
PORT=${PORT:-3173}
WEB_DIR=${WEB_DIR:-/Users/beyondb/.jcode/scratch/wt-int4/web/build/client}
BIN=${BIN:-${CARGO_TARGET_DIR:-/Users/beyondb/.jcode/scratch/wt-r5-rg-target}/debug/pulsar}
MODE=${MODE:-all}

command -v jq >/dev/null
command -v psql >/dev/null

cleanup_email() {
  local em="$1"
  psql "$DB" -v ON_ERROR_STOP=1 -qtAX <<SQL >/dev/null
DELETE FROM access_tokens WHERE owner_type = 'User' AND owner_id IN (SELECT id FROM users WHERE lower(email) = lower('$em'));
DELETE FROM account_users WHERE user_id IN (SELECT id FROM users WHERE lower(email) = lower('$em'));
DELETE FROM accounts WHERE id NOT IN (SELECT DISTINCT account_id FROM account_users) AND name IN ('RegTest Co', 'RegTest V2 Co', '');
DELETE FROM users WHERE lower(email) = lower('$em');
SQL
}

test_disabled() {
  local base="$1"
  local em="reg-disabled-$$@example.com"
  local c1 c2
  c1=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em" '{account_name:"RegTest Co",user_full_name:"Reg User",email:$e,password:"Password123!"}')" \
    "$base/api/v1/accounts")
  [[ "$c1" == "404" ]] || { echo "disabled v1 expected 404, got $c1" >&2; exit 1; }
  c2=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em" '{email:$e,password:"Password123!"}')" \
    "$base/api/v2/accounts")
  [[ "$c2" == "404" ]] || { echo "disabled v2 expected 404, got $c2" >&2; exit 1; }
  echo "signup disabled (v1=404, v2=404): OK"
}

test_enabled() {
  local base="$1"
  local log="$2"
  local em1="reg-v1-$$@example.com"
  local em2="reg-v2-$$@example.com"
  local pw="Password123!"
  trap "cleanup_email '$em1'; cleanup_email '$em2'" RETURN

  # 1. Invalid password (422)
  local code
  code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em1" '{account_name:"RegTest Co",user_full_name:"Reg User",email:$e,password:"weak"}')" \
    "$base/api/v1/accounts")
  [[ "$code" == "422" ]] || { echo "weak password expected 422, got $code" >&2; exit 1; }

  # 2. Missing account_name & user_full_name on v1 (403)
  code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em1" --arg p "$pw" '{email:$e,password:$p}')" \
    "$base/api/v1/accounts")
  [[ "$code" == "403" ]] || { echo "missing names v1 expected 403, got $code" >&2; exit 1; }

  # 3. Valid v1 signup -> 200 {email}, no auth headers, confirmation email logged
  local start hdr_file body_file
  hdr_file=$(mktemp)
  body_file=$(mktemp)
  start=$(wc -c < "$log" | tr -d ' ')
  code=$(curl -sS -D "$hdr_file" -o "$body_file" -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em1" --arg p "$pw" '{account_name:"RegTest Co",user_full_name:"Reg User",email:$e,password:$p}')" \
    "$base/api/v1/accounts")
  [[ "$code" == "200" ]] || { echo "v1 signup expected 200, got $code: $(cat "$body_file")" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  [[ "$(jq -r '.email' "$body_file")" == "$em1" ]] || { echo "v1 signup body mismatch: $(cat "$body_file")" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  if grep -qi '^access-token:' "$hdr_file"; then
    echo "v1 web signup must not return access-token header" >&2; rm -f "$hdr_file" "$body_file"; exit 1
  fi
  rm -f "$hdr_file" "$body_file"

  local tok1
  tok1=$(tail -c +$((start + 1)) "$log" | grep -oE 'confirmation_token=[A-Za-z0-9_-]+' | tail -1 | cut -d= -f2 || true)
  [[ -n "$tok1" ]] || { echo "confirmation_token not found in log after v1 signup" >&2; exit 1; }

  # 4. Duplicate email -> 422
  code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em1" --arg p "$pw" '{account_name:"RegTest Co",user_full_name:"Reg User",email:$e,password:$p}')" \
    "$base/api/v1/accounts")
  [[ "$code" == "422" ]] || { echo "duplicate email expected 422, got $code" >&2; exit 1; }

  # 5. Sign in before confirmation -> 401
  code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em1" --arg p "$pw" '{email:$e,password:$p}')" \
    "$base/auth/sign_in")
  [[ "$code" == "401" ]] || { echo "unconfirmed sign_in expected 401, got $code" >&2; exit 1; }

  # 6. Resend confirmation -> 200 + new token logged
  start=$(wc -c < "$log" | tr -d ' ')
  code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em1" '{email:$e}')" \
    "$base/resend_confirmation")
  [[ "$code" == "200" ]] || { echo "resend_confirmation expected 200, got $code" >&2; exit 1; }
  local tok2
  tok2=$(tail -c +$((start + 1)) "$log" | grep -oE 'confirmation_token=[A-Za-z0-9_-]+' | tail -1 | cut -d= -f2 || true)
  [[ -n "$tok2" && "$tok2" != "$tok1" ]] || { echo "resend_confirmation did not log new token" >&2; exit 1; }

  # 7. Confirm with invalid token -> 422
  code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d '{"confirmation_token":"invalid-tok-xyz"}' \
    "$base/auth/confirmation")
  [[ "$code" == "422" ]] || { echo "invalid confirmation token expected 422, got $code" >&2; exit 1; }

  # 8. Confirm with valid token -> 200 + auth headers + confirmed=true
  hdr_file=$(mktemp)
  body_file=$(mktemp)
  code=$(curl -sS -D "$hdr_file" -o "$body_file" -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg t "$tok2" '{confirmation_token:$t}')" \
    "$base/auth/confirmation")
  [[ "$code" == "200" ]] || { echo "confirm expected 200, got $code: $(cat "$body_file")" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  grep -qi '^access-token:' "$hdr_file" || { echo "confirm missing access-token header" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  grep -qi '^client:' "$hdr_file" || { echo "confirm missing client header" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  grep -qi '^uid:' "$hdr_file" || { echo "confirm missing uid header" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  [[ "$(jq -r '.data.confirmed' "$body_file")" == "true" ]] || { echo "confirm .data.confirmed != true" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  rm -f "$hdr_file" "$body_file"

  # 9. Re-confirm -> 422
  code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg t "$tok2" '{confirmation_token:$t}')" \
    "$base/auth/confirmation")
  [[ "$code" == "422" ]] || { echo "re-confirm expected 422, got $code" >&2; exit 1; }

  # 10. Sign in after confirmation -> 200
  code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em1" --arg p "$pw" '{email:$e,password:$p}')" \
    "$base/auth/sign_in")
  [[ "$code" == "200" ]] || { echo "confirmed sign_in expected 200, got $code" >&2; exit 1; }

  # 11. v2 signup -> 200 + auth headers + account_id
  hdr_file=$(mktemp)
  body_file=$(mktemp)
  code=$(curl -sS -D "$hdr_file" -o "$body_file" -w '%{http_code}' -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg e "$em2" --arg p "$pw" '{email:$e,password:$p}')" \
    "$base/api/v2/accounts")
  [[ "$code" == "200" ]] || { echo "v2 signup expected 200, got $code: $(cat "$body_file")" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  grep -qi '^access-token:' "$hdr_file" || { echo "v2 signup missing access-token header" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  [[ "$(jq -r '.data.account_id' "$body_file")" != "null" ]] || { echo "v2 signup missing account_id" >&2; rm -f "$hdr_file" "$body_file"; exit 1; }
  rm -f "$hdr_file" "$body_file"

  echo "signup enabled (v1, duplicate 422, resend, confirm, sign_in, v2): OK"
}

run_server_and_wait() {
  local base="http://127.0.0.1:$PORT"
  for _ in $(seq 1 40); do
    if curl -sf "$base/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "server did not start on $base" >&2
  return 1
}

if [[ "$MODE" == "disabled" ]]; then
  test_disabled "${BASE:-http://127.0.0.1:$PORT}"
elif [[ "$MODE" == "enabled" ]]; then
  test_enabled "${BASE:-http://127.0.0.1:$PORT}" "${LOG:?Set LOG for MODE=enabled}"
else
  BASE="http://127.0.0.1:$PORT"
  LOG=$(mktemp)
  PID=""
  stop_srv() { [[ -n "$PID" ]] && kill "$PID" 2>/dev/null || true; rm -f "$LOG"; }
  trap stop_srv EXIT

  # Disabled case (default env)
  BIND="127.0.0.1:$PORT" WEB_DIR="$WEB_DIR" "$BIN" >"$LOG" 2>&1 &
  PID=$!
  run_server_and_wait
  test_disabled "$BASE"
  kill "$PID" 2>/dev/null || true
  wait "$PID" 2>/dev/null || true
  PID=""

  # Enabled case
  : > "$LOG"
  BIND="127.0.0.1:$PORT" WEB_DIR="$WEB_DIR" ENABLE_ACCOUNT_SIGNUP=true PULSAR_LOG_MAIL=1 "$BIN" >"$LOG" 2>&1 &
  PID=$!
  run_server_and_wait
  test_enabled "$BASE" "$LOG"
fi
