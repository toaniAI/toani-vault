---
stepsCompleted:
  - step-01-validate-prerequisites
  - step-02-design-epics
  - step-03-create-stories
inputDocuments:
  - /Users/yvan/AIWorkspace/credbridge/_bmad-output/planning-artifacts/product-brief-credbridge-2026-03-10.md
  - /Users/yvan/AIWorkspace/credbridge/_bmad-output/planning-artifacts/architecture.md
  - /Users/yvan/AIWorkspace/credbridge/_bmad-output/planning-artifacts/prd.md
  - /Users/yvan/AIWorkspace/credbridge/docs/CredBridge_CN_设计规范_v1.0.md
project: CredBridge
user_name: CoPaw
date: '2026-03-10'
---

# CredBridge - Epic Breakdown

## Overview

本文档为 CredBridge（AI 原生零信任凭证保险库系统）提供完整的 Epic 和 User Story 分解，基于 PRD、架构设计和产品简报的需求分解为可实施的故事。

---

## Requirements Inventory

### Functional Requirements

**FR1: TEE 凭证保险库核心功能**
- TEE Enclave 内凭证加密存储与解密
- 四层密钥层次架构（L0-L3）实现
- 凭证加密使用 AES-256-GCM，密钥通过 HKDF-SHA-256 派生
- 凭证明文永不出 TEE 安全边界

**FR2: 有限 Scope Token 系统**
- PASETO v4.local Token 签发与验证
- Token 15 分钟有效期，jti 单次使用强制
- Redis 存储 Token 状态（活跃/撤销）
- Scope 权限控制：credential:read, credential:decrypt, credential:write, admin

**FR3: 审计日志系统**
- immudb 不可篡改审计日志存储
- 所有凭证访问操作记录（含时间戳、用户、操作类型、结果）
- 审计日志 Merkle Tree + 数字签名验证
- 审计记录保留期 7 天（Token 元数据）

**FR4: SDK 开发**
- TypeScript SDK（Node.js 22+）
- Rust SDK（后台集成）
- SDK 支持凭证存储、检索、Token 管理

**FR5: MCP 工具集成**
- credbridge_list_services: 列出已授权服务
- credbridge_execute: 执行具体操作
- credbridge_get_audit_log: 查询审计日志
- credbridge_request_scope: 请求新 Scope 授权
- credbridge_revoke_service: 吊销服务授权
- credbridge_tee_status: 查询 TEE 状态

**FR6: 多租户数据隔离**
- Schema-per-Tenant + RLS 混合方案
- PostgreSQL 行级安全策略
- 租户数据备份/恢复支持

**FR7: 远程认证（Remote Attestation）**
- Intel DCAP 远程认证流程
- Quote 生成与验证
- MRENCLAVE 测量值对比

**FR8: 存储后端集成**
- HashiCorp Vault KV v2 凭证密文存储
- immudb 审计日志存储
- PostgreSQL 元数据存储
- Redis Token 状态缓存

### Non-Functional Requirements

**NFR1: 性能要求**
- 凭证访问延迟 < 100ms（P99）
- Token 签发延迟 < 50ms
- API 并发处理 1000 RPS

**NFR2: 安全要求**
- 零成功入侵事件
- 密钥材料永不落地（never at rest in plaintext）
- 内存中密钥使用后立即 zeroize
- mTLS 1.3 服务间通信

**NFR3: 可用性要求**
- 系统可用性 > 99.9%
- 故障恢复时间 RTO < 5 分钟
- 数据恢复点目标 RPO < 1 分钟

**NFR4: 合规要求**
- 支持 SOC 2 Type I/II 审计
- GDPR/CCPA 数据主体权利
- 审计日志不可篡改

**NFR5: 可扩展性**
- 支持水平扩展
- 多 TEE 平台支持（SGX、SEV-SNP）
- 插件化 Connector 架构

### Additional Requirements

