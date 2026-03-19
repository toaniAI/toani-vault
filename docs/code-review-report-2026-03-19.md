# 代码审查报告

**审查任务**: `/bmad-review-adversarial-general`
**审查日期**: 2026-03-19
**审查范围**: 30个文件，2382行新增代码
**审查方式**: 4个专项审查子代理并发执行

---

## 执行摘要

| 严重级别 | 数量 | 状态 |
|---------|------|------|
| 🔴 严重（安全漏洞/功能完全失效） | 8 | 必须修复 |
| 🟠 重大（规范偏差/稳定性问题） | 13 | 建议修复 |
| 🟡 一般 | 7 | 可选修复 |

**整体结论**: 本次变更存在多处安全关键功能实质上未实现或实现与规范严重偏差的情况。**不建议合并，需逐项修复后重新审查。**

---

## 详细发现

### 一、Token 存储加密模块 (`src/mcp/token_storage.rs`)

#### 🔴 [严重] `get_random_bytes` 使用非密码学安全 RNG 生成主加密密钥和 Nonce

**问题描述**:
`rand::thread_rng()` 文档上不保证是 CSPRNG（密码学安全随机数生成器），而同文件的 `generate_new_token` 已正确使用 `ring::rand::SystemRandom`，形成不一致。

**风险**: 若 RNG 输出可预测或存在偏差，攻击者可能暴力还原 nonce 或主密钥，彻底破解所有存储的 token。

**修复建议**:
```rust
fn get_random_bytes(buffer: &mut [u8]) {
    use ring::rand::{SecureRandom, SystemRandom};
    SystemRandom::new().fill(buffer).expect("OS RNG failed");
}
```

---

#### 🔴 [严重] 主加密密钥进程重启后永久丢失，Keychain 集成是空实现

**问题描述**:
`store_to_keychain` 仅返回 `Ok()`，密钥只存在于内存。所有历史加密 Token 在进程重启后无法解密。

**风险**: 模块文档头承诺"使用操作系统密钥环"，但实际是虚假的安全声明。调用方无法得知这一差异，可能在不知情的情况下依赖一个无效的安全保证。

**修复建议**:
在 `initialize_keychain` 中真正实现 macOS Keychain 读写，或至少在启动时明确返回错误，提示生产环境中 Keychain 未实现。

---

#### 🟠 [重大] `encryption_key` 传入 `SecureBuffer` 之前未 zeroize

**问题描述**:
原始 `Vec<u8>` 通过普通 `Drop` 释放，不保证内存清零，密钥材料残留在堆上。

**风险**: 内存 dump 或 cold-boot 攻击可能从堆内存中恢复原始密钥材料。

**修复建议**:
```rust
let mut encryption_key = vec![0u8; 32];
get_random_bytes(&mut encryption_key);
let secure_key = SecureBuffer::with_data(&encryption_key);
encryption_key.zeroize();  // 显式清零原始缓冲区
```

---

#### 🟠 [重大] `clone_for_task()` 复制密钥材料，内存中密钥副本数量失控

**问题描述**:
`tokio::spawn` 中使用了 `clone_for_task()` 创建新实例并再次复制 `encryption_key`，与 `SecureBuffer` 集中管理敏感材料的设计意图背道而驰。

**修复建议**:
将 `McpTokenStorage` 包装为 `Arc<McpTokenStorage>` 并直接 `Arc::clone`，避免复制 `encryption_key`。

---

#### 🟠 [重大] `rotate_token` 中 `mark_rotated` 传入的是自己的 ID 而非前驱 Token ID

**问题描述**:
```rust
if let Some(meta) = metadata_cache.get_mut(token_id) {
    meta.mark_rotated(token_id.to_string());  // 传的是自己的 ID
}
```

**风险**: 审计链路断裂，`previous_token_id` 字段完全失去意义。

---

### 二、TEE 蓝绿升级流程 (`src/tee/upgrade.rs`)

#### 🔴 [严重] `unsafe { std::env::set_var() }` 在 Tokio 多线程运行时中存在数据竞争

**位置**: `src/tee/upgrade.rs:435`

**问题描述**:
注释声称"此时为单线程启动阶段，set_var 是安全的"，但这是错误假设。`BlueGreenUpgradeManager` 运行在 Tokio 多线程运行时中，`start_new_enclave` 是 async fn，其他线程可能并发调用 `std::env::var`。`std::env::set_var` 在多线程中是 UB（Rust 1.80 已将其标记为 `unsafe`）。

**风险**: 潜在内存安全问题；违反 Rust 多线程内存模型

**修复建议**:
PID 应存储在结构体字段中（如 `AtomicU64` 或 `RwLock<Option<u32>>`），而非环境变量。

---

#### 🔴 [严重] 回滚时状态重置顺序错误，存在状态不一致窗口

