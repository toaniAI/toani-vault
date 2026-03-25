#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

//! 远程认证协议集成测试
//!
//! 测试 SGX DCAP 远程认证协议的完整流程，包括：
//! - Quote 生成和验证
//! - 挑战-响应协议
//! - 安全通道建立
//! - 测量值白名单验证
//! - 重放攻击防护

use vault_service::tee::attestation::{
    AttestationResult, AttestationService, AttestationSession, AttestationState, EcdsaSignature,
    Quote, ReportBody, ReportData, SGX_MEASUREMENT_LEN, SGX_REPORT_DATA_LEN,
};
use vault_service::tee::challenge::{
    CHALLENGE_LENGTH, CHANNEL_KEY_LENGTH, Challenge, ChallengeError, ChallengeProtocol,
    ChallengeStatus, ProverProtocol, SecureChannel,
};
use vault_service::tee::{Enclave, EnclaveConfig};

// ============ Quote 生成和验证测试 ============

#[test]
fn test_quote_generation_and_verification() {
    // 初始化 Enclave
    let config = EnclaveConfig {
        debug_mode: true,
        ..Default::default()
    };
    let mut enclave = Enclave::new(config);
    enclave.initialize().expect("Enclave initialization failed");

    // 创建认证服务
    let service = AttestationService::new()
        .allow_simulation(true)
        .allow_mrenclave(enclave.mrenclave())
        .allow_mrsigner(enclave.mrsigner());

    // 生成挑战
    let challenge = generate_test_challenge();

    // 生成 Quote
    let quote = service
        .generate_quote(&enclave, &challenge)
        .expect("Quote generation failed");

    // 验证 Quote 基本属性
    assert_eq!(quote.report_body.mrenclave, enclave.mrenclave());
    assert_eq!(quote.report_body.mrsigner, enclave.mrsigner());

    // 验证 Quote
    let identity = generate_enclave_identity(&enclave);
    let result = service
        .verify_quote(&quote, &challenge, &identity)
        .expect("Quote verification failed");

    assert!(result.success);
    assert_eq!(result.mrenclave, enclave.mrenclave());
    assert_eq!(result.mrsigner, enclave.mrsigner());
}

#[test]
fn test_quote_with_wrong_challenge_fails() {
    let config = EnclaveConfig {
        debug_mode: true,
        ..Default::default()
    };
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    let service = AttestationService::new().allow_simulation(true);

    let challenge = generate_test_challenge();
    let quote = service.generate_quote(&enclave, &challenge).unwrap();

    // 使用错误的挑战验证
    let wrong_challenge = generate_test_challenge();
    let identity = generate_enclave_identity(&enclave);

    let result = service.verify_quote(&quote, &wrong_challenge, &identity);
    assert!(result.is_err());
}

#[test]
fn test_quote_serialization_roundtrip() {
    let config = EnclaveConfig::default();
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    let service = AttestationService::new().allow_simulation(true);
    let challenge = generate_test_challenge();

    let quote = service.generate_quote(&enclave, &challenge).unwrap();

    // 序列化和反序列化
    let bytes = quote.to_bytes();
    let restored = Quote::from_bytes(&bytes).expect("Quote deserialization failed");

    // 验证所有字段
    assert_eq!(restored.version, quote.version);
    assert_eq!(restored.sign_type, quote.sign_type);
    assert_eq!(restored.report_body.mrenclave, quote.report_body.mrenclave);
    assert_eq!(restored.report_body.mrsigner, quote.report_body.mrsigner);
    assert_eq!(
        restored.report_body.report_data.data,
        quote.report_body.report_data.data
    );
}

// ============ 测量值白名单测试 ============

#[test]
fn test_measurement_whitelist_accept() {
    let config = EnclaveConfig::default();
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    // 使用白名单模式（测试使用模拟签名，因此允许模拟模式）
    let service = AttestationService::new()
        .allow_simulation(true)
        .allow_mrenclave(enclave.mrenclave());

    let challenge = generate_test_challenge();
    let quote = service.generate_quote(&enclave, &challenge).unwrap();

    let identity = generate_enclave_identity(&enclave);
    let result = service.verify_quote(&quote, &challenge, &identity);

    if let Err(e) = &result {
        eprintln!("验证失败：{:?}", e);
    }
    assert!(result.is_ok());
}

