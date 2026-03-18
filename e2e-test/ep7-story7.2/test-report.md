# EP7 Story 7.2: Schema 隔离与 RLS 测试报告

## 测试信息
- **Story ID**: 7.2
- **测试日期**: 2026-03-12
- **测试人员**: claude_kimi
- **测试状态**: ⚠️ PARTIAL (Schema 隔离实现，RLS 策略缺失)

---

## 1. 操作留档

### 1.1 检查 Schema 管理器
```bash
grep -n "SET search_path" /Users/yvan/AIWorkspace/credbridge/src/services/db/schema.rs
grep -n "CREATE POLICY\|ENABLE ROW LEVEL" /Users/yvan/AIWorkspace/credbridge/src/services/db/schema.rs
```
**结果**:
- ✅ `SET search_path` 存在（行 160）
- ❌ `CREATE POLICY` 不存在
- ❌ `ENABLE ROW LEVEL` 不存在

### 1.2 代码审查 - Schema 切换
**文件**: `src/services/db/schema.rs:159-180`
```rust
// 设置 search_path
let set_path_sql = format!("SET search_path TO \"{}\"", schema_name);

// 在事务中执行所有 SQL
let mut tx = self.db.pool().begin().await?;

// 设置 search_path
sqlx::query(&set_path_sql)
    .execute(&mut *tx)
    .await?;

// 创建表结构
sqlx::query(TENANT_SCHEMA_SQL)
    .execute(&mut *tx)
    .await?;
```
**状态**: ✅ 动态 Schema 切换已实现

### 1.3 代码审查 - RLS 上下文
**文件**: `src/api/context.rs:169-214`
```rust
pub struct RlsContext {
    pub tenant_id: String,
    pub user_id: String,
    pub scopes: Vec<String>,
}

impl RlsContext {
    pub fn to_sql_statements(&self) -> Vec<String> {
        vec![
            format!("SET app.current_tenant_id = '{}'", ...),
            format!("SET app.current_user_id = '{}'", ...),
            format!("SET app.current_scopes = '{}'", ...),
        ]
    }
}
```
**状态**: ✅ RLS 上下文结构已定义

### 1.4 代码审查 - 跨租户访问检查
**文件**: `src/api/tenant_middleware.rs:235-264`
```rust
pub fn validate_path_tenant_id(
    context: &RequestContext,
    path_tenant_id: &str,
) -> Result<(), TenantIsolationError> {
    if context.tenant_id() != path_tenant_id {
        return Err(TenantIsolationError::CrossTenantAccessDenied {
            requested: context.tenant_id().to_string(),
            actual: path_tenant_id.to_string(),
        });
    }
    Ok(())
}
```
**状态**: ✅ 跨租户访问检查已实现

### 1.5 代码审查 - 租户查询构建器
**文件**: `src/api/context.rs:215-249`
```rust
pub struct TenantQueryBuilder {
    tenant_id: String,
    base_query: String,
}

impl TenantQueryBuilder {
    pub fn build(&self) -> (String, Vec<String>) {
        // 自动添加 WHERE tenant_id = ? 条件
        let query = if self.base_query.to_uppercase().contains("WHERE") {
            format!("{} AND tenant_id = '{}'", ...)
        } else {
            format!("{} WHERE tenant_id = '{}'", ...)
        };
        (query, vec![self.tenant_id.clone()])
    }
}
```
**状态**: ✅ 应用层租户过滤已实现

---

## 2. 数据结果

### 2.1 RLS 实现状态汇总

| 组件 | 实现状态 | 说明 |
|------|----------|------|
| RlsContext 结构 | ✅ | RLS 会话变量上下文 |
| to_sql_statements() | ✅ | 生成 `SET app.current_*` 语句 |
| CREATE POLICY | ❌ | **数据库策略未创建** |
| ALTER TABLE ENABLE RLS | ❌ | **表上未启用 RLS** |
| 连接级 RLS 设置 | ⚠️ | 代码存在但未在连接池中集成 |

### 2.2 Schema 隔离功能汇总

| 功能 | 实现状态 | 说明 |
|------|----------|------|
| Schema 创建 | ✅ | `CREATE SCHEMA IF NOT EXISTS` |
| search_path 切换 | ✅ | `SET search_path TO tenant_{uuid}` |
| 表结构创建 | ✅ | credentials, scope_tokens, audit_logs, tenant_roles |
| 默认角色 | ✅ | admin, user, readonly |
| Schema 删除 | ✅ | `DROP SCHEMA CASCADE` |
| Schema 存在检查 | ✅ | `schema_exists()` |

### 2.3 中间件隔离功能汇总

| 功能 | 实现状态 | 说明 |
|------|----------|------|
| TenantIsolationConfig | ✅ | 中间件配置结构 |
| tenant_isolation_middleware | ✅ | 主隔离中间件 |
| validate_path_tenant_id | ✅ | 路径参数验证 |
| validate_query_tenant_id | ✅ | 查询参数验证 |
| 跨租户访问拦截 | ✅ | 返回 403 Forbidden |
| RequestContextExt | ✅ | 请求扩展 trait |

---

## 3. 操作结果截图

