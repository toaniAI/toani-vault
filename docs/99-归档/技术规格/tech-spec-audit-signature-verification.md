---
title: 'P1 - 审计日志真实 Ed25519 签名验证'
slug: 'audit-signature-verification'
created: '2026-03-19'
status: 'implemented'
priority: 'P1'
---

# Tech-Spec: P1 - 审计日志真实 Ed25519 签名验证

## 概述

### 问题陈述

`src/api/audit.rs` 的 `verify_audit_log` 函数（第 661-667 行）中，签名验证步骤是假验证：

```rust
// 2. 验证签名（简化处理，实际应使用公钥验证）
let signature_valid = !entry.signature.is_empty();
```

这意味着任何非空字节序列都能通过签名验证，完全失去了数字签名的安全意义：
- 恶意篡改的审计条目，只要保留非空的签名字段，验证结果就会显示"通过"
- API 返回的 `verified: true` 给调用方虚假的安全保障
- 审计日志的防篡改属性在 API 层完全失效

### 解决方案

在 `AuditApiState` 中添加 `verifier_public_key: Vec<u8>` 字段，在 `verify_audit_log` 处理函数中使用项目已有的 `ring` crate（已在 `src/audit/recorder.rs` 中使用）和 `UnparsedPublicKey::verify` 进行真实的 Ed25519 签名验证，验证逻辑与 `AuditLogChain::verify_chain` 中保持一致。

### 范围

**在范围内：**
- 在 `AuditApiState` 结构体中添加 `verifier_public_key: Vec<u8>` 字段
- 修改 `verify_audit_log` 函数中的签名验证逻辑，调用 `ring::signature::UnparsedPublicKey::verify`
- 修改 `audit_routes` 函数或其调用处，在构造 `AuditApiState` 时传入公钥
- 更新已有的测试辅助代码（`AuditApiState` 的构造）

**不在范围内：**
- 修改 `AuditLogChain`、`AuditRecorder`、`SigningKeyPair` 等签名生成逻辑
- 修改审计存储后端（immudb、内存存储）
- 修改其他 API 端点（`list_audit_logs`、`get_audit_log_detail`、`export_audit_logs`）
- 实现密钥分发或密钥轮换机制（超出本次范围）

---

## 开发上下文

### 当前代码（关键片段）

**被验证的数据结构（`SignedAuditEntry`）：**
```rust
// src/audit/recorder.rs:38-74

pub struct SignedAuditEntry {
    pub entry: AuditEntry,
    pub content_hash: [u8; 32],
    pub prev_hash: [u8; 32],
    pub signature: Vec<u8>,         // Ed25519 签名（64 字节）
    pub signer_fingerprint: String, // 签名者公钥指纹
    pub log_index: u64,
    pub merkle_root: [u8; 32],
}
```

**签名时的数据构造（`AuditRecorder::record`，须与验证时保持一致）：**
```rust
// src/audit/recorder.rs:436-440（签名时构造的待签数据）

// 计算组合哈希（内容 + 前一个哈希）
let combined_data = [content_hash.as_slice(), prev_hash.as_slice()].concat();
let combined_hash = digest(&SHA256, &combined_data);
// 签名对象是 combined_hash（SHA256 摘要的 32 字节）
```

**现有的完整签名验证逻辑（`AuditLogChain::verify_chain`，参考实现）：**
```rust
// src/audit/recorder.rs:235-265

pub fn verify_chain(&self, public_key: &[u8]) -> Result<bool, RecorderError> {
    for entry in &self.entries {
        // 验证签名 (Ed25519)
        let public_key_unparsed = UnparsedPublicKey::new(&ED25519, public_key);
        // 签名数据结构: SHA256(content_hash + prev_hash)
        let combined_data =
            [entry.content_hash.as_slice(), entry.prev_hash.as_slice()].concat();
        let combined_hash = digest(&SHA256, &combined_data);
        match public_key_unparsed.verify(combined_hash.as_ref(), &entry.signature) {
            Ok(_) => {}
            Err(_) => return Ok(false),
        }
    }
    Ok(true)
}
```

**当前有问题的验证代码（目标修改位置）：**
```rust
// src/api/audit.rs:661-667

// 2. 验证签名（简化处理，实际应使用公钥验证）
let signature_valid = !entry.signature.is_empty();
details.push(VerificationDetail {
    step: "数字签名验证".to_string(),
    passed: signature_valid,
    message: Some(format!("签名者: {}", entry.signer_fingerprint)),
});
```

