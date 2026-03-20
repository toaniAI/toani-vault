---
title: 'SSE Token 验证与 MCP 请求分发实现'
slug: 'ts-002-sse-token-validation-and-request-dispatch'
created: '2026-03-19'
status: 'implemented'
priority: 'P1/P2'
---

# Tech-Spec: SSE Token 验证与 MCP 请求分发实现

## 概述

本文档合并处理两个紧密关联的技术债务条目：

- **P1 — SSE Token 验证**（`mcp-server/src/sse.rs:277`）：当前 `sse_handler` 仅做 Base64 格式检查，创建固定的 mock claims，未调用 `TokenValidator.validate()`
- **P2 — MCP 请求处理分发**（`mcp-server/src/sse.rs:364`）：`message_handler` 仅返回 `accepted: true`，未将请求转发给 `ToolHandler`，也未将响应通过 SSE 推送回客户端

两者共享同一个 SSE 架构上下文（`SseAppState`、`SessionManager`），实现时需要协同设计。

### 问题陈述

**问题 1 — Token 验证（P1）**

```rust
// mcp-server/src/sse.rs:277-297
// TODO: 实际验证 Token (当前简化处理，仅做演示)
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
let _ = URL_SAFE_NO_PAD.decode(token).unwrap_or_else(|_| Vec::new());

// 创建简化的 claims (开发环境)
let claims = TokenClaims {
    iss: "credbridge-mcp".to_string(),
    sub: "dev_user".to_string(),
    aud: "mcp-agent".to_string(),
    exp: 9999999999,
    iat: 1000000000,
    jti: uuid::Uuid::new_v4().to_string(),
    sid: params.session_id.clone(),
    scp: vec!["mcp:connect".to_string()],
};
```

任何携带合法 Base64 字符串（甚至空字符串）的请求都能成功建立 SSE 连接，且获得 admin 级别的虚假 claims，完全绕过身份验证。

**问题 2 — 请求分发（P2）**

```rust
// mcp-server/src/sse.rs:347-370
pub async fn message_handler(
    Query(params): Query<MessageQueryParams>,
    State(_state): State<SseAppState>,  // 注意：_state 未使用
    Json(payload): Json<serde_json::Value>,
) -> Json<MessageResponse> {
    // TODO: 处理 MCP 请求并发送到 ToolHandler

    Json(MessageResponse {
        accepted: true,
        message_id,
    })
}
```

请求被接收后立即丢弃，ToolHandler 从未被调用，客户端永远收不到工具调用结果。

### 解决方案

**Token 验证**：在 `sse_handler` 中调用 `TokenValidator::validate(token)` 替换 mock claims，并验证连接所需的 `mcp:connect` scope。`TokenValidator` 已在 `mcp-server/src/auth.rs` 中完整实现。

**请求分发**：在 `message_handler` 中解析 MCP JSON-RPC 载荷，通过 `ToolHandler` 处理，将结果包装为 `SseMessage::Message` 推送到对应 session 的 SSE 通道。

### 范围

- **变更文件**：`mcp-server/src/sse.rs`、`mcp-server/src/main.rs`（SSE 服务器初始化）
- **不涉及**：工具层逻辑、Vault 存储、加密模块

---

## 开发上下文

### 当前代码（关键片段）

**SSE 应用状态** — `mcp-server/src/sse.rs:241-248`

```rust
#[derive(Clone)]
pub struct SseAppState {
    pub sessions: Arc<SessionManager>,
    pub handler: ToolHandler,
    // 缺少：token_validator 字段
}
```

**TokenValidator 接口** — `mcp-server/src/auth.rs:155-204`

```rust
pub struct TokenValidator {
    config: Arc<TokenConfig>,
}

impl TokenValidator {
    pub fn new(config: TokenConfig) -> Self { ... }

    /// 验证 Token 并提取 Claims
    /// 当前实现：Base64+JSON 编码（开发用）
    /// 生产环境应使用完整的 PASETO V4.Local 验证
    pub fn validate(&self, token: &str) -> Result<TokenClaims, TokenError> {
        // 1. Base64 解码
        // 2. 解析 JSON Claims
        // 3. 验证 iss (allowed_issuers)
        // 4. 验证 aud (allowed_audiences)
        // 5. 验证 exp (is_expired)
        Ok(claims)
    }

    pub fn can_connect_mcp(&self, claims: &TokenClaims) -> bool {
        self.validate_scope(claims, SCOPE_MCP_CONNECT)
    }
}
```

