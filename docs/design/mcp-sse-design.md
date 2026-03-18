# MCP Server SSE Transport 设计文档

> **文档状态**: Draft
> **版本**: 1.0
> **作者**: CredBridge Team
> **创建日期**: 2026-03-12
> **关联 Epic**: EP6 Story 6.1 - MCP Server 基础架构

---

## 1. 架构概述

### 1.1 设计目标

本设计实现 MCP Server 的 SSE (Server-Sent Events) Transport，支持与 OpenClaw Agent 的远程连接。SSE Transport 提供以下能力：

- **单向实时推送**: Server → Client 的实时事件流
- **双向通信**: SSE 建立连接 + HTTP POST 接收消息
- **安全认证**: Bearer Token 认证机制
- **可靠连接**: 心跳保活 + 自动重连

### 1.2 架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                    OpenClaw Agent (Client)                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ SSE Client  │  │ HTTP Client │  │    Message Queue        │  │
│  │  /sse       │  │  /message   │  │    (Pending Messages)   │  │
│  └──────┬──────┘  └──────┬──────┘  └─────────────────────────┘  │
└─────────┼────────────────┼──────────────────────────────────────┘
          │ GET /sse       │ POST /message
          │ EventStream    │ Bearer Token
          │                │
┌─────────┼────────────────┼──────────────────────────────────────┐
│         ▼                ▼                                      │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              Axum HTTP Server (SSE Mode)                  │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐   │   │
│  │  │ Auth        │  │ SSE         │  │ Message         │   │   │
│  │  │ Middleware  │  │ Handler     │  │ Handler         │   │   │
│  │  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘   │   │
│  │         │                │                   │            │   │
│  │         ▼                ▼                   ▼            │   │
│  │  ┌─────────────────────────────────────────────────────┐  │   │
│  │  │              Session Manager                         │  │   │
│  │  │         (Session State + Token Registry)             │  │   │
│  │  └─────────────────────────────────────────────────────┘  │   │
│  │                              │                              │   │
│  │                              ▼                              │   │
│  │  ┌─────────────────────────────────────────────────────┐  │   │
│  │  │           Message Queue (Per-Session)                │  │   │
│  │  │         (tokio::sync::mpsc channels)                 │  │   │
│  │  └─────────────────────────────────────────────────────┘  │   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │         MCP Tool Handler (CredBridgeTools)                   ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│         CredBridge Services (Vault, Audit, TEE)                 │
└─────────────────────────────────────────────────────────────────┘
```

### 1.3 组件说明

| 组件 | 职责 |
|------|------|
| **Auth Middleware** | 验证 Bearer Token，检查 `mcp:connect` scope |
| **SSE Handler** | 建立 SSE 连接，发送 endpoint 事件，推送 MCP 响应 |
| **Message Handler** | 接收客户端 MCP 请求，验证并转发到 Session |
| **Session Manager** | 管理活动会话状态，Token 注册表 |
| **Message Queue** | 每个会话的待发送消息队列 |

---

## 2. SSE 协议实现

### 2.1 协议概述

MCP over SSE 遵循标准 SSE 协议 (EventSource)：

```
GET /sse?session_id={session_id}

HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive
X-Accel-Buffering: no

event: endpoint
data: {"endpoint":"/message","token":"<bearer_token>"}

event: message
data: {"jsonrpc":"2.0","id":"1","result":{...}}
```

### 2.2 心跳机制

```rust
/// 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// 心跳事件 (SSE comment)
/// SSE 规范要求定期发送 comment 保持连接活跃
:ping
```

### 2.3 重连机制

```rust
/// 客户端重连配置
/// SSE 客户端应配置:
/// - reconnectionTime: 3000  // 3 秒后重试
/// - maxReconnectionAttempts: 10
```

Server 端通过 `Last-Event-ID` 支持断点续传（可选）。

---

## 3. 端点设计

### 3.1 GET /sse - SSE 连接端点

**请求:**
```http
GET /sse?session_id={session_id}
Authorization: Bearer {initial_token}
```

**查询参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `session_id` | UUID | 是 | 会话标识符 |

**响应:**
```http
HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive
X-Accel-Buffering: no
```

**事件流:**

```
# 1. 连接建立后首先发送 endpoint 事件
event: endpoint
data: {"endpoint":"/message","token":"<session_bearer_token>"}

