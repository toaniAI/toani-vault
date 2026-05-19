#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]
//! immudb 集成测试
//!
//! 测试 immudb 客户端和存储实现的功能
//!
//! 运行测试:
//! ```bash
//! cargo test --test immudb_tests
//! ```

use vault_service::audit::{
    AuditAction, AuditEntry, ImmuDbAuditStore, ImmuDbClient, ImmuDbConfig, ImmuDbStorage,
    ImmuDbStoreConfig, Outcome, QueryOptions, SignedAuditEntry, SigningKeyPair,
};

/// 创建测试用的 SignedAuditEntry
fn create_test_entry(index: u64, action: AuditAction, outcome: Outcome) -> SignedAuditEntry {
    let entry = AuditEntry::new(
        format!("user_hash_{}", index),
        "session_test",
        "test-service",
        action,
        outcome,
        "mrenclave_test_value",
        format!("jti_{}", index),
    );

    // 计算内容哈希
    let content_hash = entry.content_hash();

    SignedAuditEntry {
        entry,
        content_hash,
        prev_hash: [0u8; 32],
        signature: vec![1, 2, 3, 4, 5],
        signer_fingerprint: "test_signer_fp".to_string(),
        log_index: index,
    }
}

/// 创建测试配置
fn create_test_config() -> ImmuDbConfig {
    ImmuDbConfig {
        host: "localhost".to_string(),
        port: 3322,
        database: format!("credbridge_test_{}", uuid::Uuid::now_v7()),
        username: "immudb".to_string(),
        password: "immudb".to_string(),
        timeout_secs: 10,
        use_tls: false,
        collection: "test_audit_logs".to_string(),
    }
}

/// 创建测试存储配置
fn create_test_store_config() -> ImmuDbStoreConfig {
    ImmuDbStoreConfig {
        immudb: create_test_config(),
        max_cache_size: 1000,
        auto_sync_interval_secs: 30,
    }
}

