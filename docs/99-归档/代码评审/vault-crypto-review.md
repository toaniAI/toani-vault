# Vault & Crypto 模块对抗性代码审查报告

**审查日期**: 2026-03-19
**审查员**: 对抗性代码审查 Agent
**审查范围**: Vault 后端、Vault 客户端、密钥派生、Token 存储
**严重程度等级**: CRITICAL / HIGH / MEDIUM / LOW / INFO

---

## 执行摘要

本次审查覆盖 4 个文件，共识别 **18 个问题**，其中：

| 严重程度 | 数量 |
|---------|------|
| CRITICAL | 2 |
| HIGH | 4 |
| MEDIUM | 6 |
| LOW | 4 |
| INFO | 2 |

**总体评估**：各模块按照 Tech-Spec 完成了主要安全修复（尤其是 Token 加密和 PBKDF2 替换），但仍存在若干结构性安全缺陷和实现偏差，部分问题需要在生产环境上线前解决。

---

## 第一部分：Token 存储加密审查 (`src/mcp/token_storage.rs`)

### 对应 Tech-Spec: `tech-spec-token-storage-encryption.md`

### [PASS] 核心加密已实现

Tech-Spec P0 要求的 AES-256-GCM 加密已正确实现：

- `encrypt_token`（L554-573）：使用 `Aes256Gcm::new_from_slice` + 随机 nonce，正确分离密文和 auth_tag
- `decrypt_token`（L577-592）：正确重组 `ciphertext || auth_tag` 后传入 `cipher.decrypt`
- 测试覆盖包含防篡改验证（L938-960）

**这是本 PR 最重要的安全修复，实现正确。**

---

### FINDING-01: 加密密钥生命周期不安全 [CRITICAL]

**文件**: `src/mcp/token_storage.rs`
**行号**: L295-301

```rust
// 生成加密密钥
let mut encryption_key = vec![0u8; 32];
get_random_bytes(&mut encryption_key);

Ok(Self {
    ...
    encryption_key: SecureBuffer::with_data(&encryption_key),
    // encryption_key Vec 在此 Ok(Self {...}) 之后 drop，SecureBuffer 已存有副本
```

**问题**: `encryption_key` 是一个栈上的 `Vec<u8>`，`SecureBuffer::with_data(&encryption_key)` 传入的是切片引用并进行了内部复制，但原始 `Vec` 在函数返回后 drop 时其内容并**未**被 zeroize 清除（`Vec<u8>` 不实现 `Zeroize`）。这意味着密钥材料在栈上或堆上存留直至内存被覆盖，存在侧信道或内存泄漏风险。

**影响**: 密钥材料在 `McpTokenStorage::new()` 调用栈帧中存在短暂泄露窗口。

**建议**: 用 `zeroize::Zeroizing<Vec<u8>>` 包装临时密钥，确保 drop 时自动清零。

---

### FINDING-02: `clone_for_task` 复制密钥到新 Arc [HIGH]

**文件**: `src/mcp/token_storage.rs`
**行号**: L641-651

```rust
fn clone_for_task(&self) -> Arc<Self> {
    Arc::new(McpTokenStorage {
        ...
        encryption_key: SecureBuffer::with_data(self.encryption_key.as_slice()),
        ...
    })
}
```

**问题**: 每次调用 `clone_for_task`（Token 轮换时调用，L511-518）都会创建新的 `SecureBuffer` 副本，即将完整的 AES-256 密钥**复制**到一个新的 `Arc<McpTokenStorage>` 实例中。该 Arc 被传入 `tokio::spawn` 的后台任务（仅用于删除旧 Token），实际上只需要共享 `metadata_cache` 和 `encrypted_tokens`，不应携带密钥材料。

**影响**:
1. 密钥材料在 heap 上存在多份副本，增加内存泄漏暴露面
2. 后台任务不需要解密能力却持有密钥，违反最小权限原则

**建议**: 将后台清理任务分离为仅持有 `Arc<RwLock<HashMap<...>>>` 的闭包，不传递 `McpTokenStorage` 实例。

