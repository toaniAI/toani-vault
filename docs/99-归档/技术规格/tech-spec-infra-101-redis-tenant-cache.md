---
title: 'INFRA-101: Redis 租户配置缓存实现'
slug: 'infra-101-redis-tenant-config-cache'
created: '2026-03-19'
status: 'implemented'
priority: 'P2'
---

# Tech-Spec: INFRA-101 Redis 租户配置缓存实现

## 概述

### 问题陈述

`RedisTenantConfigCache` 结构体已定义，且实现了 `TenantConfigCache` trait 的四个方法，但所有方法均为空实现（直接返回 `None` 或空操作）。每次需要租户配置时都必须访问数据库，对多租户高并发场景造成显著的数据库压力。

```rust
// src/tenant/config.rs:807-846
pub struct RedisTenantConfigCache {
    // Redis 连接将在这里实现
    _marker: std::marker::PhantomData<()>,
}

#[async_trait]
impl TenantConfigCache for RedisTenantConfigCache {
    async fn get(&self, _tenant_id: &TenantId) -> Option<TenantConfig> {
        // TODO(#INFRA-101): 实现 Redis 租户配置缓存
        // 需要: Redis 连接池和序列化/反序列化逻辑
        None
    }

    async fn set(&self, _tenant_id: &TenantId, _config: &TenantConfig) {
        // TODO(#INFRA-101): 实现 Redis 租户配置缓存
    }

    async fn delete(&self, _tenant_id: &TenantId) {
        // TODO(#INFRA-101): 实现 Redis 租户配置缓存
    }

    async fn clear(&self) {
        // TODO(#INFRA-101): 实现 Redis 租户配置缓存
    }
}
```

### 解决方案

为 `RedisTenantConfigCache` 注入 Redis 连接（`redis::Client` 或 `MultiplexedConnection`），并实现完整的四个缓存操作：`get`（读取并反序列化）、`set`（序列化并写入，带 TTL）、`delete`（删除单条）、`clear`（扫描并批量删除）。

### 范围

- **文件**：`src/tenant/config.rs`
- **变更类型**：功能实现（非破坏性 —— trait 接口不变）
- **不在范围内**：修改 `TenantConfigCache` trait 定义、更改调用方代码

---

## 开发上下文

### Redis 使用模式（项目现有实践）

项目已有成熟的 Redis 使用模式，见 `src/token/redis_store.rs`：

```rust
// 已有的 Redis 连接风格
use redis::{AsyncCommands, Client, aio::MultiplexedConnection};

// Key 命名惯例（与现有保持一致）
pub mod keys {
    pub const ACTIVE_TOKENS_PREFIX: &str = "credbridge:tokens";
    pub fn active_tokens_key(tenant_id: &str) -> String {
        format!("{}:{}:active", ACTIVE_TOKENS_PREFIX, tenant_id)
    }
}
```

**建议的 Key 格式**：`credbridge:tenant:config:{tenant_id}`

### `TenantConfigCache` Trait 定义

```rust
// src/tenant/config.rs:791-805
#[async_trait]
pub trait TenantConfigCache: Send + Sync {
    async fn get(&self, tenant_id: &TenantId) -> Option<TenantConfig>;
    async fn set(&self, tenant_id: &TenantId, config: &TenantConfig);
    async fn delete(&self, tenant_id: &TenantId);
    async fn clear(&self);
}
```

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/tenant/config.rs` | 主要修改目标，含 trait 和结构体定义 |
| `src/token/redis_store.rs` | 项目现有 Redis 使用模式参考 |
| `src/token/revocation.rs` | 另一个 Redis 使用示例 |
| `Cargo.toml` | 确认 `redis` crate 版本和 feature flags |

### 技术决策

1. **连接类型**：使用 `redis::aio::MultiplexedConnection`（与现有 token store 一致），避免每次操作创建新连接。
2. **序列化**：`TenantConfig` 使用 `serde_json` 序列化为 JSON 字符串存储（`redis::Value::BulkString`）。
3. **TTL 设计**：默认 TTL 300 秒（5 分钟），租户配置变更时主动调用 `delete()` 使缓存失效。
4. **`clear()` 实现**：使用 `SCAN` 命令配合 pattern `credbridge:tenant:config:*` 批量删除，避免 `KEYS` 命令阻塞 Redis。
5. **错误处理**：缓存操作失败不应影响主流程，`get()` 返回 `None`（降级到数据库读取），`set()`/`delete()` 记录 warn 日志但不 panic。

---

## 实现计划

### 任务（按依赖顺序）

**任务 1**：修改 `RedisTenantConfigCache` 结构体，注入 Redis 连接

```rust
pub struct RedisTenantConfigCache {
    client: redis::Client,
    ttl_secs: u64,
}

