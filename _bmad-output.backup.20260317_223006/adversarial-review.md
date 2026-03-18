# CredBridge 对抗性安全审查报告

**审查日期**: 2026-03-11
**审查类型**: 攻击者视角安全分析
**审查范围**: TEE 安全边界、密钥管理、API 攻击面、多租户隔离、凭证存储、Token 系统
**审查标准**: 基于零信任架构原则、TEE 安全最佳实践、密码学安全规范

---

## 执行摘要

本次审查从攻击者视角对 CredBridge 系统进行了全面的安全分析，共识别出 **13 个安全问题**，其中：

| 优先级 | 数量 | 状态 |
|--------|------|------|
| HIGH (需立即修复) | 4 | 危险 |
| MEDIUM (建议修复) | 5 | 警告 |
| LOW (改进建议) | 4 | 注意 |

---

## 高风险问题 (HIGH)

### 🔴 H-001: Token 存储使用内存 HashSet 导致内存泄漏和重启后状态丢失

**位置**: `src/api/middleware.rs:138`

**问题描述**:
```rust
pub type TokenStore = Arc<RwLock<HashSet<String>>>;
```

Token 撤销检查使用内存中的 `HashSet` 存储已使用的 jti，存在以下问题：
1. **服务重启后状态丢失**: 重启后所有已撤销 Token 将变为有效
2. **内存无限增长**: HashSet 只增不减，长期运行会导致 OOM
3. **水平扩展问题**: 多实例部署时各实例状态不一致

**攻击场景**:
1. 攻击者获取有效 Token
2. 服务重启后，Token 被错误地视为有效（即使已撤销）
3. 攻击者可重复使用该 Token 进行未授权访问

**修复建议**:
- 使用 Redis 作为集中式 Token 黑名单存储（已部分实现但未在 middleware 中使用）
- 添加 TTL 自动过期机制
- 实现定期清理任务

**严重程度**: 🔴 HIGH - 服务可用性和安全性双重风险

---

### 🔴 H-002: 模拟模式下启用调试属性 (DEBUG 标志) 导致生产环境风险

**位置**: `src/tee/attestation.rs:1007`

**问题描述**:
```rust
attributes: [0x05u8; 16], // 设置 INIT 和 DEBUG 属性位
```

在 Quote 生成时，attributes 字段设置了 DEBUG 标志位 (0x04)。如果 `allow_simulation` 配置被错误地应用到生产环境：
1. 攻击者可能利用 DEBUG 模式获取 Enclave 内存转储
2. 密钥材料可能在调试输出中泄露
3. SGX 硬件保护可能被绕过

**攻击场景**:
1. 配置错误将 `allow_simulation` 设置为 true
2. 攻击者通过 DCAP 认证流程获取 Quote
3. 发现 Quote 中 DEBUG 标志被设置
4. 利用调试接口提取敏感信息

**修复建议**:
- 添加编译时检查，禁止在非测试构建中启用模拟模式
- 使用条件编译隔离调试代码：`#[cfg(debug_assertions)]`
- 在生产环境启动时验证 `allow_simulation` 为 false，否则 panic

**严重程度**: 🔴 HIGH - 可能导致密钥泄露

---

### 🔴 H-003: 用户密钥缓存使用 `RwLock` 可能导致死锁和拒绝服务

**位置**: `src/tee/enclave.rs:124`, `src/tee/keys.rs`

**问题描述**:
```rust
user_key_cache: Arc<RwLock<UserKeyCache>>,
```

密钥缓存使用标准库的 `RwLock`，在以下场景可能导致问题：
1. **写锁饥饿**: 大量并发读取时，写操作可能无限期等待
2. **无法处理 poison**: 如果持有锁的线程 panic，锁会被标记为 poison，后续操作失败
3. **无超时机制**: 如果某个线程持有锁不释放，整个系统卡住

**攻击场景**:
1. 攻击者发起大量并发凭证解密请求
2. 每个请求都尝试获取用户密钥缓存的读锁
3. 同时攻击者触发一个会导致 panic 的操作
4. 锁被 poison，所有后续密钥操作失败
5. 整个凭证服务不可用

