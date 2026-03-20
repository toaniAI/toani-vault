---
title: 'P0 - Token 存储真实 AES-256-GCM 加密实现'
slug: 'token-storage-encryption'
created: '2026-03-19'
status: 'implemented'
priority: 'P0'
---

# Tech-Spec: P0 - Token 存储真实 AES-256-GCM 加密实现

## 概述

### 问题陈述

`src/mcp/token_storage.rs` 中的 `encrypt_token` 和 `decrypt_token` 函数是空实现，存在严重安全漏洞：

- `encrypt_token`（第 556 行）：`ciphertext` 直接返回明文字节，`auth_tag` 始终为固定零值 `[0u8; 16]`，完全没有加密
- `decrypt_token`（第 566 行）：直接将"密文"当作 UTF-8 字符串返回，实际读回的就是明文

这意味着所有通过 `McpTokenStorage` 存储的 MCP Token 均以**明文形式**保存在内存中，与注释声称的"加密存储"完全不符，违背了模块头部文档中"不再明文存储 Token"的安全承诺。

### 解决方案

使用项目中已有的 `aes-gcm` crate（Cargo.toml 第 12 行已声明 `aes-gcm = "0.10"`），参照 `src/crypto/cipher.rs` 中成熟的 `encrypt_credential` / `decrypt_credential` 模式，对 `encrypt_token` 和 `decrypt_token` 实现真实的 AES-256-GCM 加密/解密，使用存储在 `McpTokenStorage.encryption_key` 中的 32 字节密钥。

### 范围

**在范围内：**
- 修改 `src/mcp/token_storage.rs` 中 `encrypt_token` 函数，实现真实 AES-256-GCM 加密
- 修改 `src/mcp/token_storage.rs` 中 `decrypt_token` 函数，实现真实 AES-256-GCM 解密
- 添加必要的 `use` 导入（`aes_gcm::*`）
- 为两个函数添加单元测试，验证加解密正确性及认证标签防篡改

**不在范围内：**
- 修改 `EncryptedToken` 结构体（已有合适的 `ciphertext`、`auth_tag`、`nonce` 字段）
- 修改密钥管理逻辑（`encryption_key` 生成方式保持不变）
- 实现 macOS Keychain 存储（`store_to_keychain` 仍可保持模拟）
- 修改 `TokenStorageConfig`、`TokenMetadata` 等无关结构

---

## 开发上下文

### 当前代码（关键片段）

```rust
// src/mcp/token_storage.rs:549-570

/// 加密 Token
fn encrypt_token(&self, token: &str) -> Result<EncryptedToken, TokenStorageError> {
    // 使用 AES-256-GCM 加密
    // 这里简化处理
    let mut nonce = [0u8; 12];
    get_random_bytes(&mut nonce);

    let ciphertext = token.as_bytes().to_vec(); // 简化：实际应加密
    let auth_tag = [0u8; 16]; // 简化：实际应有认证标签

    Ok(EncryptedToken::new(ciphertext, auth_tag, nonce))
}

/// 解密 Token
fn decrypt_token(&self, encrypted: &EncryptedToken) -> Result<String, TokenStorageError> {
    // 使用 AES-256-GCM 解密
    // 这里简化处理
    let plaintext = std::str::from_utf8(encrypted.ciphertext())
        .map_err(|_| TokenStorageError::DecryptionError("无效的 UTF-8".to_string()))?;

    Ok(plaintext.to_string())
}
```

### 代码库模式

项目 `src/crypto/cipher.rs` 已有完整的 AES-256-GCM 实现，可直接参照：

