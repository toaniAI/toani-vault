//! Token Claims 定义
//!
//! 定义 PASETO Token 的 Claims 结构体和验证逻辑
//! 符合 PASETO v4.local 规范

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

/// Token Claims 验证错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ClaimsError {
    #[error("Token 已过期")]
    Expired,

    #[error("无效的 issuer: {0}")]
    InvalidIssuer(String),

    #[error("无效的 audience: 期望 {expected}, 实际 {actual}")]
    InvalidAudience { expected: String, actual: String },

    #[error("无效的 subject")]
    InvalidSubject,

    #[error("无效的 jti")]
    InvalidJti,

    #[error("无效的 scope")]
    InvalidScope,

    #[error("Token 尚未生效 (nbf)")]
    NotYetValid,

    #[error("Claims 序列化失败: {0}")]
    SerializationError(String),

    #[error("Claims 反序列化失败: {0}")]
    DeserializationError(String),
}

/// Token Claims 结构体
///
/// 包含 PASETO Token 的所有标准声明和自定义声明
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenClaims {
    /// Issuer - Token 签发者 (固定为 "credbridge-vault")
    pub iss: String,

    /// Subject - 用户 ID
    pub sub: String,

    /// Audience - 租户 ID
    pub aud: String,

    /// Expiration Time - 过期时间（Unix 时间戳，秒）
    pub exp: u64,

    /// Issued At - 签发时间（Unix 时间戳，秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,

    /// Not Before - 生效时间（Unix 时间戳，秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<u64>,

    /// JWT ID - 唯一标识符（UUID v4）
    pub jti: String,

    /// Scope - 权限范围（逗号分隔）
    pub scope: String,

    /// MFA 验证状态
    #[serde(default)]
    pub mfa_verified: bool,

    /// 主体类型，默认用户
    #[serde(
        default = "default_subject_type",
        skip_serializing_if = "is_default_subject_type"
    )]
    pub subject_type: String,

    /// 令牌来源，默认 session
    #[serde(
        default = "default_issued_from",
        skip_serializing_if = "is_default_issued_from"
    )]
    pub issued_from: String,

    /// 令牌平面，默认 management
    #[serde(
        default = "default_token_plane",
        skip_serializing_if = "is_default_token_plane"
    )]
    pub token_plane: String,

    /// 成员资格 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub membership_id: Option<String>,

    /// 来源会话 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// 运行时 binding handle 白名单
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binding_handles: Vec<String>,
}

impl TokenClaims {
    /// 创建新的 Token Claims
    ///
    /// # 参数
    /// - `sub`: 用户 ID
    /// - `aud`: 租户 ID
    /// - `scope`: 权限范围
    /// - `mfa_verified`: MFA 验证状态
    /// - `ttl_seconds`: Token 有效期（秒）
    ///
    /// # 返回值
    /// 返回新的 TokenClaims 实例
    pub fn new(
        sub: impl Into<String>,
        aud: impl Into<String>,
        scope: impl Into<String>,
        mfa_verified: bool,
        ttl_seconds: u64,
    ) -> Self {
        let now = current_timestamp();
        let exp = now + ttl_seconds;
        let jti = Uuid::now_v7().to_string();

        Self {
            iss: "credbridge-vault".to_string(),
            sub: sub.into(),
            aud: aud.into(),
            exp,
            iat: Some(now),
            nbf: Some(now),
            jti,
            scope: scope.into(),
            mfa_verified,
            subject_type: default_subject_type(),
            issued_from: default_issued_from(),
            token_plane: default_token_plane(),
            membership_id: None,
            session_id: None,
            binding_handles: Vec::new(),
        }
    }

    /// 创建标准有效期（15分钟）的 Token Claims
    ///
    /// 根据 SA-003 架构约束，Access Token 默认有效期为 15 分钟
    pub fn with_default_ttl(
        sub: impl Into<String>,
        aud: impl Into<String>,
        scope: impl Into<String>,
        mfa_verified: bool,
    ) -> Self {
        Self::new(sub, aud, scope, mfa_verified, DEFAULT_TOKEN_TTL_SECONDS)
    }

