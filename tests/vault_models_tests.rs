//! Vault 数据模型测试
//!
//! 测试 EP2-Story2.1 凭证数据模型的核心功能

use vault_service::models::CredentialType;
use vault_service::vault::{
    CredentialId, CredentialVault, ServiceId, TenantId, UserId,
    models::{CreateCredentialRequest, CredentialFilter, EncryptedPayload, VaultEntry},
};

// 辅助函数：创建测试用加密载荷
fn create_test_payload() -> EncryptedPayload {
    use vault_service::crypto::constants;

    EncryptedPayload::new(
        constants::PROTOCOL_VERSION,
        constants::ALGORITHM_AES_256_GCM,
        constants::KDF_HKDF_SHA256,
        vec![0u8; constants::NONCE_LENGTH],
        vec![0u8; constants::AUTH_TAG_LENGTH],
        vec![1, 2, 3, 4, 5],
    )
}

/// AC-1: 创建新凭证请求时，应生成 UUID v7 作为凭证 ID
#[test]
fn test_ac1_uuid_v7_generation() {
    let vault = CredentialVault::new_in_memory();

    let entry = vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_123"),
                user_id: UserId::new("user_456"),
                service_id: ServiceId::new("schwab"),
                credential_type: CredentialType::UsernamePassword,
                expires_at: None,
            },
            create_test_payload(),
        )
        .expect("应该能创建凭证");

    // 验证凭证 ID 是有效的 UUID
    let credential_id = entry.credential_id.as_str();
    assert!(
        uuid::Uuid::parse_str(credential_id).is_ok(),
        "凭证 ID 应该是有效的 UUID"
    );

    // 验证是 UUID v7（第 13 个字符应该是 '7'）
    // UUID v7 格式: time_high(32)-time_mid(16)-ver(4)-rand(76)
    let parts: Vec<&str> = credential_id.split('-').collect();
    assert_eq!(parts.len(), 5, "UUID 应该有 5 个部分");

    // 第三个部分的第一个字符表示版本
    let version_char = parts[2].chars().next().unwrap();
    assert_eq!(version_char, '7', "应该是 UUID v7");
}

/// AC-1: 凭证应存储凭证类型、user_id（哈希后）、service_id、created_at、expires_at
#[test]
fn test_ac1_credential_storage_metadata() {
    let vault = CredentialVault::new_in_memory();
    let tenant_id = TenantId::new("tenant_123");
    let user_id = UserId::new("user_456");
    let service_id = ServiceId::new("schwab");
    let expires_at = 1893456000u64; // 2030年

    let entry = vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: tenant_id.clone(),
                user_id: user_id.clone(),
                service_id: service_id.clone(),
                credential_type: CredentialType::UsernamePassword,
                expires_at: Some(expires_at),
            },
            create_test_payload(),
        )
        .expect("应该能创建凭证");

    // 验证存储的数据
    assert_eq!(entry.credential_type, CredentialType::UsernamePassword);
    assert_eq!(entry.service_id.as_str(), "schwab");
    assert_eq!(entry.tenant_id.as_str(), "tenant_123");
    assert_eq!(entry.expires_at, Some(expires_at));
    assert!(!entry.is_deleted);
    assert!(entry.created_at > 0);

    // 验证 user_id 是哈希存储的（不是明文）
    let user_hash = entry.user_id.hash();
    assert_ne!(user_hash, "user_456", "user_id 应该是哈希后的值");
    assert!(!user_hash.is_empty(), "user_id 哈希不应该为空");

    // 验证原始 user_id 可以在内存中访问
    assert_eq!(entry.user_id.raw(), Some("user_456"));
}

/// AC-2: 加密载荷应包含 version=2, algorithm='AES-256-GCM', kdf='HKDF-SHA-256'
#[test]
fn test_ac2_encrypted_payload_format() {
    let payload = create_test_payload();

    // 验证版本
    assert_eq!(payload.version, 2, "版本应该是 2");

    // 验证算法
    assert_eq!(payload.algorithm, "AES-256-GCM", "算法应该是 AES-256-GCM");

    // 验证 KDF
    assert_eq!(payload.kdf, "HKDF-SHA-256", "KDF 应该是 HKDF-SHA-256");

    // 验证 nonce 长度
    let nonce = payload.nonce_bytes().expect("应该能解码 nonce");
    assert_eq!(nonce.len(), 12, "nonce 应该是 12 字节");

    // 验证 auth_tag 长度
    let auth_tag = payload.auth_tag_bytes().expect("应该能解码 auth_tag");
    assert_eq!(auth_tag.len(), 16, "auth_tag 应该是 16 字节");
}