---

### FINDING-03: 密钥环 (Keychain) 集成为纯模拟 [HIGH]

**文件**: `src/mcp/token_storage.rs`
**行号**: L595-610

```rust
async fn store_to_keychain(...) -> Result<(), TokenStorageError> {
    // 在实际实现中使用 macOS Security Framework
    // 这里仅模拟
    Ok(())
}

async fn delete_from_keychain(&self, _token_id: &str) -> Result<(), TokenStorageError> {
    // 在实际实现中使用 macOS Security Framework
    Ok(())
}
```

**问题**: Tech-Spec 明确指出 Token 存储的安全承诺包括"使用操作系统密钥环（macOS Keychain）"，但两个密钥环函数均为空实现。所有加密 Token 仅存储于**进程内存**（`encrypted_tokens: Arc<RwLock<HashMap<...>>>`），进程重启后全部丢失，也没有持久化到任何安全存储。

**影响**:
- 进程崩溃后所有 MCP Token 丢失，用户需重新认证
- 与文档承诺的安全等级不符
- `keychain_initialized` 标志从未实际检查存储功能是否生效

**建议**: 若 Keychain 集成尚未就绪，应在文档和 API 注释中明确标注"内存存储模式"，并在 `TokenStorageConfig` 中增加 `storage_backend: StorageBackend` 枚举（`InMemory | Keychain`），避免误导调用方。

---

### FINDING-04: `generate_new_token` 中使用 `unwrap()` [MEDIUM]

**文件**: `src/mcp/token_storage.rs`
**行号**: L680

```rust
fn generate_new_token() -> String {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes).unwrap(); // <-- unwrap
    format!("tok_{}", hex::encode(bytes))
}
```

**问题**: 在生产代码中使用 `unwrap()`，违反项目 Rust 编码标准。`SystemRandom::fill` 在极端情况下（如 OS 熵池耗尽）可能返回错误，导致 panic。

**建议**: 将 `generate_new_token` 修改为返回 `Result<String, TokenStorageError>`，调用方在 `rotate_token` 中正确处理错误。

---

### FINDING-05: `get_random_bytes` 使用 `rand::thread_rng` 而非 `ring::SystemRandom` [MEDIUM]

**文件**: `src/mcp/token_storage.rs`
**行号**: L686-690

```rust
fn get_random_bytes(buffer: &mut [u8]) {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    rng.fill_bytes(buffer);
}
```

**问题**: 同文件的 `generate_new_token`（L676-683）已使用 `ring::rand::SystemRandom`（显式密码学安全 RNG），但 `get_random_bytes`（用于生成加密 nonce）却使用 `rand::thread_rng()`。虽然 `rand::thread_rng()` 在大多数平台上底层使用 OsRng，但与项目其他密码学代码的 RNG 策略不一致，增加审计复杂度。

**建议**: 统一使用 `ring::rand::SystemRandom` 进行密码学随机数生成，与 `generate_new_token` 保持一致。

---

### FINDING-06: Token 轮换时新 Token 为本地随机值而非服务端刷新 [MEDIUM]

**文件**: `src/mcp/token_storage.rs`
**行号**: L490-500

```rust
// 生成新 Token（这里使用随机值，实际应该从认证服务获取）
let new_token = generate_new_token();
```

**问题**: `rotate_token` 自行生成一个随机字符串作为"新 Token"，而不是向认证服务（如 OAuth 刷新端点）申请新的合法 Token。这个自生成的 Token 无法被任何下游服务认证。

**影响**: Token 轮换功能在逻辑上是空壳，轮换后的 Token 实际上无法使用，可能导致业务静默失败。

**建议**: 在 `TokenStorageConfig` 中添加轮换回调（如 `token_refresh_fn`），或在方法签名中接收新 Token 值，而非自行生成。

---

### FINDING-07: `mark_rotated` 传入自身 ID 作为 previous_token_id [LOW]

**文件**: `src/mcp/token_storage.rs`
**行号**: L504-508

