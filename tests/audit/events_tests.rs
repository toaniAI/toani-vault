//! 审计事件测试
//!
//! EP4-Story4.1 审计事件记录功能的集成测试

use vault_service::audit::{
    AuditAction, AuditEntry, AuditFilter, AuditRecorder, Outcome, PiiRedactor, RedactedParam,
    RiskTier, create_memory_storage, hash_user_id, is_high_risk_action,
};

/// 测试：审计条目创建和基本属性
#[test]
fn test_audit_entry_creation_and_properties() {
    let entry = AuditEntry::new(
        "user_hash_abc123",
        "session_xyz789",
        "vault-service",
        AuditAction::CredentialDecrypt,
        Outcome::Success,
        "mrenclave_measurement_123",
        "jti_token_456",
    );

    // 验证基本字段
    assert!(!entry.id.is_empty(), "ID should not be empty");
    assert_eq!(entry.user_id_hash, "user_hash_abc123");
    assert_eq!(entry.session_id, "session_xyz789");
    assert_eq!(entry.service, "vault-service");
    assert_eq!(entry.action, AuditAction::CredentialDecrypt);
    assert_eq!(entry.outcome, Outcome::Success);
    assert_eq!(entry.tee_mrenclave, "mrenclave_measurement_123");
    assert_eq!(entry.action_token_jti, "jti_token_456");

    // 验证时间戳
    assert!(entry.timestamp > 0, "Timestamp should be set");

    // 验证自动风险等级
    assert_eq!(
        entry.risk_tier,
        RiskTier::High,
        "CredentialDecrypt should be High risk"
    );
}

/// 测试：风险等级自动分配
#[test]
fn test_risk_tier_automatic_assignment() {
    let test_cases = vec![
        (AuditAction::TokenValidate, RiskTier::Low),
        (AuditAction::CredentialAccess, RiskTier::Medium),
        (AuditAction::TokenIssue, RiskTier::Medium),
        (AuditAction::CredentialDecrypt, RiskTier::High),
        (AuditAction::CredentialDelete, RiskTier::High),
        (AuditAction::AuditQuery, RiskTier::High),
        (AuditAction::KeyRotation, RiskTier::Critical),
        (AuditAction::SystemConfigChange, RiskTier::Critical),
        (AuditAction::AdminLogin, RiskTier::Critical),
        (AuditAction::FailedAuth, RiskTier::High),
    ];

    for (action, expected_tier) in test_cases {
        let entry = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            action,
            Outcome::Success,
            "mrenclave",
            "jti",
        );
        assert_eq!(
            entry.risk_tier, expected_tier,
            "Action {:?} should have {:?} risk tier",
            action, expected_tier
        );
    }
}

/// 测试：高风险操作检测
#[test]
fn test_high_risk_action_detection() {
    let high_risk_actions = vec![
        AuditAction::CredentialDecrypt,
        AuditAction::CredentialDelete,
        AuditAction::AuditQuery,
        AuditAction::SystemConfigChange,
        AuditAction::KeyRotation,
        AuditAction::AdminLogin,
        AuditAction::FailedAuth,
    ];

    for action in &high_risk_actions {
        assert!(
            is_high_risk_action(action),
            "{:?} should be a high-risk action",
            action
        );

        let entry = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            *action,
            Outcome::Success,
            "mrenclave",
            "jti",
        );
        assert!(
            entry.is_high_risk(),
            "Entry for {:?} should be high risk",
            action
        );
    }

    let low_risk_actions = vec![
        AuditAction::TokenValidate,
        AuditAction::TokenIssue,
        AuditAction::TokenRevoke,
        AuditAction::CredentialAccess,
        AuditAction::CredentialCreate,
        AuditAction::CredentialUpdate,
    ];

    for action in &low_risk_actions {
        let entry = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            *action,
            Outcome::Success,
            "mrenclave",
            "jti",
        );
        assert!(
            !entry.is_high_risk(),
            "Entry for {:?} should not be high risk",
            action
        );
    }
}

