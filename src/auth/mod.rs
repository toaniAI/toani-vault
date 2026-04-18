//! 认证模块
//!
//! 实现用户认证、外部身份绑定、租户邀请和会话管理。
//!
//! # 功能概述
//!
//! - **用户管理**: 创建、查询、更新用户状态
//! - **外部身份**: Privy、Email、OAuth 等身份提供商绑定
//! - **租户邀请**: 创建、消费、撤销租户邀请
//! - **会话管理**: 创建、验证、撤销认证会话
//! - **审计日志**: 记录所有认证相关事件
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     AuthService                              │
//! ├─────────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
//! │  │    User      │  │ExternalIdentity│ │AuthSession  │       │
//! │  │   (用户)      │  │  (外部身份)    │  │   (会话)    │       │
//! │  └──────────────┘  └──────────────┘  └──────────────┘       │
//! │                                                              │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
//! │  │TenantMembership│ │TenantInvitation│ │AuthAuditLog │       │
//! │  │  (成员资格)    │  │    (邀请)      │  │ (审计日志)  │       │
//! │  └──────────────┘  └──────────────┘  └──────────────┘       │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     数据存储层                                │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
//! │  │  PostgreSQL  │  │    Redis     │  │  immudb      │       │
//! │  │ (用户数据)    │  │  (会话缓存)   │  │ (审计日志)   │       │
//! │  └──────────────┘  └──────────────┘  └──────────────┘       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Privy Wallet-First 认证流程
//!
//! ```text
//! 1. 用户使用 Privy 钱包登录
//! 2. 前端获取 Privy Token
//! 3. 后端验证 Privy Token，获取用户信息
//! 4. 创建或获取 User + ExternalIdentity
//! 5. 创建 AuthSession，返回 Session Token
//! 6. 用户使用 Session Token 访问 API
//! ```
//!
//! # 租户邀请流程
//!
//! ```text
//! 1. 租户管理员创建邀请（邮箱/钱包/开放链接）
//! 2. 生成邀请 Token 和 Token Hash
//! 3. 发送邀请链接给被邀请者
//! 4. 被邀请者接受邀请，验证 Token
//! 5. 创建 TenantMembership，激活成员资格
//! ```

pub mod error;
pub mod models;
pub mod privy;
pub mod service;

// 重新导出主要类型
pub use error::AuthError;
pub use models::{
    ApiTokenMetadata, ApiTokenSubjectType, ApiTokenType, AuthAuditLog, AuthEventType, AuthSession,
    CreateSessionRequest, CreateUserRequest, ExternalIdentity, IdentityProvider, InvitationStatus,
    InviteeType, MembershipRole, MembershipSource, MembershipStatus, MfaStatus, PrivyAuthResponse,
    ServiceAccount, ServiceAccountStatus, TenantInvitation, TenantMembership, User, UserStatus,
};
pub use privy::{JwksVerifier, PrivyClaims, PrivyCustomClaims};
pub use service::{AuthService, AuthServiceImpl, create_owner_membership};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // 验证主要类型已导出
        let _ = User::new();
        let _ = AuthError::UserNotFound(uuid::Uuid::nil());
        let _ = MembershipRole::Owner;
        let _ = IdentityProvider::Privy;
    }
}
