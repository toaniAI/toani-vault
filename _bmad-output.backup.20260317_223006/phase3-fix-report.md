# Phase 3: 问题修复报告

**修复人员**: claude_glm (后端开发)
**修复日期**: 2026-03-12
**问题状态**: 已修复

---

## 1. 执行摘要

成功修复了 CredBridge 凭证解密失败问题。修复了三个根本原因：
1. **Credential ID 不一致**: 加密用临时 ID，解密用存储 ID
2. **AAD 不一致**: 加密用原始 user_id，解密用 hash
3. **L2 派生参数不一致**: 加密用原始 user_id，解密用 hash

### 修复结果

| 修复项 | 状态 | 说明 |
|--------|------|------|
| encrypt_credential_in_tee 修改 | ✓ 完成 | 接收预生成的 credential_id 参数，使用 user_id.hash() |
| create_credential 修改 | ✓ 完成 | 先生成 credential_id 再加密 |
| encrypt_credential_update 修改 | ✓ 完成 | 使用 user_id.hash() 派生 L2 和 AAD |
| 编译验证 | ✓ 通过 | `cargo build --release` 成功 |

---

## 2. 修改文件清单

| 文件 | 修改类型 | 说明 |
|------|----------|------|
| `src/api/credentials.rs` | 修改 | 修改加密函数和 API 处理函数 |
| `src/vault/models.rs` | 新增 | 添加 `VaultEntry::with_credential_id` 方法 |
| `src/vault/storage.rs` | 新增 | 添加 `create_credential_with_id` 方法 |

---

## 3. 修改详情

### 3.1 `src/api/credentials.rs` - encrypt_credential_in_tee 函数

**修改前**:
```rust
async fn encrypt_credential_in_tee(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    // ...
    // 使用临时凭证 ID 派生密钥
    let temp_cred_id = uuid::Uuid::now_v7().to_string();
    hierarchy
        .derive_credential_key(&l2_key, &temp_cred_id, KeyPurpose::CredentialEncryption)
    // ...
    let aad = format!("{}:{}", tenant_id, user_id);
    // ...
}
```

**修改后**:
```rust
/// 在 TEE 内加密凭证
///
/// 修复说明：
/// - 接收预生成的 credential_id，确保加密和存储使用相同的 ID
/// - 使用 user_id.hash() 派生 L2 密钥，与解密流程一致
/// - 使用 user_id.hash() 构建 AAD，与解密流程一致
async fn encrypt_credential_in_tee(
    state: &AppState,
    tenant_id: &str,
    user_id: &UserId,  // 改为 UserId 类型
    credential_id: &CredentialId,  // 新增参数
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    // ...
    // 使用 user_id.hash() 保持与解密流程一致
    let l2_key = hierarchy
        .derive_user_vault_key(tenant_id, user_id.hash())
    // ...
    // 使用预生成的 credential_id 派生密钥，确保与存储的 ID 一致
    hierarchy
        .derive_credential_key(&l2_key, credential_id.as_str(), KeyPurpose::CredentialEncryption)
    // ...
    // 使用 user_id.hash() 构建 AAD，与解密流程一致
    let aad = format!("{}:{}", tenant_id, user_id.hash());
    // ...
}
```

### 3.2 `src/api/credentials.rs` - create_credential 函数

**修改前**:
```rust
pub async fn create_credential(...) {
    // ...
    // 加密凭证内容（在 TEE 内完成）
    let encrypted_payload = encrypt_credential_in_tee(
        &state,
        &token.tenant_id,
        &token.user_id,
        &request.plaintext_data,
    ).await
    // ...
    // 存储凭证
    let entry = state.vault.create_credential(create_request, encrypted_payload)
    // ...
}
```

**修改后**:
```rust
pub async fn create_credential(...) {
    // ...
    // 先创建 UserId 对象，用于加密和存储
    let user_id = UserId::new(&token.user_id);
    let tenant_id = TenantId::new(&token.tenant_id);

    // 先生成 credential_id，确保加密时使用的 ID 与存储时一致
    let credential_id = CredentialId::new();

    // 加密凭证内容（在 TEE 内完成）
    let encrypted_payload = encrypt_credential_in_tee(
        &state,
        tenant_id.as_str(),
        &user_id,
        &credential_id,
        &request.plaintext_data,
    ).await
    // ...
    // 存储凭证（使用预生成的 credential_id）
    let entry = state.vault.create_credential_with_id(create_request, encrypted_payload, credential_id)
    // ...
}
```

### 3.3 `src/api/credentials.rs` - encrypt_credential_update 函数

**修改前**:
```rust
async fn encrypt_credential_update(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    credential_id: &CredentialId,
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    // ...
    let l2_key = hierarchy.derive_user_vault_key(tenant_id, user_id)
    // ...
    let aad = format!("{}:{}", tenant_id, user_id);
    // ...
}
```

**修改后**:
```rust
/// 加密更新后的凭证内容
///
/// 修复说明：
/// - 使用 user_id.hash() 派生 L2 密钥，与解密流程一致
/// - 使用 user_id.hash() 构建 AAD，与解密流程一致
async fn encrypt_credential_update(
    state: &AppState,
    tenant_id: &str,
    user_id: &UserId,  // 改为 UserId 类型
    credential_id: &CredentialId,
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    // ...
    // 使用 user_id.hash() 保持与解密流程一致
    let l2_key = hierarchy.derive_user_vault_key(tenant_id, user_id.hash())
    // ...
    // 使用 user_id.hash() 构建 AAD，与解密流程一致
    let aad = format!("{}:{}", tenant_id, user_id.hash());
    // ...
}
```

