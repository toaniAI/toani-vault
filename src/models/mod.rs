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
        rename = "oauth_token",
        alias = "oauth_refresh",
        alias = "o_auth_refresh"
    )]
    OAuthRefresh,
    /// API 密钥
    #[serde(rename = "api_key")]
    ApiKey,
    /// 会话 Cookie
    #[serde(rename = "session_cookie")]
    SessionCookie,
    /// 客户端证书
    #[serde(rename = "client_certificate")]
    ClientCertificate,
    /// SSH 密钥
    #[serde(rename = "ssh_key")]
    SshKey,
    /// 数据库连接
    #[serde(rename = "database_connection")]
    DatabaseConnection,
}

impl CredentialType {
    /// 获取凭证类型的字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            CredentialType::UsernamePassword => "username_password",
            CredentialType::OAuthRefresh => "oauth_token",
            CredentialType::ApiKey => "api_key",
            CredentialType::SessionCookie => "session_cookie",
            CredentialType::ClientCertificate => "client_certificate",
            CredentialType::SshKey => "ssh_key",
            CredentialType::DatabaseConnection => "database_connection",
        }
    }
}

/// 交易所 / 自定义 Provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialProvider {
    Okx,
    Binance,
    Custom,
}

impl CredentialProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            CredentialProvider::Okx => "okx",
            CredentialProvider::Binance => "binance",
            CredentialProvider::Custom => "custom",
        }
    }
}

/// 自定义模板函数定义
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialCustomFunction {
    pub function_name: String,
    #[serde(default)]
    pub function_description: Option<String>,
    pub function_body: String,
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
    /// 凭证状态 (active/expired/deleted)
    pub status: String,
    /// 是否需要运行时审批
    #[serde(default)]
    pub requires_approval: bool,
    /// 交易所 / 自定义 Provider
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<CredentialProvider>,
    /// HTTP 请求白名单域名
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_domains: Vec<String>,
    /// 自定义模板函数
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_functions: Vec<CredentialCustomFunction>,
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
            token_ttl_seconds: 7200,           // 2小时
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
        assert_eq!(json, "\"oauth_token\"");
    }

    #[test]
    fn oauth_refresh_deserializes_legacy_aliases() {
        let legacy_values = ["\"oauth_refresh\"", "\"oauth_token\"", "\"o_auth_refresh\""];

        for value in legacy_values {
            let parsed: CredentialType = serde_json::from_str(value).unwrap();
            assert_eq!(parsed, CredentialType::OAuthRefresh);
        }
    }

    #[test]
    fn removed_credential_types_do_not_deserialize() {
        for value in ["\"kyc_document\"", "\"zk_kyc_credential\""] {
            let parsed = serde_json::from_str::<CredentialType>(value);
            assert!(parsed.is_err(), "{value} should be rejected");
        }
    }

    #[test]
    fn new_credential_types_serialize_correctly() {
        // ClientCertificate
        let json = serde_json::to_string(&CredentialType::ClientCertificate).unwrap();
        assert_eq!(json, "\"client_certificate\"");

        // SshKey
        let json = serde_json::to_string(&CredentialType::SshKey).unwrap();
        assert_eq!(json, "\"ssh_key\"");

        // DatabaseConnection
        let json = serde_json::to_string(&CredentialType::DatabaseConnection).unwrap();
        assert_eq!(json, "\"database_connection\"");
    }

    #[test]
    fn new_credential_types_deserialize_correctly() {
        // ClientCertificate
        let parsed: CredentialType = serde_json::from_str("\"client_certificate\"").unwrap();
        assert_eq!(parsed, CredentialType::ClientCertificate);

        // SshKey
        let parsed: CredentialType = serde_json::from_str("\"ssh_key\"").unwrap();
        assert_eq!(parsed, CredentialType::SshKey);

        // DatabaseConnection
        let parsed: CredentialType = serde_json::from_str("\"database_connection\"").unwrap();
        assert_eq!(parsed, CredentialType::DatabaseConnection);
    }

    #[test]
    fn new_credential_types_as_str_returns_correct_value() {
        assert_eq!(
            CredentialType::ClientCertificate.as_str(),
            "client_certificate"
        );
        assert_eq!(CredentialType::SshKey.as_str(), "ssh_key");
        assert_eq!(
            CredentialType::DatabaseConnection.as_str(),
            "database_connection"
        );
    }
}
