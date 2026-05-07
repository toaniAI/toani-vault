# 远程认证协议

ToaniVault 实现了基于 Intel SGX DCAP（Data Center Attestation Primitives）的远程认证协议，用于验证 Enclave 的身份和完整性。

## 目录

- [DCAP 协议实现](#dcap-协议实现)
- [Quote 管理](#quote-管理)
- [签名验证机制](#签名验证机制)
- [挑战 - 响应流程](#挑战 - 响应流程)
- [API 参考](#api-参考)
- [相关文档](#相关文档)

---

## DCAP 协议实现

### 什么是 DCAP

DCAP（Data Center Attestation Primitives）是 Intel 提供的数据中心远程认证基础设施，允许验证远程 SGX Enclave 的真实性。

### 认证架构

```
┌─────────────────────────────────────────────────────────────────┐
│                  SGX DCAP 远程认证流程                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Verifier (挑战者)              Prover (Enclave)               │
│        │                              │                        │
│        │ 1. 生成随机挑战               │                        │
│        │ ─────────────────────────────>│                        │
│        │                              │                        │
│        │                              │ 2. 生成 Quote           │
│        │                              │    REPORT_DATA =       │
│        │                              │    hash(challenge +    │
│        │                              │    identity)           │
│        │                              │                        │
│        │ 3. 返回 Quote                 │                        │
│        │ <─────────────────────────────│                        │
│        │                              │                        │
│        │ 4. 验证 Quote                 │                        │
│        │    - 验证 ECDSA 签名          │                        │
│        │    - 验证 MRENCLAVE           │                        │
│        │    - 验证挑战绑定             │                        │
│        │                              │                        │
│        │ 5. 建立安全通道               │                        │
│        │ ─────────────────────────────>│                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### DCAP 组件

#### 1. Quote 生成

Enclave 生成包含测量值的签名报告：

```rust
use vault_service::tee::dcap::DcapService;

let dcap_service = DcapService::new(config)?;

// 生成 Quote
let quote = dcap_service.generate_quote(
    &enclave,
    &challenge  // 挑战数据
)?;

// Quote 包含：
// - Report Body: MRENCLAVE, MRSIGNER, ISVPRODID, ISVSVN
// - Signature: ECDSA P-256 签名
// - Report Data: 挑战哈希 + Enclave 身份
```

#### 2. Intel PCS（Provisioning Certification Service）

Intel 提供的证书颁发服务：

```rust
use vault_service::tee::dcap::{DcapConfig, INTEL_PCS_BASE_URL_PROD};

let config = DcapConfig {
    pcs_base_url: INTEL_PCS_BASE_URL_PROD.to_string(),
    api_key: Some("your_intel_pcs_api_key".to_string()),
    quote_max_age_seconds: 3600,
    verify_certificate_chain: true,
    ..Default::default()
};
```

**PCS 服务**:

- 颁发 PCK（Processor Certification Key）证书
- 提供证书吊销列表（CRL）
- 验证 Quote 签名链

#### 3. PCCS（Provisioning Certification Cache Service）

本地缓存服务（可选）：

```rust
// 部署本地 PCCS 缓存证书，减少对 Intel PCS 的依赖
let config = DcapConfig {
    pcs_base_url: "https://localhost:8081/sgx/certification/v4/".to_string(),
    ..Default::default()
};
```

### Quote 结构

```
┌─────────────────────────────────────────────────────────────────┐
│                    SGX Quote 结构                                │
├─────────────────────────────────────────────────────────────────┤
│  Header (48 bytes)                                              │
│  ├─ Version (2 bytes): 3                                        │
│  ├─ Sign Type (2 bytes): 2 (ECDSA P-256)                       │
│  ├─ Reserved (44 bytes)                                         │
├─────────────────────────────────────────────────────────────────┤
│  Report Body (384 bytes)                                        │
│  ├─ CPU SVN (16 bytes): 安全版本号                              │
│  ├─ Misc Select (4 bytes): 额外属性                             │
│  ├─ Reserved (28 bytes)                                         │
│  ├─ ISV EXT PROD ID (16 bytes): 扩展产品 ID                     │
│  ├─ ISV Family ID (16 bytes): 家族 ID                           │
│  ├─ ISV PROD ID (2 bytes): 产品 ID                              │
│  ├─ ISV SVN (2 bytes): 安全版本号                               │
│  ├─ Enclave Flags (8 bytes): 属性标志                           │
│  ├─ MRENCLAVE (32 bytes): Enclave 测量值                        │
│  ├─ Reserved (32 bytes)                                         │
│  ├─ ISV Family ID (16 bytes)                                    │
│  ├─ Report Data (64 bytes): 挑战哈希 + 身份                     │
│  ├─ MRSIGNER (32 bytes): 签名者测量值                           │
│  ├─ Reserved (96 bytes)                                         │
├─────────────────────────────────────────────────────────────────┤
│  Signature (variable)                                           │
│  ├─ ECDSA P-256 签名                                             │
│  ├─ 公钥                                                         │
│  └─ 证书链                                                       │
└─────────────────────────────────────────────────────────────────┘
```

### 验证流程

```rust
use vault_service::tee::dcap::DcapService;

let dcap_service = DcapService::new(config)?;

// 验证 Quote
let report = dcap_service.verify_attestation(
    &quote_bytes,
    Some(&challenge)  // 可选的挑战数据
)?;

// 验证步骤：
// 1. 解析 Quote 结构
// 2. 验证 ECDSA 签名
// 3. 验证证书链（PCK -> PCK Platform CA -> Intel Root CA）
// 4. 验证 MRENCLAVE 在白名单中
// 5. 验证挑战绑定（REPORT_DATA 包含挑战哈希）
// 6. 验证时间戳（Quote 未过期）

if report.result.success {
    println!("✅ Enclave 认证成功！");
    println!("MRENCLAVE: {}", report.mrenclave_hex);
} else {
    println!("❌ 认证失败：{}", report.result.error);
}
```

---

## Quote 管理

### Quote 生成

#### 自动生成

Enclave 初始化时自动生成 Quote：

```rust
use vault_service::tee::dcap::DcapService;

let dcap_service = DcapService::new(config)?;

// 初始化时自动生成 Quote
let quote = dcap_service.initialize(&enclave)?;

// Quote 缓存到内存
// 默认有效期 1 小时
```

#### 手动刷新

Quote 过期前手动刷新：

```rust
// 刷新 Quote
let new_quote = dcap_service.refresh_quote(&enclave)?;

// 建议在到期前 5 分钟刷新
```

### Quote 缓存

```rust
use vault_service::tee::dcap::QuoteCache;

let mut cache = QuoteCache::new(3600); // 1 小时 TTL

// 缓存 Quote
cache.insert(quote, timestamp)?;

// 获取 Quote
let quote = cache.get();

// 检查是否过期
if cache.is_expired() {
    // 刷新 Quote
    let new_quote = dcap_service.refresh_quote(&enclave)?;
    cache.insert(new_quote, current_timestamp())?;
}
```

### Quote 刷新策略

```rust
use std::time::{Duration, Instant};

pub struct QuoteRefreshPolicy {
    /// Quote 有效期
    quote_ttl: Duration,
    /// 提前刷新时间
    refresh_before_expiry: Duration,
    /// 最后刷新时间
    last_refresh: Instant,
}

impl QuoteRefreshPolicy {
    pub fn should_refresh(&self) -> bool {
        let elapsed = self.last_refresh.elapsed();
        elapsed >= (self.quote_ttl - self.refresh_before_expiry)
    }
}

// 使用示例
let policy = QuoteRefreshPolicy {
    quote_ttl: Duration::from_secs(3600),
    refresh_before_expiry: Duration::from_secs(300), // 提前 5 分钟
    last_refresh: Instant::now(),
};

if policy.should_refresh() {
    // 刷新 Quote
}
```

### Quote 格式转换

```rust
use vault_service::tee::quote::{QuoteParser, QuoteSerializer};

// 解析 Quote
let quote = QuoteParser::parse(&quote_bytes)?;

// 序列化为 Base64
let quote_b64 = base64::encode(&quote_bytes);

// 从 Base64 解析
let quote_bytes = base64::decode(&quote_b64)?;
let quote = QuoteParser::parse(&quote_bytes)?;

// 提取关键信息
let mrenclave = &quote.report_body.mrenclave;
let mrsigner = &quote.report_body.mrsigner;
let report_data = &quote.report_body.report_data;
```

---

## 签名验证机制

### ECDSA P-256 签名

Quote 使用 ECDSA P-256 数字签名：

```rust
use ring::signature::{ECDSA_P256_SHA256, UnparsedPublicKey};

// 验证签名
let public_key = /* 从 Quote 提取公钥 */;
let unparsed = UnparsedPublicKey::new(&ECDSA_P256_SHA256, public_key);

unparsed.verify(
    quote_body,  // 被签名的数据
    signature    // 签名
)?;
```

### 证书链验证

```
Intel SGX Root CA
    │
    ▼
Intel SGX PCK Platform CA
    │
    ▼
Intel SGX PCK Certificate (每处理器)
    │
    ▼
Quote Signature
```

#### 验证步骤

```rust
use vault_service::tee::dcap::CertificateChainVerifier;

let verifier = CertificateChainVerifier::new();

// 验证证书链
let result = verifier.verify(
    &quote_signature,
    &pcl_certificate,      // PCK 证书
    &platform_certificate, // PCK Platform CA 证书
    &root_certificate      // Intel Root CA
)?;

// 检查证书吊销
let crl_check = verifier.check_crl(&pcl_certificate)?;
assert!(crl_check.is_valid());
```

### 测量值白名单

```rust
use vault_service::tee::dcap::DcapService;

let mut service = DcapService::new(config)?;

// 添加允许的 MRENCLAVE
service.allow_mrenclave(expected_mrenclave);

// 添加允许的 MRSIGNER
service.allow_mrsigner(expected_mrsigner);

// 验证时自动检查白名单
let result = service.verify_attestation(&quote_bytes, None)?;
// 如果 MRENCLAVE/MRSIGNER 不在白名单中，验证失败
```

### 时间戳验证

```rust
use vault_service::tee::dcap::QuoteValidator;

let validator = QuoteValidator::new()
    .with_max_quote_age(3600) // Quote 最大有效期 1 小时
    .with_current_time(current_timestamp());

// 验证时间戳
validator.validate_timestamp(&quote)?;

// 检查 Quote 生成时间
let quote_age = current_timestamp() - quote.timestamp;
assert!(quote_age < 3600); // 不超过 1 小时
```

---

## 挑战 - 响应流程

### 完整流程

```
┌─────────────────────────────────────────────────────────────────┐
│                  挑战 - 响应认证流程                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  客户端                      ToaniVault API          Enclave    │
│    │                            │                        │      │
│    │ 1. POST /attestation/challenge                            │
│    │───────────────────────────>│                        │      │
│    │                            │                        │      │
│    │                            │ 2. 生成随机 nonce            │
│    │                            │───────────────────────>│      │
│    │                            │                        │      │
│    │                            │ 3. 生成 Quote               │
│    │                            │    REPORT_DATA =       │      │
│    │                            │    hash(nonce + id)    │      │
│    │                            │<───────────────────────│      │
│    │                            │                        │      │
│    │ 4. 返回 {nonce, quote}     │                        │      │
│    │<───────────────────────────│                        │      │
│    │                            │                        │      │
│    │ 5. POST /attestation/verify-response                    │
│    │───────────────────────────>│───────────────────────>│      │
│    │                            │                        │      │
│    │                            │ 6. 验证 Quote             │      │
│    │                            │    - 签名验证          │      │
│    │                            │    - 挑战绑定          │      │
│    │                            │    - 测量值验证        │      │
│    │                            │                        │      │
│    │ 7. 返回 {verified, mrenclave}                        │      │
│    │<───────────────────────────│                        │      │
│    │                            │                        │      │
│    │ 8. 建立安全通道                                          │      │
│    │────────────────────────────────────────────────────>│      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 挑战生成

```rust
use vault_service::tee::challenge::ChallengeProtocol;

let protocol = ChallengeProtocol::new(attestation_service);

// 生成随机挑战
let challenge = protocol.generate_challenge(
    Some(enclave_id),  // 可选的 Enclave 标识
    Some(metadata)     // 可选的元数据
)?;

// 挑战包含：
// - challenge_id: 唯一标识符
// - nonce: 32 字节随机值
// - expires_at: 过期时间（默认 5 分钟）
```

### 挑战响应

```rust
use vault_service::tee::enclave::ProverProtocol;

let prover = ProverProtocol::new(attestation_service);

// 接收挑战
let challenge = receive_from_verifier();

// 生成响应（Quote）
let response = prover.respond_to_challenge(
    &enclave,
    &challenge
)?;

// 响应包含：
// - quote: SGX Quote
// - challenge_id: 挑战 ID
// - identity: Enclave 身份信息
```

### 挑战验证

```rust
use vault_service::tee::challenge::ChallengeProtocol;

let protocol = ChallengeProtocol::new(attestation_service);

// 验证响应
let result = protocol.verify_response(
    &response,
    &enclave_identity
)?;

// 验证步骤：
// 1. 检查挑战是否存在且未过期
// 2. 验证挑战未被使用（防重放）
// 3. 验证 Quote 中的 REPORT_DATA 包含挑战哈希
// 4. 验证 Quote 签名和测量值
// 5. 标记挑战为已使用

if result.verified {
    println!("✅ 挑战 - 响应认证成功！");
} else {
    println!("❌ 验证失败：{}", result.error);
}
```

### 状态管理

```rust
use vault_service::tee::challenge::{ChallengeState, ChallengeStore};

enum ChallengeState {
    Pending,      // 等待响应
    Verified,     // 已验证
    Expired,      // 已过期
    Failed,       // 验证失败
}

// 挑战存储
let mut store = ChallengeStore::new();

// 创建挑战
let challenge = store.create_challenge(nonce, ttl)?;

// 更新状态
store.mark_as_verified(&challenge_id)?;

// 清理过期挑战
store.cleanup_expired()?;
```

### 防重放攻击

```rust
use vault_service::tee::challenge::ChallengeProtocol;

let protocol = ChallengeProtocol::new(attestation_service);

// 每个挑战只能使用一次
let challenge = protocol.generate_challenge(None, None)?;

// 第一次验证
let result1 = protocol.verify_response(&response1, &identity)?;
assert!(result1.verified);

// 第二次使用相同的挑战
let result2 = protocol.verify_response(&response2, &identity)?;
assert!(!result2.verified);
assert_eq!(result2.error, "Challenge already used");
```

---

## API 参考

### 端点列表

| 端点                                  | 方法 | 描述           | 认证要求 |
| ------------------------------------- | ---- | -------------- | -------- |
| `/api/v1/attestation/challenge`       | POST | 创建认证挑战   | 公开     |
| `/api/v1/attestation/verify-response` | POST | 验证挑战响应   | 公开     |
| `/api/v1/attestation/status`          | GET  | 获取认证状态   | 公开     |
| `/api/v1/attestation/quote`           | GET  | 获取当前 Quote | 公开     |
| `/api/v1/attestation/verify`          | POST | 验证 Quote     | 公开     |
| `/api/v1/attestation/report`          | GET  | 获取认证报告   | 公开     |
| `/api/v1/attestation/refresh`         | POST | 刷新 Quote     | 公开     |
| `/api/v1/attestation/health`          | GET  | 健康检查       | 公开     |

### 创建挑战

```bash
curl -X POST http://localhost:8080/api/v1/attestation/challenge \
  -H "Content-Type: application/json" \
  -d '{}'
```

**响应**:

```json
{
  "success": true,
  "challenge_id": "chal_abc123",
  "nonce": "a1b2c3d4e5f6...",
  "quote_b64": "base64_encoded_quote",
  "expires_at": 1709654400,
  "mrenclave": "e3b0c44298fc1c149afbf4c8996fb924...",
  "mrsigner": "7d865e959b2466918c9863afca942d0f..."
}
```

### 验证响应

```bash
curl -X POST http://localhost:8080/api/v1/attestation/verify-response \
  -H "Content-Type: application/json" \
  -d '{
    "challenge_id": "chal_abc123",
    "quote_b64": "base64_encoded_quote"
  }'
```

**响应**:

```json
{
  "success": true,
  "verified": true,
  "mrenclave": "e3b0c44298fc1c149afbf4c8996fb924...",
  "mrsigner": "7d865e959b2466918c9863afca942d0f...",
  "timestamp": 1709654400
}
```

### 验证 Quote

```bash
curl -X POST http://localhost:8080/api/v1/attestation/verify \
  -H "Content-Type: application/json" \
  -d '{
    "quote_b64": "base64_encoded_quote",
    "nonce": "optional_nonce_hex"
  }'
```

### 获取认证报告

```bash
curl http://localhost:8080/api/v1/attestation/report
```

**响应**:

```json
{
  "success": true,
  "data": {
    "version": "3",
    "mrenclave": "e3b0c44298fc1c149afbf4c8996fb924...",
    "mrsigner": "7d865e959b2466918c9863afca942d0f...",
    "security_version": 1,
    "product_id": 1234,
    "attributes": "0x0000000000000000",
    "timestamp": 1709654400,
    "quote_b64": "base64_encoded_quote",
    "certificate_info": {
      "subject": "CN=Intel SGX Attestation Key",
      "issuer": "CN=Intel SGX Root CA",
      "not_before": "2024-01-01T00:00:00Z",
      "not_after": "2025-01-01T00:00:00Z",
      "fingerprint": "SHA256:abcdef123456..."
    }
  }
}
```

---

## 相关文档

- [TEE 安全架构](tee-security-architecture.md) - Enclave 设计
- [Quote 管理](Quote 管理.md) - Quote 详细管理
- [挑战响应机制](挑战响应机制.md) - 挑战 - 响应协议
- [ATTESTATION-API](../03-api-reference/ATTESTATION-API.md) - API 文档
- [架构设计](../01-project-overview/architecture.md) - 整体架构

---

**文档版本**: v1.0  
**最后更新**: 2026-03-20
