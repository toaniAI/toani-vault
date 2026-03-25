-- ============================================================================
-- CredBridge 数据库初始化脚本
-- 用于本地开发环境初始化
-- 数据库: credbridge
-- 连接: 10.11.25.9:15432
-- ============================================================================

-- 创建扩展
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS citext;

-- 创建更新时间触发器函数
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- ============================================================================
-- 1. 创建基础表
-- ============================================================================

-- 租户表 (public schema)
CREATE TABLE IF NOT EXISTS tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(64) NOT NULL UNIQUE,
    description TEXT,
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    config JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 用户表 (public schema)
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    username VARCHAR(64) NOT NULL UNIQUE,
    email VARCHAR(256) NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role VARCHAR(32) NOT NULL DEFAULT 'user',
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 凭证表 (public schema)
CREATE TABLE IF NOT EXISTS credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    credential_id VARCHAR(64) NOT NULL UNIQUE,
    user_id_hash VARCHAR(128) NOT NULL,
    service_id VARCHAR(64) NOT NULL,
    credential_type VARCHAR(32) NOT NULL,
    encrypted_payload TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    algorithm VARCHAR(32) NOT NULL DEFAULT 'AES-256-GCM',
    kdf VARCHAR(32) NOT NULL DEFAULT 'HKDF-SHA-256',
    nonce VARCHAR(64) NOT NULL,
    auth_tag VARCHAR(64) NOT NULL,
    ciphertext TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
    metadata JSONB,
    change_reason TEXT,
    changed_by UUID
);

-- Scope Token 表
CREATE TABLE IF NOT EXISTS scope_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_id VARCHAR(64) NOT NULL UNIQUE,
    credential_id VARCHAR(64) NOT NULL REFERENCES credentials(credential_id),
    scopes JSONB NOT NULL,
    constraints JSONB NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    usage_count INTEGER NOT NULL DEFAULT 0
);

-- 审计日志表
CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    event_type VARCHAR(64) NOT NULL,
    event_data JSONB NOT NULL,
    action VARCHAR(64),
    user_id VARCHAR(128),
    user_id_hash VARCHAR(128),
    ip_address VARCHAR(45),
    user_agent TEXT,
    event_category VARCHAR(32),
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 租户角色表
CREATE TABLE IF NOT EXISTS tenant_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    role_name VARCHAR(64) NOT NULL UNIQUE,
    permissions JSONB NOT NULL,
    description TEXT,
    is_system_role BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 用户角色关联表
CREATE TABLE IF NOT EXISTS user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id_hash VARCHAR(128) NOT NULL,
    role_id UUID NOT NULL REFERENCES tenant_roles(id),
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    assigned_by VARCHAR(128),
    expires_at TIMESTAMPTZ
);

-- ============================================================================
-- 2. 创建索引
-- ============================================================================