**架构决策要求（来自 Architecture）：**
- DA-001: Schema-per-Tenant + RLS 多租户隔离
- SA-001: L0-L3 四层密钥层次（HKDF-SHA256）
- SA-002: In-Process Enclave + SGX DCAP 远程认证
- SA-003: PASETO v4.local + 15min Access Token + jti 单次使用

**技术约束：**
- 后端语言：Rust（内存安全）
- TEE Enclave：Intel SGX SDK
- SDK：TypeScript（Node.js 22+）
- Token：PASETO v4.local

---

## Epic List

| ID | Epic | 描述 | 优先级 |
|----|------|------|--------|
| EP1 | TEE 核心安全架构 | 四层密钥体系、Enclave 实现、ECALL/OCALL 接口 | P0 |
| EP2 | 凭证保险库服务 | 凭证 CRUD、加密/解密、Vault 集成 | P0 |
| EP3 | Scope Token 系统 | PASETO Token 签发/验证、Redis 状态管理 | P0 |
| EP4 | 审计日志系统 | immudb 集成、审计事件记录、查询 API | P0 |
| EP5 | SDK 开发 | TypeScript SDK、Rust SDK、文档示例 | P0 |
| EP6 | MCP Server 集成 | MCP Tools 实现、OpenClaw 集成 | P1 |
| EP7 | 多租户架构 | 租户管理、Schema 隔离、RLS 策略 | P1 |
| EP8 | 远程认证 | DCAP 认证、Quote 验证、MRENCLAVE | P1 |
| EP9 | 部署与运维 | Docker 部署、监控、配置管理 | P2 |

---

## Epic 1: TEE 核心安全架构

**Epic Goal:**
实现基于 Intel SGX 的 TEE 安全架构，包含四层密钥层次、Enclave 内部密钥管理和安全的 ECALL/OCALL 接口。

### Story 1.1: L0-L3 四层密钥层次实现

**As a** 系统安全架构师
**I want** 实现四层密钥层次（L0硬件根密钥 → L1 Enclave主密钥 → L2 用户保险库密钥 → L3 凭证加密密钥）
**So that** 凭证加密密钥可以安全派生且前向保密

**Acceptance Criteria:**

**Given** SGX Enclave 已初始化
**When** 调用密钥派生流程
**Then** L0 从 SGX Sealing Key 获取硬件绑定密钥
**And** L1 通过 HKDF-Extract 从 L0 派生 Enclave Master Key
**And** L2 通过 HKDF-Expand(tenant_id + user_id) 派生用户保险库密钥
**And** L3 通过 HKDF-Expand(credential_id + purpose) 派生凭证加密密钥

**Given** 凭证加密操作
**When** 使用 L3 密钥加密凭证
**Then** 使用 AES-256-GCM 算法
**And** 96-bit 随机 nonce 生成
**And** 128-bit auth tag 验证完整性
**And** L3 密钥在使用后立即 zeroize

### Story 1.2: SGX Enclave 核心模块

**As a** TEE 开发工程师
**I want** 实现 SGX Enclave 内部核心模块（密钥管理、加密/解密、Token 签发）
**So that** 所有敏感操作在安全边界内执行

**Acceptance Criteria:**

**Given** Enclave 已加载
**When** 执行密钥管理操作
**Then** 密钥管理模块在 EPC 内存中运行
**And** 主密钥（L1）持久存在于安全内存
**And** 用户密钥（L2）临时缓存（TTL: 5min）
**And** 凭证密钥（L3）即时派生，用完即焚

**Given** 加密/解密请求
**When** 调用 Enclave 加密接口
**Then** AES-256-GCM 加密在 Enclave 内执行
**And** 明文数据不离开 EPC 边界
**And** 返回加密后的 EncryptedBlob

**Given** Token 签发请求
**When** 在 Enclave 内签发 PASETO Token
**Then** 使用 XChaCha20-Poly1305 对称加密
**And** Token 密钥从 L2 派生
**And** 返回 v4.local 格式 Token

