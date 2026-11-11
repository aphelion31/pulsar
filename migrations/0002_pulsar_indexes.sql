-- Pulsar additions on top of the Chatwoot schema. Safe to run on an existing Chatwoot database:
-- every statement is idempotent and indexes are built CONCURRENTLY (no table lock). Run outside a
-- transaction: psql -d pulsar -f migrations/0002_pulsar_indexes.sql

-- chatwoot#16049: the default conversation list sorts by last_activity_at within an account.
-- Without this index Postgres does a parallel sequential scan plus a heap sort on large accounts.
CREATE INDEX CONCURRENTLY IF NOT EXISTS index_conversations_on_account_id_and_last_activity_at
  ON conversations (account_id, last_activity_at DESC NULLS LAST, id DESC);

-- chatwoot#15743: idempotent message create looks up a retried send by echo_id within a conversation.
CREATE INDEX CONCURRENTLY IF NOT EXISTS index_messages_on_conversation_id_and_echo_id
  ON messages (conversation_id, (additional_attributes ->> 'echo_id'))
  WHERE additional_attributes ? 'echo_id';
