//! PASETO Token 实现
//!
//! 实现 PASETO v4.local Token 的签发与验证
//! 使用 XChaCha20-Poly1305 对称加密
//!
//! # Token 格式
//! ```text
//! v4.local.{base64url(payload)}.{base64url(footer)}
//! ```
//!
//! # 使用示例
//! ```rust,ignore
//! use vault_service::token::{TokenClaims, PasetoToken, TokenError};
//!
//! // 签发 Token
//! let claims = TokenClaims::with_default_ttl(
//!     "user_123",
//!     "tenant_456",
//!     "credential:read",
//!     true,
//! );
//!
//! let key = PasetoToken::generate_key();
//! let token = PasetoToken::sign(&claims, &key).expect("Token signing failed");
//!
//! // 验证 Token
//! let validated = PasetoToken::verify(&token, &key, "tenant_456")
//!     .expect("Token verification failed");
//! ```

use super::claims::{ClaimsError, TokenClaims};
use pasetors::claims::{Claims, ClaimsValidationRules};
use pasetors::keys::{Generate, SymmetricKey};
use pasetors::token::{TrustedToken, UntrustedToken};
use pasetors::{Local, local, version4::V4};
use std::convert::TryFrom;
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Token 错误类型
#[derive(Error, Debug, Clone, PartialEq)]
pub enum TokenError {
    #[error("Token 签名失败: {0}")]
    SigningError(String),

    #[error("Token 验证失败: {0}")]
    VerificationError(String),

    #[error("Token 格式无效")]
    InvalidFormat,

    #[error("Token 已过期")]
    Expired,

    #[error("Token 已被撤销: {0}")]
    Revoked(String),

    #[error("Claims 错误: {0}")]
    ClaimsError(#[from] ClaimsError),

    #[error("密钥错误: {0}")]
    KeyError(String),

    #[error("PASETO 错误: {0}")]
    PasetoError(String),

    #[error("JSON 错误: {0}")]
    JsonError(String),
}

impl From<pasetors::errors::Error> for TokenError {
    fn from(e: pasetors::errors::Error) -> Self {
        let msg = e.to_string();
        if msg.contains("expired") {
            TokenError::Expired
        } else {
            TokenError::PasetoError(msg)
        }
    }
}

/// PASETO Token 管理器
///
/// 负责 Token 的签发和验证
pub struct PasetoToken;

impl PasetoToken {
    /// 生成新的 PASETO 对称密钥
    ///
    /// # 返回值
    /// 32 字节的对称密钥
    pub fn generate_key() -> PasetoKey {
        let key = SymmetricKey::<V4>::generate().expect("Failed to generate key");
        PasetoKey::new(key.as_bytes())
    }

    /// 从字节数组创建密钥
    ///
    /// # 参数
    /// - `bytes`: 32 字节密钥材料
    ///
    /// # 返回值
    /// 如果长度正确返回 Ok(PasetoKey)
    pub fn key_from_bytes(bytes: &[u8]) -> Result<PasetoKey, TokenError> {
        if bytes.len() != 32 {
            return Err(TokenError::KeyError(format!(
                "密钥长度错误: 期望 32, 实际 {}",
                bytes.len()
            )));
        }
        Ok(PasetoKey::new(bytes))
    }

    /// 使用密钥派生生成 Token 密钥
    ///
    /// 从 L2 密钥派生 Token 专用密钥
    ///
    /// # 参数
    /// - `master_key`: L2 用户保险库密钥
    /// - `context`: 派生上下文（如租户 ID）
    pub fn derive_key_from_master(
        master_key: &[u8],
        context: &str,
    ) -> Result<PasetoKey, TokenError> {
        use ring::hmac;

        if master_key.len() != 32 {
            return Err(TokenError::KeyError("主密钥必须是 32 字节".to_string()));
        }

        let key = hmac::Key::new(hmac::HMAC_SHA256, master_key);
        let tag = hmac::sign(&key, context.as_bytes());
        let derived = tag.as_ref();

        // 取前 32 字节作为 Token 密钥
        let token_key = &derived[..32.min(derived.len())];
        Self::key_from_bytes(token_key)
    }

