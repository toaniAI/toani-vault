CREATE UNIQUE INDEX IF NOT EXISTS idx_approval_requests_pending_business_unique
    ON approval_requests (tenant_id, business_type, business_id)
    WHERE status = 'pending';

CREATE TABLE IF NOT EXISTS approval_audit_events (
    audit_id UUID PRIMARY KEY,
    approval_id UUID NOT NULL REFERENCES approval_requests(approval_id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    business_type VARCHAR(128) NOT NULL,
    business_id VARCHAR(255) NOT NULL,
    action VARCHAR(64) NOT NULL,
    outcome VARCHAR(32) NOT NULL,
    actor_id UUID NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT approval_audit_events_action_check
        CHECK (
            action IN (
                'initiated',
                'initiation_replayed',
                'approved',
                'approved_replayed',
                'rejected',
                'rejected_replayed',
                'cancelled',
                'cancelled_replayed'
            )
        ),
    CONSTRAINT approval_audit_events_outcome_check
        CHECK (outcome IN ('success', 'replayed', 'failure'))
);

CREATE INDEX IF NOT EXISTS idx_approval_audit_events_approval_created_at
    ON approval_audit_events (approval_id, created_at ASC, audit_id ASC);

CREATE INDEX IF NOT EXISTS idx_approval_audit_events_business_lookup
    ON approval_audit_events (tenant_id, business_type, business_id, created_at DESC);