```rust
if let Some(meta) = metadata_cache.get_mut(token_id) {
    meta.mark_rotated(token_id.to_string()); // token_id 是旧 Token 的 ID，传入自身
}
```

**问题**: `mark_rotated(previous_token_id: String)` 的语义是"将此 Token 标记为已轮换，记录其前驱 Token ID"。但这里传入的是 `token_id`（旧 Token 自身的 ID），应该传入 `new_token_id`（指向新 Token）或不传入。

**影响**: 审计链路中 `previous_token_id` 字段的语义混乱，可能影响安全审计。

---

## 第二部分：Vault 后端审查 (`src/vault/backend.rs`)

### 对应 Tech-Spec: `tech-spec-vault-backend-fixes.md`

---

### FINDING-08: `block_on` 在异步上下文中存在死锁风险 [CRITICAL]

**文件**: `src/vault/backend.rs`
**行号**: L94-101

```rust
fn block_on<F, T>(&self, future: F) -> Result<T, VaultBackendError>
where
    F: std::future::Future<Output = Result<T, VaultClientError>>,
{
    self.runtime_handle
        .block_on(future)
        .map_err(VaultBackendError::ClientError)
}
```

**问题**: `Handle::block_on()` 在 Tokio 的**当前线程调度器**（`tokio::runtime::Builder::new_current_thread()`）上调用时会导致**死锁**——因为 `block_on` 需要运行时推进 future，而当前线程已被 `block_on` 阻塞。即使是多线程运行时，在已有异步上下文的工作线程上调用也会 panic（Tokio 明确禁止在 async 上下文中调用 `block_on`）。

**影响**: `StorageBackend` trait 方法（`store`、`get`、`delete` 等）均为同步方法，如果被异步调用链调用（如在 `async fn` 中通过 trait 对象调用），会直接 panic 或死锁。

**根因**: `StorageBackend` trait 设计为同步接口，但底层实现是异步 Vault 客户端，阻抗不匹配。

**建议**: 将 `StorageBackend` trait 改为 `async fn`，或使用 `tokio::task::spawn_blocking` 将 Vault 操作移至专用线程池，避免在异步上下文中调用 `block_on`。

---

### FINDING-09: `get` 方法遍历所有租户，O(N*M) 性能问题及安全风险 [HIGH]

**文件**: `src/vault/backend.rs`
**行号**: L199-233

```rust
fn get(&self, credential_id: &CredentialId) -> Result<Option<VaultEntry>, VaultError> {
    let tenant_ids = self.list_all_tenants()...;

    for tenant_id in tenant_ids {
        match self.block_on(self.client.read_secret(&tenant_id, credential_id.as_str())) {
            Ok(json_data) => { ... return Ok(Some(...)); }
            Err(VaultClientError::SecretNotFound(_)) => { continue; }
            Err(e) => return Err(...),
        }
    }
    Ok(None)
}
```

**问题**:
1. **性能**: 查询单个凭证需要遍历**所有租户**，对每个租户发起一次 Vault API 请求。租户数量为 N、每租户凭证数量为 M，`get` 操作最坏情况为 O(N) 次 Vault 请求。
2. **安全**: 在多租户系统中，跨租户扫描本身是一个权限问题。若 `credential_id` 恰好在错误的租户路径下存在（命名冲突），会返回错误租户的数据。Tech-Spec 也提到这个问题（问题 2），但代码中没有通过租户索引改进。
3. **信息泄露**: 遍历 404 错误时的 `continue` 逻辑正确，但每次 Vault 访问都会产生审计日志，大量跨租户扫描可能在 Vault 审计日志中留下异常模式。

**建议**: `credential_id` 应该携带 `tenant_id` 信息（如通过复合 ID 或额外参数），避免全局扫描。

---

### FINDING-10: `updated_at` 修复完成但实现方式与 Tech-Spec 不完全一致 [MEDIUM]

**文件**: `src/vault/backend.rs`
**行号**: L150-151

```rust
updated_at: data.updated_at.unwrap_or(data.created_at),
```

