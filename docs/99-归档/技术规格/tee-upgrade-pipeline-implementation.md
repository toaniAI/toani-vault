---
title: 'TEE 蓝绿升级流程完整实现'
slug: 'tee-upgrade-pipeline-implementation'
created: '2026-03-19'
status: 'implemented'
priority: 'P2'
---

# Tech-Spec: TEE 蓝绿升级流程完整实现

## 概述

### 问题陈述

`BlueGreenUpgradeManager` 的状态机框架已完整建立（10个阶段、原子状态转换、并发保护、Sealing Key 托管），但以下 5 个关键操作均为空实现（TODO 占位）：

- **TEE-201**（`start_new_enclave`，行 392）：实际 Enclave 进程启动逻辑缺失，当前直接跳转到 `ParallelRunning` 阶段
- **TEE-202**（`perform_health_check`，行 419）：健康检查硬编码返回 `true`，无法检测新 Enclave 的真实存活状态
- **TEE-203**（`migrate_traffic`，行 456）：流量迁移 `_percentage` 参数被忽略，无实际负载均衡操作
- **TEE-204**（`complete_upgrade`，行 513）：旧版本 Enclave 进程在升级完成后未被关闭，资源泄露
- **TEE-205**（`rollback`，行 542）：回滚时仅清空 `pending_version`，Sealing Key 备份恢复逻辑缺失

这导致整个升级流程形同虚设——状态机可以正常流转，但不会触发任何真实的 Enclave 生命周期操作。

### 解决方案

为每个 TODO 注入对应的真实实现：
1. 集成 Enclave 运行时 SDK（`sgx_urts` 或等效抽象层）启动/终止 Enclave 实例
2. 通过 HTTP/gRPC 健康端点轮询新 Enclave 的存活状态
3. 集成负载均衡器（如 Nginx upstream 或内部路由表）执行加权流量切换
4. 在 `complete_upgrade` 中优雅 drain 旧连接后终止旧 Enclave 进程
5. 在 `rollback` 中从 Vault 备份恢复原始 Sealing Key 托管状态

### 范围

- **包含**：`src/tee/upgrade.rs` 的 5 处 TODO 实现、配套错误处理、单元测试扩展
- **不包含**：`UpgradeConfig` 结构调整、API 层路由变更、数据库持久化升级日志

---

## 开发上下文

### 当前代码（关键片段）

**TEE-201：启动新版本 Enclave（行 382–401）**

```rust
pub async fn start_new_enclave(&self) -> Result<(), UpgradeError> {
    let current_phase = self.current_phase();
    if current_phase != UpgradePhase::StartingNew {
        return Err(UpgradeError::InvalidState(format!(
            "当前阶段 {:?} 不允许启动新 Enclave",
            current_phase
        )));
    }

    // TODO(#TEE-201): 实际启动新版本 Enclave
    // 当前限制: 热升级功能需要完整的 Enclave 生命周期管理
    // 状态: 框架已就绪，等待 Enclave 运行时集成

    // 进入并行运行阶段
    self.phase
        .store(UpgradePhase::ParallelRunning as u8, Ordering::SeqCst);

    Ok(())
}
```

**TEE-202：执行健康检查（行 412–438）**

```rust
pub async fn perform_health_check(&self) -> Result<bool, UpgradeError> {
    let pending = self.pending_version.read().await;
    if pending.is_none() {
        return Err(UpgradeError::InvalidState("新版本未设置".to_string()));
    }

    // TODO(#TEE-202): 实际执行健康检查
    // 需要: 新版本 Enclave 健康检查端点
    // 当前: 模拟成功用于框架测试
    let healthy = true;

    if !healthy {
        let failures = self.health_check_failures.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= self.max_health_check_failures {
            return Err(UpgradeError::HealthCheckFailed(format!(
                "健康检查失败次数达到上限 ({})",
                failures
            )));
        }
    } else {
        self.health_check_failures.store(0, Ordering::SeqCst);
    }

    Ok(healthy)
}
```

**TEE-203：迁移流量（行 441–464）**

```rust
pub async fn migrate_traffic(&self, _percentage: u8) -> Result<(), UpgradeError> {
    // ... 阶段校验 ...

    self.phase
        .store(UpgradePhase::MigratingTraffic as u8, Ordering::SeqCst);

    // TODO(#TEE-203): 实际迁移流量
    // 需要: 负载均衡器集成和流量路由控制

    self.phase
        .store(UpgradePhase::MigratingSealingKey as u8, Ordering::SeqCst);

    Ok(())
}
```

