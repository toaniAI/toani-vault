---
title: 'MCP TEE 解密操作实现'
slug: 'ts-001-mcp-tee-decrypt'
created: '2026-03-19'
status: 'implemented'
priority: 'P1'
---

# Tech-Spec: MCP TEE 解密操作实现

## 概述

### 问题陈述

`mcp-server/src/tools.rs` 第 166 行的 `decrypt_credential` 方法当前使用占位符返回固定的 `[REDACTED]` 字符串，没有调用任何真实的解密逻辑。这意味着 MCP 客户端（AI Agent）无法通过 MCP 通道获取真实的凭证明文，整个 `decrypt_credential` 工具形同虚设。

```rust
// 当前问题代码：mcp-server/src/tools.rs:166-172
// TODO: 实际的 TEE 解密操作
// 这里返回模拟数据，实际实现需要调用 TEE enclave
let decrypted_data = serde_json::json!({
    "username": "[REDACTED]",
    "password": "[REDACTED]",
    "note": "This is a placeholder. Actual decryption happens in TEE enclave."
});
```

### 解决方案

参照主后端 `src/api/credentials.rs` 中已经可用的 `decrypt_credential_in_tee` 函数，在 MCP 工具层执行相同的解密流程：

1. 通过 `vault.get_credential()` 获取含加密载荷的完整凭证条目
2. 构建 `EncryptedBlob`
3. 利用 `McpServerState` 中的密钥层次（`key_hierarchy`）派生 L2/L3 密钥
4. 调用 `vault_service::crypto::cipher::decrypt_credential` 执行 AES-256-GCM 解密
5. 将明文字节解析为 JSON 或 UTF-8 字符串后返回

### 范围

- **变更文件**：`mcp-server/src/tools.rs`、`mcp-server/src/lib.rs`
- **不涉及**：SSE 传输层、Token 验证、审计日志格式（已有实现保持不变）

---

## 开发上下文

### 当前代码（关键片段）

**占位符所在位置** — `mcp-server/src/tools.rs:136-194`

```rust
pub async fn decrypt_credential(
    &self,
    tenant_id: &str,
    user_id: &str,
    credential_id: &str,
    scope: &str,
) -> Result<DecryptCredentialResponse, ToolError> {
    // ... 权限检查 ...

    // TODO: 实际的 TEE 解密操作
    let decrypted_data = serde_json::json!({
        "username": "[REDACTED]",
        "password": "[REDACTED]",
        "note": "This is a placeholder. Actual decryption happens in TEE enclave."
    });

    Ok(DecryptCredentialResponse {
        credential_id: credential_id.to_string(),
        decrypted_data: decrypted_data.to_string(),
        decrypted_at: chrono::Utc::now().to_rfc3339(),
        tee_verified: true,
    })
}
```

**参考实现** — `src/api/credentials.rs:542-580`（主后端已有完整 TEE 解密）

```rust
async fn decrypt_credential_in_tee(
    state: &AppState,
    entry: &VaultEntry,
) -> Result<Vec<u8>, String> {
    let blob = EncryptedBlob {
        version: entry.encrypted_payload.version,
        algorithm: entry.encrypted_payload.algorithm.clone(),
        kdf: entry.encrypted_payload.kdf.clone(),
        nonce: entry.encrypted_payload.nonce.clone(),
        auth_tag: entry.encrypted_payload.auth_tag.clone(),
        ciphertext: entry.encrypted_payload.ciphertext.clone(),
        aad_hash: None,
    };

    let l3_key = {
        let hierarchy = state.key_hierarchy.write().await;
        let l2_key = hierarchy
            .derive_user_vault_key(entry.tenant_id.as_str(), entry.user_id.hash())
            .map_err(|e| format!("L2 密钥派生失败: {}", e))?;
        hierarchy
            .derive_credential_key(&l2_key, entry.credential_id.as_str(), KeyPurpose::CredentialEncryption)
            .map_err(|e| format!("L3 密钥派生失败: {}", e))?
    };

    let aad = format!("{}:{}", entry.tenant_id.as_str(), entry.user_id.hash());
    let plaintext = decrypt_credential(&l3_key, &blob, Some(aad.as_bytes()))
        .map_err(|e| format!("解密失败: {:?}", e))?;

    Ok(plaintext)
}
```

**Enclave 的 `decrypt_credential` 签名** — `src/tee/enclave.rs:459-465`

```rust
pub fn decrypt_credential(
    &mut self,
    tenant_id: &str,
    user_id: &str,
    credential_id: &str,
    blob: &EncryptedBlob,
) -> Result<Vec<u8>, EnclaveError>
```

**`McpServerState` 当前结构** — `mcp-server/src/lib.rs:72-79`