    /// 验证 Claims 有效性
    ///
    /// # 参数
    /// - `expected_audience`: 期望的 audience（租户 ID）
    ///
    /// # 返回值
    /// - `Ok(())`: 验证通过
    /// - `Err(ClaimsError)`: 验证失败
    pub fn validate(&self, expected_audience: &str) -> Result<(), ClaimsError> {
        let now = current_timestamp();

        // 验证 iss
        if self.iss != "credbridge-vault" {
            return Err(ClaimsError::InvalidIssuer(self.iss.clone()));
        }

        // 验证 sub
        if self.sub.is_empty() {
            return Err(ClaimsError::InvalidSubject);
        }

        // 验证 aud
        if self.aud != expected_audience {
            return Err(ClaimsError::InvalidAudience {
                expected: expected_audience.to_string(),
                actual: self.aud.clone(),
            });
        }

        // 验证 nbf (Not Before)
        if let Some(nbf) = self.nbf
            && now < nbf
        {
            return Err(ClaimsError::NotYetValid);
        }

        // 验证 exp (Expiration)
        if now >= self.exp {
            return Err(ClaimsError::Expired);
        }

        // 验证 jti
        if self.jti.is_empty() || Uuid::parse_str(&self.jti).is_err() {
            return Err(ClaimsError::InvalidJti);
        }

        // 验证 scope
        if self.scope.is_empty() {
            return Err(ClaimsError::InvalidScope);
        }

        Ok(())
    }

    /// 检查 Token 是否过期
    pub fn is_expired(&self) -> bool {
        current_timestamp() >= self.exp
    }

    /// 获取剩余有效时间（秒）
    pub fn remaining_ttl(&self) -> u64 {
        let now = current_timestamp();
        self.exp.saturating_sub(now)
    }

    /// 序列化为 JSON 字符串
    pub fn to_json(&self) -> Result<String, ClaimsError> {
        serde_json::to_string(self).map_err(|e| ClaimsError::SerializationError(e.to_string()))
    }

    /// 从 JSON 字符串反序列化
    pub fn from_json(json: &str) -> Result<Self, ClaimsError> {
        serde_json::from_str(json).map_err(|e| ClaimsError::DeserializationError(e.to_string()))
    }

    /// 解析 scope 为权限列表
    pub fn scopes(&self) -> Vec<&str> {
        self.scope.split_whitespace().collect()
    }

    /// 检查是否包含特定 scope
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes().contains(&scope)
    }

    /// 检查是否包含任意指定 scope
    pub fn has_any_scope(&self, scopes: &[&str]) -> bool {
        let my_scopes = self.scopes();
        scopes.iter().any(|s| my_scopes.contains(s))
    }

    /// 检查是否包含所有指定 scope
    pub fn has_all_scopes(&self, scopes: &[&str]) -> bool {
        let my_scopes = self.scopes();
        scopes.iter().all(|s| my_scopes.contains(s))
    }

    pub fn with_subject_type(mut self, subject_type: impl Into<String>) -> Self {
        self.subject_type = subject_type.into();
        self
    }

    pub fn with_issued_from(mut self, issued_from: impl Into<String>) -> Self {
        self.issued_from = issued_from.into();
        self
    }

    pub fn with_token_plane(mut self, token_plane: impl Into<String>) -> Self {
        self.token_plane = token_plane.into();
        self
    }

    pub fn with_membership_id(mut self, membership_id: impl Into<String>) -> Self {
        self.membership_id = Some(membership_id.into());
        self
    }

    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_binding_handles(mut self, binding_handles: Vec<String>) -> Self {
        self.binding_handles = binding_handles;
        self
    }
}

/// Scope 权限常量定义
pub mod scopes {
    /// 读取凭证元数据
    pub const CREDENTIAL_READ: &str = "credential:read";

    /// 解密凭证内容
    pub const CREDENTIAL_DECRYPT: &str = "credential:decrypt";

    /// 创建/更新凭证
    pub const CREDENTIAL_WRITE: &str = "credential:write";

    /// 删除凭证
    pub const CREDENTIAL_DELETE: &str = "credential:delete";

    /// 管理 Token（撤销/刷新）
    pub const TOKEN_MANAGE: &str = "token:manage";

    /// 读取审计日志
    pub const AUDIT_READ: &str = "audit:read";

    /// 所有管理权限
    pub const ADMIN: &str = "admin";
}

/// 默认 Token 有效期（2 小时 = 7200 秒）
/// 符合 SA-003 架构约束
pub const DEFAULT_TOKEN_TTL_SECONDS: u64 = 7200;

/// Token 最小有效期（15 分钟 = 900 秒）
pub const MIN_TOKEN_TTL_SECONDS: u64 = 900;

