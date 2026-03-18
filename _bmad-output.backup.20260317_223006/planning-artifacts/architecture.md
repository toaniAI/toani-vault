---
stepsCompleted:
  - step-01-init
  - step-02-context
  - step-03-starter
  - step-04-decisions
  - step-05-patterns
  - security-architecture-supplement
inputDocuments:
  - /Users/yvan/AIWorkspace/credbridge/_bmad-output/planning-artifacts/product-brief-credbridge-2026-03-10.md
  - /Users/yvan/AIWorkspace/credbridge/_bmad-output/planning-artifacts/prd.md
  - /Users/yvan/AIWorkspace/credbridge/docs/CredBridge_CN_设计规范_v1.0.md
workflowType: 'architecture'
project_name: 'credbridge'
user_name: 'CoPaw'
date: '2026-03-10'
classification:
  projectType: 'SaaS B2B + API Backend + Developer Tool'
  domain: 'Fintech / Security / Identity Management'
  complexity: 'high'
  projectContext: 'brownfield'
techConstraints:
  backendLanguage: 'Rust'
  frontendScript: 'TypeScript (Node.js 22+)'
  teeEnclave: 'Rust (SGX SDK)'
  architecture: 'TEE+Rust Zero Trust'
mvpCoreFeatures:
  - TEE Credential Vault (Rust + SGX)
  - Limited Scope Token System
  - Audit Logs (immudb)
  - Basic SDK (TypeScript + Rust)
---

# Architecture Decision Document - CredBridge

_This document builds collaboratively through step-by-step discovery. Sections are appended as we work through each architectural decision together._

**Author:** CoPaw
**Date:** 2026-03-10

---

## Project Context

### 产品定位
CredBridge 是首个专为 AI Agent 设计的零信任凭证保险库系统，采用商用级安全标准（对标 1Password），让 AI 能够安全地代表用户行动——无需暴露用户秘密。

### 核心技术栈
- **后端**: Rust (内存安全 + 高性能)
- **TEE Enclave**: Intel SGX (DCAP 远程认证)
- **前端/SDK**: TypeScript (Node.js 22+)
- **存储**: HashiCorp Vault + immudb + PostgreSQL + Redis
- **Token**: PASETO v4.local

### MVP 核心功能
1. **TEE 凭证保险库** - Rust + SGX 零知识存储
2. **有限 Scope Token 系统** - 最小权限原则
3. **审计日志** - immudb 不可篡改日志
4. **基础 SDK** - TypeScript + Rust 双语言支持

---

## 项目上下文分析

### 需求概览

**功能需求：**
CredBridge 是专为 AI Agent 设计的零信任凭证保险库系统。核心功能包括：
- TEE（可信执行环境）内凭证加密存储与访问
- 有限 Scope Token 生成与验证
- 不可篡改的审计日志系统
- TypeScript/Rust 双语言 SDK

**关键架构需求：**
- 多租户 SaaS 架构（B2B 场景）
- 高安全性（零知识架构）
- 低延迟凭证访问（<100ms P99）
- 合规性支持（未来 SOC2/FIPS）

### 规模与复杂度

- **主要领域**：安全基础设施 / Fintech / 身份管理
- **复杂度级别**：高（TEE 安全、多租户、B2B 合规）
- **预估架构组件**：8-10 个核心组件

### 技术约束与依赖

**已确定技术栈：**
- 后端：Rust（内存安全 + 高性能）
- TEE Enclave：Intel SGX (DCAP 远程认证)
- SDK：TypeScript (Node.js 22+)
- 存储：PostgreSQL + immudb + Redis
- Token：PASETO v4.local

### 跨领域关注点

- 安全性（TEE、加密、零知识）
- 多租户隔离
- 审计与合规
- 性能与可扩展性

---

## 技术栈评估

### 主要技术领域

基于项目需求分析：**API 后端 + TEE 安全基础设施 + TypeScript SDK**

### 选定的技术栈

CredBridge 采用自定义技术栈（非标准启动模板）：

