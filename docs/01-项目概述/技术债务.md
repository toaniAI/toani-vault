# CredBridge 技术债务清单

> 生成时间: 2026-03-19
> 扫描范围: 整个代码库 (FIXME, TODO, HACK, 简化处理, 临时方案)

---

## 一、FIXME (需要立即修复)

| 位置 | 问题描述 | 优先级 |
|------|---------|--------|
| `src/audit/immudb_store.rs:6-7` | 需要将 `std::sync::Mutex` 替换为 `tokio::sync::Mutex` 以支持跨 await 持有，当前为让 CI 通过暂时允许此警告 | **高** |

---

## 二、TODO (待实现功能)

### 2.1 TEE 相关 (高优先级)

| 编号 | 位置 | 描述 | 优先级 |
|------|------|------|--------|
| TEE-106 | `src/tee/sandbox/export/watermark.rs:146` | 实现水印验证逻辑 | **高** |
| TEE-107 | `src/api/websocket.rs:627` | 实际调用沙箱会话执行操作 | **高** |
| TEE-108 | `src/api/websocket.rs:652` | 实际调用沙箱截图功能 | **高** |
| TEE-201 | `src/tee/upgrade.rs:392` | 实际启动新版本 Enclave | **中** |
| TEE-202 | `src/tee/upgrade.rs:419` | 实际执行健康检查 | **中** |
| TEE-203 | `src/tee/upgrade.rs:456` | 实际迁移流量 | **中** |
| TEE-204 | `src/tee/upgrade.rs:513` | 关闭旧版本 Enclave | **中** |
| TEE-205 | `src/tee/upgrade.rs:542` | 实现回滚恢复逻辑 | **中** |
| TEE-301 | `src/tee/driver_verify.rs:358` | 从安全存储加载公钥 | **中** |

### 2.2 Attestation 相关

| 位置 | 描述 | 优先级 |
|------|------|--------|
| `vault-service/src/api/attestation.rs:331` | 调用 SGX DCAP 库生成 Quote | **高** |
| `vault-service/src/api/attestation.rs:335` | 调用 TDX attestation 库生成 Quote | **高** |
| `vault-service/src/api/attestation.rs:339` | 调用 SEV-SNP 库生成 attestation report | **高** |

### 2.3 基础设施相关

| 编号 | 位置 | 描述 | 优先级 |
|------|------|------|--------|
| INFRA-101 | `src/tenant/config.rs:830-844` | 实现 Redis 租户配置缓存 (4处) | **中** |

### 2.4 MCP 服务相关

| 位置 | 描述 | 优先级 |
|------|------|--------|
| `mcp-server/src/tools.rs:166` | 实际的 TEE 解密操作 | **高** |
| `mcp-server/src/sse.rs:277` | 实际验证 Token (当前简化处理) | **中** |
| `mcp-server/src/sse.rs:364` | 处理 MCP 请求并发送到 ToolHandler | **中** |

---

## 三、简化处理 (需要完善实现)

### 3.1 安全相关 (高风险)

| 位置 | 当前实现 | 应完善为 | 优先级 |
|------|---------|---------|--------|
| `src/mcp/token_storage.rs:556-557` | Token 存储未加密，使用固定 auth_tag | 使用 AES-GCM 加密，生成真实认证标签 | **关键** |
| `src/api/audit.rs:661` | 签名验证简化处理 | 使用公钥验证签名 | **高** |
| `mcp-server/src/sse.rs:277-287` | SSE Token 验证简化，使用模拟 claims | 完整 TokenValidator.validate() | **高** |

### 3.2 加密相关

| 位置 | 当前实现 | 应完善为 | 优先级 |
|------|---------|---------|--------|
| `src/crypto/key_derivation.rs:405-419` | PBKDF2 简化实现，仅用于演示 | 完整的密钥派生流程 | **中** |
| `src/tee/dcap.rs:927` | 证书验证简化处理 | 使用 webpki 库验证 | **中** |
| `src/tee/attestation.rs:897` | 仅检查签名格式 | 完整签名验证 | **中** |

### 3.3 业务逻辑相关

