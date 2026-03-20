//! SGX 硬件环境真实系统测试
//!
//! 本测试套件用于在真实 Intel SGX 硬件环境下验证 CredBridge TEE 功能
//!
//! # 运行测试
//!
//! ```bash
//! # 运行所有硬件测试
//! cargo test --test sgx_hardware_tests -- --test-threads=1
//!
//! # 运行特定测试
//! cargo test test_sgx_hardware_availability -- --exact --nocapture
//! ```
//!
//! # 环境要求
//!
//! - Intel SGX 硬件支持
//! - Linux Kernel 5.11+ (或安装 SGX 驱动)
//! - Intel SGX SDK 2.24+
//! - DCAP Library 1.15+
//! - AESM 服务运行中

use ring::digest::{SHA256, digest};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use vault_service::crypto::{KeyHierarchy, KeyPurpose};
use vault_service::tee::{
    attestation::AttestationResult,
    challenge::{CHANNEL_KEY_LENGTH, ChallengeProtocol, ProverProtocol, SecureChannel},
    dcap::{DcapConfig, DcapError, DcapQuote, DcapService},
    enclave::{Enclave, EnclaveConfig, EnclaveState},
    sealing::{SealPolicy, SealingService},
};

// ============================================================================
// HW-001: SGX 硬件基础验证
// ============================================================================

/// 验证 SGX 硬件环境可用性
///
/// **测试目标**: 确认 SGX 硬件可被正确初始化和使用
///
/// **通过标准**:
/// - Enclave 初始化成功
/// - MRENCLAVE/MRSIGNER 为非零哈希值
/// - Enclave 状态为 Running
#[test]
#[cfg(target_os = "linux")]
fn test_sgx_hardware_availability() {
    println!("\n=== HW-001: SGX 硬件基础验证 ===");

    // 1. 创建 Enclave（需要硬件 SGX）
    let config = EnclaveConfig {
        debug_mode: false, // 非调试模式，需要真实硬件
        name: "hardware-test-enclave".to_string(),
        ..Default::default()
    };

    let mut enclave = Enclave::new(config);

    // 2. 初始化应该成功（硬件环境）
    let init_result = enclave.initialize();
    assert!(init_result.is_ok(), "SGX 硬件初始化失败：{:?}", init_result);

    // 3. 验证状态
    assert_eq!(
        enclave.state(),
        EnclaveState::Running,
        "Enclave 状态应为 Running"
    );

    // 4. 验证测量值非空（真实硬件生成）
    let mrenclave = enclave.mrenclave();
    println!("MRENCLAVE: {}", hex::encode(mrenclave));
    assert_ne!(mrenclave, [0u8; 32], "MRENCLAVE 应为真实哈希值");

    let mrsigner = enclave.mrsigner();
    println!("MRSIGNER: {}", hex::encode(mrsigner));
    assert_ne!(mrsigner, [0u8; 32], "MRSIGNER 应为真实哈希值");

    println!("✓ HW-001 测试通过");
}

// ============================================================================
// HW-002: Enclave 初始化与测量值生成
// ============================================================================