```rust
pub struct McpServerState {
    pub vault: CredentialVault,
    pub audit: MemoryAuditStorage,
    pub token_key: Option<vault_service::token::PasetoKey>,
    // 缺少：key_hierarchy 字段
}
```

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `mcp-server/src/tools.rs` | 待修改：替换占位符实现 |
| `mcp-server/src/lib.rs` | 待修改：为 `McpServerState` 添加 `key_hierarchy` 字段 |
| `src/api/credentials.rs:481-580` | 参考：已工作的主后端 TEE 解密流程 |
| `src/tee/enclave.rs:459-530` | 参考：Enclave 的 AES-256-GCM 解密实现 |
| `src/crypto/cipher.rs:188` | 参考：`decrypt_credential` 函数签名与 AAD 构建方式 |
| `vault_service/src/vault/mod.rs` | 参考：`get_credential` 返回 `VaultEntry`（含加密载荷） |

### 技术决策

1. **密钥层次复用**：mcp-server 使用与主后端相同的 `vault_service` 库，密钥派生逻辑完全共享，无需重新实现。

2. **`McpServerState` 需要 `key_hierarchy` 字段**：当前状态结构缺少密钥层次对象。生产环境需从安全配置加载主密钥；开发/测试环境可使用 `KeyHierarchy::new_with_random_key()` 初始化。

3. **`CredentialVault` 需要 `get_credential` 方法**（返回含加密载荷的完整条目）：目前工具层只调用 `get_credential_metadata`（不含加密内容）。需确认 `CredentialVault` 是否暴露了 `get_credential` 接口，如未暴露则需扩展。

4. **AAD 构造规则**：必须与加密时一致，格式为 `"tenant_id:user_id_hash"`（注意 user_id 使用 hash，而非原始值）。

5. **错误映射**：`EnclaveError`/`CipherError` 应映射为 `ToolError::VaultError(msg.to_string())`，保持错误类型一致性。

---

## 实现计划

### 任务

按依赖顺序：

**任务 1：确认 `CredentialVault::get_credential` 可用性**
- 检查 `vault_service/src/vault/mod.rs` 中 `get_credential` 方法签名
- 若不存在，需在 `vault_service` 中添加返回 `VaultEntry`（含 `encrypted_payload`）的方法
- 预计工时：0.5h（确认）或 2h（新增方法）

**任务 2：为 `McpServerState` 添加密钥层次字段**
- 在 `mcp-server/src/lib.rs` 的 `McpServerState` 中添加 `key_hierarchy: Arc<RwLock<KeyHierarchy>>` 字段
- 更新 `new_in_memory()` 构造函数，使用随机主密钥初始化（开发模式）
- 添加生产模式构造函数，从环境变量/配置加载主密钥材料

**任务 3：实现 `decrypt_credential` 真实逻辑**
- 在 `mcp-server/src/tools.rs` 中替换 TODO 占位符
- 流程：`vault.get_credential()` → 构建 `EncryptedBlob` → 派生 L3 密钥 → `decrypt_credential()` → 解析明文 → 返回
- 将 `tee_verified` 字段改为真实 TEE 状态检查（或保持为 `true` 并添加注释说明当前为软件模拟模式）

**任务 4：更新审计日志中的 mrenclave 字段**
- 将 `"mrenclave_placeholder"` 替换为从 TEE 状态获取的真实 mrenclave 值（或软件模拟标识 `"software_mode"`）

### 验收标准

- [ ] `decrypt_credential` 工具返回真实明文而非 `[REDACTED]`
- [ ] 使用已知凭证数据的集成测试通过：加密后再通过 MCP 解密，明文一致
- [ ] 权限不足时仍返回 `PermissionDenied` 错误（不泄露任何密文信息）
- [ ] 凭证不存在时返回 `NotFound` 错误（不泄露存在性以外的信息）
- [ ] 解密失败（密钥错误、数据损坏）时返回 `VaultError`，不 panic
- [ ] 审计日志记录解密尝试（含结果 `Success`/`Denied`/`Failure`）
- [ ] `cargo test` 全部通过，无新增警告

---

## 附加上下文

### 依赖

- `vault_service` crate（`get_credential` 方法、`KeyHierarchy`、`decrypt_credential`）
- `aes_gcm` crate（已在 `vault_service` 中依赖）
- `tokio::sync::RwLock`（密钥层次的并发访问）

### 测试策略

1. **单元测试**（`mcp-server/src/tools.rs` 的 `#[cfg(test)]` 模块）：
   - 创建测试凭证（使用 `create_encrypted_payload` 的真实 AES-256-GCM 版本）
   - 调用 `decrypt_credential` 验证返回正确明文

2. **集成测试**（`mcp-server/tests/` 目录）：
   - 创建凭证 → 通过 MCP 工具解密 → 验证端到端正确性

3. **负面测试**：
   - 错误凭证 ID → `NotFound`
   - 缺少 `credential:decrypt` scope → `PermissionDenied`
   - 数据损坏场景 → `VaultError`（不 panic）

### 注意事项

- **安全注意**：`decrypted_data` 字段在日志中绝对不能出现，当前审计日志已用 `PiiRedactor` 脱敏，实现时确保不引入新的日志输出点
- **内存安全**：明文 `Vec<u8>` 使用完毕后应考虑 `zeroize`，与 `Rust Coding Standards` 中的安全规范一致
- **软件模式与 TEE 模式**：当前 mcp-server 在软件模式运行，`tee_verified` 应如实反映状态，避免给 AI 客户端错误的安全感知