### Story 1.3: ECALL/OCALL 接口实现

**As a** 系统集成工程师
**I want** 实现安全的 ECALL（Host → Enclave）和 OCALL（Enclave → Host）接口
**So that** Enclave 可以与外部系统安全通信

**Acceptance Criteria:**

**Given** Host 应用需要密钥派生
**When** 调用 ecall_derive_user_vault_key(tenant_id, user_id)
**Then** Enclave 内派生用户保险库密钥
**And** 返回 key_handle（不包含实际密钥）

**Given** Host 需要加密凭证
**When** 调用 ecall_encrypt_credential(tenant_id, user_id, credential_id, plaintext)
**Then** Enclave 内派生 L3 密钥并加密
**And** 返回 EncryptedBlob（密文 + nonce + auth_tag）

**Given** Enclave 需要写入审计日志
**When** 调用 ocall_write_audit_log(event_type, data_hash)
**Then** 仅传出事件类型和数据哈希（不含敏感信息）
**And** Host 端将审计记录写入 immudb

**Given** Enclave 需要获取密封数据
**When** 调用 ocall_read_sealed_data(key_id)
**Then** Host 返回之前密封存储的密钥包
**And** Enclave 使用 SGX Unsealing 解密

### Story 1.4: 内存安全与密钥清理

**As a** 安全工程师
**I want** 实现内存安全策略和密钥自动清理
**So that** 密钥材料不会残留在内存中

**Acceptance Criteria:**

**Given** 密钥结构体使用 zeroize
**When** CredentialKey 实例被 drop
**Then** 自动调用 ZeroizeOnDrop trait
**And** key_material 被覆写为零

**Given** 用户密钥缓存
**When** 缓存项超过 TTL（5分钟）
**Then** 自动从内存中移除
**And** 调用 zeroize 清理密钥数据

**Given** Enclave 重启
**When** 新的 Enclave 实例启动
**Then** 从密封存储恢复 L1 主密钥
**And** 验证 MRSIGNER 兼容性
**And** 重新初始化密钥层次

---

## Epic 2: 凭证保险库服务

**Epic Goal:**
实现凭证的加密存储、检索和管理服务，集成 HashiCorp Vault 作为密文存储后端。

### Story 2.1: 凭证数据模型与存储

**As a** 后端开发工程师
**I want** 设计并实现凭证数据模型
**So that** 凭证可以安全存储和检索

**Acceptance Criteria:**

**Given** 创建新凭证请求
**When** 调用凭证创建 API
**Then** 生成 UUID v7 作为凭证 ID
**And** 存储凭证类型（username_password/oauth_refresh/api_key/session_cookie/kyc_document）
**And** 记录 user_id（哈希后）、service_id、created_at、expires_at

**Given** 凭证加密存储
**When** 存储凭证到 Vault
**Then** encrypted_payload 包含 version=2
**And** algorithm='AES-256-GCM'
**And** kdf='HKDF-SHA-256'
**And** nonce（12 bytes）、auth_tag（16 bytes）、ciphertext

**Given** 查询凭证
**When** 根据 credential_id 查询
**Then** 返回凭证元数据（不含加密载荷）
**And** 只有授权用户可访问其租户数据

### Story 2.2: 凭证 CRUD API

**As a** API 开发者
**I want** 实现凭证的增删改查 API
**So that** 用户可以通过 API 管理凭证

**Acceptance Criteria:**

**Given** 用户请求创建凭证
**When** POST /api/v1/credentials
**Then** 验证用户权限（Token Scope: credential:write）
**And** 在 TEE 内加密凭证内容
**And** 存储加密后的密文到 Vault
**And** 记录审计日志

**Given** 用户请求读取凭证列表
**When** GET /api/v1/credentials
**Then** 验证 Token Scope: credential:read
**And** 返回用户有权限访问的凭证元数据列表
**And** 不包含加密后的凭证内容

