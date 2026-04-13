//! PostgreSQL Schema 管理
//!
//! 提供多租户 Schema 的创建、管理和删除功能。
//! 使用 Schema-per-Tenant 模式实现数据隔离。
//!
//! ## Schema 分层说明
//!
//! **Public Schema (认证域)** - 由 migrations 管理:
//! - `users`: 用户主档表 (Privy 钱包优先认证)
//! - `external_identities`: 外部身份映射 (Privy, email 等)
//! - `tenant_memberships`: 用户-租户成员关系
//! - `tenant_invitations`: 租户邀请
//! - `auth_sessions`: 认证会话
//! - `auth_audit_logs`: 认证审计日志
//!
//! **Tenant Schema (tenant_xxx)** - 由本模块管理:
//! - `credentials`: 凭证存储
//! - `scope_tokens`: Scope Token
//! - `audit_logs`: 租户级审计日志
//! - `tenant_roles`: 租户角色定义 (副本)
//! - `user_roles`: 用户角色关联

use super::pool::{DatabaseError, DatabasePool, execute_pg_script_tx};

/// Schema 管理器
pub struct SchemaManager {
    db: DatabasePool,
}

/// 租户 Schema 表结构 SQL
const TENANT_SCHEMA_SQL: &str = r#"
-- 凭证存储表
CREATE TABLE IF NOT EXISTS credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    credential_id VARCHAR(64) NOT NULL UNIQUE,
    user_id_hash VARCHAR(128) NOT NULL,
    service_id VARCHAR(64) NOT NULL,
    credential_type VARCHAR(32) NOT NULL,
    encrypted_payload TEXT NOT NULL,
    version SMALLINT NOT NULL DEFAULT 1,
    algorithm VARCHAR(32) NOT NULL,
    nonce VARCHAR(64) NOT NULL,
    auth_tag VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
    metadata JSONB
);

-- 凭证索引
CREATE INDEX IF NOT EXISTS idx_credentials_user_id ON credentials(user_id_hash);
CREATE INDEX IF NOT EXISTS idx_credentials_service ON credentials(service_id);
CREATE INDEX IF NOT EXISTS idx_credentials_type ON credentials(credential_type);
CREATE INDEX IF NOT EXISTS idx_credentials_expires ON credentials(expires_at) WHERE expires_at IS NOT NULL;

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

-- Token 索引
CREATE INDEX IF NOT EXISTS idx_scope_tokens_credential ON scope_tokens(credential_id);
CREATE INDEX IF NOT EXISTS idx_scope_tokens_expires ON scope_tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_scope_tokens_revoked ON scope_tokens(revoked_at) WHERE revoked_at IS NULL;

