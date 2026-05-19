ALTER TABLE oauth_binding_policy_snapshots
    ADD COLUMN IF NOT EXISTS validation_status VARCHAR(32) NOT NULL DEFAULT 'validated';

ALTER TABLE oauth_binding_policy_snapshots
    ADD COLUMN IF NOT EXISTS last_validation_error_code TEXT;

ALTER TABLE oauth_binding_policy_snapshots
    ADD COLUMN IF NOT EXISTS last_validation_error_message TEXT;

ALTER TABLE oauth_binding_policy_snapshots
    ADD COLUMN IF NOT EXISTS validated_at TIMESTAMPTZ;

ALTER TABLE oauth_binding_policy_snapshots
    ADD COLUMN IF NOT EXISTS applied_at TIMESTAMPTZ;

UPDATE oauth_binding_policy_snapshots
SET validation_status = 'validated',
    validated_at = COALESCE(validated_at, created_at),
    applied_at = COALESCE(applied_at, created_at)
WHERE applied_at IS NULL
   OR validated_at IS NULL
   OR validation_status IS DISTINCT FROM 'validated';

CREATE INDEX IF NOT EXISTS idx_oauth_binding_policy_snapshots_binding_applied
    ON oauth_binding_policy_snapshots(binding_id, applied_at DESC, version DESC);
