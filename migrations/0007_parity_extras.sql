-- Pulsar Parity Extras: SSO tokens, distributed rate limiting, message errors, and custom domains.
-- Idempotent additive tables only; stock Chatwoot tables remain untouched.

CREATE TABLE IF NOT EXISTS pulsar_sso_tokens (
    token TEXT PRIMARY KEY,
    user_id BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS index_pulsar_sso_tokens_expires ON pulsar_sso_tokens (expires_at);

CREATE TABLE IF NOT EXISTS pulsar_rate_limits (
    bucket TEXT PRIMARY KEY,
    hits INTEGER NOT NULL,
    window_start TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS pulsar_message_errors (
    message_id BIGINT PRIMARY KEY,
    external_error TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS pulsar_custom_domains (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL,
    domain VARCHAR NOT NULL,
    status VARCHAR NOT NULL DEFAULT 'pending',
    verification_token VARCHAR NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, domain)
);
CREATE INDEX IF NOT EXISTS index_pulsar_custom_domains_account ON pulsar_custom_domains (account_id);
