# LLM 服务代码审查报告

**审查日期**: 2026-03-19
**审查者**: 对抗性代码审查员 (Claude Code)
**审查范围**: `src/services/llm/azure.rs`, `src/services/llm/openai.rs`
**整体评级**: **中等风险** — 存在多个需要整改的安全与可靠性问题

---

## 执行摘要

两个文件实现了 Azure OpenAI 和 OpenAI 兼容 API 的客户端。代码结构清晰、类型系统使用合理，但存在以下关键问题：

1. **`panic!` 路径**：`build_headers()` 在 API Key 格式非法时会 panic，生产服务中不可接受
2. **完全没有重试逻辑**：`is_retryable()` 标记已定义但从未被调用
3. **敏感信息泄露**：错误日志直接输出原始 API 错误响应 body，可能含有敏感数据
4. **`raw_response` 字段**：将完整 API 响应体存入内存，有内容泄露风险
5. **`OpenAiCompatibleClient` 忽略 `config.pricing`**：`new()` 中硬编码 `PricingInfo::default()` 而非使用传入的 pricing 配置，导致成本追踪失效
6. **`health_check` 实际消费 Token**：Azure 实现通过真实聊天请求做健康检查，产生不必要费用

---

## 详细问题清单

### 严重 (Severity: HIGH)

#### [H-1] `build_headers()` 中的 `.expect()` — 生产 panic 风险

**文件**: `azure.rs:111`, `openai.rs:107`

```rust
// azure.rs:111
headers.insert(
    "api-key",
    self.config.api_key.parse().expect("Invalid API key format"),  // ← panic
);

// openai.rs:107
format!("Bearer {}", self.config.api_key)
    .parse()
    .expect("Invalid API key format"),  // ← panic
```

**问题**: `reqwest::header::HeaderValue::from_str()` 在 API Key 包含非可见 ASCII 字符（如换行符、Unicode 等）时会返回 `Err`，`.expect()` 直接 panic，导致整个服务崩溃。

**违反规范**: [Rust Coding Standards](../../.claude/rules/rust-coding-standards.md) — "Never use `unwrap()` or `expect()` in production code"

**修复方向**: 返回 `Result<HeaderMap, LlmError>` 并将 `build_headers` 变为 fallible。

---

#### [H-2] `OpenAiCompatibleClient::new()` 忽略 pricing 配置 — 成本追踪完全失效

**文件**: `openai.rs:96`

```rust
pub fn new(name: impl Into<String>, config: OpenAiClientConfig) -> Self {
    // ...
    Self {
        name: name.into(),
        config,
        client,
        cost_tracker: Arc::new(CostTracker::new(PricingInfo::default())),  // ← BUG: 忽略 config.pricing
    }
}
```

**问题**: `config.pricing` 字段被完全忽略，`CostTracker` 始终以 `input_price_per_1k = 0.0` / `output_price_per_1k = 0.0` 初始化。即使用户配置了正确的定价，成本追踪器永远记录 $0.00，`CostController::can_proceed()` 的预算控制形同虚设。

对比 `AzureOpenAiClient::new()` 在 `azure.rs:101` 中正确使用了 `config.pricing.clone()`，这是两个文件实现不一致的回归 bug。

**修复方向**: `openai.rs:96` 改为 `Arc::new(CostTracker::new(config.pricing.clone()))`

---

### 中等 (Severity: MEDIUM)

#### [M-1] 无重试逻辑 — 可靠性缺口

**文件**: `azure.rs:207-220`, `openai.rs:205-218`

`LlmError::is_retryable()` 在 `provider.rs:102-107` 中已经定义，可区分网络错误、速率限制和超时，但两个客户端在执行 HTTP 请求后都没有调用它。

遇到 429 (Rate Limited) 或网络抖动，调用方会直接收到错误，没有任何退避重试。对于生产级 LLM 服务，至少应实现指数退避 + jitter 的重试策略。

---

#### [M-2] 错误日志泄露完整响应体

**文件**: `azure.rs:229`, `openai.rs:227`

```rust
error!("Azure OpenAI API error: {} - {}", status, body);
error!("OpenAI API error: {} - {}", status, body);
```

`body` 是原始 API 响应，可能包含：
- 模型名称、部署路径、账户信息（429 响应头中）
- 内部错误消息（500 响应中）
- 请求 ID（可用于关联追踪）

建议仅记录 status code 和 error code，截断 body 或仅记录结构化字段。

---

#### [M-3] `raw_response` 存储完整响应体

**文件**: `azure.rs:278`, `openai.rs:276`

```rust
raw_response: Some(body),
```

`ChatResponse.raw_response` 被注释为"用于调试"，但在生产代码中全量赋值。若该响应被序列化到日志、审计链或下游系统，完整的 API 响应体（可能含系统提示 echo、内容策略违规详情等）就会外泄。

建议：仅在 `DEBUG` 模式下填充此字段，或完全移除。

---

#### [M-4] Azure health_check 实际消费 Token

**文件**: `azure.rs:419-453`

健康检查通过发送一个真实的 `chat/completions` 请求（内容为 `"hi"`，`max_tokens: 1`）实现：

```rust
let health_request = AzureChatRequest {
    messages: vec![AzureMessage {
        role: "user".to_string(),
        content: AzureMessageContent::Text("hi".to_string()),
    }],
    temperature: Some(0.0),
    max_tokens: Some(1),
    response_format: None,
};
```