**后端与 TEE：**
- Rust + Intel SGX SDK
- PostgreSQL（多租户数据）
- immudb（不可篡改审计日志）
- Redis（缓存与会话）

**SDK 与前端：**
- TypeScript (Node.js 22+)
- 双 SDK 架构（TypeScript + Rust）

**基础设施：**
- TEE 硬件环境（SGX-enabled servers）
- DCAP 远程认证

---

## 核心架构决策

### 决策优先级分析

**关键决策（阻塞实现）：**
1. 数据架构 - 多租户模型 ✅
2. 安全架构 - TEE 集成模式
3. API 设计 - Token 与访问控制

### 数据架构决策

#### 决策 DA-001：多租户数据隔离方案

**决策：** 采用 **Schema-per-Tenant + RLS 混合方案**

**方案对比：**

| 方案 | 隔离级别 | 成本 | 适用场景 |
|------|---------|------|---------|
| 独立数据库 | 最高 | 高 | 大客户/合规要求 |
| Schema-per-Tenant | 高 | 中 | **B2B SaaS 首选** |
| 共享表 + RLS | 中 | 低 | 小租户/低成本 |

**选择理由（Schema-per-Tenant + RLS）：**
- 凭证数据敏感性高，需要强隔离
- B2B 客户有合规需求（数据隔离）
- 单租户备份/恢复便利
- 与 TEE 安全理念一致

**实施方案：**
- 每个租户独立 Schema
- Schema 内启用 PostgreSQL RLS（行级安全）
- 共享连接池，Schema 动态切换
- TEE Enclave 内执行敏感操作

**影响范围：**
- 数据库迁移策略
- 租户生命周期管理
- 连接池配置

---

## 实现模式与一致性规则

### 模式类别定义

**已识别关键冲突点：** 5 个主要类别，确保多 AI Agent 实现一致性

---

### 命名模式

**数据库命名约定：**
| 类型 | 规则 | 示例 |
|------|------|------|
| 表名 | snake_case, 复数形式 | `tenant_configs`, `audit_logs` |
| 列名 | snake_case | `user_id`, `created_at` |
| 外键 | `{table}_id` 格式 | `tenant_id`, `credential_id` |
| 索引 | `idx_{table}_{column}` | `idx_credentials_tenant_id` |

**API 命名约定：**
| 类型 | 规则 | 示例 |
|------|------|------|
| 端点路径 | kebab-case, 复数名词 | `/api/v1/tenant-configs` |
| 路由参数 | camelCase | `:tenantId`, `:credentialId` |
| 查询参数 | camelCase | `?pageSize=20&sortBy=name` |
| 自定义 Header | X-CredBridge-* | `X-CredBridge-Tenant-ID` |

**代码命名约定：**
| 语言 | 类型 | 规则 | 示例 |
|------|------|------|------|
| Rust | 模块/函数 | snake_case | `credential_store`, `verify_token()` |
| Rust | 结构体/枚举 | PascalCase | `CredentialVault`, `TokenScope` |
| Rust | 常量 | SCREAMING_SNAKE | `MAX_TOKEN_LIFETIME` |
| TypeScript | 类/接口 | PascalCase | `CredVaultClient`, `TokenConfig` |
| TypeScript | 函数/变量 | camelCase | `getCredential()`, `tokenExpiry` |

---

### 结构模式

**Rust 项目组织：**
```
src/
├── main.rs           # 入口点
├── lib.rs            # 库导出
├── config/           # 配置模块
│   ├── mod.rs
│   └── app_config.rs
├── models/           # 数据模型
│   ├── mod.rs
│   ├── credential.rs
│   └── tenant.rs
├── services/         # 业务逻辑
│   ├── mod.rs
│   ├── vault_service.rs
│   └── token_service.rs
├── repositories/     # 数据访问
│   ├── mod.rs
│   └── credential_repo.rs
├── tee/              # TEE 相关
│   ├── mod.rs
│   ├── enclave.rs
│   └── attestation.rs
├── api/              # API 处理器
│   ├── mod.rs
│   ├── routes.rs
│   └── middleware.rs
└── utils/            # 工具函数
    ├── mod.rs
    └── crypto.rs
```