**分析**: 代码已实现 Tech-Spec 要求的 `updated_at.unwrap_or(created_at)` 降级逻辑（符合验收标准）。但 Tech-Spec 要求从 Vault KV v2 的 metadata API 中读取 `updated_time`，当前 `VaultCredentialData` 中的 `updated_at` 字段（`client.rs:L424`）是应用层自行维护的字段，而不是从 Vault metadata API 中读取的服务端时间戳。

**影响**: 如果应用在更新凭证时忘记设置 `updated_at`（`VaultCredentialData.updated_at` 默认为 `None`），会静默回退到 `created_at`，无错误提示。

---

### FINDING-11: `parse_encrypted_payload` 中 `unwrap_or("")` 静默丢失密文 [MEDIUM]

**文件**: `src/vault/backend.rs`
**行号**: L168-174

```rust
ciphertext: data
    .encrypted_payload
    .split('.')
    .next()
    .unwrap_or("")  // <-- 若无 '.' 分隔符，返回空字符串
    .to_string(),
```

**问题**: `encrypted_payload` 字段格式为 `"{ciphertext}.{auth_tag}"`（L105-108 中拼接），若格式不符（如数据损坏），`split('.').next()` 会返回整个字符串（无 `.` 时），或 `unwrap_or("")` 返回空字符串（不会触发）。但实际上当字段格式损坏时，`next()` 永远不会返回 `None`（`split` 至少产生一个元素），`unwrap_or("")` 永远不会执行。

更严重的是 `entry_to_data`（L104-128）将 `auth_tag` 分别存储在 `encrypted_payload` 字符串（作为后缀）和独立的 `auth_tag` 字段中，存在**数据冗余和不一致**风险：如果两处 `auth_tag` 不同步，解密时使用哪个？

**建议**: 消除 `encrypted_payload` 字段中内嵌 `auth_tag` 的格式（`ciphertext.auth_tag`），改为分字段单独存储，避免解析歧义。

---

### FINDING-12: `data_to_entry` 中 `credential_type` 缺失类型处理为 `ApiKey` [LOW]

**文件**: `src/vault/backend.rs`
**行号**: L142-149

```rust
credential_type: match data.credential_type.as_str() {
    "username_password" => crate::models::CredentialType::UsernamePassword,
    "oauth_refresh" => crate::models::CredentialType::OAuthRefresh,
    "api_key" => crate::models::CredentialType::ApiKey,
    "session_cookie" => crate::models::CredentialType::SessionCookie,
    "kyc_document" => crate::models::CredentialType::KycDocument,
    _ => crate::models::CredentialType::ApiKey, // <-- 静默降级
},
```

**问题**: 未知 `credential_type` 静默降级为 `ApiKey`，而不是返回错误。这在数据损坏或版本不兼容时会导致凭证类型被错误解析，可能引起后续操作的权限或逻辑错误。

**建议**: 对未知类型返回 `Err(VaultBackendError::InvalidCredentialFormat(...))`。

---

### FINDING-13: `purge` 操作逻辑与 `delete_secret` 不匹配 [LOW]

**文件**: `src/vault/backend.rs`
**行号**: L322-342

```rust
fn purge(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
    // 物理删除：从 Vault 中永久移除
    for tenant_id in tenant_ids {
        match self.block_on(self.client.delete_secret(&tenant_id, credential_id.as_str())) {
```

**问题**: `purge` 声称是"物理删除、不可恢复"，但调用的是 `client.delete_secret`（L292-305），后者实现为 `kv2::delete_latest`（KV v2 软删除，可通过 `undelete` 恢复）。真正的物理删除应该调用 `client.destroy_secret`（L310-324，使用 `kv2::delete_metadata`）。

**影响**: `purge` 实际上是软删除，用户认为已永久删除的凭证仍可被有 Vault 管理员权限的攻击者恢复。

---

## 第三部分：Vault 客户端审查 (`src/vault/client.rs`)

### 对应 Tech-Spec: `tech-spec-vault-backend-fixes.md`

---

### FINDING-14: `VaultConfig` 中 `token` 字段未 Zeroize [HIGH]

