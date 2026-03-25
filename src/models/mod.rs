//! 数据模型模块

use serde::{Deserialize, Serialize};

/// 凭证类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialType {
    /// 用户名密码
    #[serde(rename = "username_password")]
    UsernamePassword,
    /// OAuth 刷新令牌
    #[serde(
        rename = "oauth_refresh",
        alias = "oauth_token",
        alias = "o_auth_refresh"
    )]
    OAuthRefresh,
    /// API 密钥
    #[serde(rename = "api_key")]
    ApiKey,
    /// 会话 Cookie
    #[serde(rename = "session_cookie")]
    SessionCookie,
    /// KYC 文档
    #[serde(rename = "kyc_document")]
    KycDocument,
}

impl CredentialType {
    /// 获取凭证类型的字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            CredentialType::UsernamePassword => "username_password",
            CredentialType::OAuthRefresh => "oauth_refresh",
            CredentialType::ApiKey => "api_key",
            CredentialType::SessionCookie => "session_cookie",
            CredentialType::KycDocument => "kyc_document",
        }
    }
}

/// 凭证元数据（不含加密内容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialMetadata {
    /// 凭证ID
    pub credential_id: String,
    /// 凭证类型
    pub credential_type: CredentialType,
    /// 用户ID（哈希后）
    pub user_id_hash: String,
    /// 服务ID
    pub service_id: String,
    /// 租户ID
    pub tenant_id: String,
    /// 创建时间
    pub created_at: String,
    /// 过期时间
    pub expires_at: Option<String>,
    /// 是否已删除
    pub is_deleted: bool,
    /// 版本号
    pub version: u32,
}

/// 加密凭证存储格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredential {
    /// 元数据
    pub metadata: CredentialMetadata,
    /// 加密后的载荷
    pub encrypted_payload: String,
    /// 加密版本
    pub version: u8,
    /// 加密算法
    pub algorithm: String,
    /// KDF 算法
    pub kdf: String,
}

/// 租户配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    /// 租户ID
    pub tenant_id: String,
    /// 存储配额（字节）
    pub storage_quota: u64,
    /// Token 有效期（秒）
    pub token_ttl_seconds: u64,
    /// 审计日志保留期（天）
    pub audit_retention_days: u32,
    /// 是否启用
    pub is_active: bool,
}

impl Default for TenantConfig {
    fn default() -> Self {
        Self {
            tenant_id: String::new(),
            storage_quota: 1024 * 1024 * 1024, // 1GB
            token_ttl_seconds: 900,            // 15分钟
            audit_retention_days: 7,
            is_active: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CredentialType;

    #[test]
    fn oauth_refresh_serializes_to_canonical_value() {
        let json = serde_json::to_string(&CredentialType::OAuthRefresh).unwrap();
        assert_eq!(json, "\"oauth_refresh\"");
    }

    #[test]
    fn oauth_refresh_deserializes_legacy_aliases() {
        let legacy_values = ["\"oauth_refresh\"", "\"oauth_token\"", "\"o_auth_refresh\""];

        for value in legacy_values {
            let parsed: CredentialType = serde_json::from_str(value).unwrap();
            assert_eq!(parsed, CredentialType::OAuthRefresh);
        }
    }
}
