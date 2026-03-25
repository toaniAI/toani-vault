---
title: 'TEE-107 - WebSocket 沙箱会话执行操作真实集成'
slug: 'tee-107-websocket-sandbox-execute'
created: '2026-03-19'
status: 'implemented'
priority: 'P1'
---

# Tech-Spec: TEE-107 - WebSocket 沙箱会话执行操作真实集成

## 概述

### 问题陈述

`src/api/websocket.rs` 中的 `execute_operation` 函数（行 621-645）是 WebSocket `execute` 消息的核心处理逻辑，当前使用 `tokio::time::sleep(1500ms)` 模拟执行，并返回硬编码的成功结果。这意味着所有通过 WebSocket 发出的操作命令（Navigate、Click、Fill、GetText 等）实际上从未被执行，客户端收到的是假数据。

当前代码（`src/api/websocket.rs:620-645`）：

```rust
async fn execute_operation(
    operation: OperationRequest,
    _state: &ConnectionState,
) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
    let start = std::time::Instant::now();

    // TODO(#TEE-107): 实际调用沙箱会话执行操作
    // 需要: WebSocket 与沙箱会话池集成
    // 当前: 使用模拟实现进行开发测试
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let execution_time_ms = start.elapsed().as_millis() as u64;

    Ok(ExecutionResult {
        success: true,
        data: Some(serde_json::json!({
            "message": format!("Operation {:?} completed", operation.operation_type),
            "parameters": operation.parameters,
        })),
        error: None,
        execution_time_ms,
        screenshot: None,
        audit_log: vec![],
    })
}
```

### 解决方案

将 `execute_operation` 与 `SandboxPool`（`NsjailSandboxPool`）真实集成。核心变更：

1. 将 `Arc<dyn SandboxPool>` 注入 `ApiContext`，使 WebSocket handler 可以访问沙箱池。
2. `execute_operation` 通过 `state.session_id` 从池中获取对应的 `SandboxSession`，调用其 `execute_operation` 方法。
3. 处理会话不存在、已过期、执行错误等边界情况，映射为 WebSocket 错误消息。

### 范围

**在范围内：**
- 将 `Arc<dyn SandboxPool>` 字段添加到 `ApiContext`
- 修改 `execute_operation` 函数，通过 `SandboxPool::get_session` + `SandboxSession::execute_operation` 执行真实操作
- 修改 `execute_operation_with_timeout` 的签名以传入 `ctx`（`_ctx` 已存在但被忽略）
- 更新 WebSocket handler 测试（如果存在）

**不在范围内：**
- 实现 `NsjailSandbox::execute_operation` 的内部逻辑（假设其已实现或有 stub）
- 修改沙箱池的会话管理逻辑
- 修改 WebSocket 消息协议结构

---

## 开发上下文

### 当前代码（关键片段）

**`execute_operation` 函数（模拟实现）** — `src/api/websocket.rs:620-645`

```rust
async fn execute_operation(
    operation: OperationRequest,
    _state: &ConnectionState,
) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
    let start = std::time::Instant::now();
    // TODO(#TEE-107): 实际调用沙箱会话执行操作
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let execution_time_ms = start.elapsed().as_millis() as u64;
    Ok(ExecutionResult {
        success: true,
        data: Some(serde_json::json!({
            "message": format!("Operation {:?} completed", operation.operation_type),
            "parameters": operation.parameters,
        })),
        error: None,
        execution_time_ms,
        screenshot: None,
        audit_log: vec![],
    })
}
```

**`execute_operation_with_timeout` 函数签名（`_ctx` 未被使用）** — `src/api/websocket.rs:575-582`

```rust
async fn execute_operation_with_timeout(
    operation: OperationRequest,
    state: &ConnectionState,
    _ctx: &ApiContext,     // <-- 当前未使用
    timeout_secs: u64,
    tx: mpsc::Sender<ServerMessage>,
) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
```

**`ApiContext` 当前结构** — `src/api/context.rs:22-29`

```rust
pub struct ApiContext {
    pub db: Option<Arc<sqlx::PgPool>>,
    pub vault: Option<Arc<crate::vault::storage::CredentialVault>>,
    pub config: Option<Arc<std::collections::HashMap<String, String>>>,
}
```

**`ConnectionState` 结构**（包含 `session_id`）— `src/api/websocket.rs:178-196`

```rust
struct ConnectionState {
    session_id: SessionId,
    tenant_id: Uuid,
    user_id: Uuid,
    credential_id: Uuid,
    connected_at: chrono::DateTime<chrono::Utc>,
    last_heartbeat: chrono::DateTime<chrono::Utc>,
    current_operation: Option<String>,
}
```

**`SandboxPool` trait**（关键方法）— `src/tee/sandbox/pool.rs:18-38`