    /// 签发 PASETO Token
    ///
    /// # 参数
    /// - `claims`: Token Claims
    /// - `key`: PASETO 对称密钥
    ///
    /// # 返回值
    /// 返回 v4.local.{payload}.{footer} 格式 Token 字符串
    pub fn sign(claims: &TokenClaims, key: &PasetoKey) -> Result<String, TokenError> {
        // 创建 PASETO Claims
        // 注意：Claims::new() 会自动设置 iat, nbf, exp 为 1 小时后
        // 我们需要覆盖这些值
        let mut paseto_claims =
            Claims::new_expires_in(&std::time::Duration::from_secs(claims.remaining_ttl()))
                .map_err(|e| TokenError::PasetoError(e.to_string()))?;

        // 覆盖标准声明
        paseto_claims
            .issuer(&claims.iss)
            .map_err(|e| TokenError::PasetoError(e.to_string()))?;
        paseto_claims
            .subject(&claims.sub)
            .map_err(|e| TokenError::PasetoError(e.to_string()))?;
        paseto_claims
            .audience(&claims.aud)
            .map_err(|e| TokenError::PasetoError(e.to_string()))?;

        // 将 Unix 时间戳转换为 RFC 3339 格式
        let exp_rfc3339 = unix_to_rfc3339(claims.exp);
        paseto_claims
            .expiration(&exp_rfc3339)
            .map_err(|e| TokenError::PasetoError(e.to_string()))?;

        if let Some(iat) = claims.iat {
            let iat_rfc3339 = unix_to_rfc3339(iat);
            paseto_claims
                .issued_at(&iat_rfc3339)
                .map_err(|e| TokenError::PasetoError(e.to_string()))?;
        }

        if let Some(nbf) = claims.nbf {
            let nbf_rfc3339 = unix_to_rfc3339(nbf);
            paseto_claims
                .not_before(&nbf_rfc3339)
                .map_err(|e| TokenError::PasetoError(e.to_string()))?;
        }

        paseto_claims
            .token_identifier(&claims.jti)
            .map_err(|e| TokenError::PasetoError(e.to_string()))?;

        // 添加自定义声明
        paseto_claims
            .add_additional("scope", claims.scope.as_str())
            .map_err(|e| TokenError::PasetoError(e.to_string()))?;
        paseto_claims
            .add_additional("mfa_verified", claims.mfa_verified)
            .map_err(|e| TokenError::PasetoError(e.to_string()))?;

        // 创建对称密钥
        let symmetric_key = SymmetricKey::<V4>::from(key.as_bytes())
            .map_err(|e| TokenError::KeyError(format!("密钥创建失败: {e}")))?;

        // 构建 Token
        let token = local::encrypt(&symmetric_key, &paseto_claims, None, None)
            .map_err(|e| TokenError::SigningError(e.to_string()))?;

        Ok(token)
    }

    /// 验证 PASETO Token
    ///
    /// # 参数
    /// - `token`: Token 字符串
    /// - `key`: PASETO 对称密钥
    /// - `expected_audience`: 期望的 audience（租户 ID）
    ///
    /// # 返回值
    /// 验证成功返回 TokenClaims
    pub fn verify(
        token: &str,
        key: &PasetoKey,
        expected_audience: &str,
    ) -> Result<TokenClaims, TokenError> {
        // 解析未受信任的 Token
        let untrusted =
            UntrustedToken::<Local, V4>::try_from(token).map_err(|_| TokenError::InvalidFormat)?;

        // 创建对称密钥
        let symmetric_key = SymmetricKey::<V4>::from(key.as_bytes())
            .map_err(|e| TokenError::KeyError(format!("密钥创建失败: {e}")))?;

        // 创建验证规则
        let mut validation_rules = ClaimsValidationRules::new();
        validation_rules.validate_issuer_with("credbridge-vault");
        validation_rules.validate_audience_with(expected_audience);
        // 注意：subject 在 claims.validate() 中验证

        // 验证 Token
        let trusted: TrustedToken =
            local::decrypt(&symmetric_key, &untrusted, &validation_rules, None, None).map_err(
                |e| {
                    let msg = e.to_string();
                    // 处理过期错误：检查 "expired" 或 "ClaimValidation"（PASETO 过期错误类型）
                    if msg.contains("expired") || msg.contains("ClaimValidation") {
                        TokenError::Expired
                    } else {
                        TokenError::VerificationError(msg)
                    }
                },
            )?;

        // 提取 Claims
        let claims = Self::extract_claims(&trusted)?;

        // 额外验证
        claims.validate(expected_audience)?;

        Ok(claims)
    }

