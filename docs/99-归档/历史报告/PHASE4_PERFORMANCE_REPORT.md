# Phase 4 性能测试报告

**项目名称**: CredBridge TEE Secure Execution Sandbox
**测试日期**: 2026-03-17
**报告版本**: v1.0
**测试环境**: 本地开发环境 / TEE 模拟模式

---

## 1. 执行摘要

本报告汇总了 CredBridge TEE Secure Execution Sandbox Phase 4 的性能测试结果。测试覆盖了启动时间、并发能力、内存占用和压力测试等关键指标。

### 1.1 测试结论

| 测试项 | 本地目标 | TEE 目标 | 测试结果 | 状态 |
|--------|----------|----------|----------|------|
| 热实例启动 | ≤ 100ms | ≤ 50ms | 待测试 | - |
| 冷启动 | ≤ 3秒 | ≤ 2秒 | 待测试 | - |
| 并发会话 | ≥ 50 | ≥ 100 | 待测试 | - |
| 内存占用 | ≤ 150MB | ≤ 100MB | 待测试 | - |

**说明**: 实际性能测试需要在部署环境中运行 `cargo test --test sandbox_performance` 获取具体数据。

---

## 2. 测试环境

### 2.1 硬件配置

| 项目 | 配置 |
|------|------|
| CPU | Intel Core i7 / Xeon (支持 SGX) |
| 内存 | 32GB DDR4 |
| 存储 | NVMe SSD |
| 网络 | 千兆以太网 |

### 2.2 软件配置

| 项目 | 版本 |
|------|------|
| OS | Ubuntu 22.04 LTS |
| Docker | 24.0+ |
| Rust | 1.75+ |
| PostgreSQL | 15+ |
| Redis | 7+ |

### 2.3 测试工具

- **Rust 测试框架**: `cargo test`
- **性能测试文件**: `tests/tee/sandbox_performance.rs`
- **监控工具**: `docker stats`, `htop`

---

## 3. 性能指标

### 3.1 启动时间测试

#### 3.1.1 热实例启动

**测试目标**: 从热实例池获取会话的时间应 ≤ 100ms（本地）/ ≤ 50ms（TEE）

**测试方法**:
```rust
// 预热后测量获取时间
for i in 0..test_iterations {
    let request = create_test_session_request();
    let (result, elapsed) = measure_time_async(|| async {
        pool.acquire_session(request).await
    }).await;
    // 记录 elapsed
}
```

**预期结果**:
- 平均启动时间 ≤ 100ms
- 成功率 100%

#### 3.1.2 冷启动

**测试目标**: 在没有热实例的情况下创建新会话应 ≤ 3秒（本地）/ ≤ 2秒（TEE）

**测试方法**:
```rust
// 禁用热实例，测量首次创建时间
config.pool.min_warm_instances = 0;
config.pool.max_warm_instances = 0;
```

**预期结果**:
- 平均冷启动时间 ≤ 3秒
- 成功率 100%

### 3.2 并发能力测试

#### 3.2.1 并发会话处理

**测试目标**: 系统应能同时处理 ≥ 50 个并发会话（本地）/ ≥ 100 个（TEE）

**测试方法**:
```rust
let concurrency_levels = vec![10, 25, 50, 75, 100];

for concurrency in concurrency_levels {
    let mut join_set = JoinSet::new();

    for i in 0..concurrency {
        join_set.spawn(async move {
            let request = create_test_session_request();
            pool.acquire_session(request).await.is_ok()
        });
    }

    // 统计成功率
}
```

**预期结果**:
- 50 并发成功率 ≥ 95%
- 平均响应时间 < 500ms

#### 3.2.2 并发会话生命周期

**测试方法**:
```rust
let concurrency = 20;
let operations_per_session = 5;

// 每个会话执行多次操作
```

**预期结果**:
- 所有会话成功完成生命周期
- 无资源泄漏

### 3.3 内存占用测试

#### 3.3.1 内存使用

**测试目标**: 内存占用应 ≤ 150MB（本地）/ ≤ 100MB（TEE）

**测试方法**:
```rust
let before_memory = get_current_memory_usage();

// 创建会话
for i in 0..test_size {
    let request = create_test_session_request();
    if let Ok(session) = pool.acquire_session(request).await {
        sessions.push(session);
    }
}

let after_memory = get_current_memory_usage();
let memory_increase = after_memory - before_memory;
```

**预期结果**:
- 内存增长 ≤ 150MB
- 每会话平均内存 < 3MB

#### 3.3.2 内存稳定性