**位置**: `src/tee/upgrade.rs:838-840`

**问题描述**:
代码先将 `phase` 设为 `Idle`，再将 `is_upgrading` 设为 `false`。在这两行之间，`phase == Idle` 但 `is_upgrading == true`，任何读取 `is_upgrading` 的调用者都会看到矛盾状态。`complete_upgrade` 也存在同样问题（行 762-763）。

**修复建议**:
正确顺序应是先设置 `is_upgrading = false`，再设置 `phase = Idle`，或使用单一原子操作的复合状态。

---

#### 🟠 [重大] `migrate_traffic` 任意百分比都触发 `MigratingSealingKey`

**位置**: `src/tee/upgrade.rs:621-654`

**问题描述**:
技术规范（TEE-203）明确要求"仅当 `percentage == 100` 时才转换阶段到 `MigratingSealingKey`；否则停留在 `MigratingTraffic`"。当前实现不论传入 10%、50% 还是 100%，都无条件将阶段推进到 `MigratingSealingKey`（行 651），这意味着渐进式流量迁移（分多步调用）无法正确工作。

**修复建议**:
仅当 `percentage == 100` 时才转换阶段到 `MigratingSealingKey`，否则停留在 `MigratingTraffic`。

---

#### 🟠 [重大] `complete_upgrade` 未持有 `is_upgrading` 锁的情况下读写多个锁，存在 TOCTOU

**位置**: `src/tee/upgrade.rs:703-748`

**问题描述**:
`complete_upgrade` 先检查 `current_phase`（行 691），随后先后获取 `active_version.write()` 和 `pending_version.write()`（行 704-705）。两次锁获取之间，若任务取消，`pending_version` 将永远处于"已取走但未设置 active"状态，导致不可恢复的状态损坏。

---

#### 🟠 [重大] `std::mem::forget(child)` 造成进程孤儿

**位置**: `src/tee/upgrade.rs:438`

**问题描述**:
子进程 `child` 被 `forget` 后，进程变为孤儿进程，`SIGCHLD` 无人处理。若新 Enclave 启动后立即崩溃，父进程无法感知，会继续推进状态机到 `ParallelRunning`。

**修复建议**:
将 `child` 存入结构体字段，在后续阶段显式等待或终止。

---

#### 🟠 [重大] `check_http_health` 的总超时未被限制

**位置**: `src/tee/upgrade.rs:524-608`

**问题描述**:
TCP 连接、HTTP 写、HTTP 读三个步骤各有独立超时，但没有总体超时。若三个步骤都恰好超时，整个健康检查耗时 `3 * health_check_timeout_secs`（默认 15 秒）。

**修复建议**:
在外层包一个全局 `tokio::time::timeout`。

---

### 三、水印验证 (`src/tee/sandbox/export/watermark.rs`)

#### 🔴 [严重] 签名方案与技术规范完全不符

**位置**: `src/tee/sandbox/export/watermark.rs:152-157`

**问题描述**:
规范明确要求签名嵌入 PNG tEXt chunk（key 为 `"CredBridge-Watermark-Sig"`），实现却将 HMAC 字节**直接追加到 PNG 文件末尾**。标准 PNG 解析器在 IEND chunk 后停止读取，追加字节对下游工具不透明且不可靠。

**风险**:
- 攻击者截断末尾 53 字节即可去除签名
- 该实现对 `watermarked_data`（即已包含视觉水印的完整图片字节）签名，而规范要求对 `watermark_text` 字符串签名

**修复建议**:
使用 `png` crate 将签名嵌入 PNG tEXt chunk，而不是追加到文件末尾。

---

#### 🔴 [严重] 验签对象与签名对象不一致

**问题描述**:
`add_watermark` 对图片像素字节签名，导致攻击者可以修改水印文本内容（会话 ID、Enclave ID）而通过验证，只要像素字节不变，`verify_watermark` 就返回 `Ok(true)`。规范要求对 `watermark_text` 字符串签名。

---

#### 🟠 [重大] `new()` 构造方法使用弱密钥派生

**位置**: `src/tee/sandbox/export/watermark.rs:83-114`

**问题描述**:
`WatermarkService::new(enclave_id)` 调用 `derive_default_hmac_key`，对可预测的 `enclave_id` 做 SHA-256 得到密钥。`enclave_id` 是一个可预测的字符串标识符（如 `"credbridge-enclave-001"`），并非随机秘密材料。

**风险**: 任何知道 `enclave_id` 的人都能推导出 HMAC 密钥，从而伪造任意签名。

**修复建议**:
`new()` 应添加运行时警告或编译期区分标记（`#[cfg(debug_assertions)]`）来防止生产误用。

---

#### 🟠 [重大] `apply_watermark_mock` 回退产物通过了验签