-- 审计日志表（本地副本）
CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type VARCHAR(64) NOT NULL,
    event_data JSONB NOT NULL,
    user_id_hash VARCHAR(128),
    ip_address VARCHAR(45),
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 审计索引
CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type ON audit_logs(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_logs_user ON audit_logs(user_id_hash);

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

CREATE INDEX IF NOT EXISTS idx_user_roles_user ON user_roles(user_id_hash);
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_roles_unique ON user_roles(user_id_hash, role_id);
"#;

/// 默认角色 SQL
const DEFAULT_ROLES_SQL: &str = r#"
-- Owner 角色 (租户所有者，通常为第一个加入的用户)
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
"#;

impl SchemaManager {
    /// 创建新的 Schema 管理器
    pub fn new(db: DatabasePool) -> Self {
        Self { db }
    }

    /// 为租户创建 Schema
    ///
    /// # Arguments
    /// * `tenant_id` - 租户 ID
    ///
    /// # Returns
    /// * `Ok(())` - 创建成功
    /// * `Err(DatabaseError)` - 创建失败
    pub async fn create_tenant_schema(&self, tenant_id: &str) -> Result<(), DatabaseError> {
        let schema_name = Self::schema_name_for_tenant(tenant_id);

        // 创建 Schema
        let create_schema_sql = format!("CREATE SCHEMA IF NOT EXISTS \"{schema_name}\"");

        sqlx::query(&create_schema_sql)
            .execute(self.db.pool())
            .await
            .map_err(|e| DatabaseError::SchemaError(format!("Failed to create schema: {e}")))?;

        // 设置 search_path 并创建表结构
        let set_path_sql = format!("SET search_path TO \"{schema_name}\"");

        // 在事务中执行所有 SQL
        let mut tx = self
            .db
            .pool()
            .begin()
            .await
            .map_err(|e| DatabaseError::TransactionError(e.to_string()))?;

        // 设置 search_path
        sqlx::query(&set_path_sql)
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::SchemaError(format!("Failed to set search_path: {e}")))?;

        // 创建表结构（多语句必须逐条执行，不能塞进单个 prepared statement）
        execute_pg_script_tx(&mut tx, TENANT_SCHEMA_SQL)
            .await
            .map_err(|e| DatabaseError::SchemaError(format!("Failed to create tables: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| DatabaseError::TransactionError(e.to_string()))?;

        tracing::info!("Created tenant schema: {}", schema_name);
        Ok(())
    }

    /// 为租户创建默认角色
    ///
    /// # Arguments
    /// * `tenant_id` - 租户 ID
    pub async fn create_default_roles(&self, tenant_id: &str) -> Result<(), DatabaseError> {
        let schema_name = Self::schema_name_for_tenant(tenant_id);

        let mut tx = self
            .db
            .pool()
            .begin()
            .await
            .map_err(|e| DatabaseError::TransactionError(e.to_string()))?;

        // 设置 search_path
        let set_path_sql = format!("SET search_path TO \"{schema_name}\"");
        sqlx::query(&set_path_sql)
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::SchemaError(e.to_string()))?;

        // 创建默认角色
        execute_pg_script_tx(&mut tx, DEFAULT_ROLES_SQL)
            .await
            .map_err(|e| DatabaseError::SchemaError(format!("Failed to create roles: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| DatabaseError::TransactionError(e.to_string()))?;

        tracing::info!("Created default roles for tenant: {}", tenant_id);
        Ok(())
    }

    /// 删除租户 Schema
    ///
    /// # Arguments
    /// * `tenant_id` - 租户 ID
    ///
    /// # Warning
    /// 此操作将删除该租户的所有数据，不可恢复！
    pub async fn drop_tenant_schema(&self, tenant_id: &str) -> Result<(), DatabaseError> {
        let schema_name = Self::schema_name_for_tenant(tenant_id);

        let drop_sql = format!("DROP SCHEMA IF EXISTS \"{schema_name}\" CASCADE");

        sqlx::query(&drop_sql)
            .execute(self.db.pool())
            .await
            .map_err(|e| DatabaseError::SchemaError(format!("Failed to drop schema: {e}")))?;

        tracing::warn!("Dropped tenant schema: {}", schema_name);
        Ok(())
    }

    /// 检查 Schema 是否存在
    pub async fn schema_exists(&self, tenant_id: &str) -> Result<bool, DatabaseError> {
        let schema_name = Self::schema_name_for_tenant(tenant_id);

        let result: Option<(bool,)> = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = $1)",
        )
        .bind(&schema_name)
        .fetch_optional(self.db.pool())
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(result.map(|r| r.0).unwrap_or(false))
    }

    /// 生成租户 Schema 名称
    fn schema_name_for_tenant(tenant_id: &str) -> String {
        // 使用 tenant_ 前缀，确保合法的 PostgreSQL 标识符
        // 替换可能的非法字符
        let safe_id = tenant_id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        format!("tenant_{safe_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_name_generation() {
        assert_eq!(
            SchemaManager::schema_name_for_tenant("abc123"),
            "tenant_abc123"
        );
        assert_eq!(
            SchemaManager::schema_name_for_tenant("test-tenant"),
            "tenant_test_tenant"
        );
    }
}
