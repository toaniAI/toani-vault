-- PostgreSQL 初始化脚本
-- 创建扩展、用户、权限等
-- 注意: 此脚本仅设置基础环境，具体表结构由 migrations 管理

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

-- 连接到 credbridge 数据库
\c credbridge;

-- 创建更新时间的触发器函数（如果不存在）
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- 为 credbridge 用户授予权限
GRANT CONNECT ON DATABASE credbridge TO credbridge_app;
GRANT USAGE ON SCHEMA public TO credbridge_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO credbridge_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO credbridge_app;

-- 设置默认权限（未来创建的表也会自动授权）
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO credbridge_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT USAGE, SELECT ON SEQUENCES TO credbridge_app;

-- ============================================================================
-- 默认系统数据初始化
-- ============================================================================
-- 注意: 具体表结构由 SQLx migrations 管理，此处仅插入必要的种子数据

-- 默认系统租户（用于系统级操作）
-- 仅在 tenants 表存在时插入
DO $$
BEGIN
    IF EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'tenants') THEN
        INSERT INTO tenants (id, name, description, status, config)
        VALUES (
            '00000000-0000-0000-0000-000000000000',
            'system',
            'System tenant for internal operations',
            'active',
            '{}'::jsonb
        )
        ON CONFLICT (id) DO NOTHING;
    END IF;
END $$;

-- 默认系统角色（仅在 tenant_roles 表存在时插入）
DO $$
BEGIN
    IF EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'tenant_roles') THEN
        -- Owner 角色
        INSERT INTO tenant_roles (role_name, permissions, description, is_system_role)
        VALUES (
            'owner',
            '["credentials:read", "credentials:write", "credentials:delete", "tokens:read", "tokens:write", "tokens:revoke", "audit:read", "users:manage", "roles:manage", "tenant:manage"]'::jsonb,
            '租户所有者 - 拥有全部权限包括租户管理',
            true
        ) ON CONFLICT (role_name) DO NOTHING;

        -- Admin 角色
        INSERT INTO tenant_roles (role_name, permissions, description, is_system_role)
        VALUES (
            'admin',
            '["credentials:read", "credentials:write", "credentials:delete", "tokens:read", "tokens:write", "tokens:revoke", "audit:read", "users:manage", "roles:manage"]'::jsonb,
            '租户管理员 - 拥有所有权限',
            true
        ) ON CONFLICT (role_name) DO NOTHING;

        -- User 角色
        INSERT INTO tenant_roles (role_name, permissions, description, is_system_role)
        VALUES (
            'user',
            '["credentials:read", "credentials:write", "tokens:read", "tokens:write"]'::jsonb,
            '普通用户 - 可以管理自己的凭证和令牌',
            true
        ) ON CONFLICT (role_name) DO NOTHING;

        -- ReadOnly 角色
        INSERT INTO tenant_roles (role_name, permissions, description, is_system_role)
        VALUES (
            'readonly',
            '["credentials:read", "tokens:read"]'::jsonb,
            '只读用户 - 只能查看凭证和令牌',
            true
        ) ON CONFLICT (role_name) DO NOTHING;
    END IF;
END $$;

-- 注意: 新认证模型不再预置默认用户
-- 用户通过 Privy 钱包/邮箱认证后自动创建
-- 系统租户的第一个用户将自动成为 owner

-- 验证初始化
DO $$
DECLARE
    tenant_count INT;
    role_count INT;
BEGIN
    SELECT COUNT(*) INTO tenant_count FROM tenants WHERE EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'tenants');
    SELECT COUNT(*) INTO role_count FROM tenant_roles WHERE EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'tenant_roles');

    RAISE NOTICE '========================================';
    RAISE NOTICE 'PostgreSQL 初始化完成';
    RAISE NOTICE '========================================';
    RAISE NOTICE '  - 系统租户: % (若 tenants 表存在)', tenant_count;
    RAISE NOTICE '  - 系统角色: % (若 tenant_roles 表存在)', role_count;
    RAISE NOTICE '  - 认证方式: Privy 钱包/邮箱';
    RAISE NOTICE '  - 用户创建: 认证后自动创建';
    RAISE NOTICE '========================================';
END $$;