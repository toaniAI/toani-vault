---
title: '审计安全上下文完善：TEE/JTI 真实值 + 内容哈希验证'
slug: 'audit-security-context-tee-jti-content-hash'
created: '2026-03-19'
status: 'implemented'
priority: 'P2'
---

# Tech-Spec: 审计安全上下文完善

## 概述

本规格涵盖两个紧密相关的审计安全问题：凭证审计日志中的 TEE/JTI 字段使用固定字符串，以及审计验证 API 中内容哈希固定返回 `true`。两者都影响审计日志的安全真实性，合并处理。

---

## 问题 1：`mrenclave` 和 `jti` 使用固定值（中优先级）

### 问题陈述

`src/api/credentials.rs` 中的 `record_to_storage()` 在创建 `AuditEntry` 时，`tee_mrenclave` 和 `action_token_jti` 使用了硬编码的字符串字面量：

```rust
// src/api/credentials.rs:120-128
let entry = AuditEntry::new(
    user_id_hash,
    "session",     // session_id
    "credentials", // service
    action,
    outcome,
    "mrenclave", // tee_mrenclave - 简化处理   ← 固定值
    "jti",       // action_token_jti - 简化处理  ← 固定值
)
```

**安全影响**：
- `tee_mrenclave` 应包含当前 TEE 的 MRENCLAVE 度量值，用于证明操作发生在可信环境中。固定为 `"mrenclave"` 字符串会让审计日志失去 TEE 证明价值。
- `action_token_jti` 是当前请求的 PASETO token JTI（JWT ID），用于将审计事件与具体操作令牌关联。固定为 `"jti"` 字符串无法追踪具体是哪个 token 执行了操作。

### 解决方案

从请求上下文（`Extension<Claims>` 或 `AuthContext`）中提取真实的 `jti`，从 TEE 状态（全局或线程本地存储的 MRENCLAVE 度量值）获取真实的 `mrenclave`。

### 当前代码上下文