| 位置 | 当前实现 | 应完善为 | 优先级 |
|------|---------|---------|--------|
| `src/vault/backend.rs:148` | `updated_at` 使用 `created_at` 值 | 维护真实的更新时间 | **低** |
| `src/vault/backend.rs:405` | 租户索引简化实现 | 维护租户索引 | **中** |
| `src/api/credentials.rs:136-137` | `mrenclave` 和 `jti` 使用固定值 | 从上下文获取真实值 | **中** |
| `src/api/audit.rs:654` | `content_hash_match` 固定为 true | 实际计算并验证哈希 | **中** |
| `src/services/llm/openai.rs:159` | LLM 请求使用简化 content | 支持 content array | **低** |
| `src/services/llm/azure.rs:162` | LLM 请求使用简化 content | 支持 content array | **低** |
| `frontend/src/shared/api/services.ts:170` | 安全评分简化算法 | 完整评分算法 | **低** |

### 3.4 数据导出相关

| 位置 | 当前实现 | 应完善为 | 优先级 |
|------|---------|---------|--------|
| `src/tee/sandbox/export/data_export.rs:483` | 仅支持对象数组 | 支持更多数据结构 | **低** |
| `src/tee/sandbox/export/data_export.rs:542` | 简化的 JSON 转 XML | 完整的 XML 转换 | **低** |
| `src/tee/sandbox/export/watermark.rs:295-318` | 字符渲染为矩形点阵 | 完整字符渲染 | **低** |

### 3.5 其他简化处理

| 位置 | 描述 | 优先级 |
|------|------|--------|
| `src/api/context.rs:43` | 从全局状态获取简化处理 | **低** |
| `src/tee/mod.rs:138` | 返回模拟模式 | **低** |
| `src/tee/enclave.rs:637` | 简化处理 | **低** |
| `src/tee/sandbox/security/mod.rs:87` | 安全检查简化 | **中** |
| `src/tee/sandbox/config.rs:482` | 简化的 seccomp 策略 | **中** |

---

## 四、临时方案 (需要正式实现)

| 位置 | 临时方案描述 | 正式实现方向 | 优先级 |
|------|-------------|-------------|--------|
| `src/connector/http.rs:340` | HTTP 连接器临时方案 | - | **中** |
| `src/vault/client.rs:334` | 暂时返回未实现错误 | 实现完整功能 | **中** |
| `src/tee/sandbox/export/screenshot.rs:648` | 使用模拟数据 | 真实截图实现 | **高** |

---

## 五、按优先级汇总

### P0 - 关键 (必须尽快处理)

1. **Token 存储未加密** (`mcp/token_storage.rs:556-557`) - 安全风险
2. **FIXME: Mutex 跨 await 问题** (`audit/immudb_store.rs:6-7`) - 可能导致死锁
3. **水印验证未实现** (TEE-106) - 影响安全导出完整性

### P1 - 高优先级 (本迭代处理)

1. TEE Attestation Quote 生成 (SGX/TDX/SEV-SNP)
2. SSE Token 完整验证
3. 沙箱会话操作实际调用 (TEE-107, TEE-108)
4. 审计日志签名验证
5. MCP TEE 解密操作

### P2 - 中优先级 (下迭代处理)

1. Redis 租户配置缓存 (INFRA-101)
2. TEE 升级流程 (TEE-201 ~ TEE-205)
3. DCAP 证书验证完善
4. 租户索引维护

### P3 - 低优先级 (技术债务)

1. LLM 请求 content array 支持
2. 数据导出格式完善
3. 安全评分算法完善
4. 更新时间维护

---

## 六、统计

| 类别 | 数量 |
|------|------|
| FIXME | 1 |
| TODO (已编号) | 10 |
| TODO (未编号) | 6 |
| 简化处理 - 安全相关 | 3 |
| 简化处理 - 业务相关 | 10+ |
| 临时方案 | 3 |

---

## 七、建议

1. **立即处理** Token 存储加密问题，这是安全漏洞
2. **建立编号系统** 为所有 TODO 分配编号，便于跟踪
3. **代码审查** 在合并 PR 时检查是否引入新的 TODO/FIXME
4. **定期清理** 每个迭代分配时间处理技术债务