**文件**: `src/vault/client.rs`
**行号**: L41-68

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct VaultConfig {
    pub addr: String,
    pub token: String,  // Vault Token 明文存储
    ...
}
```

**问题**:
1. `VaultConfig` 派生了 `Clone`，Vault Token 会被任意复制，无法追踪副本数量。
2. `VaultConfig` 未实现 `Zeroize` 或 `ZeroizeOnDrop`，Vault Token 在结构体 drop 时**不会被清除**，可能在内存中长期驻留。
3. `VaultConfig` 派生了 `Debug`，Vault Token 可能被意外打印到日志（`format!("{:?}", config)` 会输出 Token）。

**影响**: Vault Token 泄漏风险，违反"仅 TEE Enclave 持有 Vault Token"的安全设计原则。

**建议**:
- 实现自定义 `Debug`，将 `token` 字段遮盖（如显示 `"[REDACTED]"`）
- 为 `VaultConfig` 实现 `Drop` + `Zeroize`，清除 `token` 字段
- 考虑将 `token` 封装为 `SecureBuffer` 或 `Zeroizing<String>`

---

### FINDING-15: `undelete_secret` 未实现但返回错误而非 `unimplemented!` [LOW]

**文件**: `src/vault/client.rs`
**行号**: L327-338

```rust
pub async fn undelete_secret(...) -> Result<(), VaultClientError> {
    // 注意：vaultrs 0.7 版本的 undelete API 可能不同
    // 这里暂时返回未实现错误
    Err(VaultClientError::WriteFailed(
        "Undelete not implemented in current vaultrs version".to_string(),
    ))
}
```

**问题**: 使用 `WriteFailed` 错误类型来表示"未实现"在语义上不准确。调用方若检查错误类型进行恢复逻辑，会误判为写入失败而不是功能缺失。

**建议**: 添加 `VaultClientError::NotImplemented(String)` 变体，或使用 `todo!()` 宏明确标注（但 `todo!()` 在生产代码中会 panic，应酌情选择）。

---

### FINDING-16: `verify_connection` 调用 `sys::health` 仅检查 Vault 存活，未验证 Token 权限 [MEDIUM]

**文件**: `src/vault/client.rs`
**行号**: L198-205

```rust
async fn verify_connection(client: &VaultClient) -> Result<(), VaultClientError> {
    match vaultrs::sys::health(client).await {
        Ok(_) => Ok(()),
        Err(e) => Err(VaultClientError::ConnectionFailed(e.to_string())),
    }
}
```

**问题**: `sys::health` 是 Vault 的无认证端点，即使 Token 无效或过期，只要 Vault 服务存活也会返回 200。初始化时不会检测 Token 权限问题，导致认证失败只在实际操作时才被发现。

**建议**: 在 `verify_connection` 中额外执行一次需要认证的轻量操作（如 `kv2::list` 根路径），真正验证 Token 有效性。

---

## 第四部分：密钥派生审查 (`src/crypto/key_derivation.rs`)

### 对应 Tech-Spec: `tech-spec-crypto-security-hardening.md`

### [PASS] PBKDF2 已正确替换

Tech-Spec 要求的核心修复已实现：

- `pbkdf2_hmac_sha256`（L405-413）：使用 `ring::pbkdf2::derive(PBKDF2_HMAC_SHA256, ...)` 标准实现，`iterations` 参数真正参与迭代计算
- 测试 `test_pbkdf2_is_not_single_hash`（L611-619）验证不同迭代次数产生不同输出
- 测试 `test_pbkdf2_deterministic`（L621-629）验证确定性

---

### FINDING-17: `HardwareMeasurements::from_system()` 返回模拟值 [HIGH]

**文件**: `src/crypto/key_derivation.rs`
**行号**: L97-106

```rust
pub fn from_system() -> Result<Self, KeyDerivationError> {
    // 在实际实现中，这里会读取真实的硬件信息
    // 这里使用模拟值
    Ok(Self {
        cpu_id_hash: generate_simulated_hash("cpu"),
        motherboard_hash: generate_simulated_hash("motherboard"),
        tee_measurement: generate_simulated_hash("tee"),
        memory_fingerprint: generate_simulated_hash("memory"),
    })
}
```

**问题**: `generate_simulated_hash` 对固定字符串（`"cpu"`、`"motherboard"` 等）计算 SHA256，**在所有机器上产生完全相同的值**。这意味着：
1. 硬件绑定完全无效——任何机器都会产生相同的"硬件测量值"
2. `derive_l1_key_enhanced` 调用此函数时，L1 主密钥的"硬件熵源"实际上是常量，而非设备特定值
3. L1 主密钥的安全性退化为仅依赖密码派生和 HSM 随机值两源

**影响**: 模块文档声称"硬件测量值绑定到特定设备，防止单点故障"，但实际无任何绑定效果。若攻击者获得 HSM 随机值（或 HSM 本身也是模拟的），即可在任意机器上重建密钥。

**建议**:
- 在注释中明确标注此函数**仅用于测试/开发环境**，生产代码禁止调用
- 添加 `#[cfg(test)]` 限制或 `debug_assert!` 防止生产环境误用
- 为生产环境提供真实的 TEE attestation 实现接口（哪怕是 trait 抽象）

