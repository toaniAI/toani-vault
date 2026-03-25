---
title: '截图服务使用模拟数据替换为真实页面信息'
slug: 'screenshot-mock-data'
created: '2026-03-19'
status: 'implemented'
priority: 'P1'
---

# Tech-Spec: 截图服务使用模拟数据替换为真实页面信息

## 概述

### 问题陈述

`ScreenshotService::create_page_info` 方法（`src/tee/sandbox/export/screenshot.rs:645-667`）使用两处模拟数据：

1. **硬编码 URL**：`url: "https://example.com".to_string()`，不是沙箱会话中浏览器的实际页面地址。
2. **伪 DOM 哈希**：`compute_dom_hash` 方法（行 670-679）使用当前时间戳的哈希值作为 DOM 哈希，而不是真实的页面 DOM 内容哈希。

这两个问题导致：
- `FrozenPageState::page_url` 记录的是假 URL，`execute_capture` 会对 `"https://example.com"` 截图（若 CDP 已连接），而非对沙箱会话中用户正在操作的真实页面截图。
- `FrozenPageState::dom_hash` 无法用于 TOCTOU 完整性验证，因为每次计算的值都不同。
- 截图元数据 `ScreenshotMetadata::page_url` 记录的是错误信息，审计日志失真。

### 解决方案

通过 Chrome DevTools Protocol（CDP）从浏览器实例获取真实页面信息：

1. 在 `PlaywrightClient` 中新增 `get_page_info(page_url_hint: Option<&str>) -> Result<RealPageInfo, ExportError>` 方法，通过 CDP 获取当前页面 URL、标题，并计算 DOM 哈希。
2. `ScreenshotService` 中 `create_page_info` 调用 `self.playwright.get_page_info()` 替代硬编码值。
3. 当 CDP 不可用时（`#[cfg(not(feature = "screenshot-cdp"))]` 或浏览器未连接），回退到从 `ScreenshotService` 持有的 `page_url_hint` 字段或从 `SandboxSession::context` 传入的 URL。

### 范围

**在范围内：**
- 在 `PlaywrightClient` 中实现获取真实页面 URL 和标题的方法（通过 CDP）
- 实现真实的 DOM 内容哈希计算（通过 CDP 执行 JavaScript 获取 DOM 序列化文本后计算 SHA-256）
- 修改 `ScreenshotService::create_page_info`，使用真实数据替代模拟数据
- 为 `ScreenshotService` 添加 `page_url_hint` 字段作为 CDP 不可用时的回退

**不在范围内：**
- 修改 `PageStateFreezer` 的结构或冻结逻辑
- 修改 `FrozenPageState` 的字段定义
- 修改截图结果的存储或审计日志写入逻辑
- 实现 Firefox/WebKit 的 CDP 替代方案

---

## 开发上下文

### 当前代码（关键片段）

**`create_page_info` 模拟实现** — `src/tee/sandbox/export/screenshot.rs:645-667`

```rust
async fn create_page_info(&self, request: &ScreenshotRequest) -> Result<PageInfo, ExportError> {
    // 这里应该从实际的浏览器实例获取页面信息
    // 暂时使用模拟数据
    let viewport = ViewportInfo {
        width: request.viewport.as_ref().map(|v| v.width).unwrap_or(1920),
        height: request.viewport.as_ref().map(|v| v.height).unwrap_or(1080),
        scroll_x: 0.0,
        scroll_y: 0.0,
        device_scale_factor: request
            .viewport
            .as_ref()
            .and_then(|v| v.device_scale_factor)
            .unwrap_or(1.0),
    };

    Ok(PageInfo {
        url: "https://example.com".to_string(), // 应从实际页面获取
        dom_hash: self.compute_dom_hash(),
        viewport,
        metadata: Default::default(),
    })
}
```

**`compute_dom_hash` 伪实现** — `src/tee/sandbox/export/screenshot.rs:669-679`

```rust
fn compute_dom_hash(&self) -> String {
    // 实际实现应该计算页面 DOM 的哈希值
    // 用于完整性验证
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    OffsetDateTime::now_utc().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
```

**`execute_capture` 使用 `frozen_state.page_url`** — `src/tee/sandbox/export/screenshot.rs:689-691`

