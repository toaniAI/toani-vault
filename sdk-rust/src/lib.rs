//! CredBridge Rust SDK
//!
//! 用于与 CredBridge Vault API 交互的 Rust SDK。
//!
//! # 特性
//!
//! - 完整的凭证管理（创建、读取、更新、删除、解密）
//! - Token 管理和自动刷新
//! - 类型安全的 API 调用
//! - 自动重试和错误处理
//! - 请求签名支持
//!
//! # 快速开始
//!
//! ```rust,no_run
//! use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK, types::CredentialType};
//! use std::collections::HashMap;
//! use serde_json::json;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 创建配置
//!     let config = CredBridgeConfig::new("https://api.credbridge.io")
//!         .with_token("your-api-token")
//!         .with_timeout_ms(30000);
//!
//!     // 创建 SDK 实例
//!     let sdk = CredBridgeSDK::new(config)?;
//!
//!     // 创建凭证
//!     let mut data = HashMap::new();
//!     data.insert("username".to_string(), json!("user@example.com"));
//!     data.insert("password".to_string(), json!("secret_password"));
//!
//!     let credential = sdk.credentials()
//!         .create("schwab", CredentialType::UsernamePassword, data, None, None)
//!         .await?;
//!
//!     println!("Created credential: {}", credential.credential_id);
//!
//!     // 解密凭证
//!     let decrypted = sdk.credentials()
//!         .decrypt(&credential.credential_id, Some("用户登录"), None)
//!         .await?;
//!
//!     println!("Username: {}", decrypted.plaintext_data.get("username").unwrap());
//!
//!     Ok(())
//! }
//! ```
//!
//! # 模块
//!
//! - [`types`] - 核心类型定义（配置、错误、请求/响应类型）
//! - [`client`] - HTTP 客户端实现
//! - [`credentials`] - 凭证管理服务
//! - [`token`] - Token 管理功能
//!
//! # 错误处理
//!
//! SDK 使用 [`CredBridgeError`] 作为统一错误类型，提供详细的错误信息：
//!
//! ```rust,no_run
//! use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK, types::CredBridgeErrorCode};
//!
//! #[tokio::main]
//! async fn main() {
//!     let sdk = CredBridgeSDK::new(
//!         CredBridgeConfig::new("https://api.credbridge.io")
//!     ).unwrap();
//!
//!     match sdk.credentials().get("invalid-id", None).await {
//!         Ok(credential) => println!("Found: {:?}", credential),
//!         Err(e) => {
//!             match e.code {
//!                 CredBridgeErrorCode::NotFound => println!("Credential not found"),
//!                 CredBridgeErrorCode::Unauthorized => println!("Not authorized"),
//!                 _ => println!("Error: {}", e),
//!             }
//!         }
//!     }
//! }
//! ```

#![doc(html_logo_url = "https://credbridge.io/logo.png")]
#![allow(missing_docs)]
#![warn(rust_2018_idioms)]

use std::sync::Arc;

// 导出子模块
pub mod client;
pub mod credentials;
pub mod audit;
pub mod sandbox;
pub mod token;
pub mod types;

// 重新导出常用类型
pub use audit::AuditService;
pub use client::CredBridgeClient;
pub use credentials::CredentialsService;
pub use sandbox::SandboxService;
pub use token::TokenManager;
pub use types::{
    AuditExportRequest, AuditExportResponse, AuditLogEntry, AuditLogFilter, AuditLogsResponse,
    AuditVerifyRequest, AuditVerifyResponse, CreateCredentialRequest,
    CreateCredentialResponse, CreateSandboxSessionRequest, CreateSandboxSessionResponse,
    CreateTokenResponse, CredBridgeConfig, CredBridgeError, CredBridgeErrorCode,
    CredentialFilter, CredentialMetadata, CredentialType, DecryptCredentialRequest,
    DecryptCredentialResponse, DeleteCredentialResponse, ExecuteSandboxOperationRequest,
    ExecuteSandboxOperationResponse, GetCredentialResponse, ListCredentialsResponse,
    ListTokensResponse, RequestOptions, Result, RollbackCredentialResponse,
    SandboxOperationDetail, SandboxSessionActionResponse, SandboxSessionDetail,
    SandboxSessionsResponse, SandboxStatsResponse, TokenInfo, TokenRefreshResult,
    TokenScope, TokenStatsResponse, UpdateCredentialResponse, VersionDetail,
    VersionHistory,
};

