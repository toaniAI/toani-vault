//! Redis Token 存储集成测试
//!
//! 这些测试需要 Redis 服务器运行。
//! 默认连接地址: redis://127.0.0.1:6379
//! 可通过 REDIS_URL 环境变量自定义连接地址
//!
//! 运行测试前请确保 Redis 可用：
//! ```bash
//! redis-server
//! cargo test --test redis_store_tests -- --nocapture
//! ```
//!
//! 使用自定义 Redis 地址：
//! ```bash
//! REDIS_URL=redis://localhost:6379 cargo test --test redis_store_tests
//! ```

use std::time::{SystemTime, UNIX_EPOCH};
use vault_service::token::{RedisTokenStore, TokenClaims, TokenStoreError, token_keys as keys};

/// 获取当前 Unix 时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_secs()
}

/// 获取 Redis 客户端，如果 Redis 不可用则跳过测试
/// 优先从 REDIS_URL 环境变量读取连接地址，默认使用 redis://127.0.0.1:6379/
async fn get_redis_client() -> Option<redis::Client> {
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://:credbridge_redis_pass@127.0.0.1:6379/".to_string());
    let client = redis::Client::open(redis_url.as_str()).ok()?;
    // 测试连接
    match client.get_multiplexed_async_connection().await {
        Ok(_) => Some(client),
        Err(e) => {
            println!("⚠️ Redis 不可用 ({}), 跳过集成测试", e);
            None
        }
    }
}

/// 创建测试用的 TokenStore
async fn create_test_store() -> Option<RedisTokenStore> {
    let client = get_redis_client().await?;
    Some(RedisTokenStore::new(client))
}

/// 生成唯一的测试租户 ID
fn test_tenant_id() -> String {
    format!("test_tenant_{}", uuid::Uuid::now_v7())
}

/// 生成唯一的测试 jti
fn test_jti() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[tokio::test]
async fn test_store_token_success() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return, // Redis 不可用，跳过
    };

    let tenant_id = test_tenant_id();
    let jti = test_jti();
    let exp = current_timestamp() + 3600;

    // 存储 Token
    let result = store
        .store_token(&tenant_id, &jti, "user_123", "credential:read", exp)
        .await;
    assert!(result.is_ok(), "存储 Token 失败: {:?}", result);

    // 验证活跃集合中有该 Token
    let active_count = store.get_active_count(&tenant_id).await.unwrap();
    assert_eq!(active_count, 1, "活跃 Token 数量应为 1");

    // 验证元数据
    let metadata = store.get_metadata(&jti).await.unwrap();
    assert!(metadata.is_some(), "应存在 Token 元数据");

    let metadata = metadata.unwrap();
    assert_eq!(metadata.user_id, "user_123");
    assert_eq!(metadata.tenant_id, tenant_id);
    assert_eq!(metadata.scope, "credential:read");
    assert_eq!(metadata.expires_at, exp);
    assert!(!metadata.revoked);

    // 清理
    store.delete_token(&tenant_id, &jti).await.unwrap();
}

#[tokio::test]
async fn test_store_claims_success() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    let tenant_id = test_tenant_id();
    let claims = TokenClaims::with_default_ttl("user_456", &tenant_id, "credential:write", true);

    // 存储 Claims
    let result = store.store_claims(&claims).await;
    assert!(result.is_ok());

    // 验证活跃集合
    let active_count = store.get_active_count(&tenant_id).await.unwrap();
    assert_eq!(active_count, 1);

    // 清理
    store.delete_token(&tenant_id, &claims.jti).await.unwrap();
}

#[tokio::test]
async fn test_is_revoked() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    let tenant_id = test_tenant_id();
    let jti = test_jti();
    let exp = current_timestamp() + 3600;

    // 存储 Token
    store
        .store_token(&tenant_id, &jti, "user_123", "credential:read", exp)
        .await
        .unwrap();

    // 初始状态：未撤销
    let is_revoked = store.is_revoked(&tenant_id, &jti).await.unwrap();
    assert!(!is_revoked, "新 Token 不应被撤销");

    // 撤销 Token
    store.revoke_token(&tenant_id, &jti).await.unwrap();

    // 撤销后：应被撤销
    let is_revoked = store.is_revoked(&tenant_id, &jti).await.unwrap();
    assert!(is_revoked, "撤销后 Token 应被标记为已撤销");

    // 清理
    store.delete_token(&tenant_id, &jti).await.unwrap();
}