**TypeScript SDK 结构：**
```
src/
├── index.ts          # 主导出
├── client.ts         # 核心客户端
├── types/            # TypeScript 类型
│   ├── index.ts
│   ├── credential.ts
│   └── token.ts
├── services/         # 服务封装
│   ├── index.ts
│   ├── vault.ts
│   └── token.ts
└── utils/            # 工具函数
    ├── index.ts
    └── encoding.ts
```

---

### 格式模式

**API 响应格式：**
```typescript
// 成功响应
{
  "success": true,
  "data": { ... },
  "meta": {
    "requestId": "req_abc123",
    "timestamp": "2026-03-10T08:30:00Z"
  }
}

// 错误响应
{
  "success": false,
  "error": {
    "code": "CREDENTIAL_NOT_FOUND",
    "message": "The requested credential does not exist",
    "details": { ... }
  },
  "meta": {
    "requestId": "req_abc123",
    "timestamp": "2026-03-10T08:30:00Z"
  }
}
```

**数据交换格式：**
| 类型 | 规则 | 示例 |
|------|------|------|
| JSON 字段 | camelCase | `{"createdAt": "2026-03-10T08:30:00Z"}` |
| 日期时间 | ISO 8601 UTC | `2026-03-10T08:30:00Z` |
| 布尔值 | true/false | `{"isActive": true}` |
| 空值 | null | `{"deletedAt": null}` |

---

### 通信模式

**事件命名约定：**
| 事件类型 | 命名格式 | 示例 |
|----------|----------|------|
| 领域事件 | `{entity}.{action}` | `credential.created` |
| 系统事件 | `system.{action}` | `system.backup_completed` |
| 审计事件 | `audit.{action}` | `audit.access_denied` |

**事件载荷结构：**
```typescript
{
  "eventId": "evt_abc123",
  "eventType": "credential.created",
  "timestamp": "2026-03-10T08:30:00Z",
  "tenantId": "tenant_xyz789",
  "payload": {
    "credentialId": "cred_def456",
    "credentialType": "oauth_token",
    "createdBy": "user_ghi789"
  }
}
```

**状态更新模式：**
- 使用不可变更新（Immutable Updates）
- Redux/状态管理遵循 `{type, payload}` 结构

---

### 过程模式

**错误处理模式（Rust）：**
```rust
// 使用 Result<T, E> 进行错误传播
pub fn verify_token(token: &str) -> Result<TokenClaims, VaultError> {
    let claims = decode_token(token)
        .map_err(|e| VaultError::InvalidToken(e.to_string()))?;

    if claims.exp < current_timestamp() {
        return Err(VaultError::TokenExpired);
    }

    Ok(claims)
}
```

**重试模式（指数退避）：**
```typescript
const retryWithBackoff = async <T>(
  fn: () => Promise<T>,
  maxRetries: number = 3,
  baseDelay: number = 1000
): Promise<T> => {
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await fn();
    } catch (error) {
      if (i === maxRetries - 1) throw error;
      const delay = baseDelay * Math.pow(2, i);
      await sleep(delay);
    }
  }
  throw new Error("Max retries exceeded");
};
```

---

### 强制指南

**所有 AI Agent 必须遵守：**

1. **命名一致性**：严格遵循各语言/上下文的命名约定
2. **错误传播**：Rust 中使用 `Result` 类型，TypeScript 中使用 `try/catch` + 包装错误
3. **日期处理**：所有 API 交互使用 ISO 8601 UTC 格式
4. **不可变更新**：状态变更必须创建新对象，禁止直接修改
5. **日志记录**：使用结构化日志，包含 `requestId` 和 `tenantId`

**模式验证：**

- 代码审查时检查命名规范
- CI 中使用 `clippy` (Rust) 和 `eslint` (TypeScript) 强制规范
- 文档中记录模式违规示例

---

### 模式示例

