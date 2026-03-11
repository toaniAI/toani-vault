//! 租户配置管理测试
//!
//! 测试租户配置管理的核心功能：
//! - 租户配置创建、读取、更新、删除
//! - 功能开关管理
//! - 配额限制验证
//! - 配置管理器操作
//! - 缓存集成

use std::collections::HashMap;

use vault_service::tenant::{
    FeatureFlags, MemoryTenantConfigStore, PartialTenantConfig, QuotaLimits, TenantConfig,
    TenantConfigError, TenantConfigManager, TenantConfigStore, TenantId, TenantSettings,
};

/// 类型别名用于简化配置管理器类型
type TestConfigManager = TenantConfigManager<MemoryTenantConfigStore>;

/// 测试租户配置默认创建
#[test]
fn test_tenant_config_default() {
    let config = TenantConfig::default();

    // 验证默认功能开关
    assert!(config.feature_flags.enable_credential_encryption);
    assert!(config.feature_flags.enable_audit_logging);
    assert!(config.feature_flags.enable_token_revocation);
    assert!(!config.feature_flags.enable_mfa);
    assert!(!config.feature_flags.enable_sso);

    // 验证默认配额
    assert_eq!(config.quota_limits.max_credentials, 1000);
    assert_eq!(config.quota_limits.max_tokens_per_user, 10);
    assert_eq!(config.quota_limits.max_requests_per_minute, 1000);

    // 验证默认设置
    assert_eq!(config.settings.token_ttl_seconds, 900);
    assert_eq!(config.settings.timezone, "UTC");
    assert_eq!(config.settings.language, "zh-CN");

    // 验证版本
    assert_eq!(config.version, 1);
    assert!(config.updated_at.is_some());
}

/// 测试不同层级配置
#[test]
fn test_tenant_config_tiers() {
    // 免费版
    let free = TenantConfig::free_tier();
    assert!(!free.feature_flags.enable_mfa);
    assert!(!free.feature_flags.enable_webhooks);
    assert_eq!(free.quota_limits.max_credentials, 100);
    assert_eq!(free.quota_limits.audit_retention_days, 7);

    // 专业版
    let pro = TenantConfig::pro_tier();
    assert!(pro.feature_flags.enable_mfa);
    assert!(pro.feature_flags.enable_webhooks);
    assert_eq!(pro.quota_limits.max_credentials, 10_000);
    assert_eq!(pro.quota_limits.audit_retention_days, 90);

    // 企业版
    let enterprise = TenantConfig::enterprise_tier();
    assert!(enterprise.feature_flags.enable_sso);
    assert!(enterprise.feature_flags.enable_custom_crypto);
    assert!(enterprise.settings.require_mfa);
    assert_eq!(enterprise.quota_limits.max_credentials, 100_000);
    assert_eq!(enterprise.quota_limits.audit_retention_days, 365);
}

/// 测试功能开关操作
#[test]
fn test_feature_flags_operations() {
    let mut flags = FeatureFlags::default();

    // 测试初始状态
    assert!(flags.is_enabled("credential_encryption"));
    assert!(flags.is_enabled("audit_logging"));
    assert!(!flags.is_enabled("mfa"));
    assert!(!flags.is_enabled("unknown_feature"));

    // 测试启用功能
    flags.enable("mfa").unwrap();
    assert!(flags.is_enabled("mfa"));

    flags.enable("sso").unwrap();
    assert!(flags.is_enabled("sso"));

    // 测试禁用功能
    flags.disable("audit_logging").unwrap();
    assert!(!flags.is_enabled("audit_logging"));

    // 测试无效功能名
    assert!(flags.enable("invalid_feature").is_err());
    assert!(flags.disable("invalid_feature").is_err());
}

/// 测试功能开关全部启用
#[test]
fn test_feature_flags_enable_all() {
    let flags = FeatureFlags::enable_all_advanced();

    assert!(flags.enable_credential_encryption);
    assert!(flags.enable_audit_logging);
    assert!(flags.enable_token_revocation);
    assert!(flags.enable_mfa);
    assert!(flags.enable_remote_attestation);
    assert!(flags.enable_auto_rotation);
    assert!(flags.allow_cors);
    assert!(flags.enable_ip_whitelist);
    assert!(flags.enable_webhooks);
    assert!(flags.enable_sso);
    assert!(flags.enable_custom_crypto);
    assert!(flags.enable_advanced_audit);
}

