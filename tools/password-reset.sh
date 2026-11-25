#!/usr/bin/env bash
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:3110}
LOG=${LOG:?Set LOG to the server log; start the server with SMTP_ADDRESS unset and PULSAR_LOG_MAIL=1}
EMAIL=${EMAIL:-admin@example.com}
PASSWORD=${PASSWORD:-ResetPassword123!}
command -v jq >/dev/null
json() { jq -nc --arg email "$EMAIL" '{email:$email}'; }
start=$(wc -c < "$LOG" | tr -d ' ')
code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' -d "$(json)" "$BASE/auth/password")
[[ $code == 200 ]] || { echo "reset request: HTTP $code" >&2; exit 1; }
token=$(tail -c +$((start + 1)) "$LOG" | grep -oE 'reset_password_token=[A-Za-z0-9_-]+' | tail -1 | cut -d= -f2)
[[ -n $token ]] || { echo 'reset token not found in log' >&2; exit 1; }
payload=$(jq -nc --arg token "$token" --arg password "$PASSWORD" '{reset_password_token:$token,password:$password,password_confirmation:$password}')
code=$(curl -sS -o /dev/null -w '%{http_code}' -X PUT -H 'Content-Type: application/json' -d "$payload" "$BASE/auth/password")
[[ $code == 200 ]] || { echo "reset update: HTTP $code" >&2; exit 1; }
code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' -d "$(jq -nc --arg email "$EMAIL" --arg password "$PASSWORD" '{email:$email,password:$password}')" "$BASE/auth/sign_in")
[[ $code == 200 ]] || { echo "sign in: HTTP $code" >&2; exit 1; }
code=$(curl -sS -o /dev/null -w '%{http_code}' -X PUT -H 'Content-Type: application/json' -d "$payload" "$BASE/auth/password")
[[ $code == 422 ]] || { echo "reused token: HTTP $code" >&2; exit 1; }
code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' -d '{"email":"nonexistent-reset-test@example.invalid"}' "$BASE/auth/password")
[[ $code == 200 ]] || { echo "unknown email: HTTP $code" >&2; exit 1; }
echo 'password reset, sign-in, token reuse, unknown email: OK'
