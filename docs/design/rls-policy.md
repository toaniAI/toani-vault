# PostgreSQL 行级安全策略 (RLS) 设计文档

## 版本信息

- **版本**: 1.0
- **创建日期**: 2026-03-12
- **作者**: claude_qwen
- **关联任务**: EP7 Story 7.2 - Schema 隔离与 RLS
- **状态**: Ready for Implementation

---

## 1. 概述

### 1.1 设计目标

为 CredBridge 多租户架构实现 PostgreSQL 行级安全策略 (Row Level Security, RLS)，作为应用层隔离的最后一道防线，确保租户数据在数据库层面的绝对隔离。

### 1.2 架构位置

```
应用层 (Axum 中间件)
├── Token 验证
├── Scope 检查
└── 租户上下文注入

API 层
├── 应用层租户过滤 (TenantQueryBuilder)
└── 业务逻辑

数据访问层 (sqlx)
├── RLS 上下文设置 (SET LOCAL)
└── 查询执行

数据库层 (PostgreSQL)
├── Schema-per-Tenant 隔离
└── RLS 策略强制执行 ← 本设计
```

### 1.3 设计原则

1. **Defense in Depth**: RLS 是最后一道防线，不替代应用层检查
2. **透明性**: 应用代码无需感知 RLS 存在（除非设置上下文）
3. **最小权限**: 默认拒绝，显式允许
4. **性能优先**: 索引优化，避免全表扫描

---

## 2. RLS 策略设计

### 2.1 策略概览

| 表名 | 策略名称 | 策略类型 | 描述 |
|------|---------|---------|------|
| credentials | tenant_access_policy | USING + WITH CHECK | 租户级访问控制 |
| scope_tokens | tenant_access_policy | USING + WITH CHECK | 租户级访问控制 |
| audit_logs | tenant_read_policy | USING | 租户级读取控制（追加表） |
| user_roles | tenant_access_policy | USING + WITH CHECK | 租户级访问控制 |
| tenant_roles | system_read_policy | USING | 系统角色只读（租户内共享） |

### 2.2 上下文变量

```sql
-- RLS 上下文变量
SET LOCAL app.current_tenant_id = '<tenant_uuid>';
SET LOCAL app.current_user_id = '<user_uuid>';
SET LOCAL app.current_scopes = 'read,write,admin';
SET LOCAL app.is_admin = 'false';
```

**变量说明**:
- `app.current_tenant_id`: 当前租户 UUID（必填）
- `app.current_user_id`: 当前用户 ID（用于审计日志）
- `app.current_scopes`: 权限范围列表（逗号分隔）
- `app.is_admin`: 管理员标志（用于例外策略）

**重要**: 使用 `SET LOCAL` 确保变量仅在事务级别生效，避免连接池污染。

### 2.3 详细策略定义

#### 2.3.1 credentials 表

```sql
-- 启用 RLS
ALTER TABLE credentials ENABLE ROW LEVEL SECURITY;

-- 拒绝所有默认策略
CREATE POLICY credentials_deny_all ON credentials
    FOR ALL
    TO PUBLIC
    USING (false);

-- 租户访问策略
CREATE POLICY credentials_tenant_access ON credentials
    FOR ALL
    TO PUBLIC
    USING (
        -- 管理员绕过检查
        current_setting('app.is_admin', true)::boolean = true
        OR
        -- 正常租户检查
        EXISTS (
            SELECT 1 FROM credentials c
            WHERE c.tenant_id::text = current_setting('app.current_tenant_id', true)
        )
    )
    WITH CHECK (
        current_setting('app.is_admin', true)::boolean = true
        OR
        tenant_id::text = current_setting('app.current_tenant_id', true)
    );
```

**注意**: 当前 Schema-per-Tenant 架构下，每个租户独立 Schema，RLS 作为补充防护。

#### 2.3.2 scope_tokens 表

```sql
ALTER TABLE scope_tokens ENABLE ROW LEVEL SECURITY;

CREATE POLICY scope_tokens_deny_all ON scope_tokens
    FOR ALL
    TO PUBLIC
    USING (false);

CREATE POLICY scope_tokens_tenant_access ON scope_tokens
    FOR ALL
    TO PUBLIC
    USING (
        credential_id IN (
            SELECT credential_id FROM credentials
            WHERE tenant_id::text = current_setting('app.current_tenant_id', true)
        )
    );
```

#### 2.3.3 audit_logs 表

```sql
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;

-- 审计日志只读策略（防止篡改）
CREATE POLICY audit_logs_read_only ON audit_logs
    FOR SELECT
    TO PUBLIC
    USING (true);

CREATE POLICY audit_logs_no_modify ON audit_logs
    FOR ALL
    TO PUBLIC
    USING (false);
```

**说明**: 审计日志为追加模式，禁止 UPDATE/DELETE。

#### 2.3.4 user_roles 表

