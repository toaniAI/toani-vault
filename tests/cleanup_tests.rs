//! TEE 内存安全与密钥清理集成测试
//!
//! 验证以下安全特性：
//! - ZeroizeOnDrop 自动清理密钥材料
//! - TTL 缓存自动过期清理
//! - Enclave 重启后密钥恢复与 MRSIGNER 验证
//! - 密钥清理调度器后台任务

use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use vault_service::tee::{
    CacheStatistics, CachedKeyEntry, CleanupConfig, CleanupScheduler, KeyCleaner, KeyLifecycle,
    KeyManager, KeyType, MasterKeyMetadata, ProtectedKeyMaterial, ProtectedMemory, SealPolicy,
    SecureScope, UserKeyCache, DEFAULT_KEY_TTL_SECONDS, MASTER_KEY_STORAGE_ID,
};
use vault_service::crypto::constants::KEY_LENGTH;

/// 测试零自动清理特性
///
/// 验证当 CredentialKey 实例被 drop 时，
/// ZeroizeOnDrop trait 自动将 key_material 覆写为零
#[test]
fn test_zeroize_on_drop_automatic_cleanup() {
    // 创建一个受保护的密钥材料
    let material = [0x42u8; KEY_LENGTH];
    let key_type = KeyType::UserVault;

    {
        let protected = ProtectedKeyMaterial::new(material, key_type);
        assert_eq!(protected.as_bytes(), &material);
        assert_eq!(protected.key_type(), key_type);
        // 离开作用域时应该自动 zeroize
    }

    // 注意：由于 ZeroizeOnDrop 是自动的，我们无法在离开作用域后验证
    // 但在实际安全审计中，可以使用内存调试工具验证
}

/// 测试用户密钥缓存 TTL 过期自动清理
///
/// 验证当缓存项超过 TTL（5 分钟）时，
/// 自动从内存中移除并调用 zeroize 清理密钥数据
#[test]
fn test_user_key_cache_ttl_expiration_cleanup() {
    // 使用较短的 TTL 便于测试
    let ttl_seconds = 1u64;
    let mut cache = UserKeyCache::new(ttl_seconds);

    // 添加缓存条目
    let handle = [0u8; 32];
    let key_material = [0x42u8; KEY_LENGTH];
    let entry = CachedKeyEntry::new(
        handle,
        "tenant_1".to_string(),
        "user_hash_1".to_string(),
        key_material,
        KeyType::UserVault,
    );

    cache.insert("tenant_1", "user_hash_1", entry);
    assert_eq!(cache.len(), 1);

    // 立即获取应该成功
    assert!(cache.get("tenant_1", "user_hash_1").is_some());

    // 等待超过 TTL
    thread::sleep(Duration::from_secs(ttl_seconds + 1));

    // 再次获取应该失败（过期后被清理）
    assert!(cache.get("tenant_1", "user_hash_1").is_none());
    assert_eq!(cache.len(), 0);
}

/// 测试缓存统计信息
#[test]
fn test_cache_statistics_tracking() {
    let mut cache = UserKeyCache::new(60);

    // 验证初始统计
    let stats = cache.stats();
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.misses, 0);
    assert_eq!(stats.insertions, 0);

    // 添加条目
    let entry = CachedKeyEntry::new(
        [0u8; 32],
        "tenant_1".to_string(),
        "user_hash_1".to_string(),
        [0x42u8; KEY_LENGTH],
        KeyType::UserVault,
    );
    cache.insert("tenant_1", "user_hash_1", entry);
    assert_eq!(cache.stats().insertions, 1);

    // 缓存命中
    cache.get("tenant_1", "user_hash_1");
    assert_eq!(cache.stats().hits, 1);

    // 缓存未命中
    cache.get("tenant_1", "nonexistent");
    assert_eq!(cache.stats().misses, 1);
}