**Given** 用户请求解密凭证
**When** POST /api/v1/credentials/{id}/decrypt
**Then** 验证 Token Scope: credential:decrypt
**And** 在 TEE 内解密密文
**And** 返回明文给授权 Connector
**And** 记录访问审计日志

**Given** 用户请求删除凭证
**When** DELETE /api/v1/credentials/{id}
**Then** 验证 Token Scope: credential:write 或 admin
**And** 从 Vault 中软删除凭证
**And** 记录删除审计日志

### Story 2.3: Vault 后端集成

**As a** DevOps 工程师
**I want** 集成 HashiCorp Vault 作为凭证存储后端
**So that** 凭证密文可以安全持久化存储

**Acceptance Criteria:**

**Given** Vault 服务已配置
**When** 系统启动时
**Then** 验证 Vault 连接
**And** 初始化 KV v2 引擎
**And** 配置 Vault Token（仅 TEE Enclave 持有）

**Given** 存储凭证密文
**When** 写入 Vault KV v2
**Then** 使用路径格式：secret/credbridge/{tenant_id}/{credential_id}
**And** 启用 Vault 自带 AES-256 加密（双重加密）

**Given** 读取凭证密文
**When** 从 Vault 读取
**Then** Vault 返回加密的密文
**And** 只有持有有效 Vault Token 的 Enclave 可读取

---

## Epic 3: Scope Token 系统

**Epic Goal:**
实现基于 PASETO v4.local 的有限 Scope Token 系统，支持 Token 签发、验证、撤销和生命周期管理。

### Story 3.1: PASETO Token 签发与验证

**As a** 安全工程师
**I want** 实现 PASETO v4.local Token 的签发和验证
**So that** Agent 可以获取有限权限的访问凭证

**Acceptance Criteria:**

**Given** 用户请求 Token
**When** 调用 Token 签发 API
**Then** 在 Enclave 内构建 Claims：
  - iss: "credbridge-vault"
  - sub: user_id
  - aud: tenant_id
  - exp: current_time + 900s（15分钟）
  - jti: UUID v4
  - scope: 请求的权限范围
  - mfa_verified: true/false
**And** 使用 XChaCha20-Poly1305 加密
**And** 返回 v4.local.{payload}.{footer} 格式 Token

**Given** 验证 Token 请求
**When** 解析并验证 Token
**Then** 在 Enclave 内解密 Token
**And** 验证 iss、aud、exp（未过期）
**And** 验证 scope 权限
**And** 检查 jti 是否已撤销

**Given** Token 过期
**When** exp < current_time
**Then** 拒绝验证并返回 TokenExpired 错误

### Story 3.2: Redis Token 状态管理

**As a** 后端工程师
**I want** 使用 Redis 管理 Token 状态（活跃/撤销）
**So that** 可以实现 Token 撤销和单次使用强制

**Acceptance Criteria:**

**Given** 新 Token 签发
**When** Token 创建成功
**Then** 存储 jti 到 Redis Sorted Set：credbridge:tokens:{tenant_id}:active
**And** 设置 score 为 exp_timestamp
**And** 设置 TTL = exp - current_time

**Given** 验证 Token
**When** 检查 Token 有效性
**Then** 查询 Redis：SISMEMBER credbridge:tokens:{tenant_id}:revoked {jti}
**And** 如果 jti 在撤销集合中，返回 TokenRevoked 错误

**Given** 撤销 Token
**When** 调用 Token 撤销 API
**Then** 从活跃集合移除 jti
**And** 添加到撤销集合：SADD credbridge:tokens:{tenant_id}:revoked {jti}
**And** 更新 Token 元数据：revoked=true, revoked_at=timestamp

**Given** Token 元数据查询
**When** 查询 Token 详情
**Then** 从 Hash 读取：credbridge:token:{jti}
**And** 返回 user_id, tenant_id, scope, issued_at, expires_at, revoked

