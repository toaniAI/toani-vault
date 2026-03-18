//! 租户管理模块
//!
//! 实现多租户系统的配置管理、租户生命周期管理和资源隔离。
//!
//! # 功能概述
//!
//! - **租户配置管理**: 功能开关、配额限制、自定义设置
//! - **租户生命周期**: 创建、激活、暂停、删除
//! - **资源隔离**: Schema-per-Tenant + RLS 数据隔离
//! - **加密密钥管理**: 每租户独立的加密密钥层次
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     TenantManager                            │
//! ├─────────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
//! │  │ TenantConfig │  │TenantService │  │TenantStorage │       │
//! │  │   (配置)      │  │   (服务层)    │  │   (存储层)    │       │
//! │  └──────────────┘  └──────────────┘  └──────────────┘       │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     数据存储层                                │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
//! │  │  PostgreSQL  │  │    Redis     │  │    Vault     │       │
//! │  │ (租户配置表)  │  │  (配置缓存)   │  │  (加密密钥)   │       │
//! │  └──────────────┘  └──────────────┘  └──────────────┘       │
//! └─────────────────────────────────────────────────────────────┘
//! ```

pub mod config;
pub mod service;

// 重新导出主要类型
pub use config::{
    FeatureFlags, MemoryTenantConfigStore, PartialTenantConfig, QuotaLimits, TenantConfig,
    TenantConfigError, TenantConfigManager, TenantConfigStore, TenantSettings, TenantStatus,
};
pub use service::{
    CreateTenantRequest, CreateTenantResult, MemoryTenantStorage, TenantCreationError,
    TenantManager, TenantManagerBuilder, TenantProvisioningError, TenantService,
    UpdateTenantRequest,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 租户ID (包装类型)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl TenantId {
    /// 创建新的租户ID
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }

    /// 从字符串创建租户ID
    pub fn from_string(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 获取租户ID字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TenantId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TenantId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// 租户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    /// 租户ID
    pub id: TenantId,
    /// 租户名称
    pub name: String,
    /// 租户状态
    pub status: TenantStatus,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    /// 删除时间（软删除）
    pub deleted_at: Option<DateTime<Utc>>,
    /// 租户配置
    pub config: TenantConfig,
}

impl Tenant {
    /// 创建新的租户
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: TenantId::new(),
            name: name.into(),
            status: TenantStatus::Pending,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            config: TenantConfig::default(),
        }
    }

    /// 检查租户是否活跃
    pub fn is_active(&self) -> bool {
        self.status == TenantStatus::Active && self.deleted_at.is_none()
    }

    /// 激活租户
    pub fn activate(&mut self) {
        self.status = TenantStatus::Active;
        self.updated_at = Utc::now();
    }

    /// 暂停租户
    pub fn suspend(&mut self) {
        self.status = TenantStatus::Suspended;
        self.updated_at = Utc::now();
    }

    /// 软删除租户
    pub fn soft_delete(&mut self) {
        self.status = TenantStatus::Deleted;
        self.deleted_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// 更新配置
    pub fn update_config(&mut self, config: TenantConfig) {
        self.config = config;
        self.updated_at = Utc::now();
    }
}

/// 租户角色
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenantRole {
    /// 租户管理员
    Admin,
    /// 普通用户
    User,
    /// 只读用户
    ReadOnly,
    /// 服务账户
    ServiceAccount,
}

impl TenantRole {
    /// 获取角色权限级别
    pub fn permission_level(&self) -> u8 {
        match self {
            TenantRole::Admin => 100,
            TenantRole::User => 50,
            TenantRole::ReadOnly => 10,
            TenantRole::ServiceAccount => 30,
        }
    }

    /// 检查角色是否有管理权限
    pub fn can_manage(&self) -> bool {
        matches!(self, TenantRole::Admin)
    }
}

impl std::fmt::Display for TenantRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TenantRole::Admin => write!(f, "admin"),
            TenantRole::User => write!(f, "user"),
            TenantRole::ReadOnly => write!(f, "readonly"),
            TenantRole::ServiceAccount => write!(f, "service_account"),
        }
    }
}

impl std::str::FromStr for TenantRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "admin" => Ok(TenantRole::Admin),
            "user" => Ok(TenantRole::User),
            "readonly" | "read_only" | "read-only" => Ok(TenantRole::ReadOnly),
            "serviceaccount" | "service_account" | "service-account" => {
                Ok(TenantRole::ServiceAccount)
            }
            _ => Err(format!("Unknown tenant role: {}", s)),
        }
    }
}

/// 默认角色配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultRoles {
    /// 是否创建默认管理员角色
    pub create_admin_role: bool,
    /// 是否创建默认用户角色
    pub create_user_role: bool,
    /// 是否创建只读角色
    pub create_readonly_role: bool,
}

impl Default for DefaultRoles {
    fn default() -> Self {
        Self {
            create_admin_role: true,
            create_user_role: true,
            create_readonly_role: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_id_creation() {
        let id1 = TenantId::new();
        let id2 = TenantId::new();
        assert_ne!(id1.as_str(), id2.as_str());
        assert!(!id1.as_str().is_empty());
    }

    #[test]
    fn test_tenant_creation() {
        let tenant = Tenant::new("Test Tenant");
        assert_eq!(tenant.name, "Test Tenant");
        assert_eq!(tenant.status, TenantStatus::Pending);
        assert!(!tenant.is_active());
    }

    #[test]
    fn test_tenant_activate() {
        let mut tenant = Tenant::new("Test");
        tenant.activate();
        assert!(tenant.is_active());
        assert_eq!(tenant.status, TenantStatus::Active);
    }

    #[test]
    fn test_tenant_soft_delete() {
        let mut tenant = Tenant::new("Test");
        tenant.activate();
        tenant.soft_delete();
        assert!(!tenant.is_active());
        assert_eq!(tenant.status, TenantStatus::Deleted);
        assert!(tenant.deleted_at.is_some());
    }

    #[test]
    fn test_tenant_role_permissions() {
        assert!(TenantRole::Admin.can_manage());
        assert!(!TenantRole::User.can_manage());
        assert!(!TenantRole::ReadOnly.can_manage());

        assert_eq!(TenantRole::Admin.permission_level(), 100);
        assert_eq!(TenantRole::User.permission_level(), 50);
        assert_eq!(TenantRole::ReadOnly.permission_level(), 10);
    }

    #[test]
    fn test_tenant_role_from_str() {
        assert_eq!("admin".parse::<TenantRole>().unwrap(), TenantRole::Admin);
        assert_eq!(
            "read-only".parse::<TenantRole>().unwrap(),
            TenantRole::ReadOnly
        );
        assert_eq!(
            "service_account".parse::<TenantRole>().unwrap(),
            TenantRole::ServiceAccount
        );
        assert!("unknown".parse::<TenantRole>().is_err());
    }
}