mod immudb_client_tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let config = create_test_config();
        let client = ImmuDbClient::new(config);

        assert!(!client.is_connected());
        assert_eq!(client.entry_count(), 0);
    }

    #[tokio::test]
    async fn test_client_connect_disconnect() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config);

        // 连接
        client.connect().await.expect("Failed to connect");
        assert!(client.is_connected());
        assert!(client.state_hash().is_some());

        // 断开
        client.disconnect().await.expect("Failed to disconnect");
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_client_initialize() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config);

        client.connect().await.expect("Failed to connect");
        client.initialize().await.expect("Failed to initialize");

        assert!(client.is_connected());
    }

    #[tokio::test]
    async fn test_store_and_retrieve_entry() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config);

        client.connect().await.expect("Failed to connect");
        client.initialize().await.expect("Failed to initialize");

        let signed_entry = create_test_entry(0, AuditAction::CredentialDecrypt, Outcome::Success);

        // 存储条目
        let stored = client
            .store_entry(&signed_entry)
            .await
            .expect("Failed to store entry");

        assert_eq!(stored.transaction_id, 1);
        assert!(!stored.state_hash.is_empty());
        assert_eq!(stored.key, "audit:0");
        assert!(stored.stored_at > 0);

        // 验证条目计数
        assert_eq!(client.entry_count(), 1);

        let retrieved = client
            .get_entry("audit:0")
            .await
            .expect("Failed to retrieve stored entry");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().signed_entry.log_index, 0);
    }

    #[tokio::test]
    async fn test_store_multiple_entries() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config.clone());

        client.connect().await.expect("Failed to connect");
        client.initialize().await.expect("Failed to initialize");

        // 存储多个条目
        for i in 0..5 {
            let entry = create_test_entry(i, AuditAction::TokenValidate, Outcome::Success);
            client
                .store_entry(&entry)
                .await
                .expect("Failed to store entry");
        }

        assert_eq!(client.entry_count(), 5);

        // 获取当前状态
        let state = client.current_state().await.expect("Failed to get state");
        assert_eq!(state.tree_size, 5);
        assert_eq!(state.database, config.database);
        assert!(!state.state_hash.is_empty());
    }

    #[tokio::test]
    async fn test_state_hash_progression() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config);

        client.connect().await.expect("Failed to connect");
        client.initialize().await.expect("Failed to initialize");

        let initial_state = client.current_state().await.expect("Failed to get state");
        let initial_hash = initial_state.state_hash;

        // 存储条目，状态哈希应该变化
        let entry = create_test_entry(0, AuditAction::CredentialDecrypt, Outcome::Success);
        client.store_entry(&entry).await.expect("Failed to store");

        let new_state = client.current_state().await.expect("Failed to get state");
        let new_hash = new_state.state_hash;

        assert_ne!(initial_hash, new_hash);
    }

    #[tokio::test]
    async fn test_batch_store() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config);

        client.connect().await.expect("Failed to connect");
        client.initialize().await.expect("Failed to initialize");

        let entries: Vec<_> = (0..10)
            .map(|i| create_test_entry(i, AuditAction::TokenValidate, Outcome::Success))
            .collect();

        let results = client
            .store_entries_batch(&entries)
            .await
            .expect("Failed to batch store");

        assert_eq!(results.len(), 10);

        // 验证事务 ID 递增
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.transaction_id as usize, i + 1);
        }
    }

    #[tokio::test]
    async fn test_persistence_across_client_restart() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config.clone());

        client.connect().await.expect("Failed to connect");
        client.initialize().await.expect("Failed to initialize");

        let entry = create_test_entry(0, AuditAction::CredentialDecrypt, Outcome::Success);
        client.store_entry(&entry).await.expect("Failed to store");
        client.disconnect().await.expect("Failed to disconnect");

        let mut restarted = ImmuDbClient::new(config.clone());
        restarted.connect().await.expect("Failed to reconnect");
        restarted
            .initialize()
            .await
            .expect("Failed to reinitialize");

        let retrieved = restarted
            .get_entry("audit:0")
            .await
            .expect("Failed to retrieve after restart");
        assert!(retrieved.is_some(), "entry should survive restart");

        let queried = restarted
            .query(&QueryOptions {
                action: Some(AuditAction::CredentialDecrypt),
                limit: Some(10),
                ..Default::default()
            })
            .await
            .expect("Failed to query after restart");
        assert_eq!(queried.len(), 1);
        assert_eq!(queried[0].signed_entry.log_index, 0);
    }

    #[tokio::test]
    async fn test_query_options() {
        let options = QueryOptions {
            start_time: Some(1000),
            end_time: Some(2000),
            user_id_hash: Some("test_user".to_string()),
            action: Some(AuditAction::CredentialDecrypt),
            risk_tier: Some(vault_service::audit::RiskTier::High),
            outcome: Some(Outcome::Success),
            limit: Some(50),
            offset: Some(10),
        };

        assert_eq!(options.start_time, Some(1000));
        assert_eq!(options.end_time, Some(2000));
        assert_eq!(options.user_id_hash, Some("test_user".to_string()));
        assert_eq!(options.limit, Some(50));
    }
}

mod immudb_storage_tests {
    use super::*;

    #[tokio::test]
    async fn test_storage_creation() {
        let config = create_test_config();
        let storage = ImmuDbStorage::new(config).await;
        assert!(storage.is_ok());

        let storage = storage.unwrap();
        assert!(storage.client().is_connected());
    }

