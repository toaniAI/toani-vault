---
title: 'P0 - immudb_store Mutex 替换为 tokio::sync::Mutex'
slug: 'immudb-tokio-mutex'
created: '2026-03-19'
status: 'implemented'
priority: 'P0'
---

# Tech-Spec: P0 - immudb_store Mutex 替换为 tokio::sync::Mutex

## 概述

### 问题陈述

`src/audit/immudb_store.rs` 的第 6-8 行存在一个已知的并发安全问题：

```rust
// FIXME: 需要将 std::sync::Mutex 替换为 tokio::sync::Mutex 以支持跨 await 持有
// 当前为让 CI 通过暂时允许此警告
#![allow(clippy::await_holding_lock)]
```

文件第 14 行使用 `std::sync::Mutex`，而该文件中多处方法在持有锁的情况下跨越 `.await` 点调用异步方法（例如 `store.query(&options).await?`、`storage.verify().await` 等）。

**风险：**
- `std::sync::Mutex` 持有期间执行 `.await` 会在当前 task 挂起时继续占用锁，阻塞整个 tokio 线程（线程级阻塞，而非 task 级等待），严重时导致死锁或线程饥饿
- Clippy 的 `await_holding_lock` lint 正是针对此问题设计的，`#![allow(...)]` 仅是掩盖问题
- 在高并发的审计写入场景下，此问题会造成可测量的性能下降和潜在死锁

### 解决方案

将 `src/audit/immudb_store.rs` 中所有 `std::sync::Mutex` 替换为 `tokio::sync::Mutex`，相应地将所有 `.lock().map_err(...)` 调用改为 `.lock().await`，并移除 `#![allow(clippy::await_holding_lock)]`。

### 范围

**在范围内：**
- 删除文件顶部的 `#![allow(clippy::await_holding_lock)]`
- 将 `use std::sync::{Arc, Mutex}` 中的 `Mutex` 改为 `tokio::sync::Mutex`（`Arc` 来自 `std::sync`，无需改变）
- 将 `ImmuDbAuditStore` 结构体字段 `storage` 和 `cache` 的类型从 `Arc<Mutex<...>>` 改为 `Arc<tokio::sync::Mutex<...>>`
- 将文件中所有 `.lock().map_err(|e| ...)` 调用改为 `.lock().await`
- 保持所有公共 API 签名不变

**不在范围内：**
- 修改 `ImmuDbStorage`、`ImmuDbClient` 等内部实现
- 修改 `AuditStorage` trait 定义
- 修改其他文件中使用 `std::sync::Mutex` 的地方（例如 `src/audit/recorder.rs` 的 `AuditLogChain` 使用的是同步上下文，不需要改）

---

## 开发上下文

### 当前代码（关键片段）

**问题声明（文件顶部）：**
```rust
// src/audit/immudb_store.rs:6-14

// FIXME: 需要将 std::sync::Mutex 替换为 tokio::sync::Mutex 以支持跨 await 持有
// 当前为让 CI 通过暂时允许此警告
#![allow(clippy::await_holding_lock)]

use super::events::{AuditEntry, Outcome};
use super::immudb_client::{ImmuDbConfig, ImmuDbState, ImmuDbStorage, QueryOptions};
use super::recorder::{RecorderError, SignedAuditEntry};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
```

**结构体字段（使用 std Mutex）：**
```rust
// src/audit/immudb_store.rs:22-33

pub struct ImmuDbAuditStore {
    storage: Arc<Mutex<ImmuDbStorage>>,
    cache: Arc<Mutex<VecDeque<SignedAuditEntry>>>,
    max_cache_size: usize,
    signer_fingerprint: String,
    public_key: Vec<u8>,
}
```

**典型问题：跨 await 持有 std Mutex**
```rust
// src/audit/immudb_store.rs:312-319（query_by_user 中）

let immu_entries = {
    let storage = self
        .storage
        .lock()
        .map_err(|e| RecorderError::StorageError(format!("Storage lock failed: {}", e)))?;
    storage.query(&options).await?   // <-- 跨 await 持有 std::sync::Mutex!
};
```

**类似问题出现在以下方法中（均需修改）：**
- `store`（第 188-195 行）：`storage.store(signed_entry).await?`
- `get_by_index`（第 250-256 行）：`storage.get(&key).await?`
- `get_recent`（第 285-296 行）：`storage.query(&options).await?`
- `query_by_user`（第 312-319 行）：`storage.query(&options).await?`
- `query_by_outcome`（第 333-340 行）：`storage.query(&options).await?`
- `query_by_time_range`（第 356-363 行）：`storage.query(&options).await?`
- `verify`（第 373-379 行）：`storage.verify().await`
- `verify_entry`（第 386-395 行）：`storage.get_proof(&key).await?`

### 代码库模式

项目在其他地方已正确使用 `tokio::sync::Mutex`：

```rust
// src/api/audit.rs:80,95 — MemoryAuditStorageAdapter 正确使用 tokio Mutex
pub struct MemoryAuditStorageAdapter {
    storage: Arc<tokio::sync::Mutex<MemoryAuditStorage>>,
}

impl MemoryAuditStorageAdapter {
    pub fn new(storage: MemoryAuditStorage) -> Self {
        Self {
            storage: Arc::new(tokio::sync::Mutex::new(storage)),
        }
    }
}

// 使用方式（正确）：
let storage = self.storage.lock().await;
```

