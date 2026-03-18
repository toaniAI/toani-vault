# CredBridge 安全修复报告

**报告日期**: 2026-03-11
**修复工作流**: BMAD Correct Course
**修复优先级**: HIGH (4 项)
**测试状态**: 325 项测试全部通过

---

## 修复概览

| 问题ID | 问题描述 | 严重程度 | 修复状态 |
|--------|----------|----------|----------|
| H-001 | Token 黑名单使用内存 HashSet，重启后状态丢失 | 🔴 HIGH | ✅ 已修复 |
| H-002 | 模拟模式启用 DEBUG 标志，密钥可能泄露 | 🔴 HIGH | ✅ 已修复 |
| H-003 | 密钥缓存使用标准库 RwLock，可能死锁/DoS | 🔴 HIGH | ✅ 已修复 |
| H-004 | 密钥派生缺少版本控制，无法安全轮换 | 🔴 HIGH | ✅ 已修复 |

---

## H-001: Token 黑名单使用内存 HashSet

### 问题描述
- **位置**: `src/api/middleware.rs:138`
- **风险**: 服务重启后 Token 黑名单状态丢失，可能导致重放攻击
- **影响**: 攻击者可在服务重启后重放已使用的 Token

### 修复方案
1. 创建了新的 `src/api/token_blacklist.rs` 模块
2. 实现了 `TokenBlacklist` trait 抽象，支持多种后端
3. 提供了两种实现：
   - `InMemoryTokenBlacklist`: 内存存储，带 TTL 自动过期
   - `RedisTokenBlacklist`: Redis 集中式存储，多实例共享
4. 添加了 `TokenBlacklistFactory` 工厂模式
5. Token 黑名单条目使用 15 分钟 TTL 自动过期

### 关键代码变更
```rust
// 新的 TokenBlacklist trait
#[async_trait]
pub trait TokenBlacklist: Send + Sync {
    async fn blacklist_token(&self, jti: &str, ttl_seconds: u64) -> Result<(), BlacklistError>;
    async fn is_blacklisted(&self, jti: &str) -> Result<bool, BlacklistError>;
    async fn remove_from_blacklist(&self, jti: &str) -> Result<(), BlacklistError>;
}
```

### 验证测试
- ✅ `test_memory_blacklist_basic`
- ✅ `test_memory_blacklist_expiration`
- ✅ `test_memory_blacklist_cleanup`

---

## H-002: 模拟模式启用 DEBUG 标志

### 问题描述
- **位置**: `src/tee/attestation.rs:1007`
- **风险**: 生产环境可能意外启用 DEBUG 标志，导致密钥泄露
- **影响**: DEBUG 模式允许外部调试器访问 Enclave 内存

### 修复方案
1. 添加了 SGX 属性标志常量定义
2. 实现了编译时安全检查：
   - `strict-production` 特性：禁止调试功能
   - `allow-debug` 特性：显式允许调试模式
3. 添加了 `is_debug_mode_allowed()` 函数，确保生产构建默认禁用 DEBUG
4. `generate_report_body()` 现在根据构建类型决定是否设置 DEBUG 标志

### 关键代码变更
```rust
// Cargo.toml 新增特性
[features]
strict-production = []  # 严格生产模式：禁止调试功能
allow-debug = []        # 允许调试模式（即使在发布构建中）

// 编译时安全检查
#[cfg(all(feature = "strict-production", debug_assertions))]
compile_error!("严格生产模式不能在调试构建中启用");

// 运行时检查
fn is_debug_mode_allowed(enclave_debug_mode: bool) -> bool {
    if cfg!(debug_assertions) { return true; }
    if cfg!(feature = "allow-debug") { return enclave_debug_mode; }
    false  // 生产构建默认不允许
}
```

---

## H-003: 密钥缓存使用标准库 RwLock

### 问题描述
- **位置**: `src/tee/enclave.rs:124`
- **风险**: 标准库 RwLock 可能导致死锁，造成 DoS
- **影响**: 攻击者可能通过特定操作序列触发死锁

### 修复方案
1. 将 `std::sync::RwLock` 替换为 `tokio::sync::RwLock`
2. 添加了锁超时机制（默认 5 秒超时）
3. 实现了同步和异步两套锁获取辅助函数：
   - `acquire_read_lock_sync()` / `acquire_write_lock_sync()`
   - `acquire_read_lock()` / `acquire_write_lock()` (async)