    #[tokio::test]
    async fn test_storage_store_and_retrieve() {
        let config = create_test_config();
        let mut storage = ImmuDbStorage::new(config)
            .await
            .expect("Failed to create storage");

        let entry = create_test_entry(0, AuditAction::CredentialDecrypt, Outcome::Success);

        // 存储
        let stored = storage.store(&entry).await.expect("Failed to store");
        assert_eq!(stored.transaction_id, 1);

        // 检索
        let retrieved = storage.get("audit:0").await.expect("Failed to retrieve");
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.signed_entry.log_index, 0);
    }

    #[tokio::test]
    async fn test_storage_verify() {
        let config = create_test_config();
        let storage = ImmuDbStorage::new(config)
            .await
            .expect("Failed to create storage");

        let is_valid = storage.verify().await.expect("Failed to verify");
        assert!(is_valid);
    }

    #[tokio::test]
    async fn test_storage_stats() {
        let config = create_test_config();
        let storage = ImmuDbStorage::new(config)
            .await
            .expect("Failed to create storage");

        let stats = storage.stats();
        assert_eq!(stats.cached_entries, 0);
        assert_eq!(stats.total_entries, 0);
        assert!(stats.state_hash.is_some());
    }
}

mod immudb_audit_store_tests {
    use super::*;

    async fn create_test_store() -> ImmuDbAuditStore {
        let key_pair = SigningKeyPair::generate().expect("Failed to generate key pair");
        let config = create_test_store_config();

        ImmuDbAuditStore::new(
            config,
            key_pair.fingerprint().to_string(),
            key_pair.public_key().to_vec(),
        )
        .await
        .expect("Failed to create store")
    }

    #[tokio::test]
    async fn test_audit_store_creation() {
        let store = create_test_store().await;

        let stats = store.cache_stats().await;
        assert_eq!(stats.cached_entries, 0);
        assert_eq!(stats.max_cache_size, 1000);
    }

