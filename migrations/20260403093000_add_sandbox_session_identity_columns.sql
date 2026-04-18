BEGIN;

ALTER TABLE sandbox_sessions
    ADD COLUMN IF NOT EXISTS credential_id UUID,
    ADD COLUMN IF NOT EXISTS original_intent TEXT;

UPDATE sandbox_sessions
SET credential_id = COALESCE(
        credential_id,
        NULLIF(metadata ->> 'credential_id', '')::UUID
    ),
    original_intent = COALESCE(
        original_intent,
        NULLIF(metadata ->> 'original_intent', '')
    )
WHERE credential_id IS NULL
   OR original_intent IS NULL;

CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_credential_id
    ON sandbox_sessions (credential_id);

COMMIT;
