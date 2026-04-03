//! Privy JWKS Token 验证器
//!
//! 实现基于 JWKS 的 Privy Token 签名验证，包括公钥缓存和自动刷新。

use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::auth::error::AuthError;
use crate::config::PrivyConfig;

/// JWKS 响应结构
#[derive(Debug, Clone, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

/// JWK 密钥结构
#[derive(Debug, Clone, Deserialize)]
struct JwkKey {
    /// 密钥 ID
    kid: String,
    /// 密钥类型
    kty: String,
    /// 算法
    alg: String,
    /// 公钥模数 (RSA)
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<String>,
    /// 公钥指数 (RSA)
    #[serde(skip_serializing_if = "Option::is_none")]
    e: Option<String>,
    /// 曲线 (EC)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    crv: Option<String>,
    /// X 坐标 (EC)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    x: Option<String>,
    /// Y 坐标 (EC)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    y: Option<String>,
}

/// Privy Token 声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivyClaims {
    /// 主题 (DID)
    pub sub: String,
    /// 发行人
    pub iss: String,
    /// 受众
    pub aud: String,
    /// 过期时间
    pub exp: i64,
    /// 签发时间
    pub iat: i64,
    /// 自定义字段
    #[serde(flatten)]
    pub custom: PrivyCustomClaims,
}

