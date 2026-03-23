//! CredBridge Vault 模块
//!
//! 凭证数据模型与存储实现
//!
//! ## 核心功能
//!
//! - **凭证数据模型**: `VaultEntry` - 支持 UUID v7 时间排序 ID
//! - **多租户隔离**: Schema-per-Tenant + 行级安全（RLS）
//! - **加密载荷**: AES-256-GCM 加密数据存储
//! - **存储后端**: 内存存储（开发/测试）+ HashiCorp Vault 集成
//!
//! ## 快速开始
//!
//! ### 使用内存存储（开发/测试）
//!
//! ```rust,ignore
//! use vault_service::vault::{CredentialVault, CredentialId, TenantId, UserId};
//! use vault_service::vault::models::{CreateCredentialRequest, EncryptedPayload};
//!
//! // 创建 Vault（内存存储）
//! let vault = CredentialVault::new_in_memory();
//!
//! // 创建凭证
//! let entry = vault.create_credential(
//!     CreateCredentialRequest {
//!         tenant_id: TenantId::new("tenant_123"),
//!         user_id: UserId::new("user_456"),
//!         service_id: ServiceId::new("schwab"),
//!         credential_type: CredentialType::UsernamePassword,
//!         expires_at: None,
//!     },
//!     encrypted_payload,
//! )?;
//! ```
//!
//! ### 使用 HashiCorp Vault（生产环境）
//!
//! ```rust,ignore
//! use vault_service::vault::backend::{VaultStorageBackend, VaultConfig};
//!
//! // 配置 Vault
//! let config = VaultConfig {
//!     addr: "http://127.0.0.1:8200".to_string(),
//!     token: "your-vault-token".to_string(),
//!     mount_path: "secret".to_string(),
//!     ..Default::default()
//! };
//!
//! // 创建 Vault 存储后端
//! let backend = VaultStorageBackend::new(config).await?;
//!
//! // 创建 Vault
//! let vault = CredentialVault::with_backend(Box::new(backend));
//! ```
//!
//! ## 模块结构
//!
//! - `models`: 数据模型（`VaultEntry`, `CredentialId`, `EncryptedPayload`, ...）
//! - `storage`: 存储逻辑（`CredentialVault`, `StorageBackend`, `InMemoryStorage`）
//! - `client`: Vault 客户端封装（`VaultKvClient`, `VaultConfig`）
//! - `backend`: Vault 存储后端实现（`VaultStorageBackend`）

pub mod backend;
pub mod client;
pub mod models;
pub mod postgres;
pub mod storage;
pub mod version;

// 重新导出核心类型
pub use models::{
    CreateCredentialRequest, CredentialFilter, CredentialId, CredentialQueryResult,
    EncryptedPayload, ServiceId, TenantId, UserId, VaultEntry, VaultError,
};

pub use storage::{CredentialVault, InMemoryStorage, StorageBackend, create_credential};

// 重新导出 Vault 后端类型
pub use backend::{
    VaultBackendError, VaultHealthStatus, VaultStorageBackend, VaultStorageBackendBuilder,
    check_vault_health,
};
pub use postgres::{PostgresBackendError, PostgresStorageBackend};

pub use client::{VaultClientError, VaultConfig, VaultCredentialData, VaultKvClient};

// 重新导出版本控制类型
pub use version::{
    CredentialVersion, DiffType, MetadataChange, RollbackRequest, RollbackResponse,
    UpdateCredentialRequest, UpdateCredentialResponse, VersionDetail, VersionDiff, VersionHistory,
    VersionMetadata, VersionSummary,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::constants;
    use crate::models::CredentialType;

    /// 创建测试用的加密载荷
    fn test_payload() -> EncryptedPayload {
        EncryptedPayload::new(
            constants::PROTOCOL_VERSION,
            constants::ALGORITHM_AES_256_GCM,
            constants::KDF_HKDF_SHA256,
            vec![0u8; constants::NONCE_LENGTH],
            vec![0u8; constants::AUTH_TAG_LENGTH],
            vec![1, 2, 3, 4, 5],
        )
    }

    #[test]
    fn test_vault_module_integration() {
        // 创建 Vault
        let vault = CredentialVault::new_in_memory();

        // 创建凭证
        let entry = create_credential(
            &vault,
            "tenant_123",
            "user_456",
            "schwab",
            CredentialType::UsernamePassword,
            test_payload(),
            None,
        )
        .unwrap();

        // 验证 UUID v7 格式
        assert!(Uuid::parse_str(entry.credential_id.as_str()).is_ok());

        // 获取元数据
        let metadata = vault
            .get_credential_metadata(
                &entry.credential_id,
                &TenantId::new("tenant_123"),
                &UserId::from_hash(entry.user_id.hash()),
            )
            .unwrap()
            .unwrap();

        assert_eq!(metadata.service_id, "schwab");
        assert_eq!(metadata.credential_type, CredentialType::UsernamePassword);

        // 验证加密载荷不包含在元数据中
        //（metadata 是 CredentialMetadata 类型，没有 encrypted_payload 字段）
    }

    #[test]
    fn test_multi_tenant_isolation() {
        let vault = CredentialVault::new_in_memory();

        // 租户 A 的凭证
        let entry_a = create_credential(
            &vault,
            "tenant_a",
            "user_1",
            "service_1",
            CredentialType::UsernamePassword,
            test_payload(),
            None,
        )
        .unwrap();

        // 租户 B 的凭证
        let entry_b = create_credential(
            &vault,
            "tenant_b",
            "user_2",
            "service_2",
            CredentialType::ApiKey,
            test_payload(),
            None,
        )
        .unwrap();

        // 租户 A 只能访问自己的凭证
        let result = vault.get_credential_metadata(
            &entry_a.credential_id,
            &TenantId::new("tenant_a"),
            &UserId::from_hash(entry_a.user_id.hash()),
        );
        assert!(result.is_ok());

        // 租户 B 不能访问租户 A 的凭证
        let result = vault.get_credential_metadata(
            &entry_a.credential_id,
            &TenantId::new("tenant_b"),
            &UserId::from_hash(entry_a.user_id.hash()),
        );
        assert!(result.is_err());

        // 租户 A 不能访问租户 B 的凭证
        let result = vault.get_credential_metadata(
            &entry_b.credential_id,
            &TenantId::new("tenant_a"),
            &UserId::from_hash(entry_b.user_id.hash()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypted_payload_validation() {
        // 有效的载荷
        let valid = test_payload();
        assert!(valid.validate().is_ok());

        // 无效的版本
        let mut invalid = test_payload();
        invalid.version = 99;
        assert!(invalid.validate().is_err());
    }

    use uuid::Uuid;
}