**正确示例：**
```rust
// Rust: 正确的错误处理
pub async fn get_credential(
    &self,
    tenant_id: &str,
    credential_id: &str
) -> Result<Credential, VaultError> {
    self.repo
        .find_by_id(tenant_id, credential_id)
        .await
        .map_err(VaultError::DatabaseError)
}
```

**反模式（避免）：**
```rust
// Rust: 错误的 panic 处理
pub fn get_credential(id: &str) -> Credential {
    self.repo.find(id).unwrap() // ❌ 不要 unwrap!
}
```

---

## 核心安全架构设计

### 决策 SA-001：四层密钥层次设计

**决策：** 采用 **L0-L3 四层密钥层次架构**

#### 密钥层次架构图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           四层密钥层次架构                               │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────┐                                                        │
│  │   L0: 硬件根密钥   │  Intel SGX Sealing Key (平台专属，硬件绑定)           │
│  │  (Hardware Root) │  • MRSIGNER / MRENCLAVE 派生                       │
│  └──────┬──────┘                                                        │
│         │ HKDF-Extract                                                   │
│         ▼                                                               │
│  ┌─────────────┐                                                        │
│  │  L1: Enclave    │  Enclave Master Key                                  │
│  │   Master Key    │  • 用于派生所有用户级密钥                              │
│  │    (TEE内)      │  • 永不离开 Enclave 边界                              │
│  └──────┬──────┘                                                        │
│         │ HKDF-Expand( user_id + tenant_id )                             │
│         ▼                                                               │
│  ┌─────────────┐                                                        │
│  │   L2: User      │  User Vault Key (每用户独立)                         │
│  │   Vault Key     │  • 派生凭证加密密钥的父密钥                            │
│  │   (每用户)       │  • 内存中临时存在，定期轮换                            │
│  └──────┬──────┘                                                        │
│         │ HKDF-Expand( credential_id + purpose )                         │
│         ▼                                                               │
│  ┌─────────────┐                                                        │
│  │   L3: Credential │  Credential Encryption Key (每条凭证独立)           │
│  │   Encryption Key │  • 实际加密凭证内容的密钥                             │
│  │   (每条凭证)      │  • 一次一密，用完即焚                               │
│  └─────────────┘                                                        │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

#### 密钥派生规范

**L0 → L1 派生：**
```rust
// 使用 SGX 密封密钥作为 PRK (Pseudo-Random Key)
let sealing_key = sgx_get_key(&key_request)?;

// HKDF-Extract: 从密封密钥派生 Enclave Master Key
let prk = hkdf_extract(
    salt: &sealing_key,
    ikm: b"CredBridge Enclave v1.0"
)?;

let enclave_master_key = hkdf_expand(
    prk: &prk,
    info: b"enclave-master-key",
    length: 32
)?;
```

**L1 → L2 派生：**
```rust
// 每用户独立的 Vault Key
let user_vault_key = hkdf_expand(
    prk: &enclave_master_key,
    info: &format!("user-vault-key:{}:{}", tenant_id, user_id),
    length: 32
)?;
```

**L2 → L3 派生：**
```rust
// 每条凭证独立的加密密钥
let cred_encryption_key = hkdf_expand(
    prk: &user_vault_key,
    info: &format!("credential-key:{}:{}", credential_id, purpose),
    length: 32
)?;
```

#### 安全属性

| 属性 | 实现方式 | 安全级别 |
|------|----------|----------|
| 前向保密 | L3 密钥一次一密，不持久化存储 | 高 |
| 密钥隔离 | 每层密钥仅用于特定目的 | 高 |
| 硬件绑定 | L0 密钥与 SGX 平台绑定 | 极高 |
| 抗重放 | HKDF info 包含唯一标识符 | 高 |
| 内存安全 | 密钥使用后立即 zeroize | 高 |

---

### 决策 SA-002：TEE 架构详细设计

**决策：** 采用 **In-Process Enclave 模式**

