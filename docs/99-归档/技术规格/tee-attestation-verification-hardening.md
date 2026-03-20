---
title: 'TEE Attestation 验证加固：证书链与签名完整验证'
slug: 'tee-attestation-verification-hardening'
created: '2026-03-19'
status: 'implemented'
priority: 'P2'
---

# Tech-Spec: TEE Attestation 验证加固：证书链与签名完整验证

## 概述

### 问题陈述

当前 attestation 验证流程存在两处关键安全简化：

1. **`src/tee/dcap.rs:927`** — `verify_certificate_chain()` 对非模拟模式直接返回 `Ok(())`，未实际验证 PCK 证书链到 Intel SGX 根证书的信任链。
2. **`src/tee/attestation.rs:897`** — `verify_signature()` 仅检查签名 r/s 分量是否全零（格式检查），未使用验证者公钥执行真实 ECDSA-P256 签名验证。

这两处简化共同导致：即使收到伪造的 DCAP Quote（签名无效、证书链断裂），验证也会通过——在生产环境中意味着完全失去远程认证的安全保证。

### 解决方案

1. 在 `verify_certificate_chain()` 中使用 `webpki`（或 `rustls-webpki`）库验证 PCK 证书链，锚定到项目已内置的 `INTEL_SGX_ROOT_CERT_PEM`。
2. 在 `verify_signature()` 中使用 `ring` 库（已作为依赖引入）执行完整 ECDSA-P256 签名验证，公钥来自 Quote 签名数据结构中的 Attestation Key。

### 范围

- `src/tee/dcap.rs`：`DcapVerifier::verify_certificate_chain()` 方法（第 916-929 行）
- `src/tee/attestation.rs`：`AttestationService::verify_signature()` 方法（第 894-909 行）
- `Cargo.toml`（主项目）：确认 `webpki` / `rustls-webpki` 依赖已引入或新增

---

## 开发上下文

### 当前代码（关键片段）

#### 1. `src/tee/dcap.rs:916-929` — 证书链验证（当前为空实现）

```rust
/// 验证证书链
fn verify_certificate_chain(&self, _quote: &DcapQuote) -> Result<(), DcapError> {
    // 实际实现中：
    // 1. 解析 PCK 证书链
    // 2. 验证每个证书的有效性
    // 3. 验证链到 Intel 根证书

    if self.config.simulation_mode {
        return Ok(());
    }

    // 这里简化处理，实际使用 webpki 或类似库   ← 第 927 行
    Ok(())
}
```

#### 2. `src/tee/attestation.rs:894-909` — 签名验证（当前仅格式检查）

```rust
/// 验证签名
fn verify_signature(&self, quote: &Quote) -> Result<(), AttestationError> {
    // 在实际实现中，这里会使用验证者公钥验证 ECDSA 签名
    // 这里简化处理，仅检查签名格式               ← 第 897 行
    if quote.signature.isv_enclave_report_signature.r == [0u8; 32]
        && quote.signature.isv_enclave_report_signature.s == [0u8; 32]
    {
        // 空签名，在模拟模式下允许
        if self.allow_simulation {
            return Ok(());
        }
        return Err(AttestationError::SignatureVerificationFailed);
    }

    Ok(())
}
```

#### 3. `src/tee/dcap.rs` — 已内置的 Intel SGX 根证书（可复用）

```rust
// src/tee/dcap.rs:46-58
/// Intel SGX 根证书（PEM 格式）
pub const INTEL_SGX_ROOT_CERT_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIICjzCCAhSgAwIBAgIUImUM1lqdNInzg7SVUr9QGzknBqwwCgYIKoZIzj0EAwIw
...
-----END CERTIFICATE-----"#;
```

#### 4. `src/tee/dcap.rs` — `verify_quote_signature()` 中也存在同样问题（dcap 层）

```rust
// src/tee/dcap.rs:898-914
fn verify_quote_signature(&self, quote: &DcapQuote) -> Result<(), DcapError> {
    if self.config.simulation_mode {
        return Ok(());
    }

    // 验证签名格式（仅零值检查，非真实验证）
    if quote.signature.isv_enclave_report_signature.r == [0u8; 32]
        && quote.signature.isv_enclave_report_signature.s == [0u8; 32]
    {
        return Err(DcapError::SignatureVerificationFailed);
    }

    Ok(())
}
```

### 相关数据结构（需确认字段路径）

```rust
// 根据 src/tee/attestation.rs 中的常量推断
pub const ECDSA_P256_SIGNATURE_LEN: usize = 64;   // r(32) + s(32)
pub const ECDSA_P256_PUBLIC_KEY_LEN: usize = 64;  // uncompressed x(32)+y(32)

// Quote.signature 包含：
// - isv_enclave_report_signature: EcdsaSignature { r: [u8;32], s: [u8;32] }
// - qe_report_signature: EcdsaSignatureDcap
// - qe_certification_data: Vec<u8>  ← 包含 PCK 证书链（PEM 拼接）
// - qe_authentication_data: Vec<u8> ← 包含 Attestation Key (ECDSA 公钥)
```