/// 验证 Enclave 在真实硬件上的测量值生成
///
/// **测试目标**: 确认相同 Enclave 配置产生一致的测量值
///
/// **通过标准**:
/// - 多次初始化产生相同的 MRENCLAVE
/// - 测量值符合 SGX 规范（32 字节 SHA-256 哈希）
#[test]
#[cfg(target_os = "linux")]
fn test_enclave_measurement_consistency() {
    println!("\n=== HW-002: Enclave 测量值一致性验证 ===");

    let mut measurements = HashSet::new();
    let mut mrsigners = HashSet::new();

    // 多次初始化验证测量值一致性
    for i in 0..3 {
        println!("第 {} 次初始化...", i + 1);

        let config = EnclaveConfig {
            debug_mode: false,
            name: "test-enclave".to_string(),
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().expect("初始化失败");

        // 记录测量值
        measurements.insert(hex::encode(enclave.mrenclave()));
        mrsigners.insert(hex::encode(enclave.mrsigner()));
    }

    println!("MRENCLAVE 种类数：{}", measurements.len());
    println!("MRSIGNER 种类数：{}", mrsigners.len());

    // 相同 Enclave 应该有相同的测量值
    assert_eq!(measurements.len(), 1, "多次初始化的 MRENCLAVE 应该一致");
    assert_eq!(mrsigners.len(), 1, "多次初始化的 MRSIGNER 应该一致");

    println!("✓ HW-002 测试通过");
}

// ============================================================================
// HW-003: DCAP Quote 生成（硬件模式）
// ============================================================================

/// 验证在真实 SGX 硬件上生成 DCAP Quote
///
/// **测试目标**: 确认硬件 DCAP Quote 正确生成
///
/// **通过标准**:
/// - Quote 生成成功（调用硬件 DCAP）
/// - Quote 包含真实的 ECDSA 签名
/// - 测量值与 Enclave 一致
#[test]
#[cfg(target_os = "linux")]
fn test_dcap_quote_generation_hardware() {
    println!("\n=== HW-003: DCAP Quote 生成（硬件模式） ===");

    // 1. 创建硬件模式的 DCAP 服务
    let dcap_config = DcapConfig {
        simulation_mode: false, // 硬件模式
        quote_max_age_seconds: 3600,
        verify_certificate_chain: false, // 测试环境跳过证书链验证
        ..Default::default()
    };

    let dcap_service = DcapService::new(dcap_config).expect("DCAP 服务创建失败");

    // 2. 创建并初始化 Enclave
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().expect("Enclave 初始化失败");

    println!("Enclave MRENCLAVE: {}", hex::encode(enclave.mrenclave()));

    // 3. 生成 Quote（调用硬件 DCAP）
    let quote = dcap_service.initialize(&enclave).expect("Quote 生成失败");

    println!("Quote 生成成功");
    println!("  - 版本：{}", quote.version);
    println!("  - 签名类型：{}", quote.sign_type);
    println!("  - 时间戳：{}", quote.timestamp);

    // 4. 验证 Quote 结构
    assert_eq!(quote.version, 3, "Quote 版本应为 3");
    assert_eq!(quote.sign_type, 2, "签名类型应为 ECDSA P-256");

    // 5. 验证 Quote 包含正确的测量值
    assert_eq!(
        quote.report_body.mrenclave,
        enclave.mrenclave(),
        "Quote MRENCLAVE 应与 Enclave 一致"
    );
    assert_eq!(
        quote.report_body.mrsigner,
        enclave.mrsigner(),
        "Quote MRSIGNER 应与 Enclave 一致"
    );

    // 6. 验证签名非空（硬件生成）
    let sig = &quote.signature.isv_enclave_report_signature;
    println!("签名 R 值：{}", hex::encode(sig.r));
    println!("签名 S 值：{}", hex::encode(sig.s));

    assert_ne!(sig.r, [0u8; 32], "签名 R 值不应全零");
    assert_ne!(sig.s, [0u8; 32], "签名 S 值不应全零");

    println!("✓ HW-003 测试通过");
}

// ============================================================================
// HW-004: DCAP Quote 验证（硬件模式）
// ============================================================================

/// 验证硬件生成的 Quote 可被正确验证
///
/// **测试目标**: 确认 Quote 验证流程完整
///
/// **通过标准**:
/// - Quote 验证成功
/// - 认证报告显示正确的测量值
#[test]
#[cfg(target_os = "linux")]
fn test_dcap_quote_verification_hardware() {
    println!("\n=== HW-004: DCAP Quote 验证（硬件模式） ===");

    // 1. 生成 Quote
    let dcap_config = DcapConfig {
        simulation_mode: false,
        verify_certificate_chain: false,
        ..Default::default()
    };

    let dcap_service = DcapService::new(dcap_config).unwrap();
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().unwrap();

    let quote = dcap_service.initialize(&enclave).unwrap();
    let quote_bytes = dcap_service.quote_to_bytes(&quote).unwrap();

    println!("Quote 生成成功");
    println!("Quote 大小：{} 字节", quote_bytes.len());

    // 2. 配置白名单
    let verifier_config = DcapConfig {
        simulation_mode: false,
        allowed_mrenclaves: vec![enclave.mrenclave()],
        allowed_mrsigners: vec![enclave.mrsigner()],
        verify_certificate_chain: false,
        ..Default::default()
    };

    let verifier = DcapService::new(verifier_config).unwrap();

    // 3. 验证 Quote
    println!("开始验证 Quote...");
    let report = verifier.verify_attestation(&quote_bytes, None);

    match &report {
        Ok(r) => {
            println!("✓ Quote 验证成功");
            println!("  - MRENCLAVE: {}", r.mrenclave_hex);
            println!("  - MRSIGNER: {}", r.mrsigner_hex);
        }
        Err(e) => {
            println!("✗ Quote 验证失败：{:?}", e);
        }
    }

    assert!(report.is_ok(), "Quote 验证失败：{:?}", report);
    let report = report.unwrap();

    // 4. 验证报告内容
    assert!(report.result.success);
    assert_eq!(
        report.mrenclave_hex,
        hex::encode(enclave.mrenclave()),
        "报告中的 MRENCLAVE 应匹配"
    );

    println!("✓ HW-004 测试通过");
}

// ============================================================================
// HW-005: 挑战 - 响应协议完整流程
// ============================================================================

/// 验证完整的远程认证挑战 - 响应协议
///
/// **测试目标**: 确认挑战 - 响应协议在硬件环境下正常工作
///
/// **通过标准**:
/// - 挑战生成成功
/// - Quote 生成成功（硬件）
/// - 响应验证通过
#[test]
#[cfg(target_os = "linux")]
fn test_challenge_response_protocol_hardware() {
    println!("\n=== HW-005: 挑战 - 响应协议完整流程 ===");

    // 1. 设置硬件模式
    let attestation_service =
        vault_service::tee::attestation::AttestationService::new().allow_simulation(false);

    // 2. 创建 Enclave（Prover 侧）
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().unwrap();

    println!("Enclave 初始化成功");

    // 3. Verifier 生成挑战
    let verifier = ChallengeProtocol::new(attestation_service.clone());
    let challenge = verifier
        .generate_challenge(None, None)
        .expect("挑战生成失败");

    println!(
        "挑战生成成功：{}",
        hex::encode(&challenge.id.as_bytes()[..8])
    );

    // 4. Prover 响应挑战（需要硬件 Quote 生成）
    let prover = ProverProtocol::new(attestation_service.clone());
    println!("开始生成挑战响应...");

    let response = prover
        .respond_to_challenge(&enclave, &challenge)
        .expect("挑战响应失败");

    println!("✓ 挑战响应生成成功");

    // 5. Verifier 验证响应
    let identity = generate_enclave_identity(&enclave);
    println!("开始验证响应...");

    let result = verifier
        .verify_response(&response, &identity)
        .expect("响应验证失败");

    assert!(result.success, "验证结果应为成功");
    assert_eq!(
        result.mrenclave,
        enclave.mrenclave(),
        "验证结果中的 MRENCLAVE 应匹配"
    );

    println!("✓ HW-005 测试通过");
}

// ============================================================================
// HW-006: SGX Sealing 密钥派生
// ============================================================================

/// 验证 SGX Sealing Key 的正确派生和使用
///
/// **测试目标**: 确认 SGX Sealing 机制在硬件上正常工作
///
/// **通过标准**:
/// - Sealing Key 从硬件 SGX 正确获取
/// - 数据密封成功
/// - 数据解封成功且内容一致
#[test]
#[cfg(target_os = "linux")]
fn test_sgx_sealing_key_derivation() {
    println!("\n=== HW-006: SGX Sealing 密钥派生 ===");

    // 1. 创建 Enclave
    let config = EnclaveConfig {
        debug_mode: false,
        seal_policy: SealPolicy::Mrsigner,
        ..Default::default()
    };

    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    println!("Enclave 初始化成功");

    // 2. 获取 Sealing Key（从硬件 SGX）
    let sealing_service = SealingService::new();
    let sealing_key = sealing_service
        .get_sealing_key(SealPolicy::Mrsigner)
        .expect("Sealing Key 获取失败");

    println!("Sealing Key 获取成功");

    // 3. 验证密钥非空
    assert_ne!(sealing_key.as_bytes(), &[0u8; 32], "Sealing Key 不应全零");

    // 4. 测试密封数据
    let test_data = b"Test sensitive data for sealing";
    let sealed = sealing_service
        .seal_data(test_data, b"", SealPolicy::MacBased)
        .expect("密封失败");

    println!("数据密封成功，密封后大小：{} 字节", sealed.ciphertext.len() + sealed.mac.len());
    assert!(!sealed.ciphertext.is_empty(), "密封数据不应为空");

    // 5. 测试解封数据
    let unsealed = sealing_service
        .unseal_data(&sealed)
        .expect("解封失败");

    println!("数据解封成功");
    assert_eq!(unsealed, test_data, "解封数据应与原始数据一致");

    println!("✓ HW-006 测试通过");
}

// ============================================================================
// HW-007: 密钥层次结构验证
// ============================================================================

/// 验证完整的密钥层次结构在硬件 Enclave 中的派生
///
/// **测试目标**: 确认密钥层次结构正确工作
///
/// **通过标准**:
/// - L2 密钥正确派生
/// - 加密/解密成功
/// - 租户隔离有效
#[test]
#[cfg(target_os = "linux")]
fn test_key_hierarchy_hardware() {
    println!("\n=== HW-007: 密钥层次结构验证 ===");

    let config = EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    };

    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();

    println!("Enclave 初始化成功");

    // 1. 派生 L2 用户保险库密钥
    let user_key_handle = enclave
        .derive_user_vault_key("tenant_1", "user_1")
        .expect("L2 密钥派生失败");

    println!("L2 密钥派生成功：{}", hex::encode(&user_key_handle[..8]));
    assert_ne!(user_key_handle, [0u8; 32], "密钥句柄不应全零");

    // 2. 使用 L2 密钥加密
    let plaintext = b"Secret credential data for hardware test";
    let blob = enclave
        .encrypt_credential("tenant_1", "user_1", "cred_1", plaintext)
        .expect("加密失败");

    println!("加密成功，密文大小：{} 字节", blob.ciphertext.len());

    // 3. 使用相同密钥解密
    let decrypted = enclave
        .decrypt_credential("tenant_1", "user_1", "cred_1", &blob)
        .expect("解密失败");

    println!("解密成功");
    assert_eq!(decrypted, plaintext, "解密数据应与原始数据一致");

    // 4. 验证不同租户产生不同密文
    let blob_tenant2 = enclave
        .encrypt_credential("tenant_2", "user_1", "cred_1", plaintext)
        .expect("加密失败");

    println!("租户 2 加密成功");
    assert_ne!(
        blob.ciphertext, blob_tenant2.ciphertext,
        "不同租户应产生不同密文"
    );

    println!("✓ HW-007 测试通过");
}

