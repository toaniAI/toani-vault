//! CredBridge SDK - 凭证管理服务
//!
//! 提供凭证的 CRUD 操作和解密功能

use crate::{
    client::CredBridgeClient,
    types::{
        CreateCredentialRequest, CreateCredentialResponse, CredentialCustomFunction,
        CredentialFilter, CredentialMetadata, CredentialProvider, CredentialType,
        DecryptCredentialRequest, DecryptCredentialResponse, DeleteCredentialResponse,
        GetCredentialResponse, ListCredentialsResponse, RequestOptions, Result,
        RollbackCredentialRequest, RollbackCredentialResponse, UpdateCredentialRequest,
        UpdateCredentialResponse, VersionDetail, VersionHistory,
    },
};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tracing::{debug, info};

/// 凭证管理服务
#[derive(Debug, Clone)]
pub struct CredentialsService {
    client: Arc<CredBridgeClient>,
}

impl CredentialsService {
    /// 创建新的凭证管理服务
    pub fn new(client: Arc<CredBridgeClient>) -> Self {
        Self { client }
    }

    /// 创建新凭证
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService, types::CredentialType};
    /// use serde_json::json;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = Arc::new(CredBridgeClient::new(
    ///     CredBridgeConfig::new("https://api.credbridge.io")
    ///         .with_token("your-api-token")
    /// )?);
    /// let credentials = CredentialsService::new(client);
    ///
    /// let mut plaintext_data = std::collections::HashMap::new();
    /// plaintext_data.insert("username".to_string(), json!("user@example.com"));
    /// plaintext_data.insert("password".to_string(), json!("secret_password"));
    ///
    /// let credential = credentials.create(
    ///     "schwab",
    ///     CredentialType::UsernamePassword,
    ///     plaintext_data,
    ///     None,
    ///     None,
    /// ).await?;
    ///
    /// println!("Created credential: {}", credential.credential_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_with_request(
        &self,
        request: CreateCredentialRequest,
        options: Option<RequestOptions>,
    ) -> Result<CreateCredentialResponse> {
        debug!(
            service_id = %request.service_id,
            credential_type = %request.credential_type,
            "Creating credential"
        );

        let response: CreateCredentialResponse = self
            .client
            .post_with_options("/credentials", request, options)
            .await?;

        info!(
            credential_id = %response.credential_id,
            "Credential created successfully"
        );

        Ok(response)
    }

    pub async fn create(
        &self,
        service_id: impl Into<String>,
        credential_type: CredentialType,
        plaintext_data: HashMap<String, Value>,
        expires_at: Option<i64>,
        options: Option<RequestOptions>,
    ) -> Result<CreateCredentialResponse> {
        let request = CreateCredentialRequest {
            service_id: service_id.into(),
            credential_type,
            plaintext_data,
            expires_at,
            provider: None,
            allowed_domains: None,
            custom_functions: None,
        };
        self.create_with_request(request, options).await
    }

    /// 创建用户名密码凭证（快捷方法）
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// let credential = credentials.create_username_password(
    ///     "schwab",
    ///     "user@example.com",
    ///     "secret_password",
    ///     None,
    ///     None,
    /// ).await?;
    ///
    /// println!("Created credential: {}", credential.credential_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_username_password(
        &self,
        service_id: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        expires_at: Option<i64>,
        options: Option<RequestOptions>,
    ) -> Result<CreateCredentialResponse> {
        let mut plaintext_data = HashMap::new();
        plaintext_data.insert("username".to_string(), Value::String(username.into()));
        plaintext_data.insert("password".to_string(), Value::String(password.into()));

        self.create(
            service_id,
            CredentialType::UsernamePassword,
            plaintext_data,
            expires_at,
            options,
        )
        .await
    }

    /// 创建 API Key 凭证（快捷方法）
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// let credential = credentials.create_api_key(
    ///     "stripe",
    ///     "sk_live_...",
    ///     Some("sk_secret_..."),
    ///     None,
    ///     None,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_api_key(
        &self,
        service_id: impl Into<String>,
        api_key: impl Into<String>,
        api_secret: Option<impl Into<String>>,
        expires_at: Option<i64>,
        options: Option<RequestOptions>,
    ) -> Result<CreateCredentialResponse> {
        let mut plaintext_data = HashMap::new();
        plaintext_data.insert("api_key".to_string(), Value::String(api_key.into()));
        if let Some(secret) = api_secret {
            let secret = secret.into();
            plaintext_data.insert("secret_key".to_string(), Value::String(secret.clone()));
            plaintext_data.insert("api_secret".to_string(), Value::String(secret));
        }

        self.create(
            service_id,
            CredentialType::ApiKey,
            plaintext_data,
            expires_at,
            options,
        )
        .await
    }

    /// 创建交易所 / 自定义 API Key 凭证（支持 provider、allowed_domains、custom_functions）
    pub async fn create_exchange_api_key(
        &self,
        service_id: impl Into<String>,
        api_key: impl Into<String>,
        secret_key: impl Into<String>,
        provider: CredentialProvider,
        options: ExchangeApiKeyOptions,
    ) -> Result<CreateCredentialResponse> {
        let mut plaintext_data = HashMap::new();
        plaintext_data.insert("api_key".to_string(), Value::String(api_key.into()));
        plaintext_data.insert("secret_key".to_string(), Value::String(secret_key.into()));
        if let Some(passphrase) = options.passphrase {
            plaintext_data.insert("passphrase".to_string(), Value::String(passphrase));
        }

        let request = CreateCredentialRequest {
            service_id: service_id.into(),
            credential_type: CredentialType::ApiKey,
            plaintext_data,
            expires_at: options.expires_at,
            provider: Some(provider),
            allowed_domains: options.allowed_domains,
            custom_functions: options.custom_functions,
        };
        self.create_with_request(request, options.request_options)
            .await
    }

    /// 创建 OAuth 刷新令牌凭证（快捷方法）
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// let credential = credentials.create_oauth_refresh(
    ///     "google",
    ///     "1//0d...",
    ///     None,
    ///     None,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_oauth_refresh(
        &self,
        service_id: impl Into<String>,
        refresh_token: impl Into<String>,
        expires_at: Option<i64>,
        options: Option<RequestOptions>,
    ) -> Result<CreateCredentialResponse> {
        let mut plaintext_data = HashMap::new();
        plaintext_data.insert(
            "refresh_token".to_string(),
            Value::String(refresh_token.into()),
        );

        self.create(
            service_id,
            CredentialType::OAuthRefresh,
            plaintext_data,
            expires_at,
            options,
        )
        .await
    }

    /// 获取凭证列表
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService, types::CredentialFilter};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// // 获取所有凭证
    /// let response = credentials.list(None, None).await?;
    ///
    /// // 按服务 ID 过滤
    /// let filter = CredentialFilter {
    ///     service_id: Some("schwab".to_string()),
    ///     ..Default::default()
    /// };
    /// let response = credentials.list(Some(filter), None).await?;
    /// println!("Current page: {}", response.page);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list(
        &self,
        filter: Option<CredentialFilter>,
        options: Option<RequestOptions>,
    ) -> Result<ListCredentialsResponse> {
        // 构建查询参数
        let mut query_params: Vec<(String, String)> = Vec::new();

        if let Some(filter) = filter {
            if let Some(service_id) = filter.service_id {
                query_params.push(("service_id".to_string(), service_id));
            }
            if let Some(credential_type) = filter.credential_type {
                query_params.push(("credential_type".to_string(), credential_type.to_string()));
            }
            if let Some(include_deleted) = filter.include_deleted {
                query_params.push(("include_deleted".to_string(), include_deleted.to_string()));
            }
            if let Some(only_valid) = filter.only_valid {
                query_params.push(("only_valid".to_string(), only_valid.to_string()));
            }
            if let Some(page) = filter.page {
                query_params.push(("page".to_string(), page.to_string()));
            }
            if let Some(page_size) = filter.page_size {
                query_params.push(("page_size".to_string(), page_size.to_string()));
            }
        }

        // 构建路径
        let path = if query_params.is_empty() {
            "/credentials".to_string()
        } else {
            let query_string: Vec<String> = query_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
                .collect();
            format!("/credentials?{}", query_string.join("&"))
        };

        debug!(path = %path, "Listing credentials");

        self.client.get_with_options(&path, options).await
    }

    /// 获取单个凭证详情
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// let credential = credentials.get("018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c", None).await?;
    /// println!("Service: {}", credential.service_id);
    /// println!("Type: {:?}", credential.credential_type);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get(
        &self,
        credential_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<GetCredentialResponse> {
        let path = format!("/credentials/{}", credential_id.as_ref());
        self.client.get_with_options(&path, options).await
    }

    /// 解密凭证
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// let decrypted = credentials.decrypt(
    ///     "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    ///     Some("用户登录操作"),
    ///     None,
    /// ).await?;
    ///
    /// println!("Username: {}", decrypted.plaintext_data.get("username").unwrap());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn decrypt(
        &self,
        credential_id: impl AsRef<str>,
        reason: Option<impl Into<String>>,
        options: Option<RequestOptions>,
    ) -> Result<DecryptCredentialResponse> {
        let path = format!("/credentials/{}/decrypt", credential_id.as_ref());
        let request = DecryptCredentialRequest {
            reason: reason.map(|r| r.into()),
        };

        debug!(
            credential_id = %credential_id.as_ref(),
            "Decrypting credential"
        );

        let response: DecryptCredentialResponse = self
            .client
            .post_with_options(&path, request, options)
            .await?;

        info!(
            credential_id = %response.credential_id,
            "Credential decrypted successfully"
        );

        Ok(response)
    }

    /// 删除凭证
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// let result = credentials.delete("018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c", None).await?;
    /// if result.deleted {
    ///     println!("Credential deleted successfully");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete(
        &self,
        credential_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<DeleteCredentialResponse> {
        let path = format!("/credentials/{}", credential_id.as_ref());

        debug!(
            credential_id = %credential_id.as_ref(),
            "Deleting credential"
        );

        let response: DeleteCredentialResponse =
            self.client.delete_with_options(&path, options).await?;

        if response.deleted {
            info!(
                credential_id = %response.credential_id,
                "Credential deleted successfully"
            );
        }

        Ok(response)
    }

    /// 更新凭证并创建新版本
    pub async fn update_with_request(
        &self,
        credential_id: impl AsRef<str>,
        request: UpdateCredentialRequest,
        options: Option<RequestOptions>,
    ) -> Result<UpdateCredentialResponse> {
        let path = format!("/credentials/{}", credential_id.as_ref());
        self.client.put_with_options(&path, request, options).await
    }

    /// 更新凭证并创建新版本
    pub async fn update(
        &self,
        credential_id: impl AsRef<str>,
        plaintext_data: Value,
        change_reason: Option<String>,
        expected_version: Option<u32>,
        options: Option<RequestOptions>,
    ) -> Result<UpdateCredentialResponse> {
        let request = UpdateCredentialRequest {
            plaintext_data,
            change_reason,
            provider: None,
            allowed_domains: None,
            custom_functions: None,
            expected_version,
        };
        self.update_with_request(credential_id, request, options)
            .await
    }

    /// 查询版本历史
    pub async fn list_versions(
        &self,
        credential_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<VersionHistory> {
        let path = format!("/credentials/{}/versions", credential_id.as_ref());
        self.client.get_with_options(&path, options).await
    }

    /// 获取指定版本详情
    pub async fn get_version(
        &self,
        credential_id: impl AsRef<str>,
        version: u32,
        options: Option<RequestOptions>,
    ) -> Result<VersionDetail> {
        let path = format!(
            "/credentials/{}/versions/{}",
            credential_id.as_ref(),
            version
        );
        self.client.get_with_options(&path, options).await
    }

    /// 回滚到指定版本
    pub async fn rollback(
        &self,
        credential_id: impl AsRef<str>,
        target_version: u32,
        reason: impl Into<String>,
        options: Option<RequestOptions>,
    ) -> Result<RollbackCredentialResponse> {
        let path = format!("/credentials/{}/rollback", credential_id.as_ref());
        let request = RollbackCredentialRequest {
            target_version,
            reason: reason.into(),
        };
        self.client.post_with_options(&path, request, options).await
    }

    /// 获取指定服务的所有凭证
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// let (credential_list, total) = credentials.get_by_service("schwab", None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_by_service(
        &self,
        service_id: impl Into<String>,
        options: Option<RequestOptions>,
    ) -> Result<(Vec<CredentialMetadata>, u32)> {
        let filter = CredentialFilter {
            service_id: Some(service_id.into()),
            ..Default::default()
        };
        self.list(Some(filter), options).await
    }

    /// 获取指定类型的所有凭证
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService, types::CredentialType};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// let (credential_list, total) = credentials.get_by_type(CredentialType::ApiKey, None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_by_type(
        &self,
        credential_type: CredentialType,
        options: Option<RequestOptions>,
    ) -> Result<(Vec<CredentialMetadata>, u32)> {
        let filter = CredentialFilter {
            credential_type: Some(credential_type),
            ..Default::default()
        };
        self.list(Some(filter), options).await
    }

    /// 检查凭证是否存在
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient, credentials::CredentialsService};
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Arc::new(CredBridgeClient::new(
    /// #     CredBridgeConfig::new("https://api.credbridge.io")
    /// #         .with_token("your-api-token")
    /// # )?);
    /// # let credentials = CredentialsService::new(client);
    /// let exists = credentials.exists("018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c", None).await?;
    /// println!("Exists: {}", exists);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn exists(
        &self,
        credential_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<bool> {
        let options = options.unwrap_or_default().with_skip_retry(true);

        match self.get(credential_id, Some(options)).await {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.code == crate::types::CredBridgeErrorCode::NotFound {
                    Ok(false)
                } else {
                    Err(e)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExchangeApiKeyOptions {
    pub expires_at: Option<i64>,
    pub passphrase: Option<String>,
    pub allowed_domains: Option<Vec<String>>,
    pub custom_functions: Option<Vec<CredentialCustomFunction>>,
    pub request_options: Option<RequestOptions>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credential_filter_default() {
        let filter = CredentialFilter::default();
        assert!(filter.service_id.is_none());
        assert!(filter.credential_type.is_none());
        assert!(filter.include_deleted.is_none());
        assert!(filter.only_valid.is_none());
    }
}
