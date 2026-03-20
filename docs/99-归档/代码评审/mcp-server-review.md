# MCP Server 代码审查报告

**审查日期**: 2026-03-19
**审查范围**: mcp-server/src/{lib.rs, main.rs, sse.rs, tools.rs}, mcp-server/tests/mcp_sse_api_test.rs
**对应 Tech-Spec**: ts-001-mcp-tee-decrypt, ts-002-sse-token-validation-and-request-dispatch
**审查类型**: 对抗性代码审查（Adversarial Code Review）

---

## 执行摘要

| Tech-Spec | 标题 | 实现状态 |
|-----------|------|---------|
| ts-001 | MCP TEE 解密操作实现 | **部分实现** — 核心逻辑完成，但存在数个安全合规缺口 |
| ts-002 | SSE Token 验证与 MCP 请求分发 | **部分实现** — Token 验证已完成，请求分发仍为占位符 |

**发现问题总数**: 12 个
**CRITICAL**: 1 | **HIGH**: 4 | **MEDIUM**: 4 | **LOW**: 3

---

## ts-001: MCP TEE 解密操作实现

### 实现状态: 部分实现

#### 已实现项目

| 验收标准 | 状态 | 说明 |
|---------|------|------|
| `McpServerState` 添加 `key_hierarchy` 字段 | 已实现 | `lib.rs:79` — `key_hierarchy: tokio::sync::RwLock<KeyHierarchy>` |
| `vault.get_credential()` 获取加密载荷 | 已实现 | `tools.rs:173-176` |
| 构建 `EncryptedBlob` | 已实现 | `tools.rs:179-187` |
| 派生 L2/L3 密钥 | 已实现 | `tools.rs:190-198` |
| 调用 `crypto_decrypt` | 已实现 | `tools.rs:204-205` |
| 权限拒绝时返回 `PermissionDenied` | 已实现 | `tools.rs:149-165` |
| 凭证不存在时返回 `NotFound` | 已实现 | `tools.rs:176` |
| 审计日志记录解密尝试 | 已实现 | `tools.rs:216-227` |

#### 发现问题

---

**[CRITICAL] ISSUE-01: 解密后明文未执行 zeroize 清零**

- **文件**: `mcp-server/src/tools.rs`
- **行号**: 204-213
- **描述**: `crypto_decrypt` 返回的 `plaintext_bytes: Vec<u8>` 包含凭证明文，在使用后未调用 `zeroize()` 清除内存。该向量在函数返回后依然滞留在堆内存中，存在内存转储攻击（memory dump）风险。这在凭证管理系统中是严重的安全缺陷，违反了 Rust Coding Standards 中"使用 `zeroize` 清除敏感数据"的强制要求。

```rust
// tools.rs:204-213 — 问题代码
let plaintext_bytes = crypto_decrypt(&l3_key, &blob, Some(aad.as_bytes()))
    .map_err(|e| ToolError::VaultError(format!("Decryption failed: {:?}", e)))?;

let decrypted_data: serde_json::Value = serde_json::from_slice(&plaintext_bytes)
    .unwrap_or_else(|_| {
        String::from_utf8_lossy(&plaintext_bytes)
            .to_string()
            .into()
    });
// plaintext_bytes 在此处 drop，但堆内存未清零
```

- **修复建议**: 使用 `zeroize::Zeroizing<Vec<u8>>` 包裹解密结果，或在 `from_slice` 调用后显式调用 `plaintext_bytes.zeroize()`。同理，L3 密钥 `l3_key` 也应在使用后清零。

---

**[HIGH] ISSUE-02: `tee_verified: true` 硬编码为真，存在安全误导**

- **文件**: `mcp-server/src/tools.rs`
- **行号**: 229-234
- **描述**: `DecryptCredentialResponse` 中的 `tee_verified` 字段被硬编码为 `true`（`tools.rs:233`），但当前 mcp-server 运行在软件模拟模式而非真实 TEE 环境中。这会给 AI 客户端（以及最终用户）传达错误的安全保证，使其误认为解密操作发生在受硬件保护的 TEE 中。

ts-001 验收标准明确要求："`tee_verified` 应如实反映状态，避免给 AI 客户端错误的安全感知"（ts-001.md: 附加上下文/注意事项）。

- **修复建议**: 引入运行时 TEE 状态检查，或将软件模式明确标记为 `tee_verified: false`，并在响应中添加 `mode: "software_simulation"` 字段。

---

**[HIGH] ISSUE-03: `mrenclave_placeholder` 占位符未替换**

