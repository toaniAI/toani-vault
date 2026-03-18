# Phase 2: 问题复现报告

**执行人员**: claude_glm (后端开发)
**执行日期**: 2026-03-12
**问题状态**: 已成功复现并验证

---

## 1. 执行摘要

通过编写自动化测试脚本，成功复现了凭证解密失败问题。测试结果完全验证了 Phase 1 分析报告中确认的三个根本原因。

### 测试结果概览

| 测试项 | 结果 | 说明 |
|--------|------|------|
| 完整加密解密流程复现 | ✓ 通过 | 问题稳定复现，解密返回 `AuthenticationFailed` |
| 多次复现测试（3次） | ✓ 通过 | 3次测试全部失败，验证问题稳定性 |
| 根本原因 1 验证 | ✓ 通过 | Credential ID 不一致导致密钥不匹配 |
| 根本原因 2 验证 | ✓ 通过 | AAD 不一致导致认证失败 |
| 根本原因 3 验证 | ✓ 通过 | L2 派生参数不一致导致密钥不匹配 |

---

## 2. 复现步骤

### 2.1 测试环境

- **项目路径**: `/Users/yvan/AIWorkspace/credbridge`
- **测试文件**: `tests/reproduction_tests.rs`
- **测试命令**: `cargo test --test reproduction_tests -- --nocapture`

### 2.2 测试场景

#### 场景 1: 完整加密解密流程

**步骤**:
1. 初始化密钥层次结构（L0 -> L1）
2. 模拟加密流程：
   - 生成临时 `credential_id`（临时 UUID v7）
   - 使用原始 `user_id` 派生 L2 密钥
   - 使用临时 `credential_id` 派生 L3 密钥
   - 使用原始 `user_id` 构建 AAD
   - 执行 AES-256-GCM 加密
3. 模拟存储流程：
   - 生成新的 `credential_id`（存储 UUID v7）
4. 模拟解密流程：
   - 使用存储的 `credential_id`
   - 使用 hash 后的 `user_id` 派生 L2 密钥
   - 使用存储的 `credential_id` 派生 L3 密钥
   - 使用 hash 后的 `user_id` 构建 AAD
   - 执行 AES-256-GCM 解密

**预期结果**: 解密失败，返回 `AuthenticationFailed`
**实际结果**: 解密失败，返回 `AuthenticationFailed` ✓

#### 场景 2: 多次复现测试

**步骤**: 执行场景 1 共 3 次，验证问题稳定性

**结果**:
- 第 1 次测试: 解密失败
- 第 2 次测试: 解密失败
- 第 3 次测试: 解密失败

**成功率**: 0/3（100% 失败率，符合预期）

---

## 3. 日志输出分析

### 3.1 加密 vs 解密参数对比

以下是典型测试运行的详细日志：

```
测试配置:
  tenant_id: test_tenant_456
  user_id (raw): test_user_123
  user_id (hash): UZbaQHSbWUBpT3NZ9Bwjn59bjlbLT_avlcy5XchPHkI

========== 加密流程 ==========
[ENCRYPT] credential_id (临时): 019ce03c-c9d6-78b1-afe5-f9498253eab8
[ENCRYPT] L2 派生参数: tenant_id=test_tenant_456, user_id=test_user_123
[ENCRYPT] L2 info: user-vault-key:v1:test_tenant_456:test_user_123
[ENCRYPT] L3 派生参数: credential_id=019ce03c-c9d6-78b1-afe5-f9498253eab8
[ENCRYPT] L3 info: credential-key:v1:019ce03c-c9d6-78b1-afe5-f9498253eab8:CredentialEncryption
[ENCRYPT] AAD: test_tenant_456:test_user_123

加密成功！
  temp_cred_id: 019ce03c-c9d6-78b1-afe5-f9498253eab8
  密文长度: 34 bytes

存储后生成的 credential_id: 019ce03c-c9d6-78b1-afe5-f960e1a67cff

========== 解密流程 ==========
[DECRYPT] credential_id (存储): 019ce03c-c9d6-78b1-afe5-f960e1a67cff
[DECRYPT] L2 派生参数: tenant_id=test_tenant_456, user_id=UZbaQHSbWUBpT3NZ9Bwjn59bjlbLT_avlcy5XchPHkI
[DECRYPT] L2 info: user-vault-key:v1:test_tenant_456:UZbaQHSbWUBpT3NZ9Bwjn59bjlbLT_avlcy5XchPHkI
[DECRYPT] L3 派生参数: credential_id=019ce03c-c9d6-78b1-afe5-f960e1a67cff
[DECRYPT] L3 info: credential-key:v1:019ce03c-c9d6-78b1-afe5-f960e1a67cff:CredentialEncryption
[DECRYPT] AAD: test_tenant_456:UZbaQHSbWUBpT3NZ9Bwjn59bjlbLT_avlcy5XchPHkI
```

