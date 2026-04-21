-- ============================================================================
-- Sandbox 沙箱执行环境表迁移脚本
-- CredBridge MVP 1.0 - EP4 Story 4.x
--
-- 迁移内容:
-- 1. 创建 sandbox_sessions 沙箱会话表
-- 2. 创建 sandbox_operations 沙箱操作记录表
-- 3. 创建相关索引
-- 4. 添加 RLS 策略
--
-- 创建日期：2026-03-17
-- 迁移类型：Schema 创建
-- 回滚：可能（需要删除表）
-- ============================================================================

BEGIN;

-- ============================================================================
-- 1. 创建 sandbox_sessions 沙箱会话表
-- ============================================================================

CREATE TABLE IF NOT EXISTS sandbox_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    created_by UUID NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'ready',
    started_at TIMESTAMPTZ DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    terminated_at TIMESTAMPTZ,
    termination_reason TEXT,
    tee_context_id VARCHAR(128),
    security_policy JSONB,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    -- 约束
    CONSTRAINT sandbox_sessions_status_check CHECK (
        status IN ('active', 'ready', 'executing', 'paused', 'terminated', 'expired')
    ),
    CONSTRAINT sandbox_sessions_expires_after_start CHECK (expires_at > started_at)
);

-- 添加注释
COMMENT ON TABLE sandbox_sessions IS '沙箱会话表 - 存储 TEE 沙箱执行环境的会话信息';
COMMENT ON COLUMN sandbox_sessions.tenant_id IS '租户 ID';
COMMENT ON COLUMN sandbox_sessions.created_by IS '创建者用户 ID';
COMMENT ON COLUMN sandbox_sessions.status IS '会话状态: ready(就绪), executing(执行中), paused(暂停), terminated(终止), expired(过期); active 为旧兼容状态';
COMMENT ON COLUMN sandbox_sessions.started_at IS '会话开始时间';
COMMENT ON COLUMN sandbox_sessions.expires_at IS '会话过期时间';
COMMENT ON COLUMN sandbox_sessions.terminated_at IS '会话终止时间';
COMMENT ON COLUMN sandbox_sessions.termination_reason IS '终止原因';
COMMENT ON COLUMN sandbox_sessions.tee_context_id IS 'TEE 上下文标识符';
COMMENT ON COLUMN sandbox_sessions.security_policy IS '安全策略配置 (JSONB)';
COMMENT ON COLUMN sandbox_sessions.metadata IS '元数据 (JSONB)';

-- ============================================================================
-- 2. 创建 sandbox_operations 沙箱操作记录表
-- ============================================================================

CREATE TABLE IF NOT EXISTS sandbox_operations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    operation_type VARCHAR(64) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    input_params JSONB,
    output_result JSONB,
    error_message TEXT,
    started_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    execution_duration_ms INTEGER,
    tee_attestation_report TEXT,
    credential_id UUID,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- 约束
    CONSTRAINT sandbox_operations_status_check CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    CONSTRAINT sandbox_operations_session_fk
        FOREIGN KEY (session_id) REFERENCES sandbox_sessions(id) ON DELETE CASCADE
);

-- 添加注释
COMMENT ON TABLE sandbox_operations IS '沙箱操作记录表 - 存储在沙箱中执行的每个操作的详细记录';
COMMENT ON COLUMN sandbox_operations.session_id IS '关联的沙箱会话 ID';
COMMENT ON COLUMN sandbox_operations.tenant_id IS '租户 ID';
COMMENT ON COLUMN sandbox_operations.operation_type IS '操作类型 (如: credential_decrypt, credential_sign 等)';
COMMENT ON COLUMN sandbox_operations.status IS '操作状态: pending(待执行), running(执行中), completed(完成), failed(失败), cancelled(取消)';
COMMENT ON COLUMN sandbox_operations.input_params IS '输入参数 (JSONB)';
COMMENT ON COLUMN sandbox_operations.output_result IS '输出结果 (JSONB)';
COMMENT ON COLUMN sandbox_operations.error_message IS '错误信息';
COMMENT ON COLUMN sandbox_operations.started_at IS '操作开始时间';
COMMENT ON COLUMN sandbox_operations.completed_at IS '操作完成时间';
COMMENT ON COLUMN sandbox_operations.execution_duration_ms IS '执行时长(毫秒)';
COMMENT ON COLUMN sandbox_operations.tee_attestation_report IS 'TEE 远程证明报告';
COMMENT ON COLUMN sandbox_operations.credential_id IS '关联的凭证 ID';

-- ============================================================================
-- 3. 创建索引
-- ============================================================================