- **文件**: `mcp-server/src/tools.rs`
- **行号**: 57, 119, 157, 221, 281, 340, 387, 432, 471, 496, 501
- **描述**: 全文件中大量使用字符串字面量 `"mrenclave_placeholder"` 作为审计日志的 mrenclave 参数。ts-001 任务 4 明确要求："将 `mrenclave_placeholder` 替换为从 TEE 状态获取的真实 mrenclave 值（或软件模拟标识 `software_mode`）"。此问题导致审计日志中的 enclave 身份信息完全不可信，审计记录失去完整性。

- **修复建议**: 定义常量 `const SOFTWARE_MODE_MRENCLAVE: &str = "software_mode_not_in_tee"` 并全局替换，或在 `McpServerState` 中添加 `tee_mrenclave: String` 字段并在初始化时确定。

---

**[HIGH] ISSUE-04: `unwrap_or_else` 在解密结果解析中使用，吞噬解析错误**

- **文件**: `mcp-server/src/tools.rs`
- **行号**: 208-212
- **描述**: 当 `serde_json::from_slice` 失败时，代码使用 `unwrap_or_else` 静默回退到 UTF-8 字符串转换，不返回任何错误。这意味着：（1）加密数据损坏或密钥错误（部分解密产生非 JSON 字节）时，调用方会收到乱码字符串而非错误；（2）掩盖了潜在的解密失败。

```rust
// tools.rs:208-213 — 问题代码
let decrypted_data: serde_json::Value = serde_json::from_slice(&plaintext_bytes)
    .unwrap_or_else(|_| {
        String::from_utf8_lossy(&plaintext_bytes)
            .to_string()
            .into()  // 静默回退，吞噬错误
    });
```

- **修复建议**: 区分两种合法场景（存储为 JSON 的凭证 vs 存储为原始字符串的凭证），并根据存储格式选择正确的解析路径，或对 JSON 解析失败返回明确错误。

---

**[MEDIUM] ISSUE-05: L3 密钥未 zeroize 清零**

- **文件**: `mcp-server/src/tools.rs`
- **行号**: 190-198
- **描述**: `l3_key` 是从密钥层次派生的 L3 凭证加密密钥，包含敏感密钥材料，在 `crypto_decrypt` 调用后未清零。与 ISSUE-01 同属内存安全问题，但密钥材料比明文更为敏感（泄露密钥意味着所有用该密钥加密的凭证均可被解密）。

- **修复建议**: 确认 `derive_credential_key` 返回类型是否实现 `ZeroizeOnDrop`；若未实现，使用 `zeroize::Zeroizing` 包裹。

---

**[MEDIUM] ISSUE-06: `get_tee_status` 全部返回 mock 占位符**

- **文件**: `mcp-server/src/tools.rs`
- **行号**: 238-246
- **描述**: `get_tee_status` 方法返回硬编码的占位符值：`mrenclave: "mrenclave_placeholder"`, `mrsigner: "mrsigner_placeholder"`, `quote_valid: true`。该方法是 ts-001 验收范围的组成部分，返回虚假数据会导致依赖此工具判断 TEE 状态的客户端做出错误决策。

- **修复建议**: 与 ISSUE-02 联动修复，实现真实的 TEE 状态查询或明确标注为软件模拟模式。

---

## ts-002: SSE Token 验证与 MCP 请求分发

### 实现状态: 部分实现

#### 已实现项目

| 验收标准 | 状态 | 说明 |
|---------|------|------|
| `SseAppState` 添加 `token_validator` 字段 | 已实现 | `sse.rs:249` |
| `main.rs` 注入 `TokenValidator` | 已实现 | `main.rs:124-131` |
| SSE handler 调用 `TokenValidator.validate()` | 已实现 | `sse.rs:280-286` |
| 验证 `mcp:connect` scope | 已实现 | `sse.rs:289-293` |
| 无 Authorization header 返回 401 | 已实现 | `sse.rs:271-273` |
| 无效 token 格式返回 401 | 已实现 | `sse.rs:275-277` |
| dev_user / mock claims 不再出现 | 已实现 | 旧的占位符已移除 |

#### 发现问题

---

**[HIGH] ISSUE-07: MCP 请求分发未实现（P2 占位符仍存在）**

- **文件**: `mcp-server/src/sse.rs`
- **行号**: 347-392
- **描述**: ts-002 的核心需求之一 — MCP 请求分发 — 仍未实现。`message_handler` 确实从占位符升级为将请求放入 `msg_tx` 通道（`sse.rs:375`），但该通道的消费端 `_msg_rx` 在 `sse_handler` 中被丢弃（`sse.rs:297`），没有任何代码读取并转发给 `ToolHandler`，也没有将响应推送回 SSE 流。

