---
title: '数据库 RLS 策略设计'
slug: 'ep7-story7.2-rls-policy'
created: '2026-03-12'
status: 'in-progress'
stepsCompleted: [1, 2, 3, 4]
tech_stack: ['PostgreSQL', 'sqlx', 'Row Level Security']
files_to_modify: ['src/services/db/schema.rs', 'docker/scripts/init-rls.sql', 'docs/design/rls-policy.md']
code_patterns: ['RLS policies', 'SET LOCAL context', 'tenant isolation']
test_patterns: ['RLS policy tests', 'cross-tenant access denial']
---

# Tech-Spec: 数据库 RLS 策略设计

**Created:** 2026-03-12

## Overview

### Problem Statement

EP7 Story 7.2 Schema 隔离与 RLS 在 E2E 测试中被标记为**部分实现**。当前架构采用 Schema-per-Tenant + 应用层过滤，但数据库层 RLS 策略未创建，缺少最后一道防线（Defense in Depth）。

### Solution

设计并实现 PostgreSQL 行级安全策略 (RLS)，作为多租户隔离的数据库层保障：
1. 在租户 Schema 内启用 RLS
2. 创建租户隔离策略
3. 通过 `SET LOCAL` 在事务级别传递租户上下文
4. 为敏感表创建用户级隔离策略

### Scope

**In Scope:**
- RLS 策略 SQL 设计（`ALTER TABLE ... ENABLE ROW LEVEL SECURITY`）
- `CREATE POLICY` 语句定义
- 租户上下文传递机制（`SET LOCAL app.current_tenant`）
- 连接池集成方案
- 性能优化策略
- 测试策略

**Out of Scope:**
- Schema-per-Tenant 架构修改
- 应用层租户过滤逻辑变更
- Token 认证机制修改

## Context for Development

### Codebase Patterns

**Schema 管理** (`src/services/db/schema.rs`):
- `SchemaManager` 负责租户 Schema 创建
- `TENANT_SCHEMA_SQL` 定义表结构
- 使用 `SET search_path TO tenant_{uuid}` 切换 Schema

**租户上下文** (`src/api/context.rs`):
- `RlsContext` 结构已定义
- `to_sql_statements()` 生成 `SET app.current_*` 语句
- `TenantQueryBuilder` 自动添加 `tenant_id` 条件

**中间件** (`src/api/tenant_middleware.rs`):
- `tenant_isolation_middleware` 提取租户信息
- `validate_path_tenant_id` 验证跨租户访问
- 配置支持 `enable_rls_context`

### Files to Reference

| File | Purpose |
| ---- | ------- |
| `src/services/db/schema.rs` | Schema 管理，表结构定义 |
| `src/api/context.rs` | RLS 上下文定义 |
| `src/api/tenant_middleware.rs` | 租户隔离中间件 |
| `docker/scripts/init-postgres.sql` | 数据库初始化脚本 |
| `docs/MULTI_TENANCY.md` | 多租户架构文档 |

### Technical Decisions

1. **RLS 策略类型**:
   - `USING` 策略：控制读操作
   - `WITH CHECK` 策略：控制写操作

2. **上下文传递**:
   - 使用 `SET LOCAL`（事务级别）而非 `SET`（会话级别）
   - 避免连接池污染

3. **策略命名**:
   - `tenant_isolation`: 租户级隔离
   - `user_isolation`: 用户级隔离（可选）
   - `admin_bypass`: 管理员例外

## Implementation Plan

### Tasks

1. 设计 RLS 策略 SQL
2. 创建 `docker/scripts/init-rls.sql` 脚本
3. 更新 `src/services/db/schema.rs` 在创建 Schema 时执行 RLS
4. 集成 RLS 上下文到数据库连接
5. 编写 RLS 测试用例

### Acceptance Criteria

**Given** 租户 Schema 已创建
**When** 查询租户数据
**Then** RLS 策略自动过滤非本租户数据

**Given** 跨租户访问尝试
**When** 执行 SQL 查询
**Then** 数据库返回空结果（策略拒绝）

**Given** `SET LOCAL app.current_tenant` 已设置
**When** 执行查询
**Then** 仅返回匹配租户的数据

**Given** 性能基准测试
**When** 对比 RLS 开启前后
**Then** 性能影响 < 10%

## Additional Context

### Dependencies

- PostgreSQL 15+ (RLS 功能)
- sqlx 连接池支持
- `RlsContext` 结构（已存在）

### Testing Strategy

1. **单元测试**: RLS SQL 生成
2. **集成测试**: 跨租户访问拒绝
3. **性能测试**: RLS 开销基准

### Notes

- RLS 是**最后一道防线**，不应替代应用层检查
- 使用 `SET LOCAL` 确保事务结束后自动清理
- 索引优化对 RLS 性能至关重要
