//! Vault 后端集成测试
//!
//! 测试 HashiCorp Vault 存储后端实现
//! 需要本地运行 Vault 服务器或使用 mock

#[cfg(test)]
mod vault_client_tests {
    use vault_service::vault::client::{VaultClientError, VaultConfig, VaultCredentialData};

    fn create_test_config() -> VaultConfig {
        VaultConfig {
            addr: "http://127.0.0.1:8200".to_string(),
            token: "test-token".to_string(),
            mount_path: "secret".to_string(),
            namespace: None,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            timeout_seconds: 30,
            max_retries: 3,
        }
    }

    #[test]
    fn test_vault_config_build_path() {
        let config = create_test_config();
        let path = config.build_path("tenant_123", "cred_456");

        assert_eq!(path, "credbridge/tenant_123/cred_456");
    }

    #[test]
    fn test_vault_config_validate_success() {
        let config = create_test_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_vault_config_validate_empty_addr() {
        let mut config = create_test_config();
        config.addr = "".to_string();

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            VaultClientError::ConfigError(_)
        ));
    }

    #[test]
    fn test_vault_config_validate_empty_token() {
        let mut config = create_test_config();
        config.token = "".to_string();

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            VaultClientError::ConfigError(_)
        ));
    }

    #[test]
    fn test_vault_credential_data_creation() {
        let data = VaultCredentialData::new(
            "cred_123".to_string(),
            "tenant_456".to_string(),
            "user_hash_xyz".to_string(),
            "schwab".to_string(),
            "username_password".to_string(),
            1234567890,
            Some(1893456000),
            "base64_encrypted_payload".to_string(),
            2,
            "AES-256-GCM".to_string(),
            "HKDF-SHA-256".to_string(),
            "base64_nonce".to_string(),
            "base64_auth_tag".to_string(),
        );

        assert_eq!(data.credential_id, "cred_123");
        assert_eq!(data.tenant_id, "tenant_456");
        assert_eq!(data.user_id_hash, "user_hash_xyz");
        assert_eq!(data.service_id, "schwab");
        assert_eq!(data.credential_type, "username_password");
        assert_eq!(data.version, 2);
        assert_eq!(data.algorithm, "AES-256-GCM");
        assert_eq!(data.kdf, "HKDF-SHA-256");
        assert!(!data.is_deleted);
    }

    #[test]
    fn test_vault_credential_data_serialization() {
        let data = VaultCredentialData::new(
            "cred_123".to_string(),
            "tenant_456".to_string(),
            "user_hash".to_string(),
            "schwab".to_string(),
            "username_password".to_string(),
            1234567890,
            None,
            "base64_ciphertext".to_string(),
            2,
            "AES-256-GCM".to_string(),
            "HKDF-SHA-256".to_string(),
            "base64_nonce".to_string(),
            "base64_auth_tag".to_string(),
        );

        // 序列化
        let json = data.to_json().expect("Failed to serialize");

        // 验证 JSON 包含所有字段
        let json_str = json.to_string();
        assert!(json_str.contains("cred_123"));
        assert!(json_str.contains("tenant_456"));
        assert!(json_str.contains("AES-256-GCM"));

        // 反序列化
        let parsed = VaultCredentialData::from_json(json).expect("Failed to deserialize");
        assert_eq!(parsed.credential_id, data.credential_id);
        assert_eq!(parsed.tenant_id, data.tenant_id);
        assert_eq!(parsed.encrypted_payload, data.encrypted_payload);
    }
}

#[cfg(test)]
mod vault_backend_tests {
    use vault_service::vault::backend::{VaultHealthStatus, VaultStorageBackendBuilder};
    use vault_service::vault::client::VaultConfig;

    #[test]
    fn test_vault_health_status_is_healthy() {
        assert!(VaultHealthStatus::Healthy.is_healthy());
        assert!(!VaultHealthStatus::Unauthenticated.is_healthy());
        assert!(!VaultHealthStatus::Unhealthy("error".to_string()).is_healthy());
        assert!(!VaultHealthStatus::Timeout.is_healthy());
    }

    #[test]
    fn test_vault_storage_backend_builder_without_config() {
        let _builder = VaultStorageBackendBuilder::new();
        // 注意：由于没有配置，构建应该失败
        // 实际测试需要在异步上下文中运行
    }

    #[test]
    fn test_vault_config_from_env_missing_vars() {
        // 清除环境变量（使用 unsafe 块）
        unsafe {
            std::env::remove_var("VAULT_ADDR");
            std::env::remove_var("VAULT_TOKEN");
        }

        let result = VaultConfig::from_env();
        assert!(result.is_err());
    }
}

/// 测试模块：Vault 后端单元测试（无需 Vault 服务器）
#[cfg(test)]
mod vault_backend_unit_tests {
    use vault_service::crypto::constants;
    use vault_service::models::CredentialType;
    use vault_service::vault::client::VaultConfig;
    use vault_service::vault::{EncryptedPayload, ServiceId, TenantId, UserId, VaultEntry};

    fn create_test_config() -> VaultConfig {
        VaultConfig {
            addr: "http://127.0.0.1:8200".to_string(),
            token: "test-token".to_string(),
            mount_path: "secret".to_string(),
            namespace: None,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            timeout_seconds: 5,
            max_retries: 1,
        }
    }

    fn create_test_payload() -> EncryptedPayload {
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
    fn test_vault_config_path_construction() {
        let config = create_test_config();

        let path1 = config.build_path("tenant_abc", "cred_xyz");
        assert_eq!(path1, "credbridge/tenant_abc/cred_xyz");

        let path2 = config.build_path("tenant/with/slashes", "cred-123");
        // 注意：Vault 路径中的斜杠会创建目录结构
        assert_eq!(path2, "credbridge/tenant/with/slashes/cred-123");
    }

    #[test]
    fn test_vault_entry_creation() {
        let entry = VaultEntry::new(
            TenantId::new("test_tenant"),
            UserId::new("test_user"),
            ServiceId::new("test_service"),
            CredentialType::OAuthRefresh,
            create_test_payload(),
            Some(1893456000),
        );

        assert_eq!(entry.tenant_id.as_str(), "test_tenant");
        assert_eq!(entry.service_id.as_str(), "test_service");
        assert_eq!(entry.credential_type, CredentialType::OAuthRefresh);
    }
}