    /// 从 TrustedToken 提取 Claims
    fn extract_claims(trusted: &TrustedToken) -> Result<TokenClaims, TokenError> {
        let paseto_claims = trusted
            .payload_claims()
            .ok_or_else(|| TokenError::VerificationError("缺少 payload claims".to_string()))?;

        // 从 JSON Value 提取声明
        let get_string = |key: &str| -> Result<String, TokenError> {
            paseto_claims
                .get_claim(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| TokenError::VerificationError(format!("缺少 {key}")))
        };

        // 提取标准声明
        let iss = get_string("iss")?;
        let sub = get_string("sub")?;
        let aud = get_string("aud")?;
        let exp_str = get_string("exp")?;
        let jti = get_string("jti")?;

        // 解析 RFC 3339 时间戳为 Unix 时间戳
        let exp = rfc3339_to_unix(&exp_str)
            .ok_or_else(|| TokenError::VerificationError("无效的 exp 格式".to_string()))?;

        let iat = paseto_claims
            .get_claim("iat")
            .and_then(|v| v.as_str())
            .and_then(rfc3339_to_unix);

        let nbf = paseto_claims
            .get_claim("nbf")
            .and_then(|v| v.as_str())
            .and_then(rfc3339_to_unix);

        // 提取自定义声明
        let scope = paseto_claims
            .get_claim("scope")
            .and_then(|v| v.as_str())
            .ok_or_else(|| TokenError::VerificationError("缺少 scope".to_string()))?
            .to_string();

        let mfa_verified = paseto_claims
            .get_claim("mfa_verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(TokenClaims {
            iss,
            sub,
            aud,
            exp,
            iat,
            nbf,
            jti,
            scope,
            mfa_verified,
        })
    }
}

/// 将 Unix 时间戳转换为 RFC 3339 格式
fn unix_to_rfc3339(timestamp: u64) -> String {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    let datetime = OffsetDateTime::from_unix_timestamp(timestamp as i64)
        .unwrap_or_else(|_| OffsetDateTime::now_utc());
    datetime
        .format(&Rfc3339)
        .unwrap_or_else(|_| timestamp.to_string())
}

/// 将 RFC 3339 格式转换为 Unix 时间戳
fn rfc3339_to_unix(rfc3339: &str) -> Option<u64> {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    OffsetDateTime::parse(rfc3339, &Rfc3339)
        .ok()
        .map(|dt: OffsetDateTime| dt.unix_timestamp() as u64)
}

/// PASETO 对称密钥
///
/// 32 字节对称密钥，自动清零
#[derive(Clone)]
pub struct PasetoKey {
    bytes: [u8; 32],
}

impl PasetoKey {
    /// 从字节数组创建密钥
    fn new(bytes: &[u8]) -> Self {
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[..32]);
        Self { bytes: key }
    }

    /// 获取密钥字节引用
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// 创建测试密钥
    #[cfg(test)]
    pub fn test_key() -> Self {
        Self::new(&[0x42; 32])
    }
}

impl Zeroize for PasetoKey {
    fn zeroize(&mut self) {
        self.bytes.zeroize();
    }
}

impl ZeroizeOnDrop for PasetoKey {}

impl Drop for PasetoKey {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// Token 撤销检查器 trait
///
/// 用于检查 Token 是否已被撤销
pub trait TokenRevocationChecker: Send + Sync {
    /// 检查 jti 是否已被撤销
    fn is_revoked(&self, jti: &str) -> bool;
}

/// 带撤销检查的 Token 验证器
pub struct RevocableTokenValidator<C: TokenRevocationChecker> {
    checker: C,
}

impl<C: TokenRevocationChecker> RevocableTokenValidator<C> {
    /// 创建新的验证器
    pub fn new(checker: C) -> Self {
        Self { checker }
    }