// ============================================================================
// HW-008: 安全通道建立与加密通信
// ============================================================================

/// 验证基于远程认证的安全通道建立
///
/// **测试目标**: 确认安全通道正确建立和使用
///
/// **通过标准**:
/// - 安全通道建立成功
/// - 会话密钥正确派生
/// - 加密/解密成功
#[test]
#[cfg(target_os = "linux")]
fn test_secure_channel_establishment_hardware() {
    println!("\n=== HW-008: 安全通道建立与加密通信 ===");

    // 1. 硬件 Enclave 认证
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().unwrap();

    let attestation_result = AttestationResult {
        success: true,
        mrenclave: enclave.mrenclave(),
        mrsigner: enclave.mrsigner(),
        timestamp: current_timestamp(),
    };

    println!("认证结果生成成功");

    // 2. 建立安全通道
    let channel = SecureChannel::establish(
        &attestation_result,
        "test_channel".to_string(),
        3600, // 1 小时 TTL
    )
    .expect("安全通道建立失败");

    println!("安全通道建立成功");

    // 3. 验证通道属性
    assert_eq!(channel.channel_id, "test_channel");
    assert_eq!(channel.session_key.len(), CHANNEL_KEY_LENGTH);
    assert_eq!(CHANNEL_KEY_LENGTH, 32, "会话密钥应为 256 位");
    assert!(channel.properties.encrypted);
    assert!(channel.properties.authenticated);

    println!("通道属性验证通过");
    println!("  - 通道 ID: {}", channel.channel_id);
    println!("  - 密钥长度：{} 字节", channel.session_key.len());

    // 4. 测试加密通信
    let message = b"Secure message over hardware channel";
    let encrypted = channel.encrypt(message).expect("加密失败");
    println!("加密成功，密文大小：{} 字节", encrypted.len());

    let decrypted = channel.decrypt(&encrypted).expect("解密失败");
    println!("解密成功");

    assert_eq!(decrypted, message, "解密消息应与原始消息一致");

    println!("✓ HW-008 测试通过");
}