**TEE-204：关闭旧版本 Enclave（行 491–522）**

```rust
pub async fn complete_upgrade(&self) -> Result<UpgradeResult, UpgradeError> {
    // ... 阶段校验与版本切换 ...

    // TODO(#TEE-204): 关闭旧版本 Enclave
    // 需要: 优雅的连接 draining 和 Enclave 终止

    self.phase
        .store(UpgradePhase::Completed as u8, Ordering::SeqCst);
    self.is_upgrading.store(false, Ordering::SeqCst);

    Ok(UpgradeResult::Success)
}
```

**TEE-205：回滚恢复逻辑（行 524–551）**

```rust
pub async fn rollback(&self) -> Result<UpgradeResult, UpgradeError> {
    // ... 阶段校验 ...

    self.phase
        .store(UpgradePhase::RollingBack as u8, Ordering::SeqCst);

    *self.pending_version.write().await = None;

    // 恢复 Sealing Key 托管
    // TODO(#TEE-205): 实现回滚恢复逻辑
    // 需要: 备份恢复机制和状态回滚

    self.phase.store(UpgradePhase::Idle as u8, Ordering::SeqCst);
    self.is_upgrading.store(false, Ordering::SeqCst);
    self.health_check_failures.store(0, Ordering::SeqCst);

    Ok(UpgradeResult::FailedWithRollback)
}
```

### 升级流程全貌

```
start_upgrade()
    │  ┌─ 并发保护：compare_exchange(is_upgrading false→true)
    │  ├─ validate_new_version()（MRENCLAVE 非零、版本号非空、版本不重复）
    │  └─ 阶段 Idle → Preparing → StartingNew
    │
start_new_enclave()  ← TEE-201
    │  ├─ 校验阶段 == StartingNew
    │  ├─ [实际启动新 Enclave 进程]
    │  └─ 阶段 StartingNew → ParallelRunning
    │
perform_health_check()  ← TEE-202（循环调用，直到通过或超过 max_health_check_failures）
    │  ├─ 校验 pending_version 存在
    │  ├─ [HTTP 健康端点轮询]
    │  └─ 失败累积 → 触发 rollback()
    │
migrate_traffic(percentage)  ← TEE-203（可分多步调用：10%→50%→100%）
    │  ├─ 校验阶段 == ParallelRunning 或 MigratingTraffic
    │  ├─ [负载均衡器权重调整]
    │  └─ 阶段 → MigratingSealingKey
    │
migrate_sealing_key()（已实现：更新 active_mrenclave）
    │  └─ 阶段 → Validating
    │
complete_upgrade()  ← TEE-204
    │  ├─ pending → active 版本切换
    │  ├─ [旧 Enclave 优雅 drain + 终止]
    │  ├─ is_upgrading → false
    │  └─ 阶段 → Completed
    │
（任意阶段出错）→ rollback()  ← TEE-205
    │  ├─ 校验 can_rollback()（Preparing/StartingNew/ParallelRunning/
    │  │   MigratingTraffic/MigratingSealingKey/Validating）
    │  ├─ pending_version 清空
    │  ├─ [Sealing Key 从备份恢复]
    │  ├─ is_upgrading → false，failures → 0
    │  └─ 阶段 → Idle
```

