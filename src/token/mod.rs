//! CredBridge Token 模块
//!
//! 实现基于 PASETO v4.local 的有限 Scope Token 系统
//! 支持 Token 签发、验证、撤销和生命周期管理
//!
//! # 模块结构
//!
//! ```text
//! token/
//! ├── mod.rs          - 模块导出
//! ├── claims.rs       - Token Claims 定义和验证
//! ├── paseto.rs       - PASETO Token 实现
//! ├── redis_store.rs  - Redis Token 状态管理
//! ├── revocation.rs   - Token 撤销逻辑
//! ├── scope.rs        - Scope 权限系统 (EP3-Story3.3)
//! └── permission.rs   - 权限检查逻辑 (EP3-Story3.3)
//! ```
//!
//! # 核心功能
//!
//! - **Claims**: Token 声明定义（iss, sub, aud, exp, jti, scope, mfa_verified）
//! - **PasetoToken**: PASETO v4.local Token 签发和验证
//! - **Scope 验证**: 细粒度的权限控制
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use vault_service::token::{
//!     TokenClaims, PasetoToken, TokenError,
//!     scopes::{CREDENTIAL_READ, CREDENTIAL_DECRYPT},
//! };
//!
//! // 生成密钥
//! let key = PasetoToken::generate_key();
//!
//! // 创建 Claims
//! let claims = TokenClaims::with_default_ttl(
//!     "user_123",           // sub: 用户 ID
//!     "tenant_456",         // aud: 租户 ID
//!     "credential:read",    // scope: 权限范围
//!     true,                 // mfa_verified: MFA 验证状态
//! );
//!
//! // 签发 Token
//! let token = PasetoToken::sign(&claims, &key).expect("signing failed");
//!
//! // 验证 Token
//! let validated = PasetoToken::verify(&token, &key, "tenant_456").expect("verification failed");
//!
//! // 检查 Scope
//! assert!(validated.has_scope("credential:read"));
//! ```
//!
//! # Token 格式
//!
//! 使用 PASETO v4.local 格式：
//! ```text
//! v4.local.{base64url(payload)}
//! ```
//!
//! - **v4**: PASETO 版本 4（XChaCha20-Poly1305）
//! - **local**: 对称加密模式
//! - **payload**: 加密的 JSON Claims
//!
//! # 架构约束
//!
//! 符合以下架构约束：
//! - **SA-003**: PASETO v4.local + 2h Access Token + jti 单次使用
//! - **FR2**: 有限 Scope Token 系统

pub mod claims;
pub mod paseto;
pub mod permission;
pub mod redis_store;
pub mod revocation;
pub mod scope;

// 公开导出 - Claims
pub use claims::{
    ClaimsError, DEFAULT_TOKEN_TTL_SECONDS, MAX_SERVICE_ACCOUNT_TOKEN_TTL_SECONDS,
    MAX_TOKEN_TTL_SECONDS, MIN_TOKEN_TTL_SECONDS, ScopeValidator, TOKEN_ISSUED_FROM_ACCESS_TOKEN,
    TOKEN_ISSUED_FROM_AUTOMATION, TOKEN_ISSUED_FROM_SERVICE_ACCOUNT, TOKEN_ISSUED_FROM_SESSION,
    TOKEN_SUBJECT_TYPE_SERVICE_ACCOUNT, TOKEN_SUBJECT_TYPE_USER, TokenClaims,
};

// 公开导出 - Scope 常量
pub use claims::scopes;

// 公开导出 - PASETO
pub use paseto::{
    PasetoKey, PasetoToken, RevocableTokenValidator, TokenError, TokenRevocationChecker,
};

// 公开导出 - Redis Token 存储
pub use redis_store::{RedisTokenStore, TokenMetadata, TokenStoreError, keys as token_keys};

// 公开导出 - Token 撤销
pub use revocation::{
    MemoryRevocationChecker, RevocationError, RevocationPolicy, RevocationResult, TokenRevoker,
};

// 公开导出 - Scope 权限系统 (EP3-Story3.3)
pub use scope::{
    Operation, PermissionChecker, RestrictedTokenContext, Scope, ScopeError, ScopeSet,
    constants as scope_constants,
};

// 公开导出 - 权限检查 (EP3-Story3.3)
pub use permission::{
    AccessContext, AccessDecision, AccessRequest, BatchPermissionChecker, CredentialAccess,
    ExtendedRestrictedContext, PermissionEngine, PermissionError, PermissionPolicy, ResourceType,
};

/// Token 模块版本
pub const TOKEN_VERSION: &str = "0.1.0";

/// Token 验证结果
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum TokenValidationResult {
    /// 验证成功
    Valid(TokenClaims),
    /// Token 已过期
    Expired,
    /// Token 无效
    Invalid(String),
    /// Token 已被撤销
    Revoked,
}

impl TokenValidationResult {
    /// 检查验证是否成功
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid(_))
    }

    /// 获取成功的 Claims
    pub fn claims(&self) -> Option<&TokenClaims> {
        match self {
            Self::Valid(claims) => Some(claims),
            _ => None,
        }
    }
}