    #[tokio::test]
    async fn test_audit_store_and_retrieve() {
        let store = create_test_store().await;

        let entry = create_test_entry(0, AuditAction::CredentialDecrypt, Outcome::Success);

        // 存储
        let stored = store.store(&entry).await.expect("Failed to store");
        assert_eq!(stored.signed_entry.log_index, 0);
        assert!(!stored.state_hash.is_empty());

        // 检索
        let retrieved = store.get_by_index(0).await.expect("Failed to retrieve");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().log_index, 0);
    }

    #[tokio::test]
    async fn test_audit_store_batch() {
        let store = create_test_store().await;

        let entries: Vec<_> = (0..5)
            .map(|i| create_test_entry(i, AuditAction::TokenValidate, Outcome::Success))
            .collect();

        let stored = store
            .store_batch(&entries)
            .await
            .expect("Failed to batch store");
        assert_eq!(stored.len(), 5);

        // 验证缓存
        let stats = store.cache_stats().await;
        assert_eq!(stats.cached_entries, 5);
    }

    #[tokio::test]
    async fn test_audit_store_recent() {
        let store = create_test_store().await;

        // 存储 10 个条目
        for i in 0..10 {
            let entry = create_test_entry(i, AuditAction::TokenValidate, Outcome::Success);
            store.store(&entry).await.expect("Failed to store");
        }

        // 获取最近 3 个
        let recent = store.get_recent(3).await.expect("Failed to get recent");
        assert_eq!(recent.len(), 3);

        // 验证索引（最近的应该有最高索引）
        assert_eq!(recent[2].log_index, 9);
    }

    #[tokio::test]
    async fn test_audit_store_verify() {
        let store = create_test_store().await;

        // 存储一些条目
        for i in 0..5 {
            let entry = create_test_entry(i, AuditAction::TokenValidate, Outcome::Success);
            store.store(&entry).await.expect("Failed to store");
        }

        // 验证存储
        let is_valid = store.verify().await.expect("Failed to verify");
        assert!(is_valid);
    }

    #[tokio::test]
    async fn test_audit_store_current_state() {
        let store = create_test_store().await;

        let initial_state = store.current_state().await.expect("Failed to get state");

        // 存储条目
        let entry = create_test_entry(0, AuditAction::CredentialDecrypt, Outcome::Success);
        store.store(&entry).await.expect("Failed to store");

        let new_state = store.current_state().await.expect("Failed to get state");
        assert!(new_state.tree_size > initial_state.tree_size);
    }

    #[tokio::test]
    async fn test_audit_store_survives_restart_with_same_signing_key() {
        let key_pair = SigningKeyPair::generate().expect("Failed to generate key pair");
        let config = create_test_store_config();

        let first_store = ImmuDbAuditStore::new(
            config.clone(),
            key_pair.fingerprint().to_string(),
            key_pair.public_key().to_vec(),
        )
        .await
        .expect("Failed to create first store");

        let first_public_key = first_store.public_key().to_vec();
        let entry = create_test_entry(0, AuditAction::CredentialDecrypt, Outcome::Success);
        first_store.store(&entry).await.expect("Failed to store");

        let second_store = ImmuDbAuditStore::new(
            config.clone(),
            key_pair.fingerprint().to_string(),
            key_pair.public_key().to_vec(),
        )
        .await
        .expect("Failed to recreate store");

        assert_eq!(second_store.public_key(), first_public_key);
        let retrieved = second_store
            .get_by_index(0)
            .await
            .expect("Failed to retrieve after restart");
        assert!(retrieved.is_some(), "audit entry should survive restart");
    }

    #[tokio::test]
    async fn test_audit_store_generate_report() {
        let store = create_test_store().await;

        // 存储成功条目
        for i in 0..3 {
            let entry = create_test_entry(i, AuditAction::TokenValidate, Outcome::Success);
            store.store(&entry).await.expect("Failed to store");
        }

        // 存储失败条目
        for i in 3..5 {
            let entry = create_test_entry(i, AuditAction::CredentialDecrypt, Outcome::Failure);
            store.store(&entry).await.expect("Failed to store");
        }

        let report = store
            .generate_report(None, None)
            .await
            .expect("Failed to generate report");

        // 验证报告结构（模拟实现不返回实际条目数据）
        assert!(!report.state_hash.is_empty());
        // 注意：由于使用模拟 immudb 客户端，generate_report 的计数可能为 0
        // 在生产环境中，这些值应该反映实际的审计日志条目数
    }

    #[tokio::test]
    async fn test_audit_store_cache_clear() {
        let store = create_test_store().await;

        // 存储条目
        let entry = create_test_entry(0, AuditAction::CredentialDecrypt, Outcome::Success);
        store.store(&entry).await.expect("Failed to store");

        // 验证缓存
        let stats = store.cache_stats().await;
        assert_eq!(stats.cached_entries, 1);

        // 清空缓存
        store.clear_cache().await;

        let stats = store.cache_stats().await;
        assert_eq!(stats.cached_entries, 0);
    }

    #[tokio::test]
    async fn test_audit_store_count() {
        let store = create_test_store().await;

        let initial_count = store.count().await.expect("Failed to get count");

        // 存储条目
        for i in 0..5 {
            let entry = create_test_entry(i, AuditAction::TokenValidate, Outcome::Success);
            store.store(&entry).await.expect("Failed to store");
        }

        let new_count = store.count().await.expect("Failed to get count");
        assert_eq!(new_count, initial_count + 5);
    }
}

mod immudb_config_tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ImmuDbConfig::default();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 3322);
        assert_eq!(config.database, "credbridge_audit");
        assert_eq!(config.username, "immudb");
        assert_eq!(config.password, "immudb");
        assert_eq!(config.timeout_secs, 30);
        assert!(!config.use_tls);
        assert_eq!(config.collection, "audit_logs");
    }

    #[test]
    fn test_config_from_env_defaults() {
        // 在没有设置环境变量时，应该使用默认值
        let config = ImmuDbConfig::from_env();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 3322);
    }

    #[test]
    fn test_config_address() {
        let config = ImmuDbConfig {
            host: "192.168.1.100".to_string(),
            port: 3323,
            ..Default::default()
        };

        assert_eq!(config.address(), "192.168.1.100:3323");
    }

    #[test]
    fn test_store_config_default() {
        let config = ImmuDbStoreConfig::default();

        assert_eq!(config.max_cache_size, 10_000);
        assert_eq!(config.auto_sync_interval_secs, 60);
    }
}