```rust
// sse.rs:297 — 通道接收端被丢弃
let (msg_tx, _msg_rx) = mpsc::channel(MESSAGE_QUEUE_CAPACITY);
//                       ^^^^^^^ 丢弃！消息入通道即消失
```

ts-002 验收标准明确要求：
- "通过 SSE 连接的客户端发送 `tools/list` 请求，能通过 SSE 收到工具列表响应"
- "发送 `tools/call` 请求，能收到工具执行结果"

这些验收标准均**未满足**。整个 MCP 工具调用链路依然中断：客户端发送请求 → 请求消失在通道中 → 客户端永远收不到响应。

- **修复建议**: 实现一个后台任务消费 `msg_rx`，调用 `SseAppState.handler` 处理请求，并通过 `session.tx.send(SseMessage::Message { data: response })` 将结果推送回客户端。参考 ts-002 任务 3 的设计方案。

---

**[MEDIUM] ISSUE-08: 过期 Token 测试存在注释性免责，测试逻辑不完整**

- **文件**: `mcp-server/tests/mcp_sse_api_test.rs`
- **行号**: 159-171
- **描述**: `test_sse_connection_expired_token` 测试函数包含注释："SSE 处理程序目前使用简化验证 (见 sse.rs:278-281)"，暗示 SSE handler 对过期 token 的处理是不完整的。然而 sse.rs:280 实际上已调用 `state.token_validator.validate(token)`，`TokenValidator` 的 `validate` 方法也确实检查 `is_expired()`。

该测试**没有**通过 HTTP 层面验证过期 token 被 SSE handler 拒绝（返回 401），而仅验证了 `TokenValidator` 单独使用时能检测过期，这是不完整的集成测试。注释本身表明开发者对当前行为存在误解，可能掩盖了真实的测试遗漏。

- **修复建议**: 将测试改为发送包含过期 token 的 SSE 连接请求，断言响应 HTTP 状态码为 401。

---

**[MEDIUM] ISSUE-09: `message_handler` 未验证请求方 Session 的 Token 合法性**

- **文件**: `mcp-server/src/sse.rs`
- **行号**: 348-392
- **描述**: `message_handler` 接受来自任意请求者的 POST 消息，只需提供有效的 `session_id` 查询参数，无需携带任何认证凭证。这意味着：如果攻击者猜测或获知某个活跃 session_id，可以向该 session 发送伪造的 MCP 请求，假冒该 session 的合法用户执行工具调用。

ts-002 spec 中未明确要求对 `/message` 端点鉴权，但从安全角度这是必要的（session_id 本身不应作为鉴权凭据）。

- **修复建议**: 在 `message_handler` 中同样验证 Bearer Token，或验证请求方的 token claims 中的 `sid` 与 URL 中的 `session_id` 一致。

---

**[MEDIUM] ISSUE-10: `TokenConfig::default()` 使用固定顺序字节密钥，非安全随机**

- **文件**: `mcp-server/src/auth.rs`
- **行号**: 65-69
- **描述**: 默认配置使用 `key[i] = i as u8` 生成密钥，即 `[0, 1, 2, ..., 31]`。尽管注释标注"生产环境应从安全配置加载"，但：（1）`main.rs:124` 直接使用 `TokenConfig::default()` 初始化生产 SSE 服务器；（2）当前 Token 格式（Base64+JSON，无 HMAC）与密钥实际上无关，导致该密钥字段名存实亡，形成对安全性的虚假印象。

- **修复建议**: 在 `run_sse_server` 中检查环境变量 `CREDBRIDGE_MCP_TOKEN_KEY`，若未设置则在非开发模式下拒绝启动，而非静默使用 default 配置。

---

**[LOW] ISSUE-11: `validate_scope` 使用字符串包含匹配，存在逻辑错误风险**

- **文件**: `mcp-server/src/tools.rs`
- **行号**: 249-257
- **描述**: `validate_scope` 方法使用 `provided_scope.contains(required_scope)` 进行权限验证。若 `required_scope = "credential:read"`，而 `provided_scope = "credential:read-only"` 或 `"credential:read credential:write"`（空格分隔的字符串），均会通过验证。虽然当前调用方均传入精确字符串，但这个方法的实现本身脆弱。

- **修复建议**: 改用 scope 列表的精确匹配，或与 `TokenClaims::has_scope` 对齐（后者使用 `iter().any(|s| s == required)`）。

---

