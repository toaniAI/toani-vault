# SGX 硬件环境真实系统测试方案

## 一、测试目标

在 Intel SGX 硬件环境下验证 CredBridge TEE 功能的完整性和安全性，确保：

1. **Quote 生成与验证**：DCAP Quote 在真实硬件上正确生成和验证
2. **远程认证协议**：挑战 - 响应协议在硬件 Enclave 中正常工作
3. **密钥层次结构**：SGX Sealing Key 正确派生和保护密钥
4. **安全通道建立**：基于认证的加密通道正确建立
5. **逃逸防护**：Enclave 隔离机制有效防止逃逸攻击

---

## 二、测试环境要求

### 2.1 硬件要求

| 组件 | 最低要求 | 推荐配置 |
|------|---------|---------|
| CPU | Intel 第 6 代酷睿 | Intel 第 10 代酷睿或更新 |
| SGX 支持 | FLC (Flexible Launch Control) | SGX2 (可编程 Enclave) |
| 内存 | 4GB | 8GB+ |
| BIOS 设置 | SGX Enabled | SGX + Virtualization Enabled |

### 2.2 软件要求

| 组件 | 版本 | 说明 |
|------|------|------|
| Linux Kernel | 5.11+ | 内置 SGX 驱动 |
| SGX Driver | 2.11+ | 内核<5.11 时需要 |
| SGX SDK | 2.24+ | Intel 官方 SDK |
| DCAP Library | 1.15+ | Data Center Attestation Primitives |
| AESM Service | 最新 | Architectural Enclave Service Manager |
| PCCS | 3.1+ | Provisioning Certificate Caching Service (可选) |

### 2.3 环境检查脚本

```bash
#!/bin/bash
# check_sgx_environment.sh

echo "=== SGX 环境检查 ==="

# 1. 检查 CPU SGX 支持
echo -n "1. CPU SGX 支持："
if grep -q "sgx" /proc/cpuinfo; then
    echo "✓ 支持"
    grep -E "sgx|sgx_lc|sgx_dcap" /proc/cpuinfo | head -1
else
    echo "✗ 不支持"
    exit 1
fi

# 2. 检查 SGX 驱动
echo -n "2. SGX 驱动："
if lsmod | grep -q "intel_sgx\|isgx"; then
    echo "✓ 已加载"
    lsmod | grep -E "intel_sgx|isgx"
else
    echo "✗ 未加载"
fi

# 3. 检查 AESM 服务
echo -n "3. AESM 服务："
if systemctl is-active --quiet aesmd; then
    echo "✓ 运行中"
else
    echo "✗ 未运行"
fi

# 4. 检查 DCAP 库
echo -n "4. DCAP 库："
if ldconfig -p | grep -q "libsgx_dcap_ql"; then
    echo "✓ 已安装"
else
    echo "✗ 未安装"
fi

# 5. 检查 SGX 设备节点
echo -n "5. SGX 设备节点："
if [ -c "/dev/sgx_enclave" ] && [ -c "/dev/sgx_provision" ]; then
    echo "✓ 存在"
    ls -la /dev/sgx_*
else
    echo "✗ 不存在"
fi

# 6. 检查 EPC 内存
echo "6. EPC 内存："
if [ -f "/sys/devices/system/cpu/sgx/epc_size" ]; then
    cat /sys/devices/system/cpu/sgx/epc_size
else
    dmesg | grep -i "sgx.*epc" | tail -1
fi
```

---

## 三、测试场景设计

### 3.1 测试场景总览

