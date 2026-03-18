# CredBridge 边界情况分析报告

**生成时间**: 2026-03-11
**分析范围**: Core Service, SDKs, API Handlers
**分析方法**: 穷举路径追踪

---

## 执行摘要

本次边界情况分析遍历了 CredBridge 核心代码的所有分支路径，识别出 **15 个未处理边界情况**。其中:
- 🔴 高风险: 2 个
- 🟡 中风险: 8 个
- 🟢 低风险: 5 个

---

## 发现清单 (JSON 格式)

```json
[
  {
    "location": "src/api/credentials.rs:374",
    "trigger_condition": "解密后的明文不是有效 UTF-8",
    "guard_snippet": "String::from_utf8(plaintext_bytes).map_err(|_| ApiError::new(\"invalid_ciphertext\", \"凭证内容编码错误\"))?",
    "potential_consequence": "凭证内容被截断或替换",
    "severity": "high"
  },
  {
    "location": "src/api/middleware.rs:96",
    "trigger_condition": "系统时间早于 Unix epoch",
    "guard_snippet": "SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::from_secs(0))",
    "potential_consequence": "系统 panic 或 Token 验证失败",
    "severity": "medium"
  },
  {
    "location": "src/api/token_blacklist.rs:94",
    "trigger_condition": "系统时间早于 Unix epoch",
    "guard_snippet": "SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()",
    "potential_consequence": "TTL 计算错误导致黑名单失效",
    "severity": "medium"
  },
  {
    "location": "src/vault/storage.rs:557",
    "trigger_condition": "系统时间早于 Unix epoch",
    "guard_snippet": "SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::from_secs(0))",
    "potential_consequence": "时间戳计算错误影响凭证过期",
    "severity": "medium"
  },
  {
    "location": "src/token/redis_store.rs:511",
    "trigger_condition": "系统时间早于 Unix epoch",
    "guard_snippet": "SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::from_secs(0))",
    "potential_consequence": "Token TTL 计算错误",
    "severity": "medium"
  },
  {
    "location": "src/api/credentials.rs:148-155",
    "trigger_condition": "加密凭证时 plaintext_data 为空对象",
    "guard_snippet": "if request.plaintext_data.is_null() || request.plaintext_data.as_object().map(|o| o.is_empty()).unwrap_or(true) { return Err(ApiError::new(\"invalid_request\", \"凭证内容不能为空\")); }",
    "potential_consequence": "创建空凭证浪费存储空间",
    "severity": "low"
  },
  {
    "location": "src/api/credentials.rs:159-163",
    "trigger_condition": "service_id 为空字符串或超长",
    "guard_snippet": "if request.service_id.is_empty() || request.service_id.len() > 256 { return Err(ApiError::new(\"invalid_request\", \"service_id 长度必须在 1-256 之间\")); }",
    "potential_consequence": "存储无效标识符或 DoS",
    "severity": "medium"
  },
  {
    "location": "src/token/revocation.rs:248-270",
    "trigger_condition": "批量撤销时 jtis 列表为空",
    "guard_snippet": "if jtis.is_empty() { return Ok(RevocationResult::empty()); }",
    "potential_consequence": "无意义的批量操作消耗资源",
    "severity": "low"
  },
  {
    "location": "src/token/revocation.rs:283-309",
    "trigger_condition": "用户有海量活跃 Token 时遍历",
    "guard_snippet": "if jtis.len() > self.policy.batch_limit { return Err(RevocationError::BatchLimitExceeded { max: self.policy.batch_limit, requested: jtis.len() }); }",
    "potential_consequence": "内存溢出或服务超时",
    "severity": "high"
  },
  {
    "location": "src/api/middleware.rs:137-152",
    "trigger_condition": "Authorization header 超长 (>16KB)",
    "guard_snippet": "if auth_header.len() > 16384 { return Err(AuthError::new(\"invalid_token\", \"Token 过长\")); }",
    "potential_consequence": "内存 DoS 攻击",
    "severity": "medium"
  },
  {
    "location": "sdk-rust/src/client.rs:414-423",
    "trigger_condition": "Token 格式无效但长度足够",
    "guard_snippet": "if parts.len() < 3 { return Err(CredBridgeError::new(InvalidToken, \"Invalid PASETO token format\")); }",
    "potential_consequence": "空值传播导致后续操作失败",
    "severity": "low"
  },
  {
    "location": "sdk-typescript/src/client.ts:399-429",
    "trigger_condition": "Token payload Base64 解码失败",
    "guard_snippet": "try { payload = JSON.parse(Buffer.from(parts[2], 'base64url').toString()); } catch (e) { console.warn('Token parse failed:', e); return; }",
    "potential_consequence": "静默失败导致 tokenInfo 未初始化",
    "severity": "medium"
  },
  {
    "location": "src/crypto/cipher.rs:69-90",
    "trigger_condition": "Base64 解码失败返回错误",
    "guard_snippet": "URL_SAFE_NO_PAD.decode(&self.nonce).map_err(|_| CryptoError::InvalidCiphertext)?",
    "potential_consequence": "错误类型不够具体难以调试",
    "severity": "low"
  },
  {
    "location": "src/audit/immudb_store.rs:179-205",
    "trigger_condition": "并发写入时锁被毒化",
    "guard_snippet": "self.storage.lock().map_err(|e| RecorderError::StorageError(format!(\"Storage lock poisoned: {}\", e)))?",
    "potential_consequence": "未处理锁毒化导致操作失败",
    "severity": "medium"
  },
  {
    "location": "src/vault/storage.rs:175-188",
    "trigger_condition": "并发写入时 RwLock 被毒化",
    "guard_snippet": "self.entries.write().map_err(|_| VaultError::StorageError(\"Lock poisoned\"))?.insert(...)",
    "potential_consequence": "服务不可用状态",
    "severity": "medium"
  }
]
```