### Story 3.3: Token Scope 权限系统

**As a** 产品经理
**I want** 实现细粒度的 Token Scope 权限控制
**So that** Agent 只能访问被授权的资源

**Acceptance Criteria:**

**Given** Token Scope 枚举
**When** 定义权限类型
**Then** 支持以下 Scope：
  - credential:read - 读取凭证元数据
  - credential:decrypt - 解密凭证内容
  - credential:write - 创建/更新凭证
  - credential:delete - 删除凭证
  - token:manage - 管理 Token（撤销/刷新）
  - audit:read - 读取审计日志
  - admin - 所有管理权限

**Given** 权限检查
**When** 验证操作权限
**Then** 实现 Scope.can_access(operation) 方法
**And** credential:read 可以访问 ReadCredential 操作
**And** credential:decrypt 可以访问 DecryptCredential 操作
**And** admin 拥有所有权限

**Given** 受限 Token
**When** Token 包含 credential_ids 列表
**Then** 只能访问列表中的凭证
**And** 访问其他凭证返回 AccessDenied 错误

---

## Epic 4: 审计日志系统

**Epic Goal:**
实现基于 immudb 的不可篡改审计日志系统，记录所有凭证访问操作。

### Story 4.1: 审计事件记录

**As a** 合规工程师
**I want** 记录所有凭证访问相关的审计事件
**So that** 满足合规要求并支持事后追溯

**Acceptance Criteria:**

**Given** 凭证访问操作
**When** 任何凭证解密/访问发生
**Then** 记录审计条目：
  - id: UUID v7
  - user_id_hash: SHA-256(user_id)
  - timestamp: UTC 时间戳
  - session_id: OpenClaw 会话 ID
  - service: 目标服务 ID
  - action: 操作类型
  - risk_tier: 0/1/2
  - outcome: success/failure/denied/timeout
  - tee_mrenclave: Enclave 测量值
  - action_token_jti: Token ID

**Given** 审计事件写入
**When** 写入 immudb
**Then** 计算 Merkle Tree 哈希
**And** 数字签名审计条目
**And** 追加到不可篡改日志链

**Given** 敏感参数处理
**When** 记录操作参数
**Then** PII 数据脱敏（如 SSN 替换为 [SSN_REDACTED]）
**And** 保留参数类型信息用于审计

### Story 4.2: 审计日志查询 API

**As a** 安全审计员
**I want** 查询和导出审计日志
**So that** 可以分析安全事件和生成合规报告

**Acceptance Criteria:**

**Given** 查询审计日志请求
**When** GET /api/v1/audit-logs
**Then** 验证 Token Scope: audit:read 或 admin
**And** 支持过滤参数：service, since, until, user_id, outcome
**And** 支持分页：page, pageSize

**Given** 导出审计日志
**When** GET /api/v1/audit-logs/export
**Then** 支持 JSON/CSV 格式导出
**And** 导出指定时间范围内的日志
**And** 包含完整性校验哈希

**Given** 审计日志验证
**When** 调用验证 API
**Then** 验证 Merkle Tree 完整性
**And** 验证数字签名
**And** 返回验证结果（是否被篡改）

### Story 4.3: immudb 集成

**As a** 后端工程师
**I want** 集成 immudb 作为审计日志存储
**So that** 确保审计日志的不可篡改性

**Acceptance Criteria:**

**Given** immudb 服务配置
**When** 系统启动
**Then** 连接 immudb 实例
**And** 创建审计日志数据库（如果不存在）
**And** 配置集合和索引

**Given** 写入审计日志
**When** 调用 immudb 写入 API
**Then** 数据被追加到 Merkle Tree
**And** 计算并存储状态哈希

**Given** 读取审计日志
**When** 查询历史记录
**Then** immudb 返回带证明的数据
**And** 可以验证数据未被篡改