**测试方法**:
```rust
let duration = Duration::from_secs(30);
let interval = Duration::from_secs(5);

while start.elapsed() < duration {
    let current_memory = get_current_memory_usage();
    // 记录内存使用

    // 模拟活动
    let request = create_test_session_request();
    if let Ok(session) = pool.acquire_session(request).await {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = pool.release_session(session.id()).await;
    }

    tokio::time::sleep(interval).await;
}
```

**预期结果**:
- 内存使用稳定，无持续增长
- 内存波动在合理范围内

### 3.4 压力测试

#### 3.4.1 高负载压力测试

**测试方法**:
```rust
let duration = Duration::from_secs(10);
let concurrency = 30;

// 启动多个工作线程
for worker_id in 0..concurrency {
    tokio::spawn(async move {
        while worker_start.elapsed() < duration {
            // 获取会话 -> 执行操作 -> 释放会话
        }
    });
}
```

**预期结果**:
- 错误率 < 1%
- 吞吐量稳定

#### 3.4.2 综合性能基准

**测试内容**:
1. 热实例启动基准
2. 会话生命周期基准
3. 并发处理基准
4. 内存使用基准

---

## 4. 测试执行

### 4.1 运行测试

```bash
# 运行所有性能测试
cargo test --test sandbox_performance -- --nocapture

# 运行特定测试
cargo test --test sandbox_performance test_warm_instance_startup_time -- --nocapture
cargo test --test sandbox_performance test_concurrent_sessions -- --nocapture
cargo test --test sandbox_performance test_memory_usage -- --nocapture

# 运行压力测试
cargo test --test sandbox_performance test_stress_high_load -- --nocapture

# 运行综合基准测试
cargo test --test sandbox_performance test_performance_benchmark -- --nocapture
```

### 4.2 监控资源使用

```bash
# 监控 Docker 容器资源
docker stats

# 监控系统资源
htop

# 监控 SGX EPC 内存
cat /sys/kernel/debug/sgx/epc_pages
```

---

## 5. 性能优化建议

### 5.1 启动时间优化

- **热实例池**: 保持最小热实例数，减少冷启动
- **预加载**: 预加载常用资源
- **并行初始化**: 并行执行初始化任务

### 5.2 并发优化

- **连接池**: 优化数据库连接池大小
- **异步处理**: 充分利用异步 I/O
- **资源限制**: 合理设置并发限制

### 5.3 内存优化

- **对象池**: 重用会话对象
- **及时释放**: 确保会话及时关闭
- **监控告警**: 设置内存使用告警

---

## 6. 测试文件说明

### 6.1 测试文件位置

```
tests/tee/sandbox_performance.rs
```

### 6.2 测试函数列表

| 测试函数 | 描述 |
|----------|------|
| `test_warm_instance_startup_time` | 热实例启动时间测试 |
| `test_cold_start_time` | 冷启动时间测试 |
| `test_concurrent_sessions` | 并发会话测试 |
| `test_concurrent_session_lifecycle` | 并发会话生命周期测试 |
| `test_memory_usage` | 内存占用测试 |
| `test_memory_stability` | 内存稳定性测试 |
| `test_stress_high_load` | 高负载压力测试 |
| `test_performance_benchmark` | 综合性能基准测试 |
| `test_pool_configuration_performance` | 池配置性能测试 |

### 6.3 性能目标常量

```rust
/// 热实例启动时间目标 (毫秒)
const WARM_STARTUP_TARGET_MS: u64 = 100;
/// 冷启动时间目标 (毫秒)
const COLD_START_TARGET_MS: u64 = 3000;
/// 并发会话目标数
const CONCURRENT_SESSIONS_TARGET: usize = 50;
/// 内存占用目标 (MB)
const MEMORY_USAGE_TARGET_MB: u64 = 150;
```

---

## 7. 附录

### 7.1 性能测试检查清单

- [ ] 热实例启动时间 ≤ 100ms
- [ ] 冷启动时间 ≤ 3秒
- [ ] 并发会话 ≥ 50
- [ ] 内存占用 ≤ 150MB
- [ ] 错误率 < 1%
- [ ] 无内存泄漏
- [ ] 10分钟持续运行稳定

### 7.2 相关文档

- [Sandbox 设计文档](./TEE_SECURE_EXECUTION_SANDBOX_DESIGN.md)
- [SDK 使用指南](./SDK_SANDBOX_GUIDE.md)
- [API 文档](./API.md)
- [Final Gate 检查清单](./FINAL_GATE_CHECKLIST.md)

### 7.3 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|----------|
| 1.0 | 2026-03-17 | 初始版本 |

---

**© 2026 CredBridge. All rights reserved.**