---

## 按模块分类

### API 层 (src/api/)

| 文件 | 边界情况数 | 主要问题 |
|------|-----------|----------|
| credentials.rs | 3 | 空内容、超长 service_id、非 UTF-8 内容 |
| middleware.rs | 2 | 系统时间 panic、超长 Token |
| token_blacklist.rs | 1 | 系统时间回退 |

### Token 管理 (src/token/)

| 文件 | 边界情况数 | 主要问题 |
|------|-----------|----------|
| revocation.rs | 2 | 空列表、海量 Token 遍历 |
| redis_store.rs | 1 | 系统时间回退 |

### Vault 存储 (src/vault/)

| 文件 | 边界情况数 | 主要问题 |
|------|-----------|----------|
| storage.rs | 2 | 系统时间 panic、锁毒化 |

### SDKs

| SDK | 边界情况数 | 主要问题 |
|-----|-----------|----------|
| sdk-rust | 1 | Token 格式解析不完整 |
| sdk-typescript | 1 | Token 解析静默失败 |

### 审计 (src/audit/)

| 文件 | 边界情况数 | 主要问题 |
|------|-----------|----------|
| immudb_store.rs | 1 | 锁毒化处理 |

---

## 修复优先级建议

### 🔴 P0 - 立即修复

1. **系统时间 unwrap() 调用** (middleware.rs:96, storage.rs:557, redis_store.rs:511)
   - 使用 `unwrap_or()` 替换 `unwrap()` 或 `expect()`
   - 添加备用时间源或降级策略

2. **批量撤销无限制遍历** (revocation.rs:283-309)
   - 添加分页或流式处理
   - 实施 batch_limit 强制限制

### 🟡 P1 - 本周修复

3. **凭证非 UTF-8 内容处理** (credentials.rs:374)
   - 使用 `String::from_utf8()` 并正确处理错误

4. **Token 长度限制** (middleware.rs:137)
   - 添加最大 Token 长度验证

5. **SDK Token 解析错误处理** (sdk-rust, sdk-typescript)
   - 统一错误处理策略

6. **空值输入验证** (credentials.rs:148)
   - 添加前置校验

7. **锁毒化处理** (immudb_store.rs, storage.rs)
   - 评估锁恢复策略或优雅降级

### 🟢 P2 - 下个迭代

8. **空列表处理** (revocation.rs:248)
   - 提前返回优化

9. **Base64 错误细化** (cipher.rs)
   - 添加具体错误类型

10. **超长 service_id** (credentials.rs)
    - 添加长度限制

---

## 测试建议

针对发现的边界情况，建议添加以下测试用例:

```rust
// 1. 系统时间边界测试
#[test]
fn test_system_time_before_epoch() {
    // 模拟系统时间回退场景
}

// 2. 超大 Token 测试
#[test]
fn test_oversized_token_rejection() {
    // 验证 16KB+ Token 被拒绝
}

// 3. 非 UTF-8 凭证内容测试
#[test]
fn test_non_utf8_credential_content() {
    // 验证正确返回错误而非 panic
}

// 4. 海量 Token 批量撤销测试
#[test]
fn test_mass_revocation_limits() {
    // 验证超过 batch_limit 时正确拒绝
}

// 5. 并发锁毒化测试
#[test]
fn test_lock_poisoning_recovery() {
    // 验证锁毒化后的优雅处理
}
```

---

## 结论

本次分析共识别出 15 个未处理的边界情况，主要集中在:

1. **系统时间依赖**: 多处使用 `unwrap()` 或 `expect()` 获取系统时间，存在 panic 风险
2. **输入验证不足**: 缺少对空值、长度、格式的严格验证
3. **资源限制缺失**: 批量操作缺少限制保护
4. **并发边界**: 锁毒化处理不完善

建议优先修复 P0 级别问题，然后逐步处理其他级别。

---

*报告生成: Edge Case Hunter (BMAD Review Workflow)*
