-- generated from chatwoot schema.rb version 20261103142055
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS "plpgsql";
CREATE EXTENSION IF NOT EXISTS "vector";
CREATE TABLE "access_tokens" (
  "id" bigserial PRIMARY KEY,
  "owner_type" varchar,
  "owner_id" bigint,
  "token" varchar,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_access_tokens_on_owner_type_and_owner_id" ON "access_tokens" ("owner_type", "owner_id");
CREATE UNIQUE INDEX "index_access_tokens_on_token" ON "access_tokens" ("token");
CREATE TABLE "account_saml_settings" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "sso_url" varchar,
  "certificate" text,
  "sp_entity_id" varchar,
  "idp_entity_id" varchar,
  "role_mappings" json DEFAULT '{}',
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "metadata_url" varchar,
  "metadata_refreshed_at" timestamp
);
CREATE INDEX "index_account_saml_settings_on_account_id" ON "account_saml_settings" ("account_id");
CREATE TABLE "account_users" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint,
  "user_id" bigint,
  "role" integer DEFAULT 0,
  "inviter_id" bigint,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "active_at" timestamp,
  "availability" integer DEFAULT 0 NOT NULL,
  "auto_offline" boolean DEFAULT true NOT NULL,
  "custom_role_id" bigint,
  "agent_capacity_policy_id" bigint,
  "accepts_conversations" boolean DEFAULT true NOT NULL,
  "accepts_calls" boolean DEFAULT true NOT NULL
);
CREATE UNIQUE INDEX "uniq_user_id_per_account_id" ON "account_users" ("account_id", "user_id");
CREATE INDEX "index_account_users_on_account_id" ON "account_users" ("account_id");
CREATE INDEX "index_account_users_on_agent_capacity_policy_id" ON "account_users" ("agent_capacity_policy_id");
CREATE INDEX "index_account_users_on_custom_role_id" ON "account_users" ("custom_role_id");
CREATE INDEX "index_account_users_on_user_id" ON "account_users" ("user_id");
CREATE TABLE "accounts" (
  "id" serial PRIMARY KEY,
  "name" varchar NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "locale" integer DEFAULT 0,
  "domain" varchar(100),
  "support_email" varchar(100),
  "feature_flags" bigint DEFAULT 0 NOT NULL,
  "auto_resolve_duration" integer,
  "limits" jsonb DEFAULT '{}',
  "custom_attributes" jsonb DEFAULT '{}',
  "status" integer DEFAULT 0,
  "internal_attributes" jsonb DEFAULT '{}' NOT NULL,
  "settings" jsonb DEFAULT '{}',
  "feature_flags_ext_1" bigint DEFAULT 0 NOT NULL
);
CREATE INDEX "index_accounts_on_status" ON "accounts" ("status");
CREATE TABLE "action_mailbox_inbound_emails" (
  "id" bigserial PRIMARY KEY,
  "status" integer DEFAULT 0 NOT NULL,
  "message_id" varchar NOT NULL,
  "message_checksum" varchar NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_action_mailbox_inbound_emails_uniqueness" ON "action_mailbox_inbound_emails" ("message_id", "message_checksum");
CREATE TABLE "active_storage_attachments" (
  "id" bigserial PRIMARY KEY,
  "name" varchar NOT NULL,
  "record_type" varchar NOT NULL,
  "record_id" bigint NOT NULL,
  "blob_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL
);
CREATE INDEX "index_active_storage_attachments_on_blob_id" ON "active_storage_attachments" ("blob_id");
CREATE UNIQUE INDEX "index_active_storage_attachments_uniqueness" ON "active_storage_attachments" ("record_type", "record_id", "name", "blob_id");
CREATE TABLE "active_storage_blobs" (
  "id" bigserial PRIMARY KEY,
  "key" varchar NOT NULL,
  "filename" varchar NOT NULL,
  "content_type" varchar,
  "metadata" text,
  "byte_size" bigint NOT NULL,
  "checksum" varchar,
  "created_at" timestamp NOT NULL,
  "service_name" varchar NOT NULL
);
CREATE UNIQUE INDEX "index_active_storage_blobs_on_key" ON "active_storage_blobs" ("key");
CREATE TABLE "active_storage_variant_records" (
  "id" bigserial PRIMARY KEY,
  "blob_id" bigint NOT NULL,
  "variation_digest" varchar NOT NULL
);
CREATE UNIQUE INDEX "index_active_storage_variant_records_uniqueness" ON "active_storage_variant_records" ("blob_id", "variation_digest");
CREATE TABLE "agent_bot_inboxes" (
  "id" bigserial PRIMARY KEY,
  "inbox_id" integer,
  "agent_bot_id" integer,
  "status" integer DEFAULT 0,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "account_id" integer
);
CREATE TABLE "agent_bots" (
  "id" bigserial PRIMARY KEY,
  "name" varchar,
  "description" varchar,
  "outgoing_url" varchar,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "account_id" bigint,
  "bot_type" integer DEFAULT 0,
  "bot_config" jsonb DEFAULT '{}',
  "secret" varchar
);
CREATE INDEX "index_agent_bots_on_account_id" ON "agent_bots" ("account_id");
CREATE TABLE "agent_capacity_policies" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "name" varchar(255) NOT NULL,
  "description" text,
  "exclusion_rules" jsonb DEFAULT '{}' NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_agent_capacity_policies_on_account_id" ON "agent_capacity_policies" ("account_id");
CREATE TABLE "agent_sessions" (
  "id" bigserial PRIMARY KEY,
  "session_type" integer NOT NULL,
  "subject_type" varchar NOT NULL,
  "subject_id" bigint NOT NULL,
  "result_type" varchar,
  "result_id" bigint,
  "account_id" bigint NOT NULL,
  "assistant_id" bigint NOT NULL,
  "user_id" bigint,
  "llm_model" varchar,
  "credits_consumed" double precision,
  "faq_ids" jsonb DEFAULT '[]',
  "document_ids" jsonb DEFAULT '[]',
  "scenario_ids" jsonb DEFAULT '[]',
  "run_context" jsonb DEFAULT '{}',
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "cited_document_ids" jsonb DEFAULT '[]' NOT NULL,
  "used_faq_ids" jsonb DEFAULT '[]' NOT NULL
);
CREATE INDEX "idx_on_account_id_result_type_result_id_ca66c00cd7" ON "agent_sessions" ("account_id", "result_type", "result_id");
CREATE INDEX "idx_on_account_id_session_type_created_at_c20a14bd4e" ON "agent_sessions" ("account_id", "session_type", "created_at");
CREATE INDEX "idx_on_account_id_subject_type_subject_id_6d60963b3d" ON "agent_sessions" ("account_id", "subject_type", "subject_id");
CREATE INDEX "index_agent_sessions_on_account_id" ON "agent_sessions" ("account_id");
CREATE INDEX "index_agent_sessions_on_assistant_id" ON "agent_sessions" ("assistant_id");
CREATE INDEX "index_agent_sessions_on_cited_document_ids" ON "agent_sessions" USING gin ("cited_document_ids");
CREATE INDEX "index_agent_sessions_on_document_ids" ON "agent_sessions" USING gin ("document_ids");
CREATE INDEX "index_agent_sessions_on_used_faq_ids" ON "agent_sessions" USING gin ("used_faq_ids");
CREATE INDEX "index_agent_sessions_on_user_id" ON "agent_sessions" ("user_id");
CREATE TABLE "applied_slas" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "sla_policy_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "sla_status" integer DEFAULT 0,
  "completed_at" timestamp
);
CREATE UNIQUE INDEX "index_applied_slas_on_account_sla_policy_conversation" ON "applied_slas" ("account_id", "sla_policy_id", "conversation_id");
CREATE INDEX "index_applied_slas_on_account_id" ON "applied_slas" ("account_id");
CREATE INDEX "index_applied_slas_on_conversation_id" ON "applied_slas" ("conversation_id");
CREATE INDEX "index_applied_slas_on_sla_policy_id" ON "applied_slas" ("sla_policy_id");
CREATE TABLE "article_embeddings" (
  "id" bigserial PRIMARY KEY,
  "article_id" bigint NOT NULL,
  "term" text NOT NULL,
  "embedding" vector(1536),
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_article_embeddings_on_embedding" ON "article_embeddings" USING ivfflat ("embedding");
CREATE TABLE "articles" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "portal_id" integer NOT NULL,
  "category_id" integer,
  "folder_id" integer,
  "title" varchar,
  "description" text,
  "content" text,
  "status" integer,
  "views" integer,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "author_id" bigint,
  "associated_article_id" bigint,
  "meta" jsonb DEFAULT '{}',
  "slug" varchar NOT NULL,
  "position" integer,
  "locale" varchar DEFAULT 'en' NOT NULL,
  "draft_title" varchar,
  "draft_content" text
);
CREATE INDEX "index_articles_on_account_id" ON "articles" ("account_id");
CREATE INDEX "index_articles_on_associated_article_id" ON "articles" ("associated_article_id");
CREATE INDEX "index_articles_on_author_id" ON "articles" ("author_id");
CREATE INDEX "index_articles_on_category_id" ON "articles" ("category_id");
CREATE INDEX "index_articles_on_portal_id" ON "articles" ("portal_id");
CREATE UNIQUE INDEX "index_articles_on_slug" ON "articles" ("slug");
CREATE INDEX "index_articles_on_status" ON "articles" ("status");
CREATE INDEX "index_articles_on_views" ON "articles" ("views");
CREATE TABLE "assignment_policies" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "name" varchar(255) NOT NULL,
  "description" text,
  "assignment_order" integer DEFAULT 0 NOT NULL,
  "conversation_priority" integer DEFAULT 0 NOT NULL,
  "fair_distribution_limit" integer DEFAULT 100 NOT NULL,
  "fair_distribution_window" integer DEFAULT 3600 NOT NULL,
  "enabled" boolean DEFAULT true NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "exclude_older_than_hours" integer DEFAULT 168
);
CREATE UNIQUE INDEX "index_assignment_policies_on_account_id_and_name" ON "assignment_policies" ("account_id", "name");
CREATE INDEX "index_assignment_policies_on_account_id" ON "assignment_policies" ("account_id");
CREATE INDEX "index_assignment_policies_on_enabled" ON "assignment_policies" ("enabled");
CREATE TABLE "attachments" (
  "id" serial PRIMARY KEY,
  "file_type" integer DEFAULT 0,
  "external_url" varchar,
  "coordinates_lat" double precision DEFAULT 0.0,
  "coordinates_long" double precision DEFAULT 0.0,
  "message_id" integer NOT NULL,
  "account_id" integer NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "fallback_title" varchar,
  "extension" varchar,
  "meta" jsonb DEFAULT '{}'
);
CREATE INDEX "index_attachments_on_account_id" ON "attachments" ("account_id");
CREATE INDEX "index_attachments_on_message_id" ON "attachments" ("message_id");
CREATE TABLE "audits" (
  "id" bigserial PRIMARY KEY,
  "auditable_id" bigint,
  "auditable_type" varchar,
  "associated_id" bigint,
  "associated_type" varchar,
  "user_id" bigint,
  "user_type" varchar,
  "username" varchar,
  "action" varchar,
  "audited_changes" jsonb,
  "version" integer DEFAULT 0,
  "comment" varchar,
  "remote_address" varchar,
  "request_uuid" varchar,
  "created_at" timestamp,
  "city" varchar,
  "country" varchar,
  "country_code" varchar
);
CREATE INDEX "index_audits_on_associated_and_created_at" ON "audits" ("associated_type", "associated_id", "created_at");
CREATE INDEX "associated_index" ON "audits" ("associated_type", "associated_id");
CREATE INDEX "auditable_index" ON "audits" ("auditable_type", "auditable_id", "version");
CREATE INDEX "index_audits_on_created_at" ON "audits" ("created_at");
CREATE INDEX "index_audits_on_request_uuid" ON "audits" ("request_uuid");
CREATE INDEX "user_index" ON "audits" ("user_id", "user_type");
CREATE TABLE "automation_rule_pending_executions" (
  "id" bigserial PRIMARY KEY,
  "automation_rule_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "account_id" bigint NOT NULL,
  "message_id" bigint,
  "due_at" timestamp NOT NULL,
  "episode_key" varchar NOT NULL,
  "status" integer DEFAULT 0 NOT NULL,
  "skip_reason" varchar,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_automation_rule_pending_executions_on_account_id" ON "automation_rule_pending_executions" ("account_id");
CREATE UNIQUE INDEX "uniq_automation_pending_execution_episode" ON "automation_rule_pending_executions" ("automation_rule_id", "conversation_id", "episode_key");
CREATE INDEX "index_automation_rule_pending_executions_on_automation_rule_id" ON "automation_rule_pending_executions" ("automation_rule_id");
CREATE INDEX "index_automation_rule_pending_executions_on_conversation_id" ON "automation_rule_pending_executions" ("conversation_id");
CREATE INDEX "index_automation_rule_pending_executions_on_status_and_due_at" ON "automation_rule_pending_executions" ("status", "due_at");
CREATE INDEX "index_automation_pending_executions_on_status_and_updated_at" ON "automation_rule_pending_executions" ("status", "updated_at");
CREATE TABLE "automation_rules" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "name" varchar NOT NULL,
  "description" text,
  "event_name" varchar NOT NULL,
  "conditions" jsonb DEFAULT '"{}"' NOT NULL,
  "actions" jsonb DEFAULT '"{}"' NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "active" boolean DEFAULT true NOT NULL,
  "execution_delay" integer,
  "monitor_id" bigint,
  "monitor_event_activated_at" timestamp
);
CREATE INDEX "index_automation_rules_on_account_id" ON "automation_rules" ("account_id");
CREATE INDEX "index_automation_rules_on_monitor_id" ON "automation_rules" ("monitor_id");
CREATE TABLE "calls" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "inbox_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "contact_id" bigint NOT NULL,
  "message_id" bigint,
  "accepted_by_agent_id" bigint,
  "provider_call_id" varchar NOT NULL,
  "provider" integer DEFAULT 0 NOT NULL,
  "direction" integer NOT NULL,
  "status" varchar DEFAULT 'ringing' NOT NULL,
  "started_at" timestamp,
  "duration_seconds" integer,
  "end_reason" varchar,
  "meta" jsonb DEFAULT '{}',
  "transcript" text,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "ring_state" jsonb DEFAULT '{}' NOT NULL
);
CREATE INDEX "index_calls_on_account_id_and_contact_id" ON "calls" ("account_id", "contact_id");
CREATE INDEX "index_calls_on_account_id_and_conversation_id" ON "calls" ("account_id", "conversation_id");
CREATE INDEX "index_calls_on_account_id_and_created_at" ON "calls" ("account_id", "created_at");
CREATE INDEX "index_calls_on_message_id" ON "calls" ("message_id");
CREATE INDEX "index_calls_ringing_on_provider_and_created_at" ON "calls" ("provider", "created_at") WHERE ((status)::text = 'ringing'::text);
CREATE UNIQUE INDEX "index_calls_on_provider_and_provider_call_id" ON "calls" ("provider", "provider_call_id");
CREATE TABLE "campaign_recipients" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "campaign_id" bigint NOT NULL,
  "contact_id" bigint NOT NULL,
  "inbox_id" bigint NOT NULL,
  "source_id" varchar,
  "status" integer DEFAULT 0 NOT NULL,
  "error_code" varchar,
  "error_title" varchar,
  "error_message" text,
  "message_content" text,
  "sent_at" timestamp,
  "delivered_at" timestamp,
  "read_at" timestamp,
  "failed_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "contact_inbox_id" bigint,
  "send_attempted_at" timestamp
);
CREATE INDEX "index_campaign_recipients_on_account_id_and_campaign_id" ON "campaign_recipients" ("account_id", "campaign_id");
CREATE INDEX "index_campaign_recipients_on_account_id" ON "campaign_recipients" ("account_id");
CREATE UNIQUE INDEX "index_campaign_recipients_on_campaign_id_and_contact_id" ON "campaign_recipients" ("campaign_id", "contact_id");
CREATE INDEX "index_campaign_recipients_on_campaign_id_and_status" ON "campaign_recipients" ("campaign_id", "status");
CREATE INDEX "index_campaign_recipients_on_campaign_id" ON "campaign_recipients" ("campaign_id");
CREATE INDEX "index_campaign_recipients_on_contact_id" ON "campaign_recipients" ("contact_id");
CREATE INDEX "index_campaign_recipients_on_contact_inbox_id" ON "campaign_recipients" ("contact_inbox_id");
CREATE INDEX "index_campaign_recipients_on_inbox_id" ON "campaign_recipients" ("inbox_id");
CREATE UNIQUE INDEX "index_campaign_recipients_on_source_id" ON "campaign_recipients" ("source_id") WHERE (source_id IS NOT NULL);
CREATE TABLE "campaigns" (
  "id" bigserial PRIMARY KEY,
  "display_id" integer NOT NULL,
  "title" varchar NOT NULL,
  "description" text,
  "message" text NOT NULL,
  "sender_id" integer,
  "enabled" boolean DEFAULT true,
  "account_id" bigint NOT NULL,
  "inbox_id" bigint NOT NULL,
  "trigger_rules" jsonb DEFAULT '{}',
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "campaign_type" integer DEFAULT 0 NOT NULL,
  "campaign_status" integer DEFAULT 0 NOT NULL,
  "audience" jsonb DEFAULT '[]',
  "scheduled_at" timestamp,
  "trigger_only_during_business_hours" boolean DEFAULT false,
  "template_params" jsonb,
  "started_at" timestamp,
  "completed_at" timestamp,
  "whatsapp_cursor" bigint
);
CREATE INDEX "index_campaigns_on_account_id" ON "campaigns" ("account_id");
CREATE INDEX "index_campaigns_on_campaign_status" ON "campaigns" ("campaign_status");
CREATE INDEX "index_campaigns_on_campaign_type" ON "campaigns" ("campaign_type");
CREATE INDEX "index_campaigns_on_inbox_id" ON "campaigns" ("inbox_id");
CREATE INDEX "index_campaigns_on_scheduled_at" ON "campaigns" ("scheduled_at");
CREATE TABLE "canned_responses" (
  "id" serial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "short_code" varchar,
  "content" text,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE TABLE "captain_assistant_responses" (
  "id" bigserial PRIMARY KEY,
  "question" varchar NOT NULL,
  "answer" text NOT NULL,
  "embedding" vector(1536),
  "assistant_id" bigint NOT NULL,
  "documentable_id" bigint,
  "account_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "status" integer DEFAULT 1 NOT NULL,
  "documentable_type" varchar,
  "edited" boolean DEFAULT false NOT NULL
);
CREATE INDEX "index_captain_assistant_responses_on_account_id" ON "captain_assistant_responses" ("account_id");
CREATE INDEX "index_captain_assistant_responses_on_assistant_id" ON "captain_assistant_responses" ("assistant_id");
CREATE INDEX "idx_cap_asst_resp_on_documentable" ON "captain_assistant_responses" ("documentable_id", "documentable_type");
CREATE INDEX "vector_idx_knowledge_entries_embedding" ON "captain_assistant_responses" USING ivfflat ("embedding");
CREATE INDEX "index_captain_assistant_responses_on_status" ON "captain_assistant_responses" ("status");
CREATE TABLE "captain_assistants" (
  "id" bigserial PRIMARY KEY,
  "name" varchar NOT NULL,
  "account_id" bigint NOT NULL,
  "description" text,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "config" jsonb DEFAULT '{}' NOT NULL,
  "response_guidelines" jsonb DEFAULT '[]',
  "guardrails" jsonb DEFAULT '[]'
);
CREATE INDEX "index_captain_assistants_on_account_id" ON "captain_assistants" ("account_id");
CREATE TABLE "captain_custom_tools" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "slug" varchar NOT NULL,
  "title" varchar NOT NULL,
  "description" text,
  "http_method" varchar DEFAULT 'GET' NOT NULL,
  "endpoint_url" text NOT NULL,
  "request_template" text,
  "response_template" text,
  "auth_type" varchar DEFAULT 'none',
  "auth_config" jsonb DEFAULT '{}',
  "param_schema" jsonb DEFAULT '[]',
  "enabled" boolean DEFAULT true NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "assistant_id" bigint,
  "headers" jsonb DEFAULT '{}' NOT NULL,
  "source_metadata" jsonb,
  "settings" jsonb DEFAULT '{}' NOT NULL
);
CREATE INDEX "index_captain_custom_tools_on_account_id" ON "captain_custom_tools" ("account_id");
CREATE UNIQUE INDEX "index_captain_custom_tools_on_assistant_id_and_slug" ON "captain_custom_tools" ("assistant_id", "slug");
CREATE TABLE "captain_documents" (
  "id" bigserial PRIMARY KEY,
  "name" varchar,
  "external_link" text NOT NULL,
  "content" text,
  "assistant_id" bigint NOT NULL,
  "account_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "status" integer DEFAULT 0 NOT NULL,
  "metadata" jsonb DEFAULT '{}',
  "sync_status" integer,
  "last_synced_at" timestamp,
  "last_sync_attempted_at" timestamp
);
CREATE UNIQUE INDEX "idx_captain_documents_on_assistant_id_and_external_link_md5" ON "captain_documents" (assistant_id, md5(external_link));
CREATE INDEX "idx_captain_documents_on_account_assistant_sync_stats" ON "captain_documents" ("account_id", "assistant_id", "sync_status", "last_synced_at");
CREATE INDEX "index_captain_documents_on_account_id_and_sync_status" ON "captain_documents" ("account_id", "sync_status");
CREATE INDEX "index_captain_documents_on_account_id" ON "captain_documents" ("account_id");
CREATE INDEX "index_captain_documents_on_assistant_id" ON "captain_documents" ("assistant_id");
CREATE INDEX "index_captain_documents_on_status" ON "captain_documents" ("status");
CREATE TABLE "captain_faq_observations" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "faq_suggestion_id" bigint,
  "generated_question" varchar NOT NULL,
  "generated_answer" text NOT NULL,
  "language" varchar DEFAULT 'en' NOT NULL,
  "status" integer DEFAULT 0 NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_captain_faq_observations_on_account_id" ON "captain_faq_observations" ("account_id");