#### TEE 架构总体设计

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           CredBridge TEE 架构                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                     Host Application (Untrusted)                 │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │   │
│  │  │   API 层    │  │  业务逻辑   │  │     数据库访问层         │  │   │
│  │  │  (Axum)     │  │  (Service)  │  │   (PostgreSQL/immudb)   │  │   │
│  │  └──────┬──────┘  └──────┬──────┘  └─────────────────────────┘  │   │
│  │         │                │                                      │   │
│  │         └────────────────┘                                      │   │
│  │                   │                                             │   │
│  │              ECALL Interface (边界层)                            │   │
│  └───────────────────┼─────────────────────────────────────────────┘   │
│                      │                                                  │
│  ════════════════════╪══════════════════════════════════════════════   │
│                      │                    SGX Enclave 边界             │
│  ════════════════════╪══════════════════════════════════════════════   │
│                      ▼                                                  │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                     Enclave (Trusted)                            │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │   │
│  │  │  密钥管理   │  │  加密/解密  │  │     签名服务             │  │   │
│  │  │  (L0-L3)    │  │ (AES-256-GCM)│  │   (Ed25519)             │  │   │
│  │  └─────────────┘  └─────────────┘  └─────────────────────────┘  │   │
│  │                                                                 │   │
│  │  ┌─────────────────────────────────────────────────────────┐   │   │
│  │  │              安全内存区域 (Protected)                      │   │   │
│  │  │  • Enclave Master Key (L1) - 持续存在                     │   │   │
│  │  │  • User Vault Keys (L2) - 临时缓存 (TTL: 5min)            │   │   │
│  │  │  • Credential Keys (L3) - 即时派生，用完即焚               │   │   │
│  │  └─────────────────────────────────────────────────────────┘   │   │
│  │                                                                 │   │
│  │  ┌─────────────────────────────────────────────────────────┐   │   │
│  │  │              OCALL Interface (受限出口)                    │   │   │
│  │  │  • 仅允许加密后的数据传出                                   │   │   │
│  │  │  • 禁止密钥材料传出                                       │   │   │
│  │  │  • 审计日志通过安全通道写入 immudb                          │   │   │
│  │  └─────────────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

#### Enclave 边界定义

**安全边界原则：**
1. **所有敏感操作在 Enclave 内执行**
   - 凭证解密
   - 签名生成
   - 密钥派生
   - Token 签名验证

2. **敏感数据不出 Enclave**
   - 原始凭证内容
   - 私钥材料 (L0-L2)
   - 解密后的用户数据

3. **OCALL 受限出口**
   - 仅允许加密后的密文传出
   - 仅允许审计日志摘要传出
   - 禁止任何形式的密钥传出

#### ECALL/OCALL 接口设计

**ECALL (Host → Enclave)：**

```rust
// Enclave 接口定义 (enclave.edl)
enclave {
    // 密钥管理
    public sgx_status_t ecall_derive_user_vault_key(
        [in, string] const char* tenant_id,
        [in, string] const char* user_id,
        [out] uint8_t key_handle[32]
    );

    // 凭证操作
    public sgx_status_t ecall_encrypt_credential(
        [in, string] const char* tenant_id,
        [in, string] const char* user_id,
        [in, string] const char* credential_id,
        [in, size=plaintext_len] const uint8_t* plaintext,
        size_t plaintext_len,
        [out, size=ciphertext_len] uint8_t* ciphertext,
        size_t ciphertext_len
    );

    public sgx_status_t ecall_decrypt_credential(
        [in, string] const char* tenant_id,
        [in, string] const char* user_id,
        [in, string] const char* credential_id,
        [in, size=ciphertext_len] const uint8_t* ciphertext,
        size_t ciphertext_len,
        [out, size=plaintext_len] uint8_t* plaintext,
        size_t plaintext_len
    );

    // 签名服务
    public sgx_status_t ecall_sign_data(
        [in, string] const char* tenant_id,
        [in, string] const char* key_id,
        [in, size=data_len] const uint8_t* data,
        size_t data_len,
        [out] uint8_t signature[64]
    );

    // Token 验证
    public sgx_status_t ecall_verify_token(
        [in, string] const char* token,
        [out] uint8_t claims_hash[32],
        [out] int64_t* expiry
    );

    // 远程认证
    public sgx_status_t ecall_generate_quote(
        [in] const uint8_t report_data[64],
        [out, size=quote_size] uint8_t* quote,
        size_t quote_size
    );
};
```

