BEGIN;

ALTER TABLE sandbox_sessions
    ADD COLUMN IF NOT EXISTS active_operation_id UUID,
    ADD COLUMN IF NOT EXISTS active_operation_started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_error_summary TEXT;

UPDATE sandbox_sessions
SET status = 'ready',
    updated_at = NOW()
WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_active_operation_id
    ON sandbox_sessions (active_operation_id)
    WHERE active_operation_id IS NOT NULL;

COMMIT;