```rust
#[async_trait::async_trait]
pub trait SandboxPool: Send + Sync {
    async fn acquire_session(&self, request: SessionRequest) -> Result<Arc<dyn SandboxSession>, SandboxError>;
    async fn release_session(&self, session_id: SessionId) -> Result<(), SandboxError>;
    async fn get_session(&self, session_id: SessionId) -> Result<Arc<dyn SandboxSession>, SandboxError>;
    async fn health(&self) -> SandboxHealth;
    async fn shutdown(&self) -> Result<(), SandboxError>;
    fn as_any(&self) -> &dyn std::any::Any;
}
```

**`SandboxSession` trait**（执行方法）— `src/tee/sandbox/session.rs:20-41`

```rust
#[async_trait]
pub trait SandboxSession: Send + Sync {
    fn id(&self) -> SessionId;
    async fn status(&self) -> SessionStatus;
    fn context(&self) -> &SessionContext;
    async fn execute_operation(&self, operation: OperationRequest) -> Result<ExecutionResult, SandboxError>;
    async fn pause(&self) -> Result<(), SandboxError>;
    async fn resume(&self) -> Result<(), SandboxError>;
    async fn close(&self) -> Result<(), SandboxError>;
    fn is_expired(&self) -> bool;
}
```

### 相关代码结构

- `handle_message` 中的 `Execute` 分支（行 425-489）：构建 `OperationRequest` 并调用 `execute_operation_with_timeout`，之后发送 `OperationCompleted` 消息。该分支已正确传入 `ctx`，但 `ctx` 在底层未被使用。
- `SandboxError` 枚举：位于 `src/tee/sandbox/error.rs`，需检查其实现 `std::error::Error` 以便可以用 `?` 转换为 `Box<dyn Error>`。
- `SessionStatus`：位于 `src/tee/sandbox/types.rs`，当会话状态为 `Closed` 或 `Error` 时，应在执行前返回错误。

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/api/websocket.rs` | 主要修改目标文件 |
| `src/api/context.rs` | 修改 ApiContext，添加 sandbox_pool 字段 |
| `src/tee/sandbox/pool.rs` | SandboxPool trait，get_session 方法 |
| `src/tee/sandbox/session.rs` | SandboxSession trait，execute_operation 方法 |
| `src/tee/sandbox/types.rs` | ExecutionResult、OperationRequest 类型定义 |
| `src/tee/sandbox/error.rs` | SandboxError，用于错误映射 |
| `src/main.rs` 或 `src/app.rs` | ApiContext 的构造位置，需要注入 sandbox_pool |

### 技术决策

1. **`ApiContext` 扩展方式**：在 `ApiContext` 中添加 `pub sandbox_pool: Option<Arc<dyn SandboxPool>>`。使用 `Option` 保持向后兼容（未配置池时，降级返回操作未支持的错误，而非 panic）。
2. **会话查找策略**：通过 `pool.get_session(state.session_id)` 查找已存在的会话。WebSocket 连接建立时（在 `handle_socket` 中），会话应已由上游（REST API 的 session 创建接口）创建并存入池中。如果查找失败（`SandboxError::SessionNotFound`），向客户端发送 `Error` 消息并关闭连接。
3. **错误映射**：`SandboxError` 转为 `Box<dyn Error + Send + Sync>` 使用 `map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)`（需要 `SandboxError: Error + Send + Sync`，验证后再决定）。
4. **`execute_operation` 签名变更**：将 `_state: &ConnectionState` 改为 `state: &ConnectionState`，将 `_ctx: &ApiContext` 改为 `ctx: &ApiContext`（移除下划线前缀），二者均已在 `execute_operation_with_timeout` 中传入，只需向下传递。
5. **进度报告**：真实执行时，进度更新 25/50/75% 的 `tokio::spawn` 逻辑保留，但不再基于固定延迟，而是在 `execute_operation` 调用前后发送 0% 和 100% 的进度，中间的模拟进度可临时保留，后续可通过操作回调改进。

---

## 实现计划

### 任务（按依赖顺序）

1. **确认 `SandboxError` 实现了 `std::error::Error + Send + Sync`** — `src/tee/sandbox/error.rs`
   - 检查 `SandboxError` 是否通过 `thiserror::Error` 宏自动实现了 `std::error::Error`
   - 确认其派生或实现了 `Send + Sync`
   - 若有缺失，添加相应 `derive` 或 `unsafe impl`

2. **向 `ApiContext` 添加 `sandbox_pool` 字段** — `src/api/context.rs:22`
   - 在 `ApiContext` 结构体中添加字段：`pub sandbox_pool: Option<Arc<dyn crate::tee::sandbox::pool::SandboxPool>>`
   - 更新 `ApiContext::new` 方法，增加 `sandbox_pool` 参数（`Option<Arc<dyn SandboxPool>>`）
   - 更新 `ApiContext::from_request_context` 中的构造（设置为 `None`）
   - 在 `src/api/context.rs` 顶部添加必要的 use 语句

3. **更新应用启动代码中的 `ApiContext` 构造** — `src/main.rs` 或 `src/app.rs`（需先定位具体文件）
   - 找到 `ApiContext::new(...)` 的调用位置
   - 创建 `NsjailSandboxPool` 实例并初始化
   - 将其作为 `Some(Arc::new(pool))` 传入 `ApiContext::new`
   - 若当前 main.rs 中 `ApiContext` 是通过 `Default` 或其他方式构造，相应更新

4. **修改 `execute_operation` 函数，实现真实调用** — `src/api/websocket.rs:620`
   - 将函数签名改为：
     ```rust
     async fn execute_operation(
         operation: OperationRequest,
         state: &ConnectionState,
         ctx: &ApiContext,
     ) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>>
     ```
   - 函数体中：
     1. 从 `ctx.sandbox_pool` 获取池引用，若为 `None` 返回错误 `"Sandbox pool not configured"`
     2. 调用 `pool.get_session(state.session_id).await`，若返回 `SessionNotFound` 错误，向上返回
     3. 检查 `session.status().await`，若为 `Closed`/`Error` 状态返回错误
     4. 调用 `session.execute_operation(operation).await`，将结果映射为 `ExecutionResult`
     5. 失败时 `ExecutionResult { success: false, error: Some(e.to_string()), ... }`
   - 删除 `tokio::time::sleep(1500ms)` 这行

5. **更新 `execute_operation_with_timeout` 的调用，传递 `ctx`** — `src/api/websocket.rs:575`
   - 将 `_ctx: &ApiContext` 改为 `ctx: &ApiContext`
   - 在内部调用 `execute_operation(operation, state, ctx).await`

6. **更新 `handle_message` 中的调用** — `src/api/websocket.rs:459`
   - `execute_operation_with_timeout` 已传入 `ctx`（见行 459-466），确认参数传递链完整

7. **运行编译验证** — 项目根目录
   - 执行 `cargo build` 确认无编译错误
   - 执行 `cargo clippy` 确认无 lint 警告

### 验收标准

- **Given** WebSocket 客户端连接到有效会话（该会话已在 `SandboxPool` 中存在），
  **When** 发送 `{"type": "execute", "operation_type": "navigate", "description": "...", "parameters": {...}}`，
  **Then** 服务端通过 `SandboxSession::execute_operation` 执行真实操作，并返回实际结果（非固定模拟 JSON）。

- **Given** WebSocket 客户端连接，但对应 session_id 在池中不存在，
  **When** 发送任意 `execute` 消息，
  **Then** 客户端收到 `{"type": "operation_completed", "success": false, "error": "..."}` 消息，不崩溃。

- **Given** `ApiContext` 中 `sandbox_pool` 为 `None`（降级场景），
  **When** 发送 `execute` 消息，
  **Then** 客户端收到错误消息 `"Sandbox pool not configured"`，不 panic。

- 执行 `cargo build` 无编译错误。
- 所有现有 WebSocket 单元测试（`#[cfg(test)]` 模块，行 668+）通过。

