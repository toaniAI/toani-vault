# CredBridge TEE 沙箱文档

**生成日期**: 2026-03-18
**模块版本**: 0.1.0
**TEE 类型**: Intel SGX (支持 DCAP 远程认证)

---

## 概述

TEE（可信执行环境）模块提供安全的代码执行环境，基于 Intel SGX 技术实现。主要功能包括：

- **Enclave 安全飞地**: 硬件隔离的执行环境
- **密钥管理**: 安全密钥生成、缓存和清理
- **远程认证**: SGX DCAP 证明和验证
- **密封存储**: 安全数据密封和恢复
- **沙箱执行**: 代码在隔离环境中运行

### 运行模式选择

CredBridge 通过 `TEE_MODE` 显式选择运行模式：

```bash
TEE_MODE=hardware cargo run
TEE_MODE=simulation cargo run
```

- `TEE_MODE=simulation` 仅用于显式模拟路径；只有在该模式下，健康/状态接口里的模拟标记才会出现。
- `TEE_MODE=hardware` 要求真实 SGX/DCAP/AESM/PCCS（或 Intel PCS）能力；如果能力未接通，初始化会 fail-closed，而不是回退到 simulation。
- 默认 CI 只跑 simulation-safe 测试；hardware-only 验证在专用 SGX runner 或 staging 环境执行。

---

## 模块结构

```
src/tee/
├── mod.rs           # 模块导出和 TEE 检测
├── enclave.rs       # Enclave 核心实现
├── keys.rs          # 密钥管理（TTL 缓存、Zeroize）
├── cleanup.rs       # 密钥清理策略与调度器
├── sealing.rs       # SGX Sealing 密钥和密封存储
├── attestation.rs   # SGX DCAP 远程认证协议
├── dcap.rs          # DCAP 服务实现
├── quote.rs         # Quote 解析和验证
├── challenge.rs     # 挑战-响应协议
├── sandbox/         # 沙箱运行时
└── upgrade.rs       # Enclave 升级支持
```

---

## 核心组件

### Enclave (安全飞地)

Enclave 是 SGX 提供的硬件隔离执行环境。

```rust
pub struct Enclave {
    config: EnclaveConfig,
    state: EnclaveState,
    key_manager: KeyManager,
    sealing_service: SealingService,
}

pub enum EnclaveState {
    Uninitialized,  // 未初始化
    Initializing,   // 初始化中
    Ready,          // 就绪
    Running,        // 运行中
    Error,          // 错误
    Terminated,     // 已终止
}
```

**主要功能**:

- 初始化/终止 Enclave
- 加密/解密凭证
- 密钥生命周期管理
- 密封数据存储

### 密钥管理 (Keys)

```rust
pub struct KeyManager {
    master_key: ProtectedKeyMaterial,
    user_key_cache: UserKeyCache,
    ttl_seconds: u64,
}

pub struct ProtectedKeyMaterial {
    data: Vec<u8>,  // 使用 zeroize 自动清零
}
```

**安全特性**:

- TTL 缓存（默认 5 分钟）
- 自动 Zeroize 清理
- Enclave 重启后密钥恢复

### 密钥清理 (Cleanup)

```rust
pub struct CleanupScheduler {
    config: CleanupConfig,
    stats: CleanupStats,
}

pub enum KeyLifecycle {
    Active,      // 活跃使用
    Expired,     // 已过期
    Compromised, // 已泄露
    Archived,    // 已归档
}
```

**清理策略**:

- 定期自动清理
- 手动触发清理
- 紧急密钥吊销

### 密封存储 (Sealing)

SGX Sealing 允许将数据加密存储在 Enclave 外部。

```rust
pub struct SealedData {
    ciphertext: Vec<u8>,
    policy: SealPolicy,
    version: u32,
}

pub enum SealPolicy {
    Mrenclave,  // 仅相同 Enclave 可解密
    Mrsigner,   // 相同签名者 Enclave 可解密
}
```

**使用场景**:

- 凭证加密存储
- 配置数据保护
- 审计日志加密

### 远程认证 (Attestation)

SGX DCAP 远程认证允许验证 Enclave 的真实性和完整性。

```rust
pub struct AttestationService {
    state: AttestationState,
    session: AttestationSession,
}

pub struct Quote {
    version: u16,
    sign_type: u16,
    report_body: ReportBody,
    signature: QuoteSignature,
}
```

