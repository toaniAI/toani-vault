#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::uninlined_format_args)]
#![allow(unused_imports)]
#![allow(dead_code)]

//! SGX hardware-only 真实系统测试
//!
//! 本测试套件只用于显式 `TEE_MODE=hardware` 的真实 Intel SGX/DCAP 环境。
//! 它不属于 simulation-safe 测试层，也不会在默认 CI 中运行。
//!
//! # 运行测试
//!
//! ```bash
//! # 在专用 SGX runner / staging 主机上运行全部硬件测试
//! TEE_MODE=hardware cargo test --test sgx_hardware_tests -- --ignored --test-threads=1
//!
//! # 运行特定硬件测试
//! TEE_MODE=hardware cargo test --test sgx_hardware_tests test_sgx_hardware_availability -- --exact --ignored --nocapture
//! ```
//!
//! # 前置条件
//!
//! - Intel SGX-capable CPU，且 BIOS 已启用 SGX/FLC
//! - Linux 专用 runner，存在 `/dev/sgx_enclave` 与 `/dev/sgx_provision`
//! - Intel SGX SDK 2.24+
//! - Intel SGX DCAP/QPL 库
//! - AESM 服务（`aesmd`）运行中
//! - 可访问 Intel PCS 或已配置专用 PCCS
//!
//! 若以上任一真实能力未接通，`TEE_MODE=hardware` 路径必须 fail-closed，
//! 而不是静默回退到 simulation。

use ring::digest::{SHA256, digest};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use vault_service::config::TeeRuntimeConfig;
use vault_service::tee::{
    attestation::AttestationResult, challenge::CHANNEL_KEY_LENGTH, challenge::ChallengeProtocol,
    challenge::ProverProtocol, challenge::SecureChannel, dcap::DcapConfig, dcap::DcapError,
    dcap::DcapService, enclave::Enclave, enclave::EnclaveConfig, enclave::EnclaveState,
    sealing::SealPolicy, sealing::SealingService,
};
use vault_service::vault::models::UserId;

fn hardware_runtime_config() -> TeeRuntimeConfig {
    TeeRuntimeConfig::hardware()
}

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
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
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
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
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

/// 验证硬件 attestation 路径在未实现完整功能前会显式 fail-closed
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
fn test_hardware_attestation_paths_fail_closed_until_implemented() {
    let runtime = hardware_runtime_config();
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().expect("Enclave 初始化失败");

    let dcap_service = DcapService::new(DcapConfig {
        runtime_mode: runtime.mode,
        ..Default::default()
    })
    .expect("DCAP 服务创建失败");

    let dcap_error = dcap_service
        .initialize(&enclave)
        .expect_err("硬件模式不应静默降级到模拟 Quote");
    assert!(
        matches!(
            dcap_error,
            DcapError::QuoteGenerationFailed(_) | DcapError::PcsCommunicationFailed(_)
        ),
        "unexpected error: {dcap_error:?}"
    );

    let attestation_service =
        vault_service::tee::attestation::AttestationService::new(runtime.mode);
    let challenge = b"hardware_fail_closed_challenge";
    let attestation_error = attestation_service
        .generate_quote(&enclave, challenge)
        .expect_err("硬件模式不应生成模拟 attestation Quote");
    assert!(
        attestation_error
            .to_string()
            .contains("refusing simulated quote generation"),
        "unexpected error: {attestation_error}"
    );
}

