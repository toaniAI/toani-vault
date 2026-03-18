# Phase 1: 凭证解密失败问题分析报告

**分析人员**: claude_qwen (CTO/架构师)
**分析日期**: 2026-03-12
**问题状态**: 根本原因已确认

---

## 1. 问题现象详细描述

### 1.1 错误表现

```json
{
  "error": "internal_error",
  "message": "解密失败：AuthenticationFailed"
}
```

### 1.2 已知行为

- **加密流程**: 正常工作，凭证创建成功并返回 `credential_id`
- **解密流程**: 失败，返回 `AuthenticationFailed` 错误
- **错误类型**: AES-GCM 认证标签验证失败

### 1.3 AES-GCM 认证失败的可能原因

AES-GCM 解密时认证标签验证失败，说明以下三者之一不匹配：
1. **密钥错误** - 解密使用的密钥与加密时不同
2. **AAD 错误** - 解密使用的 AAD 与加密时不同
3. **密文被篡改** - 密文、nonce 或 auth_tag 在存储过程中被修改

---

## 2. 代码分析

### 2.1 加密流程代码分析

**文件**: `src/api/credentials.rs:190-221`

```rust
async fn encrypt_credential_in_tee(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    // 1. 序列化明文
    let plaintext_bytes = serde_json::to_vec(plaintext)...;

    // 2. 派生 L3 密钥
    let l3_key = {
        let hierarchy = state.key_hierarchy.write().await;
        let l2_key = hierarchy
            .derive_user_vault_key(tenant_id, user_id)...;

        // ⚠️ 关键：使用临时生成的 credential_id
        let temp_cred_id = uuid::Uuid::now_v7().to_string();  // ← 问题点
        hierarchy
            .derive_credential_key(&l2_key, &temp_cred_id, KeyPurpose::CredentialEncryption)
            ...
    };

    // 3. 执行加密
    let aad = format!("{}:{}", tenant_id, user_id);
    let blob = encrypt_credential(&l3_key, &plaintext_bytes, Some(aad.as_bytes()))...;

    Ok(EncryptedPayload::from_blob(&blob))
}
```

### 2.2 解密流程代码分析

**文件**: `src/api/credentials.rs:387-431`

```rust
async fn decrypt_credential_in_tee(
    state: &AppState,
    entry: &VaultEntry,
) -> Result<Vec<u8>, String> {
    // 1. 构建 EncryptedBlob
    let blob = EncryptedBlob { ... };

    // 2. 派生 L3 密钥
    let l3_key = {
        let hierarchy = state.key_hierarchy.write().await;
        let l2_key = hierarchy
            .derive_user_vault_key(
                entry.tenant_id.as_str(),
                entry.user_id.hash(),  // 注意：这里传入的是 user_id.hash()
            )...;

        // ⚠️ 关键：使用存储的 credential_id
        hierarchy
            .derive_credential_key(
                &l2_key,
                entry.credential_id.as_str(),  // ← 使用存储的真实 ID
                KeyPurpose::CredentialEncryption,
            )...
    };

    // 3. 执行解密
    let aad = format!("{}:{}", entry.tenant_id.as_str(), entry.user_id.hash());
    let plaintext = decrypt_credential(&l3_key, &blob, Some(aad.as_bytes()))...;

    Ok(plaintext)
}
```

### 2.3 密钥派生代码分析

**文件**: `src/crypto/hkdf.rs:229-261`

```rust
pub fn derive_credential_key(
    &self,
    l2_key: &UserVaultKey,
    credential_id: &str,  // ← credential_id 作为 HKDF 的 info 参数
    purpose: KeyPurpose,
) -> Result<CredentialKey, CryptoError> {
    // 构建 info 字符串（包含版本信息）
    let info = format!(
        "credential-key:v{}:{}:{}",
        self.key_version.major,
        credential_id,        // ← credential_id 直接参与密钥派生
        purpose.as_str()
    );

    // HKDF-Expand: L2 -> L3
    let salt = Salt::new(HKDF_SHA256, l2_key.as_bytes());
    let prk = salt.extract(b"");

    let mut l3_key_material = [0u8; KEY_LENGTH];
    let info_bytes = info.into_bytes();
    let binding = [&info_bytes[..]];
    let okm = prk.expand(&binding, HKDF_SHA256)?;
    okm.fill(&mut l3_key_material)?;  // ← 生成的密钥完全依赖于 info

    Ok(CredentialKey::new(l3_key_material, ...))
}
```