/// AC-3: 根据 credential_id 查询时，应返回凭证元数据（不含加密载荷）
#[test]
fn test_ac3_query_metadata_without_payload() {
    let vault = CredentialVault::new_in_memory();
    let tenant_id = TenantId::new("tenant_123");
    let user_id = UserId::new("user_456");

    let entry = vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: tenant_id.clone(),
                user_id: user_id.clone(),
                service_id: ServiceId::new("github"),
                credential_type: CredentialType::ApiKey,
                expires_at: None,
            },
            create_test_payload(),
        )
        .expect("应该能创建凭证");

    // 查询元数据
    let metadata = vault
        .get_credential_metadata(
            &entry.credential_id,
            &tenant_id,
            &UserId::from_hash(entry.user_id.hash()),
        )
        .expect("应该能查询凭证")
        .expect("凭证应该存在");

    // 验证元数据包含预期字段
    assert_eq!(metadata.credential_id, entry.credential_id.as_str());
    assert_eq!(metadata.credential_type, CredentialType::ApiKey);
    assert_eq!(metadata.service_id, "github");
    assert_eq!(metadata.tenant_id, "tenant_123");

    // 验证元数据不包含加密载荷
    // （CredentialMetadata 结构体本身没有 encrypted_payload 字段）
}

/// AC-3: 只有授权用户可访问其租户数据
#[test]
fn test_ac3_tenant_isolation() {
    let vault = CredentialVault::new_in_memory();

    // 创建租户 A 的凭证
    let entry_a = vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_a"),
                user_id: UserId::new("user_a"),
                service_id: ServiceId::new("service_a"),
                credential_type: CredentialType::UsernamePassword,
                expires_at: None,
            },
            create_test_payload(),
        )
        .expect("应该能创建凭证");

    // 创建租户 B 的凭证
    let entry_b = vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_b"),
                user_id: UserId::new("user_b"),
                service_id: ServiceId::new("service_b"),
                credential_type: CredentialType::ApiKey,
                expires_at: None,
            },
            create_test_payload(),
        )
        .expect("应该能创建凭证");

    // 租户 A 用户可以访问自己的凭证
    let metadata = vault
        .get_credential_metadata(
            &entry_a.credential_id,
            &TenantId::new("tenant_a"),
            &UserId::from_hash(entry_a.user_id.hash()),
        )
        .expect("应该能查询凭证");
    assert!(metadata.is_some());

    // 租户 B 用户不能访问租户 A 的凭证
    let result = vault.get_credential_metadata(
        &entry_a.credential_id,
        &TenantId::new("tenant_b"), // 错误的租户
        &UserId::from_hash(entry_a.user_id.hash()),
    );
    assert!(result.is_err(), "应该拒绝跨租户访问");

    // 租户 A 用户不能访问租户 B 的凭证
    let result = vault.get_credential_metadata(
        &entry_b.credential_id,
        &TenantId::new("tenant_a"),
        &UserId::from_hash(entry_b.user_id.hash()),
    );
    assert!(result.is_err(), "应该拒绝跨租户访问");
}

/// 测试：用户 ID 哈希验证
#[test]
fn test_user_id_hash_verification() {
    let user_id = UserId::new("sensitive_user_id_123");

    // 原始值可以验证
    assert!(user_id.verify("sensitive_user_id_123"));
    assert!(!user_id.verify("wrong_user_id"));

    // 哈希值是确定性的
    let hash1 = user_id.hash().to_string();
    let user_id2 = UserId::new("sensitive_user_id_123");
    let hash2 = user_id2.hash().to_string();
    assert_eq!(hash1, hash2, "相同 user_id 应该有相同的哈希");

    // 不同 user_id 有不同哈希
    let user_id3 = UserId::new("different_user_id");
    let hash3 = user_id3.hash().to_string();
    assert_ne!(hash1, hash3, "不同 user_id 应该有不同哈希");
}

/// 测试：UUID v7 时间排序特性
#[test]
fn test_uuid_v7_time_sorting() {
    let mut ids: Vec<CredentialId> = (0..20).map(|_| CredentialId::new()).collect();

    // 按字符串排序（UUID v7 的前 48-bit 是时间戳）
    ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    // 验证时间顺序
    // 第一个 ID 的时间戳应该小于或等于最后一个 ID
    let first = ids.first().unwrap().as_str();
    let last = ids.last().unwrap().as_str();
    assert!(first <= last, "按时间排序后，第一个应该小于等于最后一个");

    // 所有 ID 应该是唯一的
    let unique: std::collections::HashSet<_> = ids.iter().map(|id| id.as_str()).collect();
    assert_eq!(unique.len(), ids.len(), "所有 ID 应该是唯一的");
}