---

### FINDING-18: `HSMRandomValue::from_hsm()` 为模拟实现 [HIGH]

**文件**: `src/crypto/key_derivation.rs`
**行号**: L215-227

```rust
pub fn from_hsm() -> Result<Self, KeyDerivationError> {
    let mut random_bytes = [0u8; 32];
    // 在实际实现中从 HSM 获取真随机数
    // 这里使用模拟
    get_random_bytes(&mut random_bytes);  // 使用 rand::thread_rng()，不是真 HSM

    Ok(Self {
        random_bytes,
        hsm_device_id: "hsm_001".to_string(),  // 硬编码设备 ID
        generated_at: current_timestamp(),
    })
}
```

**问题**:
1. HSM 随机值实际来自 `rand::thread_rng()`，不是真实 HSM 设备
2. `hsm_device_id` 硬编码为 `"hsm_001"`，没有真实设备绑定
3. 与 FINDING-17 组合：L1 主密钥的三个熵源中，硬件测量是常量，HSM 是软件 RNG，仅密码派生是真正用户输入

**影响**: L1 主密钥的多源安全设计在当前实现中降级为单源（用户密码），与架构声称的安全等级严重不符。

**建议**: 同 FINDING-17，应通过 trait 抽象（`HardwareProvider`、`HsmProvider`）注入真实实现，并在生产构建中强制要求真实实现。

---

### FINDING-19: `generate_key_handle` 通过哈希密钥材料生成句柄，暴露单向关联 [MEDIUM]

**文件**: `src/crypto/key_derivation.rs`
**行号**: L394-400

```rust
fn generate_key_handle(key_material: &[u8]) -> KeyHandle {
    let hash = digest(&SHA256, key_material);
    let mut handle = [0u8; 32];
    handle.copy_from_slice(hash.as_ref());
    handle
}
```

**问题**: `KeyHandle` 的类型定义为 `[u8; 32]`（即密钥材料的 SHA256 哈希），调用方（`derive_l1_key`，L348-350）返回此句柄，但实际上"句柄"等价于**密钥的确定性指纹**。若攻击者已知密钥候选集，可以通过比对句柄验证猜测（离线字典攻击辅助工具）。

更严重的是：`KeyHandle` 作为 `[u8; 32]` 返回，调用方可能直接将其作为密钥使用（而非真正的密钥材料），这会导致使用 SHA256(key) 而非 key 作为加密密钥，相当于对密钥做了一次哈希转换，虽不直接降低安全性，但如果调用方期望返回的是密钥本身，则逻辑错误。

**建议**: 明确区分"密钥句柄"（key handle，用于索引密钥）和"密钥材料"（key material，用于加密），避免两者混用。

---

### FINDING-20（已通过）: PBKDF2 迭代次数 100,000 低于 OWASP 2023 建议 [INFO]

