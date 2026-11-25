-- Data exports table (Chatwoot db/schema.rb data_exports)
CREATE TABLE IF NOT EXISTS "data_exports" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "initiated_by_id" bigint,
  "name" varchar NOT NULL,
  "data_type" varchar DEFAULT 'contacts' NOT NULL,
  "status" integer DEFAULT 0 NOT NULL,
  "export_options" jsonb DEFAULT '{}' NOT NULL,
  "active_run_id" varchar,
  "processed_records" integer DEFAULT 0 NOT NULL,
  "total_records" integer,
  "error_message" text,
  "started_at" timestamp,
  "completed_at" timestamp,
  "notification_sent_at" timestamp,
  "artifacts_expired_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX IF NOT EXISTS "index_data_exports_on_account_id_and_id" ON "data_exports" ("account_id", "id");
