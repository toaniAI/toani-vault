CREATE TABLE IF NOT EXISTS approval_requests (
    approval_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    requested_by UUID NOT NULL,
    business_type VARCHAR(128) NOT NULL,
    business_id VARCHAR(255) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT approval_requests_status_check
        CHECK (status IN ('pending', 'approved', 'rejected', 'cancelled'))
);

CREATE INDEX IF NOT EXISTS idx_approval_requests_tenant_created_at
    ON approval_requests (tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_approval_requests_business_lookup
    ON approval_requests (business_type, business_id);
