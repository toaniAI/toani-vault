# CredBridge 架构和安全问题清单

## 验证概述

- **验证日期**: 2026-03-11
- **验证人员**: claude_qwen (CTO/架构师/安全负责人)
- **文档状态**: 试用阶段 - 待修复

---

## 问题统计

| 优先级 | 数量 | 状态 |
|--------|------|------|
| 🔴 高优先级 | 2 | 待修复 |
| 🟡 中优先级 | 2 | 待修复 |
| 🟢 低优先级 | 3 | 建议优化 |

---

## 🔴 高优先级问题 (必须修复)

### ISSUE-001: 缺少 API 速率限制

**严重程度**: 🔴 High
**类型**: 安全 - DoS 防护
**影响**: 系统可能遭受暴力破解或 DoS 攻击

**问题描述**:
项目中未实现 API 速率限制，攻击者可以进行无限次的：
- Token 验证尝试
- 凭证解密尝试
- 凭证创建操作

**相关代码位置**:
```rust
// src/api/routes.rs - 路由定义处缺少速率限制中间件
pub fn routes() -> Router {
    Router::new()
        .route("/credentials", post(create_credential))
        .route("/credentials/:id/decrypt", post(decrypt_credential))
        // 缺少 rate_limit 中间件
}
```

**修复建议**:
```rust
use tower::limit::RateLimitLayer;

pub fn routes() -> Router {
    let rate_limit = RateLimitLayer::new(
        100, // 每秒请求数
        Duration::from_secs(1),
    );

    Router::new()
        .route("/credentials", post(create_credential))
        .layer(rate_limit)
}
```

**验证方式**:
- 使用 `wrk` 或 `ab` 进行压力测试
- 验证超过阈值后返回 429 Too Many Requests

---

### ISSUE-002: Debug 日志可能泄露敏感信息

**严重程度**: 🔴 High
**类型**: 安全 - 信息泄露
**影响**: 敏感信息可能写入日志文件

**问题描述**:
多处代码使用 `eprintln!` 输出调试信息，可能包含密钥派生上下文、内部状态等敏感信息。

**问题代码**:
```rust
// src/tee/enclave.rs:254
if self.config.debug_mode {
    eprintln!("密封存储恢复跳过: {}", e);  // 可能包含路径信息
}

// src/tee/keys.rs:547
if self.config.debug_mode {
    eprintln!("从密封存储恢复主密钥成功");
}
```

**修复建议**:
1. 使用结构化日志库（如 `tracing`）
2. 区分日志级别（debug/info/warn/error）
3. 确保敏感信息只在 debug 级别输出，生产环境使用 info 级别

```rust
// 修复后
use tracing::{debug, info, warn, error};

if let Err(e) = self.restore_from_sealed_storage() {
    debug!(error = %e, "密封存储恢复跳过");  // 仅在 debug 级别输出
}
```

**验证方式**:
- 审查所有 `eprintln!` 和 `println!` 调用
- 确保生产环境日志不包含敏感信息

---

## 🟡 中优先级问题 (建议修复)

### ISSUE-003: 缺少结构化输入验证

**严重程度**: 🟡 Medium
**类型**: 安全 - 输入验证
**影响**: 可能导致无效数据存储或处理异常

**问题描述**:
API 请求体缺乏结构化验证，仅依赖基本类型反序列化。

**问题代码**:
```rust
// src/api/credentials.rs:116
#[derive(Debug, Deserialize)]
pub struct CreateCredentialApiRequest {
    pub service_id: String,  // 缺少长度限制
    pub credential_type: CredentialType,
    pub plaintext_data: serde_json::Value,  // 缺少大小限制
    pub expires_at: Option<u64>,  // 缺少范围验证
}
```

**修复建议**:
```rust
use validator::{Validate, ValidationError};

#[derive(Debug, Deserialize, Validate)]
pub struct CreateCredentialApiRequest {
    #[validate(length(min = 1, max = 100))]
    pub service_id: String,

    pub credential_type: CredentialType,

    // 添加自定义验证器限制 JSON 大小
    #[validate(custom = "validate_json_size")]
    pub plaintext_data: serde_json::Value,

    #[validate(range(min = 0))]
    pub expires_at: Option<u64>,
}

fn validate_json_size(value: &serde_json::Value) -> Result<(), ValidationError> {
    let size = serde_json::to_vec(value).map(|v| v.len()).unwrap_or(0);
    if size > 1024 * 1024 {  // 1MB 限制
        return Err(ValidationError::new("json_too_large"));
    }
    Ok(())
}
```

---

### ISSUE-004: 缺少请求体大小限制

**严重程度**: 🟡 Medium
**类型**: 安全 - DoS 防护
**影响**: 可能遭受大请求体 DoS 攻击