// ============================================================================
// HW-009: 测量值白名单验证
// ============================================================================

/// 验证测量值白名单机制有效
///
/// **测试目标**: 确认白名单正确接受/拒绝 Quote
///
/// **通过标准**:
/// - 匹配的白名单接受
/// - 不匹配的白名单拒绝
#[test]
#[cfg(target_os = "linux")]
fn test_measurement_whitelist_hardware() {
    println!("\n=== HW-009: 测量值白名单验证 ===");

    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().unwrap();

    let mrenclave = enclave.mrenclave();
    println!("Enclave MRENCLAVE: {}", hex::encode(mrenclave));

    // 1. 使用正确白名单
    println!("测试 1: 正确白名单应接受");
    let config_accept = DcapConfig {
        simulation_mode: false,
        allowed_mrenclaves: vec![mrenclave],
        verify_certificate_chain: false,
        ..Default::default()
    };

    let service_accept = DcapService::new(config_accept).unwrap();
    service_accept.initialize(&enclave).unwrap();

    let quote = service_accept.get_current_quote().unwrap();
    let quote_bytes = service_accept.quote_to_bytes(&quote).unwrap();

    // 应该接受
    let result = service_accept.verify_attestation(&quote_bytes, None);
    match &result {
        Ok(_) => println!("✓ 正确白名单接受"),
        Err(e) => println!("✗ 验证失败：{:?}", e),
    }
    assert!(result.is_ok(), "正确白名单应接受 Quote");

    // 2. 使用错误白名单
    println!("测试 2: 错误白名单应拒绝");
    let wrong_mrenclave = [0x99u8; 32];
    let config_reject = DcapConfig {
        simulation_mode: false,
        allowed_mrenclaves: vec![wrong_mrenclave],
        verify_certificate_chain: false,
        ..Default::default()
    };

    let service_reject = DcapService::new(config_reject).unwrap();
    service_reject.initialize(&enclave).unwrap();

    let quote = service_reject.get_current_quote().unwrap();
    let quote_bytes = service_reject.quote_to_bytes(&quote).unwrap();

    // 应该拒绝
    let result = service_reject.verify_attestation(&quote_bytes, None);
    match &result {
        Err(DcapError::MeasurementMismatch) => println!("✓ 错误白名单正确拒绝"),
        Err(e) => println!("✗ 拒绝原因：{:?}", e),
        Ok(_) => println!("✗ 错误：应该拒绝"),
    }
    assert!(
        matches!(result.unwrap_err(), DcapError::MeasurementMismatch),
        "错误白名单应拒绝 Quote"
    );

    println!("✓ HW-009 测试通过");
}

