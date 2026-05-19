-- ============================================================================
-- Migration: add token_plane to api_tokens
-- Created: 2026-05-14
-- Description: Adds token_plane metadata required by /api/v1/tokens responses
-- and backfills existing records so list/get handlers can read the new column.
-- ============================================================================

ALTER TABLE api_tokens
    ADD COLUMN IF NOT EXISTS token_plane VARCHAR(32);

UPDATE api_tokens
SET token_plane = CASE
    WHEN token_plane IS NOT NULL AND token_plane <> '' THEN token_plane
    WHEN issued_from = 'session' THEN 'management'
    ELSE 'runtime'
END
WHERE token_plane IS NULL OR token_plane = '';

ALTER TABLE api_tokens
    ALTER COLUMN token_plane SET DEFAULT 'runtime',
    ALTER COLUMN token_plane SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_api_tokens_tenant_plane_created
    ON api_tokens(tenant_id, token_plane, created_at DESC);