| 编号 | 测试场景 | 测试类型 | 优先级 |
|------|---------|---------|--------|
| HW-001 | SGX 硬件基础验证 | 冒烟测试 | P0 |
| HW-002 | Enclave 初始化与测量值生成 | 功能测试 | P0 |
| HW-003 | DCAP Quote 生成（硬件模式） | 功能测试 | P0 |
| HW-004 | DCAP Quote 验证（硬件模式） | 功能测试 | P0 |
| HW-005 | 挑战 - 响应协议完整流程 | 集成测试 | P0 |
| HW-006 | SGX Sealing 密钥派生 | 安全测试 | P1 |
| HW-007 | 密钥层次结构验证 | 安全测试 | P1 |
| HW-008 | 安全通道建立与加密通信 | 集成测试 | P1 |
| HW-009 | 测量值白名单验证 | 安全测试 | P1 |
| HW-010 | 重放攻击防护验证 | 安全测试 | P1 |
| HW-011 | 证书链验证（PCCS/PCS） | 集成测试 | P2 |
| HW-012 | Enclave 逃逸防护测试 | 安全测试 | P2 |
| HW-013 | 并发认证请求处理 | 性能测试 | P2 |
| HW-014 | Quote 刷新与过期处理 | 功能测试 | P2 |
| HW-015 | 异常场景容错测试 | 稳定性测试 | P3 |

---

## 四、详细测试用例

### HW-001: SGX 硬件基础验证

**测试目标**: 验证 SGX 硬件环境可用性

**前置条件**:
- SGX 硬件支持
- 驱动和服务已安装

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_sgx_hardware_availability() {
    use vault_service::tee::{Enclave, EnclaveConfig};
    
    // 1. 创建 Enclave（需要硬件 SGX）
    let config = EnclaveConfig {
        debug_mode: false,  // 非调试模式，需要真实硬件
        ..Default::default()
    };
    
    let mut enclave = Enclave::new(config);
    
    // 2. 初始化应该成功（硬件环境）
    let result = enclave.initialize();
    assert!(result.is_ok(), "SGX 硬件初始化失败");
    
    // 3. 验证状态
    assert!(enclave.is_running());
    
    // 4. 验证测量值非空（真实硬件生成）
    let mrenclave = enclave.mrenclave();
    assert_ne!(mrenclave, [0u8; 32], "MRENCLAVE 应为真实哈希值");
    
    let mrsigner = enclave.mrsigner();
    assert_ne!(mrsigner, [0u8; 32], "MRSIGNER 应为真实哈希值");
}
```

**预期结果**:
- ✅ Enclave 初始化成功
- ✅ MRENCLAVE/MRSIGNER 为非零哈希值
- ✅ Enclave 状态为 Running

**通过标准**: 断言全部通过

---

### HW-002: Enclave 初始化与测量值生成

**测试目标**: 验证 Enclave 在真实硬件上的测量值生成

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_enclave_measurement_generation() {
    use vault_service::tee::{Enclave, EnclaveConfig};
    use std::collections::HashSet;
    
    let mut measurements = HashSet::new();
    
    // 多次初始化验证测量值一致性
    for _ in 0..3 {
        let config = EnclaveConfig {
            debug_mode: false,
            name: "test-enclave".to_string(),
            ..Default::default()
        };
        
        let mut enclave = Enclave::new(config);
        enclave.initialize().expect("初始化失败");
        
        // 记录测量值
        measurements.insert(hex::encode(enclave.mrenclave()));
    }
    
    // 相同 Enclave 应该有相同的测量值
    assert_eq!(measurements.len(), 1, "测量值应该一致");
}
```

**预期结果**:
- ✅ 多次初始化产生相同的 MRENCLAVE
- ✅ 测量值符合 SGX 规范（32 字节 SHA-256 哈希）

---

### HW-003: DCAP Quote 生成（硬件模式）

**测试目标**: 验证在真实 SGX 硬件上生成 DCAP Quote

