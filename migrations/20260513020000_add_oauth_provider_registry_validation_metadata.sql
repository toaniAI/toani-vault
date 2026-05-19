ALTER TABLE oauth_provider_definitions
    ADD COLUMN IF NOT EXISTS version INTEGER NOT NULL DEFAULT 1;

ALTER TABLE oauth_provider_definitions
    ADD COLUMN IF NOT EXISTS client_auth_method VARCHAR(64);

ALTER TABLE oauth_provider_definitions
    ADD COLUMN IF NOT EXISTS adapter_version VARCHAR(64);
