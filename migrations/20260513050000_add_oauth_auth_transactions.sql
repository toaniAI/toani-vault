CREATE TABLE IF NOT EXISTS oauth_auth_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    provider_definition_id UUID NOT NULL REFERENCES oauth_provider_definitions(id) ON DELETE RESTRICT,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    alias VARCHAR(128) NOT NULL,
    purpose TEXT NOT NULL,
    subject_ref_hint TEXT,
    subject_display_name TEXT,
    backing_credential_id TEXT,
    state VARCHAR(255) NOT NULL UNIQUE,
    nonce VARCHAR(255) NOT NULL,
    pkce_code_verifier TEXT NOT NULL,
    pkce_code_challenge TEXT NOT NULL,
    code_challenge_method VARCHAR(16) NOT NULL DEFAULT 'S256',
    redirect_uri TEXT NOT NULL,
    requested_scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    authorization_url TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_oauth_auth_transactions_tenant_created
    ON oauth_auth_transactions(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_oauth_auth_transactions_provider
    ON oauth_auth_transactions(provider_definition_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_oauth_auth_transactions_status
    ON oauth_auth_transactions(tenant_id, status, expires_at);

DROP TRIGGER IF EXISTS tr_oauth_auth_transactions_updated_at ON oauth_auth_transactions;
CREATE TRIGGER tr_oauth_auth_transactions_updated_at
    BEFORE UPDATE ON oauth_auth_transactions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
