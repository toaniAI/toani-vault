ALTER TABLE oauth_bindings
    ADD COLUMN IF NOT EXISTS backing_credential_id TEXT;