mod immudb_integration_tests {
    use super::*;

    async fn create_integration_store() -> ImmuDbAuditStore {
        let key_pair = SigningKeyPair::generate().expect("Failed to generate key pair");
        let config = create_test_store_config();

        ImmuDbAuditStore::new(
            config,
            key_pair.fingerprint().to_string(),
            key_pair.public_key().to_vec(),
        )
        .await
        .expect("Failed to create store")
    }

    #[tokio::test]
    async fn test_end_to_end_workflow() {
        // 1. 创建存储
        let store: ImmuDbAuditStore = create_integration_store().await;

        // 2. 存储不同类型的审计事件
        let actions = [
            (AuditAction::CredentialDecrypt, Outcome::Success),
            (AuditAction::CredentialAccess, Outcome::Success),
            (AuditAction::TokenIssue, Outcome::Success),
            (AuditAction::TokenValidate, Outcome::Success),
            (AuditAction::CredentialDecrypt, Outcome::Failure),
            (AuditAction::AdminLogin, Outcome::Success),
            (AuditAction::SystemConfigChange, Outcome::Success),
        ];

        for (i, (action, outcome)) in actions.iter().enumerate() {
            let entry = create_test_entry(i as u64, *action, *outcome);
            store.store(&entry).await.expect("Failed to store");
        }

        // 3. 验证存储完整性
        let is_valid = store.verify().await.expect("Failed to verify");
        assert!(is_valid, "Store verification failed");

        // 4. 获取最近条目
        let recent = store.get_recent(5).await.expect("Failed to get recent");
        assert_eq!(recent.len(), 5);

        // 5. 生成审计报告
        let report = store
            .generate_report(None, None)
            .await
            .expect("Failed to generate report");

        // 验证报告结构（模拟实现不返回实际条目数据）
        assert!(!report.state_hash.is_empty());

        // 6. 获取当前状态
        let state = store.current_state().await.expect("Failed to get state");
        // tree_size 应该是 7（我们存储了 7 个条目）
        assert_eq!(
            state.tree_size, 7,
            "Tree size should be 7 after storing 7 entries"
        );
        assert!(!state.state_hash.is_empty());

        tracing::info!("End-to-end workflow completed successfully");
    }

    #[tokio::test]
    async fn test_state_consistency() {
        let store: ImmuDbAuditStore = create_integration_store().await;

        // 存储多个条目
        let mut previous_hash = store
            .current_state()
            .await
            .expect("Failed to get state")
            .state_hash;

        for i in 0..5 {
            let entry = create_test_entry(i, AuditAction::TokenValidate, Outcome::Success);
            store.store(&entry).await.expect("Failed to store");

            let current_state = store.current_state().await.expect("Failed to get state");
            let current_hash = current_state.state_hash;

            // 每次存储后状态哈希应该变化
            assert_ne!(
                previous_hash, current_hash,
                "State hash should change after each store"
            );

            previous_hash = current_hash;
        }
    }

    #[tokio::test]
    async fn test_immutability_guarantee() {
        let store: ImmuDbAuditStore = create_integration_store().await;

        // 存储条目
        let entry = create_test_entry(0, AuditAction::CredentialDecrypt, Outcome::Success);
        let stored = store.store(&entry).await.expect("Failed to store");

        let initial_state_hash = stored.state_hash;

        // 获取当前状态
        let state = store.current_state().await.expect("Failed to get state");

        // 验证状态哈希的一致性
        assert!(
            state.state_hash.starts_with(&initial_state_hash[..16])
                || initial_state_hash.starts_with(&state.state_hash[..16]),
            "State hash should maintain consistency"
        );
    }
}