**认证流程**:

1. Verifier 生成随机挑战
2. Enclave 生成 Quote (包含挑战哈希)
3. Verifier 验证 Quote 签名
4. 验证测量值 (MRENCLAVE)
5. 建立安全通道

### DCAP 服务

Intel DCAP (Data Center Attestation Primitives) 提供数据中心级别的远程认证。

```rust
pub struct DcapService {
    config: DcapConfig,
    version: String,
}

pub struct DcapAttestationReport {
    quote: DcapQuote,
    tcb_info: String,
    pck_certificate: CertificateInfo,
}
```

**支持的证书服务**:

- Intel PCS (Production Cert Service)
- Intel PCS (Testing)

---

## 沙箱 (Sandbox)

TEE 沙箱提供隔离的代码执行环境。

```rust
pub struct SandboxSession {
    session_id: String,
    tenant_id: String,
    runtime: RuntimeType,
    status: SandboxStatus,
    enclave: Arc<Mutex<Enclave>>,
}

pub enum RuntimeType {
    Python3_11,
    Node18,
    Rust,
}

pub enum SandboxStatus {
    Initializing,
    Ready,
    Running,
    Terminated,
    Error,
}
```

### 沙箱 API

**创建会话**:

```json
POST /api/v1/sandbox/sessions
{
  "credential_id": "550e8400-e29b-41d4-a716-446655440000",
  "original_intent": "查询投资组合",
  "metadata": {
    "source": "mobile_app"
  }
}
```

**请求字段说明**:

| 字段              | 类型   | 必填 | 说明                                        |
| ----------------- | ------ | ---- | ------------------------------------------- |
| `credential_id`   | UUID   | 是   | 凭证ID，用于在沙箱中安全访问凭证            |
| `original_intent` | string | 是   | 原始意图描述，用于审计和AI审核，最大500字符 |
| `metadata`        | object | 否   | 可选的会话元数据                            |

**执行操作**:

```json
POST /api/v1/sandbox/sessions/{id}/execute
{
  "operation_type": "navigate",
  "description": "导航到登录页面",
  "parameters": {
    "url": "https://example.com/login"
  }
}
```

**关闭会话**:

```
DELETE /api/v1/sandbox/sessions/{id}
```

> **注意**: 完整 API 规范请参考 `docs/openapi/sandbox.yaml` 和 `docs/03-API 参考/REST-API.md`

---

## 远程认证流程

```
┌─────────────┐                           ┌─────────────┐
│   Client    │                           │   Enclave   │
└──────┬──────┘                           └──────┬──────┘
       │                                         │
       │  1. 请求认证                             │
       │ ──────────────────────────────────────> │
       │                                         │
       │  2. 返回挑战 (Challenge)                  │
       │ <────────────────────────────────────── │
       │                                         │
       │  3. 发送挑战响应 + Quote                 │
       │ ──────────────────────────────────────> │
       │                                         │
       │  4. 验证 Quote                           │
       │     - 验证 Intel 签名                    │
       │     - 验证 MRENCLAVE                     │
       │     - 验证挑战哈希                       │
       │                                         │
       │  5. 返回认证结果                          │
       │ <────────────────────────────────────── │
       │                                         │
```

---

## TEE 类型支持

| TEE 类型                           | 支持状态        | 远程认证  | 密封存储    |
| ---------------------------------- | --------------- | --------- | ----------- |
| Intel SGX                          | ✅ 完整支持     | ✅ 支持   | ✅ 支持     |
| AMD SEV-SNP                        | 🔜 计划支持     | 🔜 计划   | 🔜 计划     |
| AWS Nitro                          | 🔜 计划支持     | 🔜 计划   | 🔜 计划     |
| ARM TrustZone                      | 🔜 计划支持     | 🔜 计划   | 🔜 计划     |
| Simulation (`TEE_MODE=simulation`) | ✅ 显式模拟测试 | ❌ 不支持 | ⚠️ 模拟实现 |

---

## 安全最佳实践

### 密钥管理

1. **使用 TTL 缓存**: 限制密钥在内存中的存活时间
2. **自动 Zeroize**: 使用 `zeroize` crate 确保密钥安全清除
3. **分离主密钥和用户密钥**: 主密钥用于保护用户密钥
4. **定期轮换**: 支持密钥版本和轮换