```rust
let data = self
    .playwright
    .capture_screenshot(&frozen_state.page_url, request)
    .await?;
```

这里 `frozen_state.page_url` 来自 `create_page_info` 的返回值，即当前的 `"https://example.com"`，这意味着截图的目标页面始终是 example.com。

**`PlaywrightClient` 现有的 CDP 截图方法**（使用 `page.evaluate`）— `src/tee/sandbox/export/screenshot.rs:237-244`

```rust
// 隐藏指定选择器的元素
for selector in &request.hide_selectors {
    let script = format!(
        r#"document.querySelectorAll('{}').forEach(el => el.style.display = 'none');"#,
        selector.replace('\'', "\\'")
    );
    let _ = page.evaluate(script).await;
}
```

这证明了 `page.evaluate()` 已在项目中使用，可以用相同方式执行获取 URL、DOM 的 JavaScript。

**`capture_with_browser` 使用 `browser.new_page(page_url)`** — `src/tee/sandbox/export/screenshot.rs:194-197`

```rust
let page = browser
    .new_page(page_url)
    .await
    .map_err(|e| ExportError::BrowserError(format!("创建页面失败: {}", e)))?;
```

注意：当前 `capture_with_browser` 每次都 **新建页面** 并导航到传入的 URL，这个架构下"获取当前页面的 URL"并不适用——因为页面是刚创建的，其 URL 就是传入的 URL。**真正的问题是**：沙箱会话中应该有一个持久的浏览器页面（保持用户操作状态），而不是每次截图都新建页面。

### 相关代码结构

**`PageInfo` 结构体**（`src/tee/sandbox/export/freezer.rs` 中的定义，通过 `export::freezer` 导入）

```rust
pub struct PageInfo {
    pub url: String,
    pub dom_hash: String,
    pub viewport: ViewportInfo,
    pub metadata: FrozenMetadata,
}
```

**`FrozenMetadata` 包含 title 字段**（`src/tee/sandbox/export/freezer.rs:73-83`）

```rust
pub struct FrozenMetadata {
    pub title: Option<String>,
    pub active_element: Option<String>,
    pub has_focus: bool,
    pub custom_data: std::collections::HashMap<String, String>,
}
```

**`ScreenshotService` 当前字段** — `src/tee/sandbox/export/screenshot.rs:502-508`

