#!/usr/bin/env bash
# MFA + sessions smoke against a running Pulsar server. Creates a temp user (password hash cloned from the seeded
# admin), enrols TOTP, checks sign-in 206 + second step, TOTP replay, backup-code single use, disable, cleanup.
# Never prints secrets, codes or tokens.
# Requires: server started with ACTIVE_RECORD_ENCRYPTION_PRIMARY_KEY / _KEY_DERIVATION_SALT, jq, python3, psql.
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:3172}
DB_URL=${DB_URL:-postgres://localhost/pulsar}
PASSWORD=${PASSWORD:-Password123!}
EMAIL="mfa-smoke-$(date +%s)-$RANDOM@example.com"
UID_ROW=""
WORK=$(mktemp -d)

command -v jq >/dev/null && command -v python3 >/dev/null && command -v psql >/dev/null || { echo 'need jq, python3, psql' >&2; exit 1; }

cleanup() {
  if [[ -n $UID_ROW ]]; then
    psql -qX "$DB_URL" -c "DELETE FROM access_tokens WHERE owner_id=$UID_ROW AND owner_type='User'; DELETE FROM user_sessions WHERE user_id=$UID_ROW; DELETE FROM users WHERE id=$UID_ROW;" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok: $*"; }

# totp <base32 secret> -> current 6-digit code (RFC 6238, SHA1, 30s)
totp() {
  python3 - "$1" <<'PY'
import base64, hmac, hashlib, struct, sys, time
s = sys.argv[1].upper()
s += '=' * (-len(s) % 8)
key = base64.b32decode(s)
h = hmac.new(key, struct.pack('>Q', int(time.time()) // 30), hashlib.sha1).digest()
o = h[-1] & 0x0F
print(f"{(struct.unpack('>I', h[o:o+4])[0] & 0x7FFFFFFF) % 1000000:06d}")
PY
}

# post_json <path> <json> [extra curl args...] -> prints "<status>" and writes body to $WORK/last
post_json() {
  local path=$1 body=$2; shift 2
  curl -sS -o "$WORK/last" -w '%{http_code}' -H 'Content-Type: application/json' "$@" -d "$body" "$BASE$path"
}

# 1. temp user, password hash cloned from the seeded admin (bcrypt of $PASSWORD)
HASH=$(psql -qtAX "$DB_URL" -c "SELECT encrypted_password FROM users WHERE email='admin@example.com' LIMIT 1")
[[ -n $HASH ]] || fail 'seeded admin hash not found'
UID_ROW=$(psql -qtAX "$DB_URL" -c "INSERT INTO users (provider, uid, email, name, encrypted_password, confirmed_at, created_at, updated_at)
  VALUES ('email', '$EMAIL', '$EMAIL', 'MFA Smoke', '$HASH', now(), now(), now()) RETURNING id;")
ok "temp user $UID_ROW"

# 2. password sign-in before MFA -> devise headers
code=$(curl -sS -D "$WORK/h" -o "$WORK/last" -w '%{http_code}' -H 'Content-Type: application/json' \
  -d "$(jq -nc --arg e "$EMAIL" --arg p "$PASSWORD" '{email:$e,password:$p}')" "$BASE/auth/sign_in")
[[ $code == 200 ]] || fail "sign_in pre-mfa: HTTP $code"
hdr() { tr -d '\r' < "$WORK/h" | grep -i "^$1:" | awk '{print $2}'; }
H=(-H "access-token: $(hdr access-token)" -H "client: $(hdr client)" -H "uid: $(hdr uid)")
ok 'signed in, MFA off'

# 3. state
code=$(curl -sS -o "$WORK/last" -w '%{http_code}' "${H[@]}" "$BASE/api/v1/profile/mfa")
[[ $code == 200 ]] || fail "show: HTTP $code"
[[ $(jq -r .enabled "$WORK/last") == false ]] || fail 'expected enabled=false'
ok 'GET mfa enabled=false'

# 4. api_access_token must not manage MFA
ATOK=$(psql -qtAX "$DB_URL" -c "INSERT INTO access_tokens (owner_id, owner_type, token, created_at, updated_at)
  VALUES ($UID_ROW, 'User', md5(random()::text)||md5(random()::text), now(), now()) RETURNING token;")
code=$(curl -sS -o /dev/null -w '%{http_code}' -H "api_access_token: $ATOK" "$BASE/api/v1/profile/mfa")
[[ $code == 403 ]] || fail "api token on MFA: expected 403 got $code"
ok 'api_access_token refused (403)'

# 5. enrol
code=$(curl -sS -o "$WORK/last" -w '%{http_code}' -X POST "${H[@]}" "$BASE/api/v1/profile/mfa")
[[ $code == 200 ]] || fail "enrol: HTTP $code"
SECRET=$(jq -r .secret "$WORK/last")
[[ $SECRET =~ ^[A-Z2-7]+$ ]] || fail 'secret not base32'
ok 'enrol secret issued'

# 6. wrong code -> 422
code=$(post_json /api/v1/profile/mfa/verify '{"otp_code":"000000"}' "${H[@]}")
[[ $code == 422 ]] || fail "wrong verify code: expected 422 got $code"
ok 'wrong code refused (422)'

# 7. correct code -> enabled + 10 backup codes
code=$(post_json /api/v1/profile/mfa/verify "$(jq -nc --arg c "$(totp "$SECRET")" '{otp_code:$c}')" "${H[@]}")
[[ $code == 200 ]] || fail "verify: HTTP $code"
BACKUP=()
while IFS= read -r c; do BACKUP+=("$c"); done < <(jq -r '.backup_codes[]' "$WORK/last")
[[ ${#BACKUP[@]} == 10 ]] || fail "expected 10 backup codes, got ${#BACKUP[@]}"
ok 'MFA enabled, 10 backup codes issued'

# CE consumes the TOTP step at verify, so the next code is only valid in a later 30s window.
sleep 31

# 8. password sign-in -> 206 + mfa_token, no session yet
code=$(post_json /auth/sign_in "$(jq -nc --arg e "$EMAIL" --arg p "$PASSWORD" '{email:$e,password:$p}')")
[[ $code == 206 ]] || fail "sign_in with MFA: expected 206 got $code"
MFA_TOKEN=$(jq -r .mfa_token "$WORK/last")
[[ $(jq -r .mfa_required "$WORK/last") == true && -n $MFA_TOKEN ]] || fail 'missing mfa_required/mfa_token'
ok 'sign-in returns 206 mfa_required'

# 9. second step with TOTP -> 200 session
code=$(post_json /auth/sign_in "$(jq -nc --arg t "$MFA_TOKEN" --arg c "$(totp "$SECRET")" '{mfa_token:$t,otp_code:$c}')")
[[ $code == 200 ]] || fail "second step TOTP: HTTP $code"
ok 'second step with TOTP -> 200'

# 10. same TOTP step replayed -> refused. A fresh mfa_token is needed for the second attempt.
code=$(post_json /auth/sign_in "$(jq -nc --arg e "$EMAIL" --arg p "$PASSWORD" '{email:$e,password:$p}')")
[[ $code == 206 ]] || fail "re-sign-in: expected 206 got $code"
MFA_TOKEN=$(jq -r .mfa_token "$WORK/last")
code=$(post_json /auth/sign_in "$(jq -nc --arg t "$MFA_TOKEN" --arg c "$(totp "$SECRET")" '{mfa_token:$t,otp_code:$c}')")
[[ $code == 422 ]] || fail "TOTP replay: expected 422 got $code"
ok 'TOTP replay refused (422)'

# 11. backup code accepted once, reuse refused
BC=${BACKUP[0]}
code=$(post_json /auth/sign_in "$(jq -nc --arg e "$EMAIL" --arg p "$PASSWORD" '{email:$e,password:$p}')")
[[ $code == 206 ]] || fail "re-sign-in for backup: expected 206 got $code"
MFA_TOKEN=$(jq -r .mfa_token "$WORK/last")
code=$(post_json /auth/sign_in "$(jq -nc --arg t "$MFA_TOKEN" --arg c "$BC" '{mfa_token:$t,backup_code:$c}')")
[[ $code == 200 ]] || fail "backup code first use: HTTP $code"
ok 'backup code accepted'
code=$(post_json /auth/sign_in "$(jq -nc --arg e "$EMAIL" --arg p "$PASSWORD" '{email:$e,password:$p}')")
MFA_TOKEN=$(jq -r .mfa_token "$WORK/last")
code=$(post_json /auth/sign_in "$(jq -nc --arg t "$MFA_TOKEN" --arg c "$BC" '{mfa_token:$t,backup_code:$c}')")
[[ $code == 422 ]] || fail "backup code reuse: expected 422 got $code"
ok 'backup code reuse refused (422)'

# 12. disable with password + unused backup code
code=$(curl -sS -o "$WORK/last" -w '%{http_code}' -X DELETE "${H[@]}" -H 'Content-Type: application/json' \
  -d "$(jq -nc --arg p "$PASSWORD" --arg c "${BACKUP[1]}" '{password:$p,backup_code:$c}')" "$BASE/api/v1/profile/mfa")
[[ $code == 200 ]] || fail "disable: HTTP $code"
[[ $(jq -r .enabled "$WORK/last") == false ]] || fail 'expected enabled=false after disable'
ok 'MFA disabled'

echo 'mfa smoke: PASS'