/// Privy 自定义声明
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrivyCustomClaims {
    /// 钱包地址
    #[serde(rename = "wallet_address", skip_serializing_if = "Option::is_none")]
    pub wallet_address: Option<String>,
    /// 邮箱
    #[serde(rename = "email", skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// 用户名
    #[serde(rename = "name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 用户 ID
    #[serde(rename = "user_id", skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

/// 缓存的 JWKS 数据
#[derive(Debug, Clone)]
struct CachedJwks {
    /// JWKS 响应
    jwks: JwksResponse,
    /// 缓存时间
    cached_at: Instant,
}

impl CachedJwks {
    /// 检查缓存是否过期
    fn is_expired(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }
}

/// JWKS Token 验证器
#[derive(Debug, Clone)]
pub struct JwksVerifier {
    /// Privy 配置
    config: PrivyConfig,
    /// 缓存的 JWKS
    cached_jwks: Arc<RwLock<Option<CachedJwks>>>,
    /// 缓存 TTL
    cache_ttl: Duration,
    /// HTTP 客户端
    http_client: reqwest::Client,
}

impl JwksVerifier {
    /// 创建新的 JWKS 验证器
    pub fn new(config: PrivyConfig) -> Self {
        Self {
            config,
            cached_jwks: Arc::new(RwLock::new(None)),
            cache_ttl: Duration::from_secs(3600), // 1 小时默认 TTL
            http_client: reqwest::Client::new(),
        }
    }

    /// 设置缓存 TTL
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// 验证 Privy Token
    ///
    /// 解析并验证 JWT 签名，返回解析后的声明。
    pub async fn verify(&self, token: &str) -> Result<PrivyClaims, AuthError> {
        // 1. 解码头部获取 kid
        let header = decode_header(token)
            .map_err(|e| AuthError::InvalidPrivyToken(format!("Failed to decode header: {e}")))?;

        let kid = header.kid.ok_or_else(|| {
            AuthError::InvalidPrivyToken("Token missing 'kid' in header".to_string())
        })?;

        debug!("Verifying token with kid: {}", kid);

        // 2. 获取 JWKS
        let jwks = self.fetch_jwks().await?;

        // 3. 查找匹配的密钥
        let jwk = jwks
            .keys
            .into_iter()
            .find(|k| k.kid == kid)
            .ok_or_else(|| {
                AuthError::PrivyJwksError(format!("No matching key found for kid: {kid}"))
            })?;

        // 4. 创建解码密钥
        let decoding_key = self.create_decoding_key(&jwk)?;

        // 5. 验证 Token
        let mut validation = Validation::new(self.algorithm_from_str(&jwk.alg)?);
        validation.set_audience(&[&self.config.app_id]);
        validation.set_issuer(&["https://auth.privy.io"]);

        let token_data = decode::<PrivyClaims>(token, &decoding_key, &validation).map_err(|e| {
            match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::PrivyTokenExpired,
                _ => {
                    AuthError::PrivyTokenVerificationFailed(format!("Token validation failed: {e}"))
                }
            }
        })?;

        info!(
            "Token verified successfully for did: {}",
            token_data.claims.sub
        );

        Ok(token_data.claims)
    }

    /// 获取 JWKS（带缓存）
    async fn fetch_jwks(&self) -> Result<JwksResponse, AuthError> {
        // 先尝试从缓存读取
        {
            let cached = self.cached_jwks.read().await;
            if let Some(ref cached_jwks) = *cached {
                if !cached_jwks.is_expired(self.cache_ttl) {
                    debug!("Using cached JWKS");
                    return Ok(cached_jwks.jwks.clone());
                }
            }
        }

        // 缓存未命中或过期，获取新的 JWKS
        debug!("Fetching fresh JWKS from {}", self.config.jwks_url);

        let response = self
            .http_client
            .get(&self.config.jwks_url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| AuthError::PrivyJwksError(format!("Failed to fetch JWKS: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AuthError::PrivyJwksError(format!(
                "JWKS endpoint returned {status}: {text}"
            )));
        }

        let jwks: JwksResponse = response.json().await.map_err(|e| {
            AuthError::PrivyJwksError(format!("Failed to parse JWKS response: {e}"))
        })?;

        if jwks.keys.is_empty() {
            return Err(AuthError::PrivyJwksError(
                "JWKS response contains no keys".to_string(),
            ));
        }

        info!("Fetched {} keys from JWKS endpoint", jwks.keys.len());

        // 更新缓存
        {
            let mut cached = self.cached_jwks.write().await;
            *cached = Some(CachedJwks {
                jwks: jwks.clone(),
                cached_at: Instant::now(),
            });
        }

        Ok(jwks)
    }

    /// 从 JWK 创建解码密钥
    fn create_decoding_key(&self, jwk: &JwkKey) -> Result<DecodingKey, AuthError> {
        match jwk.kty.as_str() {
            "RSA" => {
                let n = jwk.n.as_ref().ok_or_else(|| {
                    AuthError::PrivyJwksError("RSA key missing 'n' parameter".to_string())
                })?;
                let e = jwk.e.as_ref().ok_or_else(|| {
                    AuthError::PrivyJwksError("RSA key missing 'e' parameter".to_string())
                })?;

                DecodingKey::from_rsa_components(n, e).map_err(|e| {
                    AuthError::PrivyJwksError(format!("Failed to create RSA decoding key: {e}"))
                })
            }
            "EC" => {
                // EC 密钥支持
                warn!("EC keys not yet fully supported");
                Err(AuthError::PrivyJwksError(
                    "EC key type not yet supported".to_string(),
                ))
            }
            _ => Err(AuthError::PrivyJwksError(format!(
                "Unsupported key type: {}",
                jwk.kty
            ))),
        }
    }

    /// 将算法字符串转换为 Algorithm
    fn algorithm_from_str(&self, alg: &str) -> Result<Algorithm, AuthError> {
        match alg {
            "RS256" => Ok(Algorithm::RS256),
            "RS384" => Ok(Algorithm::RS384),
            "RS512" => Ok(Algorithm::RS512),
            "ES256" => Ok(Algorithm::ES256),
            "ES384" => Ok(Algorithm::ES384),
            _ => Err(AuthError::PrivyJwksError(format!(
                "Unsupported algorithm: {alg}"
            ))),
        }
    }

    /// 清除缓存（用于测试或强制刷新）
    pub async fn clear_cache(&self) {
        let mut cached = self.cached_jwks.write().await;
        *cached = None;
        debug!("JWKS cache cleared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_algorithm_from_str() {
        let config = PrivyConfig {
            app_id: "test".to_string(),
            app_secret: "test".to_string(),
            jwks_url: "https://test".to_string(),
            api_url: "https://test".to_string(),
            mock_enabled: true,
        };

        let verifier = JwksVerifier::new(config);

        assert_eq!(
            verifier.algorithm_from_str("RS256").unwrap(),
            Algorithm::RS256
        );
        assert_eq!(
            verifier.algorithm_from_str("RS384").unwrap(),
            Algorithm::RS384
        );
        assert_eq!(
            verifier.algorithm_from_str("RS512").unwrap(),
            Algorithm::RS512
        );
        assert!(verifier.algorithm_from_str("INVALID").is_err());
    }

    #[test]
    fn test_cached_jwks_expiration() {
        let jwks = JwksResponse { keys: vec![] };
        let cached = CachedJwks {
            jwks,
            cached_at: Instant::now(),
        };

        assert!(!cached.is_expired(Duration::from_secs(3600)));
        assert!(cached.is_expired(Duration::from_secs(0)));
    }
}
