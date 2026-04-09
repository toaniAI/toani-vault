//! 认证模块错误类型
//!
//! 定义认证相关操作的所有错误类型，使用 thiserror 实现。

use thiserror::Error;
use uuid::Uuid;

use super::models::IdentityProvider;

/// 认证错误类型
#[derive(Debug, Error)]
pub enum AuthError {
    /// 用户未找到
    #[error("用户未找到: {0}")]
    UserNotFound(Uuid),

    /// 用户已存在
    #[error("用户已存在: {user_id}")]
    UserAlreadyExists { user_id: Uuid },

    /// 外部身份已存在
    #[error("外部身份已存在: provider={provider}, subject={subject}")]
    ExternalIdentityAlreadyExists {
        provider: IdentityProvider,
        subject: String,
    },

    /// 外部身份未找到
    #[error("外部身份未找到: provider={provider}, subject={subject}")]
    ExternalIdentityNotFound {
        provider: IdentityProvider,
        subject: String,
    },

    /// 外部身份验证失败
    #[error("外部身份验证失败: {message}")]
    ExternalIdentityVerificationFailed { message: String },

    /// 无效的 Privy Token
    #[error("无效的 Privy Token: {0}")]
    InvalidPrivyToken(String),

    /// Privy Token 已过期
    #[error("Privy Token 已过期")]
    PrivyTokenExpired,

    /// Privy Token 验证失败
    #[error("Privy Token 验证失败: {0}")]
    PrivyTokenVerificationFailed(String),

    /// Privy JWKS 获取失败
    #[error("Privy JWKS 获取失败: {0}")]
    PrivyJwksError(String),

    /// Privy API 错误
    #[error("Privy API 错误: {status} - {message}")]
    PrivyApiError { status: u16, message: String },

    /// Privy 认证失败（保留兼容性）
    #[error("Privy 认证失败: {0}")]
    PrivyAuthenticationFailed(String),

    /// MFA 验证要求
    #[error("需要 MFA 验证")]
    MfaRequired,

    /// 租户成员资格未找到
    #[error("租户成员资格未找到: user={user_id}, tenant={tenant_id}")]
    MembershipNotFound { user_id: Uuid, tenant_id: Uuid },

    /// 租户成员资格已存在
    #[error("租户成员资格已存在: user={user_id}, tenant={tenant_id}")]
    MembershipAlreadyExists { user_id: Uuid, tenant_id: Uuid },

    /// 邀请未找到
    #[error("邀请未找到: {0}")]
    InvitationNotFound(Uuid),

    /// 邀请已过期
    #[error("邀请已过期: {0}")]
    InvitationExpired(Uuid),

    /// 邵请已使用
    #[error("邀请已使用: {0}")]
    InvitationAlreadyConsumed(Uuid),

    /// 邀请已撤销
    #[error("邀请已撤销: {0}")]
    InvitationRevoked(Uuid),

    /// 邀请已存在待处理记录
    #[error("邀请已存在待处理记录: tenant={tenant_id}, invitee={invitee}")]
    DuplicatePendingInvitation { tenant_id: Uuid, invitee: String },

    /// 无效的邀请 Token
    #[error("无效的邀请 Token")]
    InvalidInvitationToken,

    /// 会话未找到
    #[error("会话未找到: {0}")]
    SessionNotFound(Uuid),

    /// 会话已过期
    #[error("会话已过期: {0}")]
    SessionExpired(Uuid),

    /// 会话已撤销
    #[error("会话已撤销: {0}")]
    SessionRevoked(Uuid),

    /// Service Account 未找到
    #[error("service account 未找到: {0}")]
    ServiceAccountNotFound(Uuid),

    /// Service Account 已存在
    #[error("service account 已存在: tenant={tenant_id}, name={name}")]
    ServiceAccountAlreadyExists { tenant_id: Uuid, name: String },

    /// API Token 元数据未找到
    #[error("API token 元数据未找到: {0}")]
    ApiTokenNotFound(String),

    /// MFA 验证失败
    #[error("MFA 验证失败")]
    MfaVerificationFailed,

    /// MFA 未设置
    #[error("MFA 未设置")]
    MfaNotSetup,

    /// 权限不足
    #[error("权限不足: 需要 {required}，当前 {current}")]
    InsufficientPermissions { required: String, current: String },

    /// 用户状态不允许操作
    #[error("用户状态不允许操作: {status}")]
    InvalidUserStatus { status: String },

