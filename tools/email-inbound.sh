#!/usr/bin/env bash
# Live check for inbound email ingestion (src/email_inbound.rs).
# Creates a temporary Channel::Email inbox for account 1, ingests a new mail and a reply
# (In-Reply-To), asserts one conversation with two incoming messages, then removes the inbox
# and channel rows. Test-created messages, conversations and contacts are removed by the test itself.
set -euo pipefail

DB_URL="${DATABASE_URL:-postgres://localhost/pulsar}"
ACCOUNT_ID="${ACCOUNT_ID:-1}"
STAMP="$(date +%s)"
EMAIL="ingest-${STAMP}@test.invalid"
FWD="ingest-fwd-${STAMP}@test.invalid"

psql_q() { psql "$DB_URL" -v ON_ERROR_STOP=1 -Atq -c "$1"; }

INBOX_ID=""
CHANNEL_ID=""
cleanup() {
  if [[ -n "$INBOX_ID" ]]; then
    psql_q "DELETE FROM inboxes WHERE id = ${INBOX_ID}" >/dev/null || true
  fi
  if [[ -n "$CHANNEL_ID" ]]; then
    psql_q "DELETE FROM channel_email WHERE id = ${CHANNEL_ID}" >/dev/null || true
  fi
}
trap cleanup EXIT

CHANNEL_ID="$(psql_q "INSERT INTO channel_email (account_id, email, forward_to_email, created_at, updated_at, imap_enabled)
  VALUES (${ACCOUNT_ID}, '${EMAIL}', '${FWD}', now(), now(), false) RETURNING id")"
INBOX_ID="$(psql_q "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at)
  VALUES (${CHANNEL_ID}, ${ACCOUNT_ID}, 'Ingest test ${STAMP}', 'Channel::Email', now(), now()) RETURNING id")"

echo "inbox ${INBOX_ID} channel ${CHANNEL_ID} account ${ACCOUNT_ID}"

cd "$(dirname "$0")/.."
DATABASE_URL="$DB_URL" PULSAR_TEST_INBOX_ID="$INBOX_ID" \
  cargo test --quiet -- --ignored ingest_live
echo "ingest_live passed"