/// 测试配额限制验证
#[test]
fn test_quota_limits_validation() {
    // 有效配额
    let valid = QuotaLimits::default();
    assert!(valid.validate().is_ok());

    // 无效配额 - 最大凭证数为0
    let invalid = QuotaLimits {
        max_credentials: 0,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    // 无效配额 - Token数为0
    let invalid = QuotaLimits {
        max_tokens_per_user: 0,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    // 无效配额 - 请求数为0
    let invalid = QuotaLimits {
        max_requests_per_minute: 0,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());
}

/// 测试不同层级配额
#[test]
fn test_quota_limits_tiers() {
    let free = QuotaLimits::free_tier();
    assert_eq!(free.max_credentials, 100);
    assert_eq!(free.max_tokens_per_user, 3);
    assert_eq!(free.max_requests_per_minute, 100);
    assert_eq!(free.storage_quota_mb, 100);

    let pro = QuotaLimits::pro_tier();
    assert_eq!(pro.max_credentials, 10_000);
    assert_eq!(pro.max_tokens_per_user, 20);
    assert_eq!(pro.max_requests_per_minute, 10_000);
    assert_eq!(pro.storage_quota_mb, 10_240);

    let enterprise = QuotaLimits::enterprise_tier();
    assert_eq!(enterprise.max_credentials, 100_000);
    assert_eq!(enterprise.max_tokens_per_user, 100);
    assert_eq!(enterprise.max_requests_per_minute, 100_000);
    assert_eq!(enterprise.storage_quota_mb, 102_400);
}

/// 测试租户设置默认值
#[test]
fn test_tenant_settings_default() {
    let settings = TenantSettings::default();

    assert_eq!(settings.token_ttl_seconds, 900); // 15分钟
    assert_eq!(settings.session_timeout_seconds, 3600); // 1小时
    assert_eq!(settings.max_login_attempts, 5);
    assert_eq!(settings.lockout_duration_seconds, 900); // 15分钟
    assert_eq!(settings.password_min_length, 8);
    assert!(settings.require_password_complexity);
    assert!(!settings.require_mfa);
    assert!(settings.allowed_callback_urls.is_empty());
    assert_eq!(settings.timezone, "UTC");
    assert_eq!(settings.language, "zh-CN");
    assert!(settings.metadata.is_empty());
}

/// 测试配置更新
#[test]
fn test_tenant_config_update() {
    let mut config = TenantConfig::default();
    let original_version = config.version;

    // 执行更新
    config.update("admin_user");

    // 验证版本递增
    assert_eq!(config.version, original_version + 1);

    // 验证更新时间
    assert!(config.updated_at.is_some());
    assert_eq!(config.updated_by, Some("admin_user".to_string()));
}

/// 测试配置合并
#[test]
fn test_tenant_config_merge() {
    let mut config = TenantConfig::default();

    // 准备部分更新
    let partial = PartialTenantConfig {
        feature_flags: Some(FeatureFlags {
            enable_mfa: true,
            ..FeatureFlags::default()
        }),
        quota_limits: Some(QuotaLimits {
            max_credentials: 5000,
            ..QuotaLimits::default()
        }),
        settings: None,
    };

    // 合并配置
    config.merge(partial, "admin");

    // 验证功能开关已更新
    assert!(config.feature_flags.enable_mfa);

    // 验证配额已更新
    assert_eq!(config.quota_limits.max_credentials, 5000);

    // 验证未提供的字段保持默认值
    assert!(!config.settings.require_mfa);

    // 验证版本递增
    assert_eq!(config.version, 2);
}

/// 测试部分配置合并 - 仅更新功能开关
#[test]
fn test_partial_config_merge_flags_only() {
    let mut config = TenantConfig::pro_tier();
    let original_max_credentials = config.quota_limits.max_credentials;

    let partial = PartialTenantConfig {
        feature_flags: Some(FeatureFlags::enable_all_advanced()),
        quota_limits: None,
        settings: None,
    };

    config.merge(partial, "admin");

    // 功能开关已更新
    assert!(config.feature_flags.enable_sso);

    // 配额未改变
    assert_eq!(config.quota_limits.max_credentials, original_max_credentials);
}

/// 测试配置验证
#[test]
fn test_tenant_config_validation() {
    let valid = TenantConfig::default();
    assert!(valid.validate().is_ok());

    let mut invalid = TenantConfig::default();
    invalid.quota_limits.max_credentials = 0;
    assert!(invalid.validate().is_err());
}

/// 测试内存配置存储 - 基本CRUD
#[tokio::test]
async fn test_memory_config_store_crud() {
    let store = MemoryTenantConfigStore::new();
    let tenant_id = TenantId::new();

    // 初始状态 - 不存在
    assert!(!store.config_exists(&tenant_id).await.unwrap());

    // 保存配置
    let config = TenantConfig::pro_tier();
    store.save_config(&tenant_id, &config).await.unwrap();

    // 现在存在
    assert!(store.config_exists(&tenant_id).await.unwrap());

    // 获取配置
    let retrieved = store.get_config(&tenant_id).await.unwrap();
    assert_eq!(retrieved.quota_limits.max_credentials, 10_000);
    assert!(retrieved.feature_flags.enable_mfa);

    // 更新配置
    let mut updated_config = config.clone();
    updated_config.update("admin");
    store.save_config(&tenant_id, &updated_config).await.unwrap();

    let retrieved = store.get_config(&tenant_id).await.unwrap();
    assert_eq!(retrieved.version, 2);

    // 删除配置
    store.delete_config(&tenant_id).await.unwrap();
    assert!(!store.config_exists(&tenant_id).await.unwrap());

    // 删除不存在的配置应返回错误
    let result = store.delete_config(&tenant_id).await;
    assert!(matches!(result, Err(TenantConfigError::NotFound(_))));
}

/// 测试内存配置存储 - 列表查询
#[tokio::test]
async fn test_memory_config_store_list() {
    let store = MemoryTenantConfigStore::new();

    // 创建多个租户配置
    let tenant1 = TenantId::new();
    let tenant2 = TenantId::new();
    let tenant3 = TenantId::new();

    store
        .save_config(&tenant1, &TenantConfig::free_tier())
        .await
        .unwrap();
    store
        .save_config(&tenant2, &TenantConfig::pro_tier())
        .await
        .unwrap();
    store
        .save_config(&tenant3, &TenantConfig::enterprise_tier())
        .await
        .unwrap();

    // 列出所有配置
    let configs = store.list_configs().await.unwrap();
    assert_eq!(configs.len(), 3);

    // 验证各租户的配置层级
    let tiers: Vec<String> = configs
        .iter()
        .map(|(_, c)| {
            if c.feature_flags.enable_sso {
                "enterprise".to_string()
            } else if c.feature_flags.enable_mfa {
                "pro".to_string()
            } else {
                "free".to_string()
            }
        })
        .collect();

    assert!(tiers.contains(&"free".to_string()));
    assert!(tiers.contains(&"pro".to_string()));
    assert!(tiers.contains(&"enterprise".to_string()));
}

/// 测试配置管理器 - 创建和获取
#[tokio::test]
async fn test_config_manager_create_and_get() {
    let store = MemoryTenantConfigStore::new();
    let manager: TestConfigManager = TenantConfigManager::new(store);
    let tenant_id = TenantId::new();

    // 创建配置
    let config = TenantConfig::pro_tier();
    let created = manager.create_config(&tenant_id, config).await.unwrap();
    assert_eq!(created.quota_limits.max_credentials, 10_000);

    // 获取配置
    let retrieved = manager.get_config(&tenant_id).await.unwrap();
    assert_eq!(retrieved.quota_limits.max_credentials, 10_000);
    assert_eq!(retrieved.version, 1);

    // 重复创建应失败
    let result = manager
        .create_config(&tenant_id, TenantConfig::default())
        .await;
    assert!(matches!(result, Err(TenantConfigError::AlreadyExists(_))));
}

/// 测试配置管理器 - 更新配置
#[tokio::test]
async fn test_config_manager_update() {
    let store = MemoryTenantConfigStore::new();
    let manager: TestConfigManager = TenantConfigManager::new(store);
    let tenant_id = TenantId::new();

    // 初始配置
    manager
        .create_config(&tenant_id, TenantConfig::free_tier())
        .await
        .unwrap();

    // 部分更新
    let partial = PartialTenantConfig {
        feature_flags: Some(FeatureFlags {
            enable_mfa: true,
            enable_webhooks: true,
            ..FeatureFlags::default()
        }),
        quota_limits: None,
        settings: None,
    };

    let updated = manager
        .update_config(&tenant_id, partial, "admin")
        .await
        .unwrap();

    // 验证更新
    assert!(updated.feature_flags.enable_mfa);
    assert!(updated.feature_flags.enable_webhooks);
    assert_eq!(updated.version, 2);
    assert_eq!(updated.updated_by, Some("admin".to_string()));

    // 更新不存在的配置应失败
    let unknown_id = TenantId::new();
    let partial = PartialTenantConfig {
        feature_flags: Some(FeatureFlags::default()),
        quota_limits: None,
        settings: None,
    };
    let result = manager.update_config(&unknown_id, partial, "admin").await;
    assert!(matches!(result, Err(TenantConfigError::NotFound(_))));
}

/// 测试配置管理器 - 验证失败
#[tokio::test]
async fn test_config_manager_validation_failure() {
    let store = MemoryTenantConfigStore::new();
    let manager: TestConfigManager = TenantConfigManager::new(store);
    let tenant_id = TenantId::new();

    // 尝试创建无效配置
    let mut invalid_config = TenantConfig::default();
    invalid_config.quota_limits.max_credentials = 0;

    let result = manager.create_config(&tenant_id, invalid_config).await;
    assert!(matches!(result, Err(TenantConfigError::ValidationError(_))));
}

/// 测试配置管理器 - 删除配置
#[tokio::test]
async fn test_config_manager_delete() {
    let store = MemoryTenantConfigStore::new();
    let manager: TestConfigManager = TenantConfigManager::new(store);
    let tenant_id = TenantId::new();

    // 创建配置
    manager
        .create_config(&tenant_id, TenantConfig::default())
        .await
        .unwrap();

    assert!(manager.config_exists(&tenant_id).await.unwrap());

    // 删除配置
    manager.delete_config(&tenant_id).await.unwrap();

    assert!(!manager.config_exists(&tenant_id).await.unwrap());

    // 获取已删除的配置应失败
    let result = manager.get_config(&tenant_id).await;
    assert!(matches!(result, Err(TenantConfigError::NotFound(_))));
}

/// 测试租户ID类型
#[test]
fn test_tenant_id_type() {
    // 创建新ID
    let id1 = TenantId::new();
    let id2 = TenantId::new();

    // ID应该是唯一的
    assert_ne!(id1.as_str(), id2.as_str());

    // 从字符串创建
    let id3 = TenantId::from("custom-id");
    assert_eq!(id3.as_str(), "custom-id");

    // 从String创建
    let id4 = TenantId::from_string("string-id".to_string());
    assert_eq!(id4.as_str(), "string-id");

    // Display trait
    let display = format!("{}", id1);
    assert!(!display.is_empty());
}

/// 测试租户状态转换
#[test]
fn test_tenant_status() {
    use vault_service::tenant::TenantStatus;

    // Pending状态
    let pending = TenantStatus::Pending;
    assert!(!pending.allows_operations());
    assert!(pending.allows_config_changes());

    // Active状态
    let active = TenantStatus::Active;
    assert!(active.allows_operations());
    assert!(active.allows_config_changes());

    // Suspended状态
    let suspended = TenantStatus::Suspended;
    assert!(!suspended.allows_operations());
    assert!(!suspended.allows_config_changes());

    // Deleted状态
    let deleted = TenantStatus::Deleted;
    assert!(!deleted.allows_operations());
    assert!(!deleted.allows_config_changes());

    // 字符串表示
    assert_eq!(pending.to_string(), "pending");
    assert_eq!(active.to_string(), "active");
    assert_eq!(suspended.to_string(), "suspended");
    assert_eq!(deleted.to_string(), "deleted");
}

/// 测试序列化和反序列化
#[test]
fn test_config_serialization() {
    let config = TenantConfig::pro_tier();

    // 序列化
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("enable_mfa"));
    assert!(json.contains("max_credentials"));

    // 反序列化
    let deserialized: TenantConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.quota_limits.max_credentials,
        config.quota_limits.max_credentials
    );
    assert_eq!(
        deserialized.feature_flags.enable_mfa,
        config.feature_flags.enable_mfa
    );
}

/// 测试带元数据的租户设置
#[test]
fn test_tenant_settings_with_metadata() {
    let mut settings = TenantSettings::default();
    settings.metadata.insert(
        "custom_key".to_string(),
        "custom_value".to_string(),
    );

    assert_eq!(settings.metadata.get("custom_key"), Some(&"custom_value".to_string()));

    // 序列化保留元数据
    let json = serde_json::to_string(&settings).unwrap();
    let deserialized: TenantSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.metadata.get("custom_key"),
        Some(&"custom_value".to_string())
    );
}

/// 测试允许的回调URL
#[test]
fn test_tenant_settings_callback_urls() {
    let mut settings = TenantSettings::default();
    settings.allowed_callback_urls = vec![
        "https://app1.example.com/callback".to_string(),
        "https://app2.example.com/callback".to_string(),
    ];

    assert_eq!(settings.allowed_callback_urls.len(), 2);
    assert!(settings
        .allowed_callback_urls
        .contains(&"https://app1.example.com/callback".to_string()));
}