CREATE UNIQUE INDEX "idx_captain_faq_observations_on_conversation_and_suggestion" ON "captain_faq_observations" ("conversation_id", "faq_suggestion_id") WHERE (faq_suggestion_id IS NOT NULL);
CREATE INDEX "index_captain_faq_observations_on_conversation_id" ON "captain_faq_observations" ("conversation_id");
CREATE INDEX "index_captain_faq_observations_on_faq_suggestion_id" ON "captain_faq_observations" ("faq_suggestion_id");
CREATE TABLE "captain_faq_suggestions" (
  "id" bigserial PRIMARY KEY,
  "question" varchar NOT NULL,
  "answer" text NOT NULL,
  "embedding" vector(1536),
  "assistant_id" bigint NOT NULL,
  "account_id" bigint NOT NULL,
  "language" varchar DEFAULT 'en' NOT NULL,
  "source_count" integer DEFAULT 0 NOT NULL,
  "status" integer DEFAULT 0 NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_captain_faq_suggestions_on_account_id" ON "captain_faq_suggestions" ("account_id");
CREATE INDEX "idx_cap_faq_suggestions_on_account_assistant_status_language" ON "captain_faq_suggestions" ("account_id", "assistant_id", "status", "language");
CREATE INDEX "index_captain_faq_suggestions_on_assistant_id" ON "captain_faq_suggestions" ("assistant_id");
CREATE INDEX "vector_idx_captain_faq_suggestions_embedding" ON "captain_faq_suggestions" USING ivfflat ("embedding" vector_cosine_ops);
CREATE TABLE "captain_inboxes" (
  "id" bigserial PRIMARY KEY,
  "captain_assistant_id" bigint NOT NULL,
  "inbox_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_captain_inboxes_on_captain_assistant_id_and_inbox_id" ON "captain_inboxes" ("captain_assistant_id", "inbox_id");
CREATE INDEX "index_captain_inboxes_on_captain_assistant_id" ON "captain_inboxes" ("captain_assistant_id");
CREATE INDEX "index_captain_inboxes_on_inbox_id" ON "captain_inboxes" ("inbox_id");
CREATE TABLE "captain_mcp_servers" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "assistant_id" bigint NOT NULL,
  "url" text NOT NULL,
  "name" varchar NOT NULL,
  "auth_type" varchar DEFAULT 'none' NOT NULL,
  "auth_header" varchar,
  "auth_config" jsonb DEFAULT '{}',
  "oauth_credentials" jsonb DEFAULT '{}',
  "status" varchar DEFAULT 'active' NOT NULL,
  "last_error" text,
  "tools_changed_at" timestamp,
  "enabled" boolean DEFAULT true NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_captain_mcp_servers_on_account_id" ON "captain_mcp_servers" ("account_id");
CREATE UNIQUE INDEX "index_captain_mcp_servers_on_assistant_id_and_url" ON "captain_mcp_servers" ("assistant_id", "url");
CREATE TABLE "captain_mcp_tools" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "mcp_server_id" bigint NOT NULL,
  "name" varchar NOT NULL,
  "captain_name" varchar NOT NULL,
  "approved_definition" jsonb DEFAULT '{}' NOT NULL,
  "approved_digest" varchar NOT NULL,
  "param_sources" jsonb DEFAULT '{}' NOT NULL,
  "settings" jsonb DEFAULT '{}' NOT NULL,
  "enabled" boolean DEFAULT true NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_captain_mcp_tools_on_account_id" ON "captain_mcp_tools" ("account_id");
CREATE UNIQUE INDEX "index_captain_mcp_tools_on_captain_name" ON "captain_mcp_tools" ("captain_name");
CREATE UNIQUE INDEX "index_captain_mcp_tools_on_mcp_server_id_and_name" ON "captain_mcp_tools" ("mcp_server_id", "name");
CREATE TABLE "captain_message_reports" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "message_id" bigint NOT NULL,
  "user_id" bigint NOT NULL,
  "report_reason" varchar NOT NULL,
  "description" text,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_captain_message_reports_on_account_id" ON "captain_message_reports" ("account_id");
CREATE INDEX "index_captain_message_reports_on_conversation_id" ON "captain_message_reports" ("conversation_id");
CREATE INDEX "index_captain_message_reports_on_message_id" ON "captain_message_reports" ("message_id");
CREATE INDEX "index_captain_message_reports_on_user_id" ON "captain_message_reports" ("user_id");
CREATE TABLE "captain_scenarios" (
  "id" bigserial PRIMARY KEY,
  "title" varchar,
  "description" text,
  "instruction" text,
  "tools" jsonb DEFAULT '[]',
  "enabled" boolean DEFAULT true NOT NULL,
  "assistant_id" bigint NOT NULL,
  "account_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_captain_scenarios_on_account_id" ON "captain_scenarios" ("account_id");
CREATE INDEX "index_captain_scenarios_on_assistant_id_and_enabled" ON "captain_scenarios" ("assistant_id", "enabled");
CREATE INDEX "index_captain_scenarios_on_assistant_id" ON "captain_scenarios" ("assistant_id");
CREATE INDEX "index_captain_scenarios_on_enabled" ON "captain_scenarios" ("enabled");
CREATE TABLE "categories" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "portal_id" integer NOT NULL,
  "name" varchar,
  "description" text,
  "position" integer,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "locale" varchar DEFAULT 'en',
  "slug" varchar NOT NULL,
  "parent_category_id" bigint,
  "associated_category_id" bigint,
  "icon" varchar DEFAULT '',
  "icon_color" varchar DEFAULT ''
);
CREATE INDEX "index_categories_on_associated_category_id" ON "categories" ("associated_category_id");
CREATE INDEX "index_categories_on_locale_and_account_id" ON "categories" ("locale", "account_id");
CREATE INDEX "index_categories_on_locale" ON "categories" ("locale");
CREATE INDEX "index_categories_on_parent_category_id" ON "categories" ("parent_category_id");
CREATE UNIQUE INDEX "index_categories_on_slug_and_locale_and_portal_id" ON "categories" ("slug", "locale", "portal_id");
CREATE TABLE "channel_api" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "webhook_url" varchar,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "identifier" varchar,
  "hmac_token" varchar,
  "hmac_mandatory" boolean DEFAULT false,
  "additional_attributes" jsonb DEFAULT '{}',
  "secret" varchar
);
CREATE UNIQUE INDEX "index_channel_api_on_hmac_token" ON "channel_api" ("hmac_token");
CREATE UNIQUE INDEX "index_channel_api_on_identifier" ON "channel_api" ("identifier");
CREATE TABLE "channel_email" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "email" varchar NOT NULL,
  "forward_to_email" varchar NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "imap_enabled" boolean DEFAULT false,
  "imap_address" varchar DEFAULT '',
  "imap_port" integer DEFAULT 0,
  "imap_login" varchar DEFAULT '',
  "imap_password" varchar DEFAULT '',
  "imap_enable_ssl" boolean DEFAULT true,
  "smtp_enabled" boolean DEFAULT false,
  "smtp_address" varchar DEFAULT '',
  "smtp_port" integer DEFAULT 0,
  "smtp_login" varchar DEFAULT '',
  "smtp_password" varchar DEFAULT '',
  "smtp_domain" varchar DEFAULT '',
  "smtp_enable_starttls_auto" boolean DEFAULT true,
  "smtp_authentication" varchar DEFAULT 'login',
  "smtp_openssl_verify_mode" varchar DEFAULT 'none',
  "smtp_enable_ssl_tls" boolean DEFAULT false,
  "provider_config" jsonb DEFAULT '{}',
  "provider" varchar,
  "imap_authentication" varchar DEFAULT 'plain',
  "verified_for_sending" boolean DEFAULT false NOT NULL
);
CREATE UNIQUE INDEX "index_channel_email_on_email" ON "channel_email" ("email");
CREATE UNIQUE INDEX "index_channel_email_on_forward_to_email" ON "channel_email" ("forward_to_email");
CREATE TABLE "channel_facebook_pages" (
  "id" serial PRIMARY KEY,
  "page_id" varchar NOT NULL,
  "user_access_token" varchar NOT NULL,
  "page_access_token" varchar NOT NULL,
  "account_id" integer NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "instagram_id" varchar,
  "provider_name" varchar
);
CREATE UNIQUE INDEX "index_channel_facebook_pages_on_page_id_and_account_id" ON "channel_facebook_pages" ("page_id", "account_id");
CREATE INDEX "index_channel_facebook_pages_on_page_id" ON "channel_facebook_pages" ("page_id");
CREATE TABLE "channel_instagram" (
  "id" bigserial PRIMARY KEY,
  "access_token" varchar NOT NULL,
  "expires_at" timestamp NOT NULL,
  "account_id" integer NOT NULL,
  "instagram_id" varchar NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "provider_name" varchar
);
CREATE UNIQUE INDEX "index_channel_instagram_on_instagram_id" ON "channel_instagram" ("instagram_id");
CREATE TABLE "channel_line" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "line_channel_id" varchar NOT NULL,
  "line_channel_secret" varchar NOT NULL,
  "line_channel_token" varchar NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_channel_line_on_line_channel_id" ON "channel_line" ("line_channel_id");