**前置条件**:
- SGX 硬件可用
- DCAP 库已安装
- AESM 服务运行中

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_dcap_quote_generation_hardware() {
    use vault_service::tee::{
        dcap::{DcapConfig, DcapService},
        enclave::{Enclave, EnclaveConfig},
    };
    
    // 1. 创建硬件模式的 DCAP 服务
    let dcap_config = DcapConfig {
        simulation_mode: false,  // 硬件模式
        quote_max_age_seconds: 3600,
        ..Default::default()
    };
    
    let dcap_service = DcapService::new(dcap_config)
        .expect("DCAP 服务创建失败");
    
    // 2. 创建并初始化 Enclave
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().expect("Enclave 初始化失败");
    
    // 3. 生成 Quote（调用硬件 DCAP）
    let quote = dcap_service.initialize(&enclave)
        .expect("Quote 生成失败");
    
    // 4. 验证 Quote 结构
    assert_eq!(quote.version, 3, "Quote 版本应为 3");
    assert_eq!(quote.sign_type, 2, "签名类型应为 ECDSA P-256");
    
    // 5. 验证 Quote 包含正确的测量值
    assert_eq!(quote.report_body.mrenclave, enclave.mrenclave());
    assert_eq!(quote.report_body.mrsigner, enclave.mrsigner());
    
    // 6. 验证签名非空（硬件生成）
    let sig = &quote.signature.isv_enclave_report_signature;
    assert_ne!(sig.r, [0u8; 32], "签名 R 值不应全零");
    assert_ne!(sig.s, [0u8; 32], "签名 S 值不应全零");
}
```

**预期结果**:
- ✅ Quote 生成成功（调用硬件 DCAP）
- ✅ Quote 包含真实的 ECDSA 签名
- ✅ 测量值与 Enclave 一致

---

### HW-004: DCAP Quote 验证（硬件模式）

**测试目标**: 验证硬件生成的 Quote 可被正确验证

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_dcap_quote_verification_hardware() {
    use vault_service::tee::{
        dcap::{DcapConfig, DcapService},
        enclave::{Enclave, EnclaveConfig},
    };
    
    // 1. 生成 Quote
    let dcap_config = DcapConfig {
        simulation_mode: false,
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
    
    // 2. 配置白名单
    let mut verifier_config = DcapConfig {
        simulation_mode: false,
        allowed_mrenclaves: vec![enclave.mrenclave()],
        allowed_mrsigners: vec![enclave.mrsigner()],
        verify_certificate_chain: true,
        ..Default::default()
    };
    
    let verifier = DcapService::new(verifier_config).unwrap();
    
    // 3. 验证 Quote
    let report = verifier.verify_attestation(&quote_bytes, None);
    
    assert!(report.is_ok(), "Quote 验证失败：{:?}", report);
    let report = report.unwrap();
    
    // 4. 验证报告内容
    assert!(report.result.success);
    assert_eq!(report.mrenclave_hex, hex::encode(enclave.mrenclave()));
}
```

**预期结果**:
- ✅ Quote 验证成功
- ✅ 认证报告显示正确的测量值
- ✅ 证书链验证通过（如果启用）

---

### HW-005: 挑战 - 响应协议完整流程

**测试目标**: 验证完整的远程认证挑战 - 响应协议

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_challenge_response_protocol_hardware() {
    use vault_service::tee::{
        attestation::AttestationService,
        challenge::{ChallengeProtocol, ProverProtocol},
        enclave::{Enclave, EnclaveConfig},
    };
    
    // 1. 设置硬件模式
    let attestation_service = AttestationService::new()
        .allow_simulation(false)
        .allow_mrenclave(enclave.mrenclave());
    
    // 2. 创建 Enclave（Prover 侧）
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().unwrap();
    
    // 3. Verifier 生成挑战
    let verifier = ChallengeProtocol::new(attestation_service.clone());
    let challenge = verifier.generate_challenge(None, None).unwrap();
    
    // 4. Prover 响应挑战（需要硬件 Quote 生成）
    let prover = ProverProtocol::new(attestation_service.clone());
    let response = prover.respond_to_challenge(&enclave, &challenge)
        .expect("挑战响应失败");
    
    // 5. Verifier 验证响应
    let identity = generate_enclave_identity(&enclave);
    let result = verifier.verify_response(&response, &identity)
        .expect("响应验证失败");
    
    assert!(result.success);
    assert_eq!(result.mrenclave, enclave.mrenclave());
}

