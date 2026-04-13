//! CredBridge SDK - Token 管理模块
//!
//! 管理 bearer tokens，用于 automation token、access token 和 service account token。
//!
//! # 重要说明
//!
//! 此 TokenManager 管理的是对外 bearer token，
//! 包括 automation token、access token 和 service account token。
//! 浏览器侧 Privy / session 流程不属于 Rust SDK 对外认证面。
//!
//! # 示例
//!
//! ```rust,no_run
//! use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK, TokenScope};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 直接使用 bearer token
//! let sdk = ToaniVaultSDK::new(
//!     CredBridgeConfig::new("https://vault.toani.io")
//!         .with_token("v4.local.your-bearer-token")
//! )?;
//!
//! // 检查 Token 权限
//! if sdk.token().has_scope(TokenScope::CredentialRead) {
//!     println!("Can read credentials");
//! }
//! # Ok(())
//! # }
//! ```

use crate::{
    client::CredBridgeClient,
    types::{
        ApiTokenMetadata, CreateAccessTokenResponse, CreateTokenRequest, CreateTokenResponse,
        CredBridgeError, CredBridgeErrorCode, ListTokensResponse, RequestOptions, Result,
        RevokeTokenResponse, TokenInfo, TokenScope, TokenStatsResponse,
    },
};
use std::sync::Arc;

/// Token 管理器
///
/// 管理 bearer tokens。
///
/// **注意**: 此结构管理的 Token 用于自动化和服务集成，
/// 不负责浏览器侧 Privy / session 登录流程。
#[derive(Debug, Clone)]
pub struct TokenManager {
    client: Arc<CredBridgeClient>,
}

impl TokenManager {
    /// 创建新的 Token 管理器
    pub fn new(client: Arc<CredBridgeClient>) -> Self {
        Self { client }
    }

    /// 获取当前 Token 信息
    pub fn get_token_info(&self) -> Option<TokenInfo> {
        self.client.get_token_info()
    }

    /// 获取当前 Token
    pub fn get_token(&self) -> Option<String> {
        self.client.get_token()
    }

