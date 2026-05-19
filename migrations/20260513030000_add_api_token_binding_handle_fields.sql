ALTER TABLE api_tokens
    ADD COLUMN IF NOT EXISTS token_plane VARCHAR(32) NOT NULL DEFAULT 'management';

ALTER TABLE api_tokens
    ADD COLUMN IF NOT EXISTS binding_handles JSONB NOT NULL DEFAULT '[]'::jsonb;