fn generate_enclave_identity(enclave: &Enclave) -> Vec<u8> {
    let mut identity = Vec::with_capacity(64);
    identity.extend_from_slice(&enclave.mrenclave());
    identity.extend_from_slice(&enclave.mrsigner());
    identity
}
```

**预期结果**:
- ✅ 挑战生成成功
- ✅ Quote 生成成功（硬件）
- ✅ 响应验证通过

---

### HW-006: SGX Sealing 密钥派生

**测试目标**: 验证 SGX Sealing Key 的正确派生和使用

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_sgx_sealing_key_derivation() {
    use vault_service::tee::{
        enclave::{Enclave, EnclaveConfig},
        sealing::{SealPolicy, SealingService},
    };
    
    // 1. 创建 Enclave
    let config = EnclaveConfig {
        debug_mode: false,
        seal_policy: SealPolicy::Mrsigner,  // 使用 MRSIGNER 策略
        ..Default::default()
    };
    
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();
    
    // 2. 获取 Sealing Key（从硬件 SGX）
    let sealing_service = SealingService::new();
    let sealing_key = sealing_service
        .get_sealing_key(SealPolicy::Mrsigner)
        .expect("Sealing Key 获取失败");
    
    // 3. 验证密钥非空
    assert_ne!(sealing_key.as_bytes(), &[0u8; 32]);
    
    // 4. 测试密封数据
    let test_data = b"Test sensitive data";
    let sealed = sealing_service
        .seal_data(&sealing_key, test_data)
        .expect("密封失败");
    
    assert!(!sealed.is_empty());
    
    // 5. 测试解封数据
    let unsealed = sealing_service
        .unseal_data(&sealing_key, &sealed)
        .expect("解封失败");
    
    assert_eq!(unsealed, test_data);
}
```

**预期结果**:
- ✅ Sealing Key 从硬件 SGX 正确获取
- ✅ 数据密封成功
- ✅ 数据解封成功且内容一致

---

### HW-007: 密钥层次结构验证

**测试目标**: 验证完整的密钥层次结构在硬件 Enclave 中的派生

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_key_hierarchy_hardware() {
    use vault_service::tee::{
        enclave::{Enclave, EnclaveConfig},
        crypto::{KeyHierarchy, KeyPurpose},
    };
    
    let config = EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    };
    
    let mut enclave = Enclave::new(config);
    enclave.initialize().unwrap();
    
    // 1. 派生 L2 用户保险库密钥
    let user_key_handle = enclave.derive_user_vault_key(
        "tenant_1",
        "user_1"
    ).expect("L2 密钥派生失败");
    
    assert_ne!(user_key_handle, [0u8; 32]);
    
    // 2. 使用 L2 密钥加密
    let plaintext = b"Secret credential data";
    let blob = enclave.encrypt_credential(
        "tenant_1",
        "user_1",
        "cred_1",
        plaintext
    ).expect("加密失败");
    
    // 3. 使用相同密钥解密
    let decrypted = enclave.decrypt_credential(
        "tenant_1",
        "user_1",
        "cred_1",
        &blob
    ).expect("解密失败");
    
    assert_eq!(decrypted, plaintext);
    
    // 4. 验证不同租户产生不同密文
    let blob_tenant2 = enclave.encrypt_credential(
        "tenant_2",
        "user_1",
        "cred_1",
        plaintext
    ).expect("加密失败");
    
    assert_ne!(blob.ciphertext, blob_tenant2.ciphertext);
}
```

**预期结果**:
- ✅ L2 密钥正确派生
- ✅ 加密/解密成功
- ✅ 租户隔离有效

---

### HW-008: 安全通道建立与加密通信

**测试目标**: 验证基于远程认证的安全通道建立

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_secure_channel_establishment_hardware() {
    use vault_service::tee::{
        attestation::AttestationResult,
        challenge::SecureChannel,
        enclave::{Enclave, EnclaveConfig},
    };
    
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
    
    // 2. 建立安全通道
    let channel = SecureChannel::establish(
        &attestation_result,
        "test_channel".to_string(),
        3600  // 1 小时 TTL
    ).expect("安全通道建立失败");
    
    // 3. 验证通道属性
    assert_eq!(channel.channel_id, "test_channel");
    assert_eq!(channel.session_key.len(), 32);  // 256 位密钥
    assert!(channel.properties.encrypted);
    assert!(channel.properties.authenticated);
    
    // 4. 测试加密通信
    let message = b"Secure message";
    let encrypted = channel.encrypt(message).expect("加密失败");
    let decrypted = channel.decrypt(&encrypted).expect("解密失败");
    
    assert_eq!(decrypted, message);
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
```

**预期结果**:
- ✅ 安全通道建立成功
- ✅ 会话密钥正确派生
- ✅ 加密/解密成功

---