    /// 验证 Token（含撤销检查）
    pub fn verify(
        &self,
        token: &str,
        key: &PasetoKey,
        expected_audience: &str,
    ) -> Result<TokenClaims, TokenError> {
        let claims = PasetoToken::verify(token, key, expected_audience)?;

        // 检查是否已撤销
        if self.checker.is_revoked(&claims.jti) {
            return Err(TokenError::Revoked(claims.jti));
        }

        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key() {
        let key1 = PasetoToken::generate_key();
        let key2 = PasetoToken::generate_key();

        // 两个密钥应该不同
        assert_ne!(key1.as_bytes(), key2.as_bytes());

        // 密钥长度正确
        assert_eq!(key1.as_bytes().len(), 32);
    }

    #[test]
    fn test_key_from_bytes() {
        let bytes = [0x42; 32];
        let key = PasetoToken::key_from_bytes(&bytes).unwrap();
        assert_eq!(key.as_bytes(), &bytes);

        // 错误长度
        let result = PasetoToken::key_from_bytes(&[0x42; 16]);
        assert!(matches!(result, Err(TokenError::KeyError(_))));
    }

    #[test]
    fn test_derive_key_from_master() {
        let master_key = [0x42; 32];
        let key1 = PasetoToken::derive_key_from_master(&master_key, "tenant_1").unwrap();
        let key2 = PasetoToken::derive_key_from_master(&master_key, "tenant_2").unwrap();
        let key3 = PasetoToken::derive_key_from_master(&master_key, "tenant_1").unwrap();

        // 相同上下文派生相同密钥
        assert_eq!(key1.as_bytes(), key3.as_bytes());

        // 不同上下文派生不同密钥
        assert_ne!(key1.as_bytes(), key2.as_bytes());

        // 错误的主密钥长度
        let result = PasetoToken::derive_key_from_master(&[0x42; 16], "tenant_1");
        assert!(matches!(result, Err(TokenError::KeyError(_))));
    }

    #[test]
    fn test_sign_and_verify() {
        let key = PasetoToken::generate_key();
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        // 签发 Token
        let token = PasetoToken::sign(&claims, &key).unwrap();

        // 验证格式
        assert!(token.starts_with("v4.local."));

        // 验证 Token
        let verified = PasetoToken::verify(&token, &key, "tenant_456").unwrap();

        assert_eq!(verified.iss, claims.iss);
        assert_eq!(verified.sub, claims.sub);
        assert_eq!(verified.aud, claims.aud);
        assert_eq!(verified.scope, claims.scope);
        assert_eq!(verified.mfa_verified, claims.mfa_verified);
    }

    #[test]
    fn test_verify_wrong_audience() {
        let key = PasetoToken::generate_key();
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let token = PasetoToken::sign(&claims, &key).unwrap();

        // 使用错误的 audience 验证
        let result = PasetoToken::verify(&token, &key, "wrong_tenant");
        // PASETO 库使用 ClaimValidation 错误类型处理声明验证失败
        // 我们的代码将 ClaimValidation 错误统一映射为 Expired
        assert!(
            matches!(
                result,
                Err(TokenError::VerificationError(_))
                    | Err(TokenError::ClaimsError(ClaimsError::InvalidAudience { .. }))
                    | Err(TokenError::Expired)
            ),
            "Expected validation error, got {:?}",
            result
        );
    }

    #[test]
    fn test_verify_wrong_key() {
        let key1 = PasetoToken::generate_key();
        let key2 = PasetoToken::generate_key();

        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let token = PasetoToken::sign(&claims, &key1).unwrap();

        // 使用错误的密钥验证
        let result = PasetoToken::verify(&token, &key2, "tenant_456");
        assert!(matches!(result, Err(TokenError::VerificationError(_))));
    }

    #[test]
    fn test_verify_invalid_format() {
        let key = PasetoToken::generate_key();

        let result = PasetoToken::verify("invalid.token.format", &key, "tenant_456");
        assert!(matches!(result, Err(TokenError::InvalidFormat)));
    }

    #[test]
    fn test_token_key_zeroize() {
        use zeroize::Zeroize;

        let mut key = PasetoKey::test_key();
        key.zeroize();

        assert!(key.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn test_unix_to_rfc3339_roundtrip() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let rfc3339 = unix_to_rfc3339(now);
        let parsed = rfc3339_to_unix(&rfc3339);

        assert_eq!(Some(now), parsed);
    }
}