**TokenConfig 默认值** — `mcp-server/src/auth.rs:62-80`

```rust
impl Default for TokenConfig {
    fn default() -> Self {
        // 开发环境默认配置（顺序密钥，非随机）
        // allowed_issuers: ["credbridge-mcp"]
        // allowed_audiences: ["mcp-agent"]
        // default_expiry: Duration::from_secs(3600)
    }
}
```

**SessionState 结构** — `mcp-server/src/sse.rs:83-97`

```rust
pub struct SessionState {
    pub id: String,
    pub claims: TokenClaims,
    pub tx: mpsc::Sender<SseMessage>,      // SSE 推送通道
    pub msg_tx: mpsc::Sender<McpRequest>,  // 消息接收通道（当前未消费）
    pub last_activity: Instant,
    pub sse_connected: bool,
}
```

**SseError 错误类型** — `mcp-server/src/sse.rs:196-222`

```rust
pub enum SseError {
    AuthFailed(String),   // → HTTP 401
    TokenExpired,         // → HTTP 401
    SessionNotFound(String),
    // ...
}
```

**ToolHandler 调用示例** — `mcp-server/src/handlers.rs:260-300`

```rust
// ToolHandler 实现了 rmcp Server trait
// call_tool 是其核心分发入口
match tool_name.as_str() {
    "list_credentials" => handle_list_credentials(&self.tools, arguments).await,
    "decrypt_credential" => handle_decrypt_credential(&self.tools, arguments).await,
    // ...
}
```

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `mcp-server/src/sse.rs` | 主要变更文件 |
| `mcp-server/src/auth.rs` | `TokenValidator`、`TokenClaims`、`TokenError` 接口 |
| `mcp-server/src/handlers.rs` | `ToolHandler` 的工具分发实现 |
| `mcp-server/src/main.rs` | SSE 服务器初始化，需注入 `TokenValidator` |

### 技术决策

1. **`TokenValidator` 注入 `SseAppState`**：在 `SseAppState` 中添加 `token_validator: Arc<TokenValidator>` 字段，在 `run_sse_server` 中初始化并传入。

2. **`TokenError` 到 `SseError` 的映射**：
   - `TokenError::TokenExpired` → `SseError::TokenExpired`
   - `TokenError::InvalidIssuer` / `InvalidAudience` / `InvalidToken` → `SseError::AuthFailed(msg)`

3. **请求分发架构**：`message_handler` 中直接调用 `SseAppState.handler`（`ToolHandler` 已在状态中），而不是通过 `msg_tx` 通道（通道方式引入不必要的异步复杂性）。流程：
   - 解析 session → 验证 session 存在 → 解析 MCP JSON-RPC → 调用 ToolHandler → 序列化结果 → 通过 `session.tx` 推送 `SseMessage::Message`

4. **MCP JSON-RPC 格式**：标准 MCP 消息格式为：
   ```json
   { "jsonrpc": "2.0", "id": "req-1", "method": "tools/call", "params": { "name": "...", "arguments": {...} } }
   ```
   响应格式：
   ```json
   { "jsonrpc": "2.0", "id": "req-1", "result": { ... } }
   ```

5. **`_state` 参数未使用的问题**：`message_handler` 中的 `State(_state)` 需改为 `State(state)`，启用状态访问。

---

## 实现计划

### 任务

按依赖顺序：

**任务 1：扩展 `SseAppState` 添加 `token_validator` 字段**

修改 `mcp-server/src/sse.rs`：

```rust
#[derive(Clone)]
pub struct SseAppState {
    pub sessions: Arc<SessionManager>,
    pub handler: ToolHandler,
    pub token_validator: Arc<TokenValidator>,  // 新增
}
```

修改 `mcp-server/src/main.rs` 中的 `run_sse_server`，使用 `TokenConfig::default()` 初始化 `TokenValidator` 并注入。

**任务 2：实现真实 Token 验证（替换 P1 占位符）**

在 `sse_handler` 中（`mcp-server/src/sse.rs:277` 附近）替换为：

