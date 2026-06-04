CREATE TABLE IF NOT EXISTS http_request_operations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    created_by UUID NOT NULL,
    credential_id UUID NOT NULL,
    description TEXT NOT NULL,
    operation_type VARCHAR(64) NOT NULL DEFAULT 'http_request',
    status VARCHAR(32) NOT NULL DEFAULT 'running',
    request_parameters JSONB NOT NULL DEFAULT '{}'::JSONB,
    response_data JSONB,
    error_message TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    execution_duration_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT http_request_operations_status_check CHECK (status IN ('running', 'completed', 'failed'))
);

CREATE INDEX IF NOT EXISTS idx_http_request_operations_tenant_id
    ON http_request_operations(tenant_id);

CREATE INDEX IF NOT EXISTS idx_http_request_operations_credential_id
    ON http_request_operations(credential_id);

CREATE INDEX IF NOT EXISTS idx_http_request_operations_status
    ON http_request_operations(status);

CREATE INDEX IF NOT EXISTS idx_http_request_operations_tenant_time
    ON http_request_operations(tenant_id, started_at DESC);

ALTER TABLE http_request_operations ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS http_request_operations_tenant_isolation ON http_request_operations;
DROP POLICY IF EXISTS http_request_operations_select_policy ON http_request_operations;
DROP POLICY IF EXISTS http_request_operations_insert_policy ON http_request_operations;
DROP POLICY IF EXISTS http_request_operations_update_policy ON http_request_operations;
DROP POLICY IF EXISTS http_request_operations_delete_policy ON http_request_operations;

CREATE POLICY http_request_operations_tenant_isolation ON http_request_operations
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

CREATE POLICY http_request_operations_select_policy ON http_request_operations
    FOR SELECT
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

CREATE POLICY http_request_operations_insert_policy ON http_request_operations
    FOR INSERT
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::UUID);

CREATE POLICY http_request_operations_update_policy ON http_request_operations
    FOR UPDATE
    USING (tenant_id = current_setting('app.current_tenant')::UUID)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::UUID);

CREATE POLICY http_request_operations_delete_policy ON http_request_operations
    FOR DELETE
    USING (tenant_id = current_setting('app.current_tenant')::UUID);