#[test]
fn test_measurement_whitelist_reject() {
    let config = EnclaveConfig::default();
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    // 使用错误的白名单
    let wrong_mrenclave = [0x99u8; SGX_MEASUREMENT_LEN];
    let service = AttestationService::new()
        .allow_simulation(false)
        .allow_mrenclave(wrong_mrenclave);

    let challenge = generate_test_challenge();
    let quote = service.generate_quote(&enclave, &challenge).unwrap();

    let identity = generate_enclave_identity(&enclave);
    let result = service.verify_quote(&quote, &challenge, &identity);

    assert!(result.is_err());
}

#[test]
fn test_mrsigner_whitelist_accept() {
    let config = EnclaveConfig::default();
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    // 使用白名单模式（测试使用模拟签名，因此允许模拟模式）
    let service = AttestationService::new()
        .allow_simulation(true)
        .allow_mrsigner(enclave.mrsigner());

    let challenge = generate_test_challenge();
    let quote = service.generate_quote(&enclave, &challenge).unwrap();

    let identity = generate_enclave_identity(&enclave);
    let result = service.verify_quote(&quote, &challenge, &identity);

    assert!(result.is_ok());
}

// ============ 挑战-响应协议测试 ============

#[test]
fn test_challenge_response_full_flow() {
    // 初始化 Enclave
    let config = EnclaveConfig::default();
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    // 设置认证服务
    let attestation_service = AttestationService::new()
        .allow_simulation(true)
        .allow_mrenclave(enclave.mrenclave());

    // 创建 Verifier 和 Prover
    let verifier = ChallengeProtocol::new(attestation_service.clone());
    let prover = ProverProtocol::new(attestation_service);

    // 1. Verifier 生成挑战
    let challenge = verifier.generate_challenge(None, None).unwrap();
    assert_eq!(challenge.status, ChallengeStatus::Pending);

    // 2. Prover 响应挑战
    let response = prover
        .respond_to_challenge(&enclave, &challenge)
        .expect("Challenge response failed");

    // 3. Verifier 验证响应
    let identity = generate_enclave_identity(&enclave);
    let result = verifier
        .verify_response(&response, &identity)
        .expect("Response verification failed");

    assert!(result.success);
}

#[test]
fn test_challenge_expiration() {
    let config = EnclaveConfig::default();
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    let attestation_service = AttestationService::new().allow_simulation(true);
    let verifier = ChallengeProtocol::new(attestation_service.clone()).with_ttl(1); // 1秒过期
    let prover = ProverProtocol::new(attestation_service);

    let challenge = verifier.generate_challenge(None, None).unwrap();

    // 等待过期
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 响应挑战
    let response = prover.respond_to_challenge(&enclave, &challenge).unwrap();
    let identity = generate_enclave_identity(&enclave);

    // 尝试验证应该失败（挑战已过期）
    let result = verifier.verify_response(&response, &identity);
    assert!(matches!(
        result.unwrap_err(),
        ChallengeError::ChallengeNotFound | ChallengeError::ChallengeExpired
    ));
}

#[test]
fn test_challenge_replay_protection() {
    let config = EnclaveConfig::default();
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    let attestation_service = AttestationService::new().allow_simulation(true);
    let verifier = ChallengeProtocol::new(attestation_service.clone());
    let prover = ProverProtocol::new(attestation_service);

    // 生成挑战
    let challenge = verifier.generate_challenge(None, None).unwrap();

    // 第一次响应
    let response = prover.respond_to_challenge(&enclave, &challenge).unwrap();
    let identity = generate_enclave_identity(&enclave);

    let result = verifier.verify_response(&response, &identity);
    assert!(result.is_ok());

    // 第二次响应（重放攻击）应该失败
    let result = verifier.verify_response(&response, &identity);
    assert!(matches!(
        result.unwrap_err(),
        ChallengeError::ChallengeNotFound | ChallengeError::ChallengeAlreadyUsed
    ));
}

