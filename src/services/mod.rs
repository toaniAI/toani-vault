//! 业务服务模块
//!
//! 实现核心业务逻辑

/// 数据库服务
pub mod db;

/// LLM 服务
pub mod llm;

/// 凭证保险库服务
pub mod vault_service {
    use crate::models::{CredentialMetadata, CredentialType};

    /// 创建凭证请求
    #[derive(Debug)]
    pub struct CreateCredentialRequest {
        pub tenant_id: String,
        pub user_id: String,
        pub credential_type: CredentialType,
        pub service_id: String,
        pub plaintext: Vec<u8>,
        pub expires_at: Option<u64>,
    }

    /// 创建凭证响应
    #[derive(Debug)]
    pub struct CreateCredentialResponse {
        pub credential_id: String,
        pub metadata: CredentialMetadata,
    }

    /// 凭证服务接口
    pub trait CredentialVaultService {
        /// 创建凭证
        fn create_credential(
            &self,
            request: CreateCredentialRequest,
        ) -> impl std::future::Future<Output = Result<CreateCredentialResponse, VaultError>> + Send;

        /// 获取凭证元数据
        fn get_credential_metadata(
            &self,
            tenant_id: &str,
            credential_id: &str,
        ) -> impl std::future::Future<Output = Result<CredentialMetadata, VaultError>> + Send;

        /// 解密凭证
        fn decrypt_credential(
            &self,
            tenant_id: &str,
            user_id: &str,
            credential_id: &str,
        ) -> impl std::future::Future<Output = Result<Vec<u8>, VaultError>> + Send;
    }

    /// 保险库错误
    #[derive(Debug, thiserror::Error)]
    pub enum VaultError {
        #[error("凭证未找到")]
        CredentialNotFound,
        #[error("访问被拒绝")]
        AccessDenied,
        #[error("加密错误: {0}")]
        EncryptionError(String),
        #[error("Token 无效")]
        InvalidToken,
        #[error("Token 已过期")]
        TokenExpired,
        #[error("数据库错误: {0}")]
        DatabaseError(String),
    }
}