```rust
pub struct ScreenshotService {
    freezer: PageStateFreezer,
    playwright: PlaywrightClient,
    config: ScreenshotConfig,
    watermark_service: WatermarkService,
}
```

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/tee/sandbox/export/screenshot.rs` | 主要修改目标文件（create_page_info、compute_dom_hash、ScreenshotService 结构体） |
| `src/tee/sandbox/export/freezer.rs` | PageInfo 和 FrozenMetadata 结构体定义 |
| `src/tee/sandbox/types.rs` | SessionContext，用于获取当前页面 URL hint |
| `src/tee/sandbox/session.rs` | SandboxSession::context 方法 |
| `Cargo.toml` | 确认 sha2 crate 是否已引入（用于 DOM 哈希） |

### 技术决策

1. **架构约束澄清**：`capture_with_browser` 当前采用"每次新建页面"的模式，这本质上不适合"截取持久会话页面"的需求。两种解决路径：
   - **路径 A（推荐，最小改动）**：在 `ScreenshotService` 中添加 `page_url_hint: Option<String>` 字段，`create_page_info` 优先使用此 hint 作为 URL，同时从 CDP 获取真实 DOM 哈希和标题。`capture_with_browser` 使用此 URL 导航到正确页面截图。
   - **路径 B（较大改动）**：重构 `PlaywrightClient` 为持久页面管理器，维护一个已打开的 `Page` 对象，`create_page_info` 直接查询该页面。此路径超出本条目范围。
   - **结论**：采用路径 A。

2. **`page_url_hint` 来源**：由调用方（`ScreenshotService` 构造时）传入，来自 `SandboxSession::context` 的当前页面 URL，或 `OperationRequest::parameters` 中最后一次 navigate 的 URL。

3. **DOM 哈希实现**：通过 `PlaywrightClient`（CDP）执行 `document.documentElement.outerHTML` 获取页面 HTML 字符串，计算 SHA-256 哈希。非 CDP 模式下，使用会话 ID + 时间戳拼接后计算哈希（比纯时间戳更稳定，虽然仍非真实 DOM 内容）。

4. **`ScreenshotService` 构造方法更新**：`with_freezer` 和 `with_playwright` 保持现有签名不变（向后兼容），新增 `with_page_url` 方法用于设置 hint：
   ```rust
   pub fn with_page_url(mut self, url: impl Into<String>) -> Self {
       self.page_url_hint = Some(url.into());
       self
   }
   ```

5. **SHA-256 依赖**：`sha2` crate 可能已通过 TEE-106 的 HMAC 任务引入，直接复用。

---

## 实现计划

### 任务（按依赖顺序）

1. **在 `ScreenshotService` 结构体中添加 `page_url_hint` 字段** — `src/tee/sandbox/export/screenshot.rs:502`
   - 添加 `page_url_hint: Option<String>` 字段
   - 更新 `new` 构造方法，增加 `page_url_hint: Option<String>` 参数
   - 更新 `with_freezer`、`with_playwright`、`with_all_services` 构造方法，将 `page_url_hint` 默认设置为 `None`（不改变现有方法签名）
   - 添加 `pub fn with_page_url(mut self, url: impl Into<String>) -> Self` 设置方法（builder 模式）

2. **在 `PlaywrightClient` 中实现 `get_current_page_info` 方法** — `src/tee/sandbox/export/screenshot.rs`（紧随 `health_check` 方法之后新增）
   - **CDP 实现**（`#[cfg(feature = "screenshot-cdp")]`）：
     1. 若 `browser.is_none()`，返回 `Err(ExportError::BrowserConnectionError(...))`
     2. 获取所有打开的 pages：`browser.pages().await`（chromiumoxide API，需确认）
     3. 若无打开的页面，创建一个导航到 `page_url_hint` 的页面
     4. 对第一个页面执行 `page.evaluate("() => window.location.href").await` 获取真实 URL
     5. 执行 `page.evaluate("() => document.title").await` 获取标题
     6. 执行 `page.evaluate("() => document.documentElement.outerHTML").await` 获取 DOM HTML
     7. 对 DOM HTML 字节计算 SHA-256 哈希，格式化为 hex 字符串
     8. 返回 `Ok((url, title, dom_hash))`
   - **非 CDP 回退**（`#[cfg(not(feature = "screenshot-cdp"))]`）：
     1. 接受 `page_url_hint: Option<&str>` 参数
     2. 使用 hint URL 或 `"unknown"` 作为 URL
     3. DOM 哈希使用 `SHA-256(session_id.to_string() + &timestamp)` 的前 16 字节 hex（比时间戳哈希更稳定但仍非真实内容）
     4. 返回 `Ok((url, None, dom_hash))`

3. **修改 `create_page_info`，使用真实数据** — `src/tee/sandbox/export/screenshot.rs:645`
   - 调用 `self.playwright.get_current_page_info(self.page_url_hint.as_deref()).await`
   - **成功路径**：使用返回的真实 URL、标题填充 `PageInfo`
   - **失败路径（CDP 不可用）**：降级使用 `self.page_url_hint.clone().unwrap_or_else(|| "unknown".to_string())` 作为 URL
   - DOM 哈希始终使用步骤 2 计算的值（无论是真实还是回退哈希）
   - `FrozenMetadata::title` 填充从 CDP 获取的标题（若可用）

4. **删除或重构 `compute_dom_hash` 方法** — `src/tee/sandbox/export/screenshot.rs:669`
   - 该方法的时间戳哈希逻辑移入步骤 2 的非 CDP 回退实现
   - 若无其他调用者，删除 `compute_dom_hash` 私有方法
   - 若需保留（向后兼容），标记为 `#[deprecated]`

5. **确认 `sha2` crate 可用，必要时添加** — `Cargo.toml`
   - 检查是否已有 `sha2` 依赖
   - 若无，添加 `sha2 = "0.10"` 到 `[dependencies]`
   - 在 `screenshot.rs` 顶部添加 `use sha2::{Sha256, Digest};`