### HW-009: 测量值白名单验证

**测试目标**: 验证测量值白名单机制有效

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_measurement_whitelist_hardware() {
    use vault_service::tee::{
        dcap::{DcapConfig, DcapService},
        enclave::{Enclave, EnclaveConfig},
    };
    
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().unwrap();
    
    // 1. 使用正确白名单
    let config_accept = DcapConfig {
        simulation_mode: false,
        allowed_mrenclaves: vec![enclave.mrenclave()],
        ..Default::default()
    };
    
    let service_accept = DcapService::new(config_accept).unwrap();
    service_accept.initialize(&enclave).unwrap();
    
    let quote = service_accept.get_current_quote().unwrap();
    let quote_bytes = service_accept.quote_to_bytes(&quote).unwrap();
    
    // 应该接受
    let result = service_accept.verify_attestation(&quote_bytes, None);
    assert!(result.is_ok(), "正确白名单应接受");
    
    // 2. 使用错误白名单
    let wrong_mrenclave = [0x99u8; 32];
    let config_reject = DcapConfig {
        simulation_mode: false,
        allowed_mrenclaves: vec![wrong_mrenclave],
        ..Default::default()
    };
    
    let service_reject = DcapService::new(config_reject).unwrap();
    service_reject.initialize(&enclave).unwrap();
    
    let quote = service_reject.get_current_quote().unwrap();
    let quote_bytes = service_reject.quote_to_bytes(&quote).unwrap();
    
    // 应该拒绝
    let result = service_reject.verify_attestation(&quote_bytes, None);
    assert!(matches!(result.unwrap_err(), DcapError::MeasurementMismatch));
}
```

**预期结果**:
- ✅ 匹配的白名单接受
- ✅ 不匹配的白名单拒绝

---

### HW-010: 重放攻击防护验证

**测试目标**: 验证 nonce 机制防止重放攻击

**测试步骤**:
```rust
#[test]
#[cfg(target_os = "linux")]
fn test_replay_attack_protection_hardware() {
    use vault_service::tee::{
        dcap::{DcapConfig, DcapService},
        enclave::{Enclave, EnclaveConfig},
    };
    use ring::digest::{SHA256, digest};
    
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: false,
        ..Default::default()
    });
    enclave.initialize().unwrap();
    
    let dcap_service = DcapService::new(DcapConfig {
        simulation_mode: false,
        allowed_mrenclaves: vec![enclave.mrenclave()],
        ..Default::default()
    }).unwrap();
    
    dcap_service.initialize(&enclave).unwrap();
    
    // 1. 生成随机 nonce
    let nonce = b"unique_challenge_nonce_12345";
    
    // 2. 第一次验证应该成功
    let quote = dcap_service.get_current_quote().unwrap();
    let quote_bytes = dcap_service.quote_to_bytes(&quote).unwrap();
    
    let result1 = dcap_service.verify_attestation(&quote_bytes, Some(nonce));
    assert!(result1.is_ok(), "第一次验证应成功");
    
    // 3. 使用相同 nonce 第二次验证应该失败（重放攻击）
    // 注意：实际实现需要服务端维护 nonce 使用记录
    // 这里验证 nonce 绑定机制
    let report_data = quote.report_body.report_data;
    let expected_hash = digest(&SHA256, nonce);
    
    // nonce 哈希应该绑定到 report_data
    assert_eq!(&report_data.data[..32], expected_hash.as_ref());
}
```

**预期结果**:
- ✅ Nonce 正确绑定到 Quote
- ✅ 重放攻击可被检测

---

## 五、测试执行流程

### 5.1 测试准备阶段

```bash
# 1. 环境检查
./check_sgx_environment.sh

# 2. 安装依赖
sudo apt install -y libsgx-dcap-ql libsgx-quote-ex sgx-aesm-service

# 3. 启动服务
sudo systemctl start aesmd
sudo systemctl start pccs  # 可选

# 4. 设置环境变量
export SGX_MODE=hardware
export RUST_LOG=debug
```

### 5.2 执行测试

```bash
# 运行所有硬件测试
cargo test --test tee_attestation_tests -- --test-threads=1

# 运行特定测试
cargo test test_dcap_quote_generation_hardware -- --exact --nocapture