4. 添加了 `EnclaveError::LockTimeout` 错误类型
5. 使用重试机制和退避策略避免活锁

### 关键代码变更
```rust
// 锁超时配置
const LOCK_TIMEOUT_MS: u64 = 5000;      // 5 秒超时
const LOCK_RETRY_INTERVAL_MS: u64 = 10; // 重试间隔

// 同步锁获取（带超时重试）
fn acquire_read_lock_sync<T>(
    lock: &RwLock<T>,
) -> Result<tokio::sync::RwLockReadGuard<'_, T>, EnclaveError> {
    let start = std::time::Instant::now();
    loop {
        match lock.try_read() {
            Ok(guard) => return Ok(guard),
            Err(_) => {
                if start.elapsed() >= Duration::from_millis(LOCK_TIMEOUT_MS) {
                    return Err(EnclaveError::LockTimeout("...".to_string()));
                }
                std::thread::sleep(Duration::from_millis(LOCK_RETRY_INTERVAL_MS));
            }
        }
    }
}
```

---

## H-004: 密钥派生缺少版本控制

### 问题描述
- **位置**: `src/crypto/hkdf.rs`, `src/tee/enclave.rs`
- **风险**: 无法实现前向保密轮换
- **影响**: 一旦密钥泄露，所有历史数据都面临风险

### 修复方案
1. 添加了 `KeyVersion` 结构体，包含：
   - 主版本号 (major)
   - 次版本号 (minor)
   - 派生时间戳
2. 定义了 `KEY_DERIVATION_VERSION` 常量（当前为 v1）
3. 定义了 `KEY_ROTATION_INTERVAL_SECONDS`（90 天轮换周期）
4. 更新了 `KeyHierarchy`：
   - 添加 `key_version` 字段
   - 添加 `legacy_versions` 历史版本列表
5. 在密钥派生 info 字符串中包含版本信息
6. 实现了 `rotate_key_version()` 和 `rotate_master_key()` 方法
7. 添加了 `derive_with_legacy_version()` 用于解密历史凭证

### 关键代码变更
```rust
/// 密钥版本信息
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyVersion {
    pub major: u32,
    pub minor: u32,
    pub derived_at: u64,
}

// 派生时包含版本信息
let info = format!(
    "user-vault-key:v{}:{}:{}",
    self.key_version.major, tenant_id, user_id
);

// 密钥轮换
pub fn rotate_master_key(&mut self, l0_key: &HardwareRootKey) -> Result<KeyHandle, CryptoError> {
    self.rotate_key_version();  // 递增版本号
    // ... 重新初始化主密钥
}
```

### 验证测试
- ✅ `test_key_version`
- ✅ `test_key_version_rotation`
- ✅ `test_key_rotation_changes_derived_keys`
- ✅ `test_key_version_needs_rotation`

---

## 验证结果

### 单元测试
```
test result: ok. 325 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
```

### 新增测试
| 测试文件 | 新增测试数 |
|----------|-----------|
| src/api/token_blacklist.rs | 5 |
| src/crypto/hkdf.rs | 4 |
| src/tee/enclave.rs | 1 (异步化) |

### 修复验证清单
- [x] H-001: Token 黑名单支持 Redis 后端
- [x] H-001: Token 黑名单 TTL 自动过期
- [x] H-002: 编译时 DEBUG 标志检查
- [x] H-002: 生产构建默认禁用 DEBUG
- [x] H-003: 异步 RwLock 替换标准库 RwLock
- [x] H-003: 锁超时机制实现
- [x] H-004: 密钥版本控制实现
- [x] H-004: 密钥轮换机制实现

---

## 后续建议

### 部署前检查清单
1. 生产环境配置 Redis 连接 URL
2. 确保 `--release` 模式构建，不启用 `allow-debug` 特性
3. 监控锁超时日志，及时调整超时阈值
4. 设置密钥轮换告警（建议提前 7 天通知）

### 监控指标
- Token 黑名单命中率
- Redis 连接健康状态
- 锁获取延迟 P99
- 密钥版本轮换事件

### 后续优化
1. 实现自动密钥轮换（基于 TTL）
2. 添加 Prometheus 指标暴露
3. 考虑 Redis 集群支持
4. 添加更详细的审计日志

---

**报告完成**
**修复验证**: 全部通过
**建议措施**: 已列出
**风险评估**: HIGH → LOW (修复后)