### 3.2 参数对比表

| 参数 | 加密时值 | 解密时值 | 是否一致 | 根本原因 |
|------|----------|----------|----------|----------|
| credential_id | `019ce03c-...f9498253eab8` | `019ce03c-...f960e1a67cff` | ✗ | 根本原因 1 |
| L2 user_id 参数 | `test_user_123` | `UZbaQHSbWUBp...` | ✗ | 根本原因 3 |
| L3 credential_id 参数 | `...f9498253eab8` | `...f960e1a67cff` | ✗ | 根本原因 1 |
| AAD | `test_tenant_456:test_user_123` | `test_tenant_456:UZbaQHSb...` | ✗ | 根本原因 2 |

---

## 4. 根本原因验证

### 4.1 根本原因 1: Credential ID 不一致

**验证测试**: `test_root_cause_1_credential_id_mismatch`

**测试方法**: 保持其他参数一致，仅改变 credential_id

**日志输出**:
```
加密时 credential_id: 019ce03c-c9d6-78b1-afe5-f9277c21b7e8
解密时 credential_id: 019ce03c-c9d6-78b1-afe5-f956c78be3fb
两个 ID 是否相同: false

L3 密钥派生 info 对比:
  加密 info: credential-key:v1:019ce03c-...f9277c21b7e8:CredentialEncryption
  解密 info: credential-key:v1:019ce03c-...f956c78be3fb:CredentialEncryption

结果: Err(AuthenticationFailed)
```

**验证结果**: ✓ 使用不同 credential_id 派生的密钥无法解密

### 4.2 根本原因 2: AAD 不一致

**验证测试**: `test_root_cause_2_aad_mismatch`

**测试方法**: 保持密钥一致，仅改变 AAD

**日志输出**:
```
原始 user_id: user_456
hash user_id: lhzxZCQP8a8sFPFPfouJQ-rQt4iswo9bBJWpe1Y93yU

加密时 AAD: tenant_123:user_456
解密时 AAD: tenant_123:lhzxZCQP8a8sFPFPfouJQ-rQt4iswo9bBJWpe1Y93yU
两个 AAD 是否相同: false

结果: Err(AuthenticationFailed)
```

**验证结果**: ✓ 使用不同 AAD 解密失败

### 4.3 根本原因 3: L2 密钥派生参数不一致

**验证测试**: `test_root_cause_3_l2_derivation_mismatch`

**测试方法**: 使用不同 user_id 参数派生 L2 密钥，验证 L3 密钥差异

**日志输出**:
```
原始 user_id: user_456
hash user_id: lhzxZCQP8a8sFPFPfouJQ-rQt4iswo9bBJWpe1Y93yU

加密时 L2 info: user-vault-key:v1:tenant_123:user_456
解密时 L2 info: user-vault-key:v1:tenant_123:lhzxZCQP8a8sFPFPfouJQ-rQt4iswo9bBJWpe1Y93yU

L3 密钥派生 info 对比:
  加密 L3 info: credential-key:v1:cred_789:CredentialEncryption
  解密 L3 info: credential-key:v1:cred_789:CredentialEncryption
  注意：虽然 L3 info 相同，但由于 L2 密钥不同，最终 L3 密钥也不同

结果: Err(AuthenticationFailed)
```