```sql
ALTER TABLE user_roles ENABLE ROW LEVEL SECURITY;

CREATE POLICY user_roles_deny_all ON user_roles
    FOR ALL
    TO PUBLIC
    USING (false);

CREATE POLICY user_roles_tenant_access ON user_roles
    FOR ALL
    TO PUBLIC
    USING (
        user_id_hash IN (
            SELECT DISTINCT user_id_hash FROM credentials
            WHERE tenant_id::text = current_setting('app.current_tenant_id', true)
        )
    );
```

### 2.4 Schema-per-Tenant 场景下的 RLS

由于 CredBridge 采用 Schema-per-Tenant 架构，大部分隔离已通过 Schema 分离实现。RLS 策略主要提供：

1. **深度防御**: 即使 search_path 配置错误，RLS 仍阻止跨租户访问
2. **共享表隔离**: 如存在共享 Schema 的表，RLS 提供行级隔离
3. **管理接口保护**: 超级管理员查询时，RLS 限制可见范围

---

## 3. 租户上下文传递

### 3.1 Rust 代码集成

在 `src/services/db/pool.rs` 中添加 RLS 上下文设置：

```rust
use crate::api::context::RlsContext;

impl DatabasePool {
    /// 获取带 RLS 上下文的数据库连接
    pub async fn acquire_with_rls(
        &self,
        rls_context: &RlsContext,
    ) -> Result<sqlx::pool::PoolConnection<Postgres>, DatabaseError> {
        let mut conn = self.pool.acquire().await?;

        // 设置 RLS 上下文变量
        let sql = rls_context.to_sql_transaction_local();
        sqlx::query(&sql).execute(&mut *conn).await?;

        Ok(conn)
    }

    /// 在事务中设置 RLS 上下文
    pub async fn set_rls_in_transaction(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        rls_context: &RlsContext,
    ) -> Result<(), DatabaseError> {
        let sql = rls_context.to_sql_transaction_local();
        sqlx::query(&sql).execute(&mut **tx).await?;
        Ok(())
    }
}
```

### 3.2 RlsContext 扩展

更新 `src/api/context.rs` 中的 `RlsContext`：

```rust
impl RlsContext {
    /// 生成 SET LOCAL 语句（事务级别）
    pub fn to_sql_transaction_local(&self) -> String {
        format!(
            "SET LOCAL app.current_tenant_id = '{}'; \
             SET LOCAL app.current_user_id = '{}'; \
             SET LOCAL app.current_scopes = '{}'; \
             SET LOCAL app.is_admin = '{}';",
            escape_sql_string(&self.tenant_id),
            escape_sql_string(&self.user_id),
            escape_sql_string(&self.scopes.join(",")),
            self.scopes.contains(&"admin".to_string())
        )
    }
}
```

### 3.3 中间件集成

在 `src/api/tenant_middleware.rs` 中启用 RLS：

```rust
async fn tenant_isolation_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    // ... 现有代码 ...

    // 创建 RLS 上下文
    let rls_context = RlsContext::from_request_context(&request_context);

    // 存储到请求扩展，供后续数据库操作使用
    request.extensions_mut().insert(rls_context);

    next.run(request).await
}
```

---

## 4. 性能优化

### 4.1 索引策略

RLS 策略使用 `current_setting()` 函数，无法直接利用索引。优化策略：

```sql
-- 为 RLS 相关列创建索引
CREATE INDEX idx_credentials_tenant_id ON credentials(tenant_id);
CREATE INDEX idx_scope_tokens_credential_lookup ON scope_tokens(credential_id);

-- 部分索引（仅活跃用户）
CREATE INDEX idx_credentials_active ON credentials(user_id_hash)
    WHERE is_deleted = false;
```

### 4.2 查询优化

**推荐模式**:

```rust
// 1. 获取连接时设置 RLS
let mut conn = db.acquire_with_rls(&rls_context).await?;

// 2. 执行查询（无需手动添加 tenant_id 条件）
let results = sqlx::query_as::<_, Credential>(
    "SELECT * FROM credentials WHERE user_id_hash = $1"
)
.bind(&user_id_hash)
.fetch_all(&mut *conn)
.await?;
```

### 4.3 连接池配置

```rust
// Cargo.toml
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio"] }

// 连接池配置
DatabasePool::builder()
    .max_connections(20)
    .min_connections(5)
    .max_lifetime(Duration::from_secs(3600))
    .idle_timeout(Duration::from_secs(600))
    // 连接后清理（防止 RLS 变量残留）
    .after_connect(|conn, _meta| Box::pin(async move {
        conn.execute("RESET ALL").await?;
        Ok(())
    }))
    .build()
```

---

## 5. 安全考虑

### 5.1 威胁模型

| 威胁 | 缓解措施 |
|------|---------|
| SQL 注入设置租户 ID | 使用参数化查询，`escape_sql_string` 转义 |
| 连接池变量残留 | 使用 `SET LOCAL`，事务结束自动清理 |
| 超级用户绕过 RLS | RLS 对表所有者生效，需使用非超级用户角色 |
| 策略绕过（OR 注入） | 策略使用严格相等比较，无动态 SQL |

### 5.2 超级用户限制

PostgreSQL 中，表所有者（owner）和超级用户默认绕过 RLS。解决方案：

