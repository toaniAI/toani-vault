-- ============================================================================
-- Migration: extend api_tokens for user automation token management
-- Created: 2026-04-10
-- Description: Adds product-facing metadata needed for profile-managed automation tokens
-- ============================================================================

ALTER TABLE api_tokens
    ADD COLUMN IF NOT EXISTS token_kind VARCHAR(64) NOT NULL DEFAULT 'user_access_token',
    ADD COLUMN IF NOT EXISTS token_name VARCHAR(128),
    ADD COLUMN IF NOT EXISTS token_prefix VARCHAR(64),
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS issued_membership_role_snapshot VARCHAR(64),
    ADD COLUMN IF NOT EXISTS permission_source VARCHAR(64),
    ADD COLUMN IF NOT EXISTS created_via VARCHAR(64),
    ADD COLUMN IF NOT EXISTS revoked_reason TEXT,
    ADD COLUMN IF NOT EXISTS oauth_client_id VARCHAR(128),
    ADD COLUMN IF NOT EXISTS oauth_grant_type VARCHAR(64),
    ADD COLUMN IF NOT EXISTS oauth_subject_mode VARCHAR(64);

UPDATE api_tokens
SET token_kind = CASE
    WHEN subject_type = 'service_account' THEN 'service_account'
    ELSE 'user_access_token'
END
WHERE token_kind IS NULL OR token_kind = '';

CREATE INDEX IF NOT EXISTS idx_api_tokens_tenant_kind_created
    ON api_tokens(tenant_id, token_kind, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_api_tokens_tenant_subject_kind
    ON api_tokens(tenant_id, subject_id, token_kind);