# 2. 后续 MCP 响应/通知
event: message
data: {"jsonrpc":"2.0","id":"1","result":{...}}

# 3. 心跳 (每 30 秒)
:ping

# 4. 错误关闭
event: error
data: {"error":"Session expired"}
```

**错误响应:**

| 状态码 | 场景 |
|--------|------|
| `400 Bad Request` | 缺少 `session_id` 参数 |
| `401 Unauthorized` | Token 无效或过期 |
| `409 Conflict` | `session_id` 已存在活跃连接 |

### 3.2 POST /message - 消息接收端点

**请求:**
```http
POST /message?session_id={session_id}
Content-Type: application/json
Authorization: Bearer {session_token}

{
  "jsonrpc": "2.0",
  "id": "1",
  "method": "tools/call",
  "params": {...}
}
```

**响应:**
```http
HTTP/1.1 202 Accepted
Content-Type: application/json

{
  "accepted": true,
  "message_id": "msg_xxx"
}
```

**错误响应:**

| 状态码 | 场景 |
|--------|------|
| `400 Bad Request` | 无效 JSON，缺少必填字段 |
| `401 Unauthorized` | Token 无效或 session 不匹配 |
| `404 Not Found` | Session 不存在 |
| `503 Service Unavailable` | 消息队列已满 |

### 3.3 GET /health - 健康检查端点

```http
GET /health

HTTP/1.1 200 OK
Content-Type: application/json

{
  "status": "healthy",
  "active_sessions": 42,
  "uptime_seconds": 3600
}
```

---

## 4. 认证机制

### 4.1 Token 格式

使用 **PASETO** (Platform-Agnostic Security Token) 作为 Token 格式：

```rust
// Token Payload 结构
struct McpTokenClaims {
    /// 发行者
    pub iss: String,          // "credbridge-mcp"
    /// 主题 (用户 ID)
    pub sub: String,          // user_id
    /// 受众
    pub aud: String,          // "mcp-agent"
    /// 过期时间
    pub exp: u64,             // Unix timestamp
    /// 签发时间
    pub iat: u64,             // Unix timestamp
    /// JWT ID (唯一标识)
    pub jti: String,          // UUID v4
    /// Session ID
    pub sid: String,          // session_id
    /// Scope 列表
    pub scp: Vec<String>,     // ["mcp:connect", "credential:read"]
}
```

### 4.2 Token 验证中间件

```rust
/// 认证中间件
pub struct McpAuthMiddleware<S> {
    inner: S,
    token_validator: Arc<TokenValidator>,
}

/// Token 验证器
pub struct TokenValidator {
    key: PasetoKey,
    allowed_issuers: Vec<String>,
}

impl TokenValidator {
    pub fn validate(&self, token: &str) -> Result<TokenClaims, AuthError> {
        // 1. 验证 PASETO 签名
        // 2. 验证过期时间
        // 3. 验证发行者
        // 4. 验证 audience
        // 5. 提取 claims
    }

    pub fn validate_scope(&self, claims: &TokenClaims, required: &str) -> bool {
        claims.scp.iter().any(|s| s == required || s == "admin")
    }
}
```

### 4.3 认证流程

```
1. Client 获取初始 Token (外部系统提供)
2. Client 发起 SSE 连接: GET /sse?session_id=xxx with Bearer Token
3. Server 验证 Token:
   - 签名有效性
   - 未过期
   - 包含 mcp:connect scope
