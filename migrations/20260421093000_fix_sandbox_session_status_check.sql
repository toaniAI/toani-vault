BEGIN;

ALTER TABLE sandbox_sessions
    DROP CONSTRAINT IF EXISTS sandbox_sessions_status_check;

ALTER TABLE sandbox_sessions
    ADD CONSTRAINT sandbox_sessions_status_check CHECK (
        status IN ('active', 'ready', 'executing', 'paused', 'terminated', 'expired')
    );

COMMIT;