/// 测试：审计条目序列化和反序列化
#[test]
fn test_audit_entry_serialization() {
    let entry = AuditEntry::new(
        "user_hash_abc123",
        "session_xyz789",
        "vault-service",
        AuditAction::CredentialDecrypt,
        Outcome::Success,
        "mrenclave_measurement",
        "jti_token",
    )
    .with_param(
        "credential_id",
        RedactedParam::Plain("cred_123".to_string()),
    )
    .with_param("ssn", RedactedParam::SsnRedacted)
    .with_error("Test error message")
    .with_client_ip_hash("ip_hash_abc")
    .with_user_agent_hash("ua_hash_xyz");

    // 序列化
    let json = entry.to_json().expect("Serialization should succeed");
    assert!(json.contains("credential_decrypt"));
    assert!(json.contains("success"));
    assert!(json.contains("high"));
    assert!(json.contains("user_hash_abc123"));

    // 反序列化
    let deserialized = AuditEntry::from_json(&json).expect("Deserialization should succeed");
    assert_eq!(deserialized.id, entry.id);
    assert_eq!(deserialized.user_id_hash, entry.user_id_hash);
    assert_eq!(deserialized.action, entry.action);
    assert_eq!(deserialized.outcome, entry.outcome);
    assert_eq!(deserialized.risk_tier, entry.risk_tier);
}

/// 测试：PII 数据脱敏
#[test]
fn test_pii_redaction() {
    // 测试各种脱敏类型
    assert_eq!(
        PiiRedactor::redact_ssn("123-45-6789").to_string(),
        "[SSN_REDACTED]"
    );
    assert_eq!(
        PiiRedactor::redact_password("secret123").to_string(),
        "[PASSWORD_REDACTED]"
    );
    assert_eq!(
        PiiRedactor::redact_api_key("api_key_abc").to_string(),
        "[API_KEY_REDACTED]"
    );
    assert_eq!(
        PiiRedactor::redact_credit_card("4111111111111111").to_string(),
        "[CREDIT_CARD_REDACTED]"
    );
    assert_eq!(
        PiiRedactor::redact_email("user@example.com").to_string(),
        "[EMAIL_REDACTED]"
    );
    assert_eq!(
        PiiRedactor::redact_phone("+1-555-123-4567").to_string(),
        "[PHONE_REDACTED]"
    );
    assert_eq!(
        PiiRedactor::redact_address("123 Main St").to_string(),
        "[ADDRESS_REDACTED]"
    );
    assert_eq!(
        PiiRedactor::redact_key("private_key_xyz").to_string(),
        "[KEY_REDACTED]"
    );

    // 测试不敏感数据保留
    assert_eq!(
        PiiRedactor::plain("normal_value").to_string(),
        "normal_value"
    );
}

/// 测试：PII 自动脱敏
#[test]
fn test_pii_auto_redaction() {
    // SSN 相关
    assert!(matches!(
        PiiRedactor::auto_redact("user_ssn", "123-45-6789"),
        RedactedParam::SsnRedacted
    ));
    assert!(matches!(
        PiiRedactor::auto_redact("social_security", "123456789"),
        RedactedParam::SsnRedacted
    ));

    // 密码相关
    assert!(matches!(
        PiiRedactor::auto_redact("password", "secret123"),
        RedactedParam::PasswordRedacted
    ));
    assert!(matches!(
        PiiRedactor::auto_redact("user_pwd", "secret"),
        RedactedParam::PasswordRedacted
    ));

    // API 密钥相关
    assert!(matches!(
        PiiRedactor::auto_redact("api_key", "key_abc"),
        RedactedParam::ApiKeyRedacted
    ));
    assert!(matches!(
        PiiRedactor::auto_redact("secret_token", "token_xyz"),
        RedactedParam::ApiKeyRedacted
    ));

    // 邮箱相关
    assert!(matches!(
        PiiRedactor::auto_redact("email_address", "test@example.com"),
        RedactedParam::EmailRedacted
    ));

    // 电话相关
    assert!(matches!(
        PiiRedactor::auto_redact("phone_number", "555-1234"),
        RedactedParam::PhoneRedacted
    ));

    // 地址相关
    assert!(matches!(
        PiiRedactor::auto_redact("home_address", "123 Main St"),
        RedactedParam::AddressRedacted
    ));

    // 密钥相关
    assert!(matches!(
        PiiRedactor::auto_redact("private_key", "key_xyz"),
        RedactedParam::KeyRedacted
    ));

    // 不敏感参数
    assert!(matches!(
        PiiRedactor::auto_redact("credential_id", "cred_123"),
        RedactedParam::Plain(_)
    ));
}

