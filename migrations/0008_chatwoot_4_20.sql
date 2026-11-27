-- Chatwoot 4.20.1 schema delta (schema.rb 20261103142055 -> 20261125100456).
-- Safe on every start and on an existing Chatwoot database: columns/indexes are IF NOT EXISTS and the
-- data steps run once, guarded by the upstream version row in schema_migrations (as Rails would).

ALTER TABLE conversations ADD COLUMN IF NOT EXISTS last_message_at timestamp;
ALTER TABLE captain_mcp_tools ADD COLUMN IF NOT EXISTS review_state varchar NOT NULL DEFAULT 'current';
ALTER TABLE captain_mcp_servers DROP COLUMN IF EXISTS tools_changed_at;
ALTER TABLE channel_instagram ADD COLUMN IF NOT EXISTS auth_type varchar NOT NULL DEFAULT 'instagram_login';
ALTER TABLE channel_instagram ADD COLUMN IF NOT EXISTS configuration_id varchar;
ALTER TABLE channel_instagram ALTER COLUMN expires_at DROP NOT NULL;
ALTER TABLE channel_sms ADD COLUMN IF NOT EXISTS secret varchar;
ALTER TABLE channel_sms ADD COLUMN IF NOT EXISTS callback_auth_enforced boolean NOT NULL DEFAULT true;

-- The IVFFlat index uses L2 distance but FAQ search orders by cosine distance, so it was never used.
DROP INDEX IF EXISTS vector_idx_knowledge_entries_embedding;

CREATE INDEX IF NOT EXISTS index_conversations_on_account_oldest_message
  ON conversations (account_id, last_message_at, display_id);
CREATE INDEX IF NOT EXISTS index_conversations_on_account_newest_message
  ON conversations (account_id, last_message_at DESC NULLS LAST, display_id DESC);
CREATE INDEX IF NOT EXISTS index_conversations_on_sla_policy_id
  ON conversations (sla_policy_id) WHERE sla_policy_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS index_contact_inboxes_on_contact_id_and_inbox_id
  ON contact_inboxes (contact_id, inbox_id);
CREATE INDEX IF NOT EXISTS index_messages_on_conv_account_created_desc
  ON messages (conversation_id, account_id, created_at DESC);
CREATE INDEX IF NOT EXISTS index_reporting_events_on_account_id_and_created_at
  ON reporting_events (account_id, created_at);
CREATE INDEX IF NOT EXISTS index_pending_data_import_items
  ON data_import_items (data_import_id, id) WHERE status = 0;

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM schema_migrations WHERE version = '20261125100456') THEN
    -- Backfill the newest public (non-private, non-activity) message per conversation.
    UPDATE conversations c SET last_message_at = m.last_at
      FROM (SELECT conversation_id, max(created_at) AS last_at FROM messages
             WHERE private = false AND message_type <> 2 GROUP BY conversation_id) m
     WHERE m.conversation_id = c.id AND c.last_message_at IS NULL;
    -- Existing SMS channels get a callback secret, like the upstream migration.
    UPDATE channel_sms SET secret = encode(gen_random_bytes(18), 'hex') WHERE secret IS NULL;
    -- `last_message_sort` is a new default-enabled feature (feature_flags_ext_1 bit 17).
    UPDATE accounts SET feature_flags_ext_1 = COALESCE(feature_flags_ext_1, 0) | 131072;
    INSERT INTO schema_migrations (version) VALUES
      ('20261029104503'), ('20261104085110'), ('20261104085633'), ('20261104104833'), ('20261107075626'),
      ('20261111031108'), ('20261113073350'), ('20261113115701'), ('20261116211055'), ('20261118005459'),
      ('20261119073144'), ('20261123065557'), ('20261124175230'), ('20261124175232'), ('20261124175258'),
      ('20261125062602'), ('20261125100456')
    ON CONFLICT DO NOTHING;
  END IF;
END $$;