**修复建议**:
- 使用 `tokio::sync::RwLock` 替代标准库版本（更好的并发性能）
- 实现锁超时机制
- 处理 poison 错误，自动恢复

**严重程度**: 🔴 HIGH - 可能导致 DoS

---

### 🔴 H-004: 密钥派生时缺少密钥版本控制，无法实现前向保密轮换

**位置**: `src/crypto/hkdf.rs`, `src/tee/enclave.rs`

**问题描述**:

当前密钥派生实现：
```rust
let user_vault_key = hkdf_expand(
    prk: &enclave_master_key,
    info: &format!("user-vault-key:{}:{}", tenant_id, user_id),
    length: 32
)?;
```

缺少密钥版本标识，当需要轮换密钥时：
1. 无法区分新旧密钥派生
2. 历史凭证无法解密（因为没有对应的旧密钥版本）
3. 无法实现密钥的前向保密轮换

**攻击场景**:
1. 系统运行一段时间后需要轮换 L1 Master Key
2. 新密钥派生出的 L2 密钥与旧密钥不同
3. 之前加密的凭证无法解密
4. 或者为了兼容性保留旧密钥，导致历史密钥泄露风险持续存在

**修复建议**:
- 在密钥派生 info 字符串中添加版本号：`user-vault-key:v1:tenant:user`
- 实现密钥版本存储和迁移机制
- 支持多版本密钥同时存在，逐步迁移凭证加密

**严重程度**: 🔴 HIGH - 长期运营风险

---

## 中风险问题 (MEDIUM)

### 🟡 M-001: Token 黑名单检查与记录之间存在竞态条件

**位置**: `src/api/middleware.rs:171-178`

**问题描述**:
```rust
// 检查 jti 是否已被使用
let store = token_store.read().await;
if store.contains(&validation_result.token_id) {
    return Err(AuthError::new("revoked_token", "Token 已被使用或撤销"));
}
drop(store);

// 记录 jti 已使用
token_store.write().await.insert(validation_result.token_id.clone());
```

检查和记录之间存在时间窗口，在高并发场景下：
1. 请求 A 检查 jti 不存在
2. 请求 B 检查 jti 不存在（同时发生）
3. 请求 A 记录 jti
4. 请求 B 记录 jti（覆盖或重复）
5. 两个请求都被处理，违反了 jti 单次使用原则

**攻击场景**:
1. 攻击者获取一次性 Token
2. 同时发起多个包含相同 Token 的请求
3. 所有请求都可能通过验证
4. 实现 Token 重放攻击

**修复建议**:
- 使用原子操作或分布式锁
- 考虑使用 Redis SETNX 命令的原子性
- 在数据库层添加唯一约束

**严重程度**: 🟡 MEDIUM

---

### 🟡 M-002: PASETO Token 密钥派生使用 HMAC-SHA256 而非 HKDF

**位置**: `src/token/paseto.rs:121-140`

**问题描述**:
```rust
pub fn derive_key_from_master(
    master_key: &[u8],
    context: &str,
) -> Result<PasetoKey, TokenError> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, master_key);
    let tag = hmac::sign(&key, context.as_bytes());
    let derived = tag.as_ref();
    let token_key = &derived[..32.min(derived.len())];
    // ...
}
```

使用 HMAC 作为 KDF 而非标准 HKDF：
1. 不符合密码学最佳实践（HKDF 专为密钥派生设计）
2. 如果输入密钥材料具有低熵，HMAC 的熵可能不足
3. 缺少 extract-then-expand 的两阶段过程

**修复建议**:
- 使用 `hkdf::Hkdf::<Sha256>` 替代 HMAC
- 添加 salt 参数增加熵
- 参考实现：
```rust
use hkdf::Hkdf;
use sha2::Sha256;

let hkdf = Hkdf::<Sha256>::new(Some(salt), master_key);
let mut okm = [0u8; 32];
hkdf.expand(context.as_bytes(), &mut okm)?;
```

**严重程度**: 🟡 MEDIUM - 密码学规范性问题

---

### 🟡 M-003: 审计日志缺少完整性签名，可被篡改

**位置**: `src/audit/events.rs`, `src/audit/immudb_store.rs`