4. Server 创建 Session，生成 Session-specific Token
5. Server 通过 SSE 发送 endpoint + Session Token
6. 后续 /message 请求使用 Session Token
```

### 4.4 Token 过期处理

```rust
/// Token 过期响应
pub struct TokenExpiredResponse {
    pub error: String,        // "token_expired"
    pub expired_at: String,   // RFC3339 timestamp
    pub renew_endpoint: String, // "/auth/renew"
}
```

---

## 5. 消息队列设计

### 5.1 队列架构

每个 Session 维护独立的 mpsc 队列：

```rust
pub struct SessionState {
    /// Session ID
    pub id: String,
    /// Token Claims
    pub claims: TokenClaims,
    /// 待发送消息队列 (Server → Client)
    pub tx: mpsc::Sender<SseMessage>,
    /// 接收客户端消息队列 (Client → Server)
    pub msg_tx: mpsc::Sender<McpRequest>,
    /// 最后活跃时间
    pub last_activity: Instant,
}
```

### 5.2 消息确认机制

```rust
/// MCP 消息确认
pub struct MessageAck {
    pub message_id: String,
    pub session_id: String,
    pub status: AckStatus,
    pub timestamp: u64,
}

pub enum AckStatus {
    Received,    // 消息已接收
    Processing,  // 处理中
    Completed,   // 处理完成
    Failed { error: String },
}
```

### 5.3 消息持久化 (可选)

对于生产环境，可选持久化：

```rust
/// 消息持久化接口
pub trait MessagePersistence: Send + Sync {
    async fn store(&self, session_id: &str, msg: &McpMessage) -> Result<()>;
    async fn load_pending(&self, session_id: &str) -> Result<Vec<McpMessage>>;
    async fn mark_sent(&self, message_id: &str) -> Result<()>;
}

/// 内存实现 (默认)
pub struct InMemoryPersistence { ... }

/// Redis 实现 (生产环境)
pub struct RedisPersistence { ... }
```

---

## 6. 时序图

### 6.1 SSE 连接建立流程

```
Client                          Server
  │                               │
  │  GET /sse?session_id=xxx      │
  │  Authorization: Bearer token  │
  │ ─────────────────────────────>│
  │                               │ 验证 Token
  │                               │ 创建 Session
  │                               │
  │  HTTP 200 OK (text/event-stream)
  │  event: endpoint              │
  │  data: {endpoint, token}      │
  │ <─────────────────────────────│
  │                               │
  │  event: message               │
  │  data: {jsonrpc response}     │
  │ <─────────────────────────────│
  │                               │
  │  :ping (heartbeat)            │
  │ <─────────────────────────────│
```

### 6.2 消息发送流程

```
Client                          Server
  │                               │
  │  POST /message?session_id=xxx │
  │  Authorization: Bearer token  │
  │  {jsonrpc request}            │
  │ ─────────────────────────────>│
  │                               │ 验证 Session
  │                               │ 入队消息
  │  HTTP 202 Accepted            │
  │  {message_id}                 │
  │ <─────────────────────────────│
  │                               │ 处理消息
  │                               │
  │  event: message               │
  │  data: {jsonrpc response}     │
  │ <─────────────────────────────│
```

### 6.3 重连流程

```
Client                          Server
  │                               │
  │  [连接断开]                    │
  │                               │
  │  GET /sse?session_id=xxx      │
  │  Last-Event-ID: msg_123       │
  │ ─────────────────────────────>│
  │                               │ 恢复 Session
  │                               │ 重发未确认消息
  │  event: message               │
  │  data: {pending messages}     │
  │ <─────────────────────────────│
```

---

## 7. 错误处理

### 7.1 错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum SseError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Session already connected")]
    SessionAlreadyConnected,

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Message queue full")]
    QueueFull,

    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    #[error("Connection lost")]
    ConnectionLost,

    #[error("Internal error: {0}")]
    Internal(String),
}
```

### 7.2 错误响应格式

```rust
/// SSE 错误事件
pub struct SseErrorResponse {
    pub error: String,
    pub code: String,
    pub message: String,
    pub retry: Option<u64>,  // 建议重试时间 (毫秒)
}

// 示例:
// event: error
// data: {"error":"auth_failed","code":"AUTH_001","message":"Invalid token","retry":5000}
```

### 7.3 错误处理策略

