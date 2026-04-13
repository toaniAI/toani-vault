-- ============================================================================
-- Migration: Privy 钱包优先认证系统数据模型
-- Created: 2026-04-02
-- Description: 创建新的认证域表，支持 Privy 外部身份、租户成员、邀请和会话管理
-- ============================================================================

-- ============================================================================
-- 1. 重构用户主档表 (users)
-- ============================================================================
-- 注意: 这是新模型，与旧的 username/password_hash/role 语义解耦
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 用户状态: active, inactive, suspended, pending_deletion
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    -- 显示名称 (可选，从 Privy profile 同步)
    display_name VARCHAR(128),
    -- 默认租户 ID (用户登录后的默认上下文)
    default_tenant_id UUID REFERENCES tenants(id),
    -- 是否完成 onboarding
    onboarding_completed BOOLEAN NOT NULL DEFAULT FALSE,
    -- 软删除标记
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 用户表索引
CREATE INDEX IF NOT EXISTS idx_users_status ON users(status) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_users_default_tenant ON users(default_tenant_id);
CREATE INDEX IF NOT EXISTS idx_users_deleted_at ON users(deleted_at) WHERE deleted_at IS NOT NULL;

-- ============================================================================
-- 2. 外部身份映射表 (external_identities)
-- ============================================================================
-- 支持多 provider (首版仅 Privy)，一个用户可有多个身份
CREATE TABLE IF NOT EXISTS external_identities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 关联的本地用户 ID
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- 身份提供商: privy, email, etc.
    provider VARCHAR(64) NOT NULL,
    -- 提供商侧唯一标识 (Privy subject)
    provider_subject VARCHAR(256) NOT NULL,
    -- 钱包地址 (钱包登录时)
    wallet_address VARCHAR(256),
    -- 邮箱地址 (邮箱登录时)
    email VARCHAR(256),
    -- 提供商原始 profile 数据 (缓存)
    provider_profile JSONB,
    -- 该身份是否已验证
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    -- 是否为首选身份
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 唯一约束: 同一 provider 的 subject 必须唯一
    UNIQUE(provider, provider_subject)
);

-- 外部身份索引
CREATE INDEX IF NOT EXISTS idx_external_identities_user ON external_identities(user_id);
CREATE INDEX IF NOT EXISTS idx_external_identities_provider_subject ON external_identities(provider, provider_subject);
CREATE INDEX IF NOT EXISTS idx_external_identities_wallet ON external_identities(wallet_address) WHERE wallet_address IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_external_identities_email ON external_identities(email) WHERE email IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_external_identities_primary ON external_identities(user_id, is_primary) WHERE is_primary = TRUE;

-- ============================================================================
-- 3. 租户成员关系表 (tenant_memberships)
-- ============================================================================
-- 用户与租户的多对多关系，包含角色和状态
CREATE TABLE IF NOT EXISTS tenant_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 租户 ID
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- 用户 ID
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- 角色: owner, admin, member, readonly
    role VARCHAR(32) NOT NULL DEFAULT 'member',
    -- 状态: active, pending, suspended, inactive
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    -- 邀请人 ID (通过邀请加入时)
    invited_by UUID REFERENCES users(id),
    -- 加入时间
    joined_at TIMESTAMPTZ,
    -- 成员来源: invitation, direct_add, system
    source VARCHAR(32) NOT NULL DEFAULT 'invitation',
    -- 业务 scopes (JSON 数组，缓存角色映射)
    scopes JSONB DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 唯一约束: 同一租户同一用户只能有一条记录
    UNIQUE(tenant_id, user_id)
);