### SGX DCAP 验证流程（完整参考）

```
Quote
 ├── Header
 ├── ISV Enclave Report (384 bytes)
 └── Quote Signature Data (ECDSA)
      ├── isv_enclave_report_signature  ← ECDSA(ISV Report, Attestation Key)
      ├── ECDSA Attestation Key         ← in qe_authentication_data (64 bytes)
      ├── QE Report                     ← contains hash(Att.Key) in report_data
      ├── qe_report_signature           ← ECDSA(QE Report, PCK Key)
      └── qe_certification_data         ← PCK cert chain (PEM)
```

验证顺序：
1. 解析 `qe_certification_data` 中的 PCK 证书链
2. 用 `webpki` 验证 PCK 链 → Intel SGX 根证书
3. 从 PCK 叶证书提取 PCK 公钥
4. 用 `ring` 验证 `qe_report_signature`（ECDSA on QE Report, PCK key）
5. 验证 QE Report 的 `report_data` == SHA-256(Attestation Key ‖ qe_authentication_data)
6. 提取 Attestation Key（来自 `qe_authentication_data`）
7. 用 `ring` 验证 `isv_enclave_report_signature`（ECDSA on ISV Report, Att. Key）

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/tee/dcap.rs` | 目标：`verify_certificate_chain()`（第 916 行）和 `verify_quote_signature()`（第 898 行） |
| `src/tee/attestation.rs` | 目标：`verify_signature()`（第 894 行） |
| `Cargo.toml` | 确认 `ring`、`webpki`/`rustls-webpki` 依赖 |
| `src/tee/dcap.rs:46-58` | 复用 `INTEL_SGX_ROOT_CERT_PEM` 常量 |

### 技术决策

#### 证书链验证（`dcap.rs:927`）

使用 `webpki`（或其维护分支 `rustls-webpki`）：

```toml
# Cargo.toml
[dependencies]
webpki         = { version = "0.22", features = ["alloc"] }
# 或使用更新维护的分支：
# rustls-webpki = { version = "0.102", features = ["alloc"] }
```

验证步骤伪代码：
```rust
use webpki::{TrustAnchor, EndEntityCert, Time, ECDSA_P256_SHA256};

// 1. 解析根证书为 TrustAnchor
let root_der = pem_to_der(INTEL_SGX_ROOT_CERT_PEM)?;
let trust_anchors = [TrustAnchor::try_from_cert_der(&root_der)?];

// 2. 从 qe_certification_data 解析证书链（PEM 拼接 → DER 列表）
let (leaf_der, intermediates) = parse_pem_chain(&quote.signature.qe_certification_data)?;

// 3. 验证端实体证书
let end_entity = EndEntityCert::try_from(leaf_der.as_ref())?;
end_entity.verify_for_usage(
    &[ECDSA_P256_SHA256],
    &trust_anchors,
    &intermediates,
    Time::from_seconds_since_unix_epoch(current_unix_time()),
    webpki::KeyUsage::client_auth(),
    None,
)?;
```

#### 完整签名验证（`attestation.rs:897`）

`ring` 库已作为依赖引入（`src/tee/dcap.rs:26` 中可见 `use ring::digest::{SHA256, digest};`）：

```rust
use ring::signature::{self, UnparsedPublicKey, ECDSA_P256_SHA256_FIXED};

// 从 qe_authentication_data 提取 Attestation Key（64字节无压缩格式）
// ring 需要 0x04 前缀的65字节格式
let mut pub_key_bytes = vec![0x04u8];
pub_key_bytes.extend_from_slice(&quote.signature.qe_authentication_data[..64]);

// 将签名 r||s（64字节）转换为 ring 接受的格式
let sig_bytes: Vec<u8> = [
    quote.signature.isv_enclave_report_signature.r,
    quote.signature.isv_enclave_report_signature.s,
].concat();

// 被签名内容 = ISV Enclave Report 原始字节（384字节）
let message = quote.report_body.as_bytes();

let public_key = UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, &pub_key_bytes);
public_key.verify(message, &sig_bytes)
    .map_err(|_| AttestationError::SignatureVerificationFailed)?;