#[test]
fn test_concurrent_challenges() {
    let attestation_service = AttestationService::new();
    let verifier = ChallengeProtocol::new(attestation_service).with_max_challenges(100);

    // 创建多个挑战
    let mut challenge_ids = Vec::new();
    for _ in 0..50 {
        let challenge = verifier.generate_challenge(None, None).unwrap();
        challenge_ids.push(challenge.id);
    }

    assert_eq!(verifier.active_challenge_count(), 50);

    // 验证所有挑战都存在
    for id in &challenge_ids {
        assert!(verifier.get_challenge(id).is_some());
    }
}

// ============ 安全通道测试 ============

#[test]
fn test_secure_channel_establishment() {
    let config = EnclaveConfig::default();
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    let attestation_result = AttestationResult {
        success: true,
        mrenclave: enclave.mrenclave(),
        mrsigner: enclave.mrsigner(),
        timestamp: current_timestamp(),
    };

    let channel =
        SecureChannel::establish(&attestation_result, "test_channel".to_string(), 3600).unwrap();

    assert_eq!(channel.channel_id, "test_channel");
    assert_eq!(channel.session_key.len(), CHANNEL_KEY_LENGTH);
    assert!(channel.is_valid());
    assert!(channel.properties.encrypted);
    assert!(channel.properties.authenticated);
}

#[test]
fn test_secure_channel_key_derivation() {
    let result1 = AttestationResult {
        success: true,
        mrenclave: [0x01u8; 32],
        mrsigner: [0x02u8; 32],
        timestamp: 1234567890,
    };

    let result2 = AttestationResult {
        success: true,
        mrenclave: [0x02u8; 32],
        mrsigner: [0x01u8; 32],
        timestamp: 1234567890,
    };

    let channel1 = SecureChannel::establish(&result1, "chan1".to_string(), 3600).unwrap();
    let channel2 = SecureChannel::establish(&result2, "chan2".to_string(), 3600).unwrap();

    // 不同的认证结果应该产生不同的会话密钥
    assert_ne!(channel1.session_key, channel2.session_key);
}

#[test]
fn test_secure_channel_expiration() {
    let result = AttestationResult {
        success: true,
        mrenclave: [0u8; 32],
        mrsigner: [0u8; 32],
        timestamp: current_timestamp(),
    };

    // 创建 0 TTL 的通道
    let channel = SecureChannel::establish(&result, "test".to_string(), 0).unwrap();

    // 应该立即过期
    assert!(!channel.is_valid());
}

// ============ Report Data 绑定测试 ============

#[test]
fn test_report_data_challenge_binding() {
    let challenge = b"test_challenge_data";
    let identity = b"test_enclave_identity";

    let report_data = ReportData::from_challenge(challenge, identity);

    // 验证正确的绑定
    assert!(report_data.verify_binding(challenge, identity));

    // 错误的挑战应该失败
    let wrong_challenge = b"wrong_challenge";
    assert!(!report_data.verify_binding(wrong_challenge, identity));

    // 错误的身份应该失败
    let wrong_identity = b"wrong_identity";
    assert!(!report_data.verify_binding(challenge, wrong_identity));
}

#[test]
fn test_report_data_empty() {
    let empty = ReportData::empty();

    for i in 0..SGX_REPORT_DATA_LEN {
        assert_eq!(empty.data[i], 0);
    }
}

// ============ 状态机测试 ============

#[test]
fn test_attestation_session_state_machine() {
    let session =
        AttestationSession::new("session_123".to_string(), 300).expect("Session creation failed");

    assert_eq!(session.state, AttestationState::ChallengeCreated);
    assert!(!session.is_expired());
}