**验证结果**: ✓ 使用不同 L2 密钥派生的 L3 密钥无法解密

---

## 5. 验证正确性

### 5.1 使用一致参数解密

**测试方法**: 使用与加密完全相同的参数进行解密

**日志输出**:
```
========== 正确解密流程（使用一致参数） ==========
[DECRYPT-CORRECT] credential_id: 019ce03c-c9d6-78b1-afe5-f9498253eab8
[DECRYPT-CORRECT] L2 派生参数: tenant_id=test_tenant_456, user_id=test_user_123
[DECRYPT-CORRECT] L3 派生参数: credential_id=019ce03c-c9d6-78b1-afe5-f9498253eab8
[DECRYPT-CORRECT] AAD: test_tenant_456:test_user_123

✓ 解密成功
  解密数据: "my secret credential data"
```

**验证结果**: ✓ 使用一致参数可以成功解密

---

## 6. Phase 1 分析验证结论

### 6.1 验证结果总结

| Phase 1 分析结论 | 验证结果 | 证据 |
|------------------|----------|------|
| 根本原因 1: Credential ID 不一致 | ✓ 验证通过 | 临时 ID ≠ 存储 ID，HKDF 产生不同密钥 |
| 根本原因 2: AAD 不一致 | ✓ 验证通过 | 原始 user_id ≠ hash，AES-GCM 认证失败 |
| 根本原因 3: L2 派生参数不一致 | ✓ 验证通过 | 原始 user_id ≠ hash，产生不同的 L2/L3 密钥 |

### 6.2 问题复现确认

**确认**: 问题已稳定复现，三个根本原因均得到验证。

### 6.3 代码问题定位

| 文件 | 行号 | 问题描述 |
|------|------|----------|
| `src/api/credentials.rs` | 209 | 生成临时 credential_id 用于加密 |
| `src/api/credentials.rs` | 216 | AAD 使用原始 user_id |
| `src/api/credentials.rs` | 204-206 | L2 派生使用原始 user_id |
| `src/api/credentials.rs` | 417 | L3 派生使用存储的 credential_id |
| `src/api/credentials.rs` | 409 | L2 派生使用 hash 后的 user_id |
| `src/api/credentials.rs` | 424 | AAD 使用 hash 后的 user_id |

---

## 7. 下一步建议

### 7.1 修复方案

推荐 Phase 1 报告中的**综合修复方案**：

1. **统一 credential_id**: 在加密前先生成 VaultEntry，使用存储的 credential_id 进行加密
2. **统一 AAD**: 加密时也使用 `user_id.hash()` 构建 AAD
3. **统一 L2 派生参数**: 加密时也使用 `user_id.hash()` 派生 L2 密钥

### 7.2 测试覆盖

建议在修复后添加以下测试：

1. 加解密往返测试（使用一致参数）
2. 参数一致性检查测试
3. 边界条件测试（不同 tenant/user 组合）

---

## 8. 附录

### 8.1 测试文件

- **路径**: `/Users/yvan/AIWorkspace/credbridge/tests/reproduction_tests.rs`
- **测试数量**: 5 个
- **通过率**: 100%

### 8.2 运行命令

```bash
cd /Users/yvan/AIWorkspace/credbridge
cargo test --test reproduction_tests -- --nocapture
```

### 8.3 完整测试输出

```
running 5 tests
test test_root_cause_1_credential_id_mismatch ... ok
test test_root_cause_2_aad_mismatch ... ok
test test_root_cause_3_l2_derivation_mismatch ... ok
test test_reproduce_decryption_failure ... ok
test test_reproduce_multiple_times ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

---

**报告完成时间**: 2026-03-12
**验证人员签名**: claude_glm