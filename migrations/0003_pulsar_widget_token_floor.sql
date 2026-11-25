-- Pulsar-owned table (Chatwoot's schema is left untouched). Idempotent, safe on an existing Chatwoot DB.
-- psql -d pulsar -f migrations/0003_pulsar_widget_token_floor.sql

-- chatwoot#15776: after a sensitive widget contact write the visitor token is rotated. Tokens whose
-- iat is below min_iat for that contact_inbox are rejected, so a captured older token stops working.
CREATE TABLE IF NOT EXISTS pulsar_widget_token_floors (
  contact_inbox_id bigint PRIMARY KEY REFERENCES contact_inboxes (id) ON DELETE CASCADE,
  min_iat bigint NOT NULL,
  updated_at timestamp NOT NULL DEFAULT now()
);