```rust
// src/crypto/cipher.rs:118-173（encrypt_credential 核心逻辑）

use aes_gcm::{
    Aes256Gcm, Nonce as AesGcmNonce,
    aead::{Aead, KeyInit, Payload},
};

// 生成随机 nonce (96-bit)
let mut nonce_bytes = [0u8; NONCE_LENGTH]; // NONCE_LENGTH = 12
let mut rng = rand::thread_rng();
rng.fill_bytes(&mut nonce_bytes);

// 创建 AES-256-GCM cipher
let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
    .map_err(|_| CryptoError::EncryptionError("Invalid key".to_string()))?;

let nonce = AesGcmNonce::from_slice(&nonce_bytes);

// 执行加密（返回值 = 密文 + auth_tag 后 16 字节）
let ciphertext = cipher
    .encrypt(nonce, plaintext)
    .map_err(|_| CryptoError::EncryptionError("Encryption failed".to_string()))?;

// 分离密文和 auth tag
let auth_tag_start = ciphertext.len().saturating_sub(16);
let ciphertext_only = ciphertext[..auth_tag_start].to_vec();
let auth_tag = ciphertext[auth_tag_start..].to_vec();
```

**关键点：** `aes-gcm` crate 的 `encrypt` 返回值是 `密文 || auth_tag` 的拼接，解密时需将两者重新拼接后传入 `decrypt`，或使用 `decrypt` 直接传入拼接后的 ciphertext。

`McpTokenStorage` 结构体：
- `self.encryption_key` 是 `SecureBuffer`（32 字节），通过 `.as_slice()` 获取原始字节
- `EncryptedToken` 字段：`ciphertext: SecureBuffer`（存 ciphertext-only 部分），`auth_tag: [u8; 16]`，`nonce: [u8; 12]`

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/mcp/token_storage.rs:549-570` | 要修改的目标函数 |
| `src/mcp/token_storage.rs:135-178` | `EncryptedToken` 结构定义，了解字段含义 |
| `src/mcp/token_storage.rs:270-302` | `McpTokenStorage` 结构定义，了解 `encryption_key` |
| `src/crypto/cipher.rs:118-240` | 已有 AES-256-GCM 加密/解密参考实现 |
| `src/crypto/constant_time.rs` | `SecureBuffer` 的 `.as_slice()` 方法 |
| `Cargo.toml:12` | 确认 `aes-gcm = "0.10"` 已在依赖中 |

### 技术决策

1. **密钥来源**：使用 `self.encryption_key.as_slice()`（已有 32 字节随机密钥，在 `new()` 中生成）
2. **Nonce 生成**：保留已有的 `get_random_bytes(&mut nonce)` 调用（已正确生成随机 nonce）
3. **解密时的字节拼接**：`aes-gcm` 解密需要传入 `ciphertext || auth_tag` 的完整拼接字节，需在 `decrypt_token` 中手动拼接：
   ```
   let mut full_ciphertext = encrypted.ciphertext().to_vec();
   full_ciphertext.extend_from_slice(encrypted.auth_tag());
   ```
4. **`EncryptedToken::new` 参数**：`auth_tag` 参数类型为 `[u8; 16]`，需从 Vec 转换为数组：
   ```
   let mut tag_arr = [0u8; 16];
   tag_arr.copy_from_slice(&auth_tag_bytes);
   ```
5. **错误映射**：加密失败映射到 `TokenStorageError::EncryptionError`，解密失败映射到 `TokenStorageError::DecryptionError`

---

## 实现计划

### 任务

（按依赖顺序排列）

1. **在 `token_storage.rs` 顶部添加 aes-gcm 导入** — `src/mcp/token_storage.rs:16`

   在现有 `use` 块中添加：
   ```rust
   use aes_gcm::{
       Aes256Gcm, Nonce as AesGcmNonce,
       aead::{Aead, KeyInit},
   };
   ```

2. **实现 `encrypt_token`** — `src/mcp/token_storage.rs:549-560`

   替换函数体，实现真实 AES-256-GCM 加密：
   - 保留已有的 `get_random_bytes(&mut nonce)` 调用
   - 使用 `Aes256Gcm::new_from_slice(self.encryption_key.as_slice())` 初始化 cipher
   - 调用 `cipher.encrypt(AesGcmNonce::from_slice(&nonce), token.as_bytes())`
   - 从返回值中分离密文（前 N 字节）和 auth_tag（后 16 字节）
   - 将 auth_tag `Vec<u8>` 转换为 `[u8; 16]` 数组
   - 返回 `EncryptedToken::new(ciphertext_only, auth_tag_arr, nonce)`

3. **实现 `decrypt_token`** — `src/mcp/token_storage.rs:562-570`

   替换函数体，实现真实 AES-256-GCM 解密：
   - 使用 `Aes256Gcm::new_from_slice(self.encryption_key.as_slice())` 初始化 cipher
   - 拼接 `ciphertext || auth_tag` 为完整字节序列
   - 调用 `cipher.decrypt(AesGcmNonce::from_slice(encrypted.nonce()), full_ciphertext.as_slice())`
   - 将解密后的字节转换为 UTF-8 字符串，失败时返回 `DecryptionError`

4. **添加单元测试** — `src/mcp/token_storage.rs`（文件末尾 `#[cfg(test)] mod tests` 块）

   测试用例见验收标准。