**关键发现**: L3 密钥是通过 HKDF-Expand 确定性派生的，`credential_id` 作为 `info` 参数的一部分，**不同的 credential_id 会产生完全不同的 L3 密钥**。

---

## 3. 数据流分析

### 3.1 加密流程数据流图

```
┌─────────────────────────────────────────────────────────────────┐
│                    加密流程 (create_credential)                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. 用户提交明文凭证数据                                         │
│                    ↓                                            │
│  2. encrypt_credential_in_tee()                                 │
│         ↓                                                       │
│    a) 序列化明文为 bytes                                         │
│         ↓                                                       │
│    b) 生成临时 credential_id (temp_cred_id)                      │
│       temp_cred_id = uuid::Uuid::now_v7()  ← 临时 ID，不保存     │
│         ↓                                                       │
│    c) 派生 L2 密钥 (tenant_id, user_id)                          │
│         ↓                                                       │
│    d) 派生 L3 密钥 (L2, temp_cred_id, CredentialEncryption)      │
│       ⚠️ L3 密钥依赖于 temp_cred_id                             │
│         ↓                                                       │
│    e) AES-GCM 加密 (L3, plaintext, aad)                          │
│       aad = "tenant_id:user_id"                                 │
│         ↓                                                       │
│    f) 返回 EncryptedPayload (不含 credential_id)                 │
│                    ↓                                            │
│  3. vault.create_credential()                                    │
│         ↓                                                       │
│    a) VaultEntry::new() 创建条目                                │
│       entry.credential_id = CredentialId::new()  ← 新 UUID v7    │
│         ↓                                                       │
│    b) 存储到数据库                                               │
│       stored_credential_id ≠ temp_cred_id (几乎肯定不同)         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 解密流程数据流图

```
┌─────────────────────────────────────────────────────────────────┐
│                    解密流程 (decrypt_credential_endpoint)        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. 用户请求解密 (携带 credential_id)                            │
│                    ↓                                            │
│  2. vault.get_credential(credential_id)                         │
│         ↓                                                       │
│    返回 VaultEntry {                                            │
│      credential_id: 存储时的 UUID v7,                           │
│      encrypted_payload: ...,                                    │
│      tenant_id: ...,                                            │
│      user_id: ...                                               │
│    }                                                            │
│                    ↓                                            │
│  3. decrypt_credential_in_tee(entry)                            │
│         ↓                                                       │
│    a) 派生 L2 密钥 (entry.tenant_id, entry.user_id.hash())      │
│         ↓                                                       │
│    b) 派生 L3 密钥 (L2, entry.credential_id, ...)               │
│       ⚠️ 使用存储的 credential_id                               │
│         ↓                                                       │
│    c) AES-GCM 解密 (L3, blob, aad)                              │
│       aad = "tenant_id:user_id.hash()"                          │
│         ↓                                                       │
│    d) 验证失败 → AuthenticationFailed ❌                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.3 分歧点对比

| 步骤 | 加密流程 | 解密流程 | 是否一致 |
|------|----------|----------|----------|
| L2 密钥派生 | `derive_user_vault_key(tenant_id, user_id)` | `derive_user_vault_key(tenant_id, user_id.hash())` | ⚠️ **不一致** |
| credential_id 来源 | `uuid::Uuid::now_v7()` (临时生成) | `entry.credential_id` (存储的 UUID) | ❌ **不一致** |
| L3 密钥派生 | `derive_credential_key(L2, temp_cred_id, ...)` | `derive_credential_key(L2, stored_cred_id, ...)` | ❌ **不同密钥** |
| AAD | `"tenant_id:user_id"` | `"tenant_id:user_id.hash()"` | ❌ **不一致** |

---

## 4. 根本原因分析

### 4.1 主要根本原因 (确认)

**Credential ID 不一致导致 L3 密钥不匹配**

1. **加密时**:
   - 在 `encrypt_credential_in_tee()` 函数第 209 行生成一个**临时的** `temp_cred_id`
   - 使用 `temp_cred_id` 派生 L3 密钥进行加密
   - 之后在 `vault.create_credential()` 中生成**另一个** `credential_id` 并存储

2. **解密时**:
   - 使用存储的 `entry.credential_id` 派生 L3 密钥进行解密