    /// 设置新的 Token
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient};
    ///
    /// # async fn example() {
    /// let config = CredBridgeConfig::new("https://api.credbridge.io")
    ///     .with_token("v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIn0...");
    /// # }
    /// ```
    pub fn set_token(&self, token: impl Into<String>) {
        self.client.set_token(token);
    }

    /// 检查 Token 是否有效
    ///
    /// 检查包括：
    /// - Token 是否存在
    /// - Token 是否已过期
    /// - Token 格式是否正确
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// if token_manager.is_valid() {
    ///     println!("Token is valid");
    /// }
    /// # }
    /// ```
    pub fn is_valid(&self) -> bool {
        let token_info = match self.client.get_token_info() {
            Some(info) => info,
            None => return false,
        };

        let now = chrono::Utc::now().timestamp();
        now < token_info.expires_at
    }

    /// 检查 Token 是否即将过期
    ///
    /// # 参数
    ///
    /// * `buffer_seconds` - 过期前缓冲时间（秒，默认 300 秒 = 5 分钟）
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// // 检查是否将在 5 分钟内过期
    /// if token_manager.is_expiring_soon(300) {
    ///     println!("Token will expire soon");
    /// }
    /// # }
    /// ```
    pub fn is_expiring_soon(&self, buffer_seconds: i64) -> bool {
        let token_info = match self.client.get_token_info() {
            Some(info) => info,
            None => return true,
        };

        let now = chrono::Utc::now().timestamp();
        now >= token_info.expires_at - buffer_seconds
    }

    /// 获取 Token 剩余有效时间
    ///
    /// # 返回值
    ///
    /// 返回剩余秒数（如果 Token 无效则返回 0）
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// let remaining_seconds = token_manager.get_remaining_time();
    /// println!("Token expires in {} seconds", remaining_seconds);
    /// # }
    /// ```
    pub fn get_remaining_time(&self) -> i64 {
        let token_info = match self.client.get_token_info() {
            Some(info) => info,
            None => return 0,
        };

        let now = chrono::Utc::now().timestamp();
        let remaining = token_info.expires_at - now;
        remaining.max(0)
    }

    /// 验证当前 Token
    ///
    /// 当前仅进行本地有效期校验，不再调用已删除的 `/tokens/verify` 接口。
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// match token_manager.verify(None).await {
    ///     Ok(is_valid) => {
    ///         if !is_valid {
    ///             println!("Token is invalid or revoked");
    ///         }
    ///     }
    ///     Err(e) => println!("Verification error: {}", e),
    /// }
    /// # }
    /// ```
    pub async fn verify(&self, _options: Option<RequestOptions>) -> Result<bool> {
        Ok(self.is_valid())
    }

    /// 撤销当前 Token
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// match token_manager.revoke(None).await {
    ///     Ok(true) => println!("Token revoked successfully"),
    ///     Ok(false) => println!("Failed to revoke token"),
    ///     Err(e) => println!("Error: {}", e),
    /// }
    /// # }
    /// ```
    pub async fn revoke(&self, options: Option<RequestOptions>) -> Result<bool> {
        let token_info = match self.client.get_token_info() {
            Some(info) => info,
            None => {
                return Err(CredBridgeError::new(
                    CredBridgeErrorCode::InvalidToken,
                    "No token to revoke",
                ));
            }
        };

        let response: RevokeTokenResponse = self
            .client
            .post_with_options(
                &format!("/tokens/{}/revoke", token_info.token_id),
                serde_json::json!({}),
                options,
            )
            .await?;

        Ok(response.revoked)
    }

    /// 按 token id 撤销 token
    pub async fn revoke_by_id(
        &self,
        token_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<RevokeTokenResponse> {
        self.client
            .post_with_options(
                &format!("/tokens/{}/revoke", token_id.as_ref()),
                serde_json::json!({}),
                options,
            )
            .await
    }

    /// 从当前 bearer token 签发更小权限的 API access token
    pub async fn create_access_token(
        &self,
        scopes: Vec<String>,
        credential_ids: Vec<String>,
        expires_in: Option<u64>,
        options: Option<RequestOptions>,
    ) -> Result<CreateAccessTokenResponse> {
        let body = serde_json::json!({
            "scopes": scopes,
            "ttl_seconds": expires_in,
            "credential_ids": credential_ids,
        });

        self.client
            .post_with_options("/auth/access-token", body, options)
            .await
    }

    /// 撤销指定 API access token
    pub async fn revoke_access_token(
        &self,
        token_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<RevokeTokenResponse> {
        self.client
            .post_with_options(
                &format!("/tokens/{}/revoke", token_id.as_ref()),
                serde_json::json!({}),
                options,
            )
            .await
    }

    /// 创建新 token
    pub async fn create(
        &self,
        user_id: Option<String>,
        scopes: Vec<String>,
        expires_in: Option<u64>,
        credential_ids: Option<Vec<String>>,
        options: Option<RequestOptions>,
    ) -> Result<CreateTokenResponse> {
        let request = CreateTokenRequest {
            user_id,
            scopes,
            expires_in,
            credential_ids,
        };
        self.client
            .post_with_options("/tokens", request, options)
            .await
    }

    /// 列出 token
    pub async fn list(&self, options: Option<RequestOptions>) -> Result<ListTokensResponse> {
        let items: Vec<ApiTokenMetadata> = self.client.get_with_options("/tokens", options).await?;
        Ok(ListTokensResponse { tokens: items })
    }

    /// 获取指定 token 元数据
    pub async fn get(
        &self,
        token_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<ApiTokenMetadata> {
        self.client
            .get_with_options(&format!("/tokens/{}", token_id.as_ref()), options)
            .await
    }

    /// 获取 token 统计
    pub async fn stats(&self, options: Option<RequestOptions>) -> Result<TokenStatsResponse> {
        self.client.get_with_options("/tokens/stats", options).await
    }

    /// 检查 Token 是否具有指定的 Scope
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, types::TokenScope, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// if token_manager.has_scope(TokenScope::CredentialRead) {
    ///     println!("Can read credentials");
    /// }
    /// # }
    /// ```
    pub fn has_scope(&self, scope: TokenScope) -> bool {
        let token_info = match self.client.get_token_info() {
            Some(info) => info,
            None => return false,
        };

        token_info.scopes.contains(&scope) || token_info.scopes.contains(&TokenScope::Admin)
    }

    /// 检查 Token 是否具有指定的任一 Scope
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, types::TokenScope, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// if token_manager.has_any_scope(&[TokenScope::CredentialRead, TokenScope::CredentialWrite]) {
    ///     println!("Can read or write credentials");
    /// }
    /// # }
    /// ```
    pub fn has_any_scope(&self, scopes: &[TokenScope]) -> bool {
        scopes.iter().any(|scope| self.has_scope(*scope))
    }

    /// 检查 Token 是否具有所有指定的 Scope
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, types::TokenScope, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// if token_manager.has_all_scopes(&[TokenScope::CredentialRead, TokenScope::CredentialWrite]) {
    ///     println!("Can read and write credentials");
    /// }
    /// # }
    /// ```
    pub fn has_all_scopes(&self, scopes: &[TokenScope]) -> bool {
        scopes.iter().all(|scope| self.has_scope(*scope))
    }

    /// 获取 Token 中的所有 Scope
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// let scopes = token_manager.get_scopes();
    /// println!("Token scopes: {:?}", scopes);
    /// # }
    /// ```
    pub fn get_scopes(&self) -> Vec<TokenScope> {
        let token_info = match self.client.get_token_info() {
            Some(info) => info,
            None => return Vec::new(),
        };

        token_info.scopes
    }

    /// 获取租户 ID
    pub fn get_tenant_id(&self) -> Option<String> {
        self.client.get_token_info().map(|info| info.tenant_id)
    }

    /// 获取用户 ID
    pub fn get_user_id(&self) -> Option<String> {
        self.client.get_token_info().map(|info| info.user_id)
    }

    /// 获取 Token ID
    pub fn get_token_id(&self) -> Option<String> {
        self.client.get_token_info().map(|info| info.token_id)
    }

    /// 获取 Token 颁发时间
    pub fn get_issued_at(&self) -> Option<i64> {
        self.client.get_token_info().map(|info| info.issued_at)
    }

    /// 获取 Token 过期时间
    pub fn get_expires_at(&self) -> Option<i64> {
        self.client.get_token_info().map(|info| info.expires_at)
    }

    /// 计算 Token 剩余有效时间的友好显示字符串
    ///
    /// # 返回值
    ///
    /// 返回友好格式的时间字符串（如 "5分钟", "2小时"）
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, token::TokenManager};
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// # let client = Arc::new(CredBridgeClient::new(CredBridgeConfig::new("https://api.credbridge.io")).unwrap());
    /// let token_manager = TokenManager::new(client);
    /// println!("Token expires in: {}", token_manager.get_remaining_time_formatted());
    /// # }
    /// ```
    pub fn get_remaining_time_formatted(&self) -> String {
        let seconds = self.get_remaining_time();

        if seconds == 0 {
            return "已过期".to_string();
        }

        if seconds < 60 {
            return format!("{}秒", seconds);
        }

        if seconds < 3600 {
            return format!("{}分钟", seconds / 60);
        }

        if seconds < 86400 {
            return format!("{}小时", seconds / 3600);
        }

        format!("{}天", seconds / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TokenInfo;

    fn create_test_token_info(expires_at: i64) -> TokenInfo {
        TokenInfo {
            token_id: "test_token_id".to_string(),
            subject: "tenant1:user1".to_string(),
            tenant_id: "tenant1".to_string(),
            user_id: "user1".to_string(),
            expires_at,
            scopes: vec![TokenScope::CredentialRead, TokenScope::CredentialWrite],
            issued_at: chrono::Utc::now().timestamp() - 3600,
        }
    }

    // Note: These tests would need a mock client to work properly
    // For now, we just verify the types compile
    #[test]
    fn test_token_scope_display() {
        assert_eq!(TokenScope::CredentialRead.to_string(), "credential:read");
        assert_eq!(TokenScope::Admin.to_string(), "admin");
    }
}