CREATE TABLE "channel_sms" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "phone_number" varchar NOT NULL,
  "provider" varchar DEFAULT 'default',
  "provider_config" jsonb DEFAULT '{}',
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_channel_sms_on_phone_number" ON "channel_sms" ("phone_number");
CREATE TABLE "channel_telegram" (
  "id" bigserial PRIMARY KEY,
  "bot_name" varchar,
  "account_id" integer NOT NULL,
  "bot_token" varchar NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_channel_telegram_on_bot_token" ON "channel_telegram" ("bot_token");
CREATE TABLE "channel_tiktok" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "business_id" varchar NOT NULL,
  "access_token" varchar NOT NULL,
  "expires_at" timestamp NOT NULL,
  "refresh_token" varchar NOT NULL,
  "refresh_token_expires_at" timestamp NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "provider_name" varchar
);
CREATE UNIQUE INDEX "index_channel_tiktok_on_business_id" ON "channel_tiktok" ("business_id");
CREATE TABLE "channel_twilio_sms" (
  "id" bigserial PRIMARY KEY,
  "phone_number" varchar,
  "auth_token" varchar NOT NULL,
  "account_sid" varchar NOT NULL,
  "account_id" integer NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "medium" integer DEFAULT 0,
  "messaging_service_sid" varchar,
  "api_key_sid" varchar,
  "content_templates" jsonb DEFAULT '{}',
  "content_templates_last_updated" timestamp,
  "voice_enabled" boolean DEFAULT false NOT NULL,
  "twiml_app_sid" varchar,
  "api_key_secret" varchar,
  "provider_config" jsonb DEFAULT '{}'
);
CREATE UNIQUE INDEX "index_channel_twilio_sms_on_account_sid_and_phone_number" ON "channel_twilio_sms" ("account_sid", "phone_number");
CREATE UNIQUE INDEX "index_channel_twilio_sms_on_messaging_service_sid" ON "channel_twilio_sms" ("messaging_service_sid");
CREATE UNIQUE INDEX "index_channel_twilio_sms_on_phone_number" ON "channel_twilio_sms" ("phone_number");
CREATE TABLE "channel_twitter_profiles" (
  "id" bigserial PRIMARY KEY,
  "profile_id" varchar NOT NULL,
  "twitter_access_token" varchar NOT NULL,
  "twitter_access_token_secret" varchar NOT NULL,
  "account_id" integer NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "tweets_enabled" boolean DEFAULT true
);
CREATE UNIQUE INDEX "index_channel_twitter_profiles_on_account_id_and_profile_id" ON "channel_twitter_profiles" ("account_id", "profile_id");
CREATE TABLE "channel_web_widgets" (
  "id" serial PRIMARY KEY,
  "website_url" varchar,
  "account_id" integer,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "website_token" varchar,
  "widget_color" varchar DEFAULT '#1f93ff',
  "welcome_title" varchar,
  "welcome_tagline" varchar,
  "feature_flags" integer DEFAULT 7 NOT NULL,
  "reply_time" integer DEFAULT 0,
  "hmac_token" varchar,
  "pre_chat_form_enabled" boolean DEFAULT false,
  "pre_chat_form_options" jsonb DEFAULT '{}',
  "hmac_mandatory" boolean DEFAULT false,
  "continuity_via_email" boolean DEFAULT true NOT NULL,
  "allowed_domains" text DEFAULT ''
);
CREATE UNIQUE INDEX "index_channel_web_widgets_on_hmac_token" ON "channel_web_widgets" ("hmac_token");
CREATE UNIQUE INDEX "index_channel_web_widgets_on_website_token" ON "channel_web_widgets" ("website_token");
CREATE TABLE "channel_whatsapp" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "business_management_token" text,
  "phone_number" varchar NOT NULL,
  "provider" varchar DEFAULT 'default',
  "provider_config" jsonb DEFAULT '{}',
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "message_templates" jsonb DEFAULT '{}',
  "message_templates_last_updated" timestamp,
  "phone_number_health" jsonb DEFAULT '{}' NOT NULL,
  "phone_number_health_checked_at" timestamp,
  "phone_number_health_error" varchar(500)
);
CREATE INDEX "index_channel_whatsapp_on_phone_number_health_checked_at" ON "channel_whatsapp" ("phone_number_health_checked_at");
CREATE UNIQUE INDEX "index_channel_whatsapp_on_phone_number" ON "channel_whatsapp" ("phone_number");
CREATE TABLE "companies" (
  "id" bigserial PRIMARY KEY,
  "name" varchar NOT NULL,
  "domain" varchar,
  "description" text,
  "account_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "contacts_count" integer,
  "additional_attributes" jsonb DEFAULT '{}',
  "custom_attributes" jsonb DEFAULT '{}',
  "last_activity_at" timestamp
);
CREATE UNIQUE INDEX "index_companies_on_account_and_domain" ON "companies" ("account_id", "domain") WHERE (domain IS NOT NULL);
CREATE INDEX "index_companies_on_account_id" ON "companies" ("account_id");
CREATE INDEX "index_companies_on_name_and_account_id" ON "companies" ("name", "account_id");
CREATE TABLE "contact_inboxes" (
  "id" bigserial PRIMARY KEY,
  "contact_id" bigint,
  "inbox_id" bigint,
  "source_id" text NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "hmac_verified" boolean DEFAULT false,
  "pubsub_token" varchar
);
CREATE INDEX "index_contact_inboxes_on_contact_id" ON "contact_inboxes" ("contact_id");
CREATE UNIQUE INDEX "index_contact_inboxes_on_inbox_id_and_source_id" ON "contact_inboxes" ("inbox_id", "source_id");
CREATE INDEX "index_contact_inboxes_on_inbox_id" ON "contact_inboxes" ("inbox_id");
CREATE UNIQUE INDEX "index_contact_inboxes_on_pubsub_token" ON "contact_inboxes" ("pubsub_token");
CREATE INDEX "index_contact_inboxes_on_source_id" ON "contact_inboxes" ("source_id");
CREATE TABLE "contacts" (
  "id" serial PRIMARY KEY,
  "name" varchar DEFAULT '',
  "email" varchar,
  "phone_number" varchar,
  "account_id" integer NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "additional_attributes" jsonb DEFAULT '{}',
  "identifier" varchar,
  "custom_attributes" jsonb DEFAULT '{}',
  "last_activity_at" timestamp,
  "contact_type" integer DEFAULT 0,
  "middle_name" varchar DEFAULT '',
  "last_name" varchar DEFAULT '',
  "location" varchar DEFAULT '',
  "country_code" varchar DEFAULT '',
  "blocked" boolean DEFAULT false NOT NULL,
  "company_id" bigint
);
CREATE INDEX "index_contacts_on_lower_email_account_id" ON "contacts" (lower((email)::text), account_id);
CREATE INDEX "index_contacts_on_account_id_and_contact_type" ON "contacts" ("account_id", "contact_type");
CREATE INDEX "index_contacts_on_nonempty_fields" ON "contacts" ("account_id", "email", "phone_number", "identifier") WHERE (((email)::text <> ''::text) OR ((phone_number)::text <> ''::text) OR ((identifier)::text <> ''::text));
CREATE INDEX "index_contacts_on_account_id_and_last_activity_at" ON "contacts" ("account_id", "last_activity_at" DESC NULLS LAST);
CREATE INDEX "index_contacts_on_account_id" ON "contacts" ("account_id");
CREATE INDEX "index_resolved_contact_account_id" ON "contacts" ("account_id") WHERE (((email)::text <> ''::text) OR ((phone_number)::text <> ''::text) OR ((identifier)::text <> ''::text));
CREATE INDEX "index_contacts_on_blocked" ON "contacts" ("blocked");
CREATE INDEX "index_contacts_on_company_id" ON "contacts" ("company_id");
CREATE UNIQUE INDEX "uniq_email_per_account_contact" ON "contacts" ("email", "account_id");
CREATE UNIQUE INDEX "uniq_identifier_per_account_contact" ON "contacts" ("identifier", "account_id");
CREATE INDEX "index_contacts_on_name_email_phone_number_identifier" ON "contacts" USING gin ("name" gin_trgm_ops, "email" gin_trgm_ops, "phone_number" gin_trgm_ops, "identifier" gin_trgm_ops);
CREATE INDEX "index_contacts_on_phone_number_and_account_id" ON "contacts" ("phone_number", "account_id");
CREATE TABLE "conversation_monitor_automation_deliveries" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "monitor_id" bigint NOT NULL,
  "automation_rule_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "status" varchar DEFAULT 'pending' NOT NULL,
  "skip_reason" varchar,
  "claimed_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_conversation_monitor_automation_deliveries_on_account_id" ON "conversation_monitor_automation_deliveries" ("account_id");