---

## Epic 5: SDK 开发

**Epic Goal:**
开发 TypeScript 和 Rust SDK，使开发者可以方便地集成 CredBridge 到他们的应用。

### Story 5.1: TypeScript SDK

**As a** 前端/Node.js 开发者
**I want** 使用 TypeScript SDK 集成 CredBridge
**So that** 可以在 Node.js 应用中安全使用凭证管理

**Acceptance Criteria:**

**Given** SDK 初始化
**When** 创建 CredBridgeClient 实例
**Then** 配置 baseURL、apiKey、teeAttestation
**And** 可选配置：timeout, retries

**Given** 凭证存储
**When** 调用 client.storeCredential(credential)
**Then** SDK 加密敏感数据（如果本地加密启用）
**And** 调用 API 创建凭证
**And** 返回 credential_id

**Given** 凭证检索
**When** 调用 client.getCredential(id)
**Then** 返回凭证元数据
**And** 可选返回解密后的凭证（如果权限允许）

**Given** Token 管理
**When** 调用 client.requestToken(scope)
**Then** 向 API 请求指定 Scope 的 Token
**And** 缓存 Token 直到过期
**And** 自动刷新即将过期的 Token

**Given** 错误处理
**When** API 返回错误
**Then** SDK 抛出类型化错误：
  - VaultError.InvalidToken
  - VaultError.TokenExpired
  - VaultError.AccessDenied
  - VaultError.CredentialNotFound

### Story 5.2: Rust SDK

**As a** Rust 开发者
**I want** 使用 Rust SDK 集成 CredBridge
**So that** 可以在 Rust 应用中安全使用凭证管理

**Acceptance Criteria:**

**Given** SDK 依赖
**When** 在 Cargo.toml 添加 credbridge-sdk
**Then** 支持异步运行时（tokio）
**And** 依赖：reqwest, serde, zeroize

**Given** 客户端创建
**When** 使用 builder 模式创建客户端
```rust
let client = CredBridgeClient::builder()
    .base_url("https://api.credbridge.ai")
    .api_key("sk_...")
    .tee_attestation(true)
    .build()?;
```
**Then** 客户端配置完成

**Given** 异步凭证操作
**When** 调用 client.store_credential(credential).await
**Then** 返回 Result<CredentialId, VaultError>
**And** 正确处理超时和重试

**Given** 内存安全
**When** 处理敏感数据（密码、密钥）
**Then** 使用 zeroize 自动清理
**And** SecretString 类型防止意外日志泄露

### Story 5.3: SDK 文档和示例

**As a** 开发者
**I want** 有完整的 SDK 文档和示例代码
**So that** 可以快速上手集成 CredBridge

**Acceptance Criteria:**

**Given** SDK 文档
**When** 访问文档网站
**Then** 包含快速开始指南
**And** 完整的 API 参考文档
**And** TypeScript/Rust 代码示例

**Given** 示例项目
**When** 查看 examples/ 目录
**Then** 包含基本凭证管理示例
**And** 包含 Token 使用示例
**And** 包含错误处理示例

**Given** 集成指南
**When** 阅读 README.md
**Then** 包含安装说明
**And** 包含配置指南
**And** 包含最佳实践

---

## Epic 6: MCP Server 集成

**Epic Goal:**
实现标准 MCP Server，提供 Tools 供 OpenClaw Agent 调用。

### Story 6.1: MCP Server 基础架构

**As a** 集成工程师
**I want** 实现 MCP Server 基础架构
**So that** Agent 可以通过 MCP 协议调用 CredBridge

**Acceptance Criteria:**

**Given** MCP Server 启动
**When** 调用 credbridge start
**Then** 启动 SSE Transport 服务（默认端口 3721）
**And** 注册 MCP Tools
**And** 输出服务状态

**Given** Agent 连接
**When** Agent 通过 mcporter 连接
**Then** 验证 Bearer Token
**And** 建立 SSE 连接
**And** 返回可用 Tools 列表

