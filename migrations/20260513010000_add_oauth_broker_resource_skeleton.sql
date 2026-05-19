-- ============================================================================
-- Migration: OAuth Broker resource skeleton
-- Created: 2026-05-13
-- Description: introduce first-class provider definitions, bindings,
--              binding policy snapshots, and binding runtime states
-- ============================================================================

-- ============================================================================
-- 1. oauth_provider_definitions
-- ============================================================================
CREATE TABLE IF NOT EXISTS oauth_provider_definitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    alias VARCHAR(128) NOT NULL,
    display_name VARCHAR(128) NOT NULL,
    provider_family VARCHAR(128) NOT NULL,
    binding_kind VARCHAR(64) NOT NULL,
    grant_family VARCHAR(64) NOT NULL,
    authorization_endpoint TEXT,
    token_endpoint TEXT,
    client_id TEXT,
    callback_mode VARCHAR(64),
    adapter_key VARCHAR(128),
    scope_template JSONB NOT NULL DEFAULT '[]'::jsonb,
    runtime_config JSONB NOT NULL DEFAULT '{}'::jsonb,
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_oauth_provider_definitions_tenant_alias UNIQUE (tenant_id, alias)
);

CREATE INDEX IF NOT EXISTS idx_oauth_provider_definitions_tenant_created
    ON oauth_provider_definitions(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_oauth_provider_definitions_tenant_status
    ON oauth_provider_definitions(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_oauth_provider_definitions_family
    ON oauth_provider_definitions(provider_family);

DROP TRIGGER IF EXISTS tr_oauth_provider_definitions_updated_at ON oauth_provider_definitions;
CREATE TRIGGER tr_oauth_provider_definitions_updated_at
    BEFORE UPDATE ON oauth_provider_definitions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- 2. oauth_bindings
-- ============================================================================
CREATE TABLE IF NOT EXISTS oauth_bindings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    provider_definition_id UUID NOT NULL REFERENCES oauth_provider_definitions(id) ON DELETE RESTRICT,
    binding_handle VARCHAR(128) NOT NULL UNIQUE,
    alias VARCHAR(128) NOT NULL,
    purpose TEXT NOT NULL,
    binding_kind VARCHAR(64) NOT NULL,
    subject_ref VARCHAR(255) NOT NULL,
    subject_display_name TEXT,
    status VARCHAR(32) NOT NULL DEFAULT 'draft',
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    CONSTRAINT uq_oauth_bindings_tenant_alias UNIQUE (tenant_id, alias)
);

CREATE INDEX IF NOT EXISTS idx_oauth_bindings_tenant_created
    ON oauth_bindings(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_oauth_bindings_provider
    ON oauth_bindings(provider_definition_id);
CREATE INDEX IF NOT EXISTS idx_oauth_bindings_status
    ON oauth_bindings(tenant_id, status);

DROP TRIGGER IF EXISTS tr_oauth_bindings_updated_at ON oauth_bindings;
CREATE TRIGGER tr_oauth_bindings_updated_at
    BEFORE UPDATE ON oauth_bindings
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- 3. oauth_binding_policy_snapshots
-- ============================================================================
CREATE TABLE IF NOT EXISTS oauth_binding_policy_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    binding_id UUID NOT NULL REFERENCES oauth_bindings(id) ON DELETE CASCADE,
    version INTEGER NOT NULL DEFAULT 1,
    granted_scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    effective_scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_domains JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_methods JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_path_prefixes JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_oauth_binding_policy_snapshots_binding_version UNIQUE (binding_id, version)
);

CREATE INDEX IF NOT EXISTS idx_oauth_binding_policy_snapshots_binding
    ON oauth_binding_policy_snapshots(binding_id, version DESC);
CREATE INDEX IF NOT EXISTS idx_oauth_binding_policy_snapshots_tenant
    ON oauth_binding_policy_snapshots(tenant_id, created_at DESC);

-- ============================================================================
-- 4. oauth_binding_runtime_states
-- ============================================================================
CREATE TABLE IF NOT EXISTS oauth_binding_runtime_states (
    binding_id UUID PRIMARY KEY REFERENCES oauth_bindings(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    health_status VARCHAR(32) NOT NULL DEFAULT 'draft',
    last_error_code TEXT,
    last_error_message TEXT,
    cooldown_until TIMESTAMPTZ,
    rebind_required BOOLEAN NOT NULL DEFAULT FALSE,
    last_validated_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_oauth_binding_runtime_states_tenant_status
    ON oauth_binding_runtime_states(tenant_id, health_status);
CREATE INDEX IF NOT EXISTS idx_oauth_binding_runtime_states_rebind
    ON oauth_binding_runtime_states(tenant_id, rebind_required);

DROP TRIGGER IF EXISTS tr_oauth_binding_runtime_states_updated_at ON oauth_binding_runtime_states;
CREATE TRIGGER tr_oauth_binding_runtime_states_updated_at
    BEFORE UPDATE ON oauth_binding_runtime_states
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
