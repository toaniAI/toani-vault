ALTER TABLE approval_business_results
    ADD COLUMN IF NOT EXISTS tenant_id UUID,
    ADD COLUMN IF NOT EXISTS consumed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS reservation_id UUID,
    ADD COLUMN IF NOT EXISTS reserved_at TIMESTAMPTZ;

UPDATE approval_business_results AS results
SET tenant_id = requests.tenant_id
FROM approval_requests AS requests
WHERE results.approval_id = requests.approval_id
  AND results.tenant_id IS NULL;

WITH ranked AS (
    SELECT
        ctid,
        ROW_NUMBER() OVER (
            PARTITION BY tenant_id, business_type, business_id
            ORDER BY updated_at DESC, approval_id DESC
        ) AS row_rank
    FROM approval_business_results
    WHERE tenant_id IS NOT NULL
)
DELETE FROM approval_business_results AS results
USING ranked
WHERE results.ctid = ranked.ctid
  AND ranked.row_rank > 1;

DELETE FROM approval_business_results
WHERE tenant_id IS NULL;

ALTER TABLE approval_business_results
    ALTER COLUMN tenant_id SET NOT NULL;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'approval_business_results_pkey'
    ) THEN
        ALTER TABLE approval_business_results
            DROP CONSTRAINT approval_business_results_pkey;
    END IF;

    ALTER TABLE approval_business_results
        ADD CONSTRAINT approval_business_results_pkey
            PRIMARY KEY (tenant_id, business_type, business_id);
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS idx_approval_business_results_approval
    ON approval_business_results (approval_id);