### 远程认证

1. **验证 Intel 签名**: 确保证书链有效
2. **检查 TCB 版本**: 防止回滚攻击
3. **验证 MRENCLAVE**: 确保运行预期的 Enclave 代码
4. **使用安全挑战**: 防止重放攻击

### 沙箱安全

1. **资源限制**: 设置内存、CPU、时间限制
2. **网络隔离**: 限制沙箱网络访问
3. **输入验证**: 严格验证所有输入
4. **审计日志**: 记录所有沙箱操作

---

## 使用示例

### 初始化 Enclave

```rust
use vault_service::tee::{Enclave, EnclaveConfig, EnclaveState};

let config = EnclaveConfig {
    enclave_path: "/path/to/enclave.signed.so",
    sealed_storage_path: "/var/lib/credbridge/sealed",
    enable_dcap: true,
};

let mut enclave = Enclave::new(config);
enclave.initialize()?;

assert_eq!(enclave.state(), EnclaveState::Ready);
```

### 加密/解密凭证

```rust
// 加密
let plaintext = b"sensitive credential data";
let encrypted = enclave.encrypt_credential(
    "tenant_123",
    "user_456",
    "credential_789",
    plaintext
)?;

// 解密
let decrypted = enclave.decrypt_credential(
    "tenant_123",
    "user_456",
    "credential_789",
    &encrypted
)?;

assert_eq!(decrypted, plaintext);
```

### 密封存储

```rust
use vault_service::tee::{SealPolicy, SealedData};

// 密封数据
let data = b"confidential data";
let sealed: SealedData = enclave.seal_data(data, SealPolicy::Mrenclave)?;

// 解封数据
let unsealed = enclave.unseal_data(&sealed)?;
assert_eq!(unsealed, data);
```

### 远程认证

```rust
use vault_service::tee::{AttestationService, ChallengeProtocol};

// 创建认证服务
let attestation = AttestationService::new(&enclave)?;

// 生成挑战
let challenge = attestation.generate_challenge()?;

// 生成 Quote
let quote = attestation.generate_quote(&challenge)?;

// 验证 (在 Verifier 端)
let result = attestation.verify_quote(&quote, &challenge)?;
assert!(result.is_valid);
```

---

## 配置

### Enclave 配置

```rust
EnclaveConfig {
    // Enclave 文件路径
    enclave_path: String,

    // 密封存储目录
    sealed_storage_path: String,

    // 启用 DCAP 远程认证
    enable_dcap: bool,

    // DCAP 配置
    dcap_config: DcapConfig {
        pcs_base_url: String,
        use_test_pcs: bool,
        cache_pck_certs: bool,
    },

    // 密钥 TTL（秒）
    key_ttl_seconds: u64,

    // 最大内存使用 (MB)
    max_memory_mb: u32,
}
```

### 环境变量

```bash
# TEE 运行模式（必须显式设置）
TEE_MODE=hardware   # 或 simulation

# SGX 环境
SGX_SDK=/opt/intel/sgxsdk

# DCAP 配置
DCAP_PCS_URL=https://api.trustedservices.intel.com
DCAP_USE_TEST=false

# 密封存储
SEALED_STORAGE_PATH=/var/lib/credbridge/sealed

# 密钥 TTL
KEY_TTL_SECONDS=300
```

---

## 故障排查

### 常见问题

**Enclave 初始化失败**:

- 检查 SGX 驱动是否加载: `ls /dev/sgx*`
- 检查 Enclave 文件路径是否正确
- 检查文件权限
- 若只需 simulation-safe 调试，请显式设置 `TEE_MODE=simulation`

**远程认证失败**:

- 检查网络连接 (Intel PCS)
- 检查 PCK 证书缓存
- 验证 TCB 级别

**密钥解密失败**:

- 检查密封策略匹配
- 验证 Enclave 测量值
- 检查密钥版本

### 调试命令

```bash
# 检查 SGX 状态
sgx-enable -s

# 查看 Enclave 日志
tail -f /var/log/credbridge/enclave.log

# 验证 Quote
/credbridge/bin/verify-quote -q quote.bin

# 检查密封数据
ls -la /var/lib/credbridge/sealed/
```

---

_本文档由 BMAD document-project 工作流自动生成_