**OCALL (Enclave → Host)：**

```rust
// 受限的 OCALL 接口
untrusted {
    // 仅用于获取时间（通过安全通道验证）
    void ocall_get_timestamp([out] int64_t* timestamp);

    // 写入审计日志（仅传出哈希/摘要）
    void ocall_write_audit_log(
        [in, string] const char* event_type,
        [in, size=hash_len] const uint8_t* data_hash,
        size_t hash_len
    );

    // 获取 sealed 数据存储（已加密的密钥）
    void ocall_read_sealed_data(
        [in, string] const char* key_id,
        [out, size=max_len] uint8_t* sealed_data,
        size_t max_len,
        [out] size_t* actual_len
    );

    void ocall_write_sealed_data(
        [in, string] const char* key_id,
        [in, size=len] const uint8_t* sealed_data,
        size_t len
    );
};
```

#### 内存安全策略

```rust
// 密钥使用后立即清除
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(ZeroizeOnDrop)]
struct CredentialKey {
    #[zeroize(skip)]  // key_handle 不需要 zeroize
    key_handle: KeyHandle,
    key_material: [u8; 32],  // 自动 zeroize
}

impl CredentialKey {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let result = aes_gcm_encrypt(&self.key_material, plaintext)?;
        // key_material 在 drop 时自动 zeroize
        Ok(result)
    }
}
```

---

### 决策 SA-003：Token 系统详细设计

**决策：** 采用 **PASETO v4.local + Redis 黑名单** 架构

#### Token 系统架构

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CredBridge Token 系统                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                        Token 颁发流程                            │   │
│  │                                                                 │   │
│  │   Client        API Gateway        Enclave        Redis         │   │
│  │     │                │                │            │            │   │
│  │     │ ── 1.请求Token ─>│                │            │            │   │
│  │     │                │ ── 2.验证权限 ─>│            │            │   │
│  │     │                │                │            │            │   │
│  │     │                │ <─ 3.签名Token ─│            │            │   │
│  │     │                │                │            │            │   │
│  │     │                │ ── 4.存储jti ──────────────>│            │   │
│  │     │                │                │            │            │   │
│  │     │ <─ 5.返回Token ─│                │            │            │   │
│  │     │                │                │            │            │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                        Token 验证流程                            │   │
│  │                                                                 │   │
│  │   Client        API Gateway        Enclave        Redis         │   │
│  │     │                │                │            │            │   │
│  │     │ ── 1.API请求 ─>│                │            │            │   │
│  │     │   (with Token) │                │            │            │   │
│  │     │                │ ── 2.检查黑名单 ────────────>│            │   │
│  │     │                │                │            │            │   │
│  │     │                │ ── 3.验证签名 ─>│            │            │   │
│  │     │                │                │            │            │   │
│  │     │                │ <─ 4.返回Claims ─│            │            │   │
│  │     │                │                │            │            │   │
│  │     │ <─ 5.执行请求 ──│                │            │            │   │
│  │     │                │                │            │            │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

#### PASETO v4.local 实现规范

**Token 结构：**
```
v4.local.{base64url-encoded payload}.{base64url-encoded footer (optional)}
```

**Token Claims 结构：**
```typescript
interface TokenClaims {
  // PASETO 标准声明
  iss: string;        // 颁发者: "credbridge-vault"
  sub: string;        // 主题: user_id
  aud: string;        // 受众: tenant_id
  exp: number;        // 过期时间: Unix timestamp (15分钟后)
  nbf: number;        // 生效时间: Unix timestamp
  iat: number;        // 颁发时间: Unix timestamp
  jti: string;        // JWT ID: 唯一标识符 (UUID v4)

  // CredBridge 自定义声明
  scope: string;      // 权限范围: "credential:read" | "credential:write" | "admin"
  tenant_id: string;  // 租户ID
  credential_ids?: string[];  // 允许的凭证ID列表 (可选，用于受限Token)
  ip_bound?: string;  // 绑定IP (可选)
  mfa_verified: boolean;  // MFA验证状态
}
```