---

## 附加上下文

### 依赖

- `SandboxPool` trait 和 `NsjailSandboxPool` 已在 `src/tee/sandbox/pool.rs` 实现
- `SandboxSession` trait 和 `ActiveNsjailSession` 已在 `src/tee/sandbox/session.rs` 实现
- `ExecutionResult` 类型已在 `src/tee/sandbox/types.rs` 定义，与 `websocket.rs` 中使用的类型一致（需确认是同一个类型）

### 测试策略

- 单元测试：使用 `mockall` 模拟 `SandboxPool` 和 `SandboxSession` trait，验证 `execute_operation` 调用链
- 集成测试：在 `tests/` 目录中添加 WebSocket 集成测试，使用真实的 `NsjailSandboxPool`（或其测试替代品）

### 注意事项

- `ConnectionState` 中的 `session_id` 是 WebSocket 连接建立时从 URL path 解析的（`Path((session_id, credential_id))`）。这个 session 必须在 WebSocket 连接建立之前通过 REST API（如 `POST /api/tee/sessions`）创建并注册到 `SandboxPool`。若池中不存在对应会话，WebSocket handler 应返回会话未找到错误并关闭连接，而不是创建新会话。
- `execute_operation` 原函数参数有 `_state: &ConnectionState`（下划线前缀表示未使用），移除下划线前缀后编译器会警告若未使用，确保在函数体中实际使用 `state.session_id`。
- `execute_operation_with_timeout` 内部的模拟进度更新（25/50/75%，每 500ms 一个）在真实执行时会并行运行，若真实操作在 500ms 内完成，模拟进度消息仍会发出（顺序可能颠倒）。可用 `progress_handle.abort()` 的位置保证正确性（已在行 611 存在），确认 `abort()` 在真实操作完成后立即被调用。
