---
title: 'TEE-108 - WebSocket 沙箱截图功能真实集成'
slug: 'tee-108-websocket-screenshot'
created: '2026-03-19'
status: 'implemented'
priority: 'P1'
---

# Tech-Spec: TEE-108 - WebSocket 沙箱截图功能真实集成

## 概述

### 问题陈述

`src/api/websocket.rs` 中的 `take_screenshot` 函数（行 648-666）当前返回一个硬编码的 1x1 像素最小 PNG，从未调用 `ScreenshotService`。客户端发送 `{"type": "screenshot"}` 消息时，收到的是一张无意义的占位图，而非沙箱会话页面的真实截图。

当前代码（`src/api/websocket.rs:647-666`）：

```rust
async fn take_screenshot(
    _state: &ConnectionState,
    _ctx: &ApiContext,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // TODO(#TEE-108): 实际调用沙箱截图功能
    // 需要: 集成 ScreenshotService
    // 当前: 返回一个1x1像素的PNG图片作为模拟
    let png_data = vec![
        0x89, 0x50, 0x4E, 0x47, ...  // 1x1 PNG 字节数组
    ];
    Ok(png_data)
}
```

### 解决方案

将 `take_screenshot` 与 `ScreenshotService` 真实集成。核心变更：

1. 通过 `SandboxPool::get_session` 取得当前会话的 `SandboxSession`。
2. 从会话上下文（`SessionContext`）获取页面 URL 和 session_id。
3. 构建 `PageStateFreezer`、`PlaywrightClient` 和 `ScreenshotService`，调用 `ScreenshotService::capture`。
4. 返回 `ScreenshotResult::data`（PNG 字节）。

**前置依赖**：本条目依赖 TEE-107 中对 `ApiContext` 的 `sandbox_pool` 字段扩展。如果 TEE-107 尚未合并，需要先完成 TEE-107 中的任务 1-2。

### 范围

**在范围内：**
- 修改 `take_screenshot` 函数，通过 `ScreenshotService` 执行真实截图
- 修改函数签名（移除下划线前缀），实际使用 `state` 和 `ctx` 参数
- 处理 `ScreenshotService` 初始化（使用 `PlaywrightConfig` 中的 WebSocket 端点）
- 处理截图失败的错误路径

**不在范围内：**
- 实现 `PlaywrightClient` 与浏览器的连接管理池（复用连接）
- 实现截图结果的持久化存储
- 修改 WebSocket 消息协议（`ScreenshotResult` 消息结构不变）
- 修改 `ScreenshotService` 内部逻辑

---

## 开发上下文

### 当前代码（关键片段）

**`take_screenshot` 函数（模拟实现）** — `src/api/websocket.rs:647-666`

```rust
async fn take_screenshot(
    _state: &ConnectionState,
    _ctx: &ApiContext,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // TODO(#TEE-108): 实际调用沙箱截图功能
    // 需要: 集成 ScreenshotService
    // 当前: 返回一个1x1像素的PNG图片作为模拟
    let png_data = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        // ... 省略 ...
    ];
    Ok(png_data)
}
```

**`handle_message` 中 `Screenshot` 分支的调用** — `src/api/websocket.rs:491-516`

```rust
ClientMessage::Screenshot { request_id } => {
    let req_id = request_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    debug!("Screenshot requested: {}", req_id);

    // 模拟截图操作
    let screenshot_result = take_screenshot(state, ctx).await;

    let result_msg = match screenshot_result {
        Ok(image_data) => ServerMessage::ScreenshotResult {
            request_id: req_id,
            success: true,
            image_data: Some(STANDARD.encode(&image_data)),
            format: Some("png".to_string()),
            error: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
        // ...
    };
    let _ = tx.send(result_msg).await;
}
```

**`ScreenshotService` 入口**（`src/tee/sandbox/export/screenshot.rs:578-615`）

```rust
pub async fn capture(
    &self,
    request: ScreenshotRequest,
) -> Result<ScreenshotResult, ExportError> {
    let start_time = OffsetDateTime::now_utc();
    info!("开始捕获会话 {} 的截图", self.freezer.session_id);
    let page_info = self.create_page_info(&request).await?;
    let frozen_state = self.freezer.freeze(page_info).await?;
    let result = self.execute_capture(&request, &frozen_state, start_time).await;
    // 解冻页面...
    Ok(screenshot)
}
```

**`PlaywrightClient` 默认配置**（`src/tee/sandbox/export/screenshot.rs:37-45`）

```rust
impl Default for PlaywrightConfig {
    fn default() -> Self {
        Self {
            ws_endpoint: "ws://localhost:9222".to_string(),
            timeout: Duration::from_secs(30),
            browser_type: BrowserType::Chromium,
        }
    }
}
```

**`ScreenshotService::with_freezer` 构造方法** — `src/tee/sandbox/export/screenshot.rs:527-534`

```rust
pub fn with_freezer(freezer: PageStateFreezer) -> Self {
    Self::new(
        freezer,
        PlaywrightClient::with_default_config(),
        ScreenshotConfig::default(),
        WatermarkService::default_service(),
    )
}
```

