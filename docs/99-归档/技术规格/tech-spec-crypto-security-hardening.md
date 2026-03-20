---
title: '密码学安全加固：PBKDF2 标准实现 + 前端安全评分优化'
slug: 'crypto-pbkdf2-and-frontend-security-score'
created: '2026-03-19'
status: 'implemented'
priority: 'P2'
---

# Tech-Spec: 密码学安全加固

## 概述

本规格涵盖后端 PBKDF2 简化实现和前端安全评分算法的完善。两者共同影响系统的密码学安全性（一个直接影响密钥安全，另一个影响安全可见性），因此合并为一个规格。

---

## 问题 1：PBKDF2 简化实现（中优先级）

### 问题陈述

`src/crypto/key_derivation.rs` 中的 `pbkdf2_hmac_sha256()` 函数没有实现真正的 PBKDF2 算法，而是仅做了一次 SHA256 哈希（将 password、salt、iterations 拼接后哈希）：

```rust
// src/crypto/key_derivation.rs:402-414
/// PBKDF2-HMAC-SHA256 实现（简化版）
fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    // 在实际实现中使用标准的 PBKDF2 实现
    // 这里简化处理，仅用于演示
    let mut data = Vec::new();
    data.extend_from_slice(password);
    data.extend_from_slice(salt);
    data.extend_from_slice(&iterations.to_be_bytes());

    let hash = digest(&SHA256, &data);
    let len = output.len().min(hash.as_ref().len());
    output[..len].copy_from_slice(&hash.as_ref()[..len]);
}
```

**安全影响（严重）**：
1. `iterations` 参数完全无效 —— 仅作为字节拼入哈希，不执行迭代拉伸
2. 实际上只做了一次 SHA256，而 PBKDF2 的意义在于通过大量迭代（如 600,000 次）增加暴力破解成本
3. 此函数若用于凭证密钥派生，会使派生密钥的安全性大打折扣

```rust
// 同文件 416-423：get_random_bytes 也有注释说明"简化处理"
fn get_random_bytes(buffer: &mut [u8]) {
    // 在实际实现中使用密码学安全的随机数生成器
    // 这里简化处理
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    rng.fill_bytes(buffer);
}
```

注意：`get_random_bytes` 实际上已使用了 `rand::thread_rng()`，虽然注释说"简化处理"，但 `rand::thread_rng()` 在支持平台上会使用密码学安全的随机源（OsRng）。主要问题是 `pbkdf2_hmac_sha256`。

### 解决方案

使用项目已依赖的 `ring` crate 提供的 PBKDF2 实现（`ring::pbkdf2`），或使用 `pbkdf2` crate。`ring` 已在项目中使用（可见 `use ring::digest::{SHA256, digest}`），优先使用 `ring::pbkdf2`。

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/crypto/key_derivation.rs` | 主要修改目标（402-423 行） |
| `Cargo.toml` | 确认 `ring` crate 版本和已启用的 features |
| `src/crypto/` 下其他文件 | 了解密钥派生在整体密码学模块中的调用关系 |

### Ring PBKDF2 API 参考

```rust
use ring::pbkdf2;
use std::num::NonZeroU32;

// ring 中的 PBKDF2-HMAC-SHA256
static PBKDF2_ALG: pbkdf2::Algorithm = pbkdf2::PBKDF2_HMAC_SHA256;

fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    let n = NonZeroU32::new(iterations).unwrap_or(NonZeroU32::new(600_000).unwrap());
    pbkdf2::derive(PBKDF2_ALG, n, salt, password, output);
}
```

**关键参数**：
- 最小迭代次数：OWASP 2023 建议 PBKDF2-SHA256 使用 **600,000** 次迭代
- 输出长度：通常 32 字节（256 位），用于 AES-256 密钥派生

### 实现计划

**任务 1**：确认 `Cargo.toml` 中 `ring` 的 feature 包含 `pbkdf2`（ring 默认包含，通常不需要额外配置）。

**任务 2**：将 `pbkdf2_hmac_sha256()` 函数替换为使用 `ring::pbkdf2::derive()`：

```rust
use ring::pbkdf2;
use std::num::NonZeroU32;

static PBKDF2_ALG: pbkdf2::Algorithm = pbkdf2::PBKDF2_HMAC_SHA256;

fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    let n = NonZeroU32::new(iterations)
        .unwrap_or_else(|| NonZeroU32::new(600_000).expect("600_000 is non-zero"));
    pbkdf2::derive(PBKDF2_ALG, n, salt, password, output);
}
```

**任务 3**：确认调用方传入的 `iterations` 值是否合理（建议至少 600,000）。若调用方传入值过小（如测试中使用 1），需要在文档中注明最小值要求，或在函数内部强制下限。

**任务 4**：更新函数注释，移除"简化版"和"仅用于演示"的说明。

**任务 5**：检查 `get_random_bytes()` 的注释，更新为准确描述（`rand::thread_rng()` 实际上是密码学安全的，注释误导）：

```rust
/// 生成密码学安全的随机字节
fn get_random_bytes(buffer: &mut [u8]) {
    use rand::RngCore;
    rand::thread_rng().fill_bytes(buffer); // 使用 OsRng 作为底层熵源
}
```

### 验收标准

- [ ] `pbkdf2_hmac_sha256(password, salt, 1000, output)` 与 `pbkdf2_hmac_sha256(password, salt, 2000, output)` 的结果不同（证明 `iterations` 参数真正生效）
- [ ] 相同输入（password、salt、iterations）得到相同输出（确定性）
- [ ] 不同 salt 得到不同输出（盐值有效）
- [ ] 生产代码路径使用 >= 600,000 次迭代
- [ ] 函数注释不再包含"简化"或"演示"字样

---

## 问题 2：前端安全评分简化算法（低优先级）

### 问题陈述

`frontend/src/shared/api/services.ts` 中的 `calculateSecurityScore()` 函数使用了简化的安全评分算法：

```typescript
// frontend/src/shared/api/services.ts:183-189
function calculateSecurityScore(auditLogs: AuditLogEntry[], credentialCount: number): number {
  let score = 100;

  // 检查最近的失败认证
  const recentFailedAuths = auditLogs.filter(
    (entry) => entry.action === 'FailedAuth' && entry.outcome === 'Success'
    //                                                             ^^^^^^
    //                                           注意：这里过滤的是 outcome='Success'
    //                                           逻辑可能有误（Failed Auth with Success outcome?）
  );
```

**问题**：
1. 过滤条件 `entry.outcome === 'Success'` 在 `FailedAuth` 动作上可能是逻辑错误（应过滤 `outcome === 'Failure'`）
2. 评分仅基于客户端已获取的有限日志（可能未覆盖完整时间窗口），缺乏准确性
3. `active_tokens: 0` 注释说明后端暂无此接口，评分维度不完整

### 解决方案

**短期**：修复已知的逻辑错误（`FailedAuth` 的 `outcome` 过滤条件），完善现有评分维度的权重设计，并添加文档注释说明算法局限性。

**长期**：将安全评分计算移至后端 API，后端有完整的事件数据，前端仅展示。这需要后端新增 `GET /dashboard/security-score` 接口（超出本规格范围，记录为后续任务）。

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `frontend/src/shared/api/services.ts` | 主要修改目标（160-200 行） |
| `frontend/src/` 中的类型定义文件 | `AuditLogEntry` 类型定义，确认 `action` 和 `outcome` 字段的枚举值 |
| `src/api/audit_models.rs` | 后端对应的 action/outcome 枚举值（前后端对齐） |

### 实现计划

**任务 1**：阅读 `AuditLogEntry` 类型定义，确认 `action` 和 `outcome` 字段的合法值。

**任务 2**：修复过滤条件逻辑错误：
```typescript
// 修改前（可能有误）
const recentFailedAuths = auditLogs.filter(
  (entry) => entry.action === 'FailedAuth' && entry.outcome === 'Success'
);

// 修改后（验证后的正确条件）
const recentFailedAuths = auditLogs.filter(
  (entry) => entry.action === 'TokenValidate' && entry.outcome === 'Failure'
  // 具体字段值需对照 AuditLogEntry 类型确认
);
```

**任务 3**：完善评分维度，补充合理的扣分规则：
- 最近 24 小时失败认证次数（已有，修复逻辑后）
- 高风险操作比例（`risk_tier === 'High'`）
- 凭证数量超阈值（超过 100 个凭证时适度扣分）
- 长期未使用的高权限凭证（需要 `last_accessed` 字段支持）

**任务 4**：在函数头部添加注释，说明算法局限性（基于有限客户端数据，非权威评分），并标注 TODO 指向后端迁移的长期计划。

**任务 5**：为 `calculateSecurityScore()` 添加单元测试（纯函数，易于测试）。

### 验收标准

- [ ] `FailedAuth` / `TokenValidate` 失败事件正确降低评分（当前可能逻辑相反）
- [ ] 正常操作不意外降低评分
- [ ] 评分范围在 0-100 之间，不出现负数或超过 100 的情况
- [ ] 函数有明确注释说明局限性

---

## 附加上下文

### 依赖

- 问题 1（PBKDF2）：需要密码学安全审查；**此为高安全优先级修复**，建议尽快处理
- 问题 2（安全评分）：独立，前端修改不影响后端

### 测试策略

**问题 1（PBKDF2）**：
- 单元测试（必须）：
  ```rust
  #[test]
  fn test_pbkdf2_iterations_matter() {
      let mut out1 = [0u8; 32];
      let mut out2 = [0u8; 32];
      pbkdf2_hmac_sha256(b"password", b"salt", 1000, &mut out1);
      pbkdf2_hmac_sha256(b"password", b"salt", 2000, &mut out2);
      assert_ne!(out1, out2, "不同迭代次数应产生不同输出");
  }

  #[test]
  fn test_pbkdf2_deterministic() {
      let mut out1 = [0u8; 32];
      let mut out2 = [0u8; 32];
      pbkdf2_hmac_sha256(b"password", b"salt", 100_000, &mut out1);
      pbkdf2_hmac_sha256(b"password", b"salt", 100_000, &mut out2);
      assert_eq!(out1, out2, "相同输入应产生相同输出");
  }
  ```

**问题 2（安全评分）**：
- 单元测试：
  - 空日志 → 满分 100
  - 包含多次失败认证 → 分数降低
  - 分数不低于 0

### 注意事项

- **PBKDF2 修复是安全关键变更**，需要代码审查员重点检查迭代次数配置
- 若存量数据使用了旧的简化 PBKDF2 派生密钥，需要评估是否需要重新派生（密钥轮换）；这是一个重要的迁移风险，需在实施前与安全团队确认
- 前端安全评分在后端 API 就绪前是临时方案，应避免在安全报告中引用此评分