/// Token 统计信息
#[derive(Debug, Clone, Default)]
pub struct TokenStats {
    /// 已签发 Token 数量
    pub issued_count: u64,
    /// 已验证 Token 数量
    pub verified_count: u64,
    /// 已撤销 Token 数量
    pub revoked_count: u64,
    /// 过期 Token 数量
    pub expired_count: u64,
}

/// 创建新的 Token（便捷函数）
///
/// # 参数
/// - `user_id`: 用户 ID
/// - `tenant_id`: 租户 ID
/// - `scope`: 权限范围（空格分隔）
/// - `mfa_verified`: MFA 验证状态
///
/// # 返回值
/// (TokenClaims, PasetoKey) - Claims 和生成的密钥
pub fn create_token(
    user_id: impl Into<String>,
    tenant_id: impl Into<String>,
    scope: impl Into<String>,
    mfa_verified: bool,
) -> (TokenClaims, PasetoKey) {
    let claims = TokenClaims::with_default_ttl(user_id, tenant_id, scope, mfa_verified);
    let key = PasetoToken::generate_key();
    (claims, key)
}

/// 快速验证 Token（便捷函数）
///
/// # 参数
/// - `token`: Token 字符串
/// - `key`: PASETO 密钥
/// - `tenant_id`: 期望的租户 ID
///
/// # 返回值
/// TokenValidationResult - 验证结果
pub fn quick_verify(token: &str, key: &PasetoKey, tenant_id: &str) -> TokenValidationResult {
    match PasetoToken::verify(token, key, tenant_id) {
        Ok(claims) => TokenValidationResult::Valid(claims),
        Err(TokenError::Expired) | Err(TokenError::ClaimsError(ClaimsError::Expired)) => {
            TokenValidationResult::Expired
        }
        Err(TokenError::Revoked(_)) => TokenValidationResult::Revoked,
        Err(e) => TokenValidationResult::Invalid(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_version() {
        assert_eq!(TOKEN_VERSION, "0.1.0");
    }

    #[test]
    fn test_create_token() {
        let (claims, key) = create_token("user_123", "tenant_456", "credential:read", true);

        assert_eq!(claims.sub, "user_123");
        assert_eq!(claims.aud, "tenant_456");
        assert_eq!(claims.scope, "credential:read");
        assert!(claims.mfa_verified);
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn test_quick_verify_valid() {
        let (claims, key) = create_token("user_123", "tenant_456", "credential:read", true);

        let token = PasetoToken::sign(&claims, &key).unwrap();
        let result = quick_verify(&token, &key, "tenant_456");

        assert!(result.is_valid());
        assert!(result.claims().is_some());
    }

    #[test]
    fn test_quick_verify_invalid() {
        let key = PasetoToken::generate_key();
        let result = quick_verify("invalid.token", &key, "tenant_456");

        assert!(!result.is_valid());
        assert!(matches!(result, TokenValidationResult::Invalid(_)));
    }

    #[test]
    fn test_token_stats_default() {
        let stats = TokenStats::default();
        assert_eq!(stats.issued_count, 0);
        assert_eq!(stats.verified_count, 0);
        assert_eq!(stats.revoked_count, 0);
        assert_eq!(stats.expired_count, 0);
    }

    #[test]
    fn test_token_validation_result() {
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let valid = TokenValidationResult::Valid(claims.clone());
        assert!(valid.is_valid());
        assert_eq!(valid.claims().unwrap().sub, "user_123");

        let expired = TokenValidationResult::Expired;
        assert!(!expired.is_valid());
        assert!(expired.claims().is_none());

        let revoked = TokenValidationResult::Revoked;
        assert!(!revoked.is_valid());

        let invalid = TokenValidationResult::Invalid("test".to_string());
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_scope_constants() {
        assert_eq!(scopes::CREDENTIAL_READ, "credential:read");
        assert_eq!(scopes::CREDENTIAL_DECRYPT, "credential:decrypt");
        assert_eq!(scopes::CREDENTIAL_WRITE, "credential:write");
        assert_eq!(scopes::CREDENTIAL_DELETE, "credential:delete");
        assert_eq!(scopes::TOKEN_MANAGE, "token:manage");
        assert_eq!(scopes::AUDIT_READ, "audit:read");
        assert_eq!(scopes::ADMIN, "admin");
    }
}