impl RedisTenantConfigCache {
    pub fn new(client: redis::Client) -> Self {
        Self { client, ttl_secs: 300 }
    }

    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = ttl_secs;
        self
    }

    fn cache_key(tenant_id: &TenantId) -> String {
        format!("credbridge:tenant:config:{}", tenant_id)
    }

    async fn get_conn(&self) -> Result<redis::aio::MultiplexedConnection, redis::RedisError> {
        self.client.get_multiplexed_async_connection().await
    }
}
```

**任务 2**：实现 `get()` 方法

逻辑：连接 Redis → `GET key` → 若存在则 `serde_json::from_str::<TenantConfig>` → 失败时记录 warn 并返回 `None`。

**任务 3**：实现 `set()` 方法

逻辑：序列化 `TenantConfig` → `SET key value EX ttl_secs`。

**任务 4**：实现 `delete()` 方法

逻辑：`DEL key`。

**任务 5**：实现 `clear()` 方法

逻辑：使用 `SCAN 0 MATCH credbridge:tenant:config:* COUNT 100` 迭代，批量 `DEL` 匹配的 key。避免使用 `KEYS *` 命令。

**任务 6**：更新 `Default` 实现

`Default` 不再可用（需要 Redis client），移除 `#[derive(Default)]` 或要求调用方通过 `new(client)` 构造。在调用方注入点更新构造代码。

**任务 7**：单元测试

使用 `mockall` mock Redis 连接，或使用 `fakeredis` / `testcontainers-redis` 进行集成测试。

### 验收标准

- [ ] `RedisTenantConfigCache::get()` 在缓存命中时返回 `Some(TenantConfig)`，未命中返回 `None`
- [ ] `RedisTenantConfigCache::set()` 写入 Redis 并设置 TTL（通过 `TTL key` 命令可验证）
- [ ] `RedisTenantConfigCache::delete()` 删除指定 key
- [ ] `RedisTenantConfigCache::clear()` 清空所有 `credbridge:tenant:config:*` 前缀的 key
- [ ] Redis 连接失败时所有方法优雅降级（不 panic，记录 warn 日志）
- [ ] `TenantConfig` 序列化/反序列化往返无数据丢失
- [ ] 编译通过，`cargo clippy` 无新 warning

---

## 附加上下文

### 依赖

- `redis` crate（已在 `Cargo.toml` 中，确认 `tokio-comp` feature 已启用）
- `serde_json`（已有）
- `async-trait`（已有）

### 测试策略

1. **单元测试**（`#[cfg(test)]` 内）：使用 `fakeredis` 或 mock 验证各方法行为
2. **集成测试**（`tests/` 目录）：启动真实 Redis 实例（或 `testcontainers`），验证完整缓存生命周期
3. **降级测试**：模拟 Redis 不可用，验证 `get()` 返回 `None` 且不影响服务启动

### 注意事项

- `TenantConfig` 必须实现 `serde::Serialize + serde::Deserialize`，检查是否已有 derive
- `clear()` 使用 SCAN 时注意 cursor 循环逻辑，直到 cursor 返回 0 为止
- 生产环境中 Redis 连接数限制，建议使用连接池（`bb8-redis` 或 `deadpool-redis`）而非每次 `get_multiplexed_async_connection()`；可在后续迭代优化
