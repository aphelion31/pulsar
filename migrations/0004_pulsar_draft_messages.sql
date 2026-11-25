-- Pulsar-owned persistence for conversation drafts.
CREATE TABLE IF NOT EXISTS pulsar_draft_messages (
  conversation_id bigint PRIMARY KEY REFERENCES conversations (id) ON DELETE CASCADE,
  message text NOT NULL DEFAULT '',
  updated_at timestamp NOT NULL DEFAULT now()
);