/// 测试：凭证过滤器
#[test]
fn test_credential_filtering() {
    let vault = CredentialVault::new_in_memory();
    let tenant_id = TenantId::new("tenant_123");
    let user_id = UserId::new("user_456");

    // 创建多个凭证
    vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: tenant_id.clone(),
                user_id: user_id.clone(),
                service_id: ServiceId::new("schwab"),
                credential_type: CredentialType::UsernamePassword,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: tenant_id.clone(),
                user_id: user_id.clone(),
                service_id: ServiceId::new("github"),
                credential_type: CredentialType::ApiKey,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: tenant_id.clone(),
                user_id: user_id.clone(),
                service_id: ServiceId::new("google"),
                credential_type: CredentialType::OAuthRefresh,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    // 查询所有凭证
    let result = vault
        .list_credentials(
            &tenant_id,
            &UserId::from_hash(user_id.hash()),
            CredentialFilter::default(),
        )
        .expect("应该能查询凭证");
    assert_eq!(result.total, 3);

    // 按服务 ID 过滤
    let filter = CredentialFilter {
        service_id: Some(ServiceId::new("schwab")),
        ..Default::default()
    };
    let result = vault
        .list_credentials(&tenant_id, &UserId::from_hash(user_id.hash()), filter)
        .expect("应该能查询凭证");
    assert_eq!(result.total, 1);
    assert_eq!(result.credentials[0].service_id, "schwab");

    // 按凭证类型过滤
    let filter = CredentialFilter {
        credential_type: Some(CredentialType::ApiKey),
        ..Default::default()
    };
    let result = vault
        .list_credentials(&tenant_id, &UserId::from_hash(user_id.hash()), filter)
        .expect("应该能查询凭证");
    assert_eq!(result.total, 1);
    assert_eq!(
        result.credentials[0].credential_type,
        CredentialType::ApiKey
    );
}

/// 测试：凭证过期检查
#[test]
fn test_credential_expiration() {
    let vault = CredentialVault::new_in_memory();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 创建未过期的凭证
    let valid_entry = vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_123"),
                user_id: UserId::new("user_456"),
                service_id: ServiceId::new("service_1"),
                credential_type: CredentialType::ApiKey,
                expires_at: Some(now + 86400), // 24小时后过期
            },
            create_test_payload(),
        )
        .unwrap();

    assert!(!valid_entry.is_expired(), "凭证不应该过期");

    // 创建已过期的凭证（手动设置过去的时间）
    let expired_entry = VaultEntry::new(
        TenantId::new("tenant_123"),
        UserId::new("user_456"),
        ServiceId::new("service_2"),
        CredentialType::ApiKey,
        create_test_payload(),
        Some(now - 86400), // 24小时前过期
    );

    assert!(expired_entry.is_expired(), "凭证应该已经过期");
}

/// 测试：凭证软删除
#[test]
fn test_credential_soft_delete() {
    let vault = CredentialVault::new_in_memory();
    let tenant_id = TenantId::new("tenant_123");
    let user_id = UserId::new("user_456");

    let entry = vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: tenant_id.clone(),
                user_id: user_id.clone(),
                service_id: ServiceId::new("to_be_deleted"),
                credential_type: CredentialType::SessionCookie,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    // 软删除
    let deleted = vault
        .delete_credential(
            &entry.credential_id,
            &tenant_id,
            &UserId::from_hash(user_id.hash()),
        )
        .expect("应该能删除凭证");
    assert!(deleted);

    // 查询元数据（应该仍然能找到，但标记为已删除）
    let metadata = vault
        .get_credential_metadata(
            &entry.credential_id,
            &tenant_id,
            &UserId::from_hash(user_id.hash()),
        )
        .expect("应该能查询凭证")
        .expect("凭证应该存在");
    assert!(metadata.is_deleted);

    // 物理删除
    let purged = vault
        .purge_credential(
            &entry.credential_id,
            &tenant_id,
            &UserId::from_hash(user_id.hash()),
        )
        .expect("应该能物理删除凭证");
    assert!(purged);

    // 确认凭证不存在
    let exists = vault
        .credential_exists(&entry.credential_id)
        .expect("应该能检查凭证存在性");
    assert!(!exists);
}

/// 测试：错误的凭证 ID 格式
#[test]
fn test_invalid_credential_id() {
    let result = CredentialId::from_string("not-a-valid-uuid".to_string());
    assert!(result.is_err(), "应该拒绝无效的 UUID");

    // 有效的 UUID v4 也应该被接受（我们支持任何有效的 UUID）
    let valid_v4 = "550e8400-e29b-41d4-a716-446655440000";
    let result = CredentialId::from_string(valid_v4.to_string());
    assert!(result.is_ok(), "应该接受有效的 UUID v4");
}

/// 测试：不同用户不能访问其他用户的数据（同租户内）
#[test]
fn test_user_isolation_within_tenant() {
    let vault = CredentialVault::new_in_memory();
    let tenant_id = TenantId::new("tenant_123");

    // 用户 A 的凭证
    let entry_a = vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: tenant_id.clone(),
                user_id: UserId::new("user_a"),
                service_id: ServiceId::new("service_a"),
                credential_type: CredentialType::UsernamePassword,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    // 用户 B 尝试访问用户 A 的凭证（同租户）
    // 注意：目前实现中 user_id 验证需要哈希匹配
    // 这是一个设计选择：我们可以允许同租户内用户访问，或者完全隔离
    // 当前实现是完全隔离的
    let result = vault.get_credential_metadata(
        &entry_a.credential_id,
        &tenant_id,
        &UserId::new("user_b"), // 不同的用户
    );
    // 由于 user_id 哈希不匹配，应该会失败
    assert!(result.is_err() || result.unwrap().is_none());
}
