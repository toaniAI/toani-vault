# 边缘案例审查报告

**日期**: 2026-03-19
**审查方法**: 穷举路径枚举（Edge Case Hunter）
**审查范围**: 当前所有未提交代码变更（30 个文件，+2382 / -465 行）
**参考规范**: `_bmad-output/implementation-artifacts/tech-specs/`

---

## 说明

本报告仅列出**未处理的边界条件和分支路径**，不评价代码整体质量。每个条目包含：

- **位置**: 文件及行号
- **触发条件**: 导致问题的具体输入或状态
- **修复草图**: 最小化的代码修复建议
- **潜在后果**: 若不处理可能发生的问题

---

## 高危问题（可能导致崩溃或未定义行为）

### 1. `env::set_var` 在多线程环境中的竞争

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/upgrade.rs:430` |
| **触发条件** | `start_new_enclave` 调用 `set_var("TEE_NEW_ENCLAVE_PID", ...)` 时，其他线程同时读取环境变量 |
| **修复草图** | 用 `AtomicU32` 字段替代 `env::set_var` 存储 PID |
| **潜在后果** | `std::env::set_var` 在多线程下为 UB，可能导致内存损坏 |

### 2. `AesGcmNonce::from_slice` 长度不匹配时 panic

| 字段 | 内容 |
|------|------|
| **位置** | `src/mcp/token_storage.rs:584` |
| **触发条件** | `encrypted.nonce()` 返回非 12 字节切片（外部构造 `EncryptedToken`） |
| **修复草图** | `if encrypted.nonce().len() != 12 { return Err(...) }` |
| **潜在后果** | `from_slice` 断言 panic，进程崩溃 |

### 3. WebSocket 截图构造 `PageStateFreezer` 可能 panic

| 字段 | 内容 |
|------|------|
| **位置** | `src/api/websocket.rs:661` |
| **触发条件** | `session_id` 不是合法 UUID 时，`PageStateFreezer::new` 内部断言失败 |
| **修复草图** | 在构造前验证 `session_id` 格式，失败返回错误响应 |
| **潜在后果** | WebSocket handler panic，连接静默断开，无错误响应返回客户端 |

---

## 安全问题

### 4. `record_to_storage` 审计静默丢失

| 字段 | 内容 |
|------|------|
| **位置** | `src/api/credentials.rs:163` |
| **触发条件** | `try_lock` 在高并发下失败，审计记录被跳过 |
| **修复草图** | `if try_lock fails { log::warn!("Audit record dropped for credential {}", credential_id); }` |
| **潜在后果** | 凭证操作无审计记录，且无任何可观测信号 |

### 5. 审计验证使用空公钥

| 字段 | 内容 |
|------|------|
| **位置** | `src/api/audit.rs:674` |
| **触发条件** | `state.verifier_public_key` 为空 `Vec`（未初始化） |
| **修复草图** | `if state.verifier_public_key.is_empty() { return Err(ApiError::new("config_error", "Verifier key not configured")); }` |
| **潜在后果** | `ring::UnparsedPublicKey` 返回不透明错误，所有签名验证静默失败 |

### 6. 公钥格式错误导致验证结果不可区分

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/attestation.rs:920` |
| **触发条件** | `to_uncompressed()` 返回非 65 字节数据 |
| **修复草图** | `assert_eq!(pub_key_bytes.len(), 65, "Expected uncompressed P-256 key")` 或返回 `SignatureVerificationFailed` |
| **潜在后果** | 密钥损坏与签名不合法产生相同错误，无法区分 |

---

## 加密问题

### 7. AES-GCM 输出截断导致空密文

| 字段 | 内容 |
|------|------|
| **位置** | `src/mcp/token_storage.rs:570` |
| **触发条件** | `combined.len() < 16`（理论上不应发生，但防御性检查缺失） |
| **修复草图** | `if combined.len() < 16 { return Err(TokenStorageError::EncryptionError("AES-GCM output too short".to_string())); }` |
| **潜在后果** | `saturating_sub(16)` 返回 0，密文为空，auth_tag 提取错误 |