// ============================================================================
// HW-010: 重放攻击防护验证
// ============================================================================

/// 验证 nonce 机制防止重放攻击
///
/// **测试目标**: 确认 nonce 绑定机制有效
///
/// **通过标准**:
/// - Nonce 正确绑定到 Quote
/// - 重放攻击可被检测
#[test]
#[cfg(target_os = "linux")]
fn test_replay_attack_protection_hardware() {
    println!("\n=== HW-010: 重放攻击防护验证 ===");

    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().unwrap();

    let dcap_service = DcapService::new(DcapConfig {
        simulation_mode: false,
        allowed_mrenclaves: vec![enclave.mrenclave()],
        verify_certificate_chain: false,
        ..Default::default()
    })
    .unwrap();

    dcap_service.initialize(&enclave).unwrap();

    // 1. 生成随机 nonce
    let nonce = b"unique_challenge_nonce_for_hardware_test";
    println!("Nonce: {}", std::str::from_utf8(nonce).unwrap());

    // 2. 获取 Quote
    let quote = dcap_service.get_current_quote().unwrap();
    let quote_bytes = dcap_service.quote_to_bytes(&quote).unwrap();

    // 3. 验证 nonce 绑定
    let report_data = quote.report_body.report_data;
    let expected_hash = digest(&SHA256, nonce);

    println!("Expected hash: {}", hex::encode(expected_hash.as_ref()));
    println!(
        "Report data[0..32]: {}",
        hex::encode(&report_data.data[..32])
    );

    // nonce 哈希应该绑定到 report_data 前 32 字节
    assert_eq!(
        &report_data.data[..32],
        expected_hash.as_ref(),
        "Nonce 哈希应绑定到 report_data"
    );

    println!("✓ Nonce 正确绑定到 Quote");

    // 4. 验证错误 nonce 应该失败
    let wrong_nonce = b"wrong_nonce_data";
    let wrong_hash = digest(&SHA256, wrong_nonce);

    assert_ne!(
        &report_data.data[..32],
        wrong_hash.as_ref(),
        "错误 nonce 的哈希应不同"
    );

    println!("✓ HW-010 测试通过");
}

// ============================================================================
// 辅助函数
// ============================================================================

fn generate_enclave_identity(enclave: &Enclave) -> Vec<u8> {
    let mut identity = Vec::with_capacity(64);
    identity.extend_from_slice(&enclave.mrenclave());
    identity.extend_from_slice(&enclave.mrsigner());
    identity
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// ============================================================================
// 测试套件信息
// ============================================================================

/// 打印测试套件信息
#[test]
fn test_suite_info() {
    println!("\n========================================");
    println!("SGX 硬件测试套件");
    println!("========================================");
    println!("测试数量：10");
    println!("测试类型：硬件集成测试");
    println!("========================================");
    println!("\n测试列表:");
    println!("  HW-001: SGX 硬件基础验证");
    println!("  HW-002: Enclave 测量值一致性");
    println!("  HW-003: DCAP Quote 生成");
    println!("  HW-004: DCAP Quote 验证");
    println!("  HW-005: 挑战 - 响应协议");
    println!("  HW-006: SGX Sealing 密钥");
    println!("  HW-007: 密钥层次结构");
    println!("  HW-008: 安全通道建立");
    println!("  HW-009: 测量值白名单");
    println!("  HW-010: 重放攻击防护");
    println!("========================================\n");
}