/// 测试：用户 ID 哈希
#[test]
fn test_user_id_hashing() {
    // 相同输入产生相同哈希
    let hash1 = hash_user_id("user_123");
    let hash2 = hash_user_id("user_123");
    assert_eq!(hash1, hash2, "Same user ID should produce same hash");

    // 不同输入产生不同哈希
    let hash3 = hash_user_id("user_456");
    assert_ne!(
        hash1, hash3,
        "Different user IDs should produce different hashes"
    );

    // SHA-256 哈希长度为 64 个十六进制字符
    assert_eq!(hash1.len(), 64, "SHA-256 hash should be 64 hex characters");
}

/// 测试：审计条目内容哈希
#[test]
fn test_audit_entry_content_hash() {
    let entry = AuditEntry::new(
        "user_hash",
        "session",
        "service",
        AuditAction::CredentialDecrypt,
        Outcome::Success,
        "mrenclave",
        "jti",
    );

    let hash = entry.content_hash();
    assert_eq!(hash.len(), 32, "Content hash should be 32 bytes (SHA-256)");

    // 相同内容的条目应有不同哈希（因为 ID 不同）
    let entry2 = AuditEntry::new(
        "user_hash",
        "session",
        "service",
        AuditAction::CredentialDecrypt,
        Outcome::Success,
        "mrenclave",
        "jti",
    );
    let hash2 = entry2.content_hash();
    assert_ne!(
        hash, hash2,
        "Different entries should have different content hashes"
    );
}

/// 测试：审计记录器创建和基本操作
#[test]
fn test_audit_recorder_creation() {
    let recorder = AuditRecorder::new(100).expect("Should create recorder");

    let entry = AuditEntry::new(
        "user_hash",
        "session_123",
        "vault-service",
        AuditAction::CredentialDecrypt,
        Outcome::Success,
        "mrenclave_abc",
        "jti_xyz",
    );

    let signed = recorder.record(entry).expect("Should record entry");
    assert_eq!(signed.log_index, 0);
    assert!(!signed.signature.is_empty(), "Should have signature");
}

/// 测试：签名后的审计条目验证
#[test]
fn test_signed_audit_entry_verification() {
    let storage = create_memory_storage().expect("Should create storage");

    let entry = AuditEntry::new(
        "user_hash",
        "session",
        "service",
        AuditAction::CredentialDecrypt,
        Outcome::Success,
        "mrenclave",
        "jti",
    );

    let signed = storage.record(entry).expect("Should record entry");

    // 验证签名后的条目属性
    assert!(signed.log_index < 1000, "Log index should be reasonable");
    assert_eq!(
        signed.content_hash.len(),
        32,
        "Content hash should be 32 bytes"
    );
    assert_eq!(
        signed.prev_hash.len(),
        32,
        "Previous hash should be 32 bytes"
    );
    assert_eq!(
        signed.merkle_root.len(),
        32,
        "Merkle root should be 32 bytes"
    );
    assert!(!signed.signature.is_empty(), "Should have signature");
    assert!(
        !signed.signer_fingerprint.is_empty(),
        "Should have signer fingerprint"
    );
}

/// 测试：审计链完整性验证
#[test]
fn test_audit_chain_integrity() {
    let storage = create_memory_storage().expect("Should create storage");

    // 记录多个事件
    for i in 0..10 {
        let entry = AuditEntry::new(
            format!("user_{}", i),
            format!("session_{}", i),
            "vault-service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave_measurement",
            format!("jti_{}", i),
        );
        storage.record(entry).expect("Should record entry");
    }

    // 验证链完整性
    let is_valid = storage.verify().expect("Verification should not fail");
    assert!(is_valid, "Audit chain should be valid");
}

/// 测试：查询最近的事件
#[test]
fn test_query_recent_entries() {
    let storage = create_memory_storage().expect("Should create storage");

    // 记录事件
    for i in 0..5 {
        let entry = AuditEntry::new(
            format!("user_{}", i),
            "session",
            "service",
            AuditAction::TokenValidate,
            Outcome::Success,
            "mrenclave",
            format!("jti_{}", i),
        );
        storage.record(entry).unwrap();
    }

    // 查询最近的 3 个
    let recent = storage.query_recent(3).expect("Should query recent");
    assert_eq!(recent.len(), 3);

    // 验证顺序（最近的在最后）
    assert!(recent[2].log_index > recent[0].log_index);
}

