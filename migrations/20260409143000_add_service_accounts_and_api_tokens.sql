-- ============================================================================
-- Migration: service accounts 与 API token 元数据
-- Created: 2026-04-09
-- Description: 为独立 service account 主体与 access token 持久化元数据提供基础表
-- ============================================================================

-- ============================================================================
-- 1. service_accounts
-- ============================================================================
CREATE TABLE IF NOT EXISTS service_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    role VARCHAR(64) NOT NULL DEFAULT 'service_account',
    scope_ceiling JSONB NOT NULL DEFAULT '[]'::jsonb,
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    CONSTRAINT uq_service_accounts_tenant_name UNIQUE (tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_service_accounts_tenant_created
    ON service_accounts(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_service_accounts_tenant_status
    ON service_accounts(tenant_id, status)
    WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_service_accounts_deleted_at
    ON service_accounts(deleted_at)
    WHERE deleted_at IS NOT NULL;

DROP TRIGGER IF EXISTS tr_service_accounts_updated_at ON service_accounts;
CREATE TRIGGER tr_service_accounts_updated_at
    BEFORE UPDATE ON service_accounts
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- 2. api_tokens
-- ============================================================================
CREATE TABLE IF NOT EXISTS api_tokens (
    id VARCHAR(128) PRIMARY KEY,
    token_type VARCHAR(64) NOT NULL,
    subject_type VARCHAR(64) NOT NULL,
    subject_id UUID NOT NULL,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    issued_from VARCHAR(64) NOT NULL,
    session_id UUID REFERENCES auth_sessions(id) ON DELETE SET NULL,
    membership_id UUID REFERENCES tenant_memberships(id) ON DELETE SET NULL,
    display_name VARCHAR(128),
    scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_tenant_created
    ON api_tokens(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_api_tokens_subject
    ON api_tokens(subject_type, subject_id);
CREATE INDEX IF NOT EXISTS idx_api_tokens_revoked
    ON api_tokens(revoked_at);
CREATE INDEX IF NOT EXISTS idx_api_tokens_expires
    ON api_tokens(expires_at);