---

## 升级流程问题

### 8. `migrate_traffic` 重复调用跳过 SealingKey 迁移阶段

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/upgrade.rs:629` |
| **触发条件** | 渐进式迁移多次调用 `migrate_traffic`，当前阶段已为 `MigratingSealingKey` |
| **修复草图** | 允许 `MigratingSealingKey` 阶段重入，或在 `MigratingSealingKey` 时仅更新权重不重置阶段 |
| **潜在后果** | Sealing Key 迁移步骤被绕过，加密密钥未完成迁移 |

### 9. 健康检查超时为 0 导致即时失败

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/upgrade.rs:536` |
| **触发条件** | `health_check_timeout_secs = 0` |
| **修复草图** | `let timeout_secs = self.config.health_check_timeout_secs.max(1);` |
| **潜在后果** | `Duration::from_secs(0)` 导致每次健康检查立即超时，连续失败触发自动回滚 |

### 10. HTTP 响应跨 TCP 分段导致状态行解析失败

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/upgrade.rs:556` |
| **触发条件** | HTTP 响应行在多个 TCP 数据包中到达，单次 `read` 仅捕获部分首行 |
| **修复草图** | 循环读取至 `\r\n\r\n` 或状态行结束后再解析 |
| **潜在后果** | 截断响应无法解析状态码，健康检查返回 false，误触发回滚 |

### 11. HTTPS URL 使用明文 TCP 连接

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/upgrade.rs:900`（`parse_health_url`） |
| **触发条件** | `new_enclave_health_url` 配置为 `https://...` |
| **修复草图** | `if url.starts_with("https://") { log::error!("HTTPS not supported"); return None; }` |
| **潜在后果** | TLS 握手数据通过明文 TCP 发送，健康检查始终失败或传输损坏数据 |

### 12. `complete_upgrade` 在 `pending_version` 为 None 时静默完成

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/upgrade.rs:699` |
| **触发条件** | `complete_upgrade` 被调用时 `pending_version` 已被意外清空 |
| **修复草图** | `if pending.is_none() { return Err(UpgradeError::InvalidState("No pending version to complete".to_string())); }` |
| **潜在后果** | `active_version` 不更新，升级状态标记为完成但实际旧版本仍在运行 |

---

## Redis 缓存问题

### 13. `set_ex` TTL 为 0 导致写入静默丢弃

| 字段 | 内容 |
|------|------|
| **位置** | `src/tenant/config.rs:830` |
| **触发条件** | `RedisTenantConfigCache::new(url, 0)` |
| **修复草图** | `let ttl = if self.ttl_seconds == 0 { 1 } else { self.ttl_seconds };` |
| **潜在后果** | Redis `SET EX 0` 返回错误，`_: Result<(), _>` 忽略该错误，缓存永不写入 |

### 14. `set()` 序列化失败静默，下次 `get()` 返回旧值

| 字段 | 内容 |
|------|------|
| **位置** | `src/tenant/config.rs:851` |
| **触发条件** | `serde_json::to_string(config)` 失败 |
| **修复草图** | `Err(e) => { log::error!("Failed to serialize tenant config: {}", e); }` |
| **潜在后果** | 配置更新无提示失败，旧缓存无限期提供服务 |

### 15. `clear()` 中 Redis 连接断开导致部分删除

| 字段 | 内容 |
|------|------|
| **位置** | `src/tenant/config.rs:860` |
| **触发条件** | SCAN 循环中 `query_async` 失败，`unwrap_or((0, vec![]))` 使 cursor 强制归零，提前退出循环 |
| **修复草图** | 区分错误（cursor 保持非零）和扫描完成（cursor 返回 0）：失败时 `break` 并记录错误 |
| **潜在后果** | 部分租户配置键未删除，形成孤儿缓存条目 |

---

## DCAP 证书验证问题

### 16. `cert_data` 单字节非零非 `0x30` 时越界判断逻辑问题

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/dcap.rs:943` |
| **触发条件** | `cert_data = [0x01]`（单字节，非 PEM 非 DER） |
| **修复草图** | `let is_der = !cert_data.is_empty() && cert_data[0] == 0x30;`（增加长度守卫） |
| **潜在后果** | 与 all-zeros 检查路径重叠，逻辑流向 `!is_pem && !is_der` 错误返回格式错误 |