-- sandbox_sessions 索引
CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_tenant_id
    ON sandbox_sessions(tenant_id);

CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_status
    ON sandbox_sessions(status);

CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_created_by
    ON sandbox_sessions(created_by);

CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_expires_at
    ON sandbox_sessions(expires_at);

CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_tee_context
    ON sandbox_sessions(tee_context_id);

-- 复合索引（优化常用查询）
CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_tenant_status
    ON sandbox_sessions(tenant_id, status);

CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_active_lookup
    ON sandbox_sessions(tenant_id, created_by, status)
    WHERE status = 'active';

-- sandbox_operations 索引
CREATE INDEX IF NOT EXISTS idx_sandbox_operations_session_id
    ON sandbox_operations(session_id);

CREATE INDEX IF NOT EXISTS idx_sandbox_operations_tenant_id
    ON sandbox_operations(tenant_id);

CREATE INDEX IF NOT EXISTS idx_sandbox_operations_status
    ON sandbox_operations(status);

CREATE INDEX IF NOT EXISTS idx_sandbox_operations_operation_type
    ON sandbox_operations(operation_type);

CREATE INDEX IF NOT EXISTS idx_sandbox_operations_credential_id
    ON sandbox_operations(credential_id);

CREATE INDEX IF NOT EXISTS idx_sandbox_operations_started_at
    ON sandbox_operations(started_at);

-- 复合索引（优化常用查询）
CREATE INDEX IF NOT EXISTS idx_sandbox_operations_session_status
    ON sandbox_operations(session_id, status);

CREATE INDEX IF NOT EXISTS idx_sandbox_operations_tenant_time
    ON sandbox_operations(tenant_id, started_at DESC);

-- GIN 索引（用于 JSONB 查询）
CREATE INDEX IF NOT EXISTS idx_sandbox_sessions_metadata
    ON sandbox_sessions USING GIN (metadata);

CREATE INDEX IF NOT EXISTS idx_sandbox_operations_input_params
    ON sandbox_operations USING GIN (input_params);

CREATE INDEX IF NOT EXISTS idx_sandbox_operations_output_result
    ON sandbox_operations USING GIN (output_result);

-- ============================================================================
-- 4. 启用 RLS 并创建策略
-- ============================================================================

-- 启用 sandbox_sessions 的 RLS
ALTER TABLE sandbox_sessions ENABLE ROW LEVEL SECURITY;

-- 删除已存在的策略（避免重复创建错误）
DROP POLICY IF EXISTS sandbox_sessions_tenant_isolation ON sandbox_sessions;
DROP POLICY IF EXISTS sandbox_sessions_select_policy ON sandbox_sessions;
DROP POLICY IF EXISTS sandbox_sessions_insert_policy ON sandbox_sessions;
DROP POLICY IF EXISTS sandbox_sessions_update_policy ON sandbox_sessions;
DROP POLICY IF EXISTS sandbox_sessions_delete_policy ON sandbox_sessions;

-- 租户隔离策略（基础策略）
CREATE POLICY sandbox_sessions_tenant_isolation ON sandbox_sessions
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

-- 查询策略
CREATE POLICY sandbox_sessions_select_policy ON sandbox_sessions
    FOR SELECT
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

-- 插入策略
CREATE POLICY sandbox_sessions_insert_policy ON sandbox_sessions
    FOR INSERT
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::UUID);

-- 更新策略（只允许更新自己的会话）
CREATE POLICY sandbox_sessions_update_policy ON sandbox_sessions
    FOR UPDATE
    USING (tenant_id = current_setting('app.current_tenant')::UUID)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::UUID);

-- 删除策略（只允许删除自己的会话）
CREATE POLICY sandbox_sessions_delete_policy ON sandbox_sessions
    FOR DELETE
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

-- 启用 sandbox_operations 的 RLS
ALTER TABLE sandbox_operations ENABLE ROW LEVEL SECURITY;

-- 删除已存在的策略（避免重复创建错误）
DROP POLICY IF EXISTS sandbox_operations_tenant_isolation ON sandbox_operations;
DROP POLICY IF EXISTS sandbox_operations_select_policy ON sandbox_operations;
DROP POLICY IF EXISTS sandbox_operations_insert_policy ON sandbox_operations;
DROP POLICY IF EXISTS sandbox_operations_update_policy ON sandbox_operations;
DROP POLICY IF EXISTS sandbox_operations_delete_policy ON sandbox_operations;

-- 租户隔离策略（基础策略）
CREATE POLICY sandbox_operations_tenant_isolation ON sandbox_operations
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

-- 查询策略
CREATE POLICY sandbox_operations_select_policy ON sandbox_operations
    FOR SELECT
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