**问题描述**:
未配置 Axum 请求体大小限制，攻击者可发送超大请求体消耗内存。

**修复建议**:
```rust
use axum::extract::DefaultBodyLimit;

let app = Router::new()
    .route("/credentials", post(create_credential))
    .layer(DefaultBodyLimit::max(1024 * 1024));  // 1MB 限制
```

---

## 🟢 低优先级问题 (建议优化)

### ISSUE-005: 缺少安全响应头

**严重程度**: 🟢 Low
**类型**: 安全 - HTTP 头
**影响**: 降低浏览器安全策略保护

**问题描述**:
未配置安全相关的 HTTP 响应头。

**修复建议**:
```rust
use tower_http::add_extension::AddExtensionLayer;
use tower_http::cors::CorsLayer;

let app = Router::new()
    .layer(
        CorsLayer::new()
            .allow_origin(["https://trusted-domain.com".parse().unwrap()])
    )
    // 添加安全头中间件
    .layer(tower_http::set_header::SetResponseHeaderLayer::if_not_present(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    ));
```

---

### ISSUE-006: Token 黑名单检查非原子操作

**严重程度**: 🟢 Low
**类型**: 安全 - 竞态条件
**影响**: 极低概率的重放攻击窗口

**问题描述**:
Token 验证和加入黑名单是两个独立操作，存在极小时间窗口可能重放。

**问题代码**:
```rust
// src/api/middleware.rs:169-187
match token_store.is_blacklisted(&validation_result.token_id).await {
    Ok(true) => return Err(...),
    Ok(false) => {}
    Err(e) => { /* 记录错误但不阻止 */ }
}

// 记录 jti 已使用
if let Err(e) = token_store.blacklist_token(&validation_result.token_id, ...).await {
    eprintln!("Token 添加到黑名单错误: {}", e);
}
```

**修复建议**:
使用 Redis Lua 脚本实现原子检查和设置：
```rust
// 使用 Redis Lua 脚本实现原子操作
const CHECK_AND_SET_SCRIPT: &str = r#"
    if redis.call('exists', KEYS[1]) == 1 then
        return {err="token already used"}
    end
    redis.call('setex', KEYS[1], ARGV[1], '1')
    return 'OK'
"#;
```

**风险评估**:
- 实际风险极低（网络延迟通常在毫秒级）
- Token 本身有 15 分钟有效期限制
- 标记为低优先级

---

### ISSUE-007: 缺少异常行为检测

**严重程度**: 🟢 Low
**类型**: 安全 - 监控告警
**影响**: 无法及时发现攻击行为

**问题描述**:
未实现异常行为检测和实时告警机制。

**建议实现**:
```rust
// 异常检测中间件
pub async fn anomaly_detection_middleware(
    Extension(context): Extension<RequestContext>,
    request: Request,
    next: Next,
) -> Response {
    // 记录请求频率
    // 检测异常模式：
    // - 短时间内大量解密请求
    // - 异常时间访问（凌晨）
    // - 来自新 IP 的高风险操作

    if is_anomalous(&context) {
        alert_security_team(&context).await;
    }

    next.run(request).await
}
```

---

## 修复跟踪

| Issue ID | 优先级 | 状态 | 负责人 | 预计修复时间 |
|----------|--------|------|--------|-------------|
| ISSUE-001 | 🔴 高 | 待修复 | - | 1 天 |
| ISSUE-002 | 🔴 高 | 待修复 | - | 0.5 天 |
| ISSUE-003 | 🟡 中 | 待修复 | - | 1 天 |
| ISSUE-004 | 🟡 中 | 待修复 | - | 0.5 天 |
| ISSUE-005 | 🟢 低 | 待修复 | - | 0.5 天 |
| ISSUE-006 | 🟢 低 | 待修复 | - | 1 天 |
| ISSUE-007 | 🟢 低 | 待修复 | - | 2 天 |

---

## 重新验证清单

修复完成后，需要重新验证以下项目：

- [ ] ISSUE-001: 速率限制功能测试
- [ ] ISSUE-002: 日志审查，确认无敏感信息
- [ ] ISSUE-003: 输入验证测试（边界值、异常输入）
- [ ] ISSUE-004: 大请求体拒绝测试
- [ ] 全量回归测试
- [ ] 渗透测试（可选）

---

## 试用结论

**当前状态**: ⚠️ **有条件通过**

**结论说明**:
- 架构设计 ✅ 通过
- 安全设计 ✅ 核心安全机制正确
- 待修复: 2个高优先级问题

**建议**:
1. 修复 ISSUE-001 和 ISSUE-002 后重新验证
2. 中低优先级问题可在后续迭代中修复
3. 修复完成后建议进行渗透测试

**最终试用结论**: 🔶 **有条件通过 - 待修复高优先级问题**