```rust
// src/api/credentials.rs:101-139（StorageAuditLogger）
impl StorageAuditLogger {
    pub fn new(storage: Arc<tokio::sync::Mutex<MemoryAuditStorage>>) -> Self {
        Self { storage }
    }

    fn record_to_storage(
        &self,
        action: AuditAction,
        user_id: &str,
        credential_id: &str,
        outcome: Outcome,
    ) {
        if let Ok(storage) = self.storage.try_lock() {
            let user_id_hash = crate::audit::events::hash_user_id(user_id);
            let entry = AuditEntry::new(
                user_id_hash,
                "session",
                "credentials",
                action,
                outcome,
                "mrenclave", // ← 需修复
                "jti",       // ← 需修复
            )
            // ...
        }
    }
}
```

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/api/credentials.rs` | 主要修改目标（120-128 行） |
| `src/audit/events.rs` | `AuditEntry::new()` 签名，查看参数含义 |
| `src/tee/` | TEE 模块，查找 MRENCLAVE 获取方式 |
| `src/api/` 中的中间件/认证文件 | 查找 token JTI 的请求上下文传递方式 |
| `src/models/` 或 `src/token/` | PASETO claims 结构体，确认 `jti` 字段名 |

### 实现计划

**任务 1**：修改 `record_to_storage()` 签名，增加 `mrenclave: &str` 和 `jti: &str` 参数：

```rust
fn record_to_storage(
    &self,
    action: AuditAction,
    user_id: &str,
    credential_id: &str,
    outcome: Outcome,
    mrenclave: &str,  // 新增
    jti: &str,        // 新增
) {
```

**任务 2**：在调用 `record_to_storage()` 的上层函数中传入真实值：
- `mrenclave`：从 `crate::tee::get_mrenclave()` 或等效函数获取（若 TEE 不可用，使用 `"simulation"` 或空字符串，并记录 warn）
- `jti`：从请求的认证上下文（`AuthContext` / `TokenClaims` Extension）中提取

**任务 3**：确认 TEE 模块提供 MRENCLAVE 读取接口；若无，新增一个 `pub fn current_mrenclave() -> String` 函数（模拟模式返回常量，真实 TEE 模式读取寄存器）。

**任务 4**：更新所有 `record_to_storage()` 调用点（`AuditLogger` trait 的实现方法中）。

### 验收标准

- [ ] 审计日志中 `tee_mrenclave` 字段包含真实的 TEE 度量值（或明确的模拟标识符）
- [ ] 审计日志中 `action_token_jti` 字段包含发起操作的 PASETO token 的 JTI
- [ ] TEE 不可用时优雅降级（记录 warn，使用明确的占位符而非误导性的 `"mrenclave"` 字符串）
- [ ] 现有审计日志写入流程不受影响（向后兼容）

---

## 问题 2：`content_hash_match` 固定为 `true`（中优先级）

### 问题陈述

审计验证 API（`GET /audit/verify`）在验证审计条目时，内容哈希验证步骤被简化为固定返回 `true`：

```rust
// src/api/audit.rs:653-659
// 1. 验证内容哈希
let content_hash_match = true; // 简化处理
details.push(VerificationDetail {
    step: "内容哈希验证".to_string(),
    passed: content_hash_match,
    message: Some(format!("哈希: {}", hex::encode(entry.content_hash))),
});
```

这使得内容哈希验证步骤毫无意义 —— 即使哈希被篡改也会返回"验证通过"，破坏了审计系统的可信性。

**验证逻辑上下文**：总体验证结果由三个步骤 AND 组合：

```rust
// src/api/audit.rs:682
let verified = content_hash_match && signature_valid && merkle_proof_valid;
```

### 已有的计算逻辑

项目中已有正确的内容哈希计算逻辑：

```rust
// src/audit/events.rs:359-367
pub fn content_hash(&self) -> [u8; 32] {
    use ring::digest::{SHA256, digest};
    let json = self.to_json().unwrap_or_default();
    let digest = digest(&SHA256, json.as_bytes());
    let mut hash = [0u8; 32];
    hash.copy_from_slice(digest.as_ref());
    hash
}
```

`verify_chain()` 中也已正确使用此方法：

```rust
// src/audit/recorder.rs:244-248
let computed_hash = entry.entry.content_hash();
if computed_hash != entry.content_hash {
    return Ok(false);
}
```

### 解决方案

在 `audit.rs` 的验证 handler 中，复用 `AuditEntry::content_hash()` 方法计算哈希，与存储的 `entry.content_hash` 字段对比。

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/api/audit.rs` | 主要修改目标（654 行） |
| `src/audit/events.rs` | `AuditEntry::content_hash()` 方法（359-367 行） |
| `src/audit/recorder.rs` | `verify_chain()` 中的正确验证模式参考（244-248 行） |
| `src/api/audit_models.rs` | `SignedAuditEntry` 结构体，确认 `content_hash` 字段类型 |

### `SignedAuditEntry` 结构体（关键）

```rust
// src/audit/recorder.rs:40-49
pub struct SignedAuditEntry {
    pub entry: AuditEntry,          // 原始审计条目
    pub content_hash: [u8; 32],     // 存储时计算的内容哈希
    pub prev_hash: [u8; 32],        // 链式结构前一条目哈希
    pub signature: Vec<u8>,         // Ed25519 数字签名
    pub signer_fingerprint: String, // 签名者公钥指纹
    pub log_index: u64,
    // ...
}
```

### 实现计划

**任务 1**：将 `audit.rs` 第 654 行替换为真实计算：

```rust
// 修改前
let content_hash_match = true; // 简化处理

// 修改后
let computed_hash = entry.entry.content_hash();
let content_hash_match = computed_hash == entry.content_hash;
```

这是一次精确的单行改动，已有所有必要的数据（`entry.entry` 是 `AuditEntry`，`entry.content_hash` 是存储的哈希）。

**任务 2**：更新 `VerificationDetail` 的 `message` 字段，提供更丰富的诊断信息：

```rust
message: Some(format!(
    "存储哈希: {}, 计算哈希: {}",
    hex::encode(entry.content_hash),
    hex::encode(computed_hash)
)),
```

**任务 3**：补充测试用例：人为篡改 `AuditEntry` 后验证哈希不匹配。

### 验收标准

- [ ] 正常条目：`content_hash_match = true`，验证通过
- [ ] 篡改 `entry.entry` 内容后：`content_hash_match = false`，整体 `verified = false`
- [ ] `VerificationDetail` 的 `message` 同时显示存储哈希和计算哈希（便于调试）
- [ ] 编译通过，类型检查无误（`[u8; 32]` 比较使用 `==` 即可）

---

## 附加上下文

### 依赖

- 问题 1 和问题 2 相互独立，可以并行修复
- 问题 1 需要 TEE 模块提供 MRENCLAVE 读取接口（可能需要与 TEE 专家协作）
- 问题 2 完全独立，所有依赖代码已存在

### 测试策略

**问题 1（mrenclave/jti）**：
- 单元测试：验证审计日志条目中的 `tee_mrenclave` 不等于字符串 `"mrenclave"`
- 集成测试：执行凭证操作后，查询审计日志，验证 JTI 与操作使用的 token 一致

**问题 2（content_hash）**：
- 单元测试（关键负面测试）：
  1. 创建一个 `SignedAuditEntry`，修改 `entry.entry` 中的任意字段
  2. 调用验证逻辑，断言 `content_hash_match = false`
- 正常路径测试：未篡改的条目验证通过

### 安全注意事项

- 内容哈希验证是审计日志防篡改的第一道防线；此修复是必须的安全加固，不应延期
- TEE MRENCLAVE 获取在模拟模式（非 SGX 硬件）下，应返回明确的 `"simulation"` 标识而非随机值，确保日志可审查