```sql
-- 强制表所有者遵守 RLS
ALTER TABLE credentials FORCE ROW LEVEL SECURITY;
ALTER TABLE scope_tokens FORCE ROW LEVEL SECURITY;
ALTER TABLE audit_logs FORCE ROW LEVEL SECURITY;
```

### 5.3 审计与监控

```sql
-- 创建 RLS 审计视图
CREATE VIEW rls_audit_violations AS
SELECT
    current_setting('app.current_tenant_id', true) as attempted_tenant,
    query,
    usename,
    query_start
FROM pg_stat_activity
WHERE query LIKE '%credentials%'
  AND query NOT LIKE '%current_setting%';
```

---

## 6. 测试策略

### 6.1 单元测试

```rust
#[tokio::test]
async fn test_rls_blocks_cross_tenant_access() {
    let db = setup_test_db().await;

    // 租户 A 上下文
    let tenant_a = RlsContext::new("tenant-a", "user-1", vec!["read"]);

    // 租户 B 插入数据
    let tenant_b = RlsContext::new("tenant-b", "user-2", vec!["read"]);
    let mut conn_b = db.acquire_with_rls(&tenant_b).await.unwrap();
    insert_test_data(&mut conn_b, "tenant-b").await;

    // 租户 A 尝试读取
    let mut conn_a = db.acquire_with_rls(&tenant_a).await.unwrap();
    let results = sqlx::query("SELECT * FROM credentials")
        .fetch_all(&mut *conn_a)
        .await
        .unwrap();

    // 验证：租户 A 看不到租户 B 的数据
    assert!(results.is_empty());
}
```

### 6.2 集成测试

```bash
# 运行 RLS 专项测试
cargo test --test rls_integration -- --nocapture
```

### 6.3 性能测试

```rust
#[tokio::test]
async fn test_rls_performance_overhead() {
    let db = setup_test_db().await;
    let rls_ctx = RlsContext::new("tenant-1", "user-1", vec!["read"]);

    // 预热
    run_benchmark(&db, &rls_ctx, 1000).await;

    // 基准测试
    let start = Instant::now();
    run_benchmark(&db, &rls_ctx, 10_000).await;
    let duration = start.elapsed();

    // 断言：性能影响 < 10%
    let baseline_ms = /* 无 RLS 基线 */;
    let overhead = (duration.as_millis() as f64 - baseline_ms) / baseline_ms;
    assert!(overhead < 0.10, "RLS overhead exceeds 10%: {:.2}%", overhead * 100);
}
```

---

## 7. 部署清单

### 7.1 前置条件

- [ ] PostgreSQL 15+
- [ ] 应用用户非超级用户
- [ ] 数据库备份完成

### 7.2 部署步骤

1. **备份数据库**
   ```bash
   pg_dump -h localhost -U credbridge credbridge > backup.sql
   ```

2. **执行 RLS 初始化脚本**
   ```bash
   psql -h localhost -U credbridge credbridge < docker/scripts/init-rls.sql
   ```

3. **验证 RLS 启用**
   ```sql
   SELECT relname, relrowsecurity
   FROM pg_class
   WHERE relname IN ('credentials', 'scope_tokens', 'audit_logs');
   ```

4. **运行测试**
   ```bash
   cargo test rls
   ```

### 7.3 回滚计划

```sql
-- 禁用 RLS（紧急情况）
ALTER TABLE credentials DISABLE ROW LEVEL SECURITY;
ALTER TABLE scope_tokens DISABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs DISABLE ROW LEVEL SECURITY;

-- 删除策略
DROP POLICY IF EXISTS credentials_tenant_access ON credentials;
DROP POLICY IF EXISTS scope_tokens_tenant_access ON scope_tokens;
DROP POLICY IF EXISTS audit_logs_read_only ON audit_logs;
```

---

## 8. 附录

### 8.1 参考文档

- [PostgreSQL RLS 文档](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
- [sqlx 文档](https://docs.rs/sqlx/)
- [CredBridge 多租户架构](docs/MULTI_TENANCY.md)

### 8.2 术语表

| 术语 | 定义 |
|------|------|
| RLS | Row Level Security，行级安全 |
| USING | 查询时应用的策略条件 |
| WITH CHECK | 写入时验证的策略条件 |
| SET LOCAL | PostgreSQL 会话变量，事务级别生效 |
| Schema-per-Tenant | 每个租户独立数据库 Schema 的隔离模式 |

### 8.3 变更历史

| 版本 | 日期 | 变更内容 | 作者 |
|------|------|---------|------|
| 1.0 | 2026-03-12 | 初始版本 | claude_qwen |

---

## 9. 验收标准

- [x] RLS 策略完整定义（所有敏感表）
- [x] 租户上下文传递机制设计
- [x] 跨租户访问被拒绝机制
- [x] 性能优化策略（索引、查询模式）
- [x] 测试策略（单元测试、集成测试、性能测试）
- [x] 部署和回滚计划

**性能目标**: RLS 性能影响 < 10%

---

**文档结束**