**[LOW] ISSUE-12: `HealthResponse::uptime_seconds` 永远返回 0**

- **文件**: `mcp-server/src/sse.rs`
- **行号**: 396-403
- **描述**: `health_handler` 硬编码 `uptime_seconds: 0`，该字段在监控/运维场景下毫无价值，但不影响安全性。

- **修复建议**: 在 `SseAppState` 中记录启动时间戳，计算实际运行时长。

---

**[LOW] ISSUE-13: 测试中的 `generate_expired_token` 不通过 `TokenValidator` 签发**

- **文件**: `mcp-server/tests/mcp_sse_api_test.rs`
- **行号**: 50-63
- **描述**: `generate_expired_token` 直接构造 `TokenClaims` 并 Base64 编码，绕过 `TokenValidator` 的签发流程。这在当前 Base64+JSON token 格式下有效，但一旦 token 格式迁移到 PASETO（需要签名），该测试辅助函数将立即失效，且失效方式是静默通过（生成无效 token 但不报错）。

- **修复建议**: 使用 `validator.generate_token(&expired_claims)` 替代手动 Base64 编码，保持测试与实现的一致性。

---

## 综合代码质量评估

### 错误处理

- 整体使用 `thiserror` + `Result` 返回，符合规范。
- `tools.rs:61` 和其他多处使用 `let _ = self.state.audit.record(audit_entry)` 静默忽略审计记录错误，在凭证管理系统中审计日志失败应至少记录警告日志。
- `sse.rs:375` 当消息发送到 `msg_tx` 失败时仅打印 `warn!` 日志，由于 `_msg_rx` 已丢弃（ISSUE-07），此警告在当前实现中将频繁触发但被忽视。

### 安全性

- 核心安全缺陷：ISSUE-01（明文未清零）是 CRITICAL 级别问题，在生产 TEE 环境中可能被内存转储攻击利用。
- `tee_verified: true` 硬编码（ISSUE-02）构成安全误导，不符合零信任原则。
- `message_handler` 缺乏鉴权（ISSUE-09）在请求分发功能上线后将成为越权攻击入口。

### Tech-Spec 符合度

| 功能点 | ts-001 要求 | 实现状态 |
|-------|-------------|---------|
| 占位符 `[REDACTED]` 替换为真实解密 | 是 | 已完成 |
| `McpServerState.key_hierarchy` 字段 | 是 | 已完成 |
| `vault.get_credential()` 获取加密载荷 | 是 | 已完成 |
| 正确派生 L2/L3 密钥 | 是 | 已完成 |
| 调用 `decrypt_credential` | 是 | 已完成 |
| `tee_verified` 真实反映状态 | 是 | **未完成** (硬编码 true) |
| `mrenclave` 真实值或软件标识 | 是 | **未完成** (仍为 placeholder) |
| 明文 `zeroize` | 是（隐含于 Coding Standards） | **未完成** |

| 功能点 | ts-002 要求 | 实现状态 |
|-------|-------------|---------|
| `SseAppState.token_validator` 字段 | 是 | 已完成 |
| SSE handler 调用 `validate()` | 是 | 已完成 |
| 验证 `mcp:connect` scope | 是 | 已完成 |
| mock claims 移除 | 是 | 已完成 |
| MCP 请求分发到 `ToolHandler` | 是 | **未完成** |
| SSE 推送响应回客户端 | 是 | **未完成** |

---

## 必须修复项（阻塞合并）

以下问题需在合并前解决：

1. **ISSUE-01 (CRITICAL)**: 为 `plaintext_bytes` 添加 `zeroize` 调用
2. **ISSUE-07 (HIGH)**: 实现 MCP 请求分发 — 这是 ts-002 的核心功能，当前完全缺失
3. **ISSUE-02 (HIGH)**: 修正 `tee_verified` 字段，软件模式应返回 `false`
4. **ISSUE-03 (HIGH)**: 将 `mrenclave_placeholder` 替换为明确的软件模式标识

## 建议修复项（不阻塞合并）

5. **ISSUE-04 (HIGH)**: 修正 JSON 解析失败的错误处理逻辑
6. **ISSUE-05 (MEDIUM)**: L3 密钥 zeroize
7. **ISSUE-08 (MEDIUM)**: 修复过期 Token 集成测试
8. **ISSUE-09 (MEDIUM)**: `message_handler` 添加鉴权
9. **ISSUE-10 (MEDIUM)**: 生产环境 token 密钥来源检查

---

*报告生成者: adversarial-code-reviewer*
*基于代码快照: 2026-03-19*