**Token 生成 (Enclave 内执行)：**
```rust
use paseto::v4::local::{encrypt, decrypt};
use paseto::claims::{Claims, ClaimsValidationRules};

pub fn generate_token(
    &self,
    user_id: &str,
    tenant_id: &str,
    scope: TokenScope,
    credential_ids: Option<Vec<String>>,
) -> Result<String, VaultError> {
    // 1. 构建 Claims
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let exp = now + 900; // 15 分钟有效期
    let jti = generate_secure_uuid()?;

    let claims = Claims::new()
        .set_issuer("credbridge-vault")
        .set_subject(user_id)
        .set_audience(tenant_id)
        .set_expiration(exp)
        .set_not_before(now)
        .set_issued_at(now)
        .set_jti(&jti)
        .add_additional_claim("scope", scope.to_string())
        .add_additional_claim("tenant_id", tenant_id)
        .add_additional_claim("mfa_verified", true);

    // 2. 加密 Token (使用 L2 派生的 Token 密钥)
    let token_key = self.derive_token_key(tenant_id, user_id)?;
    let token = encrypt(&claims, &token_key)?;

    // 3. 存储 jti 到 Redis (用于撤销)
    self.store_jti(tenant_id, &jti, exp)?;

    Ok(token)
}
```

**Token 验证：**
```rust
pub fn verify_token(
    &self,
    token: &str,
    expected_tenant_id: &str,
) -> Result<TokenClaims, VaultError> {
    // 1. 解析 Token (获取 jti 用于黑名单检查)
    let unvalidated = parse_unvalidated(token)?;
    let jti = unvalidated.jti.ok_or(VaultError::InvalidToken)?;

    // 2. 检查黑名单
    if self.is_jti_revoked(&jti)? {
        return Err(VaultError::TokenRevoked);
    }

    // 3. 解密并验证 (Enclave 内)
    let token_key = self.derive_token_key(&unvalidated.tenant_id, &unvalidated.sub)?;
    let claims = decrypt(token, &token_key)?;

    // 4. 验证声明
    if claims.aud != expected_tenant_id {
        return Err(VaultError::InvalidAudience);
    }

    // 5. 验证 scope
    self.validate_scope(&claims.scope, &requested_operation)?;

    Ok(claims)
}
```

#### Redis 黑名单数据结构

**存储策略：**
```
# Token 活跃集合 (Sorted Set) - 用于快速验证
Key:    credbridge:tokens:{tenant_id}:active
Value:  jti (member), exp_timestamp (score)
TTL:    自动过期

# Token 撤销集合 (Set) - 用于撤销检查
Key:    credbridge:tokens:{tenant_id}:revoked
Value:  jti
TTL:    与最长 Token 有效期相同

# Token 元数据 (Hash) - 用于审计
Key:    credbridge:token:{jti}
Fields:
  - user_id: string
  - tenant_id: string
  - scope: string
  - issued_at: timestamp
  - expires_at: timestamp
  - revoked: boolean
  - revoked_at: timestamp (可选)
TTL:    expires_at + 7天 (审计保留)
```

