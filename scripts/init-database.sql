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

-- 用户表 (public schema) - Privy 钱包优先认证
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    display_name VARCHAR(128),
    default_tenant_id UUID REFERENCES tenants(id),
    onboarding_completed BOOLEAN NOT NULL DEFAULT FALSE,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 外部身份映射表 - 支持 Privy 等多身份提供商
CREATE TABLE IF NOT EXISTS external_identities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(64) NOT NULL,
    provider_subject VARCHAR(256) NOT NULL,
    wallet_address VARCHAR(256),
    email VARCHAR(256),
    provider_profile JSONB,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(provider, provider_subject)
);

-- 租户成员关系表
CREATE TABLE IF NOT EXISTS tenant_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(32) NOT NULL DEFAULT 'member',
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    invited_by UUID REFERENCES users(id),
    joined_at TIMESTAMPTZ,
    source VARCHAR(32) NOT NULL DEFAULT 'invitation',
    scopes JSONB DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, user_id)
);

-- 租户邀请表
CREATE TABLE IF NOT EXISTS tenant_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    role VARCHAR(32) NOT NULL DEFAULT 'member',
    invitee_type VARCHAR(32) NOT NULL,
    invitee_email VARCHAR(256),
    invitee_wallet VARCHAR(256),
    token_hash VARCHAR(256) NOT NULL UNIQUE,
    created_by UUID NOT NULL REFERENCES users(id),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    consumed_by UUID REFERENCES users(id),
    consumed_identity_id UUID REFERENCES external_identities(id),
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    max_uses INTEGER NOT NULL DEFAULT 1,
    use_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_invitation_target CHECK (
        (invitee_type = 'any') OR
        (invitee_type = 'email' AND invitee_email IS NOT NULL) OR
        (invitee_type = 'wallet' AND invitee_wallet IS NOT NULL)
    )
);

-- 认证会话表
CREATE TABLE IF NOT EXISTS auth_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_token_hash VARCHAR(256) NOT NULL UNIQUE,
    identity_id UUID REFERENCES external_identities(id),
    active_membership_id UUID REFERENCES tenant_memberships(id),
    mfa_status VARCHAR(32) NOT NULL DEFAULT 'not_required',
    mfa_verified_at TIMESTAMPTZ,
    user_agent TEXT,
    ip_address VARCHAR(45),
    expires_at TIMESTAMPTZ NOT NULL,
    last_active_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    revoked_reason VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 认证审计日志表
CREATE TABLE IF NOT EXISTS auth_audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type VARCHAR(64) NOT NULL,
    severity VARCHAR(32) NOT NULL DEFAULT 'info',
    user_id UUID REFERENCES users(id),
    identity_id UUID REFERENCES external_identities(id),
    tenant_id UUID REFERENCES tenants(id),
    membership_id UUID REFERENCES tenant_memberships(id),
    invitation_id UUID REFERENCES tenant_invitations(id),
    session_id UUID REFERENCES auth_sessions(id),
    event_data JSONB NOT NULL DEFAULT '{}'::jsonb,
    ip_address VARCHAR(45),
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
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

-- 用户表索引 (新模型)
CREATE INDEX IF NOT EXISTS idx_users_status ON users(status) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_users_default_tenant ON users(default_tenant_id);
CREATE INDEX IF NOT EXISTS idx_users_deleted_at ON users(deleted_at) WHERE deleted_at IS NOT NULL;

-- 外部身份索引
CREATE INDEX IF NOT EXISTS idx_external_identities_user ON external_identities(user_id);
CREATE INDEX IF NOT EXISTS idx_external_identities_provider_subject ON external_identities(provider, provider_subject);
CREATE INDEX IF NOT EXISTS idx_external_identities_wallet ON external_identities(wallet_address) WHERE wallet_address IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_external_identities_email ON external_identities(email) WHERE email IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_external_identities_primary ON external_identities(user_id, is_primary) WHERE is_primary = TRUE;