-- 插入策略
CREATE POLICY sandbox_operations_insert_policy ON sandbox_operations
    FOR INSERT
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::UUID);

-- 更新策略
CREATE POLICY sandbox_operations_update_policy ON sandbox_operations
    FOR UPDATE
    USING (tenant_id = current_setting('app.current_tenant')::UUID)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::UUID);

-- 删除策略
CREATE POLICY sandbox_operations_delete_policy ON sandbox_operations
    FOR DELETE
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

-- ============================================================================
-- 5. 创建触发器函数（自动更新 updated_at）
-- ============================================================================

-- 创建触发器函数
CREATE OR REPLACE FUNCTION update_sandbox_sessions_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 删除已存在的触发器（避免重复创建错误）
DROP TRIGGER IF EXISTS tr_sandbox_sessions_updated_at ON sandbox_sessions;

-- 创建触发器
CREATE TRIGGER tr_sandbox_sessions_updated_at
    BEFORE UPDATE ON sandbox_sessions
    FOR EACH ROW
    EXECUTE FUNCTION update_sandbox_sessions_updated_at();

-- ============================================================================
-- 6. 创建视图（可选 - 用于简化查询）
-- ============================================================================

-- 活跃会话视图
CREATE OR REPLACE VIEW sandbox_active_sessions AS
SELECT
    s.*,
    COUNT(o.id) AS operation_count,
    MAX(o.started_at) AS last_operation_at
FROM sandbox_sessions s
LEFT JOIN sandbox_operations o ON s.id = o.session_id
WHERE s.status IN ('active', 'ready', 'executing')
GROUP BY s.id;

COMMENT ON VIEW sandbox_active_sessions IS '活跃沙箱会话视图 - 包含操作统计信息';

-- 会话操作统计视图
CREATE OR REPLACE VIEW sandbox_session_stats AS
SELECT
    s.id AS session_id,
    s.tenant_id,
    s.status,
    COUNT(o.id) AS total_operations,
    COUNT(o.id) FILTER (WHERE o.status = 'completed') AS completed_operations,
    COUNT(o.id) FILTER (WHERE o.status = 'failed') AS failed_operations,
    AVG(o.execution_duration_ms) FILTER (WHERE o.status = 'completed') AS avg_execution_time_ms,
    MAX(o.started_at) AS last_operation_at
FROM sandbox_sessions s
LEFT JOIN sandbox_operations o ON s.id = o.session_id
GROUP BY s.id, s.tenant_id, s.status;

COMMENT ON VIEW sandbox_session_stats IS '沙箱会话统计视图 - 包含操作统计信息';

-- ============================================================================
-- 迁移完成
-- ============================================================================

COMMIT;

-- ============================================================================
-- 回滚脚本（仅用于参考，不要在生产环境执行）
-- ============================================================================

/*
-- 回滚步骤（仅在需要时执行）:

BEGIN;

-- 1. 删除视图
DROP VIEW IF EXISTS sandbox_session_stats;
DROP VIEW IF EXISTS sandbox_active_sessions;

-- 2. 删除触发器
DROP TRIGGER IF EXISTS tr_sandbox_sessions_updated_at ON sandbox_sessions;
DROP FUNCTION IF EXISTS update_sandbox_sessions_updated_at();

-- 3. 删除 sandbox_operations 表的 RLS 策略
DROP POLICY IF EXISTS sandbox_operations_delete_policy ON sandbox_operations;
DROP POLICY IF EXISTS sandbox_operations_update_policy ON sandbox_operations;
DROP POLICY IF EXISTS sandbox_operations_insert_policy ON sandbox_operations;
DROP POLICY IF EXISTS sandbox_operations_select_policy ON sandbox_operations;
DROP POLICY IF EXISTS sandbox_operations_tenant_isolation ON sandbox_operations;

-- 4. 删除 sandbox_sessions 表的 RLS 策略
DROP POLICY IF EXISTS sandbox_sessions_delete_policy ON sandbox_sessions;
DROP POLICY IF EXISTS sandbox_sessions_update_policy ON sandbox_sessions;
DROP POLICY IF EXISTS sandbox_sessions_insert_policy ON sandbox_sessions;
DROP POLICY IF EXISTS sandbox_sessions_select_policy ON sandbox_sessions;
DROP POLICY IF EXISTS sandbox_sessions_tenant_isolation ON sandbox_sessions;

-- 5. 删除表（这会级联删除所有相关索引和触发器）
DROP TABLE IF EXISTS sandbox_operations;
DROP TABLE IF EXISTS sandbox_sessions;

COMMIT;
*/