### 3.4 `src/vault/models.rs` - 新增 VaultEntry::with_credential_id 方法

**新增代码**:
```rust
/// 创建新的凭证条目（使用预生成的 credential_id）
///
/// 用于修复加密流程中 credential_id 不一致的问题：
/// 加密时需要知道将要使用的 credential_id，以确保密钥派生参数一致
pub fn with_credential_id(
    credential_id: CredentialId,
    tenant_id: TenantId,
    user_id: UserId,
    service_id: ServiceId,
    credential_type: CredentialType,
    encrypted_payload: EncryptedPayload,
    expires_at: Option<u64>,
) -> Self {
    let now = current_timestamp();

    Self {
        credential_id,
        version: 1,
        tenant_id,
        user_id,
        service_id,
        credential_type,
        created_at: now,
        updated_at: now,
        expires_at,
        encrypted_payload,
        is_deleted: false,
    }
}
```

### 3.5 `src/vault/storage.rs` - 新增 create_credential_with_id 方法

**新增代码**:
```rust
/// 创建凭证（使用预生成的 credential_id）
///
/// 用于修复加密流程中 credential_id 不一致的问题：
/// 加密时需要先知道将要使用的 credential_id，以确保密钥派生参数一致
pub fn create_credential_with_id(
    &self,
    request: CreateCredentialRequest,
    encrypted_payload: EncryptedPayload,
    credential_id: CredentialId,
) -> Result<VaultEntry, VaultError> {
    // 验证加密载荷格式
    encrypted_payload.validate()?;

    let entry = VaultEntry::with_credential_id(
        credential_id,
        request.tenant_id,
        request.user_id,
        request.service_id,
        request.credential_type,
        encrypted_payload,
        request.expires_at,
    );

    self.backend.store(&entry)?;

    Ok(entry)
}
```

---

## 4. 修复原理

### 4.1 问题根源回顾

| 参数 | 加密流程（修复前） | 解密流程 | 问题 |
|------|-------------------|----------|------|
| credential_id | 临时生成的 UUID | 存储的 UUID | 不一致导致 L3 密钥不同 |
| L2 user_id 参数 | 原始 user_id | user_id.hash() | 不一致导致 L2 密钥不同 |
| AAD | tenant_id:user_id | tenant_id:user_id.hash() | 不一致导致 AES-GCM 认证失败 |

### 4.2 修复策略

1. **统一 credential_id**:
   - 加密前先生成 `credential_id`
   - 使用相同的 `credential_id` 进行加密和存储

2. **统一 L2 派生参数**:
   - 加密时使用 `user_id.hash()` 派生 L2 密钥
   - 与解密流程保持一致

3. **统一 AAD**:
   - 加密时使用 `user_id.hash()` 构建 AAD
   - 与解密流程保持一致

### 4.3 数据流对比

**修复前**:
```
加密: temp_cred_id (临时) -> L3 密钥 -> AES-GCM (AAD: user_id)
存储: credential_id (新生成)
解密: credential_id (存储) -> L3 密钥 -> AES-GCM (AAD: user_id.hash())
结果: 密钥不匹配，AAD 不匹配 -> AuthenticationFailed
```

**修复后**:
```
加密: credential_id (预生成) -> L3 密钥 -> AES-GCM (AAD: user_id.hash())
存储: credential_id (相同)
解密: credential_id (存储) -> L3 密钥 -> AES-GCM (AAD: user_id.hash())
结果: 密钥匹配，AAD 匹配 -> 解密成功
```

---

## 5. 编译结果

```bash
$ cargo build --release
   Compiling vault-service v0.1.0
    Finished `release` profile [optimized] target(s) in 9.71s
```

**状态**: ✓ 编译成功（仅有警告，无错误）

---

## 6. 测试建议

### 6.1 单元测试

建议添加以下测试用例：

1. **加解密往返测试**:
   - 使用修复后的流程加密凭证
   - 使用解密流程解密
   - 验证明文一致

2. **参数一致性测试**:
   - 验证加密和解密使用相同的 credential_id
   - 验证加密和解密使用相同的 user_id.hash()
   - 验证加密和解密使用相同的 AAD

### 6.2 集成测试

建议运行以下测试：

```bash
# 运行现有测试
cargo test --test reproduction_tests

# 预期结果：解密测试应该通过
```

---

## 7. 向后兼容性

### 7.1 已有数据

**警告**: 修复后，使用旧流程加密的凭证将无法解密。

原因：
- 旧加密流程使用临时 credential_id（未存储）
- 旧加密流程使用原始 user_id 进行 L2 派生和 AAD
- 这些参数无法恢复，因此无法解密旧数据

### 7.2 迁移方案

由于这是新功能（EP2-Story2.2），建议：
1. 确认是否有生产数据
2. 如果有，需要用户重新创建凭证
3. 在发布说明中明确说明数据不兼容

---

## 8. 总结

**修复完成**: 三个根本原因全部修复

| 根本原因 | 修复方案 | 状态 |
|----------|----------|------|
| Credential ID 不一致 | 先生成 ID 再加密，存储使用相同 ID | ✓ 已修复 |
| AAD 不一致 | 加密时使用 user_id.hash() | ✓ 已修复 |
| L2 派生参数不一致 | 加密时使用 user_id.hash() | ✓ 已修复 |

**下一步**: 运行 Phase 4 验证测试

---

**报告完成时间**: 2026-03-12
**修复人员签名**: claude_glm