---

## MCP Server 问题

### 17. 空 `session_id` 注册后遮蔽所有后续消息

| 字段 | 内容 |
|------|------|
| **位置** | `mcp-server/src/sse.rs:299` |
| **触发条件** | `params.session_id` 为空字符串，Token 验证通过 |
| **修复草图** | `if params.session_id.is_empty() { return Err(SseError::AuthFailed("Empty session_id".to_string())); }` |
| **潜在后果** | 空 session_id 被注册，后续所有空 session_id 消息路由到同一条目，消息混淆 |

### 18. `key_hierarchy` 读锁跨两个 `await` 派生操作持有时间过长

| 字段 | 内容 |
|------|------|
| **位置** | `mcp-server/src/tools.rs:196` |
| **触发条件** | L2、L3 密钥派生均在同一 `read()` 锁范围内完成 |
| **修复草图** | 在锁内只做 L2 派生，将 L2 key clone 出来后释放锁，再用 L2 派生 L3 |
| **潜在后果** | 读锁跨越两次异步操作，阻塞所有写操作（如密钥轮换） |

---

## LLM 服务问题

### 19. `choices` 为空时响应内容静默为空字符串

| 字段 | 内容 |
|------|------|
| **位置** | `src/services/llm/azure.rs:265` 及 `src/services/llm/openai.rs:265` |
| **触发条件** | API 返回 0 个 choices（速率限制、内容过滤等） |
| **修复草图** | `if choices.is_empty() { return Err(LlmError::EmptyResponse("No choices returned".to_string())); }` |
| **潜在后果** | 调用方收到 `Ok("")`，无法区分空响应与合法空内容 |

---

## 数据导出问题

### 20. CSV 导出混合 Schema 对象数组丢失列值

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/sandbox/export/data_export.rs:498` |
| **触发条件** | 数组中各对象的键集合不一致（第一个对象缺少某些键，后续对象有额外键） |
| **修复草图** | 以所有对象键的并集构建表头，每行按表头顺序填充，缺失键填空字符串 |
| **潜在后果** | 额外键的值静默丢失；若按索引访问则 panic |

### 21. 截图页面信息 `about:blank` 过滤导致真实 URL 丢失

| 字段 | 内容 |
|------|------|
| **位置** | `src/tee/sandbox/export/screenshot.rs:706` |
| **触发条件** | CDP 返回的真实沙箱页面 URL 恰好是 `about:blank`（罕见但可能） |
| **修复草图** | 接受任何非 `None` 的 URL，包括 `about:blank`；仅在 CDP 不可用时才降级为 `"unknown"` |
| **潜在后果** | 截图元数据记录 `"unknown"` URL，审计溯源失败 |

---

## Vault 后端问题

### 22. `updated_at = 0` 时间戳语义歧义

| 字段 | 内容 |
|------|------|
| **位置** | `src/vault/backend.rs:118` |
| **触发条件** | `entry.updated_at == 0`（Unix 纪元，即 1970-01-01） |
| **修复草图** | 使用 `Option<u64>` 而非魔法值 0；`None` 明确表示"从未更新" |
| **潜在后果** | `unwrap_or(created_at)` 将纪元时间戳误替换为创建时间，审计时间轴记录错误 |

---

## 汇总统计

| 严重级别 | 数量 |
|--------|------|
| 高危（崩溃/UB） | 3 |
| 安全 | 3 |
| 加密 | 1 |
| 升级流程 | 5 |
| Redis 缓存 | 3 |
| 其他 | 7 |
| **合计** | **22** |

---

*本报告由 Edge Case Hunter 自动生成，仅列出未处理路径，不包含风格建议或架构评价。*