/// 测试：按结果筛选事件
#[test]
fn test_query_by_outcome() {
    let storage = create_memory_storage().expect("Should create storage");

    // 记录成功事件
    for i in 0..3 {
        let entry = AuditEntry::new(
            format!("user_{}", i),
            "session",
            "service",
            AuditAction::TokenValidate,
            Outcome::Success,
            "mrenclave",
            format!("jti_{}", i),
        );
        storage.record(entry).unwrap();
    }

    // 记录失败事件
    for i in 0..2 {
        let entry = AuditEntry::new(
            format!("user_fail_{}", i),
            "session",
            "service",
            AuditAction::CredentialDecrypt,
            Outcome::Failure,
            "mrenclave",
            format!("jti_fail_{}", i),
        );
        storage.record(entry).unwrap();
    }

    // 查询失败事件
    let failures = storage.query_by_outcome(Outcome::Failure).unwrap();
    assert_eq!(failures.len(), 2);
    for entry in &failures {
        assert_eq!(entry.entry.outcome, Outcome::Failure);
    }
}

/// 测试：审计报告生成
#[test]
fn test_audit_report_generation() {
    let storage = create_memory_storage().expect("Should create storage");
    let recorder = storage.recorder();

    // 记录混合结果的事件
    for i in 0..10 {
        let outcome = if i % 3 == 0 {
            Outcome::Failure
        } else {
            Outcome::Success
        };
        let entry = AuditEntry::new(
            format!("user_{}", i),
            "session",
            "service",
            AuditAction::TokenValidate,
            outcome,
            "mrenclave",
            format!("jti_{}", i),
        );
        storage.record(entry).unwrap();
    }

    // 生成报告
    let report = recorder
        .generate_report(None, None)
        .expect("Should generate report");
    assert_eq!(report.total_entries, 10);
    assert_eq!(report.success_count, 6); // 1,2,4,5,7,8 (not divisible by 3)
    assert_eq!(report.failure_count, 4); // 0,3,6,9 (divisible by 3)
    assert!(!report.merkle_root.is_empty(), "Should have merkle root");
}

/// 测试：带参数的事件记录
#[test]
fn test_audit_entry_with_params() {
    let entry = AuditEntry::new(
        "user_hash",
        "session",
        "service",
        AuditAction::CredentialDecrypt,
        Outcome::Success,
        "mrenclave",
        "jti",
    )
    .with_param(
        "credential_id",
        RedactedParam::Plain("cred_123".to_string()),
    )
    .with_param("ssn", RedactedParam::SsnRedacted)
    .with_param("password", RedactedParam::PasswordRedacted)
    .with_param("api_key", RedactedParam::ApiKeyRedacted);

    let params = entry.params.as_ref().expect("Should have params");
    assert_eq!(params.len(), 4);

    // 验证每个参数
    let cred_param = params.iter().find(|(k, _)| k == "credential_id").unwrap();
    assert_eq!(cred_param.1.to_string(), "cred_123");

    let ssn_param = params.iter().find(|(k, _)| k == "ssn").unwrap();
    assert_eq!(ssn_param.1.to_string(), "[SSN_REDACTED]");

    let pwd_param = params.iter().find(|(k, _)| k == "password").unwrap();
    assert_eq!(pwd_param.1.to_string(), "[PASSWORD_REDACTED]");

    let api_key_param = params.iter().find(|(k, _)| k == "api_key").unwrap();
    assert_eq!(api_key_param.1.to_string(), "[API_KEY_REDACTED]");
}

/// 测试：带错误信息的事件记录
#[test]
fn test_audit_entry_with_error() {
    let entry = AuditEntry::new(
        "user_hash",
        "session",
        "service",
        AuditAction::CredentialDecrypt,
        Outcome::Failure,
        "mrenclave",
        "jti",
    )
    .with_error("Decryption failed: invalid key provided");

    assert_eq!(
        entry.error_message,
        Some("Decryption failed: invalid key provided".to_string())
    );
}

/// 测试：审计过滤器
#[test]
fn test_audit_filter() {
    let filter = AuditFilter::new()
        .with_start_time(1000)
        .with_end_time(2000)
        .with_user_id_hash("hash123")
        .with_action(AuditAction::CredentialDecrypt)
        .with_risk_tier(RiskTier::High)
        .with_outcome(Outcome::Success)
        .with_service("vault-service");

    assert_eq!(filter.start_time, Some(1000));
    assert_eq!(filter.end_time, Some(2000));
    assert_eq!(filter.user_id_hash, Some("hash123".to_string()));
    assert_eq!(filter.action, Some(AuditAction::CredentialDecrypt));
    assert_eq!(filter.risk_tier, Some(RiskTier::High));
    assert_eq!(filter.outcome, Some(Outcome::Success));
    assert_eq!(filter.service, Some("vault-service".to_string()));
}