CREATE UNIQUE INDEX "index_monitor_automation_deliveries_unique" ON "conversation_monitor_automation_deliveries" ("automation_rule_id", "monitor_id", "conversation_id");
CREATE INDEX "idx_on_automation_rule_id_c6ed670e50" ON "conversation_monitor_automation_deliveries" ("automation_rule_id");
CREATE INDEX "idx_on_conversation_id_ea2ef1d691" ON "conversation_monitor_automation_deliveries" ("conversation_id");
CREATE INDEX "index_conversation_monitor_automation_deliveries_on_monitor_id" ON "conversation_monitor_automation_deliveries" ("monitor_id");
CREATE INDEX "index_monitor_automation_deliveries_sweep" ON "conversation_monitor_automation_deliveries" ("status", "updated_at");
CREATE TABLE "conversation_monitor_daily_usages" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "usage_date" date NOT NULL,
  "calls_count" integer DEFAULT 0 NOT NULL,
  "limit_reached_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  CONSTRAINT "monitor_daily_usage_nonnegative" CHECK (calls_count >= 0)
);
CREATE UNIQUE INDEX "index_monitor_daily_usage_unique" ON "conversation_monitor_daily_usages" ("account_id", "usage_date");
CREATE TABLE "conversation_monitor_evaluations" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "monitor_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "status" varchar DEFAULT 'pending' NOT NULL,
  "input_revision" bigint DEFAULT 0 NOT NULL,
  "generation" bigint DEFAULT 0 NOT NULL,
  "requested_version" bigint,
  "score" double precision,
  "model" varchar,
  "error_code" varchar,
  "matched_at" timestamp,
  "evaluated_at" timestamp,
  "first_matched_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_conversation_monitor_evaluations_on_account_id" ON "conversation_monitor_evaluations" ("account_id");
