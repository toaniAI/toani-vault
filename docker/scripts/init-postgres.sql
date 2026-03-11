-- PostgreSQL 初始化脚本
-- 创建扩展、用户、权限等

-- 创建 pgcrypto 扩展（用于加密功能）
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- 创建 uuid-ossp 扩展（用于 UUID 生成）
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- 创建 citext 扩展（用于大小写不敏感文本）
CREATE EXTENSION IF NOT EXISTS citext;

-- 创建 credbridge 应用用户（可选，用于只读访问）
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = 'credbridge_app') THEN
        CREATE USER credbridge_app WITH PASSWORD 'credbridge_app_pass';
    END IF;
END
$$;

-- 授权
c \c credbridge;

-- 为 credbridge 用户授予权限
GRANT CONNECT ON DATABASE credbridge TO credbridge_app;
GRANT USAGE ON SCHEMA public TO credbridge_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO credbridge_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO credbridge_app;

-- 设置默认权限
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO credbridge_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT USAGE, SELECT ON SEQUENCES TO credbridge_app;

-- 创建更新时间的触发器函数
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- 创建索引（优化查询性能）
-- 凭证表索引
CREATE INDEX IF NOT EXISTS idx_credentials_tenant_id ON credentials(tenant_id) WHERE tenant_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_credentials_created_at ON credentials(created_at);
CREATE INDEX IF NOT EXISTS idx_credentials_status ON credentials(status);

-- Token 表索引
CREATE INDEX IF NOT EXISTS idx_tokens_tenant_id ON tokens(tenant_id) WHERE tenant_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tokens_user_id ON tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_tokens_expires_at ON tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_tokens_status ON tokens(status);

-- 审计日志表索引
CREATE INDEX IF NOT EXISTS idx_audit_logs_tenant_id ON audit_logs(tenant_id) WHERE tenant_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action);
CREATE INDEX IF NOT EXISTS idx_audit_logs_user_id ON audit_logs(user_id);

-- 租户表索引
CREATE INDEX IF NOT EXISTS idx_tenants_status ON tenants(status);

-- 插入默认系统数据
-- 默认系统租户（用于系统级操作）
INSERT INTO tenants (id, name, description, status, config)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    'system',
    'System tenant for internal operations',
    'active',
    '{}'::jsonb
)
ON CONFLICT (id) DO NOTHING;

-- 插入默认管理员用户（仅在 users 表存在时）
DO $$
BEGIN
    IF EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'users') THEN
        EXECUTE 'INSERT INTO users (id, tenant_id, username, email, password_hash, role, status)
        VALUES (
            uuid_generate_v4(),
            ''00000000-0000-0000-0000-000000000000'',
            ''admin'',
            ''admin@credbridge.local'',
            ''$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewKyNiAYMyzJ/Igu'', -- bcrypt hash of "admin123"
            ''admin'',
            ''active''
        )
        ON CONFLICT (username) DO NOTHING';
    END IF;
END $$;

echo 'PostgreSQL 初始化完成';