### 3.1 Schema 切换 SQL 截图
**文件**: `src/services/db/schema.rs`
```rust
let set_path_sql = format!("SET search_path TO \"{}\"", schema_name);

sqlx::query(&set_path_sql)
    .execute(&mut *tx)
    .await
    .map_err(|e| DatabaseError::SchemaError(format!("Failed to set search_path: {}", e)))?;
```

### 3.2 跨租户访问检查测试截图
**文件**: `tests/api/tenant_middleware_tests.rs:109-131`
```rust
#[tokio::test]
async fn test_cross_tenant_access_blocked() {
    let app = create_test_router();

    // 创建 Token，租户ID 为 tenant_abc123
    let token = create_test_token("tenant_abc123", "user_xyz789", vec![TokenScope::CredentialRead]);

    // 但请求路径中尝试访问 tenant_different 的资源
    let request = Request::builder()
        .uri("/api/v1/tenants/tenant_different/credentials")
        .body(Body::empty())
        .unwrap();

    // ... 应该返回 403 Forbidden
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
```
**结果**: ✅ 测试通过

### 3.3 RLS 文档示例截图
**文件**: `docs/MULTI_TENANCY.md:139-146`
```sql
-- 启用 RLS
ALTER TABLE credentials ENABLE ROW LEVEL SECURITY;

-- 创建策略
CREATE POLICY tenant_isolation_policy ON credentials
    USING (tenant_id = current_setting('app.current_tenant_id')::TEXT);
```
**状态**: ❌ 仅文档示例，未在代码中实现

---

## 4. 用例结果判断

| 验收项 | 状态 | 备注 |
|--------|------|------|
| SET search_path = tenant_{uuid} | ✅ | 已实现在 SchemaManager 中 |
| 连接池支持多租户 | ⚠️ | 基础连接池存在，但无租户感知的连接分配 |
| RLS 策略：user_id 隔离 | ❌ | **数据库层 RLS 未实现** |
| 跨租户访问被拒绝 | ✅ | 中间件层已实现 |

### 详细分析

#### ✅ 已实现部分
1. **Schema 隔离**: `SET search_path` 动态切换
2. **应用层租户过滤**: `TenantQueryBuilder` 自动添加 tenant_id 条件
3. **跨租户访问检查**: 中间件验证路径/查询参数中的租户ID
4. **RLS 上下文**: `RlsContext` 结构定义了会话变量设置
5. **租户隔离测试**: 跨租户访问测试通过

#### ❌ 未实现部分
1. **数据库层 RLS**: 代码中没有 `CREATE POLICY` 语句
2. **表级 RLS 启用**: 代码中没有 `ALTER TABLE ... ENABLE ROW LEVEL SECURITY`
3. **连接池集成**: RLS 上下文未在数据库连接上自动执行
4. **user_id 隔离**: RLS 策略未定义用户级隔离

---

## 5. 测试结论

**Story 7.2 状态**: ⚠️ **PARTIAL (部分实现)**

### 关键问题
- ❌ 数据库层 RLS 策略未创建（`CREATE POLICY` 缺失）
- ❌ 表级 RLS 未启用（`ENABLE ROW LEVEL SECURITY` 缺失）
- ⚠️ 连接池未集成 RLS 上下文设置

### 已有功能
- ✅ Schema-per-Tenant 完全实现
- ✅ 应用层租户隔离完全实现
- ✅ 跨租户访问检查完全实现

### 架构说明
当前实现采用 **Schema-per-Tenant + 应用层过滤** 的混合方案：
- **Schema 隔离**: 每个租户独立 Schema，通过 `search_path` 切换
- **应用层过滤**: `TenantQueryBuilder` 自动添加 `tenant_id` 条件
- **中间件检查**: `tenant_isolation_middleware` 阻止跨租户请求

**但缺少**: 数据库层 RLS 作为最后防线（Defense in Depth）

---

## 6. 问题追踪

| 问题 ID | 描述 | 严重程度 | 状态 |
|---------|------|----------|------|
| RLS-001 | 数据库 RLS 策略未创建 | 🟡 Medium | Open |
| RLS-002 | 表级 RLS 未启用 | 🟡 Medium | Open |
| RLS-003 | 连接池未集成 RLS 上下文 | 🟡 Medium | Open |
| RLS-004 | user_id 级隔离策略缺失 | 🟢 Low | Open |

### 修复建议

在 `src/services/db/schema.rs` 中添加 RLS SQL：

```sql
-- 在 TENANT_SCHEMA_SQL 后添加：

-- 启用 RLS
ALTER TABLE credentials ENABLE ROW LEVEL SECURITY;
ALTER TABLE scope_tokens ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;

-- 创建租户隔离策略
CREATE POLICY tenant_isolation_policy ON credentials
    USING (tenant_id = current_setting('app.current_tenant_id')::TEXT);

CREATE POLICY tenant_isolation_policy ON scope_tokens
    USING (tenant_id = current_setting('app.current_tenant_id')::TEXT);

-- 创建用户隔离策略（可选）
CREATE POLICY user_isolation_policy ON credentials
    USING (user_id_hash = current_setting('app.current_user_id')::TEXT);
```