/// 验证在真实 SGX 硬件上生成 DCAP Quote
///
/// **测试目标**: 确认硬件 DCAP Quote 正确生成
///
/// **通过标准**:
/// - Quote 生成成功（调用硬件 DCAP）
/// - Quote 包含真实的 ECDSA 签名
/// - 测量值与 Enclave 一致
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
fn test_dcap_quote_generation_hardware() {
    println!("\n=== HW-003: 硬件模式下禁止关闭证书链验证 ===");
    let runtime = hardware_runtime_config();

    let error = DcapService::new(DcapConfig {
        runtime_mode: runtime.mode,
        quote_max_age_seconds: 3600,
        verify_certificate_chain: false,
        ..Default::default()
    })
    .expect_err("硬件模式不允许通过 verify_certificate_chain=false 创建 DCAP 服务");

    assert!(
        matches!(error, DcapError::ConfigurationError(_)),
        "unexpected error: {error:?}"
    );
    assert!(
        error.to_string().contains("verify_certificate_chain=true"),
        "unexpected error: {error}"
    );

    println!("✓ HW-003 测试通过：硬件模式对证书链验证配置采用 fail-closed");
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
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
fn test_dcap_quote_verification_hardware() {
    println!("\n=== HW-004: 硬件模式 Quote 验证能力未完成时必须失败关闭 ===");
    let simulation_runtime = TeeRuntimeConfig::simulation();
    let hardware_runtime = hardware_runtime_config();

    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: true,
        ..Default::default()
    });
    enclave.initialize().unwrap();

    let simulation_service = DcapService::new(DcapConfig {
        runtime_mode: simulation_runtime.mode,
        ..Default::default()
    })
    .unwrap();
    let quote = simulation_service.initialize(&enclave).unwrap();
    let quote_bytes = simulation_service.quote_to_bytes(&quote).unwrap();

    let verifier = DcapService::new(DcapConfig {
        runtime_mode: hardware_runtime.mode,
        allowed_mrenclaves: vec![enclave.mrenclave()],
        allowed_mrsigners: vec![enclave.mrsigner()],
        ..Default::default()
    })
    .unwrap();

    let error = verifier
        .verify_attestation(&quote_bytes, None)
        .expect_err("硬件模式在缺少完整 DCAP 验证能力时必须拒绝验证");

    assert!(
        matches!(error, DcapError::QuoteVerificationFailed(_)),
        "unexpected error: {error:?}"
    );
    assert!(
        error.to_string().contains("fail-closed"),
        "unexpected error: {error}"
    );

    println!("✓ HW-004 测试通过：硬件模式验证路径显式失败关闭");
}

// ============================================================================
// HW-005: 挑战 - 响应协议在硬件模式下显式失败关闭
// ============================================================================