-- 凭证表索引
CREATE INDEX IF NOT EXISTS idx_credentials_user_id ON credentials(user_id_hash);
CREATE INDEX IF NOT EXISTS idx_credentials_service ON credentials(service_id);
CREATE INDEX IF NOT EXISTS idx_credentials_type ON credentials(credential_type);
CREATE INDEX IF NOT EXISTS idx_credentials_expires ON credentials(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_credentials_tenant_lookup ON credentials(user_id_hash, credential_type) WHERE is_deleted = false;
CREATE INDEX IF NOT EXISTS idx_credentials_service_lookup ON credentials(service_id, credential_type);

-- Token 表索引
CREATE INDEX IF NOT EXISTS idx_scope_tokens_credential ON scope_tokens(credential_id);
CREATE INDEX IF NOT EXISTS idx_scope_tokens_expires ON scope_tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_scope_tokens_revoked ON scope_tokens(revoked_at) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_scope_tokens_active ON scope_tokens(credential_id, expires_at) WHERE revoked_at IS NULL;

-- 审计日志表索引
CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type ON audit_logs(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_logs_user ON audit_logs(user_id_hash);
CREATE INDEX IF NOT EXISTS idx_audit_logs_tenant_id ON audit_logs(tenant_id) WHERE tenant_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_logs_recent ON audit_logs(created_at DESC, event_type);
CREATE INDEX IF NOT EXISTS idx_audit_logs_category ON audit_logs(event_category);
CREATE INDEX IF NOT EXISTS idx_audit_logs_metadata ON audit_logs USING GIN (metadata);

-- 租户表索引
CREATE INDEX IF NOT EXISTS idx_tenants_status ON tenants(status);

-- 用户表索引
CREATE INDEX IF NOT EXISTS idx_users_tenant ON users(tenant_id);
CREATE INDEX IF NOT EXISTS idx_users_status ON users(status);

-- 用户角色索引
CREATE INDEX IF NOT EXISTS idx_user_roles_user ON user_roles(user_id_hash);
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_roles_unique ON user_roles(user_id_hash, role_id);
CREATE INDEX IF NOT EXISTS idx_user_roles_lookup ON user_roles(user_id_hash, role_id);

-- ============================================================================
-- 3. 创建触发器
-- ============================================================================

-- tenants 表更新触发器
DROP TRIGGER IF EXISTS tr_tenants_updated_at ON tenants;
CREATE TRIGGER tr_tenants_updated_at
    BEFORE UPDATE ON tenants
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- users 表更新触发器
DROP TRIGGER IF EXISTS tr_users_updated_at ON users;
CREATE TRIGGER tr_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- credentials 表更新触发器
DROP TRIGGER IF EXISTS tr_credentials_updated_at ON credentials;
CREATE TRIGGER tr_credentials_updated_at
    BEFORE UPDATE ON credentials
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- tenant_roles 表更新触发器
DROP TRIGGER IF EXISTS tr_tenant_roles_updated_at ON tenant_roles;
CREATE TRIGGER tr_tenant_roles_updated_at
    BEFORE UPDATE ON tenant_roles
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- 4. 插入默认数据
-- ============================================================================

-- 默认系统租户
INSERT INTO tenants (id, name, description, status, config)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    'system',
    'System tenant for internal operations',
    'active',
    '{}'::jsonb
)
ON CONFLICT (id) DO NOTHING;

-- 默认管理员用户 (密码: admin123)
INSERT INTO users (id, tenant_id, username, email, password_hash, role, status)
VALUES (
    uuid_generate_v4(),
    '00000000-0000-0000-0000-000000000000',
    'admin',
    'admin@credbridge.local',
    '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewKyNiAYMyzJ/Igu',
    'admin',
    'active'
)
ON CONFLICT (username) DO NOTHING;

-- 默认系统角色
INSERT INTO tenant_roles (role_name, permissions, description, is_system_role)
VALUES (
    'admin',
    '["credentials:read", "credentials:write", "credentials:delete", "tokens:read", "tokens:write", "tokens:revoke", "audit:read", "users:manage", "roles:manage"]'::jsonb,
    '租户管理员 - 拥有所有权限',
    true
) ON CONFLICT (role_name) DO NOTHING;

INSERT INTO tenant_roles (role_name, permissions, description, is_system_role)
VALUES (
    'user',
    '["credentials:read", "credentials:write", "tokens:read", "tokens:write"]'::jsonb,
    '普通用户 - 可以管理自己的凭证和令牌',
    true
) ON CONFLICT (role_name) DO NOTHING;

INSERT INTO tenant_roles (role_name, permissions, description, is_system_role)
VALUES (
    'readonly',
    '["credentials:read", "tokens:read"]'::jsonb,
    '只读用户 - 只能查看凭证和令牌',
    true
) ON CONFLICT (role_name) DO NOTHING;

-- ============================================================================
-- 5. 验证初始化
-- ============================================================================

DO $$
DECLARE
    tenant_count INT;
    user_count INT;
    role_count INT;
    table_count INT;
BEGIN
    SELECT COUNT(*) INTO tenant_count FROM tenants;
    SELECT COUNT(*) INTO user_count FROM users;
    SELECT COUNT(*) INTO role_count FROM tenant_roles;
    SELECT COUNT(*) INTO table_count FROM information_schema.tables WHERE table_schema = 'public';

    RAISE NOTICE '========================================';
    RAISE NOTICE 'CredBridge 数据库初始化完成!';
    RAISE NOTICE '========================================';
    RAISE NOTICE '  - 租户数量: %', tenant_count;
    RAISE NOTICE '  - 用户数量: %', user_count;
    RAISE NOTICE '  - 角色数量: %', role_count;
    RAISE NOTICE '  - 表数量: %', table_count;
    RAISE NOTICE '========================================';
END
$$;