```

---

## 实现计划

### 任务

1. **DCAP 证书链验证** — `src/tee/dcap.rs:927`
   - 在 `Cargo.toml` 中添加 `webpki` / `rustls-webpki` 依赖
   - 实现 PEM 链解析辅助函数（`parse_pem_cert_chain(data: &[u8]) -> Result<(Vec<u8>, Vec<Vec<u8>>), DcapError>`）
   - 在 `verify_certificate_chain()` 中调用 webpki 完成端到端证书链验证
   - 验证锚点：复用 `INTEL_SGX_ROOT_CERT_PEM` 常量

2. **DCAP Quote 签名验证** — `src/tee/dcap.rs`（`verify_quote_signature()`，与 attestation.rs 同类问题）
   - 提取 `qe_authentication_data` 中的 Attestation Key（64字节）
   - 使用 `ring::signature::UnparsedPublicKey::verify()` 验证 `isv_enclave_report_signature`
   - 同步验证 `qe_report_signature`（对 QE Report 的签名，公钥来自 PCK 叶证书）

3. **AttestationService 签名验证** — `src/tee/attestation.rs:897`
   - 在 `verify_signature()` 中实现与任务2相同的 ECDSA 验证逻辑
   - 注意：`AttestationService` 使用的 `Quote` 类型与 `DcapVerifier` 使用的 `DcapQuote` 类型可能不同，需对齐字段名称

4. **错误信息增强**
   - `SignatureVerificationFailed` 和 `CertificateVerificationFailed` 错误应携带失败原因字符串，便于调试
   - 但不得在错误信息中暴露签名原始字节或证书内容（安全）

5. **负面测试用例**
   - 测试全零签名（现有）→ 返回 `Err`
   - 测试正确格式但错误值的签名 → 返回 `Err`
   - 测试证书链断裂（缺中间证书）→ 返回 `Err`
   - 测试根证书不匹配 → 返回 `Err`
   - 测试模拟模式下跳过验证 → 返回 `Ok`

### 验收标准

- [ ] 非模拟模式下，`verify_certificate_chain()` 对有效 PCK 链返回 `Ok`，对伪造/截断链返回 `Err`
- [ ] 非模拟模式下，`verify_signature()` 对有效 ECDSA 签名返回 `Ok`，对任意字节错误签名返回 `Err`
- [ ] `simulation_mode = true` 时，两个函数均跳过验证并返回 `Ok`（兼容现有行为）
- [ ] 通过 `cargo test` 中所有负面测试用例
- [ ] 引入 `webpki` 后，`cargo audit` 无新的高危漏洞报告
- [ ] `cargo clippy -- -D warnings` 无新增警告

---

## 附加上下文

### 依赖（所需 crate）

```toml
# Cargo.toml（主项目）

[dependencies]
# ring 已存在（用于 digest），确认版本 ≥ 0.17 以支持 ECDSA_P256_SHA256_FIXED
ring = "0.17"

# 新增：证书链验证
webpki = { version = "0.22", features = ["alloc"] }
# 或（更新维护，API 兼容）：
# rustls-webpki = { version = "0.102", features = ["alloc"] }

# PEM 解码（若未引入）
pem = "3"
# 或 rustpki-pem = "0.5"
```

### 测试策略

| 测试类型 | 方式 | 说明 |
|---------|------|------|
| 正路径（模拟模式） | 现有单元测试 | 不需要真实证书 |
| 负路径（签名伪造） | 单元测试，手动构造错误 Quote | 可在 CI 运行 |
| 负路径（证书链断裂） | 单元测试，截断 PEM 数据 | 可在 CI 运行 |
| 正路径（真实 Quote） | 集成测试，需真实 TEE 或离线 Quote 样本 | 使用 Intel 公开测试 Quote 样本 |

**建议**：从 Intel DCAP 测试套件或 [confidential-containers/attestation-agent](https://github.com/confidential-containers/attestation-agent) 获取公开测试 Quote 样本用于集成测试，无需真实 TEE 硬件。

### 注意事项

1. **`webpki` 时间验证**：webpki 会检查证书有效期，需传入准确的当前时间；使用 `SystemTime::now()` 转换为 Unix 秒即可。
2. **证书链顺序**：SGX PCK 证书链通常为 `[叶证书, 中间CA, 根CA]` 的 PEM 拼接；`webpki` 期望叶证书单独传入，其余作为中间证书列表。
3. **`ring` 公钥格式**：`ECDSA_P256_SHA256_FIXED` 期望 `0x04 ‖ x ‖ y`（65 字节），而 Quote 中存储的是裸 64 字节；拼接 `0x04` 前缀是关键步骤。
4. **两处验证逻辑的复用**：`DcapVerifier`（`dcap.rs`）和 `AttestationService`（`attestation.rs`）存在重复的签名验证逻辑，建议提取为 `src/tee/crypto_utils.rs` 中的共享函数，避免维护两份实现。
5. **安全合规**：修改完成后必须通过 code-reviewer 的安全审查，重点关注：常量时间比较（签名验证使用 ring 已满足此要求）、错误信息不泄露敏感数据。
