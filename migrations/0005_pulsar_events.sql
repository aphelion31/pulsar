-- Pulsar-owned outbox for async listeners (webhooks, email, automation, notifications).
-- Rows are deleted once every listener ran; rows left after a crash are replayed on startup.
CREATE TABLE IF NOT EXISTS pulsar_events (
  id bigserial PRIMARY KEY,
  name text NOT NULL,
  account_id bigint NOT NULL,
  data jsonb NOT NULL,
  changed jsonb,
  attempts int NOT NULL DEFAULT 0,
  created_at timestamp NOT NULL DEFAULT now()
);