#[tokio::test]
async fn test_revoke_token_updates_metadata() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    let tenant_id = test_tenant_id();
    let jti = test_jti();
    let exp = current_timestamp() + 3600;

    // 存储 Token
    store
        .store_token(&tenant_id, &jti, "user_123", "credential:read", exp)
        .await
        .unwrap();

    // 撤销前元数据
    let metadata_before = store.get_metadata(&jti).await.unwrap().unwrap();
    assert!(!metadata_before.revoked);
    assert!(metadata_before.revoked_at.is_none());

    // 撤销 Token
    store.revoke_token(&tenant_id, &jti).await.unwrap();

    // 撤销后元数据
    let metadata_after = store.get_metadata(&jti).await.unwrap().unwrap();
    assert!(metadata_after.revoked);
    assert!(metadata_after.revoked_at.is_some());

    // 活跃集合中应不再存在
    let active_count = store.get_active_count(&tenant_id).await.unwrap();
    assert_eq!(active_count, 0, "撤销后活跃集合应为空");

    // 撤销集合中应存在
    let revoked_count = store.get_revoked_count(&tenant_id).await.unwrap();
    assert_eq!(revoked_count, 1, "撤销集合应有 1 个 Token");

    // 清理
    store.delete_token(&tenant_id, &jti).await.unwrap();
}

#[tokio::test]
async fn test_revoke_already_revoked_token() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    let tenant_id = test_tenant_id();
    let jti = test_jti();
    let exp = current_timestamp() + 3600;

    // 存储并撤销 Token
    store
        .store_token(&tenant_id, &jti, "user_123", "credential:read", exp)
        .await
        .unwrap();
    store.revoke_token(&tenant_id, &jti).await.unwrap();

    // 再次撤销应返回错误
    let result = store.revoke_token(&tenant_id, &jti).await;
    assert!(
        matches!(result, Err(TokenStoreError::TokenAlreadyRevoked(_))),
        "重复撤销应返回 TokenAlreadyRevoked 错误"
    );

    // 清理
    store.delete_token(&tenant_id, &jti).await.unwrap();
}

#[tokio::test]
async fn test_store_token_with_invalid_timestamp() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    let tenant_id = test_tenant_id();
    let jti = test_jti();
    let past_exp = current_timestamp() - 100; // 过去的时间

    // 存储过期时间已过期的 Token 应失败
    let result = store
        .store_token(&tenant_id, &jti, "user_123", "credential:read", past_exp)
        .await;

    assert!(
        matches!(result, Err(TokenStoreError::InvalidTimestamp)),
        "过期时间戳无效应返回错误"
    );
}

#[tokio::test]
async fn test_list_active_tokens() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    let tenant_id = test_tenant_id();
    let exp = current_timestamp() + 3600;

    // 存储多个 Token
    let mut jtis = Vec::new();
    for _ in 0..5 {
        let jti = test_jti();
        store
            .store_token(&tenant_id, &jti, "user_123", "credential:read", exp)
            .await
            .unwrap();
        jtis.push(jti);
    }

    // 列出所有活跃 Token
    let listed = store.list_active_tokens(&tenant_id, 0).await.unwrap();
    assert_eq!(listed.len(), 5, "应列出 5 个活跃 Token");

    // 使用 limit
    let limited = store.list_active_tokens(&tenant_id, 3).await.unwrap();
    assert_eq!(limited.len(), 3, "使用 limit=3 应返回 3 个");

    // 清理
    for jti in jtis {
        store.delete_token(&tenant_id, &jti).await.unwrap();
    }
}

#[tokio::test]
async fn test_list_revoked_tokens() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    let tenant_id = test_tenant_id();
    let exp = current_timestamp() + 3600;

    // 存储并撤销多个 Token
    let mut jtis = Vec::new();
    for _ in 0..3 {
        let jti = test_jti();
        store
            .store_token(&tenant_id, &jti, "user_123", "credential:read", exp)
            .await
            .unwrap();
        store.revoke_token(&tenant_id, &jti).await.unwrap();
        jtis.push(jti);
    }

    // 列出所有撤销的 Token
    let listed = store.list_revoked_tokens(&tenant_id).await.unwrap();
    assert_eq!(listed.len(), 3, "应列出 3 个撤销 Token");

    // 清理
    for jti in jtis {
        store.delete_token(&tenant_id, &jti).await.unwrap();
    }
}