**问题描述**:
回退路径不添加视觉水印，但签名仍被 `add_watermark` 附加，导致 `verify_watermark` 对无水印图片返回 `Ok(true)`，违反验收标准第5条。

---

#### 🟠 [重大] 4个核心验签单元测试全部缺失

**问题描述**:
规范要求实现以下4个测试：
- `test_verify_watermark_valid`
- `test_verify_watermark_no_signature`
- `test_verify_watermark_wrong_key`
- `test_verify_watermark_tampered`

当前仅有 `test_add_watermark`，且该测试只断言返回值非空，未验证 HMAC 签名是否可被 `verify_watermark` 成功验证。

---

### 四、MCP SSE Token 验证与请求分发 (`mcp-server/src/sse.rs`)

#### 🔴 [严重] `msg_rx` 接收端被立即 drop，`ToolHandler` 从未被调用

**位置**: `mcp-server/src/sse.rs:297`

**问题描述**:
```rust
let (msg_tx, _msg_rx) = mpsc::channel(MESSAGE_QUEUE_CAPACITY);
//              ^^^^^^^ 立即 drop，通道发送端随即也会关闭
```

所有 `session.msg_tx.send()` 调用都会返回 `SendError`，`ToolHandler` 从未被调用，客户端永远收不到任何工具响应。这是 P2 技术债务的核心症状。

**修复建议**:
技术规范要求"直接调用 `SseAppState.handler`"而非通过通道。

---

#### 🟠 [重大] session 不存在时返回 `accepted: true`

**位置**: `mcp-server/src/sse.rs:388`

**问题描述**:
规范验收标准明确要求"session 不存在时返回 HTTP 404"，当前行为会让客户端误认为消息已送达，无法感知 session 已过期或不存在，导致静默丢消息。

---

#### 🟠 [重大] `session_token` 是 session_id 的 Base64 编码，不具备认证能力

**问题描述**:
```rust
let session_token = base64_encode(&params.session_id);
```

`message_handler` 当前也完全没有对这个 `session_token` 做任何验证——只接收 `session_id` 查询参数就转发消息。这意味着：知道任意 session_id（可通过枚举或泄露获取）就能向该 session 注入任意 MCP 请求。

---

#### 🟠 [重大] Scope 校验绕过了 `TokenValidator.can_connect_mcp()`

**位置**: `mcp-server/src/sse.rs:289`

**问题描述**:
```rust
// 当前实现
if !claims.has_scope(crate::auth::SCOPE_MCP_CONNECT) {

// 规范要求
if !state.token_validator.can_connect_mcp(&claims) {
```

如果 `TokenValidator` 未来加入额外的 scope 检查逻辑（例如黑名单、rate limit），`sse_handler` 中的直接调用会静默绕过这些策略。

---

### 五、规范与实现偏差汇总

| 规范要求 | 实现状态 |
|---------|---------|
| 签名嵌入 PNG tEXt chunk | 🔴 **未实现** — 改为追加到文件末尾 |
| 对 `watermark_text` 签名 | 🔴 **未实现** — 改为对图片字节签名 |
| 同时存储 `CredBridge-Watermark-Text` chunk | 🔴 **未实现** |
| `apply_watermark_mock` 产物应使验签返回 false | 🔴 **违反** |
| 4个验签单元测试 | 🔴 **全部缺失** |
| `new()` 不应作为生产构造路径 | 🔴 **未区分** |
| `migrate_traffic` 仅 100% 时转换阶段 | 🔴 **违反** — 任意百分比都触发 |
| Keychain 集成 | 🔴 **空实现** |
| SSE 请求分发 | 🔴 **功能断裂** — 通道接收端被 drop |

---

## 修复建议优先级

### P0 - 阻塞合并

1. **Token 存储**: 使用 `ring::rand::SystemRandom` 替换 `rand::thread_rng()`
2. **水印验证**: 重新实现为 PNG tEXt chunk 嵌入方案
3. **MCP SSE**: 修复消息分发路径，确保 `ToolHandler` 被调用
4. **TEE 升级**: 修复 `migrate_traffic` 阶段转换逻辑

### P1 - 合并前必须修复

1. Token 存储 Keychain 集成实现或添加生产环境检查
2. TEE 升级状态重置顺序修复
3. 水印验证添加单元测试
4. SSE session 不存在时返回 HTTP 404

### P2 - 建议修复

1. 所有 `zeroize` 遗漏问题
2. 错误处理和日志记录改进
3. 测试覆盖率补充

---

## 审查元数据

- **审查任务 ID**: `review-adversarial-general`
- **技术规范目录**: `_bmad-output/implementation-artifacts/tech-specs/`
- **并发审查子代理**: 4个
- **审查耗时**: 约 75 秒（并发）

---

*本报告由 BMAD 对抗性审查流程自动生成*