**Redis 操作示例：**
```rust
impl TokenBlacklist {
    // 存储新 Token
    pub async fn store_jti(
        &self,
        tenant_id: &str,
        jti: &str,
        exp: u64,
    ) -> Result<(), RedisError> {
        let key = format!("credbridge:tokens:{}:active", tenant_id);

        // 添加到活跃集合，使用过期时间作为 score
        self.redis.zadd(&key, jti, exp as f64).await?;

        // 设置自动过期
        let ttl = exp - current_timestamp();
        self.redis.expire(&key, ttl as usize).await?;

        Ok(())
    }

    // 检查是否撤销
    pub async fn is_revoked(&self, tenant_id: &str, jti: &str) -> Result<bool, RedisError> {
        let revoked_key = format!("credbridge:tokens:{}:revoked", tenant_id);
        self.redis.sismember(&revoked_key, jti).await
    }

    // 撤销 Token
    pub async fn revoke_token(
        &self,
        tenant_id: &str,
        jti: &str,
    ) -> Result<(), RedisError> {
        let active_key = format!("credbridge:tokens:{}:active", tenant_id);
        let revoked_key = format!("credbridge:tokens:{}:revoked", tenant_id);

        // 从活跃集合移除
        self.redis.zrem(&active_key, jti).await?;

        // 添加到撤销集合
        self.redis.sadd(&revoked_key, jti).await?;

        // 更新元数据
        let meta_key = format!("credbridge:token:{}", jti);
        self.redis.hset(&meta_key, "revoked", "true").await?;
        self.redis.hset(&meta_key, "revoked_at", &current_timestamp().to_string()).await?;

        Ok(())
    }
}
```

#### Token Scope 权限设计

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenScope {
    // 凭证访问
    CredentialRead,      // 读取凭证元数据
    CredentialDecrypt,   // 解密凭证内容 (需要额外授权)
    CredentialWrite,     // 创建/更新凭证
    CredentialDelete,    // 删除凭证

    // 签名服务
    SignData,            // 数据签名
    SignTransaction,     // 交易签名 (高敏感)

    // 管理操作
    Admin,               // 所有管理权限
    TokenManage,         // Token 管理 (撤销/刷新)
    AuditRead,           // 审计日志读取

    // 系统
    HealthCheck,         // 健康检查
}

impl TokenScope {
    pub fn can_access(&self, operation: &Operation) -> bool {
        match (self, operation) {
            // 读权限可访问读操作
            (TokenScope::CredentialRead, Operation::ReadCredential) => true,

            // 解密权限可访问解密操作
            (TokenScope::CredentialDecrypt, Operation::DecryptCredential) => true,

            // 写权限可访问写操作
            (TokenScope::CredentialWrite, Operation::CreateCredential) => true,
            (TokenScope::CredentialWrite, Operation::UpdateCredential) => true,

            // Admin 拥有所有权限
            (TokenScope::Admin, _) => true,

            _ => false,
        }
    }
}
```

#### Token 生命周期管理

```
┌─────────────────────────────────────────────────────────────────┐
│                     Token 生命周期                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   颁发                    验证               过期/撤销          │
│     │                      │                      │             │
│     ▼                      ▼                      ▼             │
│  ┌──────┐              ┌──────┐              ┌──────┐          │
│  │创建  │              │检查  │              │清理  │          │
│  │jti   │─────────────>│黑名单│─────────────>│Redis │          │
│  │存储  │              │验证  │              │过期  │          │
│  │Redis │              │scope │              │数据  │          │
│  └──────┘              └──────┘              └──────┘          │
│     │                      │                      │             │
│     │                 15分钟后                 7天后            │
│     │                      │                      │             │
│  [撤销] <──────────────────┘                      │             │
│     │                                             │             │
│     ▼                                             ▼             │
│  加入黑名单                                  审计归档           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 安全架构总结

### 三层防御体系

| 层级 | 组件 | 保护机制 |
|------|------|----------|
| **L1: 硬件层** | Intel SGX | Sealing Key, 内存加密, 远程认证 |
| **L2: 密钥层** | 四层密钥体系 | HKDF 派生, 一次一密, 前向保密 |
| **L3: 访问层** | PASETO Token | 15分钟过期, jti 撤销, scope 限制 |

### 关键安全属性

1. **零知识架构**：服务器永不接触明文凭证
2. **硬件绑定**：密钥与 SGX 平台绑定，无法复制
3. **最小权限**：Token scope 精确控制访问范围
4. **完整审计**：所有操作记录到 immudb，不可篡改
5. **前向保密**：即使长期密钥泄露，历史凭证仍然安全

---