**问题描述**:

虽然审计日志存储在 immudb（具有 Merkle Tree 保证），但审计条目本身缺少 Ed25519 签名：
```rust
pub struct AuditEntry {
    // ... 字段
    // 缺少签名字段
}
```

如果 immudb 的私钥泄露或出现内部威胁：
1. 攻击者可以修改历史审计记录
2. 无法证明特定审计条目确实由服务生成
3. 合规审计时无法提供不可否认性证明

**修复建议**:
- 为每个审计条目添加 Ed25519 签名
- 使用仅存在于 Enclave 中的签名密钥
- 实现签名验证 API

**严重程度**: 🟡 MEDIUM - 合规和不可否认性风险

---

### 🟡 M-004: 凭证解密原因 (`reason`) 字段未经审计验证

**位置**: `API.md:162-166`

**问题描述**:

API 文档显示解密请求需要一个 reason 字段：
```json
{
  "reason": "用户登录操作"
}
```

但当前实现缺少：
1. reason 字段的必填验证
2. reason 内容的长度限制和格式验证
3. reason 与实际操作的一致性检查

**攻击场景**:
1. 攻击者获取凭证解密权限
2. 使用虚假或误导性的 reason 进行解密
3. 审计日志记录的 reason 无法反映真实意图
4. 事后无法追踪真实的解密目的

**修复建议**:
- 强制要求 reason 字段，长度 10-500 字符
- 实现 reason 分类（预设原因代码）
- 对 reason 进行语义分析，检测异常模式
- 高风险操作需要额外审批

**严重程度**: 🟡 MEDIUM - 审计可追溯性风险

---

### 🟡 M-005: 缺少速率限制的实现细节

**位置**: `API.md:604-616`

**问题描述**:

API 文档定义了速率限制：
```
| POST /api/v1/credentials/*/decrypt | 60/分钟 |
```

但代码审查发现：
1. 未在中间件中实现速率限制逻辑
2. 没有基于用户/租户/IP 的细粒度限流
3. 缺少 burst 处理和优雅降级
4. 没有速率限制违规的告警

**攻击场景**:
1. 攻击者获取有效 Token
2. 发起暴力破解尝试（尝试不同凭证 ID）
3. 或发起大量解密请求耗尽 Enclave 资源
4. 系统无法自动阻止或告警

**修复建议**:
- 使用 `tower::limit::RateLimitLayer` 或 `governor` crate
- 实现分层限流：全局、租户、用户、IP
- 对高风险操作（解密）实施更严格的限制
- 超限请求返回 429 并记录审计日志

**严重程度**: 🟡 MEDIUM - DoS 风险

---

## 低风险问题 (LOW)

### 🟢 L-001: `debug_mode` 配置可能导致敏感信息泄露到日志

**位置**: `src/tee/enclave.rs:43`, `src/tee/enclave.rs:252-254`

**问题描述**:
```rust
pub debug_mode: bool, // 禁用安全特性，仅用于开发
```

虽然 `debug_mode` 有注释警告，但没有限制其使用场景：
- 日志中可能输出密钥句柄、用户 ID 等敏感信息
- 没有运行时警告提醒用户当前处于不安全模式

**修复建议**:
- 添加启动时醒目的警告日志
- 在 API 响应头中添加 `X-Debug-Mode: true` 标识（便于客户端识别）
- 考虑编译时完全移除调试代码

**严重程度**: 🟢 LOW

---

### 🟢 L-002: 缺少请求 ID 和链路追踪实现

**位置**: 全局 API 层

**问题描述**:

架构文档提到使用 `requestId` 进行日志追踪，但代码实现中：
1. 没有生成和传播 request ID
2. 无法关联审计日志和具体请求
3. 分布式追踪（trace_id, span_id）缺失

**修复建议**:
- 使用 `tower-http::request_id` 中间件
- 将 request_id 注入请求上下文
- 在审计日志中记录 request_id

**严重程度**: 🟢 LOW - 可观测性改进

---

### 🟢 L-003: 凭证明文数据在内存中停留时间过长

**位置**: `src/tee/enclave.rs:458-524`

**问题描述**:

解密后的明文数据：
```rust
pub fn decrypt_credential(...) -> Result<Vec<u8>, EnclaveError> {
    // ... 解密逻辑
    Ok(plaintext) // Vec<u8> 返回，内存可能长时间存在
}
```

解密后的明文以 `Vec<u8>` 返回，缺少：
1. 返回后的 zeroize 保证
2. 内存锁定（mlock）防止交换到磁盘
3. 使用期限的严格控制

**修复建议**:
- 返回 `Zeroizing<Vec<u8>>` 类型
- 使用 `secrets` crate 进行内存管理
- 考虑使用内存池并立即清零

**严重程度**: 🟢 LOW - 需要复杂的内存分析攻击

---

### 🟢 L-004: 多租户 Schema 切换缺少 SQL 注入防护验证

**位置**: `src/tenant/service.rs` (预期实现)

**问题描述**:

架构文档计划使用 Schema-per-Tenant，但如果在 SQL 查询中直接拼接 schema 名称：
```rust
// 危险示例（假设代码）
let query = format!("SELECT * FROM {}.credentials", tenant_schema);
```

虽然当前代码未完全实现，但需要确保：
1. Schema 名称严格验证（只允许 alphanumeric 和下划线）
2. 使用参数化查询而非字符串拼接
3. 租户 ID 到 Schema 的映射有白名单验证

**修复建议**:
- 实现 schema 名称白名单验证
- 使用连接级别的 `SET search_path TO schema` 而非字符串拼接
- 定期审计 SQL 生成代码

**严重程度**: 🟢 LOW - 当前实现不完整，但需预防

---

## 攻击场景汇总

### 场景 1: 重放攻击链
1. 利用 M-001 的竞态条件重放 Token
2. 利用 H-001 的服务重启状态丢失
3. 绕过 Token 撤销机制获得持久访问

**风险等级**: 🔴 CRITICAL

### 场景 2: 凭证批量泄露
1. 利用 M-005 的缺少速率限制
2. 暴力遍历 credential_id 进行批量解密
3. 利用 L-003 的内存残留提取明文

**风险等级**: 🔴 HIGH

### 场景 3: 审计逃避
1. 利用 M-004 的 reason 字段验证缺失
2. 使用虚假 reason 执行恶意解密
3. 利用 M-003 的审计签名缺失修改审计记录

**风险等级**: 🟡 MEDIUM

---

## 修复优先级建议

### 立即修复 (本周)
1. **H-001**: 实现 Redis 集中式 Token 黑名单
2. **H-002**: 添加模拟模式编译时隔离

### 短期修复 (2周内)
3. **H-003**: 替换 RwLock 并添加超时机制
4. **M-001**: 修复 Token 检查竞态条件
5. **M-005**: 实现 API 速率限制

### 中期修复 (1月内)
6. **H-004**: 设计并实现密钥版本控制
7. **M-002**: 迁移到标准 HKDF
8. **M-003**: 审计日志签名
9. **M-004**: reason 字段验证

### 持续改进
10. **L-001, L-002, L-003, L-004**: 安全加固

---

## 合规性影响

| 标准 | 影响 | 相关发现 |
|------|------|----------|
| SOC 2 Type II | 🔴 高风险 | H-003, M-003, M-004 |
| GDPR | 🟡 中风险 | H-001, L-003 |
| FIPS 140-2 | 🔴 高风险 | M-002, H-004 |
| PCI-DSS | 🟡 中风险 | M-005, M-003 |

---

## 审查结论

CredBridge 在架构设计上采用了业界最佳实践（TEE、四层密钥、PASETO、immudb 审计），但在实现层面存在多个安全漏洞需要修复。特别是 Token 管理、密钥轮换和并发安全方面的缺陷，可能在生产环境中导致严重的安全事件。

**建议**: 在修复所有 HIGH 优先级问题之前，不建议将系统部署到生产环境处理真实凭证数据。

---

*报告生成时间: 2026-03-11*
*审查方法: 静态代码分析 + 架构审查 + 攻击树建模*
*遵循标准: OWASP ASVS 4.0, NIST SP 800-57, SGX 安全编程指南*