/// 测试：操作结果枚举
#[test]
fn test_outcome_enum() {
    assert!(Outcome::Success.is_success());
    assert!(!Outcome::Success.is_failure());

    assert!(!Outcome::Failure.is_success());
    assert!(Outcome::Failure.is_failure());

    assert!(!Outcome::Denied.is_success());
    assert!(Outcome::Denied.is_failure());

    assert!(!Outcome::Timeout.is_success());
    assert!(Outcome::Timeout.is_failure());

    assert!(!Outcome::Aborted.is_success());
    assert!(Outcome::Aborted.is_failure());
}

/// 测试：风险等级枚举
#[test]
fn test_risk_tier_enum() {
    assert_eq!(RiskTier::Low.weight(), 1);
    assert_eq!(RiskTier::Medium.weight(), 2);
    assert_eq!(RiskTier::High.weight(), 4);
    assert_eq!(RiskTier::Critical.weight(), 8);

    assert!(!RiskTier::Low.requires_review());
    assert!(!RiskTier::Medium.requires_review());
    assert!(RiskTier::High.requires_review());
    assert!(RiskTier::Critical.requires_review());

    assert_eq!(RiskTier::Low.to_string(), "low");
    assert_eq!(RiskTier::Medium.to_string(), "medium");
    assert_eq!(RiskTier::High.to_string(), "high");
    assert_eq!(RiskTier::Critical.to_string(), "critical");
}

/// 测试：审计操作类型显示
#[test]
fn test_audit_action_display() {
    assert_eq!(
        AuditAction::CredentialDecrypt.to_string(),
        "credential_decrypt"
    );
    assert_eq!(
        AuditAction::CredentialAccess.to_string(),
        "credential_access"
    );
    assert_eq!(AuditAction::TokenIssue.to_string(), "token_issue");
    assert_eq!(AuditAction::TokenValidate.to_string(), "token_validate");
    assert_eq!(AuditAction::AuditQuery.to_string(), "audit_query");
    assert_eq!(AuditAction::KeyRotation.to_string(), "key_rotation");
}

/// 测试：审计条目年龄计算
#[test]
fn test_audit_entry_age() {
    let entry = AuditEntry::new(
        "user_hash",
        "session",
        "service",
        AuditAction::TokenValidate,
        Outcome::Success,
        "mrenclave",
        "jti",
    );

    // 条目刚创建，年龄应该很小
    let age = entry.age_millis();
    assert!(age < 1000, "Entry age should be less than 1 second");
}

/// 测试：Merkle Tree 根哈希更新
#[test]
fn test_merkle_root_update() {
    let storage = create_memory_storage().expect("Should create storage");
    let recorder = storage.recorder();

    let root1 = recorder.merkle_root().expect("Should get merkle root");

    // 记录事件
    let entry = AuditEntry::new(
        "user_hash",
        "session",
        "service",
        AuditAction::TokenValidate,
        Outcome::Success,
        "mrenclave",
        "jti",
    );
    storage.record(entry).unwrap();

    let root2 = recorder.merkle_root().expect("Should get merkle root");

    // Merkle 根应该变化
    assert_ne!(root1, root2, "Merkle root should change after adding entry");
}

/// 测试：大规模审计记录性能
#[test]
fn test_mass_audit_recording() {
    let storage = create_memory_storage().expect("Should create storage");

    let count = 100;
    for i in 0..count {
        let entry = AuditEntry::new(
            format!("user_{}", i % 10), // 10 个不同用户
            format!("session_{}", i),
            "vault-service",
            AuditAction::TokenValidate,
            Outcome::Success,
            "mrenclave_measurement",
            format!("jti_{}", i),
        );
        storage.record(entry).expect("Should record entry");
    }

    // 验证链完整性
    let is_valid = storage.verify().expect("Should verify");
    assert!(is_valid, "Chain should be valid after mass recording");

    // 验证条目数
    let recorder = storage.recorder();
    let count_in_storage = recorder.entry_count().expect("Should get count");
    assert_eq!(count_in_storage, count);
}