# 生成测试报告
cargo test --test tee_attestation_tests -- --format=json > test_results.json
```

### 5.3 测试结果分析

```bash
# 查看测试结果
cat test_results.json | jq '.test_results[] | select(.status != "ok")'

# 生成覆盖率报告
cargo tarpaulin --output-dir ./coverage --out Html
```

---

## 六、预期结果与验收标准

### 6.1 功能测试验收

| 测试项 | 通过标准 | 实际结果 |
|-------|---------|---------|
| HW-001 | Enclave 初始化成功 | ☐ |
| HW-002 | 测量值一致性验证通过 | ☐ |
| HW-003 | Quote 生成成功（硬件签名） | ☐ |
| HW-004 | Quote 验证成功 | ☐ |
| HW-005 | 挑战 - 响应完整流程通过 | ☐ |

### 6.2 安全测试验收

| 测试项 | 通过标准 | 实际结果 |
|-------|---------|---------|
| HW-006 | Sealing Key 正确派生 | ☐ |
| HW-007 | 密钥层次结构验证通过 | ☐ |
| HW-008 | 安全通道建立成功 | ☐ |
| HW-009 | 白名单机制有效 | ☐ |
| HW-010 | 重放攻击防护有效 | ☐ |

### 6.3 性能测试验收

| 测试项 | 通过标准 | 实际结果 |
|-------|---------|---------|
| HW-011 | Quote 生成<500ms | ☐ |
| HW-012 | Quote 验证<200ms | ☐ |
| HW-013 | 并发 100 请求成功率>99% | ☐ |

---

## 七、故障排查指南

### 7.1 常见问题

**问题 1: "SGX not available"**

```bash
# 检查 BIOS 设置
# 重启进入 BIOS，确保 SGX 设置为 Enabled

# 检查驱动
sudo modprobe intel_sgx

# 检查设备节点
ls -la /dev/sgx_*
```

**问题 2: "Quote generation failed"**

```bash
# 检查 AESM 服务
sudo systemctl status aesmd
sudo journalctl -u aesmd -n 50

# 重启服务
sudo systemctl restart aesmd
```

**问题 3: "Certificate verification failed"**

```bash
# 检查 PCCS 配置
cat /etc/sgx_default_qcnl.conf

# 测试 PCS 连接
curl -v https://api.trustedservices.intel.com/sgx/certification/v4/
```

### 7.2 日志收集

```bash
# 收集所有相关日志
journalctl -u aesmd > aesmd.log
journalctl -u pccs > pccs.log
dmesg | grep -i sgx > sgx_dmesg.log

# 打包日志
tar czf sgx_logs.tar.gz aesmd.log pccs.log sgx_dmesg.log
```

---

## 八、测试报告模板

### 8.1 测试执行摘要

```markdown
## SGX 硬件测试执行摘要

**测试日期**: YYYY-MM-DD
**测试环境**: 
- CPU: Intel Core i7-xxxx
- Linux Kernel: 5.x.x
- SGX Driver: x.xx
- DCAP Library: x.xx

**测试结果**:
- 总测试数：15
- 通过：xx
- 失败：xx
- 跳过：xx

**关键发现**:
1. ...
2. ...

**建议**:
1. ...
2. ...
```

### 8.2 详细测试结果

每个测试用例应包含：
- 测试名称
- 执行状态（Pass/Fail/Skip）
- 执行时间
- 日志输出
- 失败原因（如果失败）

---

## 九、后续步骤

1. **自动化测试**: 将硬件测试集成到 CI/CD 流程
2. **性能基准**: 建立性能基线并持续监控
3. **安全审计**: 定期进行第三方安全审计
4. **文档更新**: 根据测试结果更新运维文档

---

## 附录 A: 参考文档

- [Intel SGX 官方文档](https://www.intel.com/content/www/us/en/developer/tools/software-guard-extensions/overview.html)
- [DCAP GitHub 仓库](https://github.com/intel/SGXDataCenterAttestationPrimitives)
- [CredBridge DCAP 设置指南](./DCAP_SETUP.md)
- [CredBridge TEE 沙箱分析](./SANDBOX_ANALYSIS.md)