**关键差异：**
- `std::sync::Mutex::lock()` 返回 `LockResult<MutexGuard>`，需 `.map_err(...)`
- `tokio::sync::Mutex::lock()` 返回 `MutexGuard`（future，不会 poison），直接 `.await`，无需 `map_err`

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/audit/immudb_store.rs` | 目标文件，需全量修改 |
| `src/api/audit.rs:77-105` | 参照：正确使用 `tokio::sync::Mutex` 的模式 |
| `src/audit/recorder.rs:10,386-404` | 对比：`AuditLogChain` 的 `std::sync::Mutex` 不需要改（纯同步上下文） |

### 技术决策

1. **`tokio::sync::Mutex` 不会 panic on poison**：`std::sync::Mutex` 在持有锁的线程 panic 时会 "poison" 锁，后续 `lock()` 返回 `Err`；`tokio::sync::Mutex` 没有 poison 机制，`lock().await` 总是成功返回 guard。因此所有 `.lock().map_err(|e| RecorderError::StorageError(...))` 都可以简化为 `.lock().await`。

2. **`Arc` 来源不变**：`Arc` 仍然来自 `std::sync::Arc`，只需修改 `Mutex` 的导入路径。修改后 `use` 行：
   ```rust
   use std::sync::Arc;
   use tokio::sync::Mutex;
   ```

3. **`new()` 中的构造方式相同**：`tokio::sync::Mutex::new(value)` 与 `std::sync::Mutex::new(value)` 调用签名完全一致，无需改动构造代码。

---

## 实现计划

### 任务

（按依赖顺序排列）

1. **删除 allow 属性并修改导入** — `src/audit/immudb_store.rs:6-14`

   - 删除第 6-8 行的 `// FIXME` 注释和 `#![allow(clippy::await_holding_lock)]`
   - 将 `use std::sync::{Arc, Mutex};` 改为：
     ```rust
     use std::sync::Arc;
     use tokio::sync::Mutex;
     ```

2. **将所有 `.lock().map_err(...)` 改为 `.lock().await`** — `src/audit/immudb_store.rs`（全文）

   使用编辑器的全局搜索替换，将以下模式：
   ```rust
   self.storage
       .lock()
       .map_err(|e| RecorderError::StorageError(format!("Storage lock failed: {}", e)))?;
   ```
   替换为：
   ```rust
   self.storage.lock().await;
   ```

   以及将：
   ```rust
   self.cache
       .lock()
       .map_err(|e| RecorderError::StorageError(format!("Cache lock failed: {}", e)))?;
   ```
   替换为：
   ```rust
   self.cache.lock().await;
   ```

   **注意**：替换后无需 `?`（`.lock().await` 不返回 `Result`）。

3. **编译并运行 Clippy 验证** — 项目根目录

   ```bash
   cargo clippy -p vault-service 2>&1 | grep -i "await_holding\|immudb_store"
   cargo build
   ```

   预期：不再出现 `await_holding_lock` 警告，编译通过。

### 验收标准

**场景 1：Clippy 检查通过**
- Given: 完成所有修改
- When: 运行 `cargo clippy`
- Then: 不出现 `clippy::await_holding_lock` 警告，`#![allow(...)]` 已移除

**场景 2：编译成功**
- Given: 完成所有修改
- When: 运行 `cargo build`
- Then: 编译无错误，无关于 Mutex 类型不匹配的编译错误

**场景 3：并发场景不死锁**
- Given: 多个并发 task 同时调用 `ImmuDbAuditStore::store`
- When: 执行并发写入
- Then: 所有 task 正常完成，不出现死锁或线程阻塞超时

**场景 4：现有功能不回归**
- Given: 所有修改完成
- When: 运行 `cargo test`
- Then: 所有已有测试通过

---

## 附加上下文

### 依赖

`tokio` 已在项目中使用（`src/mcp/token_storage.rs:22` 已有 `use tokio::sync::RwLock`），**无需添加新依赖**。

### 测试策略

本次修改是纯机制替换，不改变任何业务逻辑，以下验证即可：

1. `cargo build`：确保类型系统通过编译
2. `cargo clippy`：确保 `await_holding_lock` 警告消失
3. 若项目有针对 `ImmuDbAuditStore` 的集成测试，运行确认无回归

### 注意事项

1. **不要遗漏任何 `.lock()` 调用**：`immudb_store.rs` 中所有对 `self.storage.lock()` 和 `self.cache.lock()` 的调用都必须更新，漏改任意一处会导致编译错误（类型不匹配）。
2. **`?` 运算符去除**：`std::sync::Mutex::lock()` 返回 `Result` 需要 `?`，`tokio::sync::Mutex::lock().await` 返回 `MutexGuard` 直接使用，不需要 `?`。若漏掉删除 `?` 会有类型错误提示，编译器会明确指出。
3. **`Arc` 导入分离**：确保 `Arc` 从 `std::sync` 导入，`Mutex` 从 `tokio::sync` 导入，两者不要混写。
4. **不影响 `recorder.rs`**：`src/audit/recorder.rs` 中的 `AuditLogChain` 使用 `std::sync::Mutex` 是正确的（仅在同步上下文中调用），**不在本次修改范围内**。