/// 测试：签名后的条目 JSON 导出
#[test]
fn test_signed_entry_json_export() {
    let storage = create_memory_storage().expect("Should create storage");

    let entry = AuditEntry::new(
        "user_hash",
        "session",
        "service",
        AuditAction::CredentialDecrypt,
        Outcome::Success,
        "mrenclave",
        "jti",
    );

    let signed = storage.record(entry).expect("Should record");

    let json = signed.to_json().expect("Should export to JSON");

    // 验证 JSON 包含关键字段
    assert!(json.contains("log_index"));
    assert!(json.contains("content_hash"));
    assert!(json.contains("prev_hash"));
    assert!(json.contains("signature"));
    assert!(json.contains("signer_fingerprint"));
    assert!(json.contains("merkle_root"));
}

/// 测试：审计条目导出
#[test]
fn test_audit_export_json() {
    let storage = create_memory_storage().expect("Should create storage");
    let recorder = storage.recorder();

    // 记录几个事件
    for i in 0..3 {
        let entry = AuditEntry::new(
            format!("user_{}", i),
            "session",
            "service",
            AuditAction::TokenValidate,
            Outcome::Success,
            "mrenclave",
            format!("jti_{}", i),
        );
        storage.record(entry).unwrap();
    }

    // 导出为 JSON
    let json = recorder.export_json().expect("Should export");
    assert!(!json.is_empty());

    // 应该能解析为 JSON 数组
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Should parse JSON");
    assert!(parsed.is_array());
}

/// 测试：审计日志的不可篡改性（通过链式哈希）
#[test]
fn test_audit_tamper_resistance() {
    let storage = create_memory_storage().expect("Should create storage");

    // 记录一系列事件
    for i in 0..5 {
        let entry = AuditEntry::new(
            format!("user_{}", i),
            "session",
            "service",
            AuditAction::TokenValidate,
            Outcome::Success,
            "mrenclave",
            format!("jti_{}", i),
        );
        storage.record(entry).unwrap();
    }

    // 验证初始状态
    let is_valid = storage.verify().expect("Should verify");
    assert!(is_valid, "Chain should be valid initially");

    // 每个条目都应该有其前一个条目的哈希
    let recent = storage.query_recent(5).expect("Should query");
    for i in 1..recent.len() {
        // 当前条目的 prev_hash 应该等于前一个条目的 content_hash
        assert_eq!(
            recent[i].prev_hash,
            recent[i - 1].content_hash,
            "Chain link {} should reference previous entry",
            i
        );
    }
}

/// 测试：带客户端信息的审计条目
#[test]
fn test_audit_entry_with_client_info() {
    let entry = AuditEntry::new(
        "user_hash",
        "session",
        "service",
        AuditAction::CredentialDecrypt,
        Outcome::Success,
        "mrenclave",
        "jti",
    )
    .with_client_ip_hash("hash_of_192.168.1.1")
    .with_user_agent_hash("hash_of_user_agent_string");

    assert_eq!(
        entry.client_ip_hash,
        Some("hash_of_192.168.1.1".to_string())
    );
    assert_eq!(
        entry.user_agent_hash,
        Some("hash_of_user_agent_string".to_string())
    );
}

/// 测试：审计记录器的公钥获取
#[test]
fn test_recorder_public_key() {
    let recorder = AuditRecorder::new(100).expect("Should create recorder");

    let public_key = recorder.public_key();
    assert!(!public_key.is_empty(), "Should have public key");

    let fingerprint = recorder.fingerprint();
    assert!(!fingerprint.is_empty(), "Should have fingerprint");
}

/// 测试：不同操作类型的审计记录
#[test]
fn test_various_audit_actions() {
    let storage = create_memory_storage().expect("Should create storage");

    let actions = vec![
        AuditAction::CredentialCreate,
        AuditAction::CredentialUpdate,
        AuditAction::CredentialDelete,
        AuditAction::TokenIssue,
        AuditAction::TokenRevoke,
        AuditAction::AuditQuery,
        AuditAction::TeeAttestation,
    ];

    for action in actions {
        let entry = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            action,
            Outcome::Success,
            "mrenclave",
            "jti",
        );
        let signed = storage.record(entry).expect("Should record");
        assert_eq!(signed.entry.action, action);
    }

    // 验证链完整性
    assert!(storage.verify().expect("Should verify"));
}