/// 最大 Token 有效期（7 天）
pub const MAX_TOKEN_TTL_SECONDS: u64 = 604800;

/// 服务账号 Token 的最大有效期（24 小时）
pub const MAX_SERVICE_ACCOUNT_TOKEN_TTL_SECONDS: u64 = 86400;

pub const TOKEN_SUBJECT_TYPE_USER: &str = "user";
pub const TOKEN_SUBJECT_TYPE_SERVICE_ACCOUNT: &str = "service_account";
pub const TOKEN_ISSUED_FROM_SESSION: &str = "session";
pub const TOKEN_ISSUED_FROM_AUTOMATION: &str = "automation";
pub const TOKEN_ISSUED_FROM_ACCESS_TOKEN: &str = "access_token";
pub const TOKEN_ISSUED_FROM_SERVICE_ACCOUNT: &str = "service_account";

fn default_subject_type() -> String {
    TOKEN_SUBJECT_TYPE_USER.to_string()
}

fn is_default_subject_type(value: &str) -> bool {
    value == TOKEN_SUBJECT_TYPE_USER
}

fn default_issued_from() -> String {
    TOKEN_ISSUED_FROM_SESSION.to_string()
}

fn is_default_issued_from(value: &str) -> bool {
    value == TOKEN_ISSUED_FROM_SESSION
}

fn default_token_plane() -> String {
    "management".to_string()
}

fn is_default_token_plane(value: &str) -> bool {
    value == "management"
}

/// 获取当前 Unix 时间戳（秒）
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_secs()
}

/// Scope 权限验证器
pub struct ScopeValidator;

impl ScopeValidator {
    /// 验证操作是否被 scope 允许
    ///
    /// # 参数
    /// - `token_scopes`: Token 中的 scope 列表
    /// - `required_scope`: 操作所需的 scope
    ///
    /// # 返回值
    /// `true` 如果允许访问
    pub fn can_access(token_scopes: &[&str], required_scope: &str) -> bool {
        // admin 拥有所有权限
        if token_scopes.contains(&scopes::ADMIN) {
            return true;
        }

        // 检查特定权限
        match required_scope {
            scopes::CREDENTIAL_READ => {
                token_scopes.contains(&scopes::CREDENTIAL_READ)
                    || token_scopes.contains(&scopes::CREDENTIAL_DECRYPT)
                    || token_scopes.contains(&scopes::CREDENTIAL_WRITE)
                    || token_scopes.contains(&scopes::ADMIN)
            }
            scopes::CREDENTIAL_DECRYPT => {
                token_scopes.contains(&scopes::CREDENTIAL_DECRYPT)
                    || token_scopes.contains(&scopes::ADMIN)
            }
            scopes::CREDENTIAL_WRITE => {
                token_scopes.contains(&scopes::CREDENTIAL_WRITE)
                    || token_scopes.contains(&scopes::ADMIN)
            }
            scopes::CREDENTIAL_DELETE => {
                token_scopes.contains(&scopes::CREDENTIAL_DELETE)
                    || token_scopes.contains(&scopes::CREDENTIAL_WRITE)
                    || token_scopes.contains(&scopes::ADMIN)
            }
            scopes::TOKEN_MANAGE => {
                token_scopes.contains(&scopes::TOKEN_MANAGE)
                    || token_scopes.contains(&scopes::ADMIN)
            }
            scopes::AUDIT_READ => {
                token_scopes.contains(&scopes::AUDIT_READ) || token_scopes.contains(&scopes::ADMIN)
            }
            _ => token_scopes.contains(&required_scope),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_claims_new() {
        let claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read",
            true,
            DEFAULT_TOKEN_TTL_SECONDS,
        );

        assert_eq!(claims.iss, "credbridge-vault");
        assert_eq!(claims.sub, "user_123");
        assert_eq!(claims.aud, "tenant_456");
        assert_eq!(claims.scope, "credential:read");
        assert!(claims.mfa_verified);
        assert!(!claims.jti.is_empty());
        assert!(claims.iat.is_some());
        assert!(claims.nbf.is_some());
    }

    #[test]
    fn test_token_claims_with_default_ttl() {
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", false);

        let expected_exp = claims.iat.unwrap() + DEFAULT_TOKEN_TTL_SECONDS;
        assert_eq!(claims.exp, expected_exp);
    }

    #[test]
    fn test_validate_valid_claims() {
        let claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read",
            true,
            DEFAULT_TOKEN_TTL_SECONDS,
        );

        assert!(claims.validate("tenant_456").is_ok());
    }

