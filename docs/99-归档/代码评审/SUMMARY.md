# CredBridge 代码审查总报告

**审查日期**: 2026-03-19
**审查范围**: 34 个修改文件，覆盖 18 份 Tech-Spec
**审查方法**: 6 个并发对抗性代码审查 Agent

---

## 一、执行摘要

| 模块 | CRITICAL | HIGH | MEDIUM | LOW | Tech-Spec 符合率 |
|------|---------|------|--------|-----|----------------|
| MCP Server | 1 | 4 | 4 | 3 | 60% |
| TEE 核心 | 5 | 7 | 8 | 8 | 14% (7份均有缺口) |
| API 层 | 1 | 6 | 8 | 5 | 70% |
| Vault & Crypto | 2 | 4 | 6 | 4 | 75% |
| 基础设施 | 0 | 4 | 9 | 7 | 85% |
| LLM 服务 | 0 | 2 | 5 | 4 | N/A |
| **合计** | **9** | **27** | **40** | **31** | — |

**核心结论**：本次变更完成了部分有价值的安全修复（Token 加密、签名验证、PBKDF2 替换等），但 **7 份 Tech-Spec 全部标记为 `implemented` 而实际均有关键缺口**，存在严重的文档与代码状态不同步问题。TEE 核心模块的认证安全缺陷（CRIT-001~004）构成生产部署的直接阻断风险。

---

## 二、P0 阻塞问题（生产禁止部署）

以下问题必须在合并/部署前修复：

### 🔴 TEE 认证安全漏洞（CRIT-001 ~ CRIT-004）

1. **签名验证 Fail-Open**（`src/tee/attestation.rs:940`）
   无公钥配置时任意非零签名静默通过，伪造 Quote 可骗过认证系统。
   → **修复**: `verifier_public_key` 为 None 时返回 Err

2. **证书链验证空实现**（`src/tee/dcap.rs:927`）
   `verify_certificate_chain()` 直接 `return Ok(())`，任何自签名证书均通过。
   → **修复**: 集成 `webpki`，用 Intel SGX 根证书验证 PCK 链

3. **Quote ECDSA 仅检查零值**（`src/tee/dcap.rs:898-914`）
   `verify_quote_signature()` 不做真实 ECDSA 验证，任意非零 r/s 通过。
   → **修复**: 使用 `ring::signature::UnparsedPublicKey` 执行真实验证

4. **Nonce 验证空桩**（`src/tee/dcap.rs`）
   `verify_nonce()` 为空实现，历史 Quote 可无限重放。
   → **修复**: 验证 report_data 与当前挑战值匹配，设置 5 分钟有效窗口

5. **ECDSA 双重哈希导致驱动验证始终失败**（`src/tee/driver_verify.rs:257`）
   `ECDSA_P256_SHA256_ASN1` 会对输入再次 SHA256，而输入已是哈希值，导致验证永远失败。
   → **修复**: 传入原始数据或使用接受预哈希输入的算法

### 🔴 Vault 后端死锁风险（FINDING-08）

6. **`block_on` 在异步上下文死锁**（`src/vault/backend.rs:94-101`）
   `StorageBackend` 同步接口包装异步 Vault 客户端，Tokio 多线程运行时禁止此模式。
   → **修复**: 将 `StorageBackend` trait 改为 async，或使用 `spawn_blocking`

### 🔴 MCP 核心功能缺失

7. **MCP 请求分发未实现**（`mcp-server/src/sse.rs:297`）
   `_msg_rx` 被丢弃，客户端发送的所有 MCP 工具调用永远得不到响应。
   → **修复**: 实现后台任务消费 msg_rx，调用 ToolHandler 并推送 SSE 响应

---

## 三、P1 高优先级问题（Sprint 内修复）

### 安全类

| # | 文件 | 问题 | 严重度 |
|---|------|------|--------|
| 8 | `mcp-server/src/tools.rs:204` | 解密后明文 `Vec<u8>` 未 zeroize | CRITICAL |
| 9 | `src/mcp/token_storage.rs:295` | 临时加密密钥未 zeroize | CRITICAL |
| 10 | `src/vault/client.rs:41` | `VaultConfig.token` 未 Zeroize 且 Debug trait 会打印 Token | HIGH |
| 11 | `src/tee/sandbox/export/watermark.rs` | `verify_watermark()` 空实现始终返回 `Ok(true)` | HIGH |
| 12 | `src/tee/sandbox/export/watermark.rs:480` | HMAC 密钥从公开的 `enclave_id` 派生，可被伪造 | HIGH |
| 13 | `src/tee/sandbox/export/data_export.rs:340` | 清单哈希使用非加密 `DefaultHasher` | HIGH |
| 14 | `src/api/audit.rs:410,567` | `usize::MAX` 读取 body，OOM DoS 风险 | HIGH |
| 15 | `src/api/credentials.rs:163` | `try_lock` 导致高敏感操作审计日志可丢失 | HIGH |
| 16 | `src/api/websocket.rs:219` | WebSocket 端点无 scope 权限检查 | HIGH |
| 17 | `src/api/websocket.rs:288` | session_id/credential_id 未验证用户所有权 | HIGH |
| 18 | `src/services/llm/openai.rs:96` | pricing 配置被忽略，成本控制完全失效 | HIGH |
| 19 | `src/services/llm/azure.rs:111` | `build_headers()` 中 `.expect()` 生产 panic 路径 | HIGH |