CREATE INDEX "index_conversation_monitor_evaluations_on_conversation_id" ON "conversation_monitor_evaluations" ("conversation_id");
CREATE UNIQUE INDEX "index_monitor_evaluations_unique" ON "conversation_monitor_evaluations" ("monitor_id", "conversation_id");
CREATE INDEX "index_monitor_evaluations_status" ON "conversation_monitor_evaluations" ("monitor_id", "status", "conversation_id");
CREATE INDEX "index_conversation_monitor_evaluations_on_monitor_id" ON "conversation_monitor_evaluations" ("monitor_id");
CREATE TABLE "conversation_monitor_scans" (
  "id" bigserial PRIMARY KEY,
  "monitor_id" bigint NOT NULL,
  "kind" varchar NOT NULL,
  "collection_version" bigint NOT NULL,
  "started_at" timestamp NOT NULL,
  "ended_at" timestamp NOT NULL,
  "cursor" bigint DEFAULT 0 NOT NULL,
  "enumerated_at" timestamp,
  "cancelled_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_monitor_scans_version_kind" ON "conversation_monitor_scans" ("monitor_id", "collection_version", "kind");
CREATE UNIQUE INDEX "index_monitor_scans_initial" ON "conversation_monitor_scans" ("monitor_id") WHERE ((kind)::text = 'initial'::text);
CREATE TABLE "conversation_monitor_work_items" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "revision" bigint DEFAULT 0 NOT NULL,
  "processed_revision" bigint DEFAULT 0 NOT NULL,
  "full_history_revision" bigint DEFAULT 0 NOT NULL,
  "generation" bigint DEFAULT 0 NOT NULL,
  "due_at" timestamp,
  "lease_token" varchar,
  "lease_expires_at" timestamp,
  "attempts" integer DEFAULT 0 NOT NULL,
  "error_code" varchar,
  "requested_at" timestamp,
  "activity_at" timestamp,
  "live_activity_revision" bigint DEFAULT 0 NOT NULL,
  "live_activity_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_conversation_monitor_work_items_on_account_id" ON "conversation_monitor_work_items" ("account_id");
CREATE UNIQUE INDEX "index_conversation_monitor_work_items_on_conversation_id" ON "conversation_monitor_work_items" ("conversation_id");
CREATE INDEX "index_monitor_work_due" ON "conversation_monitor_work_items" ("due_at") WHERE (due_at IS NOT NULL);
CREATE TABLE "conversation_monitors" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "user_id" bigint,
  "name" varchar NOT NULL,
  "condition" text NOT NULL,
  "model" varchar NOT NULL,
  "threshold" double precision NOT NULL,
  "history_since" timestamp NOT NULL,
  "paused_at" timestamp,
  "resumed_at" timestamp,
  "deleted_at" timestamp,
  "data_revision" bigint DEFAULT 0 NOT NULL,
  "collection_version" bigint DEFAULT 0 NOT NULL,
  "recheck_requested_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "icon" varchar DEFAULT 'chat-3-line' NOT NULL,
  "icon_color" varchar DEFAULT '#3B82F6' NOT NULL,
  "slack_channel_id" varchar
);
CREATE INDEX "index_conversation_monitors_on_account_id" ON "conversation_monitors" ("account_id");
CREATE INDEX "index_conversation_monitors_on_user_id" ON "conversation_monitors" ("user_id");
CREATE TABLE "conversation_outcomes" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "assistant_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "inbox_id" bigint NOT NULL,
  "first_captain_reply_at" timestamp,
  "last_captain_reply_at" timestamp,
  "captain_reply_count" integer DEFAULT 0 NOT NULL,
  "first_human_reply_at" timestamp,
  "handoff_at" timestamp,
  "handoff_reason_category" varchar,
  "resolved_at" timestamp,
  "csat_rating" integer,
  "csat_received_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "episode_trigger" varchar DEFAULT 'initial' NOT NULL,
  "started_at" timestamp NOT NULL,
  "ended_at" timestamp
);
CREATE INDEX "idx_conversation_outcomes_on_assistant_handoff_at" ON "conversation_outcomes" ("account_id", "assistant_id", "handoff_at");
CREATE INDEX "idx_conversation_outcomes_on_assistant_resolved_at" ON "conversation_outcomes" ("account_id", "assistant_id", "resolved_at");
CREATE INDEX "idx_conversation_outcomes_on_assistant_started_at" ON "conversation_outcomes" ("account_id", "assistant_id", "started_at");
CREATE UNIQUE INDEX "idx_conversation_outcomes_unique_boundary" ON "conversation_outcomes" ("account_id", "conversation_id", "started_at");
CREATE UNIQUE INDEX "idx_conversation_outcomes_initial_episode" ON "conversation_outcomes" ("account_id", "conversation_id") WHERE ((episode_trigger)::text = 'initial'::text);
CREATE UNIQUE INDEX "idx_conversation_outcomes_open_episode" ON "conversation_outcomes" ("account_id", "conversation_id") WHERE (ended_at IS NULL);
CREATE INDEX "index_conversation_outcomes_on_account_id" ON "conversation_outcomes" ("account_id");
CREATE INDEX "index_conversation_outcomes_on_assistant_id" ON "conversation_outcomes" ("assistant_id");
CREATE INDEX "index_conversation_outcomes_on_conversation_id" ON "conversation_outcomes" ("conversation_id");
CREATE INDEX "index_conversation_outcomes_on_inbox_id" ON "conversation_outcomes" ("inbox_id");
CREATE TABLE "conversation_participants" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "user_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_conversation_participants_on_account_id" ON "conversation_participants" ("account_id");
CREATE INDEX "index_conversation_participants_on_conversation_id" ON "conversation_participants" ("conversation_id");
CREATE UNIQUE INDEX "index_conversation_participants_on_user_id_and_conversation_id" ON "conversation_participants" ("user_id", "conversation_id");
CREATE INDEX "index_conversation_participants_on_user_id" ON "conversation_participants" ("user_id");
CREATE TABLE "conversations" (
  "id" serial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "inbox_id" integer NOT NULL,
  "status" integer DEFAULT 0 NOT NULL,
  "assignee_id" integer,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "contact_id" bigint,
  "display_id" integer NOT NULL,
  "contact_last_seen_at" timestamp,
  "agent_last_seen_at" timestamp,
  "additional_attributes" jsonb DEFAULT '{}',
  "contact_inbox_id" bigint,
  "uuid" uuid DEFAULT gen_random_uuid() NOT NULL,
  "identifier" varchar,
  "last_activity_at" timestamp DEFAULT CURRENT_TIMESTAMP NOT NULL,
  "team_id" bigint,
  "campaign_id" bigint,
  "snoozed_until" timestamp,
  "custom_attributes" jsonb DEFAULT '{}',
  "assignee_last_seen_at" timestamp,
  "first_reply_created_at" timestamp,
  "priority" integer,
  "sla_policy_id" bigint,
  "waiting_since" timestamp,
  "cached_label_list" text,
  "assignee_agent_bot_id" bigint,
  "ai_assignee_type" varchar,
  "status_changed_at" timestamp
);
CREATE UNIQUE INDEX "index_conversations_on_account_id_and_display_id" ON "conversations" ("account_id", "display_id");
CREATE INDEX "index_conversations_on_id_and_account_id" ON "conversations" ("account_id", "id");
CREATE INDEX "conv_acid_inbid_stat_asgnid_idx" ON "conversations" ("account_id", "inbox_id", "status", "assignee_id");
CREATE INDEX "index_conversations_on_account_id_status_created_at" ON "conversations" ("account_id", "status", "created_at");
CREATE INDEX "index_conversations_on_account_id" ON "conversations" ("account_id");
CREATE INDEX "index_conversations_on_assignee_id_and_account_id" ON "conversations" ("assignee_id", "account_id");
CREATE INDEX "index_conversations_on_campaign_id" ON "conversations" ("campaign_id");
CREATE INDEX "index_conversations_on_contact_id" ON "conversations" ("contact_id");
CREATE INDEX "index_conversations_on_contact_inbox_id" ON "conversations" ("contact_inbox_id");
CREATE INDEX "index_conversations_on_created_at" ON "conversations" ("created_at");
CREATE INDEX "index_conversations_on_first_reply_created_at" ON "conversations" ("first_reply_created_at");
CREATE INDEX "index_conversations_on_identifier_and_account_id" ON "conversations" ("identifier", "account_id");
CREATE INDEX "index_conversations_on_inbox_id" ON "conversations" ("inbox_id");
CREATE INDEX "index_conversations_on_priority" ON "conversations" ("priority");
CREATE INDEX "index_conversations_on_status_and_account_id" ON "conversations" ("status", "account_id");
CREATE INDEX "index_conversations_on_status_and_priority" ON "conversations" ("status", "priority");
CREATE INDEX "index_conversations_on_team_id" ON "conversations" ("team_id");
CREATE UNIQUE INDEX "index_conversations_on_uuid" ON "conversations" ("uuid");
CREATE INDEX "index_conversations_on_waiting_since" ON "conversations" ("waiting_since");
CREATE TABLE "copilot_action_items" (
  "id" bigserial PRIMARY KEY,
  "copilot_run_id" bigint NOT NULL,
  "record_id" bigint NOT NULL,
  "status" varchar NOT NULL,
  "error" varchar,
  "attempts" integer DEFAULT 0 NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_copilot_action_items_on_copilot_run_id_and_record_id" ON "copilot_action_items" ("copilot_run_id", "record_id");
CREATE TABLE "copilot_messages" (
  "id" bigserial PRIMARY KEY,
  "copilot_thread_id" bigint NOT NULL,
  "account_id" bigint NOT NULL,
  "message" jsonb DEFAULT '{}' NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "message_type" integer DEFAULT 0
);
CREATE INDEX "index_copilot_messages_on_account_id" ON "copilot_messages" ("account_id");
CREATE INDEX "index_copilot_messages_on_copilot_thread_id" ON "copilot_messages" ("copilot_thread_id");
CREATE TABLE "copilot_review_findings" (
  "id" bigserial PRIMARY KEY,
  "copilot_run_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "status" varchar NOT NULL,
  "matched" boolean DEFAULT false NOT NULL,
  "category" varchar,
  "reason" text,
  "screening_score" double precision,
  "evidence_message_ids" jsonb DEFAULT '[]' NOT NULL,
  "reviewed_message_ids" jsonb DEFAULT '[]' NOT NULL,
  "history_truncated" boolean DEFAULT false NOT NULL,
  "attempts" integer DEFAULT 0 NOT NULL,
  "error" varchar,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_copilot_findings_on_run_and_conversation" ON "copilot_review_findings" ("copilot_run_id", "conversation_id");
CREATE TABLE "copilot_runs" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "user_id" bigint NOT NULL,
  "copilot_thread_id" bigint NOT NULL,
  "copilot_message_id" bigint,
  "copilot_tool_call_id" bigint,
  "source_run_id" bigint,
  "type" varchar NOT NULL,
  "status" varchar DEFAULT 'queued' NOT NULL,
  "context" jsonb DEFAULT '{}' NOT NULL,
  "transcript" jsonb DEFAULT '{}' NOT NULL,
  "total_cost_usd" numeric(16,10) DEFAULT '0.0' NOT NULL,
  "cost_complete" boolean DEFAULT true NOT NULL,
  "lease_until" timestamp,
  "lease_token" varchar,
  "attempts" integer DEFAULT 0 NOT NULL,
  "error" varchar,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_copilot_runs_on_account_id" ON "copilot_runs" ("account_id");
CREATE UNIQUE INDEX "index_copilot_runs_on_copilot_message_id" ON "copilot_runs" ("copilot_message_id");
CREATE INDEX "index_copilot_runs_on_copilot_thread_id" ON "copilot_runs" ("copilot_thread_id");
CREATE UNIQUE INDEX "index_copilot_runs_on_copilot_tool_call_id" ON "copilot_runs" ("copilot_tool_call_id");
CREATE INDEX "index_copilot_runs_on_source_run_id" ON "copilot_runs" ("source_run_id");
CREATE INDEX "index_copilot_runs_awaiting_approval" ON "copilot_runs" ("status") WHERE ((status)::text = 'awaiting_approval'::text);
CREATE INDEX "index_copilot_runs_claimable" ON "copilot_runs" ("status") WHERE ((status)::text = ANY ((ARRAY['queued'::character varying, 'running'::character varying])::text[]));
CREATE INDEX "index_copilot_runs_on_user_id" ON "copilot_runs" ("user_id");
CREATE TABLE "copilot_threads" (
  "id" bigserial PRIMARY KEY,
  "title" varchar NOT NULL,
  "user_id" bigint NOT NULL,
  "account_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "assistant_id" integer
);
CREATE INDEX "index_copilot_threads_on_account_id" ON "copilot_threads" ("account_id");
CREATE INDEX "index_copilot_threads_on_assistant_id" ON "copilot_threads" ("assistant_id");
CREATE INDEX "index_copilot_threads_on_user_id" ON "copilot_threads" ("user_id");
CREATE TABLE "copilot_tool_calls" (
  "id" bigserial PRIMARY KEY,
  "copilot_run_id" bigint NOT NULL,
  "call_id" varchar NOT NULL,
  "name" varchar NOT NULL,
  "status" varchar DEFAULT 'queued' NOT NULL,
  "arguments" jsonb DEFAULT '{}' NOT NULL,
  "result" jsonb,
  "attempts" integer DEFAULT 0 NOT NULL,
  "error" varchar,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_copilot_tool_calls_on_copilot_run_id_and_call_id" ON "copilot_tool_calls" ("copilot_run_id", "call_id");
CREATE INDEX "index_copilot_tool_calls_running" ON "copilot_tool_calls" ("status") WHERE ((status)::text = 'running'::text);
CREATE TABLE "csat_survey_responses" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "message_id" bigint NOT NULL,
  "rating" integer NOT NULL,
  "feedback_message" text,
  "contact_id" bigint NOT NULL,
  "assigned_agent_id" bigint,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "csat_review_notes" text,
  "review_notes_updated_at" timestamp,
  "review_notes_updated_by_id" bigint
);
CREATE INDEX "index_csat_survey_responses_on_account_id" ON "csat_survey_responses" ("account_id");
CREATE INDEX "index_csat_survey_responses_on_assigned_agent_id" ON "csat_survey_responses" ("assigned_agent_id");
CREATE INDEX "index_csat_survey_responses_on_contact_id" ON "csat_survey_responses" ("contact_id");
CREATE INDEX "index_csat_survey_responses_on_conversation_id" ON "csat_survey_responses" ("conversation_id");
CREATE UNIQUE INDEX "index_csat_survey_responses_on_message_id" ON "csat_survey_responses" ("message_id");
CREATE INDEX "index_csat_survey_responses_on_review_notes_updated_by_id" ON "csat_survey_responses" ("review_notes_updated_by_id");
CREATE TABLE "custom_attribute_definitions" (
  "id" bigserial PRIMARY KEY,
  "attribute_display_name" varchar,
  "attribute_key" varchar,
  "attribute_display_type" integer DEFAULT 0,
  "default_value" integer,
  "attribute_model" integer DEFAULT 0,
  "account_id" bigint,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "attribute_description" text,
  "attribute_values" jsonb DEFAULT '[]',
  "regex_pattern" varchar,
  "regex_cue" varchar
);
CREATE INDEX "index_custom_attribute_definitions_on_account_id" ON "custom_attribute_definitions" ("account_id");
CREATE UNIQUE INDEX "attribute_key_model_index" ON "custom_attribute_definitions" ("attribute_key", "attribute_model", "account_id");
CREATE TABLE "custom_filters" (
  "id" bigserial PRIMARY KEY,
  "name" varchar NOT NULL,
  "filter_type" integer DEFAULT 0 NOT NULL,
  "query" jsonb DEFAULT '"{}"' NOT NULL,
  "account_id" bigint NOT NULL,
  "user_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_custom_filters_on_account_id" ON "custom_filters" ("account_id");
CREATE INDEX "index_custom_filters_on_user_id" ON "custom_filters" ("user_id");
CREATE TABLE "custom_roles" (
  "id" bigserial PRIMARY KEY,
  "name" varchar,
  "description" varchar,
  "account_id" bigint NOT NULL,
  "permissions" text[] DEFAULT '{}',
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_custom_roles_on_account_id" ON "custom_roles" ("account_id");
CREATE TABLE "dashboard_apps" (
  "id" bigserial PRIMARY KEY,
  "title" varchar NOT NULL,
  "content" jsonb DEFAULT '[]',
  "account_id" bigint NOT NULL,
  "user_id" bigint,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_dashboard_apps_on_account_id" ON "dashboard_apps" ("account_id");
CREATE INDEX "index_dashboard_apps_on_user_id" ON "dashboard_apps" ("user_id");
CREATE TABLE "data_import_errors" (
  "id" bigserial PRIMARY KEY,
  "data_import_id" bigint NOT NULL,
  "data_import_item_id" bigint,
  "source_object_type" varchar,
  "source_object_id" varchar,
  "error_code" varchar NOT NULL,
  "message" text,
  "details" jsonb DEFAULT '{}' NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_data_import_errors_on_data_import_id" ON "data_import_errors" ("data_import_id");
CREATE INDEX "index_data_import_errors_on_data_import_item_id" ON "data_import_errors" ("data_import_item_id");
CREATE INDEX "idx_data_import_errors_on_source" ON "data_import_errors" ("source_object_type", "source_object_id");
CREATE TABLE "data_import_items" (
  "id" bigserial PRIMARY KEY,
  "data_import_id" bigint NOT NULL,
  "source_provider" varchar NOT NULL,
  "source_object_type" varchar NOT NULL,
  "source_object_id" varchar NOT NULL,
  "status" integer DEFAULT 0 NOT NULL,
  "chatwoot_record_type" varchar,
  "chatwoot_record_id" bigint,
  "attempt_count" integer DEFAULT 0 NOT NULL,
  "last_error_code" varchar,
  "last_error_message" text,
  "metadata" jsonb DEFAULT '{}' NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "idx_data_import_items_on_record" ON "data_import_items" ("chatwoot_record_type", "chatwoot_record_id");
CREATE UNIQUE INDEX "idx_data_import_items_on_import_and_source" ON "data_import_items" ("data_import_id", "source_object_type", "source_object_id");
CREATE INDEX "index_data_import_items_on_data_import_id" ON "data_import_items" ("data_import_id");
CREATE INDEX "idx_data_import_items_on_source" ON "data_import_items" ("source_provider", "source_object_type", "source_object_id");
CREATE TABLE "data_import_mappings" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "data_import_id" bigint NOT NULL,
  "source_provider" varchar NOT NULL,
  "source_object_type" varchar NOT NULL,
  "source_object_id" varchar NOT NULL,
  "chatwoot_record_type" varchar NOT NULL,
  "chatwoot_record_id" bigint NOT NULL,
  "metadata" jsonb DEFAULT '{}' NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "idx_data_import_mappings_on_account_and_source" ON "data_import_mappings" ("account_id", "source_provider", "source_object_type", "source_object_id");
CREATE INDEX "idx_data_import_mappings_on_record" ON "data_import_mappings" ("chatwoot_record_type", "chatwoot_record_id");
CREATE INDEX "index_data_import_mappings_on_data_import_id" ON "data_import_mappings" ("data_import_id");
CREATE TABLE "data_imports" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "data_type" varchar NOT NULL,
  "status" integer DEFAULT 0 NOT NULL,
  "processing_errors" text,
  "total_records" integer,
  "processed_records" integer,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "name" varchar,
  "source_type" varchar,
  "source_provider" varchar,
  "import_types" jsonb DEFAULT '[]' NOT NULL,
  "initiated_by_id" integer,
  "access_token" text,
  "source_metadata" jsonb DEFAULT '{}' NOT NULL,
  "stats" jsonb DEFAULT '{}' NOT NULL,
  "cursor" jsonb DEFAULT '{}' NOT NULL,
  "started_at" timestamp,
  "completed_at" timestamp,
  "abandoned_at" timestamp,
  "last_error_at" timestamp
);
CREATE INDEX "index_data_imports_on_account_id" ON "data_imports" ("account_id");
CREATE INDEX "index_data_imports_on_initiated_by_id" ON "data_imports" ("initiated_by_id");
CREATE INDEX "index_data_imports_on_source_provider" ON "data_imports" ("source_provider");
CREATE TABLE "email_templates" (
  "id" bigserial PRIMARY KEY,
  "name" varchar NOT NULL,
  "body" text NOT NULL,
  "account_id" integer,
  "template_type" integer DEFAULT 1,
  "locale" integer DEFAULT 0 NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "inbox_id" integer
);
CREATE UNIQUE INDEX "index_email_templates_on_account_scope" ON "email_templates" ("account_id", "name", "template_type", "locale") WHERE (account_id IS NOT NULL) AND (inbox_id IS NULL);
CREATE UNIQUE INDEX "index_email_templates_on_inbox_scope" ON "email_templates" ("inbox_id", "name", "template_type", "locale") WHERE (inbox_id IS NOT NULL);
CREATE INDEX "index_email_templates_on_inbox_id" ON "email_templates" ("inbox_id");
CREATE UNIQUE INDEX "index_email_templates_on_installation_scope" ON "email_templates" ("name", "template_type", "locale") WHERE (account_id IS NULL) AND (inbox_id IS NULL);
CREATE TABLE "folders" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "category_id" integer NOT NULL,
  "name" varchar,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE TABLE "inbox_assignment_policies" (
  "id" bigserial PRIMARY KEY,
  "inbox_id" bigint NOT NULL,
  "assignment_policy_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_inbox_assignment_policies_on_assignment_policy_id" ON "inbox_assignment_policies" ("assignment_policy_id");
CREATE UNIQUE INDEX "index_inbox_assignment_policies_on_inbox_id" ON "inbox_assignment_policies" ("inbox_id");
CREATE TABLE "inbox_capacity_limits" (
  "id" bigserial PRIMARY KEY,
  "agent_capacity_policy_id" bigint NOT NULL,
  "inbox_id" bigint NOT NULL,
  "conversation_limit" integer NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "idx_on_agent_capacity_policy_id_inbox_id_71c7ec4caf" ON "inbox_capacity_limits" ("agent_capacity_policy_id", "inbox_id");
CREATE INDEX "index_inbox_capacity_limits_on_agent_capacity_policy_id" ON "inbox_capacity_limits" ("agent_capacity_policy_id");
CREATE INDEX "index_inbox_capacity_limits_on_inbox_id" ON "inbox_capacity_limits" ("inbox_id");
CREATE TABLE "inbox_members" (
  "id" serial PRIMARY KEY,
  "user_id" integer NOT NULL,
  "inbox_id" integer NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_inbox_members_on_inbox_id_and_user_id" ON "inbox_members" ("inbox_id", "user_id");
CREATE INDEX "index_inbox_members_on_inbox_id" ON "inbox_members" ("inbox_id");
CREATE TABLE "inboxes" (
  "id" serial PRIMARY KEY,
  "channel_id" integer NOT NULL,
  "account_id" integer NOT NULL,
  "name" varchar NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "channel_type" varchar,
  "enable_auto_assignment" boolean DEFAULT true,
  "greeting_enabled" boolean DEFAULT false,
  "greeting_message" varchar,
  "email_address" varchar,
  "working_hours_enabled" boolean DEFAULT false,
  "out_of_office_message" varchar,
  "timezone" varchar DEFAULT 'UTC',
  "enable_email_collect" boolean DEFAULT true,
  "csat_survey_enabled" boolean DEFAULT false,
  "allow_messages_after_resolved" boolean DEFAULT true,
  "auto_assignment_config" jsonb DEFAULT '{}',
  "lock_to_single_conversation" boolean DEFAULT false NOT NULL,
  "portal_id" bigint,
  "sender_name_type" integer DEFAULT 0 NOT NULL,
  "business_name" varchar,
  "csat_config" jsonb DEFAULT '{}' NOT NULL
);
CREATE INDEX "index_inboxes_on_account_id" ON "inboxes" ("account_id");
CREATE INDEX "index_inboxes_on_channel_id_and_channel_type" ON "inboxes" ("channel_id", "channel_type");
CREATE INDEX "index_inboxes_on_portal_id" ON "inboxes" ("portal_id");
CREATE TABLE "installation_configs" (
  "id" bigserial PRIMARY KEY,
  "name" varchar NOT NULL,
  "serialized_value" jsonb DEFAULT '{}' NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "locked" boolean DEFAULT true NOT NULL
);
CREATE UNIQUE INDEX "index_installation_configs_on_name_and_created_at" ON "installation_configs" ("name", "created_at");
CREATE UNIQUE INDEX "index_installation_configs_on_name" ON "installation_configs" ("name");
CREATE TABLE "integrations_hooks" (
  "id" bigserial PRIMARY KEY,
  "status" integer DEFAULT 1,
  "inbox_id" integer,
  "account_id" integer,
  "app_id" varchar,
  "hook_type" integer DEFAULT 0,
  "reference_id" varchar,
  "access_token" varchar,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "settings" jsonb DEFAULT '{}'
);
CREATE TABLE "labels" (
  "id" bigserial PRIMARY KEY,
  "title" varchar,
  "description" text,
  "color" varchar DEFAULT '#1f93ff' NOT NULL,
  "show_on_sidebar" boolean,
  "account_id" bigint,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_labels_on_account_id" ON "labels" ("account_id");
CREATE UNIQUE INDEX "index_labels_on_title_and_account_id" ON "labels" ("title", "account_id");
CREATE TABLE "leaves" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "user_id" bigint NOT NULL,
  "start_date" date NOT NULL,
  "end_date" date NOT NULL,
  "leave_type" integer DEFAULT 0 NOT NULL,
  "status" integer DEFAULT 0 NOT NULL,
  "reason" text,
  "approved_by_id" bigint,
  "approved_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_leaves_on_account_id_and_status" ON "leaves" ("account_id", "status");
CREATE INDEX "index_leaves_on_account_id" ON "leaves" ("account_id");
CREATE INDEX "index_leaves_on_approved_by_id" ON "leaves" ("approved_by_id");
CREATE INDEX "index_leaves_on_user_id" ON "leaves" ("user_id");
CREATE TABLE "macros" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "name" varchar NOT NULL,
  "visibility" integer DEFAULT 0,
  "created_by_id" bigint,
  "updated_by_id" bigint,
  "actions" jsonb DEFAULT '{}' NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_macros_on_account_id" ON "macros" ("account_id");
CREATE TABLE "mentions" (
  "id" bigserial PRIMARY KEY,
  "user_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "account_id" bigint NOT NULL,
  "mentioned_at" timestamp NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_mentions_on_account_id" ON "mentions" ("account_id");
CREATE INDEX "index_mentions_on_conversation_id" ON "mentions" ("conversation_id");
CREATE UNIQUE INDEX "index_mentions_on_user_id_and_conversation_id" ON "mentions" ("user_id", "conversation_id");
CREATE INDEX "index_mentions_on_user_id" ON "mentions" ("user_id");
CREATE TABLE "messages" (
  "id" serial PRIMARY KEY,
  "content" text,
  "account_id" integer NOT NULL,
  "inbox_id" integer NOT NULL,
  "conversation_id" integer NOT NULL,
  "message_type" integer NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "private" boolean DEFAULT false NOT NULL,
  "status" integer DEFAULT 0,
  "source_id" text,
  "content_type" integer DEFAULT 0 NOT NULL,
  "content_attributes" json DEFAULT '{}',
  "sender_type" varchar,
  "sender_id" bigint,
  "external_source_ids" jsonb DEFAULT '{}',
  "additional_attributes" jsonb DEFAULT '{}',
  "processed_message_content" text,
  "sentiment" jsonb DEFAULT '{}'
);
CREATE INDEX "index_messages_on_additional_attributes_campaign_id" ON "messages" USING gin (((additional_attributes -> 'campaign_id'::text)));
CREATE INDEX "idx_messages_account_content_created" ON "messages" ("account_id", "content_type", "created_at");
CREATE INDEX "index_messages_on_account_created_type" ON "messages" ("account_id", "created_at", "message_type");
CREATE INDEX "index_messages_on_account_id_and_inbox_id" ON "messages" ("account_id", "inbox_id");
CREATE INDEX "index_messages_on_account_id" ON "messages" ("account_id");
CREATE INDEX "index_messages_on_content" ON "messages" USING gin ("content" gin_trgm_ops);
CREATE INDEX "index_messages_on_conversation_account_type_created" ON "messages" ("conversation_id", "account_id", "message_type", "created_at");
CREATE INDEX "index_messages_on_conversation_id" ON "messages" ("conversation_id");
CREATE INDEX "index_messages_on_created_at" ON "messages" ("created_at");
CREATE INDEX "index_messages_on_inbox_id" ON "messages" ("inbox_id");
CREATE INDEX "index_messages_on_sender_and_created" ON "messages" ("sender_type", "sender_id", "created_at");
CREATE INDEX "index_messages_on_sender_type_and_sender_id" ON "messages" ("sender_type", "sender_id");
CREATE INDEX "index_messages_on_source_id" ON "messages" ("source_id");
CREATE TABLE "notes" (
  "id" bigserial PRIMARY KEY,
  "content" text NOT NULL,
  "account_id" bigint NOT NULL,
  "contact_id" bigint NOT NULL,
  "user_id" bigint,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_notes_on_account_id" ON "notes" ("account_id");
CREATE INDEX "index_notes_on_contact_id" ON "notes" ("contact_id");
CREATE INDEX "index_notes_on_user_id" ON "notes" ("user_id");
CREATE TABLE "notification_settings" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer,
  "user_id" integer,
  "email_flags" integer DEFAULT 0 NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "push_flags" integer DEFAULT 0 NOT NULL
);
CREATE UNIQUE INDEX "by_account_user" ON "notification_settings" ("account_id", "user_id");
CREATE TABLE "notification_subscriptions" (
  "id" bigserial PRIMARY KEY,
  "user_id" bigint NOT NULL,
  "subscription_type" integer NOT NULL,
  "subscription_attributes" jsonb DEFAULT '{}' NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "identifier" text
);
CREATE UNIQUE INDEX "index_notification_subscriptions_on_identifier" ON "notification_subscriptions" ("identifier");
CREATE INDEX "index_notification_subscriptions_on_user_id" ON "notification_subscriptions" ("user_id");
CREATE TABLE "notifications" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "user_id" bigint NOT NULL,
  "notification_type" integer NOT NULL,
  "primary_actor_type" varchar NOT NULL,
  "primary_actor_id" bigint NOT NULL,
  "secondary_actor_type" varchar,
  "secondary_actor_id" bigint,
  "read_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "snoozed_until" timestamp,
  "last_activity_at" timestamp DEFAULT CURRENT_TIMESTAMP,
  "meta" jsonb DEFAULT '{}'
);
CREATE INDEX "index_notifications_on_account_id" ON "notifications" ("account_id");
CREATE INDEX "index_notifications_on_last_activity_at" ON "notifications" ("last_activity_at");
CREATE INDEX "uniq_primary_actor_per_account_notifications" ON "notifications" ("primary_actor_type", "primary_actor_id");
CREATE INDEX "uniq_secondary_actor_per_account_notifications" ON "notifications" ("secondary_actor_type", "secondary_actor_id");
CREATE INDEX "idx_notifications_performance" ON "notifications" ("user_id", "account_id", "snoozed_until", "read_at");
CREATE INDEX "index_notifications_on_user_id" ON "notifications" ("user_id");
CREATE TABLE "platform_app_permissibles" (
  "id" bigserial PRIMARY KEY,
  "platform_app_id" bigint NOT NULL,
  "permissible_type" varchar NOT NULL,
  "permissible_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_platform_app_permissibles_on_permissibles" ON "platform_app_permissibles" ("permissible_type", "permissible_id");
CREATE UNIQUE INDEX "unique_permissibles_index" ON "platform_app_permissibles" ("platform_app_id", "permissible_id", "permissible_type");
CREATE INDEX "index_platform_app_permissibles_on_platform_app_id" ON "platform_app_permissibles" ("platform_app_id");
CREATE TABLE "platform_apps" (
  "id" bigserial PRIMARY KEY,
  "name" varchar NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE TABLE "platform_banners" (
  "id" bigserial PRIMARY KEY,
  "banner_message" text NOT NULL,
  "banner_type" integer DEFAULT 0 NOT NULL,
  "active" boolean DEFAULT true NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "title" varchar,
  "video_url" varchar
);
CREATE TABLE "portals" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "name" varchar NOT NULL,
  "slug" varchar NOT NULL,
  "custom_domain" varchar,
  "color" varchar,
  "homepage_link" varchar,
  "page_title" varchar,
  "header_text" text,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "config" jsonb DEFAULT '{"allowed_locales":["en"]}',
  "archived" boolean DEFAULT false,
  "channel_web_widget_id" bigint,
  "ssl_settings" jsonb DEFAULT '{}' NOT NULL
);
CREATE INDEX "index_portals_on_channel_web_widget_id" ON "portals" ("channel_web_widget_id");
CREATE UNIQUE INDEX "index_portals_on_custom_domain" ON "portals" ("custom_domain");
CREATE UNIQUE INDEX "index_portals_on_slug" ON "portals" ("slug");
CREATE TABLE "portals_members" (
  "portal_id" bigint NOT NULL,
  "user_id" bigint NOT NULL
);
CREATE UNIQUE INDEX "index_portals_members_on_portal_id_and_user_id" ON "portals_members" ("portal_id", "user_id");
CREATE INDEX "index_portals_members_on_portal_id" ON "portals_members" ("portal_id");
CREATE INDEX "index_portals_members_on_user_id" ON "portals_members" ("user_id");
CREATE TABLE "related_categories" (
  "id" bigserial PRIMARY KEY,
  "category_id" bigint,
  "related_category_id" bigint,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_related_categories_on_category_id_and_related_category_id" ON "related_categories" ("category_id", "related_category_id");
CREATE UNIQUE INDEX "index_related_categories_on_related_category_id_and_category_id" ON "related_categories" ("related_category_id", "category_id");
CREATE TABLE "reporting_events" (
  "id" bigserial PRIMARY KEY,
  "name" varchar,
  "value" double precision,
  "account_id" integer,
  "inbox_id" integer,
  "user_id" integer,
  "conversation_id" integer,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "value_in_business_hours" double precision,
  "event_start_time" timestamp,
  "event_end_time" timestamp
);
CREATE INDEX "reporting_events__account_id__name__created_at" ON "reporting_events" ("account_id", "name", "created_at");
CREATE INDEX "index_reporting_events_for_response_distribution" ON "reporting_events" ("account_id", "name", "inbox_id", "created_at");
CREATE INDEX "index_reporting_events_on_account_id" ON "reporting_events" ("account_id");
CREATE INDEX "index_reporting_events_on_conversation_id" ON "reporting_events" ("conversation_id");
CREATE INDEX "index_reporting_events_on_created_at" ON "reporting_events" ("created_at");
CREATE INDEX "index_reporting_events_on_inbox_id" ON "reporting_events" ("inbox_id");
CREATE INDEX "index_reporting_events_on_name" ON "reporting_events" ("name");
CREATE INDEX "index_reporting_events_on_user_id" ON "reporting_events" ("user_id");
CREATE TABLE "reporting_events_rollups" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer NOT NULL,
  "date" date NOT NULL,
  "dimension_type" varchar NOT NULL,
  "dimension_id" bigint NOT NULL,
  "metric" varchar NOT NULL,
  "count" bigint DEFAULT 0 NOT NULL,
  "sum_value" double precision DEFAULT 0.0 NOT NULL,
  "sum_value_business_hours" double precision DEFAULT 0.0 NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_rollup_unique_key" ON "reporting_events_rollups" ("account_id", "date", "dimension_type", "dimension_id", "metric");
CREATE INDEX "index_rollup_summary" ON "reporting_events_rollups" ("account_id", "dimension_type", "date");
CREATE INDEX "index_rollup_timeseries" ON "reporting_events_rollups" ("account_id", "metric", "date");
CREATE TABLE "shopify_custom_apps" (
  "id" bigserial PRIMARY KEY,
  "account_id" bigint NOT NULL,
  "shop_domain" varchar NOT NULL,
  "client_id" varchar NOT NULL,
  "client_secret" text NOT NULL,
  "install_url" text NOT NULL,
  "enabled" boolean DEFAULT true NOT NULL,
  "installation_generation" integer DEFAULT 0 NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_shopify_custom_apps_on_account_id" ON "shopify_custom_apps" ("account_id");
CREATE UNIQUE INDEX "index_shopify_custom_apps_on_client_id" ON "shopify_custom_apps" ("client_id");
CREATE UNIQUE INDEX "index_shopify_custom_apps_on_shop_domain" ON "shopify_custom_apps" ("shop_domain");
CREATE TABLE "sla_events" (
  "id" bigserial PRIMARY KEY,
  "applied_sla_id" bigint NOT NULL,
  "conversation_id" bigint NOT NULL,
  "account_id" bigint NOT NULL,
  "sla_policy_id" bigint NOT NULL,
  "inbox_id" bigint NOT NULL,
  "event_type" integer,
  "meta" jsonb DEFAULT '{}',
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE INDEX "index_sla_events_on_account_id" ON "sla_events" ("account_id");
CREATE INDEX "index_sla_events_on_applied_sla_id" ON "sla_events" ("applied_sla_id");
CREATE INDEX "index_sla_events_on_conversation_id" ON "sla_events" ("conversation_id");
CREATE INDEX "index_sla_events_on_inbox_id" ON "sla_events" ("inbox_id");
CREATE INDEX "index_sla_events_on_sla_policy_id" ON "sla_events" ("sla_policy_id");
CREATE TABLE "sla_policies" (
  "id" bigserial PRIMARY KEY,
  "name" varchar NOT NULL,
  "first_response_time_threshold" double precision,
  "next_response_time_threshold" double precision,
  "only_during_business_hours" boolean DEFAULT false,
  "account_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "description" varchar,
  "resolution_time_threshold" double precision
);
CREATE INDEX "index_sla_policies_on_account_id" ON "sla_policies" ("account_id");
CREATE TABLE "taggings" (
  "id" serial PRIMARY KEY,
  "tag_id" integer,
  "taggable_type" varchar,
  "taggable_id" integer,
  "tagger_type" varchar,
  "tagger_id" integer,
  "context" varchar(128),
  "created_at" timestamp
);
CREATE INDEX "index_taggings_on_context" ON "taggings" ("context");
CREATE UNIQUE INDEX "taggings_idx" ON "taggings" ("tag_id", "taggable_id", "taggable_type", "context", "tagger_id", "tagger_type");
CREATE INDEX "index_taggings_on_tag_id" ON "taggings" ("tag_id");
CREATE INDEX "index_taggings_on_taggable_id_and_taggable_type_and_context" ON "taggings" ("taggable_id", "taggable_type", "context");
CREATE INDEX "taggings_idy" ON "taggings" ("taggable_id", "taggable_type", "tagger_id", "context");
CREATE INDEX "index_taggings_on_taggable_id" ON "taggings" ("taggable_id");
CREATE INDEX "index_taggings_on_taggable_type" ON "taggings" ("taggable_type");
CREATE INDEX "index_taggings_on_tagger_id_and_tagger_type" ON "taggings" ("tagger_id", "tagger_type");
CREATE INDEX "index_taggings_on_tagger_id" ON "taggings" ("tagger_id");
CREATE TABLE "tags" (
  "id" serial PRIMARY KEY,
  "name" varchar,
  "taggings_count" integer DEFAULT 0
);
CREATE INDEX "tags_name_trgm_idx" ON "tags" USING gin (lower((name)::text) gin_trgm_ops);
CREATE UNIQUE INDEX "index_tags_on_name" ON "tags" ("name");
CREATE TABLE "team_members" (
  "id" bigserial PRIMARY KEY,
  "team_id" bigint NOT NULL,
  "user_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_team_members_on_team_id_and_user_id" ON "team_members" ("team_id", "user_id");
CREATE INDEX "index_team_members_on_team_id" ON "team_members" ("team_id");
CREATE INDEX "index_team_members_on_user_id" ON "team_members" ("user_id");
CREATE TABLE "teams" (
  "id" bigserial PRIMARY KEY,
  "name" varchar NOT NULL,
  "description" text,
  "allow_auto_assign" boolean DEFAULT true,
  "account_id" bigint NOT NULL,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "icon" varchar DEFAULT '',
  "icon_color" varchar DEFAULT ''
);
CREATE INDEX "index_teams_on_account_id" ON "teams" ("account_id");
CREATE UNIQUE INDEX "index_teams_on_name_and_account_id" ON "teams" ("name", "account_id");
CREATE TABLE "user_sessions" (
  "id" bigserial PRIMARY KEY,
  "user_id" bigint NOT NULL,
  "client_id" varchar NOT NULL,
  "ip_address" varchar,
  "user_agent" varchar,
  "browser_name" varchar,
  "browser_version" varchar,
  "device_name" varchar,
  "platform_name" varchar,
  "platform_version" varchar,
  "city" varchar,
  "country" varchar,
  "country_code" varchar,
  "last_activity_at" timestamp,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL
);
CREATE UNIQUE INDEX "index_user_sessions_on_user_id_and_client_id" ON "user_sessions" ("user_id", "client_id");
CREATE INDEX "index_user_sessions_on_user_id" ON "user_sessions" ("user_id");
CREATE TABLE "users" (
  "id" serial PRIMARY KEY,
  "provider" varchar DEFAULT 'email' NOT NULL,
  "uid" varchar DEFAULT '' NOT NULL,
  "encrypted_password" varchar DEFAULT '' NOT NULL,
  "reset_password_token" varchar,
  "reset_password_sent_at" timestamp,
  "remember_created_at" timestamp,
  "sign_in_count" integer DEFAULT 0 NOT NULL,
  "current_sign_in_at" timestamp,
  "last_sign_in_at" timestamp,
  "current_sign_in_ip" varchar,
  "last_sign_in_ip" varchar,
  "confirmation_token" varchar,
  "confirmed_at" timestamp,
  "confirmation_sent_at" timestamp,
  "unconfirmed_email" varchar,
  "name" varchar NOT NULL,
  "display_name" varchar,
  "email" varchar,
  "tokens" json,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "pubsub_token" varchar,
  "availability" integer DEFAULT 0,
  "ui_settings" jsonb DEFAULT '{}',
  "custom_attributes" jsonb DEFAULT '{}',
  "type" varchar,
  "message_signature" text,
  "otp_secret" varchar,
  "consumed_timestep" integer,
  "otp_required_for_login" boolean DEFAULT false,
  "otp_backup_codes" text,
  "device_trust_version" integer DEFAULT 0 NOT NULL
);
CREATE INDEX "index_users_on_email" ON "users" ("email");
CREATE INDEX "index_users_on_otp_required_for_login" ON "users" ("otp_required_for_login");
CREATE UNIQUE INDEX "index_users_on_otp_secret" ON "users" ("otp_secret");
CREATE UNIQUE INDEX "index_users_on_pubsub_token" ON "users" ("pubsub_token");
CREATE UNIQUE INDEX "index_users_on_reset_password_token" ON "users" ("reset_password_token");
CREATE UNIQUE INDEX "index_users_on_uid_and_provider" ON "users" ("uid", "provider");
CREATE TABLE "webhooks" (
  "id" bigserial PRIMARY KEY,
  "account_id" integer,
  "inbox_id" integer,
  "url" text,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "webhook_type" integer DEFAULT 0,
  "subscriptions" jsonb DEFAULT '["conversation_status_changed","conversation_updated","conversation_created","contact_created","contact_updated","message_created","message_updated","webwidget_triggered"]',
  "name" varchar,
  "secret" varchar
);
CREATE UNIQUE INDEX "index_webhooks_on_account_id_and_url" ON "webhooks" ("account_id", "url");
CREATE TABLE "working_hours" (
  "id" bigserial PRIMARY KEY,
  "inbox_id" bigint,
  "account_id" bigint,
  "day_of_week" integer NOT NULL,
  "closed_all_day" boolean DEFAULT false,
  "open_hour" integer,
  "open_minutes" integer,
  "close_hour" integer,
  "close_minutes" integer,
  "created_at" timestamp NOT NULL,
  "updated_at" timestamp NOT NULL,
  "open_all_day" boolean DEFAULT false
);
CREATE INDEX "index_working_hours_on_account_id" ON "working_hours" ("account_id");
CREATE INDEX "index_working_hours_on_inbox_id" ON "working_hours" ("inbox_id");
ALTER TABLE "active_storage_attachments" ADD FOREIGN KEY ("blob_id") REFERENCES "active_storage_blobs"(id);
ALTER TABLE "active_storage_variant_records" ADD FOREIGN KEY ("blob_id") REFERENCES "active_storage_blobs"(id);
ALTER TABLE "campaign_recipients" ADD FOREIGN KEY ("account_id") REFERENCES "accounts"(id) ON DELETE CASCADE;
ALTER TABLE "campaign_recipients" ADD FOREIGN KEY ("campaign_id") REFERENCES "campaigns"(id) ON DELETE CASCADE;
ALTER TABLE "campaign_recipients" ADD FOREIGN KEY ("contact_id") REFERENCES "contacts"(id) ON DELETE CASCADE;
ALTER TABLE "campaign_recipients" ADD FOREIGN KEY ("inbox_id") REFERENCES "inboxes"(id) ON DELETE CASCADE;
ALTER TABLE "conversation_monitor_daily_usages" ADD FOREIGN KEY ("account_id") REFERENCES "accounts"(id) ON DELETE CASCADE;
ALTER TABLE "conversation_monitor_evaluations" ADD FOREIGN KEY ("account_id") REFERENCES "accounts"(id) ON DELETE CASCADE;
ALTER TABLE "conversation_monitor_evaluations" ADD FOREIGN KEY ("monitor_id") REFERENCES "conversation_monitors"(id) ON DELETE CASCADE;
ALTER TABLE "conversation_monitor_evaluations" ADD FOREIGN KEY ("conversation_id") REFERENCES "conversations"(id) ON DELETE CASCADE;
ALTER TABLE "conversation_monitor_scans" ADD FOREIGN KEY ("monitor_id") REFERENCES "conversation_monitors"(id) ON DELETE CASCADE;
ALTER TABLE "conversation_monitor_work_items" ADD FOREIGN KEY ("account_id") REFERENCES "accounts"(id) ON DELETE CASCADE;
ALTER TABLE "conversation_monitor_work_items" ADD FOREIGN KEY ("conversation_id") REFERENCES "conversations"(id) ON DELETE CASCADE;
ALTER TABLE "conversation_monitors" ADD FOREIGN KEY ("account_id") REFERENCES "accounts"(id) ON DELETE CASCADE;
ALTER TABLE "conversation_monitors" ADD FOREIGN KEY ("user_id") REFERENCES "users"(id) ON DELETE SET NULL;
ALTER TABLE "inboxes" ADD FOREIGN KEY ("portal_id") REFERENCES "portals"(id);
ALTER TABLE "shopify_custom_apps" ADD FOREIGN KEY ("account_id") REFERENCES "accounts"(id);
ALTER TABLE "user_sessions" ADD FOREIGN KEY ("user_id") REFERENCES "users"(id);
CREATE OR REPLACE FUNCTION accounts_after_insert_row_tr() RETURNS TRIGGER AS $$ BEGIN execute format('create sequence IF NOT EXISTS conv_dpid_seq_%s', NEW.id); RETURN NULL; END; $$ LANGUAGE plpgsql;
CREATE TRIGGER accounts_after_insert_row_tr AFTER INSERT ON "accounts" FOR EACH ROW EXECUTE FUNCTION accounts_after_insert_row_tr();

CREATE OR REPLACE FUNCTION conversations_before_insert_row_tr() RETURNS TRIGGER AS $$ BEGIN NEW.display_id := nextval('conv_dpid_seq_' || NEW.account_id); RETURN NEW; END; $$ LANGUAGE plpgsql;
CREATE TRIGGER conversations_before_insert_row_tr BEFORE INSERT ON "conversations" FOR EACH ROW EXECUTE FUNCTION conversations_before_insert_row_tr();

CREATE OR REPLACE FUNCTION camp_dpid_before_insert() RETURNS TRIGGER AS $$ BEGIN execute format('create sequence IF NOT EXISTS camp_dpid_seq_%s', NEW.id); RETURN NULL; END; $$ LANGUAGE plpgsql;
CREATE TRIGGER camp_dpid_before_insert AFTER INSERT ON "accounts" FOR EACH ROW EXECUTE FUNCTION camp_dpid_before_insert();

CREATE OR REPLACE FUNCTION campaigns_before_insert_row_tr() RETURNS TRIGGER AS $$ BEGIN NEW.display_id := nextval('camp_dpid_seq_' || NEW.account_id); RETURN NEW; END; $$ LANGUAGE plpgsql;
CREATE TRIGGER campaigns_before_insert_row_tr BEFORE INSERT ON "campaigns" FOR EACH ROW EXECUTE FUNCTION campaigns_before_insert_row_tr();

CREATE TABLE IF NOT EXISTS schema_migrations (version varchar PRIMARY KEY);
CREATE TABLE IF NOT EXISTS ar_internal_metadata (key varchar PRIMARY KEY, value varchar, created_at timestamp(6) NOT NULL, updated_at timestamp(6) NOT NULL);
INSERT INTO schema_migrations VALUES ('20261103142055') ON CONFLICT DO NOTHING;