| 错误类型 | 处理策略 |
|----------|----------|
| `SessionNotFound` | 返回 404，客户端应重新初始化会话 |
| `SessionAlreadyConnected` | 返回 409，客户端应等待或强制断开旧连接 |
| `AuthFailed` | 返回 401，客户端应刷新 Token |
| `TokenExpired` | 返回 401 + `retry` 建议 |
| `QueueFull` | 返回 503，客户端应指数退避重试 |
| `ConnectionLost` | 服务端清理 Session，等待重连 |

### 7.4 连接清理

```rust
/// Session 清理策略
pub struct SessionCleanupConfig {
    /// 无活动超时时间
    pub idle_timeout: Duration,       // 默认 5 分钟
    /// 最大重连窗口
    pub reconnect_window: Duration,   // 默认 30 秒
    /// 清理检查间隔
    pub cleanup_interval: Duration,   // 默认 1 分钟
}
```

---

## 8. 实现清单

### 8.1 代码模块

| 模块 | 文件 | 状态 |
|------|------|------|
| SSE Transport | `mcp-server/src/sse.rs` | ☐ |
| 认证中间件 | `mcp-server/src/auth.rs` | ☐ |
| 消息队列 | `mcp-server/src/message_queue.rs` | ☐ |
| Session 管理 | `mcp-server/src/session.rs` | ☐ |
| 配置 | `mcp-server/src/config.rs` | ☐ |

### 8.2 测试清单

- [ ] SSE 连接建立测试
- [ ] Token 验证测试
- [ ] 消息收发测试
- [ ] 重连测试
- [ ] 超时清理测试
- [ ] 并发压力测试

---

## 附录 A: 完整 SSE 示例

```javascript
// Client 端示例 (TypeScript)
const connectMcpSse = async (sessionId: string, initialToken: string) => {
  const eventSource = new EventSource(
    `http://localhost:3721/sse?session_id=${sessionId}`,
    {
      headers: { 'Authorization': `Bearer ${initialToken}` }
    }
  );

  let messageEndpoint: string | null = null;
  let sessionToken: string | null = null;

  eventSource.addEventListener('endpoint', (event) => {
    const data = JSON.parse(event.data);
    messageEndpoint = data.endpoint;
    sessionToken = data.token;
    console.log('MCP endpoint ready:', messageEndpoint);
  });

  eventSource.addEventListener('message', (event) => {
    const mcpResponse = JSON.parse(event.data);
    console.log('MCP response:', mcpResponse);
  });

  eventSource.addEventListener('error', (event) => {
    const error = JSON.parse(event.data);
    console.error('SSE error:', error);
    if (error.retry) {
      setTimeout(() => reconnect(), error.retry);
    }
  });

  // 发送 MCP 请求
  const sendMcpRequest = async (request: McpRequest) => {
    if (!messageEndpoint || !sessionToken) {
      throw new Error('SSE not ready');
    }
    const response = await fetch(`http://localhost:3721${messageEndpoint}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${sessionToken}`
      },
      body: JSON.stringify(request)
    });
    return response.json();
  };

  return { eventSource, sendMcpRequest };
};
```

---

## 附录 B: Rust 服务端示例

```rust
// SSE Handler 示例
async fn sse_handler(
    Query(params): Query<SseQueryParams>,
    auth: AuthenticatedSession,
    Extension(sessions): Extension<Arc<SessionManager>>,
) -> Result<SseResponse, SseError> {
    // 创建 SSE 流
    let (tx, rx) = mpsc::channel(100);

    // 注册 Session
    sessions.register(params.session_id, tx, auth.claims).await?;

    // 发送 endpoint 事件
    let endpoint_msg = SseMessage::Endpoint {
        endpoint: "/message".to_string(),
        token: sessions.generate_session_token(&params.session_id)?,
    };

    // 创建流
    let stream = stream::once(future::ok(endpoint_msg))
        .chain(rx.map(Ok))
        .chain(stream::once(future::ok(SseMessage::Connected)));

    Ok(SseResponse::new(stream))
}
```

---

**文档结束**