### 功能类

| # | 文件 | 问题 |
|---|------|------|
| 20 | `vault-service/src/api/attestation.rs:331,335,339` | SGX/TDX/SEV-SNP Quote 生成全部返回 Err，TEE 认证完全不可用 |
| 21 | `src/tee/upgrade.rs:542` | 回滚未从 Vault 恢复 Sealing Key，数据可能不可达 |
| 22 | `src/tee/sandbox` | `execute_operation` 和 `take_screenshot` 仍为模拟实现 |
| 23 | `src/main.rs:207` | `init_logging()` 未真正初始化日志系统，所有 warn 均丢失 |
| 24 | `src/main.rs:240,408` | 生产环境硬编码内存存储，重启数据全失 |

---

## 四、Tech-Spec 符合性矩阵

| Tech-Spec | 标记状态 | 实际状态 | 主要差距 |
|-----------|---------|---------|---------|
| ts-001: MCP TEE 解密 | implemented | **部分实现** | 明文未 zeroize；tee_verified 硬编码 true；mrenclave 仍为占位符 |
| ts-002: SSE Token 验证与请求分发 | implemented | **部分实现** | P1(Token 验证)已完成；P2(请求分发)仍为占位符 |
| tee-attestation-quote-generation | implemented | **未实现** | 所有平台返回 Err；无 SDK 依赖；无 report_data 参数 |
| tee-attestation-verification-hardening | implemented | **部分实现** | ECDSA 验证有 3 个绕过路径；webpki 未集成 |
| tee-driver-verify-public-key-loading | implemented | **替代方案** | 用环境变量替代 Vault；同步函数；无缓存 |
| tee-upgrade-pipeline-implementation | implemented | **部分实现** | 渐进迁移逻辑失效；Vault 回滚未实现 |
| tech-spec-tee-106-watermark-verification | implemented | **未实现** | `verify_watermark` 为空实现返回 `Ok(true)` |
| tech-spec-tee-107-websocket-sandbox-execute | implemented | **未实现** | `execute_operation` 仍为 sleep(1500ms) 模拟 |
| tech-spec-tee-108-websocket-screenshot | implemented | **未实现** | `take_screenshot` 返回 1x1 像素占位 PNG |
| tech-spec-audit-security-context | implemented | **已完成** | content_hash 真实计算已实现 ✓ |
| tech-spec-audit-signature-verification | implemented | **已完成** | Ed25519 真实签名验证已实现 ✓ |
| tech-spec-crypto-security-hardening | implemented | **已完成** | PBKDF2 替换为 ring 标准实现 ✓ |
| tech-spec-token-storage-encryption | implemented | **已完成** | AES-256-GCM 加密正确实现 ✓ |
| tech-spec-vault-backend-fixes | implemented | **部分实现** | updated_at 修复 ✓；block_on 死锁风险未修复 |
| tech-spec-infra-101-redis-tenant-cache | implemented | **部分实现** | 缓存读写正确；失败时无 warn 日志 |
| tech-spec-screenshot-mock-data | — | 待核查 | — |
| tech-spec-p3-low-priority | — | 待核查 | — |
| tech-spec-immudb-tokio-mutex | — | 待核查 | — |

---

## 五、值得肯定的修复（已正确实现）

1. **AES-256-GCM Token 加密**（`src/mcp/token_storage.rs`）- 防篡改测试覆盖完整
2. **Ed25519 审计日志签名验证**（`src/api/audit.rs`）- 已从假验证升级为真实 ring 签名验证
3. **PBKDF2 标准实现**（`src/crypto/key_derivation.rs`）- 迭代次数真正生效
4. **SSE Token 验证**（`mcp-server/src/sse.rs`）- mock claims 已移除，调用真实 `TokenValidator`
5. **MCP TEE 解密核心逻辑**（`mcp-server/src/tools.rs`）- L2/L3 密钥派生流程正确
6. **审计日志 JTI**（`src/api/credentials.rs`）- 已从 `token.token_id` 获取真实值
7. **Redis 租户配置缓存**（`src/tenant/config.rs`）- SCAN 迭代、TTL 设置实现正确

---

## 六、测试质量问题

| 测试文件 | 关键缺口 |
|---------|---------|
| `tests/api/audit_tests.rs` | 无签名验证断言；空公钥测试不验证 `verified` 字段值 |
| `tests/token/redis_store_tests.rs` | 无并发安全测试；使用 sleep(2s) 不确定 |
| `mcp-server/tests/mcp_sse_api_test.rs` | 过期 Token 测试未通过 HTTP 层验证 401 |

---

## 七、推荐修复顺序

**立即修复（本周）**
- P0 安全漏洞：CRIT-001~004（TEE 认证绕过）
- P0 功能：MCP 请求分发（ts-002 P2 核心功能）
- P0 安全：明文/密钥未 zeroize

**本 Sprint 内修复**
- TEE Quote 生成（至少 SGX 平台）
- Vault backend block_on 死锁修复
- WebSocket 权限检查
- 审计日志不可丢失（try_lock → lock().await）
- `init_logging` 真正初始化

**下个 Sprint**
- WebSocket sandbox 真实集成
- 水印验证实现
- 并发测试补充
- 生产存储后端切换

---

*生成时间: 2026-03-19 | 审查者: 6x 并发对抗性代码审查 Agent*