**Given** TEE 状态查询
**When** 调用 credbridge_tee_status
**Then** 返回当前 Enclave 状态
**And** 返回 MRENCLAVE 测量值
**And** 返回远程认证报告

### Story 6.2: MCP Tools 实现

**As a** Agent 开发者
**I want** 使用 MCP Tools 与 CredBridge 交互
**So that** Agent 可以安全执行用户授权的操作

**Acceptance Criteria:**

**Given** 查询已授权服务
**When** 调用 credbridge_list_services
**Then** 返回用户已配置的服务列表
**And** 每个服务包含：service_id, scopes, tier

**Given** 执行操作
**When** 调用 credbridge_execute
**Then** 参数：service, action, params, reason
**And** Gateway 验证 Scope 权限
**And** TEE 签发 Action Token
**And** Connector 执行操作
**And** 返回操作结果

**Given** 查询审计日志
**When** 调用 credbridge_get_audit_log
**Then** 参数：service?, limit?, since?
**And** 返回审计记录列表
**And** 不包含凭证数据

**Given** 请求新 Scope
**When** 调用 credbridge_request_scope
**Then** 参数：service, requested_scopes, reason
**And** 触发用户审批流程（如需要）
**And** 返回审批状态

**Given** 吊销服务授权
**When** 调用 credbridge_revoke_service
**Then** 参数：service
**And** 吊销该服务的所有 Token
**And** 删除相关凭证（可选）

---

## Epic 7: 多租户架构

**Epic Goal:**
实现 Schema-per-Tenant + RLS 多租户数据隔离架构。

### Story 7.1: 租户管理

**As a** SaaS 运营人员
**I want** 管理多租户（创建、配置、删除）
**So that** 可以为不同客户隔离数据

**Acceptance Criteria:**

**Given** 创建新租户
**When** 调用创建租户 API
**Then** 生成 tenant_id（UUID）
**And** 创建租户专属 PostgreSQL Schema
**And** 初始化租户配置表
**And** 创建 RLS 策略

**Given** 租户配置
**When** 更新租户设置
**Then** 支持配置：存储配额、Token 有效期、审计保留期
**And** 配置生效到租户环境

**Given** 租户删除
**When** 调用删除租户 API
**Then** 软删除租户数据
**And** 吊销该租户所有活跃 Token
**And** 保留审计日志（合规要求）

### Story 7.2: Schema 隔离与 RLS

**As a** 数据库工程师
**I want** 实现 Schema-per-Tenant + RLS 数据隔离
**So that** 租户数据完全隔离

**Acceptance Criteria:**

**Given** 数据库连接
**When** 租户查询数据
**Then** 动态切换 PostgreSQL Schema（SET search_path）
**And** 使用连接池管理多租户连接

**Given** RLS 策略
**When** 在租户 Schema 内查询
**Then** 应用行级安全策略
**And** 用户只能访问其 user_id 的数据

**Given** 租户数据备份
**When** 导出租户数据
**Then** 仅导出该 Schema 的数据
**And** 加密导出文件

---

## Epic 8: 远程认证

**Epic Goal:**
实现 Intel DCAP 远程认证流程，使用户可以验证 Enclave 真实性。

### Story 8.1: DCAP 远程认证实现

**As a** 安全工程师
**I want** 实现 DCAP 远程认证流程
**So that** 用户可以密码学验证 Enclave 真实性

**Acceptance Criteria:**

**Given** 认证挑战请求
**When** 用户请求远程认证
**Then** 生成随机 nonce
**And** 返回给客户端

**Given** Quote 生成
**When** Enclave 收到 nonce
**Then** 在 Enclave 内生成 Quote
**And** 包含：MRENCLAVE, MRSIGNER, nonce, 安全版本
**And** 使用 Intel QE 签名

