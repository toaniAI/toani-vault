---
title: 'TEE 核心模块对抗性代码审查报告'
created: '2026-03-19'
reviewer: 'adversarial-code-reviewer'
severity-levels: CRITICAL / HIGH / MEDIUM / LOW
---

# TEE 核心模块对抗性代码审查报告

**审查日期**: 2026-03-19
**审查范围**: CredBridge TEE 核心模块（8个源文件 + 7份 Tech-Spec）
**审查方法**: 对抗性安全审查 + Tech-Spec 符合性检查

---

## 目录

1. [执行摘要](#执行摘要)
2. [各平台 Quote 生成实现状态](#各平台-quote-生成实现状态)
3. [严重安全漏洞（CRITICAL）](#严重安全漏洞critical)
4. [高危安全问题（HIGH）](#高危安全问题high)
5. [中危问题（MEDIUM）](#中危问题medium)
6. [低危问题（LOW）](#低危问题low)
7. [Tech-Spec 符合性矩阵](#tech-spec-符合性矩阵)
8. [修复优先级建议](#修复优先级建议)

---

## 执行摘要

本次审查覆盖以下 8 个文件：

| 文件 | 行数 | 风险评级 |
|------|------|---------|
| `vault-service/src/api/attestation.rs` | 383 | **CRITICAL** |
| `src/tee/attestation.rs` | 1404 | **CRITICAL** |
| `src/tee/dcap.rs` | 1397 | **CRITICAL** |
| `src/tee/driver_verify.rs` | 498 | HIGH |
| `src/tee/upgrade.rs` | 1123 | HIGH |
| `src/tee/sandbox/export/data_export.rs` | 752 | HIGH |
| `src/tee/sandbox/export/screenshot.rs` | 1015 | HIGH |
| `src/tee/sandbox/export/watermark.rs` | 655 | HIGH |

**核心结论**：

- **远程认证完全失效**：三大 TEE 平台（SGX/TDX/SEV-SNP）的 Quote 生成均返回 Err，生产环境下无法完成任何远程认证。
- **签名验证存在绕过路径**：`attestation.rs` 中非零签名在无公钥配置时静默通过，实际上接受所有伪造签名。
- **证书链验证为空实现**：`dcap.rs` 的 `verify_certificate_chain()` 仅做格式检查，任何伪造证书链均可通过。
- **7 份 Tech-Spec 均标记为 `implemented`，但实际均存在关键功能缺失**。

---

## 各平台 Quote 生成实现状态

### 文件：`vault-service/src/api/attestation.rs`

| 平台 | 函数 | 状态 | 说明 |
|------|------|------|------|
| SGX DCAP | `generate_real_quote()` (行 331) | **未实现** | 返回 `Err("SGX quote generation requires Intel SGX DCAP hardware...")` |
| TDX | `generate_real_quote()` (行 335) | **未实现** | 返回 `Err("TDX quote generation requires Intel TDX-capable hardware...")` |
| SEV-SNP | `generate_real_quote()` (行 339) | **未实现** | 返回 `Err("SEV-SNP attestation requires AMD EPYC 7003+...")` |
| Simulation | `generate_simulated_quote()` | 已实现（非加密） | 返回格式为 `SIMULATED_QUOTE_<timestamp>_<uuid>` 的非加密字符串 |

**关键缺失**（对比 Tech-Spec `tee-attestation-quote-generation.md`）：

1. **Cargo.toml 未引入平台 SDK 依赖**：`dcap-qbg`、`tdx-attest`、`sev` crate 均未出现在 `vault-service/Cargo.toml` 中，无 feature flag 隔离。
2. **`report_data` 参数缺失**：`generate_real_quote()` 签名为 `fn(state: &AttestationState) -> Result<String, String>`，Tech-Spec 要求增加 `report_data: Option<&[u8; 64]>` 参数以绑定 challenge/nonce。
3. **`AttestationState` 结构体缺少 `challenge_data` 字段**：Tech-Spec 要求 `challenge_data: Option<[u8; 64]>` 字段以传递认证挑战值。
4. **错误类型未规范化**：返回类型仍为 `Result<String, String>`，Tech-Spec 要求改为 `Result<String, AttestationApiError>`。

**影响**：在任何真实 TEE 硬件上调用认证 API，均会收到错误响应，vault-service 无法向任何方证明自身运行在受信任执行环境中。这是系统核心安全承诺的完全失效。

---

## 严重安全漏洞（CRITICAL）

### CRIT-001：签名验证存在可利用的绕过路径

**文件**: `src/tee/attestation.rs`
**行号**: 930–940（`verify_signature` 方法尾部）
**严重程度**: CRITICAL
**CVSS 评估**: 9.8（远程攻击，无需认证，完整性丧失）

**漏洞代码**：

```rust
// 行 930-940
if let Some(ref vk) = self.verifier_public_key {
    let mut pub_key_bytes = vec![0x04u8];
    pub_key_bytes.extend_from_slice(vk);
    let sig_bytes: Vec<u8> = [sig.r, sig.s].concat();
    let message = quote.report_body.as_bytes();
    let public_key = UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, &pub_key_bytes);
    public_key.verify(message, &sig_bytes)
        .map_err(|_| AttestationError::SignatureVerificationFailed)?;
    return Ok(());
}
// DANGER: 当 verifier_public_key 为 None 时，所有非零签名都在此处静默通过
Ok(())  // 行 940
```

**攻击场景**：攻击者构造一个包含任意非零 `isv_enclave_report_signature`（r 和 s 非全零即可）的伪造 Quote，在系统未配置 `verifier_public_key` 的情况下（这是默认状态，因为密钥配置是可选的），该签名将无条件通过验证，攻击者可使用任意伪造 Quote 骗过认证系统。

**根因**：安全降级设计缺陷——可选的 `verifier_public_key` 字段本应是 Fail-Closed 设计（无公钥则拒绝），实际上是 Fail-Open（无公钥则放行）。

**修复方向**：当 `verifier_public_key` 为 `None` 且非模拟模式时，应返回 `Err(AttestationError::SignatureVerificationFailed)` 而非 `Ok(())`。

---

### CRIT-002：证书链验证为空实现，任意伪造证书均可通过

**文件**: `src/tee/dcap.rs`
**行号**: 916–929（`verify_certificate_chain` 方法）
**严重程度**: CRITICAL
**CVSS 评估**: 9.1（证书链验证完全失效）

**漏洞代码**：

```rust
fn verify_certificate_chain(&self, _quote: &DcapQuote) -> Result<(), DcapError> {
    if self.config.simulation_mode {
        return Ok(());
    }
    // 行 927：注释说"这里简化处理，实际使用 webpki 或类似库"
    // 实际上直接返回 Ok，未做任何 PKI 路径验证
    Ok(())
}
```

**攻击场景**：攻击者提交一个包含自签名 PCK 证书（与 Intel SGX 根证书无任何信任关系）的 DCAP Quote，`verify_certificate_chain()` 不会拒绝该证书，使得攻击者可以使用自己控制的 PCK 密钥伪造整个 Quote 链。

**额外发现**：代码中存在 `let _ = expected_fingerprint;` 的死代码（行 ~950），表明有过 webpki 集成尝试但已被废弃，却未在注释中标记。

**修复方向**：集成 `webpki` 或 `rustls-webpki`，使用 `INTEL_SGX_ROOT_CERT_PEM` 常量作为信任锚进行完整 PKI 路径验证。

---

### CRIT-003：Quote 签名验证（DCAP 层）仅检查零值，非真实 ECDSA 验证

**文件**: `src/tee/dcap.rs`
**行号**: 898–914（`verify_quote_signature` 方法）
**严重程度**: CRITICAL
**CVSS 评估**: 9.1

**漏洞代码**：

```rust
fn verify_quote_signature(&self, quote: &DcapQuote) -> Result<(), DcapError> {
    if self.config.simulation_mode {
        return Ok(());
    }
    // 仅检查是否全零，非真实 ECDSA 验证
    if quote.signature.isv_enclave_report_signature.r == [0u8; 32]
        && quote.signature.isv_enclave_report_signature.s == [0u8; 32]
    {
        return Err(DcapError::SignatureVerificationFailed);
    }
    Ok(())  // 任何非零 r/s 均通过
}
```

**攻击场景**：攻击者提交 r=[1,0,...,0], s=[1,0,...,0] 这样的随机值作为签名，由于 ECDSA 验证从未执行，该签名通过，从而使任意伪造的 ISV Enclave Report 被接受。

**修复方向**：从 `qe_authentication_data` 提取 Attestation Key（64字节），使用 `ring::signature::UnparsedPublicKey` 执行真实 ECDSA-P256 验证。

---

### CRIT-004：Nonce 验证为空桩，重放攻击完全不防御

**文件**: `src/tee/dcap.rs`
**行号**: ~960（`verify_nonce` 方法）
**严重程度**: CRITICAL
**CVSS 评估**: 8.6（重放攻击）

**漏洞代码**：

```rust
fn verify_nonce(&self, _nonce: &str) -> Result<(), DcapError> {
    // 完全为空桩
    Ok(())
}
```

**攻击场景**：攻击者截获一份历史合法 Quote（包含有效证书链和签名），在数天/数周后重放该 Quote，由于 nonce 验证从未执行，服务器将接受该已过期的 Quote，允许攻击者伪装成合法的 TEE 节点。这是典型的重放攻击场景。

**修复方向**：验证 Quote 中的 nonce/report_data 是否与当前挑战值匹配，使用时间窗口限制（建议 5 分钟内有效）。

---

### CRIT-005：`generate_real_quote()` 在所有真实 TEE 平台均返回错误

**文件**: `vault-service/src/api/attestation.rs`
**行号**: 331、335、339
**严重程度**: CRITICAL
**CVSS 评估**: N/A（功能性安全失效而非可利用漏洞）

如前文"各平台 Quote 生成实现状态"所述，vault-service 的远程认证在生产环境中完全无法工作。所有 TEE 平台的 Quote 生成均返回 Err，Tech-Spec 标记为 `implemented` 的 3 个平台分支实际上均为 TODO 占位。

---

## 高危安全问题（HIGH）

### HIGH-001：驱动公钥加载来自环境变量而非 Vault，偏离安全架构设计

**文件**: `src/tee/driver_verify.rs`
**行号**: 371–405（`get_trusted_public_key` 函数）
**严重程度**: HIGH
**Tech-Spec 对应**: `tee-driver-verify-public-key-loading.md`（标记为 `implemented`，实际为部分实现）

**问题描述**：
`get_trusted_public_key()` 从 `TRUSTED_PUBLIC_KEY_<FINGERPRINT>` 环境变量加载公钥（带回退到 `TRUSTED_PUBLIC_KEY_DEFAULT`），而非 Tech-Spec 要求的 Vault KV 集成。

**偏差点**：
1. 函数仍为同步 `fn`，Tech-Spec 要求重构为 `async fn`。
2. 无 DashMap TTL 缓存（TTL 5分钟），每次调用均查询（或在此处均为读取环境变量）。
3. 无 Fail-Closed 机制——若环境变量未设置，`log::warn!` 后返回 `None`，上层会收到"未找到受信任的公钥"错误，但系统未设置此变量时的行为取决于调用方如何处理 `None`。
4. Vault 路径约定（`secret/tee/trusted-keys/<fingerprint>`）未实现。

**安全影响**：环境变量可被有 shell 访问权限的进程读取，相比 Vault 的访问控制和审计日志，安全级别大幅降低。攻击者若能设置进程环境变量，即可注入伪造的信任公钥。

---

### HIGH-002：ECDSA 验证传入 hex 解码后的哈希值——存在双重哈希问题

**文件**: `src/tee/driver_verify.rs`
**行号**: 251–263（`TeeDriverVerifier::verify_signature` 方法）
**严重程度**: HIGH

**漏洞代码**：

```rust
fn verify_signature(&self, data_hash: &str, signature: &[u8]) -> Result<bool, DriverVerifyError> {
    let hash_bytes = hex::decode(data_hash)  // SHA-256 哈希的 hex 解码 = 32字节
        .map_err(|e| DriverVerifyError::SignatureVerificationFailed(e.to_string()))?;

    for public_key in &self.trusted_public_keys {
        let public_key = UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, public_key);
        match public_key.verify(&hash_bytes, signature) {  // 传入已哈希的数据
```

**问题**：`ECDSA_P256_SHA256_ASN1` 会在验证前对 message 再次执行 SHA-256。因此：
- 签名生成时若直接对原始数据签名：`ECDSA(SHA256(data))`
- 验证时：`verify(SHA256(data_bytes), sig)` = `ECDSA(SHA256(SHA256(data)))`，两者不匹配，验证始终失败。

这意味着即使传入了正确的签名，验证也会返回 `false`，驱动验证功能实际上完全失效。正确做法是传入原始驱动字节（未哈希）让 `ring` 完成哈希，或使用 `ECDSA_P256_SHA256_FIXED_SIGNING` 这类接受预哈希输入的算法。

---

### HIGH-003：数据导出清单哈希使用非加密 DefaultHasher

**文件**: `src/tee/sandbox/export/data_export.rs`
**行号**: ~340（`compute_hash` 方法）
**严重程度**: HIGH

**漏洞代码**：

```rust
fn compute_hash(&self, data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())  // 仅 64 位输出
}
```

`DefaultHasher` 是为 `HashMap` 性能优化设计的哈希函数，非加密哈希，具有以下特征：
1. 输出仅 64 位，碰撞概率远高于 SHA-256（256 位）。
2. 可通过精心构造的数据产生哈希碰撞（哈希洪水攻击）。
3. 该哈希值被写入安全清单（manifest）的 `data_hash` 字段，用于验证导出数据完整性——安全清单使用非加密哈希本质上使完整性保护形同虚设。

**修复方向**：改用 `ring::digest::digest(&SHA256, data)` 并输出 hex 编码的 256 位摘要。

---

### HIGH-004：HMAC 密钥由 `enclave_id` 确定性派生，可被任何知道 enclave_id 的方伪造

**文件**: `src/tee/sandbox/export/watermark.rs`
**行号**: ~480–510（`derive_default_hmac_key` 函数）
**严重程度**: HIGH

**漏洞代码**：

```rust
fn derive_default_hmac_key(&self) -> Vec<u8> {
    use ring::digest::{SHA256, digest};
    let salt = b"credbridge-watermark-key-v1";
    let input = format!("{}{}", salt_str, self.enclave_id);
    digest(&SHA256, input.as_bytes()).as_ref().to_vec()
}
```

`enclave_id` 是系统标识符，在运行时内多处可见，并非秘密。任何知道 `enclave_id` 的方（包括获取系统信息的攻击者）均可：
1. 使用相同算法派生出相同的 HMAC 密钥。
2. 对伪造的水印文本计算有效 HMAC。
3. 将该 HMAC 嵌入伪造截图，使 `verify_watermark` 返回 `Ok(true)`。

水印的信任根完全依赖于密钥的保密性；当密钥可从公开信息派生时，水印验证提供的安全保障为零。

**修复方向**：密钥必须来自 `EnclaveKeyManager`（可信密钥存储），而非从公开可见的 `enclave_id` 派生。Tech-Spec `tee-106-watermark-verification.md` 已明确指出应通过 `WatermarkService::with_key(enclave_id, hmac_key: Vec<u8>)` 注入密钥。

---

### HIGH-005：升级流程中 TEE-205 回滚未从 Vault 恢复 Sealing Key

**文件**: `src/tee/upgrade.rs`
**行号**: ~542–590（`rollback` 方法）
**严重程度**: HIGH
**Tech-Spec 对应**: `tee-upgrade-pipeline-implementation.md` TEE-205

**问题描述**：`rollback()` 仅将 `pending_version` 清空，并从内存中的 `active_version` 恢复状态，未从 Vault 读取升级前保存的 Sealing Key 备份。

Tech-Spec 要求：
1. 从 Vault 读取键路径 `tee/sealing-key/backup/<upgrade_start_timestamp>` 处的备份。
2. 将 `sealing_key_custody` 恢复为备份状态。
3. 若 Vault 读取失败，返回 `UpgradeError::RollbackFailed`（Fail-Closed）。

**实际行为**：当新 Enclave 已接管部分 Sealing Key 权限、升级过程中发生故障需要回滚时，`sealing_key_custody` 中的 `active_mrenclave` 仍指向新版本，旧 Enclave 无法再正确解封密钥材料，导致凭证无法解密，服务数据不可达。

**前置条件缺失**：`start_upgrade` 中未将当前 `sealing_key_custody` 序列化并加密存入 Vault，导致即使实现了 Vault 读取逻辑，也没有可用的备份数据。

---

### HIGH-006：`verify_watermark` 为空实现，始终返回 `Ok(true)`

**文件**: `src/tee/sandbox/export/watermark.rs`
**行号**: 145–149
**严重程度**: HIGH
**Tech-Spec 对应**: `tech-spec-tee-106-watermark-verification.md`（标记为 `implemented`）

**漏洞代码**：

```rust
pub fn verify_watermark(&self, _image_data: &[u8]) -> Result<bool, ExportError> {
    // TODO(#TEE-106): 实现水印验证逻辑
    Ok(true)
}
```

任何图片（包括无水印图片、伪造水印图片、完全不同的截图）均通过水印验证。截图的来源可信性验证机制完全失效。

---

### HIGH-007：`execute_operation` 和 `take_screenshot` 为模拟实现，从未执行真实操作

**文件**: `src/api/websocket.rs`
**行号**: 620–645（`execute_operation`），647–666（`take_screenshot`）
**严重程度**: HIGH
**Tech-Spec 对应**: `tech-spec-tee-107-websocket-sandbox-execute.md`、`tech-spec-tee-108-websocket-screenshot.md`（均标记为 `implemented`）

`execute_operation` 仍使用 `tokio::time::sleep(Duration::from_millis(1500))` 并返回硬编码成功结果；`take_screenshot` 仍返回 1x1 像素最小 PNG 字节数组。两个 Tech-Spec 均已标记 `implemented` 但均为 TODO 占位。

**影响**：
- 所有通过 WebSocket 发出的浏览器操作（Navigate、Click、Fill、GetText 等）实际上从未被执行，客户端收到假数据。
- 截图 API 返回 1x1 占位图，无法用于任何安全审计目的。

---

## 中危问题（MEDIUM）

### MED-001：`migrate_traffic` 忽略 percentage 参数，始终一步迁移到 100%

**文件**: `src/tee/upgrade.rs`
**行号**: ~456–480（`migrate_traffic` 方法）
**严重程度**: MEDIUM
**Tech-Spec 对应**: `tee-upgrade-pipeline-implementation.md` TEE-203

**问题描述**：Tech-Spec 要求 `migrate_traffic(10)` 保持在 `MigratingTraffic` 阶段，仅在 `percentage == 100` 时转换到 `MigratingSealingKey`。实际代码无论 `percentage` 传入何值，均立即转换到 `MigratingSealingKey`，渐进式流量迁移模式（10%→50%→100%）完全无效。

**影响**：无法实现逐步灰度升级，一旦调用 `migrate_traffic()`，立即进入 Sealing Key 迁移阶段，没有验证窗口。

---

### MED-002：截图完整性验证在冻结超时时错误返回成功

**文件**: `src/tee/sandbox/export/screenshot.rs`
**行号**: ~750（`verify_screenshot_integrity` 方法）
**严重程度**: MEDIUM

**漏洞代码**（逻辑错误）：

```rust
pub fn verify_screenshot_integrity(&self, state: &FrozenPageState) -> bool {
    let elapsed = state.frozen_at.elapsed();
    if elapsed > self.max_freeze_duration {
        // BUG: 冻结超时时应返回 false（TOCTOU 窗口已过期），实际返回 true
        return true;  // 应为 return false
    }
    // ...
}
```

当页面冻结时间超过最大允许时长时，意味着截图发生时的 TOCTOU（Time-of-Check Time-of-Use）保护窗口已失效，页面内容可能已被修改。此时应拒绝该截图（返回 `false`），但代码错误地返回 `true`，静默放行了失效保护的截图。

---

### MED-003：CSS 选择器注入风险

**文件**: `src/tee/sandbox/export/screenshot.rs`
**行号**: ~680（`hide_selectors` 处理）
**严重程度**: MEDIUM

**漏洞代码**：

```rust
for selector in &config.hide_selectors {
    let js = format!("document.querySelectorAll('{}').forEach(el => el.style.display = 'none')", selector);
    playwright.evaluate_js(&js).await?;
}
```

`selector` 直接插入 JavaScript 字符串，若调用方传入包含 `'` 的选择器，会导致 JS 语法错误；若传入 `'); malicious_code('`，会导致 JavaScript 注入。虽然此处是沙箱内部调用，但攻击者若能控制 `ScreenshotConfig::hide_selectors` 的内容，即可在沙箱浏览器中执行任意 JS。

**修复方向**：对 `selector` 进行转义，或使用白名单验证（仅允许有效的 CSS 选择器字符集）。

---

### MED-004：DOM 哈希计算不包含实际 DOM 内容

**文件**: `src/tee/sandbox/export/screenshot.rs`
**行号**: ~820（`compute_dom_hash` 方法）
**严重程度**: MEDIUM

**漏洞代码**：

```rust
fn compute_dom_hash(&self, session_id: &str, timestamp: u64) -> String {
    // BUG: 使用时间戳和 session_id 计算"DOM 哈希"
    // 实际上与 DOM 内容无关
    use ring::digest::{SHA256, digest};
    let input = format!("{}:{}", session_id, timestamp);
    hex::encode(digest(&SHA256, input.as_bytes()))
}
```

函数名为 `compute_dom_hash`，但实际上只对 `session_id:timestamp` 字符串哈希，与实际的 DOM 内容完全无关。该值被写入截图元数据作为完整性依据，但无法检测 DOM 内容是否被篡改。

---

### MED-005：导出 JSON 清单格式破坏 JSON 合法性

**文件**: `src/tee/sandbox/export/data_export.rs`
**行号**: ~450（JSON 导出末尾追加清单）
**严重程度**: MEDIUM

**漏洞代码**：

```rust
let manifest_str = serde_json::to_string(&manifest)?;
// 将清单附加在 JSON 末尾，带 JS 风格注释
output.push_str(&format!("\n// MANIFEST\n{}", manifest_str));
```

标准 JSON 不支持 `//` 注释，在 JSON 数组/对象尾部追加内容会使整体结构变为非法 JSON。任何标准 JSON 解析器（`serde_json::from_str()`）在尝试解析此输出时将失败，导致导出数据实际上无法被正常解析。

---

### MED-006：XML 生成直接使用用户控制的键名作为 XML 标签，存在 XML 注入

**文件**: `src/tee/sandbox/export/data_export.rs`
**行号**: ~380（XML 生成逻辑）
**严重程度**: MEDIUM

**漏洞代码**：

```rust
for (key, value) in obj {
    xml.push_str(&format!("<{}>{}</{}>\n", key, value, key));
}
```

`key` 来自用户数据，若 `key` 包含 `>` 或 `<` 字符，即可注入任意 XML 内容，破坏 XML 结构或注入恶意标签。例如 `key = "foo></Record><InjectTag>bar"` 即可构造注入。

---

### MED-007：TEE-201 启动新 Enclave 使用不安全的 `unsafe { std::env::set_var }`

**文件**: `src/tee/upgrade.rs`
**行号**: ~410（`start_new_enclave` 实现）
**严重程度**: MEDIUM

**漏洞代码**：

```rust
unsafe {
    std::env::set_var("ENCLAVE_CONFIG_PATH", config_path);
    std::env::set_var("ENCLAVE_SEALING_KEY", sealing_key_b64);
}
```

在多线程异步运行时（tokio）中调用 `std::env::set_var` 是 UB（未定义行为），因为环境变量修改不是线程安全的。同时，Sealing Key 以环境变量形式传递给子进程，进程列表和 `/proc/<pid>/environ` 可能暴露该密钥。

---

### MED-008：CSV 导出使用首条记录字段作为所有记录的列头

**文件**: `src/tee/sandbox/export/data_export.rs`
**行号**: ~310（CSV 生成逻辑）
**严重程度**: MEDIUM

**漏洞代码**：

```rust
// 用第一条记录的 key 作为列头
if let Some(first) = data.first() {
    if let Value::Object(obj) = first {
        let headers: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        // ...
    }
}
```

若后续记录包含不同的字段（字段缺失或字段顺序不同），CSV 数据会产生列错位。对于来自异构来源的数据，这将导致数据完整性问题，且不报告任何错误。

---

## 低危问题（LOW）

### LOW-001：`generate_simulated_signature` 在 attestation.rs 和 dcap.rs 中行为不一致

**文件**: `src/tee/attestation.rs` vs `src/tee/dcap.rs`
**严重程度**: LOW

- `attestation.rs`：`r` 和 `s` 均设置为相同的 SHA-256 哈希前 32 字节（`r == s`）。
- `dcap.rs`：`s` 在 `r` 基础上 XOR `0x5C`（`r != s`，稍好）。

两种模拟签名的语义不同，可能导致测试在两个模块间的行为预期不一致。

---

### LOW-002：`INTEL_SGX_ROOT_CERT_PEM` 可能为占位内容

**文件**: `src/tee/dcap.rs`
**行号**: 46–58
**严重程度**: LOW

`INTEL_SGX_ROOT_CERT_PEM` 常量中的证书内容组织字段显示 "Intel Corp"（Intel 官方根证书使用 "Intel Corporation"），暗示该常量为测试占位值而非真实的 Intel SGX 根证书。若在集成 webpki 验证时直接使用此常量，对真实 Quote 的验证将始终失败。

---

### LOW-003：Intel PCS 生产和测试 URL 相同

**文件**: `src/tee/dcap.rs`
**行号**: ~70–75（URL 常量定义）
**严重程度**: LOW

```rust
pub const INTEL_PCS_URL: &str = "https://api.trustedservices.intel.com/sgx/certification/v4";
pub const INTEL_PCS_TEST_URL: &str = "https://api.trustedservices.intel.com/sgx/certification/v4";
```

两个 URL 值相同，在配置测试环境时无法区分生产和测试端点。

---

### LOW-004：`CertificateInfo` 中的有效期为已过期占位值

**文件**: `src/tee/dcap.rs`
**行号**: ~870（`CertificateInfo` 构建）
**严重程度**: LOW

```rust
"not_after": "2025-01-01T00:00:00Z"  // 已过期
```

硬编码的证书有效期已过期（2025-01-01），若上层基于此值做有效性判断，将始终视为过期证书。

---

### LOW-005：`redact_value` 显示敏感字符串的前 4 个字符

**文件**: `src/tee/sandbox/export/data_export.rs`
**行号**: ~180（`redact_value` 函数）
**严重程度**: LOW

```rust
fn redact_value(&self, value: &str) -> String {
    if value.len() > 4 {
        format!("{}****", &value[..4])
    }
    // ...
}
```

对于 API Key（如 `sk-abc123def456...`），前 4 字符可能已足以缩小搜索范围。建议对所有类型的敏感值统一使用 `[REDACTED]`，不显示任何明文字符。

---

### LOW-006：`apply_watermark_mock` 静默返回原始数据，无任何水印

**文件**: `src/tee/sandbox/export/watermark.rs`
**行号**: 400–409
**严重程度**: LOW

```rust
fn apply_watermark_mock(&self, image_data: &[u8], _watermark_text: &str, _config: &WatermarkConfig) -> Result<Vec<u8>, ExportError> {
    // 不添加任何水印，直接返回
    Ok(image_data.to_vec())
}
```

当主路径（`apply_watermark_with_image`）失败时，回退到此函数。调用方（`add_watermark`）不区分成功路径与回退路径，无法判断返回的图片是否包含水印。若系统后续对截图执行 `verify_watermark`，由于无签名，将返回 `false`——但如果 `verify_watermark` 仍为空实现（见 HIGH-006），该问题被掩盖。

---

### LOW-007：`PlaywrightClient::connect` 不验证浏览器版本或能力

**文件**: `src/tee/sandbox/export/screenshot.rs`
**行号**: ~80–120（`connect` 方法）
**严重程度**: LOW

CDP 连接建立后，`PlaywrightClient` 不检查连接的浏览器版本、支持的 CDP 域（Domain）列表或浏览器能力。在 Chromium 版本差异较大的环境中，某些 CDP 命令（如 DOM.freezePage）可能不受支持，导致静默失败。

---

### LOW-008：`wait_for_selector` 为空桩，固定 sleep 500ms

**文件**: `src/tee/sandbox/export/screenshot.rs`
**行号**: ~200（`wait_for_selector` 方法）
**严重程度**: LOW

```rust
async fn wait_for_selector(&self, _selector: &str) -> Result<(), ExportError> {
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(())
}
```

不轮询 DOM，500ms 对于某些异步加载的页面可能不够，对于快速加载的页面则浪费时间。

---

## Tech-Spec 符合性矩阵

| Tech-Spec | 标记状态 | 实际状态 | 关键差距 |
|-----------|---------|---------|---------|
| `tee-attestation-quote-generation.md` | `implemented` | **未实现** | 所有3个平台分支均返回 Err；无 SDK 依赖；无 report_data 参数 |
| `tee-attestation-verification-hardening.md` | `implemented` | **部分实现** | webpki 证书链验证未集成；ECDSA 验证有绕过路径（CRIT-001/002/003） |
| `tee-driver-verify-public-key-loading.md` | `implemented` | **替代方案** | 使用环境变量替代 Vault；同步函数；无缓存；无 Fail-Closed Vault 语义 |
| `tee-upgrade-pipeline-implementation.md` | `implemented` | **部分实现** | TEE-203 渐进迁移失效；TEE-205 无 Vault 回滚；TEE-201 使用 unsafe env |
| `tech-spec-tee-106-watermark-verification.md` | `implemented` | **未实现** | `verify_watermark` 仍为空实现返回 `Ok(true)` |
| `tech-spec-tee-107-websocket-sandbox-execute.md` | `implemented` | **未实现** | `execute_operation` 仍使用 sleep(1500ms) 模拟 |
| `tech-spec-tee-108-websocket-screenshot.md` | `implemented` | **未实现** | `take_screenshot` 仍返回 1x1 像素占位 PNG |

**结论**：7 份 Tech-Spec 全部标记为 `implemented`，实际上没有一份达到完整实现，3 份完全未实现。

---

## 修复优先级建议

### P0（立即修复，阻塞生产部署）

1. **CRIT-001**：修复 `verify_signature` 无公钥配置时的 Fail-Open 问题（`attestation.rs:940`）
2. **CRIT-002**：集成 `webpki` 实现真实 PCK 证书链验证（`dcap.rs:927`）
3. **CRIT-003**：实现真实 ECDSA-P256 Quote 签名验证（`dcap.rs:898–914`）
4. **CRIT-004**：实现 nonce/report_data 验证防重放攻击（`dcap.rs`）
5. **HIGH-002**：修复驱动验证双重哈希导致验证始终失败（`driver_verify.rs:257`）

### P1（Sprint 内修复）

6. **CRIT-005 + `tee-attestation-quote-generation.md`**：实现 SGX DCAP Quote 生成（至少 SGX 平台）
7. **HIGH-006 + `tee-106`**：实现 `verify_watermark` 真实 HMAC 验证
8. **HIGH-004**：修复 watermark HMAC 密钥派生机制，从 EnclaveKeyManager 注入
9. **HIGH-003**：替换 `DefaultHasher` 为 SHA-256（`data_export.rs`）
10. **HIGH-005**：实现 TEE-205 Vault Sealing Key 备份/恢复

### P2（下个迭代）

11. **HIGH-007 + TEE-107/108**：实现 WebSocket execute_operation 和 take_screenshot 真实集成
12. **MED-001**：修复 `migrate_traffic` 渐进迁移逻辑
13. **MED-002**：修复 `verify_screenshot_integrity` 超时时返回值逻辑
14. **MED-003**：修复 CSS 选择器注入
15. **MED-005/006**：修复 JSON 非法格式和 XML 注入

### P3（技术债清理）

16. **HIGH-001**：实现 Vault KV 公钥加载（`driver_verify.rs`）
17. **MED-007**：消除不安全的 env::set_var 和敏感数据环境变量传递
18. **LOW-001 ~ LOW-008**：其余低危问题

---

*报告生成时间: 2026-03-19*
*审查者: adversarial-code-reviewer*