关键约束：
- `is_upgrading` 用 `AtomicBool::compare_exchange` 防并发，整个升级期间为 `true`
- `phase` 用 `AtomicU8` + `SeqCst` 顺序保证，所有状态转换可观测
- `SealingKeyCustody.sealed_seed` 在 `Drop` 时 `zeroize()` 安全清零

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/tee/upgrade.rs` | 主实现文件，所有 TODO 均在此 |
| `src/tee/driver_verify.rs` | 同级 TEE 模块，可参考错误处理模式 |
| `src/vault/` | Vault 集成，TEE-205 回滚需读取备份 Sealing Key |
| `src/api/` | 负载均衡/路由控制接入点（TEE-203 需要） |
| `Cargo.toml` | 确认 `sgx_urts`、`reqwest`、`ring` 等依赖版本 |

### 技术决策

| 决策 | 选择 | 理由 |
|------|------|------|
| Enclave 启动方式 | `sgx_urts::enclave::SgxEnclave::create()` 或通过子进程调用 enclave loader | 与现有 TEE 模块保持一致；子进程方式隔离 crash 影响 |
| 健康检查协议 | HTTP GET `/health`（新 Enclave 内部监听端口） | 最简单；可直接用 `reqwest` 发起；失败语义清晰 |
| 流量路由控制 | 修改内存路由表（`RwLock<HashMap<EnclaveId, u8>>` 权重表） | 无需外部依赖；与现有原子状态模型一致 |
| 旧 Enclave 关闭 | 发送 SIGTERM → 等待 drain_timeout → SIGKILL | 标准优雅关闭模式；drain 时间从 `UpgradeConfig` 读取 |
| Sealing Key 备份 | 升级开始时序列化并加密存入 Vault，回滚时读回 | Vault 已集成；与其他密钥管理路径一致 |

---

## 实现计划

### 任务

按升级流程顺序实现，每个任务依赖上一个通过测试。

#### 任务 1：TEE-201 — 启动新版本 Enclave

**位置**：`src/tee/upgrade.rs:392`（`start_new_enclave` 方法体内）

**实现要点**：
1. 读取 `pending_version`，获取新版本 MRENCLAVE 和版本号
2. 调用 Enclave 运行时 API 创建/启动新 Enclave 实例（或 spawn 子进程）
3. 记录新 Enclave 的进程 ID 或句柄，存入 `BlueGreenUpgradeManager` 的新字段（如 `new_enclave_handle: RwLock<Option<EnclaveHandle>>`）
4. 若启动失败，立即返回 `UpgradeError::NewEnclaveStartFailed`，框架层触发回滚
5. 启动成功后再转换阶段到 `ParallelRunning`

**新增字段（需添加到结构体）**：
```rust
/// 新版本 Enclave 句柄（PID 或运行时句柄）
new_enclave_handle: RwLock<Option<EnclaveHandle>>,
/// 旧版本 Enclave 句柄（用于后续关闭）
old_enclave_handle: RwLock<Option<EnclaveHandle>>,
```

#### 任务 2：TEE-202 — 执行健康检查

**位置**：`src/tee/upgrade.rs:419`（`perform_health_check` 方法体内，替换 `let healthy = true`）

**实现要点**：
1. 从 `new_enclave_handle` 读取新 Enclave 的监听地址（或固定约定端口）
2. 使用 `reqwest::Client` 发起 `GET /health`，设置超时（建议 5 秒）
3. HTTP 200 + 响应体包含 `"status": "ok"` 视为健康
4. 非 200 或超时视为不健康，`healthy = false`
5. 保留现有失败计数累积和 `max_health_check_failures` 上限逻辑

**注意**：需在 `UpgradeConfig` 增加 `health_check_timeout_secs: u64` 字段（建议默认 5）。

#### 任务 3：TEE-203 — 迁移流量

**位置**：`src/tee/upgrade.rs:456`（`migrate_traffic` 方法体内，`_percentage` 参数改为 `percentage`）

**实现要点**：
1. 在 `BlueGreenUpgradeManager` 增加字段 `traffic_percentage: AtomicU8`，初始为 0
2. 验证 `percentage` 在 `[0, 100]` 范围内
3. 更新路由权重表（新 Enclave 承接 `percentage%`，旧 Enclave 承接 `100 - percentage%`）
4. 将 `percentage` 写入 `traffic_percentage`
5. 仅当 `percentage == 100` 时才转换阶段到 `MigratingSealingKey`；否则停留在 `MigratingTraffic`，等待下次调用

**调用约定**（外部调用方需按步长多次调用）：
```
migrate_traffic(10)  → 阶段保持 MigratingTraffic
migrate_traffic(50)  → 阶段保持 MigratingTraffic
migrate_traffic(100) → 阶段转换到 MigratingSealingKey
```

#### 任务 4：TEE-204 — 关闭旧版本 Enclave

**位置**：`src/tee/upgrade.rs:513`（`complete_upgrade` 方法体内，新版本写入 `active` 之后）

**实现要点**：
1. 读取 `old_enclave_handle`（在 `start_upgrade` 时记录当前活跃 Enclave 为旧版本）
2. 向旧 Enclave 发送 SIGTERM（或调用运行时 `graceful_shutdown()`）
3. 等待至多 `drain_timeout_secs`（建议从 `UpgradeConfig` 读取，默认 30 秒）
4. 超时后发送 SIGKILL（或强制销毁句柄）
5. 清空 `old_enclave_handle`
6. 关闭失败不应阻断升级完成（记录警告日志，返回 `UpgradeResult::Success`）

**注意**：需在 `UpgradeConfig` 增加 `drain_timeout_secs: u64` 字段（建议默认 30）。

#### 任务 5：TEE-205 — 回滚恢复逻辑

**位置**：`src/tee/upgrade.rs:542`（`rollback` 方法体内，`*self.pending_version.write().await = None` 之后）

**实现要点**：
1. 从 Vault 读取升级前保存的 Sealing Key 备份（键路径约定：`tee/sealing-key/backup/<upgrade_start_timestamp>`）
2. 将 `sealing_key_custody` 恢复为备份状态（`active_mrenclave` 恢复为旧 Enclave 的值）
3. 若新 Enclave 已启动（`new_enclave_handle` 有值），强制终止它（SIGKILL，无需 drain）
4. 若流量已部分迁移（`traffic_percentage > 0`），将路由权重恢复为旧 Enclave 100%
5. Vault 读取失败时返回 `UpgradeError::RollbackFailed`（比静默失败更安全）

**前置动作（需在 `start_upgrade` 中添加）**：
升级开始时，将当前 `sealing_key_custody` 序列化后加密存入 Vault，键路径带升级时间戳，用于回滚读取。

---

### 验收标准

**功能验收**：
- [ ] `start_new_enclave()` 成功启动新 Enclave 进程，`new_enclave_handle` 有效
- [ ] `perform_health_check()` 能检测到新 Enclave 返回非 200 时增加失败计数
- [ ] `migrate_traffic(percentage)` 正确更新路由权重；`percentage=100` 时阶段转换
- [ ] `complete_upgrade()` 执行后旧 Enclave 进程终止（`ps` 无对应进程）
- [ ] `rollback()` 后 Sealing Key 恢复为旧版本值，新 Enclave 进程已终止

**安全验收**：
- [ ] 回滚失败（Vault 不可达）时，`rollback()` 返回 `Err(UpgradeError::RollbackFailed)`，不做静默降级
- [ ] 新 Enclave 的 MRENCLAVE 经过 `subtle::ConstantTimeEq` 比对，防时序攻击
- [ ] 升级完成后，旧 Enclave 的内存已被 zeroize（通过 `SealingKeyCustody::drop()` 保证）

**回归验收**：
- [ ] 现有测试 `test_upgrade_phase_transitions`、`test_rollback`、`test_concurrent_upgrade_conflict` 全部通过
- [ ] 新增集成测试模拟 Enclave 启动失败场景，验证自动触发回滚

---

## 附加上下文

### 依赖

| Crate | 版本 | 用途 |
|-------|------|------|
| `reqwest` | 已有 | 健康检查 HTTP 客户端 |
| `tokio::time::timeout` | 已有 | 健康检查超时控制 |
| `nix::sys::signal` | 需确认 | 向旧 Enclave 发 SIGTERM/SIGKILL（Linux） |
| 现有 Vault 集成 | — | TEE-205 备份读写 |

### 测试策略

- **单元测试**：对 TEE-202，mock HTTP 服务器（`wiremock`），模拟 200/500/超时三种响应
- **单元测试**：对 TEE-203，验证 `traffic_percentage` 原子值变化和阶段转换边界
- **集成测试**：启动最小化 mock Enclave（echo HTTP server），走完完整升级流程
- **负面测试**：健康检查连续失败 `max_health_check_failures` 次后，验证自动触发 `rollback()`

### 注意事项

1. **TEE 环境限制**：真实 SGX 环境中 Enclave 启动需要 `sgx_urts` 驱动可用；开发/测试环境可用 SIM 模式或 mock handle。建议通过 trait 抽象 Enclave 生命周期，方便 mock。
2. **流量迁移原子性**：路由权重更新必须对所有请求线程立即可见，使用 `AtomicU8` 或 `RwLock<u8>` 均可，选择 `AtomicU8` 性能更优。
3. **Sealing Key 备份时机**：必须在 `start_upgrade` 的最开始（`is_upgrading` 设为 true 之后立即）完成备份写入，避免备份失败后状态机进入不可回滚状态。
4. **连接 drain 不阻断完成**：TEE-204 旧 Enclave 关闭超时后采用强制终止（SIGKILL）并记录告警，而非让整个 `complete_upgrade()` 失败，因为新版本已成功接管。