**Given** Quote 验证
**When** 用户验证 Quote
**Then** 验证 QE 签名
**And** 验证 nonce 匹配
**And** 验证 MRENCLAVE 在官方注册表中

**Given** 可信信道建立
**When** 认证验证通过
**Then** 建立 RA-TLS 连接
**And** 证书绑定 Enclave 测量值

### Story 8.2: MRENCLAVE 注册与验证

**As a** 运维工程师
**I want** 管理 MRENCLAVE 注册表
**So that** 可以验证官方发布的 Enclave 版本

**Acceptance Criteria:**

**Given** MRENCLAVE 注册
**When** 发布新版本 Enclave
**Then** 计算并记录 MRENCLAVE
**And** 记录版本号、发布日期、安全补丁
**And** 发布到公开注册表

**Given** 验证请求
**When** 验证 Enclave 版本
**Then** 对比 MRENCLAVE 与注册表
**And** 返回验证结果和版本信息

---

## Epic 9: 部署与运维

**Epic Goal:**
提供 Docker 部署、监控和配置管理支持。

### Story 9.1: Docker 部署支持

**As a** DevOps 工程师
**I want** 使用 Docker 部署 CredBridge
**So that** 可以快速启动和扩展服务

**Acceptance Criteria:**

**Given** Docker Compose 配置
**When** 运行 docker-compose up
**Then** 启动所有服务：
  - CredBridge API 服务
  - PostgreSQL 数据库
  - Redis 缓存
  - immudb 审计日志
  - HashiCorp Vault

**Given** TEE 环境部署
**When** 部署到 SGX 服务器
**Then** 检测 SGX 可用性
**And** 加载 Enclave 镜像
**And** 初始化远程认证

**Given** 配置管理
**When** 通过环境变量配置
**Then** 支持：
  - DATABASE_URL
  - REDIS_URL
  - VAULT_ADDR
  - IMMUDB_ADDR
  - TEE_MODE (sgx/software)

### Story 9.2: 监控与健康检查

**As a** 运维工程师
**I want** 监控 CredBridge 运行状态
**So that** 可以及时发现和处理问题

**Acceptance Criteria:**

**Given** 健康检查端点
**When** GET /health
**Then** 返回各组件状态：
  - API: healthy/unhealthy
  - Database: connected/error
  - Redis: connected/error
  - Vault: connected/error
  - Enclave: initialized/error

**Given** 指标采集
**When** 调用 /metrics
**Then** 返回 Prometheus 格式指标：
  - 请求延迟（P50, P95, P99）
  - 错误率
  - Token 签发数量
  - 凭证访问次数
  - Enclave 内存使用

---

## Requirements Coverage Map

| FR/NFR | 覆盖的 Story | 状态 |
|--------|-------------|------|
| FR1 (TEE 保险库) | 1.1, 1.2, 1.3, 1.4, 2.1, 2.2 | 规划中 |
| FR2 (Scope Token) | 1.2, 3.1, 3.2, 3.3 | 规划中 |
| FR3 (审计日志) | 1.3, 4.1, 4.2, 4.3 | 规划中 |
| FR4 (SDK) | 5.1, 5.2, 5.3 | 规划中 |
| FR5 (MCP 工具) | 6.1, 6.2 | 规划中 |
| FR6 (多租户) | 7.1, 7.2 | 规划中 |
| FR7 (远程认证) | 8.1, 8.2 | 规划中 |
| FR8 (存储后端) | 2.3, 4.3 | 规划中 |
| NFR1 (性能) | 全部 | 验收标准 |
| NFR2 (安全) | 1.1, 1.4, 3.1 | 核心关注 |
| NFR3 (可用性) | 9.2 | 监控覆盖 |
| NFR4 (合规) | 4.1, 4.2 | 已实现 |
| NFR5 (可扩展性) | 7.2, 9.1 | 架构支持 |

---

*文档生成时间: 2026-03-10*
*作者: CoPaw*