    /// 成员资格状态不允许操作
    #[error("成员资格状态不允许操作: {status}")]
    InvalidMembershipStatus { status: String },

    /// 数据库错误
    #[error("数据库错误: {0}")]
    DatabaseError(#[from] sqlx::Error),

    /// 序列化错误
    #[error("序列化错误: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// 加密错误
    #[error("加密错误: {0}")]
    CryptoError(String),

    /// Token 生成错误
    #[error("Token 生成错误: {0}")]
    TokenGenerationError(String),

    /// 内部错误
    #[error("内部错误: {0}")]
    InternalError(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    ConfigError(String),
}

impl AuthError {
    /// 判断错误是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AuthError::DatabaseError(_) | AuthError::InternalError(_) | AuthError::CryptoError(_)
        )
    }

    /// 判断错误是否为客户端错误（不可重试）
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            AuthError::UserNotFound(_)
                | AuthError::InvalidPrivyToken(_)
                | AuthError::PrivyTokenExpired
                | AuthError::PrivyTokenVerificationFailed(_)
                | AuthError::MfaRequired
                | AuthError::InvitationExpired(_)
                | AuthError::InvalidInvitationToken
                | AuthError::InsufficientPermissions { .. }
                | AuthError::InvalidUserStatus { .. }
                | AuthError::InvalidMembershipStatus { .. }
        )
    }

    /// 获取 HTTP 状态码映射
    pub fn http_status_code(&self) -> u16 {
        match self {
            AuthError::UserNotFound(_) => 404,
            AuthError::ExternalIdentityNotFound { .. } => 404,
            AuthError::MembershipNotFound { .. } => 404,
            AuthError::InvitationNotFound(_) => 404,
            AuthError::SessionNotFound(_) => 404,
            AuthError::ServiceAccountNotFound(_) => 404,
            AuthError::ApiTokenNotFound(_) => 404,

            AuthError::UserAlreadyExists { .. } => 409,
            AuthError::ExternalIdentityAlreadyExists { .. } => 409,
            AuthError::MembershipAlreadyExists { .. } => 409,
            AuthError::InvitationAlreadyConsumed(_) => 409,
            AuthError::DuplicatePendingInvitation { .. } => 409,
            AuthError::ServiceAccountAlreadyExists { .. } => 409,

            AuthError::InvalidPrivyToken(_) => 401,
            AuthError::PrivyTokenExpired => 401,
            AuthError::PrivyTokenVerificationFailed(_) => 401,
            AuthError::PrivyAuthenticationFailed(_) => 401,
            AuthError::PrivyJwksError(_) => 503,
            AuthError::PrivyApiError { status, .. } => *status,
            AuthError::MfaRequired => 403,
            AuthError::ExternalIdentityVerificationFailed { .. } => 401,
            AuthError::SessionExpired(_) => 401,
            AuthError::SessionRevoked(_) => 401,
            AuthError::MfaVerificationFailed => 401,
            AuthError::MfaNotSetup => 401,
            AuthError::InvalidInvitationToken => 401,
            AuthError::InsufficientPermissions { .. } => 403,

            AuthError::InvitationExpired(_) => 410,
            AuthError::InvitationRevoked(_) => 410,

            AuthError::InvalidUserStatus { .. } => 400,
            AuthError::InvalidMembershipStatus { .. } => 400,
            AuthError::SerializationError(_) => 400,
            AuthError::ConfigError(_) => 400,

            AuthError::DatabaseError(_) => 500,
            AuthError::CryptoError(_) => 500,
            AuthError::TokenGenerationError(_) => 500,
            AuthError::InternalError(_) => 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_retryable() {
        let db_error = AuthError::DatabaseError(sqlx::Error::RowNotFound);
        assert!(db_error.is_retryable());

        let client_error = AuthError::UserNotFound(Uuid::nil());
        assert!(!client_error.is_retryable());
    }

    #[test]
    fn test_error_http_status() {
        assert_eq!(AuthError::UserNotFound(Uuid::nil()).http_status_code(), 404);
        assert_eq!(
            AuthError::UserAlreadyExists {
                user_id: Uuid::nil()
            }
            .http_status_code(),
            409
        );
        assert_eq!(
            AuthError::InvalidPrivyToken("test".to_string()).http_status_code(),
            401
        );
        assert_eq!(
            AuthError::DatabaseError(sqlx::Error::RowNotFound).http_status_code(),
            500
        );
    }
}