3. **结果**:
   - 由于 HKDF 是确定性 KDF，不同的 `credential_id` 输入产生完全不同的输出密钥
   - 加密 L3 密钥 ≠ 解密 L3 密钥
   - AES-GCM 使用错误的密钥解密，认证标签验证失败 → `AuthenticationFailed`

### 4.2 次要根本原因 (确认)

**AAD 不一致**

1. **加密时**: `aad = format!("{}:{}", tenant_id, user_id)` - 使用原始 user_id
2. **解密时**: `aad = format!("{}:{}", tenant_id, user_id.hash())` - 使用哈希后的 user_id
3. **结果**: 即使 L3 密钥正确，AAD 不匹配也会导致 AES-GCM 认证失败

### 4.3 L2 密钥派生参数不一致

1. **加密时**: `derive_user_vault_key(tenant_id, user_id)` - 传入原始 user_id
2. **解密时**: `derive_user_vault_key(..., entry.user_id.hash())` - 传入哈希后的 user_id
3. **查看 `derive_user_vault_key` 实现**:
   ```rust
   pub fn derive_user_vault_key(&self, tenant_id: &str, user_id: &str) -> ... {
       let info = format!("user-vault-key:v{}:{}:{}", self.key_version.major, tenant_id, user_id);
       // ...
       let user_id_hash = format!("hash:{}:v{}", user_id, self.key_version.major);
       // ...
   }
   ```
4. **结果**: L2 密钥的 `info` 包含传入的 `user_id` 参数，传入不同值会派生出不同的 L2 密钥，进而导致 L3 密钥也不同

---

## 5. 假设与验证方法

### 假设 1: Credential ID 不一致 (已确认)

**假设**: 加密时使用的 `temp_cred_id` 与解密时使用的 `stored_cred_id` 不同

**验证方法**:
```rust
// 在 encrypt_credential_in_tee 中添加日志
let temp_cred_id = uuid::Uuid::now_v7().to_string();
log::info!("[ENCRYPT] temp_cred_id: {}", temp_cred_id);

// 在 decrypt_credential_in_tee 中添加日志
log::info!("[DECRYPT] stored_cred_id: {}", entry.credential_id.as_str());

// 运行加解密测试，比较两个 ID 是否相同
```

**预期结果**: 两个 ID 几乎 100% 不同（UUID v7 碰撞概率极低）

### 假设 2: AAD 不一致 (已确认)

**假设**: 加密和解密使用的 AAD 格式不同

**验证方法**:
```rust
// 在加密和解密函数中添加日志
log::info!("[ENCRYPT] AAD: {}", aad);  // 加密时
log::info!("[DECRYPT] AAD: {}", aad);  // 解密时
```

**预期结果**: 加密 AAD = "tenant:user_raw", 解密 AAD = "tenant:user_hash"

### 假设 3: L2 密钥派生参数不一致 (已确认)

**假设**: 传入 `derive_user_vault_key` 的 user_id 参数格式不同

**验证方法**:
```rust
// 在 derive_user_vault_key 入口添加日志
log::info!("[L2] Deriving with tenant={}, user={}", tenant_id, user_id);
```

**预期结果**: 加密时 user=user_raw, 解密时 user=user_hash

---

## 6. 推荐的修复方案

### 方案 A: 统一使用存储的 credential_id (推荐)

**修改点**: `encrypt_credential_in_tee()`

```rust
// 修改前（第 208-212 行）
let temp_cred_id = uuid::Uuid::now_v7().to_string();
hierarchy.derive_credential_key(&l2_key, &temp_cred_id, KeyPurpose::CredentialEncryption)

// 修改后：先创建 VaultEntry 获取 credential_id，再加密
let mut entry = VaultEntry::new(...);  // 先生成 entry，包含 credential_id
let l3_key = hierarchy.derive_credential_key(
    &l2_key,
    entry.credential_id.as_str(),  // 使用将要存储的 ID
    KeyPurpose::CredentialEncryption
)?;
let encrypted_payload = encrypt_credential(&l3_key, &plaintext_bytes, Some(aad.as_bytes()))?;
// 然后存储 entry
```

**优点**:
- 逻辑清晰，credential_id 在加密前就确定
- 解密时自然使用相同的 ID

**缺点**:
- 需要调整代码结构，先创建 entry 再加密

### 方案 B: 调整 AAD 格式