#[tokio::test]
async fn test_cleanup_expired_tokens() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    let tenant_id = test_tenant_id();
    let now = current_timestamp();

    // 存储一个即将过期的 Token（1秒后过期）
    let jti_expired = test_jti();
    store
        .store_token(
            &tenant_id,
            &jti_expired,
            "user_123",
            "credential:read",
            now + 1,
        )
        .await
        .unwrap();

    // 存储一个长期有效的 Token
    let jti_valid = test_jti();
    store
        .store_token(
            &tenant_id,
            &jti_valid,
            "user_123",
            "credential:read",
            now + 3600,
        )
        .await
        .unwrap();

    // 等待过期
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 清理过期 Token
    let cleaned = store.cleanup_expired(&tenant_id).await.unwrap();
    assert_eq!(cleaned, 1, "应清理 1 个过期 Token");

    // 验证活跃集合
    let active_count = store.get_active_count(&tenant_id).await.unwrap();
    assert_eq!(active_count, 1, "应剩余 1 个活跃 Token");

    // 清理
    store.delete_token(&tenant_id, &jti_valid).await.unwrap();
}

#[tokio::test]
async fn test_get_metadata_not_found() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    // 查询不存在的 Token
    let metadata = store.get_metadata("non_existent_jti").await.unwrap();
    assert!(metadata.is_none(), "不存在的 Token 应返回 None");
}

#[tokio::test]
async fn test_multiple_tenants_isolation() {
    let store = match create_test_store().await {
        Some(s) => s,
        None => return,
    };

    let tenant1 = test_tenant_id();
    let tenant2 = test_tenant_id();
    let exp = current_timestamp() + 3600;

    // 在租户 1 存储 Token
    let jti1 = test_jti();
    store
        .store_token(&tenant1, &jti1, "user_1", "credential:read", exp)
        .await
        .unwrap();

    // 在租户 2 存储 Token
    let jti2 = test_jti();
    store
        .store_token(&tenant2, &jti2, "user_2", "credential:write", exp)
        .await
        .unwrap();

    // 验证数量隔离
    assert_eq!(store.get_active_count(&tenant1).await.unwrap(), 1);
    assert_eq!(store.get_active_count(&tenant2).await.unwrap(), 1);

    // 撤销租户 1 的 Token，不应影响租户 2
    store.revoke_token(&tenant1, &jti1).await.unwrap();
    assert!(store.is_revoked(&tenant1, &jti1).await.unwrap());
    assert!(!store.is_revoked(&tenant2, &jti2).await.unwrap());

    // 清理
    store.delete_token(&tenant1, &jti1).await.unwrap();
    store.delete_token(&tenant2, &jti2).await.unwrap();
}

#[tokio::test]
async fn test_redis_keys_generation() {
    // 验证 Redis key 生成规则
    let tenant_id = "test_tenant";
    let jti = "test_jti";

    assert_eq!(
        keys::active_tokens_key(tenant_id),
        "credbridge:tokens:test_tenant:active"
    );
    assert_eq!(
        keys::revoked_tokens_key(tenant_id),
        "credbridge:tokens:test_tenant:revoked"
    );
    assert_eq!(keys::token_metadata_key(jti), "credbridge:token:test_jti");
}

#[tokio::test]
async fn test_from_url_success() {
    // 测试从 URL 创建 store
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://:credbridge_redis_pass@127.0.0.1:6379/".to_string());
    let result = RedisTokenStore::from_url(&redis_url);

    // 如果 Redis 不可用，跳过测试
    if result.is_err() {
        println!("⚠️ Redis 不可用，跳过测试");
        return;
    }

    let store = result.unwrap();
    let tenant_id = test_tenant_id();
    let jti = test_jti();
    let exp = current_timestamp() + 3600;

    // 尝试存储 Token，如果 Redis 不可用则跳过
    if store
        .store_token(&tenant_id, &jti, "user_123", "credential:read", exp)
        .await
        .is_err()
    {
        println!("⚠️ Redis 连接失败，跳过测试");
        return;
    }

    store.delete_token(&tenant_id, &jti).await.unwrap();
}

#[tokio::test]
async fn test_from_url_invalid() {
    // 测试无效 URL
    let result = RedisTokenStore::from_url("invalid_url");
    assert!(result.is_err(), "无效 URL 应返回错误");
}