```rust
// 调用 TokenValidator 真实验证
let claims = state.token_validator
    .validate(token)
    .map_err(|e| match e {
        TokenError::TokenExpired => SseError::TokenExpired,
        e => SseError::AuthFailed(e.to_string()),
    })?;

// 验证 mcp:connect scope
if !state.token_validator.can_connect_mcp(&claims) {
    return Err(SseError::AuthFailed(
        "Missing required scope: mcp:connect".to_string()
    ));
}
```

**任务 3：实现 MCP 请求分发（替换 P2 占位符）**

重写 `message_handler`（`mcp-server/src/sse.rs:347-370`）：

1. 从 `State(state)` 获取应用状态（移除 `_` 前缀）
2. 通过 `state.sessions.get_session(&params.session_id)` 获取 session
3. 解析 `payload` 为 MCP JSON-RPC 请求，提取 `method` 和 `params`
4. 根据 `method` 类型路由：
   - `tools/call` → 调用 `state.handler`（需要确认 `ToolHandler` 的调用接口）
   - `tools/list` → 返回工具列表
   - 其他 → 返回 JSON-RPC 错误响应
5. 将结果包装为 `SseMessage::Message { data: response_json }` 通过 `session.tx.send()` 推送
6. 更新 session 的 `last_activity` 时间戳（防止被清理）

**任务 4：添加 `SseError::Unauthorized` 变体（可选优化）**

若当前的 `SseError::AuthFailed` 语义不够清晰，可添加 `Unauthorized` 变体并映射到 HTTP 403，区分「未认证」和「无权限」。

### 验收标准

**Token 验证（P1）**：
- [ ] 不携带 Authorization header 的 SSE 请求返回 HTTP 401
- [ ] 携带无效 token（非法 Base64、错误 issuer、过期）的请求返回 HTTP 401
- [ ] 携带有效但缺少 `mcp:connect` scope 的 token 返回 HTTP 401
- [ ] 携带有效 token 的请求成功建立 SSE 连接，session 中的 claims 为真实解析结果
- [ ] dev_user / 固定过期时间 mock claims 不再出现

**请求分发（P2）**：
- [ ] 通过 SSE 连接的客户端发送 `tools/list` 请求，能通过 SSE 收到工具列表响应
- [ ] 发送 `tools/call` 请求，能收到工具执行结果（或错误）
- [ ] session 不存在时返回 HTTP 404
- [ ] 消息格式错误时返回 JSON-RPC 格式的错误响应（`-32700 Parse error`）
- [ ] `cargo test` 全部通过

---

## 附加上下文

### 依赖

- `mcp-server/src/auth.rs`：`TokenValidator`、`TokenClaims`、`TokenError`（已存在，无需新增）
- `mcp-server/src/handlers.rs`：`ToolHandler`（已存在）
- `rmcp` crate：MCP 协议 JSON-RPC 格式（需确认 request/response 结构）

### 测试策略

1. **单元测试** — Token 验证路径：
   - 使用 `TokenValidator` 生成合法 token，模拟 `sse_handler` 调用，验证 claims 正确提取
   - 使用过期/错误 issuer token，验证返回 `SseError::AuthFailed`

2. **集成测试** — 端到端 SSE 流程：
   - 启动测试 SSE 服务器
   - 客户端连接 → 发送 `tools/list` → 验证通过 SSE 收到响应
   - 负面测试：无 token 连接 → 验证 401 响应

3. **现有测试兼容**：
   - `mcp-server/src/auth.rs` 中的 `create_test_validator()` 可直接复用于测试场景

### 注意事项

- **Token 格式迁移**：当前 `TokenValidator::validate` 使用 Base64+JSON 格式（开发用），文档注释中明确标注「生产环境应使用完整 PASETO V4.Local 验证」。本次实现不改变 token 格式，只是将验证从「不验证」升级为「调用 validate()」。PASETO 升级作为单独任务处理。
- **`ToolHandler` 的 `call_tool` 需要 `RequestContext`**：`handlers.rs` 中 `call_tool` 签名接受 `RequestContext<RoleServer>`，在 SSE message handler 中调用时需要构造合适的上下文（或提取内部 handler 函数直接调用）。实现前需确认最简调用路径。
- **背压处理**：`session.tx.send()` 若通道满（`MESSAGE_QUEUE_CAPACITY = 100`），应返回 `SseError::QueueFull` 而非静默丢弃。
- **并发安全**：`SessionManager` 使用 `RwLock`，`get_session` 返回 `Arc<SessionState>`，`tx.send()` 是 clone 安全的，无需额外同步。