-- 租户成员索引
CREATE INDEX IF NOT EXISTS idx_tenant_memberships_tenant ON tenant_memberships(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tenant_memberships_user ON tenant_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_tenant_memberships_tenant_status ON tenant_memberships(tenant_id, status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_tenant_memberships_role ON tenant_memberships(tenant_id, role);

-- ============================================================================
-- 4. 租户邀请表 (tenant_invitations)
-- ============================================================================
-- 邀请链接机制，支持邮箱或钱包地址邀请
CREATE TABLE IF NOT EXISTS tenant_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 租户 ID
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- 邀请角色
    role VARCHAR(32) NOT NULL DEFAULT 'member',
    -- 被邀请人标识类型: email, wallet, any
    invitee_type VARCHAR(32) NOT NULL,
    -- 被邀请人邮箱 (invite_type=email 时)
    invitee_email VARCHAR(256),
    -- 被邀请人钱包地址 (invite_type=wallet 时)
    invitee_wallet VARCHAR(256),
    -- 邀请 token hash (用于验证邀请链接)
    token_hash VARCHAR(256) NOT NULL UNIQUE,
    -- 创建人 ID
    created_by UUID NOT NULL REFERENCES users(id),
    -- 过期时间
    expires_at TIMESTAMPTZ NOT NULL,
    -- 消费时间
    consumed_at TIMESTAMPTZ,
    -- 消费者用户 ID
    consumed_by UUID REFERENCES users(id),
    -- 消费时使用的身份 ID
    consumed_identity_id UUID REFERENCES external_identities(id),
    -- 状态: pending, consumed, expired, revoked
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    -- 最大使用次数 (1 表示一次性)
    max_uses INTEGER NOT NULL DEFAULT 1,
    -- 已使用次数
    use_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 约束: email 或 wallet 至少填一个 (当 invitee_type != any 时)
    CONSTRAINT chk_invitation_target CHECK (
        (invitee_type = 'any') OR
        (invitee_type = 'email' AND invitee_email IS NOT NULL) OR
        (invitee_type = 'wallet' AND invitee_wallet IS NOT NULL)
    )
);

-- 邀请表索引
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_tenant ON tenant_invitations(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_token ON tenant_invitations(token_hash);
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_status ON tenant_invitations(tenant_id, status) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_expires ON tenant_invitations(expires_at) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_email ON tenant_invitations(invitee_email) WHERE invitee_email IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_wallet ON tenant_invitations(invitee_wallet) WHERE invitee_wallet IS NOT NULL;

-- ============================================================================
-- 5. 认证会话表 (auth_sessions)
-- ============================================================================
-- 服务端会话管理，用于跟踪用户登录状态
CREATE TABLE IF NOT EXISTS auth_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 用户 ID
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- 会话 token hash (用于验证)
    session_token_hash VARCHAR(256) NOT NULL UNIQUE,
    -- 关联的身份 ID
    identity_id UUID REFERENCES external_identities(id),
    -- 当前活跃租户 membership ID
    active_membership_id UUID REFERENCES tenant_memberships(id),
    -- MFA 验证状态: pending, verified, not_required
    mfa_status VARCHAR(32) NOT NULL DEFAULT 'not_required',
    -- MFA 验证时间
    mfa_verified_at TIMESTAMPTZ,
    -- 客户端信息
    user_agent TEXT,
    ip_address VARCHAR(45),
    -- 过期时间
    expires_at TIMESTAMPTZ NOT NULL,
    -- 最后活跃时间
    last_active_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 撤销时间
    revoked_at TIMESTAMPTZ,
    -- 撤销原因
    revoked_reason VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 会话表索引
CREATE INDEX IF NOT EXISTS idx_auth_sessions_user ON auth_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_token ON auth_sessions(session_token_hash);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires ON auth_sessions(expires_at) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_auth_sessions_active ON auth_sessions(user_id, revoked_at) WHERE revoked_at IS NULL;

-- ============================================================================
-- 6. 认证审计日志表 (auth_audit_logs)
-- ============================================================================
-- 认证相关事件的不可变审计日志
CREATE TABLE IF NOT EXISTS auth_audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 事件类型: login, logout, session_refresh, mfa_verify, invite_created,
    --           invite_consumed, membership_created, account_deleted, etc.
    event_type VARCHAR(64) NOT NULL,
    -- 事件严重程度: info, warning, error, critical
    severity VARCHAR(32) NOT NULL DEFAULT 'info',
    -- 用户 ID (可能为空，如登录前事件)
    user_id UUID REFERENCES users(id),
    -- 身份 ID
    identity_id UUID REFERENCES external_identities(id),
    -- 租户 ID (与租户相关的事件)
    tenant_id UUID REFERENCES tenants(id),
    -- membership ID (与成员关系相关的事件)
    membership_id UUID REFERENCES tenant_memberships(id),
    -- 邀请 ID (与邀请相关的事件)
    invitation_id UUID REFERENCES tenant_invitations(id),
    -- 会话 ID (与会话相关的事件)
    session_id UUID REFERENCES auth_sessions(id),
    -- 事件详情 (JSON)
    event_data JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- 客户端信息
    ip_address VARCHAR(45),
    user_agent TEXT,
    -- 事件时间
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 审计日志索引
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_user ON auth_audit_logs(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_tenant ON auth_audit_logs(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_event ON auth_audit_logs(event_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_created ON auth_audit_logs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_audit_logs_severity ON auth_audit_logs(severity, created_at DESC) WHERE severity IN ('error', 'critical');

-- ============================================================================
-- 7. 创建更新时间触发器
-- ============================================================================

-- users 表触发器
DROP TRIGGER IF EXISTS tr_users_updated_at ON users;
CREATE TRIGGER tr_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- external_identities 表触发器
DROP TRIGGER IF EXISTS tr_external_identities_updated_at ON external_identities;
CREATE TRIGGER tr_external_identities_updated_at
    BEFORE UPDATE ON external_identities
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- tenant_memberships 表触发器
DROP TRIGGER IF EXISTS tr_tenant_memberships_updated_at ON tenant_memberships;
CREATE TRIGGER tr_tenant_memberships_updated_at
    BEFORE UPDATE ON tenant_memberships
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- tenant_invitations 表触发器
DROP TRIGGER IF EXISTS tr_tenant_invitations_updated_at ON tenant_invitations;
CREATE TRIGGER tr_tenant_invitations_updated_at
    BEFORE UPDATE ON tenant_invitations
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- auth_sessions 表触发器
DROP TRIGGER IF EXISTS tr_auth_sessions_updated_at ON auth_sessions;
CREATE TRIGGER tr_auth_sessions_updated_at
    BEFORE UPDATE ON auth_sessions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- 8. 迁移旧用户数据 (可选，首版可跳过，后续提供独立迁移脚本)
-- ============================================================================
-- 注意: 旧 users 表的数据迁移需要独立脚本处理，此处仅创建新表结构
-- 旧 users 表中的数据可通过以下方式手动迁移:
-- 1. 创建新的 users 记录
-- 2. 创建 external_identities 记录 (标记为未验证，等待用户重新认证)
-- 3. 创建 tenant_memberships 记录

-- ============================================================================
-- 9. 验证迁移
-- ============================================================================
DO $$
DECLARE
    table_count INT;
BEGIN
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables
    WHERE table_schema = 'public'
    AND table_name IN ('users', 'external_identities', 'tenant_memberships',
                       'tenant_invitations', 'auth_sessions', 'auth_audit_logs');

    RAISE NOTICE '========================================';
    RAISE NOTICE 'Privy 认证域迁移完成!';
    RAISE NOTICE '========================================';
    RAISE NOTICE '  - 新建表数量: %', table_count;
    RAISE NOTICE '  - 新表: users, external_identities, tenant_memberships';
    RAISE NOTICE '  - 新表: tenant_invitations, auth_sessions, auth_audit_logs';
    RAISE NOTICE '========================================';
END
$$;