#[test]
fn test_challenge_state_transitions() {
    let mut challenge =
        Challenge::new("test_123".to_string(), 300).expect("Challenge creation failed");

    assert_eq!(challenge.status, ChallengeStatus::Pending);

    // Pending -> Responded
    challenge.mark_responded().unwrap();
    assert_eq!(challenge.status, ChallengeStatus::Responded);

    // Responded -> Verified
    challenge.mark_verified().unwrap();
    assert_eq!(challenge.status, ChallengeStatus::Verified);

    // Verified -> (no transition allowed)
    assert!(challenge.mark_responded().is_err());
}

#[test]
fn test_invalid_state_transitions() {
    let mut challenge = Challenge::new("test_123".to_string(), 300).unwrap();

    // 不能直接从 Pending -> Verified
    assert!(challenge.mark_verified().is_err());

    // 不能从 Pending -> Failed
    assert!(challenge.mark_failed().is_err());

    // 正常流程
    challenge.mark_responded().unwrap();

    // Verified 后不能再转换
    challenge.mark_verified().unwrap();
    assert!(challenge.mark_failed().is_err());
}

// ============ 并发安全测试 ============

#[test]
fn test_challenge_cleanup() {
    let attestation_service = AttestationService::new();
    let verifier = ChallengeProtocol::new(attestation_service);

    // 创建挑战
    let challenge1 = verifier.generate_challenge(None, None).unwrap();
    let challenge2 = verifier.generate_challenge(None, None).unwrap();

    assert_eq!(verifier.active_challenge_count(), 2);

    // 取消一个
    verifier.cancel_challenge(&challenge1.id).unwrap();
    assert_eq!(verifier.active_challenge_count(), 1);
    assert!(verifier.get_challenge(&challenge1.id).is_none());
    assert!(verifier.get_challenge(&challenge2.id).is_some());
}

#[test]
fn test_challenge_limit_enforcement() {
    let attestation_service = AttestationService::new();
    let verifier = ChallengeProtocol::new(attestation_service).with_max_challenges(3);

    // 创建 3 个挑战
    let _ = verifier.generate_challenge(None, None).unwrap();
    let _ = verifier.generate_challenge(None, None).unwrap();
    let _ = verifier.generate_challenge(None, None).unwrap();

    assert_eq!(verifier.active_challenge_count(), 3);

    // 第 4 个应该失败
    let result = verifier.generate_challenge(None, None);
    assert!(matches!(
        result.unwrap_err(),
        ChallengeError::SessionLimitExceeded
    ));
}

// ============ 辅助函数 ============

fn generate_test_challenge() -> Vec<u8> {
    use rand::RngCore;

    let mut challenge = vec![0u8; CHALLENGE_LENGTH];
    let mut rng = rand::thread_rng();
    rng.fill_bytes(&mut challenge);
    challenge
}

fn generate_enclave_identity(enclave: &Enclave) -> Vec<u8> {
    let mut identity = Vec::with_capacity(64);
    identity.extend_from_slice(&enclave.mrenclave());
    identity.extend_from_slice(&enclave.mrsigner());
    identity
}

fn create_mock_quote() -> Quote {
    Quote::new(
        ReportBody {
            cpusvn: [0u8; 16],
            miscselect: 0,
            reserved1: [0u8; 12],
            isvextprodid: [0u8; 16],
            attributes: [0u8; 16],
            mrenclave: [0u8; 32],
            reserved2: [0u8; 32],
            mrsigner: [0u8; 32],
            reserved3: [0u8; 96],
            isvprodid: 0,
            isvsvn: 0,
            reserved4: [0u8; 60],
            report_data: ReportData::empty(),
        },
        vault_service::tee::attestation::QuoteSignature {
            isv_enclave_report_signature: EcdsaSignature::new([0u8; 32], [0u8; 32]),
            qe_report: vec![0u8; 384],
            qe_report_signature: EcdsaSignature::new([0u8; 32], [0u8; 32]),
            qe_authentication_data: vec![0u8; 32],
            qe_certification_data: vec![0u8; 64],
        },
    )
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
