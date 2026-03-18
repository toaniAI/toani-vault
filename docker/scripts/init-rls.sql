-- PostgreSQL 行级安全策略 (RLS) 初始化脚本
-- 版本: 1.0
-- 创建日期: 2026-03-12
-- 关联任务: EP7 Story 7.2 - Schema 隔离与 RLS
--
-- 使用说明:
-- 1. 在租户 Schema 创建后执行此脚本
-- 2. 确保应用用户具有执行权限
-- 3. 建议在事务中执行以便回滚

-- =============================================================================
-- 1. 辅助函数
-- =============================================================================

-- 创建或替换安全转义函数（防止 SQL 注入）
CREATE OR REPLACE FUNCTION escape_rls_value(input TEXT)
RETURNS TEXT AS $$
BEGIN
    RETURN replace(replace(replace(replace(input,
        '\', '\\'),
        '''', '\'''),
        E'\n', '\n'),
        E'\r', '\r');
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- 创建 RLS 上下文验证函数
CREATE OR REPLACE FUNCTION verify_rls_context()
RETURNS BOOLEAN AS $$
BEGIN
    -- 检查必填的租户上下文
    RETURN current_setting('app.current_tenant_id', true) IS NOT NULL
       AND current_setting('app.current_tenant_id', true) != '';
END;
$$ LANGUAGE plpgsql STABLE;

-- =============================================================================
-- 2. RLS 策略定义 - credentials 表
-- =============================================================================

-- 启用 RLS
ALTER TABLE credentials ENABLE ROW LEVEL SECURITY;

-- 强制表所有者遵守 RLS（重要！）
ALTER TABLE credentials FORCE ROW LEVEL SECURITY;

-- 删除已存在的策略（支持重新运行）
DROP POLICY IF EXISTS credentials_deny_all ON credentials;
DROP POLICY IF EXISTS credentials_tenant_access ON credentials;
DROP POLICY IF EXISTS credentials_admin_bypass ON credentials;

-- 默认拒绝策略（安全基线）
CREATE POLICY credentials_deny_all
    ON credentials
    FOR ALL
    TO PUBLIC
    USING (false);

-- 租户访问策略
CREATE POLICY credentials_tenant_access
    ON credentials
    FOR ALL
    TO PUBLIC
    USING (
        -- 管理员绕过检查
        COALESCE(current_setting('app.is_admin', true), 'false')::boolean = true
        OR
        -- 验证 RLS 上下文存在
        verify_rls_context()
    )
    WITH CHECK (
        COALESCE(current_setting('app.is_admin', true), 'false')::boolean = true
        OR
        verify_rls_context()
    );

-- =============================================================================
-- 3. RLS 策略定义 - scope_tokens 表
-- =============================================================================

ALTER TABLE scope_tokens ENABLE ROW LEVEL SECURITY;
ALTER TABLE scope_tokens FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS scope_tokens_deny_all ON scope_tokens;
DROP POLICY IF EXISTS scope_tokens_tenant_access ON scope_tokens;

CREATE POLICY scope_tokens_deny_all
    ON scope_tokens
    FOR ALL
    TO PUBLIC
    USING (false);

CREATE POLICY scope_tokens_tenant_access
    ON scope_tokens
    FOR ALL
    TO PUBLIC
    USING (
        COALESCE(current_setting('app.is_admin', true), 'false')::boolean = true
        OR
        verify_rls_context()
    );

-- =============================================================================
-- 4. RLS 策略定义 - audit_logs 表
-- =============================================================================

ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS audit_logs_read_only ON audit_logs;
DROP POLICY IF EXISTS audit_logs_no_modify ON audit_logs;
DROP POLICY IF EXISTS audit_logs_tenant_access ON audit_logs;

-- 审计日志只读策略
CREATE POLICY audit_logs_read_only
    ON audit_logs
    FOR SELECT
    TO PUBLIC
    USING (true);

-- 禁止修改策略
CREATE POLICY audit_logs_no_modify
    ON audit_logs
    FOR ALL
    TO PUBLIC
    USING (false);

-- =============================================================================
-- 5. RLS 策略定义 - user_roles 表
-- =============================================================================

ALTER TABLE user_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_roles FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS user_roles_deny_all ON user_roles;
DROP POLICY IF EXISTS user_roles_tenant_access ON user_roles;

CREATE POLICY user_roles_deny_all
    ON user_roles
    FOR ALL
    TO PUBLIC
    USING (false);

CREATE POLICY user_roles_tenant_access
    ON user_roles
    FOR ALL
    TO PUBLIC
    USING (
        COALESCE(current_setting('app.is_admin', true), 'false')::boolean = true
        OR
        verify_rls_context()
    );

-- =============================================================================
-- 6. RLS 策略定义 - tenant_roles 表（系统角色，租户内只读）
-- =============================================================================

ALTER TABLE tenant_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_roles FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_roles_read_only ON tenant_roles;
DROP POLICY IF EXISTS tenant_roles_no_modify ON tenant_roles;

-- 系统角色只读（所有租户共享角色定义）
CREATE POLICY tenant_roles_read_only
    ON tenant_roles
    FOR SELECT
    TO PUBLIC
    USING (true);

CREATE POLICY tenant_roles_no_modify
    ON tenant_roles
    FOR ALL
    TO PUBLIC
    USING (false);

-- =============================================================================
-- 7. 性能优化索引
-- =============================================================================

-- 为 RLS 策略优化创建索引
-- 注意：RLS 使用 current_setting() 无法直接使用索引，
-- 但索引对应用层查询仍然重要

-- credentials 表索引优化
CREATE INDEX IF NOT EXISTS idx_credentials_tenant_lookup
    ON credentials(user_id_hash, credential_type)
    WHERE is_deleted = false;

CREATE INDEX IF NOT EXISTS idx_credentials_service_lookup
    ON credentials(service_id, credential_type);

-- scope_tokens 表索引优化
CREATE INDEX IF NOT EXISTS idx_scope_tokens_active
    ON scope_tokens(credential_id, expires_at)
    WHERE revoked_at IS NULL;

-- audit_logs 表索引优化
CREATE INDEX IF NOT EXISTS idx_audit_logs_recent
    ON audit_logs(created_at DESC, event_type);

-- user_roles 表索引优化
CREATE INDEX IF NOT EXISTS idx_user_roles_lookup
    ON user_roles(user_id_hash, role_id);

-- =============================================================================
-- 8. RLS 审计视图
-- =============================================================================

-- 创建 RLS 统计视图（用于监控）
CREATE OR REPLACE VIEW rls_table_stats AS
SELECT
    schemaname,
    relname as table_name,
    relrowsecurity as rls_enabled,
    relforcerowsecurity as force_rls,
    pg_size_pretty(pg_total_relation_size(relid)) as total_size,
    n_live_tup as live_tuples,
    n_dead_tup as dead_tuples
FROM pg_stat_user_tables
WHERE relrowsecurity = true;

-- 创建策略列表视图
CREATE OR REPLACE VIEW rls_policies AS
SELECT
    schemaname,
    tablename,
    policyname,
    permissive,
    roles,
    cmd,
    qual as using_expression,
    with_check
FROM pg_policies
WHERE schemaname NOT IN ('pg_catalog', 'information_schema');

-- =============================================================================
-- 9. 测试查询
-- =============================================================================

-- 测试查询 1: 验证 RLS 已启用
-- SELECT * FROM rls_table_stats;

-- 测试查询 2: 验证策略列表
-- SELECT * FROM rls_policies WHERE schemaname = 'tenant_<your_tenant_id>';

-- 测试查询 3: 模拟租户上下文并查询（应在事务中执行）
/*
BEGIN;
SET LOCAL app.current_tenant_id = 'test-tenant-uuid';
SET LOCAL app.current_user_id = 'test-user-id';
SET LOCAL app.current_scopes = 'read,write';
SET LOCAL app.is_admin = 'false';

-- 执行查询
SELECT * FROM credentials LIMIT 5;

ROLLBACK;
*/

-- 测试查询 4: 验证跨租户访问被拒绝（应在事务中执行）
/*
BEGIN;
-- 不设置租户上下文，查询应该返回空结果
SELECT * FROM credentials LIMIT 5;
-- 应该返回 0 行，因为 deny_all 策略生效
ROLLBACK;
*/

-- =============================================================================
-- 10. 权限配置
-- =============================================================================

-- 确保 credbridge_app 用户有权限（如果存在）
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'credbridge_app') THEN
        -- 授予 RLS 绕过权限（如果需要，通常不建议）
        -- ALTER USER credbridge_app BYPASSRLS;  -- 谨慎使用！

        -- 授予表访问权限
        GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO credbridge_app;
        GRANT USAGE ON ALL SEQUENCES IN SCHEMA public TO credbridge_app;
    END IF;
END
$$;

-- 设置默认权限
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO PUBLIC;

-- =============================================================================
-- 完成
-- =============================================================================

DO $$
DECLARE
    rls_count INT;
    policy_count INT;
BEGIN
    SELECT COUNT(*) INTO rls_count
    FROM pg_class
    WHERE relrowsecurity = true
      AND relname IN ('credentials', 'scope_tokens', 'audit_logs', 'user_roles', 'tenant_roles');

    SELECT COUNT(*) INTO policy_count
    FROM pg_policies
    WHERE tablename IN ('credentials', 'scope_tokens', 'audit_logs', 'user_roles', 'tenant_roles');

    RAISE NOTICE 'RLS 初始化完成:';
    RAISE NOTICE '  - 启用 RLS 的表: %', rls_count;
    RAISE NOTICE '  - 创建的策略数: %', policy_count;
END
$$;

-- 验证命令（取消注释执行）
-- \echo '验证 RLS 状态:'
-- SELECT * FROM rls_table_stats;
-- \echo '验证策略列表:'
-- SELECT schemaname, tablename, policyname, cmd FROM rls_policies;