    #[test]
    fn test_validate_invalid_issuer() {
        let mut claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read",
            true,
            DEFAULT_TOKEN_TTL_SECONDS,
        );
        claims.iss = "invalid-issuer".to_string();

        assert_eq!(
            claims.validate("tenant_456"),
            Err(ClaimsError::InvalidIssuer("invalid-issuer".to_string()))
        );
    }

    #[test]
    fn test_validate_invalid_audience() {
        let claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read",
            true,
            DEFAULT_TOKEN_TTL_SECONDS,
        );

        assert_eq!(
            claims.validate("wrong_tenant"),
            Err(ClaimsError::InvalidAudience {
                expected: "wrong_tenant".to_string(),
                actual: "tenant_456".to_string(),
            })
        );
    }

    #[test]
    fn test_validate_expired() {
        let mut claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read",
            true,
            DEFAULT_TOKEN_TTL_SECONDS,
        );
        // 设置为已过期
        claims.exp = 1; // 过去的 Unix 时间戳

        assert_eq!(claims.validate("tenant_456"), Err(ClaimsError::Expired));
    }

    #[test]
    fn test_is_expired() {
        let mut claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read",
            true,
            DEFAULT_TOKEN_TTL_SECONDS,
        );

        assert!(!claims.is_expired());

        // 设置为已过期
        claims.exp = 1;
        assert!(claims.is_expired());
    }

    #[test]
    fn test_remaining_ttl() {
        let claims = TokenClaims::new("user_123", "tenant_456", "credential:read", true, 900);

        let ttl = claims.remaining_ttl();
        assert!(ttl > 0 && ttl <= DEFAULT_TOKEN_TTL_SECONDS);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read credential:write",
            true,
            DEFAULT_TOKEN_TTL_SECONDS,
        );

        let json = claims.to_json().unwrap();
        let deserialized = TokenClaims::from_json(&json).unwrap();

        assert_eq!(claims, deserialized);
    }

    #[test]
    fn test_scopes_parsing() {
        let claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read credential:write admin",
            true,
            DEFAULT_TOKEN_TTL_SECONDS,
        );

        let scopes = claims.scopes();
        assert_eq!(scopes.len(), 3);
        assert!(scopes.contains(&"credential:read"));
        assert!(scopes.contains(&"credential:write"));
        assert!(scopes.contains(&"admin"));
    }

    #[test]
    fn test_has_scope() {
        let claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read admin",
            true,
            DEFAULT_TOKEN_TTL_SECONDS,
        );

        assert!(claims.has_scope("credential:read"));
        assert!(claims.has_scope("admin"));
        assert!(!claims.has_scope("credential:write"));
    }

    #[test]
    fn test_scope_validator_admin() {
        let scopes = vec!["admin"];
        assert!(ScopeValidator::can_access(&scopes, scopes::CREDENTIAL_READ));
        assert!(ScopeValidator::can_access(
            &scopes,
            scopes::CREDENTIAL_DECRYPT
        ));
        assert!(ScopeValidator::can_access(
            &scopes,
            scopes::CREDENTIAL_WRITE
        ));
        assert!(ScopeValidator::can_access(&scopes, scopes::TOKEN_MANAGE));
        assert!(ScopeValidator::can_access(&scopes, scopes::AUDIT_READ));
    }

    #[test]
    fn test_scope_validator_read_only() {
        let scopes = vec!["credential:read"];
        assert!(ScopeValidator::can_access(&scopes, scopes::CREDENTIAL_READ));
        assert!(!ScopeValidator::can_access(
            &scopes,
            scopes::CREDENTIAL_DECRYPT
        ));
        assert!(!ScopeValidator::can_access(
            &scopes,
            scopes::CREDENTIAL_WRITE
        ));
    }

    #[test]
    fn test_scope_validator_hierarchy() {
        // write 包含 delete 权限
        let scopes = vec!["credential:write"];
        assert!(ScopeValidator::can_access(
            &scopes,
            scopes::CREDENTIAL_DELETE
        ));

        // decrypt 包含 read 权限
        let scopes = vec!["credential:decrypt"];
        assert!(ScopeValidator::can_access(&scopes, scopes::CREDENTIAL_READ));
    }
}