**文件**: `src/crypto/key_derivation.rs`
**行号**: L163

```rust
let iterations = 100_000; // OWASP 推荐值
```

**分析**: Tech-Spec 注明 OWASP 2023 建议为 **600,000 次**（PBKDF2-SHA256），当前实现为 100,000 次（基于 OWASP 较旧建议）。虽代码注释称"OWASP 推荐值"但实际低于最新建议值。这不是错误，属于安全加固建议。

**建议**: 考虑将迭代次数升级至 600,000（OWASP 2023），注意会增加约 6 倍 CPU 开销，需要在性能和安全之间权衡。

---

### FINDING-21: `current_timestamp` 使用 `expect`，在系统时钟异常时 panic [INFO]

**文件**: `src/crypto/key_derivation.rs` (L435-440) 及 `src/mcp/token_storage.rs` (L693-698)

```rust
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")  // <-- 系统时钟回拨时 panic
        .as_secs()
}
```

**问题**: 若系统时钟早于 UNIX_EPOCH（极端情况，如 VM 时钟错误），`duration_since` 返回 `Err`，`expect` 触发 panic，导致整个服务崩溃。

**建议**: 用 `unwrap_or(0)` 或 `unwrap_or_else(|_| ...)` 替代，提供降级处理。

---

## 审查总结

### 按文件汇总

| 文件 | CRITICAL | HIGH | MEDIUM | LOW | INFO |
|------|---------|------|--------|-----|------|
| `src/mcp/token_storage.rs` | 1 | 2 | 2 | 1 | 1 |
| `src/vault/backend.rs` | 1 | 1 | 2 | 2 | 0 |
| `src/vault/client.rs` | 0 | 1 | 1 | 1 | 0 |
| `src/crypto/key_derivation.rs` | 0 | 2 | 1 | 0 | 2 |

### 必须在生产上线前修复

| 优先级 | 问题 | 文件 |
|--------|------|------|
| CRITICAL | FINDING-08: `block_on` 死锁风险 | `backend.rs` |
| CRITICAL | FINDING-01: 临时密钥未 zeroize | `token_storage.rs` |
| HIGH | FINDING-14: `VaultConfig.token` 未 zeroize + Debug 泄露 | `client.rs` |
| HIGH | FINDING-17/18: 硬件测量和 HSM 为模拟实现 | `key_derivation.rs` |
| HIGH | FINDING-02: `clone_for_task` 无必要复制密钥 | `token_storage.rs` |
| HIGH | FINDING-13: `purge` 调用软删除而非物理删除 | `backend.rs` |

### 可在后续迭代修复

| 优先级 | 问题 | 文件 |
|--------|------|------|
| MEDIUM | FINDING-03: Keychain 集成为空实现 | `token_storage.rs` |
| MEDIUM | FINDING-06: Token 轮换生成无效 Token | `token_storage.rs` |
| MEDIUM | FINDING-09: `get` 方法全局租户扫描 | `backend.rs` |
| MEDIUM | FINDING-11: `encrypted_payload` 格式冗余 | `backend.rs` |
| MEDIUM | FINDING-16: `verify_connection` 不验证 Token 权限 | `client.rs` |
| MEDIUM | FINDING-19: `KeyHandle` 语义混淆 | `key_derivation.rs` |

### 符合 Tech-Spec 要求的项目 [PASS]

- Token 存储 AES-256-GCM 加密已正确实现（FINDING-01 的临时变量问题独立存在）
- PBKDF2 已替换为 `ring::pbkdf2::derive` 标准实现
- `updated_at` 字段已实现 `unwrap_or(created_at)` 降级逻辑
- AES-GCM 防篡改测试已覆盖
- zeroize 在结构体 Drop 中已实现（`HardwareMeasurements`、`PasswordDerivation`、`HSMRandomValue`、`EncryptedToken`）
- 常量时间比较已使用 `ct_compare`

---

*审查完成时间: 2026-03-19*
*下一步: CRITICAL 和 HIGH 级别问题建议在下一个 Sprint 内解决*