**`ScreenshotResult` 关键字段** — `src/tee/sandbox/export/screenshot.rs:462-483`

```rust
pub struct ScreenshotResult {
    pub data: Vec<u8>,
    pub format: ImageFormat,
    pub dimensions: ImageDimensions,
    pub timestamp: OffsetDateTime,
    pub session_id: SessionId,
    pub frozen_state: Option<FrozenPageState>,
    pub metadata: ScreenshotMetadata,
    pub signature: Option<Signature>,
    pub watermark_applied: bool,
    pub review_result: Option<ReviewResult>,
}
```

**`ConnectionState` 包含 session_id** — `src/api/websocket.rs:178-196`

```rust
struct ConnectionState {
    session_id: SessionId,
    // ...
}
```

**`PageStateFreezer` 构造**（需在 `src/tee/sandbox/export/freezer.rs` 中确认）

```rust
// 位于 src/tee/sandbox/export/freezer.rs
pub struct PageStateFreezer {
    pub session_id: SessionId,
    // ...
}
```

### 相关代码结构

- `create_page_info` 方法（行 646-667）：当前使用模拟数据（`url: "https://example.com"`），**这是截图条目 4（tech-spec-screenshot-mock-data）的修复范围**，TEE-108 不需修改此方法，但需知道它存在硬编码数据。
- `execute_capture` 方法（行 682-720+）：通过 `self.playwright.capture_screenshot` 调用 CDP 真实截图，只要 `PlaywrightClient` 已连接浏览器，此路径是完整的。
- `PlaywrightClient::connect` 方法：需要在使用 `capture_screenshot` 前调用，但在 `with_default_config()` 构造时不会自动连接。
- `SandboxSession::context` 方法：返回 `&SessionContext`，`SessionContext` 应包含页面 URL 等信息（需确认 `SessionContext` 结构）。

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/api/websocket.rs` | 主要修改目标文件 |
| `src/tee/sandbox/export/screenshot.rs` | ScreenshotService、PlaywrightClient、ScreenshotRequest |
| `src/tee/sandbox/export/freezer.rs` | PageStateFreezer 构造方法和字段 |
| `src/tee/sandbox/types.rs` | SessionContext 结构体，用于获取页面 URL |
| `src/tee/sandbox/pool.rs` | SandboxPool::get_session |
| `src/tee/sandbox/session.rs` | SandboxSession::context |
| `src/api/context.rs` | ApiContext（TEE-107 修改后含 sandbox_pool） |

### 技术决策

1. **`PlaywrightClient` 连接时机**：每次截图时创建新的 `PlaywrightClient` 并尝试连接，失败时回退到 `PlaywrightClient` 的模拟模式（`generate_mock_image_data`）。不在 `ApiContext` 中持久化 `PlaywrightClient` 连接，避免增加全局状态复杂度（后续可优化为连接池）。
2. **`PageStateFreezer` 构造**：`PageStateFreezer::new(session_id)` 需从 `freezer.rs` 确认构造方法签名，使用 `state.session_id` 创建。
3. **页面 URL 来源**：从 `session.context().page_url`（或类似字段）获取。若 `SessionContext` 不包含页面 URL，退而使用空字符串（`PageStateFreezer` 的 `page_url` 字段在后续的 `create_page_info` 中会被模拟数据覆盖，这个问题是截图条目 4 的修复范围）。
4. **错误处理**：`ExportError` 需转换为 `Box<dyn Error + Send + Sync>`，使用 `.map_err(|e| Box::new(e) as Box<dyn ...>)`（确认 `ExportError: Error + Send + Sync`）。
5. **格式映射**：`ScreenshotResult::format` 是 `ImageFormat` 枚举，需映射为字符串（`"png"`/`"jpeg"`/`"webp"`）发送给客户端，使用 `format.to_string()`（`Display` 已实现）。

---

## 实现计划

### 任务（按依赖顺序）

1. **确认 TEE-107 的 `ApiContext::sandbox_pool` 字段已存在** — `src/api/context.rs`
   - 若 TEE-107 未完成，先执行 TEE-107 的任务 1-2（添加 `sandbox_pool` 字段）

2. **确认 `PageStateFreezer` 的构造方法签名** — `src/tee/sandbox/export/freezer.rs`
   - 找到 `PageStateFreezer::new` 或类似构造方法，确认接受哪些参数
   - 若仅需 `session_id`，直接使用 `state.session_id`

3. **确认 `SessionContext` 结构体包含页面 URL** — `src/tee/sandbox/types.rs`
   - 查找 `SessionContext` 的字段定义
   - 确认是否有 `page_url: String` 或类似字段
   - 若无，记录需要在后续 sprint 中添加（当前使用空字符串占位）

4. **确认 `ExportError` 实现 `std::error::Error + Send + Sync`** — `src/tee/sandbox/error.rs`
   - 同 TEE-107 任务 1，两者可合并检查

5. **实现 `take_screenshot` 真实截图逻辑** — `src/api/websocket.rs:647`
   - 将函数签名改为：
     ```rust
     async fn take_screenshot(
         state: &ConnectionState,
         ctx: &ApiContext,
     ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>
     ```
   - 函数体：
     1. 从 `ctx.sandbox_pool` 获取池，若为 `None` 返回错误
     2. `pool.get_session(state.session_id).await` 获取会话，失败返回错误
     3. 从 `session.context()` 获取页面 URL（或使用空字符串占位）
     4. 构建 `PageStateFreezer::new(state.session_id)` 或等效构造
     5. 构建 `PlaywrightClient::with_default_config()`，并调用 `connect().await`（失败时 `warn!` 继续，会回退到模拟模式）
     6. 构建 `ScreenshotService::with_playwright(freezer, playwright)`
     7. 调用 `service.capture(ScreenshotRequest::default()).await`
     8. 返回 `result.data`
   - 删除整个硬编码 `png_data` 字节数组

6. **更新 `handle_message` 中 `Screenshot` 分支的格式映射** — `src/api/websocket.rs:499`
   - 当前 `format: Some("png".to_string())` 是硬编码；真实实现后从 `ScreenshotResult::format.to_string()` 获取
   - 由于 `take_screenshot` 只返回 `Vec<u8>` 而不包含格式信息，有两种选项：
     - 方案 A：将返回类型改为 `(Vec<u8>, String)` 元组（format 字符串一起返回）
     - 方案 B：保持 `Vec<u8>` 返回，在 `take_screenshot` 内部始终使用 `ScreenshotRequest::default()` 的 PNG 格式，调用方硬编码 `"png"`
   - 推荐**方案 B**（最小改动），若后续需要支持 JPEG 格式，届时再扩展接口

7. **运行编译和测试验证** — 项目根目录
   - 执行 `cargo build` 确认无编译错误
   - 执行 `cargo test` 确认现有测试通过

### 验收标准

- **Given** WebSocket 客户端连接到有效会话，且 Playwright 浏览器服务在 `ws://localhost:9222` 可用，
  **When** 发送 `{"type": "screenshot"}` 消息，
  **Then** 服务端调用 `ScreenshotService::capture`，客户端收到包含真实页面截图的 base64 PNG 数据（非 1x1 占位图）。

- **Given** Playwright 浏览器服务不可用（CDP feature 未启用或浏览器未启动），
  **When** 发送 `{"type": "screenshot"}` 消息，
  **Then** 服务端回退到 `PlaywrightClient` 的模拟数据模式，客户端收到 `success: true`（不是错误），图片为模拟 PNG。

- **Given** `ApiContext::sandbox_pool` 为 `None`，
  **When** 发送截图消息，
  **Then** 客户端收到 `{"type": "screenshot_result", "success": false, "error": "..."}`，不 panic。

- **Given** session_id 在池中不存在，
  **When** 发送截图消息，
  **Then** 客户端收到 `success: false` 的截图结果，包含 session not found 错误信息。

- `cargo build` 无编译错误。

---

## 附加上下文

### 依赖

- **前置条件（强）**：TEE-107 中 `ApiContext::sandbox_pool: Option<Arc<dyn SandboxPool>>` 字段必须已添加
- `ScreenshotService` 已在 `src/tee/sandbox/export/screenshot.rs` 实现，包含 `with_playwright` 和 `with_freezer` 构造方法
- `PlaywrightClient::capture_screenshot` 已支持 CDP 回退到模拟数据（行 148-183），无需额外处理浏览器未连接情况

### 测试策略

- 单元测试：mock `SandboxPool` 返回 mock `SandboxSession`，mock `ScreenshotService` 返回固定 PNG 数据，测试 `take_screenshot` 的调用路径
- 集成测试：需要 Playwright 浏览器服务，可在 CI 环境中通过 Docker 启动 Chromium，然后运行 WebSocket 集成测试

### 注意事项

- `ScreenshotService` 每次截图都需要 `PageStateFreezer`，后者在 `capture` 调用时会执行 freeze/unfreeze 操作。这要求 `PageStateFreezer` 能够与真实浏览器页面交互。当前 `freeze` 的实现（`src/tee/sandbox/export/freezer.rs`）需要检查是否包含模拟逻辑——若 freezer 本身也是模拟的，截图仍能工作（只是没有 TOCTOU 保护）。
- 本规格不要求解决 `create_page_info` 中的模拟数据问题（`url: "https://example.com"`），该问题由截图条目 4 的规格单独处理。即使 URL 是模拟的，只要 `PlaywrightClient` 能打开该 URL 并截图，流程就是完整的。
- `PlaywrightClient::connect` 是异步方法，且 `#[cfg(feature = "screenshot-cdp")]` 下才有真实实现。未启用该 feature 时，`connect` 返回 `Err`，但 `capture_screenshot` 内部会检测 `browser.is_none()` 并回退到模拟数据，整个截图流程不会因此失败。
