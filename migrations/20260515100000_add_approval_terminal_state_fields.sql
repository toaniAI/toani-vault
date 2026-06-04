ALTER TABLE approval_requests
    ADD COLUMN IF NOT EXISTS processed_by UUID,
    ADD COLUMN IF NOT EXISTS processed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS remark TEXT,
    ADD COLUMN IF NOT EXISTS result_code VARCHAR(64),
    ADD COLUMN IF NOT EXISTS result_payload JSONB,
    ADD COLUMN IF NOT EXISTS business_result_written_at TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS approval_business_results (
    business_type VARCHAR(128) NOT NULL,
    business_id VARCHAR(255) NOT NULL,
    approval_id UUID NOT NULL,
    status VARCHAR(32) NOT NULL,
    result_code VARCHAR(64),
    result_payload JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (business_type, business_id),
    CONSTRAINT approval_business_results_status_check
        CHECK (status IN ('approved', 'rejected', 'cancelled'))
);

CREATE INDEX IF NOT EXISTS idx_approval_business_results_approval
    ON approval_business_results (approval_id);