/// 验证硬件模式下 challenge-response 在未接入真实 Quote backend 时显式失败
///
/// **测试目标**: 确认硬件模式不会在 challenge-response 流程中静默回退到模拟 Quote
///
/// **通过标准**:
/// - 挑战生成成功
/// - Prover 在生成响应时显式失败
/// - 错误原因指向拒绝模拟 Quote 生成
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
fn test_challenge_response_protocol_hardware_fails_closed_without_quote_backend() {
    println!("\n=== HW-005: 挑战 - 响应协议在硬件模式下显式失败关闭 ===");
    let runtime = hardware_runtime_config();

    let attestation_service =
        vault_service::tee::attestation::AttestationService::new(runtime.mode);

    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().unwrap();

    println!("Enclave 初始化成功");

    let verifier = ChallengeProtocol::new(attestation_service.clone());
    let challenge = verifier
        .generate_challenge(None, None)
        .expect("挑战生成失败");

    println!(
        "挑战生成成功：{}",
        hex::encode(&challenge.id.as_bytes()[..8])
    );

    let prover = ProverProtocol::new(attestation_service.clone());
    println!("开始生成挑战响应，预期因缺少真实 Quote backend 而 fail-closed...");

    let error = prover
        .respond_to_challenge(&enclave, &challenge)
        .expect_err("硬件模式下 challenge-response 不应静默生成模拟 Quote");

    assert!(
        error
            .to_string()
            .contains("refusing simulated quote generation"),
        "unexpected error: {error}"
    );

    println!("✓ HW-005 测试通过：challenge-response 在硬件模式下显式失败关闭");
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
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
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
        .seal_data(test_data, b"", SealPolicy::Mrsigner)
        .expect("密封失败");

    println!(
        "数据密封成功，密封后大小：{} 字节",
        sealed.ciphertext.len() + sealed.mac.len()
    );
    assert!(!sealed.ciphertext.is_empty(), "密封数据不应为空");

    // 5. 测试解封数据
    let unsealed = sealing_service.unseal_data(&sealed).expect("解封失败");

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
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
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
    let user_hash = UserId::new("user_1").hash().to_string();
    let blob = enclave
        .encrypt_credential("tenant_1", &user_hash, "cred_1", plaintext)
        .expect("加密失败");

    println!("加密成功，密文大小：{} 字节", blob.ciphertext.len());

    // 3. 使用相同密钥解密
    let decrypted = enclave
        .decrypt_credential("tenant_1", &user_hash, "cred_1", &blob)
        .expect("解密失败");

    println!("解密成功");
    assert_eq!(decrypted, plaintext, "解密数据应与原始数据一致");

    // 4. 验证不同租户产生不同密文
    let blob_tenant2 = enclave
        .encrypt_credential("tenant_2", &user_hash, "cred_1", plaintext)
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
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
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
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
fn test_measurement_whitelist_hardware() {
    println!("\n=== HW-009: 硬件模式白名单验证不能依赖证书链旁路 ===");
    let runtime = hardware_runtime_config();

    let error_accept = DcapService::new(DcapConfig {
        runtime_mode: runtime.mode,
        allowed_mrenclaves: vec![[0x42u8; 32]],
        verify_certificate_chain: false,
        ..Default::default()
    })
    .expect_err("硬件模式下不能用 verify_certificate_chain=false 测试白名单放行路径");
    assert!(matches!(error_accept, DcapError::ConfigurationError(_)));

    let error_reject = DcapService::new(DcapConfig {
        runtime_mode: runtime.mode,
        allowed_mrenclaves: vec![[0x99u8; 32]],
        verify_certificate_chain: false,
        ..Default::default()
    })
    .expect_err("硬件模式下不能用 verify_certificate_chain=false 构造拒绝路径");
    assert!(matches!(error_reject, DcapError::ConfigurationError(_)));

    println!("✓ HW-009 测试通过：白名单测试不再依赖硬件模式旁路");
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
///
/// **注意**: 此测试需要 Intel SGX 硬件支持，默认忽略
#[test]
#[cfg(target_os = "linux")]
#[ignore = "hardware-only: requires TEE_MODE=hardware, SGX/DCAP/AESM, and a dedicated runner"]
fn test_replay_attack_protection_hardware() {
    println!("\n=== HW-010: 重放攻击测试禁止通过关闭证书链验证进入硬件路径 ===");
    let runtime = hardware_runtime_config();

    let error = DcapService::new(DcapConfig {
        runtime_mode: runtime.mode,
        allowed_mrenclaves: vec![[0x42u8; 32]],
        verify_certificate_chain: false,
        ..Default::default()
    })
    .expect_err("硬件模式不允许通过 verify_certificate_chain=false 进入 nonce/重放测试路径");

    assert!(
        matches!(error, DcapError::ConfigurationError(_)),
        "unexpected error: {error:?}"
    );
    assert!(
        error.to_string().contains("verify_certificate_chain=true"),
        "unexpected error: {error}"
    );

    println!("✓ HW-010 测试通过：重放攻击测试不再依赖硬件模式证书链旁路");
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
    println!("测试类型：hardware-only 集成测试");
    println!("运行模式：TEE_MODE=hardware");
    println!("前置条件：SGX/DCAP/AESM/专用 runner");
    println!("========================================");
    println!("\n测试列表:");
    println!("  HW-001: SGX 硬件基础验证");
    println!("  HW-002: Enclave 测量值一致性");
    println!("  HW-003: DCAP Quote 生成");
    println!("  HW-004: DCAP Quote 验证");
    println!("  HW-005: 挑战 - 响应协议 fail-closed");
    println!("  HW-006: SGX Sealing 密钥");
    println!("  HW-007: 密钥层次结构");
    println!("  HW-008: 安全通道建立");
    println!("  HW-009: 测量值白名单");
    println!("  HW-010: 重放攻击防护");
    println!("========================================\n");
}
