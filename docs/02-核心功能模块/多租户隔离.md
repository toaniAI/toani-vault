# CredBridge 多租户架构设计文档

## 概述

CredBridge 采用 **Schema-per-Tenant + RLS（行级安全）** 的多租户架构，确保租户数据的强隔离。这是 CredBridge 安全模型的核心组件之一。

## 架构决策

### DA-001: Schema-per-Tenant + RLS 混合方案

**决策**: 采用 Schema-per-Tenant + RLS 混合方案实现多租户隔离

**选择理由**:
- 凭证数据敏感性高，需要强隔离
- B2B 客户有合规需求（数据隔离）
- 单租户备份/恢复便利
- 与 TEE 安全理念一致

**实施方案**:
- 每个租户独立 Schema
- Schema 内启用 PostgreSQL RLS（行级安全）
- 共享连接池，Schema 动态切换
- TEE Enclave 内执行敏感操作

## 核心组件

### 1. 租户隔离中间件 (`tenant_middleware.rs`)

```rust
// 中间件自动从 Token 中提取 tenant_id
pub async fn tenant_isolation_middleware(
    State(state): State<TenantIsolationState>,
    mut request: Request,
    next: Next,
) -> Response {
    // 1. 提取 ValidatedToken
    // 2. 创建 RequestContext
    // 3. 注入到请求扩展
    // 4. 继续处理
}
```

**功能**:
- 从 PASETO Token 中提取 `tenant_id`
- 创建 `RequestContext` 并注入到请求
- 验证租户隔离（阻止跨租户访问）
- 支持公开路径绕过检查

### 2. 请求上下文 (`context.rs`)

```rust
pub struct RequestContext {
    pub tenant_id: String,
    pub user_id: String,
    pub token_id: String,
    pub scopes: Vec<String>,
    pub request_id: String,
}
```

**功能**:
- 存储当前请求的租户信息
- 提供 scope 权限检查
- 支持 Axum 提取器模式 (`FromRequestParts`)

### 3. 租户ID提取器

```rust
// 在处理器中直接提取租户ID
async fn handler(
    TenantId(tenant_id): TenantId,
) -> impl IntoResponse {
    // tenant_id 自动从 Token 中提取
}
```

## 跨租户访问防护

### 路径参数验证

```rust
// 验证 URL 路径中的租户ID
pub fn validate_path_tenant_id(
    ctx: &RequestContext,
    path_tenant_id: &str,
) -> Result<(), TenantIsolationError> {
    if ctx.tenant_id() != path_tenant_id {
        return Err(TenantIsolationError::CrossTenantAccessDenied {
            requested: ctx.tenant_id().to_string(),
            actual: path_tenant_id.to_string(),
        });
    }
    Ok(())
}
```

### 响应格式

跨租户访问被拒绝时返回 `403 Forbidden`:

```json
{
  "success": false,
  "error": {
    "code": "CROSS_TENANT_ACCESS_DENIED",
    "message": "跨租户访问被拒绝: 请求租户 'tenant_a' 不匹配资源租户 'tenant_b'"
  },
  "meta": {
    "request_id": "req_abc123",
    "timestamp": "2026-03-11T08:30:00Z"
  }
}
```

## 数据库 RLS（行级安全）

### RLS 上下文

```rust
pub struct RlsContext {
    pub tenant_id: String,
    pub user_id: String,
    pub scopes: Vec<String>,
}

impl RlsContext {
    pub fn to_sql_statements(&self) -> Vec<String> {
        vec![
            "SET app.current_tenant_id = '{}'",
            "SET app.current_user_id = '{}'",
            "SET app.current_scopes = '{}'",
        ]
    }
}
```

### PostgreSQL RLS 策略示例

```sql
-- 启用 RLS
ALTER TABLE credentials ENABLE ROW LEVEL SECURITY;

-- 创建策略
CREATE POLICY tenant_isolation_policy ON credentials
    USING (tenant_id = current_setting('app.current_tenant_id')::TEXT);
```

## 使用示例

### 基本使用

```rust
use vault_service::api::{
    TenantId, RequestContext,
    tenant_isolation_middleware,
};

// 在路由中应用中间件
let app = Router::new()
    .route("/credentials", get(list_credentials))
    .route_layer(middleware::from_fn_with_state(
        state,
        tenant_isolation_middleware,
    ));

// 在处理器中提取租户ID
async fn list_credentials(
    TenantId(tenant_id): TenantId,
) -> impl IntoResponse {
    // tenant_id 自动从 Token 中提取
    format!("Listing credentials for tenant: {}", tenant_id)
}
```

### 完整上下文提取

```rust
async fn create_credential(
    context: RequestContext,
    Json(payload): Json<CreateCredentialRequest>,
) -> impl IntoResponse {
    // 检查权限
    if !context.has_scope("credential:write") {
        return StatusCode::FORBIDDEN;
    }

    // 使用 tenant_id
    let tenant_id = context.tenant_id();
    // ...
}
```

### 跨租户访问检查

```rust
async fn get_credential(
    axum::extract::Path((path_tenant_id, credential_id)): Path<(String, String)>,
    context: RequestContext,
) -> Result<impl IntoResponse, StatusCode> {
    // 验证路径中的租户ID
    validate_path_tenant_id(&context, &path_tenant_id)
        .map_err(|_| StatusCode::FORBIDDEN)?;

    // 继续处理...
}
```

## 配置选项

### 中间件配置

```rust
pub struct TenantIsolationConfig {
    /// 是否启用跨租户访问检查
    pub enable_cross_tenant_check: bool,
    /// 是否启用租户激活状态检查
    pub enable_tenant_active_check: bool,
    /// 是否自动注入 RLS 上下文
    pub enable_rls_context: bool,
    /// 允许的 URL 路径（不需要租户隔离）
    pub public_paths: Vec<String>,
}
```

### 预定义配置

```rust
// 生产环境配置
let config = TenantIsolationConfig::production();

// 测试环境配置
let config = TenantIsolationConfig::testing();

// 自定义配置
let state = TenantMiddlewareBuilder::new()
    .enable_cross_tenant_check()
    .add_public_path("/webhook")
    .build();
```

## 测试

### 运行租户隔离测试

```bash
cargo test --test tenant_middleware_tests
```

### 测试覆盖

- ✅ 从 Token 提取租户上下文
- ✅ 跨租户访问被拒绝（403 Forbidden）
- ✅ 同租户访问允许（200 OK）
- ✅ 公开路径绕过检查
- ✅ SQL 注入防护
- ✅ RLS SQL 生成
- ✅ Scope 权限检查
- ✅ Admin scope 拥有所有权限

## 安全最佳实践

1. **始终验证路径参数中的租户ID**
   ```rust
   validate_path_tenant_id(&context, &path_tenant_id)?;
   ```

2. **使用 RLS 作为最后防线**
   - 中间件检查 + 数据库 RLS 双重保障

3. **定期审计租户访问日志**
   - 检查跨租户访问尝试
   - 监控异常行为

4. **租户ID格式验证**
   - 使用 `TenantId` 提取器确保格式正确

## 相关文档

- [架构设计](./architecture.md)
- [安全架构](./SECURITY.md)
- [API 文档](./API.md)

## 变更日志

### 2026-03-11
- 实现租户隔离中间件
- 添加请求上下文模块
- 实现跨租户访问防护
- 添加 RLS 上下文支持
- 编写集成测试（15个测试全部通过）