### 验收标准

**场景 1：正常加解密轮**
- Given: 一个 `McpTokenStorage` 实例，token 值为任意非空字符串（如 `"my-secret-token-12345"`）
- When: 先调用 `encrypt_token(token)` 再调用 `decrypt_token(&encrypted)`
- Then: 返回的字符串与原始 token 完全一致

**场景 2：加密后密文不等于明文**
- Given: 同上
- When: 调用 `encrypt_token(token)` 后检查 `encrypted.ciphertext()`
- Then: `encrypted.ciphertext()` 字节内容与 `token.as_bytes()` 不相同

**场景 3：auth_tag 不再是全零**
- Given: 同上
- When: 调用 `encrypt_token(token)` 后检查 `encrypted.auth_tag()`
- Then: `encrypted.auth_tag()` 不等于 `[0u8; 16]`

**场景 4：篡改密文后解密失败**
- Given: 一个加密后的 `EncryptedToken`
- When: 手动修改 ciphertext 的第一个字节（翻转），再调用 `decrypt_token`
- Then: 返回 `Err(TokenStorageError::DecryptionError(...))`，不能返回任何明文

**场景 5：使用不同密钥解密失败**
- Given: 实例 A 加密的 `EncryptedToken`
- When: 使用实例 B（不同的 `encryption_key`）调用 `decrypt_token`
- Then: 返回 `Err(TokenStorageError::DecryptionError(...))`

---

## 附加上下文

### 依赖

`aes-gcm = "0.10"` 已在 `Cargo.toml` 第 12 行声明，**无需添加新依赖**。

### 测试策略

在 `src/mcp/token_storage.rs` 文件末尾新增 `#[cfg(test)] mod tests` 块（若已存在则追加），包含：
- `test_encrypt_decrypt_roundtrip`：验证正常加解密轮
- `test_ciphertext_is_not_plaintext`：验证密文不等于明文
- `test_auth_tag_not_zero`：验证 auth_tag 不为全零
- `test_tampered_ciphertext_fails`：验证篡改检测

### 注意事项

1. **`aes-gcm` encrypt 返回格式**：`encrypt` 返回 `ciphertext || tag`（tag 为最后 16 字节），而不是单独的密文。分离方法：
   ```rust
   let combined = cipher.encrypt(...)?;
   let split_at = combined.len() - 16;
   let ciphertext_only = combined[..split_at].to_vec();
   let auth_tag_bytes = &combined[split_at..]; // 16 字节
   ```
2. **`decrypt` 输入格式**：必须传入 `ciphertext || tag` 的拼接，而不是单独密文。
3. **空 token 边界情况**：若 token 为空字符串，加密后 ciphertext 为空但 auth_tag 仍有效，解密应返回空字符串。无需特殊处理，`aes-gcm` 自动支持。
4. **`SecureBuffer::as_slice()`**：查看 `src/crypto/constant_time.rs` 确认方法签名。若 `McpTokenStorage` 结构体的 `encryption_key` 字段是私有的，`encrypt_token` 是 `McpTokenStorage` 的方法，可直接通过 `self.encryption_key.as_slice()` 访问。
5. **不要 `clone` 敏感数据**：密钥 slice 使用后无需手动 zeroize，`SecureBuffer` 的 `Drop` 实现已处理。