**修改点**: 统一 AAD 格式

```rust
// 加密时（第 216 行）
let aad = format!("{}:{}", tenant_id, user_id);  // 改为使用 hash
// 或者
let aad = format!("{}:{}", tenant_id, UserId::new(user_id).hash());

// 解密时保持不变（已经是 hash）
let aad = format!("{}:{}", entry.tenant_id.as_str(), entry.user_id.hash());
```

### 方案 C: 调整 L2 派生参数

**修改点**: 统一传入 `derive_user_vault_key` 的参数格式

```rust
// 加密时（第 204-206 行）
let l2_key = hierarchy
    .derive_user_vault_key(tenant_id, UserId::new(user_id).hash())  // 改为 hash
    ...;

// 解密时保持不变
let l2_key = hierarchy
    .derive_user_vault_key(entry.tenant_id.as_str(), entry.user_id.hash())
    ...;
```

### 综合修复建议

三个问题需要**同时修复**：

```rust
// encrypt_credential_in_tee 完整修复
async fn encrypt_credential_in_tee(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    let plaintext_bytes = serde_json::to_vec(plaintext)...;

    // 1. 先创建 VaultEntry 获取 credential_id
    let user_id_obj = UserId::new(user_id);
    let mut entry = VaultEntry::new(
        TenantId::new(tenant_id),
        user_id_obj.clone(),
        // ... 其他参数
    );

    // 2. 使用 entry.credential_id 派生 L3 密钥
    let l3_key = {
        let hierarchy = state.key_hierarchy.write().await;
        // 使用 hash 保持一致
        let l2_key = hierarchy
            .derive_user_vault_key(tenant_id, user_id_obj.hash())
            ...;
        hierarchy
            .derive_credential_key(&l2_key, entry.credential_id.as_str(), ...)
            ...
    };

    // 3. 使用 hash 构建 AAD
    let aad = format!("{}:{}", tenant_id, user_id_obj.hash());
    let blob = encrypt_credential(&l3_key, &plaintext_bytes, Some(aad.as_bytes()))...;

    Ok(EncryptedPayload::from_blob(&blob))
}
```

---

## 7. 风险评估

### 7.1 修复风险

| 风险项 | 可能性 | 影响 | 缓解措施 |
|--------|--------|------|----------|
| 破坏现有数据 | 低 | 高 | 旧凭证无法解密，需要数据迁移脚本 |
| 引入新 bug | 中 | 中 | 编写完整的单元测试和集成测试 |
| 性能影响 | 低 | 低 | 代码结构调整不影响性能 |

### 7.2 不修复的风险

- **功能完全不可用**: 解密流程 100% 失败，凭证创建后无法使用
- **安全审计失败**: 无法验证加密系统的正确性
- **用户信任损失**: 敏感凭证无法检索

### 7.3 数据迁移考虑

修复后，**旧凭证将无法解密**（因为加解密参数不一致）。需要考虑：

1. **方案 A**: 在解密时尝试旧参数（temp_cred_id 模式）
   - 问题：temp_cred_id 未存储，无法恢复

2. **方案 B**: 提供数据迁移工具，重新加密所有旧凭证
   - 需要用户重新认证
   - 迁移期间服务不可用

3. **方案 C**: 宣布旧凭证作废，用户重新创建
   - 用户体验最差
   - 实现成本最低

**推荐**: 由于这是一个新功能（EP2-Story2.2），很可能还没有生产数据，建议选择**方案 C**，并在发布说明中明确说明。

---

## 8. 验证计划

修复后需要验证：

1. **单元测试**:
   - 加解密往返测试
   - AAD 一致性测试
   - L2 密钥派生参数一致性测试

2. **集成测试**:
   - 完整 API 流程测试
   - 多租户隔离测试

3. **边界测试**:
   - 不同 tenant/user 组合
   - 不同 credential_type
   - 大 payload 测试

---

## 9. 总结

**根本原因**: 加密和解密流程使用了不一致的参数：
1. ❌ `credential_id`: 加密用临时 ID，解密用存储 ID
2. ❌ `AAD`: 加密用原始 user_id，解密用 hash
3. ❌ `L2 派生参数`: 加密用原始 user_id，解密用 hash

**修复方向**: 统一加密和解密流程的所有参数，确保确定性 KDF 产生相同的密钥。

**下一步**: 进入 Phase 2 修复实施阶段。