如果健康检查被高频调用（例如每 30 秒一次），每次调用都会：
1. 消耗 Token 产生费用
2. 触发 Azure 的部署限流配额

OpenAI 实现（`openai.rs:419`）更合理，使用 `/models` 列表端点，不消耗 Token。Azure 也可以使用 `/openai/deployments?api-version=xxx` 端点替代。

---

#### [M-5] 缺乏 Base URL 的 HTTPS 强制校验

**文件**: `openai.rs:49-61`

```rust
pub fn custom(
    base_url: impl Into<String>,
    api_key: impl Into<String>,
    model: impl Into<String>,
) -> Self {
    Self {
        base_url: base_url.into(),  // ← 未验证协议
        // ...
    }
}
```

`custom()` 构造器接受任意 `base_url`，包括 `http://`。API Key 会通过明文 HTTP 发送给任意端点，存在中间人攻击风险。即使在内网场景，也应至少在 debug 模式发出警告。

---

### 低风险 (Severity: LOW)

#### [L-1] `build_headers()` 中 `.unwrap()` 用于 Content-Type

**文件**: `azure.rs:115`, `openai.rs:111`

```rust
reqwest::header::CONTENT_TYPE,
"application/json".parse().unwrap(),  // ← unwrap
```

字面量 `"application/json"` 永远不会解析失败，`.unwrap()` 实际上是安全的。但为了与项目编码规范一致，应改为使用 `HeaderValue::from_static("application/json")` 避免运行时解析并消除 unwrap。

---

#### [L-2] 代码大量重复 — chat_completion 与 chat_completion_with_image

**文件**: `azure.rs:201-358`, `openai.rs:199-356`

`chat_completion` 和 `chat_completion_with_image` 两个方法在 HTTP 请求执行、响应解析、成本记录、内容提取部分几乎完全相同，仅 request 构建逻辑不同。约 80 行代码被直接复制。

这违反 DRY 原则，且当响应解析逻辑需要修改时需要同步多处。可以提取一个私有方法 `fn execute_chat_request(&self, url: &str, request: &AzureChatRequest) -> Result<ChatResponse, LlmError>` 复用。

---

#### [L-3] `AzureResponseFormat::Text` 序列化行为未验证

**文件**: `azure.rs:509-512`, `openai.rs:492-495`

```rust
#[serde(untagged)]
enum AzureResponseFormat {
    Text,                              // ← 空变体
    JsonObject { r#type: String },
}
```

`Text` 是空变体，使用 `#[serde(untagged)]` 时，`Text` 会序列化为 `null`（而非 `{"type": "text"}`）。而 Azure/OpenAI API 期望的是 `{"type": "text"}` 格式的对象。这可能导致服务端接收到意外格式而报错。

建议改为：
```rust
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AzureResponseFormat {
    Text,
    JsonObject,
}
```

---

#### [L-4] MIME 类型未验证即写入 data URI

**文件**: `azure.rs:153-156`, `openai.rs:151-154`

```rust
let image_url = format!(
    "data:{};base64,{}",
    request.image_mime_type, request.image_base64  // ← 未验证
);
```

`image_mime_type` 来自调用方，未经任何验证直接写入 data URI。恶意或错误的 MIME 类型字符串（如包含 `;base64` 注入、换行符等）可能破坏 URI 格式或绕过内容策略检查。

---

## 安全合规总结

| 检查项 | Azure | OpenAI | 状态 |
|--------|-------|--------|------|
| API Key 非硬编码 | 通过配置传入 | 通过配置传入 | PASS |
| 使用 HTTPS | 未强制校验 | 未强制校验 | WARN |
| 请求日志不含 API Key | 只记录 URL | 只记录 URL | PASS |
| 响应日志脱敏 | 全量记录 error body | 全量记录 error body | FAIL |
| 无重试限流保护 | 无重试 | 无重试 | FAIL |
| 无硬编码密钥 | PASS | PASS | PASS |
| 错误信息不泄露内部实现 | 部分泄露 | 部分泄露 | WARN |
| raw_response 内存暴露 | 全量存储 | 全量存储 | WARN |

---

## 必须修复（合并前）

优先级排序：

1. **[H-2]** `openai.rs:96` — pricing 配置被忽略，成本控制完全失效，是功能 bug
2. **[H-1]** `azure.rs:111`, `openai.rs:107` — `.expect()` panic 路径，违反 Rust 编码规范
3. **[L-3]** `azure.rs:509-512`, `openai.rs:492-495` — `ResponseFormat::Text` 序列化错误，API 调用可能失败
4. **[M-2]** `azure.rs:229`, `openai.rs:227` — 错误日志泄露原始响应体

---

## 建议改进（非阻塞）

- 为 `chat_completion` 和 `chat_completion_with_image` 提取共用私有方法消除重复
- 在 `LlmProvider` trait 层面或 `service.rs` 中实现统一的重试中间件
- Azure `health_check` 改为使用非消耗 Token 的端点
- `custom()` 构造器增加 HTTPS 校验或至少 WARN 日志
- `image_mime_type` 增加白名单校验（仅允许 `image/jpeg`, `image/png`, `image/gif`, `image/webp`）

---

*报告生成于 2026-03-19，基于代码快照审查。*