-- 租户成员索引
CREATE INDEX IF NOT EXISTS idx_tenant_memberships_tenant ON tenant_memberships(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tenant_memberships_user ON tenant_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_tenant_memberships_tenant_status ON tenant_memberships(tenant_id, status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_tenant_memberships_role ON tenant_memberships(tenant_id, role);

-- 租户邀请索引
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_tenant ON tenant_invitations(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_token ON tenant_invitations(token_hash);
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_status ON tenant_invitations(tenant_id, status) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_expires ON tenant_invitations(expires_at) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_email ON tenant_invitations(invitee_email) WHERE invitee_email IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_wallet ON tenant_invitations(invitee_wallet) WHERE invitee_wallet IS NOT NULL;

-- 认证会话索引
CREATE INDEX IF NOT EXISTS idx_auth_sessions_user ON auth_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_token ON auth_sessions(session_token_hash);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires ON auth_sessions(expires_at) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_auth_sessions_active ON auth_sessions(user_id, revoked_at) WHERE revoked_at IS NULL;

-- 认证审计日志索引
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_user ON auth_audit_logs(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_tenant ON auth_audit_logs(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_event ON auth_audit_logs(event_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_created ON auth_audit_logs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_severity ON auth_audit_logs(severity, created_at DESC) WHERE severity IN ('error', 'critical');

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

-- users 表更新触发器 (新模型)
DROP TRIGGER IF EXISTS tr_users_updated_at ON users;
CREATE TRIGGER tr_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- external_identities 表更新触发器
DROP TRIGGER IF EXISTS tr_external_identities_updated_at ON external_identities;
CREATE TRIGGER tr_external_identities_updated_at
    BEFORE UPDATE ON external_identities
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- tenant_memberships 表更新触发器
DROP TRIGGER IF EXISTS tr_tenant_memberships_updated_at ON tenant_memberships;
CREATE TRIGGER tr_tenant_memberships_updated_at
    BEFORE UPDATE ON tenant_memberships
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- tenant_invitations 表更新触发器
DROP TRIGGER IF EXISTS tr_tenant_invitations_updated_at ON tenant_invitations;
CREATE TRIGGER tr_tenant_invitations_updated_at
    BEFORE UPDATE ON tenant_invitations
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- auth_sessions 表更新触发器
DROP TRIGGER IF EXISTS tr_auth_sessions_updated_at ON auth_sessions;
CREATE TRIGGER tr_auth_sessions_updated_at
    BEFORE UPDATE ON auth_sessions
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
    jsonb_build_object(
        'feature_flags', jsonb_build_object(
            'enable_credential_encryption', true,
            'enable_audit_logging', true,
            'enable_token_revocation', true,
            'enable_mfa', false,
            'enable_remote_attestation', false,
            'enable_auto_rotation', false,
            'allow_cors', false,
            'enable_ip_whitelist', false,
            'enable_webhooks', false,
            'enable_sso', false,
            'enable_custom_crypto', false,
            'enable_advanced_audit', false
        ),
        'quota_limits', jsonb_build_object(
            'max_credentials', 1000,
            'max_tokens_per_user', 10,
            'max_requests_per_minute', 1000,
            'max_users', 100,
            'max_connectors', 20,
            'max_webhooks', 10,
            'storage_quota_mb', 1024,
            'audit_retention_days', 30,
            'max_token_ttl_seconds', 86400,
            'max_batch_size', 100
        ),
        'settings', jsonb_build_object(
            'token_ttl_seconds', 900,
            'session_timeout_seconds', 3600,
            'max_login_attempts', 5,
            'lockout_duration_seconds', 900,
            'password_min_length', 8,
            'require_password_complexity', true,
            'require_mfa', false,
            'allowed_callback_urls', '[]'::jsonb,
            'timezone', 'UTC',
            'language', 'zh-CN',
            'metadata', '{}'::jsonb
        ),
        'version', 1,
        'updated_at', NULL,
        'updated_by', NULL
    )
)
ON CONFLICT (id) DO NOTHING;

-- 注意: 新模型不再预置默认用户，用户通过 Privy 认证后自动创建
-- 默认管理员用户已移除，用户需通过 Privy 钱包/邮箱登录
-- 系统租户的第一个用户将自动成为 owner

-- 默认系统角色 (在 public schema 的 tenant_roles 表中定义角色模板)
INSERT INTO tenant_roles (role_name, permissions, description, is_system_role)
VALUES (
    'admin',
    '["credentials:read", "credentials:write", "credentials:delete", "tokens:read", "tokens:write", "tokens:revoke", "audit:read", "users:manage", "roles:manage"]'::jsonb,
    '租户管理员 - 拥有所有权限',
    true
) ON CONFLICT (role_name) DO NOTHING;

INSERT INTO tenant_roles (role_name, permissions, description, is_system_role)
VALUES (
    'owner',
    '["credentials:read", "credentials:write", "credentials:delete", "tokens:read", "tokens:write", "tokens:revoke", "audit:read", "users:manage", "roles:manage", "tenant:manage"]'::jsonb,
    '租户所有者 - 拥有全部权限包括租户管理',
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
    role_count INT;
    table_count INT;
    auth_table_count INT;
BEGIN
    SELECT COUNT(*) INTO tenant_count FROM tenants;
    SELECT COUNT(*) INTO role_count FROM tenant_roles;
    SELECT COUNT(*) INTO table_count FROM information_schema.tables WHERE table_schema = 'public';
    SELECT COUNT(*) INTO auth_table_count FROM information_schema.tables
        WHERE table_schema = 'public'
        AND table_name IN ('users', 'external_identities', 'tenant_memberships',
                           'tenant_invitations', 'auth_sessions', 'auth_audit_logs');

    RAISE NOTICE '========================================';
    RAISE NOTICE 'CredBridge 数据库初始化完成!';
    RAISE NOTICE '========================================';
    RAISE NOTICE '  - 租户数量: %', tenant_count;
    RAISE NOTICE '  - 角色数量: %', role_count;
    RAISE NOTICE '  - public schema 表数量: %', table_count;
    RAISE NOTICE '  - 认证域表数量: %', auth_table_count;
    RAISE NOTICE '  - 认证域表: users, external_identities, tenant_memberships';
    RAISE NOTICE '  - 认证域表: tenant_invitations, auth_sessions, auth_audit_logs';
    RAISE NOTICE '========================================';
    RAISE NOTICE '注意: 用户通过 Privy 钱包/邮箱认证后自动创建';
    RAISE NOTICE '========================================';
END
$$;