**`AuditApiState` 当前结构（需要添加公钥字段）：**
```rust
// src/api/audit.rs:33-37

#[derive(Clone)]
pub struct AuditApiState {
    /// 审计存储
    pub storage: Arc<dyn AuditStorage>,
}
```

### 代码库模式

`ring` crate 在项目中已广泛使用：

```rust
// src/audit/recorder.rs:7-8（已有导入）
use ring::digest::{SHA256, digest};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
```

`ring::signature::UnparsedPublicKey::verify` 的使用方式：
```rust
let public_key = UnparsedPublicKey::new(&ED25519, &public_key_bytes);
let result = public_key.verify(message, signature);
// result: Result<(), ring::error::Unspecified>
// Ok(()) 表示签名有效，Err(_) 表示无效
```

待签名/验证的数据格式（固定协议，由 `AuditRecorder::record` 确立）：
```
待验证消息 = SHA256(entry.content_hash || entry.prev_hash)
```

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/api/audit.rs:33-37` | `AuditApiState` 结构，需要添加 `verifier_public_key` 字段 |
| `src/api/audit.rs:650-693` | `verify_audit_log` 函数的验证逻辑，签名验证段需重写 |
| `src/api/audit.rs:704-715` | `audit_routes` 函数，了解 state 的构造方式 |
| `src/audit/recorder.rs:235-265` | `AuditLogChain::verify_chain`，签名验证的参考实现 |
| `src/audit/recorder.rs:7-8` | `ring` 相关导入 |
| `src/audit/recorder.rs:325-378` | `SigningKeyPair` 结构，了解公钥的格式（raw bytes，不是 PEM） |
| `src/main.rs` | 了解 `AuditApiState` 在应用启动时如何构造（以便添加公钥传入） |

### 技术决策

1. **公钥格式**：`ring` 的 `Ed25519KeyPair::public_key()` 返回 raw bytes（32 字节），不是 PEM。`AuditApiState.verifier_public_key` 存储相同格式的 raw bytes。

2. **公钥来源**：`verifier_public_key` 从应用配置或环境变量中注入（例如 `AUDIT_VERIFIER_PUBLIC_KEY` 环境变量，hex 或 base64 编码）。具体注入方式需要查看 `src/main.rs` 中 `AuditApiState` 的构造点。

3. **验证失败的处理**：签名验证失败（`Err`）不应返回 500 错误，应返回 `signature_valid = false`，整体 `verified = false`。这是数据完整性问题，不是服务器错误。

4. **`signer_fingerprint` 与公钥匹配**：当前 spec 不要求校验 `entry.signer_fingerprint` 与提供的 `verifier_public_key` 的指纹是否匹配（简化实现）。若需要，可参照 `SigningKeyPair::compute_fingerprint` 计算指纹后比较，作为可选增强。

5. **`ring` 已在 Cargo.toml 中**：通过 `src/audit/recorder.rs` 的使用可确认，无需新增依赖。

---

## 实现计划

### 任务

（按依赖顺序排列）

1. **在 `AuditApiState` 中添加 `verifier_public_key` 字段** — `src/api/audit.rs:33-37`

   ```rust
   #[derive(Clone)]
   pub struct AuditApiState {
       /// 审计存储
       pub storage: Arc<dyn AuditStorage>,
       /// 审计签名验证公钥（Ed25519 raw bytes，32 字节）
       pub verifier_public_key: Vec<u8>,
   }
   ```

2. **在 `src/api/audit.rs` 顶部添加 `ring` 导入** — `src/api/audit.rs`（现有 use 块）

   ```rust
   use ring::digest::{SHA256, digest};
   use ring::signature::{ED25519, UnparsedPublicKey};
   ```

3. **重写 `verify_audit_log` 中的签名验证段** — `src/api/audit.rs:661-667`

   将：
   ```rust
   // 2. 验证签名（简化处理，实际应使用公钥验证）
   let signature_valid = !entry.signature.is_empty();
   ```

   替换为：
   ```rust
   // 2. 验证签名（Ed25519，与 AuditRecorder::record 签名数据格式一致）
   let signature_valid = {
       let combined_data = [entry.content_hash.as_slice(), entry.prev_hash.as_slice()].concat();
       let combined_hash = digest(&SHA256, &combined_data);
       let pub_key = UnparsedPublicKey::new(&ED25519, &state.verifier_public_key);
       pub_key.verify(combined_hash.as_ref(), &entry.signature).is_ok()
   };
   ```

4. **修复 `AuditApiState` 构造点** — 查找所有 `AuditApiState {` 的构造调用

   在应用初始化代码（`src/main.rs` 或路由注册处）添加 `verifier_public_key` 字段的传入。如果找不到现有的公钥，可从环境变量读取：
   ```rust
   let verifier_public_key = std::env::var("AUDIT_VERIFIER_PUBLIC_KEY")
       .map(|hex_str| hex::decode(hex_str).expect("无效的公钥格式"))
       .unwrap_or_default(); // 降级：空公钥时验证永远失败（安全 fail-closed）

   let audit_state = AuditApiState {
       storage: Arc::new(storage_adapter),
       verifier_public_key,
   };
   ```

5. **更新文件中的测试辅助构造** — `src/api/audit.rs`（约第 780-800 行的测试代码）

   测试中构造 `AuditApiState` 时添加 `verifier_public_key` 字段，使用测试专用的 Ed25519 公钥（可通过 `SigningKeyPair::generate()` 生成并提取）。

### 验收标准

**场景 1：有效签名的条目验证通过**
- Given: 一个由 `AuditRecorder` 正常签名的 `SignedAuditEntry`，且 `AuditApiState.verifier_public_key` 是对应的公钥
- When: 调用 `POST /api/v1/audit/verify`
- Then: 响应中 `signature_valid = true`，`verified = true`（假设哈希和 Merkle 也通过）

**场景 2：被篡改的条目签名验证失败**
- Given: 一个 `SignedAuditEntry`，其 `content_hash` 被手动修改（模拟篡改）
- When: 调用 `POST /api/v1/audit/verify`
- Then: 响应中 `signature_valid = false`，`verified = false`

**场景 3：签名字段非空但无效（旧行为漏洞）**
- Given: 一个 `signature` 字段为任意非空字节（如 `vec![1, 2, 3]`）的伪造条目
- When: 调用 `POST /api/v1/audit/verify`
- Then: 响应中 `signature_valid = false`（区别于旧行为的 `true`）

**场景 4：无公钥（降级配置）**
- Given: `verifier_public_key` 为空 `Vec`
- When: 调用 `POST /api/v1/audit/verify`
- Then: `signature_valid = false`（`ring` 会因无效公钥返回错误），系统以安全方式失败

---

## 附加上下文

### 依赖

`ring` 已在项目中使用（见 `src/audit/recorder.rs:7-8`）。`hex` crate 也已在 `src/api/audit.rs:25` 中导入（`use hex`）。**无需添加新依赖**。

### 测试策略

在 `src/api/audit.rs` 的测试块中（约第 780 行起）：

1. 使用 `SigningKeyPair::generate()` 生成测试密钥对
2. 用私钥通过 `AuditRecorder` 生成真实签名的 `SignedAuditEntry`
3. 构造 `AuditApiState { verifier_public_key: signing_key.public_key().to_vec(), ... }`
4. 测试：正常签名 → `signature_valid = true`
5. 测试：修改 `content_hash` 后 → `signature_valid = false`
6. 测试：替换 `signature` 为随机字节 → `signature_valid = false`

### 注意事项

1. **待验证消息的格式必须与签名时完全一致**：签名时对 `SHA256(content_hash || prev_hash)` 签名（见 `recorder.rs:436-440`），验证时必须重现完全相同的数据，包括字节拼接顺序。

2. **`digest` 返回值的使用**：`ring::digest::digest` 返回 `ring::digest::Digest`，通过 `.as_ref()` 获取字节 slice：
   ```rust
   let combined_hash = digest(&SHA256, &combined_data);
   pub_key.verify(combined_hash.as_ref(), &entry.signature)
   ```

3. **`ring::error::Unspecified` 错误处理**：`verify` 返回 `Result<(), ring::error::Unspecified>`，`.is_ok()` 即可转换为 bool，不需要详细的错误信息（`ring` 不提供）。

4. **公钥注入策略**：若 `AuditRecorder` 由应用在启动时创建，可在创建后调用 `signing_key.public_key().to_vec()` 获取公钥，通过应用状态传递给 `AuditApiState`。避免将公钥硬编码在代码中。

5. **`content_hash` 字段类型**：`entry.content_hash` 是 `[u8; 32]`，`.as_slice()` 可直接获取 `&[u8]`。