/// SDK 版本
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// CredBridge SDK 主入口
///
/// 这是 SDK 的高级封装，提供便捷的方法来访问各种服务。
#[derive(Debug, Clone)]
pub struct CredBridgeSDK {
    client: Arc<CredBridgeClient>,
}

impl CredBridgeSDK {
    /// 创建新的 SDK 实例
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let sdk = CredBridgeSDK::new(
    ///     CredBridgeConfig::new("https://api.credbridge.io")
    ///         .with_token("your-api-token")
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: CredBridgeConfig) -> Result<Self> {
        let client = Arc::new(CredBridgeClient::new(config)?);
        Ok(Self { client })
    }

    /// 从现有客户端创建 SDK 实例
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use credbridge_sdk::{CredBridgeConfig, CredBridgeClient, CredBridgeSDK};
    /// use std::sync::Arc;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = Arc::new(CredBridgeClient::new(
    ///     CredBridgeConfig::new("https://api.credbridge.io")
    ///         .with_token("your-api-token")
    /// )?);
    ///
    /// let sdk = CredBridgeSDK::from_client(client);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_client(client: Arc<CredBridgeClient>) -> Self {
        Self { client }
    }

    /// 获取原始客户端
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let sdk = CredBridgeSDK::new(
    ///     CredBridgeConfig::new("https://api.credbridge.io")
    /// )?;
    ///
    /// let client = sdk.client();
    /// # Ok(())
    /// # }
    /// ```
    pub fn client(&self) -> &CredBridgeClient {
        &self.client
    }

    /// 获取凭证管理服务
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let sdk = CredBridgeSDK::new(
    ///     CredBridgeConfig::new("https://api.credbridge.io")
    ///         .with_token("your-api-token")
    /// )?;
    ///
    /// // 列出所有凭证
    /// let (credentials, total) = sdk.credentials().list(None, None).await?;
    /// println!("Total credentials: {}", total);
    /// # Ok(())
    /// # }
    /// ```
    pub fn credentials(&self) -> CredentialsService {
        CredentialsService::new(Arc::clone(&self.client))
    }

    /// 获取 Token 管理器
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK, types::TokenScope};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let sdk = CredBridgeSDK::new(
    ///     CredBridgeConfig::new("https://api.credbridge.io")
    ///         .with_token("your-api-token")
    /// )?;
    ///
    /// // 检查 Token 权限
    /// if sdk.token().has_scope(TokenScope::CredentialRead) {
    ///     println!("Can read credentials");
    /// }
    ///
    /// // 检查 Token 是否即将过期
    /// if sdk.token().is_expiring_soon(300) {
    ///     println!("Token will expire soon");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn token(&self) -> TokenManager {
        TokenManager::new(Arc::clone(&self.client))
    }

    /// 获取审计日志服务
    pub fn audit(&self) -> AuditService {
        AuditService::new(Arc::clone(&self.client))
    }

    /// 获取沙箱服务
    pub fn sandbox(&self) -> SandboxService {
        SandboxService::new(Arc::clone(&self.client))
    }

    /// 获取 SDK 版本
    ///
    /// # 示例
    ///
    /// ```rust
    /// use credbridge_sdk;
    ///
    /// println!("CredBridge SDK version: {}", credbridge_sdk::version());
    /// ```
    pub fn version() -> &'static str {
        VERSION
    }
}

/// 获取 SDK 版本
///
/// # 示例
///
/// ```rust
/// use credbridge_sdk;
///
/// println!("CredBridge SDK version: {}", credbridge_sdk::version());
/// ```
pub fn version() -> &'static str {
    VERSION
}

/// 创建新的 CredBridge 客户端
///
/// 这是一个便捷函数，等同于 `CredBridgeClient::new(config)`。
///
/// # 示例
///
/// ```rust,no_run
/// use credbridge_sdk::{CredBridgeConfig, create_client};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let client = create_client(
///     CredBridgeConfig::new("https://api.credbridge.io")
///         .with_token("your-api-token")
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn create_client(config: CredBridgeConfig) -> Result<CredBridgeClient> {
    CredBridgeClient::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!version().is_empty());
        assert_eq!(version(), VERSION);
    }

    #[test]
    fn test_sdk_version() {
        assert_eq!(CredBridgeSDK::version(), VERSION);
    }
}
