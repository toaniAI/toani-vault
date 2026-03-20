---
title: 'Vault 后端：updated_at 时间戳修复 + 租户索引实现'
slug: 'vault-backend-updated-at-and-tenant-index'
created: '2026-03-19'
status: 'implemented'
priority: 'P2'
---

# Tech-Spec: Vault 后端两处简化处理修复

## 概述

`src/vault/backend.rs` 存在两处独立的简化处理，合并为一个规格文档，因为两者都在同一文件，可以一次 PR 修复。

---

## 问题 1：`updated_at` 使用 `created_at` 值（低优先级）

### 问题陈述

在将 `VaultCredentialData` 转换为 `CredentialMetadata` 时，`updated_at` 字段被错误地赋值为 `created_at` 的值：

```rust
// src/vault/backend.rs:147-148
created_at: data.created_at,
updated_at: data.created_at, // 简化处理
```

这导致所有从 Vault 读取的凭证，`updated_at` 始终等于 `created_at`，无法反映凭证实际最后更新时间。

### 解决方案

`VaultCredentialData`（从 Vault KV v2 读取的数据结构）中需要包含 `updated_at` 字段。Vault KV v2 的 metadata API 提供了 `updated_time` 字段，应在读取时同时获取并映射。

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/vault/backend.rs` | 主要修改目标（148 行） |
| `src/vault/client.rs` | `VaultCredentialData` 结构体定义 |
| `src/vault/models.rs` | Vault 数据模型 |
| `src/models/credential.rs` | `CredentialMetadata` 结构体定义 |

### 实现计划

**任务 1**：检查 `VaultCredentialData` 结构体（`src/vault/client.rs`），确认是否已有 `updated_at` 或 `updated_time` 字段。

**任务 2**：若 `VaultCredentialData` 缺少 `updated_at`，在结构体中添加：
```rust
pub updated_at: Option<DateTime<Utc>>,  // 来自 Vault KV v2 metadata
```

**任务 3**：更新 Vault KV v2 读取逻辑，从 metadata 中提取 `updated_time`（Vault 返回字段名），映射到 `VaultCredentialData::updated_at`。

**任务 4**：修改 `backend.rs` 第 148 行：
```rust
// 修改前
updated_at: data.created_at, // 简化处理

// 修改后
updated_at: data.updated_at.unwrap_or(data.created_at),
```

### 验收标准

- [ ] 更新凭证后，`updated_at` 时间戳与 `created_at` 不同
- [ ] 若 Vault metadata 无 `updated_time`，降级使用 `created_at`（不 panic）
- [ ] 现有测试不受影响

---

## 问题 2：`list_all_tenants()` 租户索引简化实现（中优先级）

### 问题陈述

`list_all_tenants()` 方法直接调用 Vault 的 list API 列举路径，依赖 Vault 路径结构来推断租户列表：

```rust
// src/vault/backend.rs:401-408
impl VaultStorageBackend {
    /// 列出所有租户 ID（内部方法）
    fn list_all_tenants(&self) -> Result<Vec<String>, VaultBackendError> {
        // 从 Vault 列出 credbridge/ 下的所有子目录
        // 这是一个简化的实现，实际应该维护租户索引
        self.block_on(self.client.list_secrets(""))
    }
}
```

**问题**：
1. `list_secrets("")` 列出的是 Vault 根路径下的条目，不一定是租户 ID，可能包含其他非租户路径
2. 随着凭证数量增长，遍历所有路径成本高
3. 如果某个租户没有任何凭证，其 ID 不会出现在列表中（即使租户存在）
4. 调用方依赖此方法的功能（批量操作、审计）会得到不完整或错误的结果

### 解决方案

维护一个独立的租户索引，存储在 Vault 的固定路径（如 `credbridge/_index/tenants`）或 PostgreSQL 数据库中。租户注册时写入索引，注销时删除。

**推荐方案**：使用 Vault KV 存储租户索引（`credbridge/_index/tenants`），存储一个 JSON 数组或哈希表，包含所有已注册租户 ID。这样不依赖外部数据库，保持 Vault 作为单一数据源。

**备选方案**：若 PostgreSQL 可访问，从 `tenants` 表查询更可靠（租户表是权威来源）。结合项目架构分析，推荐使用数据库查询方式，因为 CredBridge 的租户生命周期由数据库管理。

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/vault/backend.rs` | 主要修改目标（403-407 行） |
| `src/vault/client.rs` | `VaultKvClient` 的 `list_secrets` 实现 |
| `src/vault/models.rs` | Vault 数据模型 |
| `src/tenant/config.rs` | 租户配置存储逻辑，了解租户生命周期 |
| `migrations/` | 数据库 schema，确认 `tenants` 表结构 |

### 实现计划

**方案 A（Vault 索引，推荐）**：

**任务 1**：在 `VaultKvClient` 中添加索引读写方法：
- `read_tenant_index() -> Result<Vec<String>, VaultClientError>`
- `write_tenant_index(tenants: &[String]) -> Result<(), VaultClientError>`

**任务 2**：在租户注册（`add_tenant`）和注销（`remove_tenant`）流程中，更新 Vault 中的租户索引。

**任务 3**：修改 `list_all_tenants()`：
```rust
fn list_all_tenants(&self) -> Result<Vec<String>, VaultBackendError> {
    self.block_on(self.client.read_tenant_index())
        .map_err(VaultBackendError::from)
}
```

**任务 4**：为现有数据提供迁移路径：扫描 Vault 当前路径重建索引（一次性迁移脚本）。

### 验收标准

- [ ] `list_all_tenants()` 返回所有已注册租户的完整列表
- [ ] 即使某个租户当前没有凭证，仍出现在列表中
- [ ] 租户注册/注销时索引正确更新
- [ ] 索引写入失败时有明确的错误日志和处理
- [ ] 现有凭证 CRUD 操作不受影响

---

## 附加上下文

### 依赖关系

- 问题 1（`updated_at`）：独立，可单独修复
- 问题 2（租户索引）：需要了解租户生命周期管理流程，协调 tenant 模块

### 测试策略

**问题 1**：
- 单元测试：mock `VaultCredentialData`，验证 `updated_at != created_at`
- 集成测试：更新凭证后验证 `updated_at` 时间戳更新

**问题 2**：
- 单元测试：mock `VaultKvClient`，验证 `list_all_tenants()` 返回索引内容
- 集成测试：注册租户后立即 `list_all_tenants()`，验证出现在列表中

### 注意事项

- 问题 2 中 `block_on` 是一个同步包装器（阻塞当前线程执行异步代码），潜在的性能问题；在实现索引时，若有机会，可以将方法改为异步版本（但这属于更大重构范围，本规格不要求）
- Vault KV v2 的 `updated_time` 字段格式为 RFC3339 字符串，需要正确解析