/// 测试 Enclave 重启后密钥恢复
///
/// 验证当新的 Enclave 实例启动时，
/// 可以从密封存储恢复 L1 主密钥
#[test]
fn test_enclave_restart_key_recovery() {
    // 使用临时目录作为密封存储
    let temp_dir = std::env::temp_dir().join("credbridge_test_recovery");
    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = std::fs::create_dir_all(&temp_dir);

    let storage_path = temp_dir.to_str().unwrap().to_string();
    let mrsigner = [0x01u8; 32];
    let mrenclave = [0x02u8; 32];

    // 第一次：创建密钥并密封
    {
        let manager = KeyManager::new(
            storage_path.clone(),
            mrsigner,
            mrenclave,
        );

        // 创建主密钥材料
        let master_key = [0xABu8; KEY_LENGTH];

        // 密封主密钥
        manager.seal_master_key(&master_key, SealPolicy::Mrsigner)
            .expect("密封主密钥失败");

        // 验证密封存储存在
        assert!(manager.has_sealed_master_key());
    }

    // 第二次：模拟 Enclave 重启，恢复密钥
    {
        let manager = KeyManager::new(
            storage_path.clone(),
            mrsigner,
            mrenclave,
        );

        // 验证密封存储仍然存在
        assert!(manager.has_sealed_master_key());

        // 恢复主密钥
        let (recovered_key, metadata) = manager.restore_master_key()
            .expect("恢复主密钥失败");

        // 验证恢复的密钥
        assert_eq!(recovered_key, [0xABu8; KEY_LENGTH]);
        assert_eq!(metadata.seal_policy, SealPolicy::Mrsigner);
        assert_eq!(metadata.sealed_mrsigner, mrsigner);
        assert_eq!(metadata.sealed_mrenclave, mrenclave);
    }

    // 清理
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 测试 MRSIGNER 兼容性验证
///
/// 验证当使用不同的 MRSIGNER 恢复密钥时应该失败
#[test]
fn test_mrsigner_compatibility_verification() {
    let temp_dir = std::env::temp_dir().join("credbridge_test_mrsigner");
    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = std::fs::create_dir_all(&temp_dir);

    let storage_path = temp_dir.to_str().unwrap().to_string();
    let mrsigner1 = [0x01u8; 32];
    let mrsigner2 = [0x02u8; 32];
    let mrenclave = [0x03u8; 32];

    // 使用 mrsigner1 创建并密封密钥
    {
        let manager = KeyManager::new(
            storage_path.clone(),
            mrsigner1,
            mrenclave,
        );

        let master_key = [0xABu8; KEY_LENGTH];
        manager.seal_master_key(&master_key, SealPolicy::Mrsigner)
            .expect("密封主密钥失败");
    }

    // 使用 mrsigner2 尝试恢复（应该失败）
    {
        let manager = KeyManager::new(
            storage_path.clone(),
            mrsigner2, // 不同的 MRSIGNER
            mrenclave,
        );

        let result = manager.restore_master_key();
        assert!(result.is_err());

        // 验证错误类型
        match result {
            Err(vault_service::tee::KeyManagerError::MrsignerMismatch) => {
                // 预期的错误
            }
            _ => panic!("期望 MrsignerMismatch 错误，但得到 {:?}", result),
        }
    }

    // 清理
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 测试 MRENCLAVE 严格模式验证
///
/// 验证在 MRENCLAVE 密封策略下，
/// 不同的 MRENCLAVE 无法恢复密钥
#[test]
fn test_mrenclave_strict_verification() {
    let temp_dir = std::env::temp_dir().join("credbridge_test_mrenclave");
    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = std::fs::create_dir_all(&temp_dir);

    let storage_path = temp_dir.to_str().unwrap().to_string();
    let mrsigner = [0x01u8; 32];
    let mrenclave1 = [0x02u8; 32];
    let mrenclave2 = [0x03u8; 32];

    // 使用 mrenclave1 创建并密封密钥
    {
        let manager = KeyManager::new(
            storage_path.clone(),
            mrsigner,
            mrenclave1,
        );

        let master_key = [0xABu8; KEY_LENGTH];
        manager.seal_master_key(&master_key, SealPolicy::Mrenclave)
            .expect("密封主密钥失败");
    }

    // 使用 mrenclave2 尝试恢复（应该失败）
    {
        let manager = KeyManager::new(
            storage_path.clone(),
            mrsigner,
            mrenclave2, // 不同的 MRENCLAVE
        );

        let result = manager.restore_master_key();
        assert!(result.is_err());

        // 验证错误类型
        match result {
            Err(vault_service::tee::KeyManagerError::MrenclaveMismatch) => {
                // 预期的错误
            }
            _ => panic!("期望 MrenclaveMismatch 错误，但得到 {:?}", result),
        }
    }

    // 清理
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 测试密钥清理调度器
///
/// 验证后台清理任务正确运行
#[test]
fn test_cleanup_scheduler_background_task() {
    let config = CleanupConfig {
        cleanup_interval: Duration::from_millis(100),
        deep_cleanup_on_drop: true,
        verify_cleanup: false,
        cleanup_thread_name: "test-cleanup".to_string(),
    };

    let mut scheduler = CleanupScheduler::new(config);
    let cache = Arc::new(RwLock::new(UserKeyCache::new(1))); // 1秒 TTL

    // 添加过期条目
    {
        let mut cache = cache.write().unwrap();
        for i in 0..3 {
            let entry = CachedKeyEntry::new(
                [i as u8; 32],
                format!("tenant_{}", i),
                format!("user_{}", i),
                [i as u8; KEY_LENGTH],
                KeyType::UserVault,
            );
            cache.insert(&format!("tenant_{}", i), &format!("user_{}", i), entry);
        }
        assert_eq!(cache.len(), 3);
    }

    // 启动调度器
    scheduler.start(cache.clone());
    assert!(scheduler.is_running());

    // 等待条目过期并被清理
    thread::sleep(Duration::from_secs(3));

    // 停止调度器
    scheduler.stop();
    assert!(!scheduler.is_running());

    // 验证清理统计
    let stats = scheduler.stats();
    assert!(stats.total_cleanups > 0);

    // 验证缓存已被清理
    let cache = cache.read().unwrap();
    assert!(cache.is_empty());
}

/// 测试密钥清理工具函数
#[test]
fn test_key_cleaner_utilities() {
    // 测试 clear_bytes
    let mut data = vec![0x42u8; 32];
    KeyCleaner::clear_bytes(&mut data);
    assert!(data.iter().all(|&b| b == 0));

    // 测试 clear_key_material
    let mut key = [0x42u8; KEY_LENGTH];
    KeyCleaner::clear_key_material(&mut key);
    assert!(key.iter().all(|&b| b == 0));

    // 测试 deep_clear
    let mut deep_data = vec![0x42u8; 64];
    KeyCleaner::deep_clear(&mut deep_data);
    assert!(deep_data.iter().all(|&b| b == 0));

    // 测试 clear_and_verify
    let mut verify_data = vec![0x42u8; 16];
    let result = KeyCleaner::clear_and_verify(&mut verify_data);
    assert!(result);
    assert!(verify_data.iter().all(|&b| b == 0));
}

/// 测试受保护内存自动清理
#[test]
fn test_protected_memory_cleanup() {
    let mut memory = ProtectedMemory::new(64, "test_memory");

    // 填充敏感数据
    memory.as_mut_slice().fill(0x42);
    assert!(memory.as_slice().iter().all(|&b| b == 0x42));
    assert!(!memory.is_cleared());

    // 手动清理
    memory.secure_clear();
    assert!(memory.is_cleared());

    // 再次清理不应该出错（幂等）
    memory.secure_clear();
}

/// 测试安全作用域自动清理
#[test]
fn test_secure_scope_cleanup() {
    let sensitive_data = vec![0x42u8; 32];

    {
        let scope = SecureScope::new(sensitive_data);
        assert!(scope.get().is_some());
        assert_eq!(scope.get().unwrap().len(), 32);
        // 离开作用域时自动清理
    }

    // 手动清理版本
    let data2 = vec![0xABu8; 16];
    let scope2 = SecureScope::new(data2);
    scope2.clear();
}

/// 测试密钥生命周期管理
#[test]
fn test_key_lifecycle_management() {
    let mut lifecycle = KeyLifecycle::new(Some(Duration::from_millis(500)));

    assert!(!lifecycle.is_expired());
    assert_eq!(lifecycle.use_count, 0);
    assert!(lifecycle.expires_at.is_some());

    // 记录使用
    lifecycle.record_use();
    assert_eq!(lifecycle.use_count, 1);

    // 等待过期
    thread::sleep(Duration::from_millis(600));
    assert!(lifecycle.is_expired());

    // 标记清理
    lifecycle.mark_cleared();
    assert!(lifecycle.is_cleared);

    // 验证存活时间和闲置时间
    assert!(lifecycle.age() >= Duration::from_millis(500));
    assert!(lifecycle.idle_time() >= Duration::from_millis(500));
}

/// 测试主密钥元数据序列化
#[test]
fn test_master_key_metadata_serialization() {
    let mrsigner = [0x01u8; 32];
    let mrenclave = [0x02u8; 32];
    let metadata = MasterKeyMetadata::new(
        SealPolicy::Mrsigner,
        mrsigner,
        mrenclave,
        1,
    );

    // 序列化
    let bytes = metadata.to_bytes();
    assert_eq!(bytes.len(), 77); // 4 + 1 + 32 + 32 + 8

    // 反序列化
    let restored = MasterKeyMetadata::from_bytes(&bytes)
        .expect("反序列化失败");

    assert_eq!(restored.seal_policy, metadata.seal_policy);
    assert_eq!(restored.sealed_mrsigner, metadata.sealed_mrsigner);
    assert_eq!(restored.sealed_mrenclave, metadata.sealed_mrenclave);
    assert_eq!(restored.version, metadata.version);
}

/// 测试默认 TTL 常量
#[test]
fn test_default_ttl_constant() {
    assert_eq!(DEFAULT_KEY_TTL_SECONDS, 300); // 5 分钟
}

/// 测试主密钥存储标识
#[test]
fn test_master_key_storage_id() {
    assert_eq!(MASTER_KEY_STORAGE_ID, "enclave_master_key");
}

/// 测试缓存清空功能
#[test]
fn test_cache_clear_all_entries() {
    let mut cache = UserKeyCache::new(60);

    // 添加多个条目
    for i in 0..5 {
        let entry = CachedKeyEntry::new(
            [i as u8; 32],
            format!("tenant_{}", i),
            format!("user_{}", i),
            [i as u8; KEY_LENGTH],
            KeyType::UserVault,
        );
        cache.insert(&format!("tenant_{}", i), &format!("user_{}", i), entry);
    }

    assert_eq!(cache.len(), 5);

    // 清空所有条目
    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.stats().clears, 1);
}

/// 测试 MRSIGNER 模式的兼容性验证
#[test]
fn test_mrsigner_mode_compatibility() {
    let mrsigner = [0x01u8; 32];
    let mrenclave1 = [0x02u8; 32];
    let mrenclave2 = [0x03u8; 32];

    let metadata = MasterKeyMetadata::new(
        SealPolicy::Mrsigner,
        mrsigner,
        mrenclave1,
        1,
    );

    // MRSIGNER 模式：相同签名者，不同 MRENCLAVE 应该通过
    assert!(metadata.verify_compatibility(&mrsigner, &mrenclave2).is_ok());

    // MRSIGNER 模式：不同签名者应该失败
    let wrong_mrsigner = [0x04u8; 32];
    assert!(metadata.verify_compatibility(&wrong_mrsigner, &mrenclave1).is_err());
}

/// 测试 MRENCLAVE 模式的严格验证
#[test]
fn test_mrenclave_mode_strictness() {
    let mrsigner = [0x01u8; 32];
    let mrenclave = [0x02u8; 32];

    let metadata = MasterKeyMetadata::new(
        SealPolicy::Mrenclave,
        mrsigner,
        mrenclave,
        1,
    );

    // MRENCLAVE 模式：完全相同的测量值应该通过
    assert!(metadata.verify_compatibility(&mrsigner, &mrenclave).is_ok());

    // MRENCLAVE 模式：不同的 MRENCLAVE 应该失败
    let wrong_mrenclave = [0x03u8; 32];
    assert!(metadata.verify_compatibility(&mrsigner, &wrong_mrenclave).is_err());
}