6. **更新截图服务的构造调用点** — `src/api/websocket.rs`（TEE-108 已修改）
   - 在 TEE-108 实现的 `take_screenshot` 函数中，通过 `.with_page_url(page_url)` 传入从 `session.context()` 获取的 URL
   - 示例：`ScreenshotService::with_playwright(freezer, playwright).with_page_url(session_url)`

7. **编写单元测试** — `src/tee/sandbox/export/screenshot.rs`（`#[cfg(test)]` 模块）
   - `test_create_page_info_uses_hint_url`：构造带 `page_url_hint` 的 `ScreenshotService`（CDP 禁用），调用 `create_page_info`，断言返回的 `PageInfo::url` 等于 hint URL
   - `test_create_page_info_fallback_without_hint`：不传 hint，断言 URL 不是 `"https://example.com"`（即使是 `"unknown"` 也是正确的回退）
   - `test_dom_hash_is_deterministic_for_same_session`：同一个 session_id 的两次哈希计算（非 CDP 回退路径），断言相同（固定算法，不依赖时间）

### 验收标准

- **Given** `ScreenshotService` 通过 `.with_page_url("https://app.example.com/dashboard")` 设置了 URL hint，且 CDP 浏览器服务不可用，
  **When** 调用 `capture`，
  **Then** `ScreenshotResult::metadata.page_url` 等于 `"https://app.example.com/dashboard"`（而非 `"https://example.com"`）。

- **Given** CDP 浏览器服务可用且已连接，
  **When** 调用 `capture`，
  **Then** `ScreenshotResult::metadata.page_url` 等于浏览器当前页面的真实 URL（通过 `window.location.href` 获取）。

- **Given** `compute_dom_hash` 旧实现在相同 session 中被调用两次（旧行为），
  **Then** 两次返回值不同（旧 bug）。
  **Given** 新的非 CDP 回退 DOM 哈希在相同 session 中被计算两次，
  **Then** 两次返回值相同（新行为）。

- **Given** CDP 浏览器服务可用，
  **When** `create_page_info` 成功从浏览器获取 DOM，
  **Then** `PageInfo::dom_hash` 是该页面 DOM HTML 的 SHA-256 hex 字符串。

- `"https://example.com"` 不再出现在 `create_page_info` 的返回值中。

- `cargo build` 无编译错误。
- 所有现有截图服务单元测试通过。

---

## 附加上下文

### 依赖

- **与 TEE-108 的协同**：TEE-108 负责将 `ScreenshotService` 接入 WebSocket handler，并通过 `session.context()` 传入页面 URL。本条目负责让 `create_page_info` 使用该 URL 而非硬编码值。两条规格应协同开发，但可独立合并（本条目不依赖 TEE-108 完成）。
- `sha2 = "0.10"` 依赖可能已通过 TEE-106 引入，需确认。

### 测试策略

- 单元测试覆盖 CDP 不可用（非 cdp feature 编译）下的回退逻辑
- CDP 可用路径的集成测试需要浏览器服务，可在 CI Docker 环境中进行
- 重点测试：DOM 哈希的稳定性（相同内容哈希相同）和 URL 来源正确性

### 注意事项

- **架构限制**：`capture_with_browser` 目前是每次截图都 `browser.new_page(page_url)` 创建新页面，这意味着即使从 CDP 获取到真实的"当前页面 URL"，截图时仍然是对该 URL **重新导航**（新建页面），而非截取用户会话中已有的页面状态。这是截图架构的根本性限制，需要在更高层级（沙箱会话持久化浏览器页面）解决，超出本规格范围。本条目的改进主要是：
  1. 确保 `page_url` 不再是 `"https://example.com"` 而是实际的目标 URL
  2. 确保 `dom_hash` 不再是随机值
  3. 为未来的持久页面架构预留接口（`page_url_hint` 字段）
- `chromiumoxide` 的 `browser.pages()` API 需要在实现时验证具体方法名称（可能是 `pages()` 或需要通过其他方式获取当前页面列表），参考 [chromiumoxide 文档](https://docs.rs/chromiumoxide)。
- 获取完整 `document.documentElement.outerHTML` 对于复杂页面可能返回较大的字符串（数 MB），在 CDP evaluate 调用时可能有性能影响。可考虑仅对前 64KB 计算哈希，或对 `document.body.innerHTML` 计算哈希以减小数据量。实现时记录此权衡决策为代码注释。
