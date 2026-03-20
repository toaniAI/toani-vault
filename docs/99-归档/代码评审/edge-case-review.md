---
title: '全量变更集边缘案例审查报告'
created: '2026-03-19'
reviewer: 'edge-case-hunter'
method: 'exhaustive-path-enumeration'
severity-levels: CRASH / SECURITY / CRYPTO / UPGRADE / CACHE / OTHER
---

# 全量变更集边缘案例审查报告

**审查日期**: 2026-03-19
**审查范围**: 当前所有未提交变更（30 个文件，+2382 / -465 行）
**审查方法**: 穷举路径枚举（Edge Case Hunter）— 机械地遍历每条分支路径和边界条件，只报告未处理路径，不评价代码质量

---

## 目录

1. [执行摘要](#执行摘要)
2. [高危问题（崩溃 / 未定义行为）](#高危问题崩溃--未定义行为)
3. [安全问题](#安全问题)
4. [加密问题](#加密问题)
5. [升级流程问题](#升级流程问题)
6. [Redis 缓存问题](#redis-缓存问题)
7. [其他问题](#其他问题)
8. [修复优先级建议](#修复优先级建议)

---

## 执行摘要

本次审查覆盖以下核心变更文件：

| 文件 | 变更量 | 风险评级 |
|------|--------|---------|
| `src/tee/upgrade.rs` | +389/-? | **CRASH** |
| `src/mcp/token_storage.rs` | +102/-? | **CRASH / CRYPTO** |
| `src/api/websocket.rs` | +92/-? | **CRASH** |
| `src/tenant/config.rs` | +89/-? | **CACHE** |
| `src/api/audit.rs` | +19/-? | **SECURITY** |
| `src/tee/attestation.rs` | +44/-? | **SECURITY** |
| `src/tee/dcap.rs` | +63/-? | **SECURITY** |
| `src/tee/driver_verify.rs` | +77/-? | MEDIUM |
| `src/tee/sandbox/export/data_export.rs` | +91/-? | MEDIUM |
| `src/tee/sandbox/export/screenshot.rs` | +68/-? | MEDIUM |
| `src/vault/backend.rs` | +24/-? | LOW |
| `mcp-server/src/sse.rs` | +62/-? | MEDIUM |
| `mcp-server/src/tools.rs` | +132/-? | MEDIUM |
| `src/services/llm/azure.rs` | +93/-? | LOW |
| `src/services/llm/openai.rs` | +81/-? | LOW |

**核心结论**：

- **未定义行为（UB）**：`src/tee/upgrade.rs` 在多线程 tokio 运行时中调用 `unsafe { std::env::set_var }`，这是已知的 Rust UB。
- **审计静默丢失**：`try_lock` 在高并发下失败时，凭证操作审计记录被静默跳过，无任何可观测信号。
- **加密实现缺陷**：`token_storage.rs` 的 AES-GCM 输出长度未校验，nonce 长度未校验。
- **升级状态机存在多处未处理路径**：`migrate_traffic` 重复调用会绕过 Sealing Key 迁移阶段；`complete_upgrade` 在 `pending_version` 为 None 时静默完成。

---

## 高危问题（崩溃 / 未定义行为）

### CRASH-001：`env::set_var` 在多线程运行时中引发未定义行为

**文件**: `src/tee/upgrade.rs:430`
**严重程度**: CRASH / UB
**触发条件**: `start_new_enclave` 调用 `unsafe { std::env::set_var(...) }` 时，其他 tokio 线程同时读取或迭代环境变量

**漏洞路径**：
```rust
// 触发路径：start_new_enclave() 成功启动子进程后
unsafe { std::env::set_var("TEE_NEW_ENCLAVE_PID", child.id().to_string()) };
// ↑ 多线程下调用 set_var 是 UB（Rust 标准库明确文档）
```

**修复草图**：
```rust
// 用 AtomicU32 字段替代 env var 存储 PID
struct BlueGreenUpgradeManager {
    // ...
    new_enclave_pid: AtomicU32,  // 新增字段
}

// 替换 set_var 调用
self.new_enclave_pid.store(child.id(), Ordering::SeqCst);
std::mem::forget(child);
```

**潜在后果**: 内存损坏，进程崩溃，数据竞争。

---

### CRASH-002：`AesGcmNonce::from_slice` 在 nonce 非 12 字节时 panic

**文件**: `src/mcp/token_storage.rs:584`
**严重程度**: CRASH
**触发条件**: `encrypted.nonce()` 返回非 12 字节切片（外部构造或反序列化的 `EncryptedToken`）

**漏洞路径**：
```rust
fn decrypt_token(&self, encrypted: &EncryptedToken) -> Result<String, TokenStorageError> {
    // ...
    let nonce_val = AesGcmNonce::from_slice(encrypted.nonce());
    // ↑ 内部调用 assert_eq!(nonce.len(), 12)，长度不符则 panic
```

**修复草图**：
```rust
if encrypted.nonce().len() != 12 {
    return Err(TokenStorageError::DecryptionError(
        format!("Invalid nonce length: {}, expected 12", encrypted.nonce().len())
    ));
}
let nonce_val = AesGcmNonce::from_slice(encrypted.nonce());
```

**潜在后果**: 进程 panic，token 解密服务不可用。

---

### CRASH-003：WebSocket 截图路径构造 `PageStateFreezer` 可能 panic

**文件**: `src/api/websocket.rs:661`
**严重程度**: CRASH
**触发条件**: `state.session_id` 不是合法格式，`PageStateFreezer::new` 内部断言失败

**漏洞路径**：
```rust
async fn take_screenshot(state: &ConnectionState, _ctx: &ApiContext) -> ... {
    let freezer = PageStateFreezer::new(state.session_id);
    // ↑ 若 session_id 格式非法，PageStateFreezer::new 内部可能 panic
```

**修复草图**：
```rust
// 在调用前验证 session_id 格式
let session_uuid = Uuid::parse_str(&state.session_id)
    .map_err(|_| format!("Invalid session_id format: {}", state.session_id))?;
let freezer = PageStateFreezer::new(session_uuid);
```

**潜在后果**: WebSocket handler panic，连接静默断开，客户端无错误响应。

---

## 安全问题

### SEC-001：审计记录在高并发下静默丢失

**文件**: `src/api/credentials.rs:163`
**严重程度**: SECURITY
**触发条件**: `record_to_storage` 内 `try_lock()` 在多个并发请求下竞争失败

**漏洞路径**：
```rust
fn record_to_storage(&self, action, user_id, credential_id, outcome, jti, mrenclave) {
    if let Ok(storage) = self.storage.try_lock() {
        // 写入审计
    }
    // ↑ try_lock 失败时：静默返回，无日志，无错误，审计记录永久丢失
```

**修复草图**：
```rust
match self.storage.try_lock() {
    Ok(storage) => { /* 写入审计 */ }
    Err(_) => {
        log::warn!(
            "[AUDIT-DROP] Lock contention: credential={}, action={:?}, user={}",
            credential_id, action, user_id
        );
    }
}
```

**潜在后果**: 凭证操作无审计记录，合规审计失效，且无任何可观测信号。

---

### SEC-002：`verify_audit_log` 使用空公钥时行为不可预期

**文件**: `src/api/audit.rs:674`
**严重程度**: SECURITY
**触发条件**: `state.verifier_public_key` 为空 `Vec`（服务未正确初始化）

**漏洞路径**：
```rust
let pub_key = UnparsedPublicKey::new(&ED25519, &state.verifier_public_key);
// ↑ verifier_public_key 为空时，ring 返回不透明 Unspecified 错误
// 调用方无法区分"空公钥配置错误"与"签名无效"
```

**修复草图**：
```rust
if state.verifier_public_key.is_empty() {
    return Err(ApiError::new(
        "config_error",
        "Audit verifier public key not configured"
    ));
}
```

**潜在后果**: 所有审计签名验证静默失败，或因 ring 内部错误路径产生误导性错误消息。

---

### SEC-003：`verify_signature` 无公钥配置时非零签名静默通过

**文件**: `src/tee/attestation.rs:920`
**严重程度**: SECURITY
**触发条件**: `verifier_public_key` 为 `None`，且 Quote 签名非全零

**漏洞路径**：
```rust
fn verify_signature(&self, quote: &Quote) -> Result<(), AttestationError> {
    // ...
    if let Some(ref vk) = self.verifier_public_key {
        // 真实 ECDSA 验证
        return Ok(());
    }
    // ↑ 未配置公钥时：所有非零签名在此处静默通过（Fail-Open）
    Ok(())
}
```

**修复草图**：
```rust
// 未配置公钥且非模拟模式时，拒绝非零签名
if self.verifier_public_key.is_none() && !self.allow_simulation {
    return Err(AttestationError::SignatureVerificationFailed);
}
Ok(())
```

**潜在后果**: 攻击者可提交任意非零签名的伪造 Quote，绕过 TEE 认证。

---

## 加密问题

### CRYPTO-001：AES-GCM 输出长度过短时截断产生空密文

**文件**: `src/mcp/token_storage.rs:570`
**严重程度**: CRYPTO
**触发条件**: `cipher.encrypt()` 返回长度小于 16 字节（理论上不应发生，但缺乏防御性校验）

**漏洞路径**：
```rust
let combined = cipher.encrypt(nonce_val, token.as_bytes())?;
let split_at = combined.len().saturating_sub(16);
// ↑ 若 combined.len() < 16：split_at = 0
let ciphertext_only = combined[..split_at].to_vec();  // 空 vec
let auth_tag_bytes = &combined[split_at..];  // 全部内容被误作 auth_tag
```

**修复草图**：
```rust
if combined.len() < 16 {
    return Err(TokenStorageError::EncryptionError(
        format!("AES-GCM output too short: {} bytes", combined.len())
    ));
}
let split_at = combined.len() - 16;
```

**潜在后果**: 空密文存储后解密时 auth_tag 校验失败，token 永久不可用。

---

## 升级流程问题

### UPGRADE-001：`migrate_traffic` 重复调用绕过 Sealing Key 迁移阶段

**文件**: `src/tee/upgrade.rs:629`
**严重程度**: HIGH
**触发条件**: 渐进式迁移多次调用 `migrate_traffic()`，当阶段已为 `MigratingSealingKey` 时再次调用

**漏洞路径**：
```rust
pub async fn migrate_traffic(&self, percentage: u8) -> Result<(), UpgradeError> {
    // 入口检查允许 MigratingTraffic OR MigratingSealingKey
    // ...
    self.phase.store(UpgradePhase::MigratingTraffic as u8, ...);  // 重置为 MigratingTraffic
    // 更新权重...
    self.phase.store(UpgradePhase::MigratingSealingKey as u8, ...);  // 立即推进
    // ↑ 无论 percentage 是 10% 还是 100%，都立即进入 SealingKey 阶段
    // 第二次调用时：先退回 MigratingTraffic，再推进 MigratingSealingKey，跳过实际的 key 迁移
```

**修复草图**：
```rust
// 仅在 percentage == 100 时推进到 MigratingSealingKey
if clamped == 100 {
    self.phase.store(UpgradePhase::MigratingSealingKey as u8, Ordering::SeqCst);
} else {
    self.phase.store(UpgradePhase::MigratingTraffic as u8, Ordering::SeqCst);
}
```

**潜在后果**: Sealing Key 迁移步骤被跳过，密钥绑定停留在旧 MRENCLAVE，新 Enclave 无法解密数据。

---

### UPGRADE-002：健康检查超时为 0 时所有检查立即失败

**文件**: `src/tee/upgrade.rs:536`
**严重程度**: HIGH
**触发条件**: `UpgradeConfig { health_check_timeout_secs: 0, .. }` 传入

**漏洞路径**：
```rust
let timeout_secs = self.config.health_check_timeout_secs;
// timeout_secs = 0
let stream = tokio::time::timeout(
    Duration::from_secs(timeout_secs),  // Duration::ZERO
    tokio::net::TcpStream::connect(&addr),
).await  // 立即超时
```

**修复草图**：
```rust
let timeout_secs = self.config.health_check_timeout_secs.max(1);
```

**潜在后果**: 每次健康检查立即超时，失败计数器快速达到上限，自动触发回滚。

---

### UPGRADE-003：HTTP 响应跨 TCP 段时状态行解析失败

**文件**: `src/tee/upgrade.rs:556`
**严重程度**: MEDIUM
**触发条件**: HTTP 响应头跨多个 TCP 数据包传输，单次 `read` 仅读取到响应行的一部分

**漏洞路径**：
```rust
let mut buf = [0u8; 256];
let n = stream.read(&mut buf).await?;
// ↑ 单次 read，若响应分包则可能只读到 "HTTP/1.1 2" 而非 "HTTP/1.1 200 OK"
if let Ok(response) = std::str::from_utf8(&buf[..n]) {
    let first_line = response.lines().next().unwrap_or("");
    if let Some(status_str) = first_line.split_whitespace().nth(1) {
        // ↑ 若 first_line 被截断，nth(1) 可能返回 None
```

**修复草图**：
```rust
// 循环读取直到收到完整首行（包含 \r\n）
let mut buf = Vec::with_capacity(256);
loop {
    let mut chunk = [0u8; 64];
    let n = stream.read(&mut chunk).await?;
    buf.extend_from_slice(&chunk[..n]);
    if buf.windows(2).any(|w| w == b"\r\n") || n == 0 { break; }
    if buf.len() > 512 { break; }  // 防超长响应
}
```

**潜在后果**: 状态码解析失败，健康检查返回 false，误触发自动回滚。

---

### UPGRADE-004：HTTPS URL 通过明文 TCP 连接导致健康检查失败

**文件**: `src/tee/upgrade.rs:900`（`parse_health_url` 函数）
**严重程度**: MEDIUM
**触发条件**: `new_enclave_health_url` 配置为 `https://...`

**漏洞路径**：
```rust
fn parse_health_url(url: &str) -> Option<(String, u16, String)> {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    // ↑ 接受 https:// 但后续建立的是明文 TCP 连接，TLS 握手会失败
```

**修复草图**：
```rust
fn parse_health_url(url: &str) -> Option<(String, u16, String)> {
    if url.starts_with("https://") {
        log::error!("HTTPS health check URL not supported (no TLS): {}", url);
        return None;
    }
    let without_scheme = url.strip_prefix("http://")?;
    // ...
}
```

**潜在后果**: 健康检查始终失败（TLS 握手失败或乱码响应），误触发自动回滚。

---

### UPGRADE-005：`complete_upgrade` 在 `pending_version` 为 None 时静默完成

**文件**: `src/tee/upgrade.rs:699`
**严重程度**: HIGH
**触发条件**: `complete_upgrade` 被调用时 `pending_version` 已被意外清空（并发回滚、重复调用等）

**漏洞路径**：
```rust
pub async fn complete_upgrade(&self) -> Result<UpgradeResult, UpgradeError> {
    // ...
    {
        let mut active = self.active_version.write().await;
        let mut pending = self.pending_version.write().await;
        if let Some(new_version) = pending.take() {
            *active = Some(new_version);
            // ...
        }
        // ↑ pending 为 None 时：if let 不匹配，active 不更新，但函数继续执行
    }
    // 设置完成状态...
    self.phase.store(UpgradePhase::Completed as u8, ...);
    Ok(UpgradeResult::Success)  // ← 静默成功，但升级实际未完成
```

**修复草图**：
```rust
let new_version = pending.take().ok_or_else(|| {
    UpgradeError::InvalidState("No pending version to complete upgrade".to_string())
})?;
*active = Some(new_version);
```

**潜在后果**: 升级标记为成功，但 `active_version` 未更新，旧 Enclave 继续运行。

---

## Redis 缓存问题

### CACHE-001：`set_ex` TTL 为 0 时写入静默丢弃

**文件**: `src/tenant/config.rs:830`
**严重程度**: HIGH
**触发条件**: `RedisTenantConfigCache::new(url, 0)` 创建实例后调用 `set()`

**漏洞路径**：
```rust
let _: Result<(), _> = conn.set_ex(&key, data, self.ttl_seconds).await;
// ↑ ttl_seconds = 0 时：Redis 返回 ERR invalid expire time
// Result 被 `_:` 绑定静默丢弃
```

**修复草图**：
```rust
let ttl = if self.ttl_seconds == 0 {
    log::warn!("Redis TTL is 0, defaulting to 1 second");
    1u64
} else {
    self.ttl_seconds
};
let _: Result<(), _> = conn.set_ex(&key, data, ttl).await;
```

**潜在后果**: `set()` 调用无任何效果，缓存永不写入，下游始终命中 miss，性能急剧下降。

---

### CACHE-002：`set()` 序列化失败无日志，下次 `get()` 返回旧值

**文件**: `src/tenant/config.rs:851`
**严重程度**: MEDIUM
**触发条件**: `serde_json::to_string(config)` 失败（循环引用、不可序列化字段等）

**漏洞路径**：
```rust
async fn set(&self, tenant_id: &TenantId, config: &TenantConfig) {
    // ...
    if let Ok(data) = serde_json::to_string(config) {
        let _: Result<(), _> = conn.set_ex(&key, data, self.ttl_seconds).await;
    }
    // ↑ serde_json 失败时：静默退出，无日志，调用方不感知
```

**修复草图**：
```rust
match serde_json::to_string(config) {
    Ok(data) => { let _: Result<(), _> = conn.set_ex(&key, data, self.ttl_seconds).await; }
    Err(e) => {
        log::error!("Failed to serialize TenantConfig for tenant {}: {}", tenant_id.as_str(), e);
    }
}
```

**潜在后果**: 配置更新静默失败，旧缓存无限期提供服务，系统行为与预期配置不符。

---

### CACHE-003：`clear()` 中 SCAN 出错时 cursor 归零提前退出，部分键未删除

**文件**: `src/tenant/config.rs:860`
**严重程度**: MEDIUM
**触发条件**: Redis SCAN 命令在循环中途失败，`unwrap_or((0, vec![]))` 使 cursor 返回 0，触发退出条件

**漏洞路径**：
```rust
loop {
    let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
        // ...
        .query_async(&mut conn)
        .await
        .unwrap_or((0, vec![]));  // ← 错误时 cursor 强制为 0
    // ...
    cursor = next_cursor;
    if cursor == 0 { break; }  // ← 正常结束条件与错误条件相同，无法区分
}
```

**修复草图**：
```rust
let scan_result: Result<(u64, Vec<String>), _> = redis::cmd("SCAN")
    // ...
    .query_async(&mut conn).await;
match scan_result {
    Ok((next_cursor, keys)) => {
        // 正常处理
        cursor = next_cursor;
        if cursor == 0 { break; }
    }
    Err(e) => {
        log::error!("SCAN failed during clear(), tenant config keys may be orphaned: {}", e);
        break;
    }
}
```

**潜在后果**: 部分租户配置键未删除，形成孤儿缓存，影响后续租户配置读取一致性。

---

## 其他问题

### OTHER-001：MCP SSE 空 `session_id` 注册后遮蔽后续消息

**文件**: `mcp-server/src/sse.rs:299`
**严重程度**: MEDIUM
**触发条件**: `params.session_id` 为空字符串，Token 验证通过

**漏洞路径**：
```rust
// Token 验证通过后：
state.sessions.register_session(&params.session_id, ...).await;
// ↑ session_id 为 "" 被注册
// 后续所有 session_id="" 的消息路由到同一 session，消息混淆
```

**修复草图**：
```rust
if params.session_id.is_empty() {
    return Err(SseError::AuthFailed("session_id must not be empty".to_string()));
}
```

**潜在后果**: 多个空 session_id 连接共享同一消息队列，消息严重混淆。

---

### OTHER-002：`key_hierarchy` 读锁跨两个 await 点持有时间过长

**文件**: `mcp-server/src/tools.rs:196`
**严重程度**: MEDIUM
**触发条件**: L2、L3 密钥派生均在同一 `read().await` 锁范围内，两次派生各有 await 点

**漏洞路径**：
```rust
let l3_key = {
    let hierarchy = self.state.key_hierarchy.read().await;  // 获取读锁
    let l2_key = hierarchy.derive_user_vault_key(...)?;     // await 1
    hierarchy.derive_credential_key(&l2_key, ...)?          // await 2
    // ↑ 读锁持有跨越两次异步操作，阻塞所有写操作（密钥轮换等）
};  // 锁在此释放
```

**修复草图**：
```rust
// 仅在锁内完成 L2 派生，clone 后释放锁
let l2_key = {
    let hierarchy = self.state.key_hierarchy.read().await;
    hierarchy.derive_user_vault_key(tenant_id, user_hash)?
};  // 读锁释放
// 用 l2_key 继续派生 L3（不持有锁）
let l3_key = {
    let hierarchy = self.state.key_hierarchy.read().await;
    hierarchy.derive_credential_key(&l2_key, credential_id, KeyPurpose::CredentialEncryption)?
};
```

**潜在后果**: 写锁等待时间过长，密钥轮换操作被长期阻塞。

---

### OTHER-003：LLM 服务 `choices` 为空时静默返回空字符串

**文件**: `src/services/llm/azure.rs:265` 及 `src/services/llm/openai.rs:265`
**严重程度**: LOW
**触发条件**: API 返回 `choices: []`（速率限制、内容过滤、模型错误等场景）

**漏洞路径**：
```rust
let content = azure_response.choices.first()
    .map(|c| match &c.message.content { ... })
    .unwrap_or_default();  // ← choices 为空时返回 ""
Ok(ChatResponse { content, .. })  // ← Ok("") 与正常空回复无法区分
```

**修复草图**：
```rust
let choice = azure_response.choices.first()
    .ok_or_else(|| LlmError::EmptyResponse("API returned 0 choices".to_string()))?;
```

**潜在后果**: 调用方收到 `Ok("")`，无法区分 API 错误和正常空响应，可能将空结果持久化。

---

### OTHER-004：CSV 导出混合 Schema 对象数组丢失列值

**文件**: `src/tee/sandbox/export/data_export.rs:498`
**严重程度**: MEDIUM
**触发条件**: 数组中各对象键集不一致（首对象缺少某键，后续对象有额外键）

**漏洞路径**：
```rust
// 以第一个对象的键作为表头
let headers: Vec<_> = first.keys().cloned().collect();
// ...
for item in arr {
    if let Object(obj) = item {
        let row: Vec<String> = obj.values().map(...).collect();
        // ↑ obj.values() 顺序与 headers 无关，混合 schema 时列错位
```

**修复草图**：
```rust
// 收集所有对象键的有序并集作为表头
let all_keys: IndexSet<String> = arr.iter()
    .filter_map(|v| v.as_object())
    .flat_map(|o| o.keys().cloned())
    .collect();
// 按 all_keys 顺序填充每行，缺失键填 ""
for item in arr {
    if let Object(obj) = item {
        let row: Vec<String> = all_keys.iter()
            .map(|k| obj.get(k).map(json_value_to_csv_cell).unwrap_or_default())
            .collect();
```

**潜在后果**: 额外键的值静默丢失，CSV 数据存在列错位，无法被正确解析。

---

### OTHER-005：截图 PageInfo 中 `about:blank` URL 被过滤为 `"unknown"`

**文件**: `src/tee/sandbox/export/screenshot.rs:706`
**严重程度**: LOW
**触发条件**: 沙箱页面 URL 恰好是 `about:blank`（初始化阶段或特殊页面）

**漏洞路径**：
```rust
if let Some(real_url) = self.playwright.get_page_info().await {
    // get_page_info 内部：
    // if url != "about:blank" { return Some(url) }
    // ↑ about:blank 被过滤，返回 None
}
// None 时降级为 "unknown"
```

**修复草图**：
```rust
// 接受所有非 None URL，包括 about:blank
pub async fn get_page_info(&self) -> Option<String> {
    // ...
    let result: Option<String> = page.evaluate("window.location.href").await.ok()?.into_value().ok();
    result  // 不过滤 about:blank
}
```

**潜在后果**: 截图元数据记录 `"unknown"` URL，审计溯源失败。

---

### OTHER-006：`vault/backend.rs` 中 `updated_at = 0` 时间戳语义歧义

**文件**: `src/vault/backend.rs:118`
**严重程度**: LOW
**触发条件**: `entry.updated_at == 0`（Unix 纪元 1970-01-01，表示"从未更新"）

**漏洞路径**：
```rust
data.updated_at = Some(entry.updated_at);  // Some(0) 而非 None
// ...
updated_at: data.updated_at.unwrap_or(data.created_at),
// ↑ Some(0).unwrap_or(...) = 0，不会回退到 created_at
// 数据库记录 updated_at = Unix 纪元，审计时间轴错误
```

**修复草图**：
```rust
data.updated_at = if entry.updated_at == 0 { None } else { Some(entry.updated_at) };
```

**潜在后果**: 审计日志和监控面板显示 `1970-01-01` 为最后更新时间，误导运维人员。

---

## 修复优先级建议

### P0（立即修复，可能导致生产事故）

| ID | 文件 | 问题摘要 |
|----|------|---------|
| CRASH-001 | `src/tee/upgrade.rs:430` | `env::set_var` 多线程 UB |
| CRASH-002 | `src/mcp/token_storage.rs:584` | nonce 长度 panic |
| SEC-001 | `src/api/credentials.rs:163` | 审计记录静默丢失 |
| UPGRADE-001 | `src/tee/upgrade.rs:629` | migrate_traffic 绕过 SealingKey 迁移 |
| UPGRADE-005 | `src/tee/upgrade.rs:699` | complete_upgrade 静默成功但未更新版本 |

### P1（Sprint 内修复）

| ID | 文件 | 问题摘要 |
|----|------|---------|
| CRASH-003 | `src/api/websocket.rs:661` | PageStateFreezer 构造 panic |
| SEC-002 | `src/api/audit.rs:674` | 空公钥验证行为不可预期 |
| SEC-003 | `src/tee/attestation.rs:920` | 签名验证 Fail-Open |
| CRYPTO-001 | `src/mcp/token_storage.rs:570` | AES-GCM 输出长度未校验 |
| UPGRADE-002 | `src/tee/upgrade.rs:536` | 健康检查超时为 0 |
| CACHE-001 | `src/tenant/config.rs:830` | TTL=0 写入静默丢弃 |

### P2（下个迭代）

| ID | 文件 | 问题摘要 |
|----|------|---------|
| UPGRADE-003 | `src/tee/upgrade.rs:556` | HTTP 分段读取解析失败 |
| UPGRADE-004 | `src/tee/upgrade.rs:900` | HTTPS URL 明文 TCP |
| CACHE-002 | `src/tenant/config.rs:851` | set() 序列化失败无日志 |
| CACHE-003 | `src/tenant/config.rs:860` | SCAN 错误部分删除 |
| OTHER-001 | `mcp-server/src/sse.rs:299` | 空 session_id 注册 |
| OTHER-002 | `mcp-server/src/tools.rs:196` | 读锁跨 await 时间过长 |
| OTHER-004 | `src/tee/sandbox/export/data_export.rs:498` | CSV 混合 schema 列错位 |

### P3（技术债清理）

| ID | 文件 | 问题摘要 |
|----|------|---------|
| OTHER-003 | `src/services/llm/azure.rs:265` | LLM 空 choices 静默 |
| OTHER-005 | `src/tee/sandbox/export/screenshot.rs:706` | about:blank URL 过滤 |
| OTHER-006 | `src/vault/backend.rs:118` | updated_at=0 语义歧义 |

---

## 汇总统计

| 分类 | 数量 |
|------|------|
| 高危（CRASH / UB） | 3 |
| 安全（SECURITY） | 3 |
| 加密（CRYPTO） | 1 |
| 升级流程（UPGRADE） | 5 |
| Redis 缓存（CACHE） | 3 |
| 其他（OTHER） | 6 |
| **合计** | **21** |

---

*报告生成时间: 2026-03-19*
*审查者: edge-case-hunter*
*审查方法: 穷举路径枚举——遍历每条分支路径和边界条件，仅报告未处理路径，不评价代码整体质量*
