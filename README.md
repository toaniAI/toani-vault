# CredBridge - TEE Enclave 核心模块

## 项目概述

CredBridge 是一个 AI 原生零信任凭证保险库系统，采用 Intel SGX TEE（可信执行环境）技术，实现四层密钥层次架构，确保凭证数据在硬件级别的安全隔离中处理。

## 功能特性

| 功能 | 状态 | 描述 |
|------|------|------|
| **TEE 安全保险库** | ✅ 已实现 | Intel SGX TEE 硬件级加密隔离 |
| **四层密钥架构** | ✅ 已实现 | L0-L3 密钥层次派生 |
| **凭证加密存储** | ✅ 已实现 | AES-256-GCM 认证加密 |
| **PASETO Token 认证** | ✅ 已实现 | v4.local 安全 Token |
| **审计日志** | ✅ 已实现 | 不可篡改的审计追踪 |
| **远程认证** | ✅ 已实现 | SGX DCAP 远程证明 |
| **多租户隔离** | ✅ 已实现 | Schema-per-Tenant + RLS |
| **Web 控制台** | ✅ 已实现 | React 管理界面 |
| **TypeScript SDK** | ✅ 已实现 | TypeScript 客户端 SDK |
| **Rust SDK** | ✅ 已实现 | Rust 客户端 SDK |
| **MCP Server** | ✅ 已实现 | Model Context Protocol 支持 |
| **TEE Sandbox API** | ✅ 已实现 | TEE 安全执行沙箱（浏览器自动化 + AI 审核） |
| **TypeScript SDK Sandbox** | ✅ 已实现 | Sandbox 客户端 SDK（WebSocket 实时连接） |
| **OpenAPI 规范** | ✅ 已实现 | 完整的 REST API 文档 |
| **CLI 命令行工具** | 📝 计划中 | 命令行管理工具（当前可通过 SDK 或 API 使用系统） |

## 架构设计

### 四层密钥层次

```text
L0: Hardware Root Key (SGX Sealing Key)
  │ HKDF-Extract
  ▼
L1: Enclave Master Key (Enclave 内派生)
  │ HKDF-Expand(tenant_id + user_id)
  ▼
L2: User Vault Key (每用户独立，缓存 5 分钟)
  │ HKDF-Expand(credential_id + purpose)
  ▼
L3: Credential Encryption Key (每条凭证独立)
  │ AES-256-GCM
  ▼
Encrypted Credential
```

### 核心组件

| 模块 | 路径 | 功能 |
|------|------|------|
| Enclave | `src/tee/enclave.rs` | Enclave 生命周期管理、加密/解密操作 |
| Keys | `src/tee/keys.rs` | 密钥管理（TTL 缓存、重启恢复、MRSIGNER 验证） |
| Cleanup | `src/tee/cleanup.rs` | 密钥清理策略（Zeroize、后台调度器） |
| Sealing | `src/tee/sealing.rs` | SGX Sealing Key 获取、密封存储 |
| Attestation | `src/tee/attestation.rs` | SGX DCAP 远程认证协议 |
| DCAP | `src/tee/dcap.rs` | DCAP Quote 生成和验证 |
| Quote | `src/tee/quote.rs` | Quote 结构解析和序列化 |
| Challenge | `src/tee/challenge.rs` | 挑战-响应协议 |
| Sandbox | `src/tee/sandbox/` | TEE 安全执行沙箱（浏览器自动化） |
| Review | `src/tee/sandbox/review/` | AI 审核引擎（操作审核、提示词安全） |
| Export | `src/tee/sandbox/export/` | 安全导出通道（截图、数据导出） |
| Keys | `src/crypto/keys.rs` | 四层密钥结构定义 |
| HKDF | `src/crypto/hkdf.rs` | 密钥派生实现 |
| Vault Models | `src/vault/models.rs` | 凭证数据模型（UUID v7、多租户隔离） |
| Vault Storage | `src/vault/storage.rs` | 凭证存储逻辑（内存/PostgreSQL） |

### 远程认证协议

```text
┌─────────────────────────────────────────────────────────────┐
│              SGX DCAP 远程认证流程                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Verifier (挑战者)              Prover (Enclave)            │
│        │                              │                     │
│        │ 1. 生成随机挑战               │                     │
│        │ ─────────────────────────────>│                     │
│        │                              │                     │
│        │                              │ 2. 生成 Quote        │
│        │                              │    REPORT_DATA =    │
│        │                              │    hash(challenge + │
│        │                              │    identity)        │
│        │                              │                     │
│        │ 3. 返回 Quote                 │                     │
│        │ <─────────────────────────────│                     │
│        │                              │                     │
│        │ 4. 验证 Quote                 │                     │
│        │    - 验证 ECDSA 签名          │                     │
│        │    - 验证 MRENCLAVE           │                     │
│        │    - 验证挑战绑定             │                     │
│        │                              │                     │
│        │ 5. 建立安全通道               │                     │
│        │ ─────────────────────────────>│                     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### TEE Sandbox 安全执行架构

CredBridge TEE Sandbox 提供"凭证不出 Enclave"的安全浏览器自动化能力，支持 AI Agent 在可信执行环境内完成敏感操作。

```text
┌─────────────────────────────────────────────────────────────────┐
│                    TEE Sandbox 架构                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                  SDK / API 客户端                        │   │
│  │         (TypeScript SDK / REST API / MCP)               │   │
│  └──────────────────────────┬──────────────────────────────┘   │
│                             │  WebSocket / HTTPS               │
│  ┌──────────────────────────▼──────────────────────────────┐   │
│  │              CredBridge API Gateway                      │   │
│  │        (认证、路由、审计日志、请求限流)                  │   │
│  └──────────────────────────┬──────────────────────────────┘   │
│                             │  内部 API                        │
│  ╔══════════════════════════╧══════════════════════════════╗   │
│  ║                    Intel SGX TEE                        ║   │
│  ║  ┌─────────────────────────────────────────────────┐   ║   │
│  ║  │           TEE Sandbox Manager                    │   ║   │
│  ║  │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐  │   ║   │
│  ║  │  │ Session Pool│  │NsjailSandbox│  │AI Review │  │   ║   │
│  ║  │  │ (热实例池)  │  │(隔离沙箱)   │  │ Engine   │  │   ║   │
│  ║  │  └──────┬──────┘  └──────┬──────┘  └────┬─────┘  │   ║   │
│  ║  │         │                │              │        │   ║   │
│  ║  │         └────────────────┴──────────────┘        │   ║   │
│  ║  │                        │                         │   ║   │
│  ║  │         ┌──────────────▼──────────────┐          │   ║   │
│  ║  │         │    Secure Export Channel    │          │   ║   │
│  ║  │         │  (截图审核 / 数据导出签名)  │          │   ║   │
│  ║  │         └─────────────────────────────┘          │   ║   │
│  ║  └─────────────────────────────────────────────────┘   ║   │
│  ║                          │                             ║   │
│  ║  ┌───────────────────────┼───────────────────────────┐ ║   │
│  ║  │                       ▼                           │ ║   │
│  ║  │  ┌─────────────────────────────────────────────┐ │ ║   │
│  ║  │  │         Chromium (nsjail 隔离)              │ │ ║   │
│  ║  │  │  • Namespaces (PID/Network/Mount/IPC)       │ │ ║   │
│  ║  │  │  • seccomp-bpf 系统调用过滤                 │ │ ║   │
│  ║  │  │  • cgroups v2 资源限制                      │ │ ║   │
│  ║  │  │  • Credential Namespace 隔离              │ │ ║   │
│  ║  │  └─────────────────────────────────────────────┘ │ ║   │
│  ║  └──────────────────────────────────────────────────┘ ║   │
│  ╚═══════════════════════════════════════════════════════╝   │
│                             │                                  │
│  ┌──────────────────────────▼──────────────────────────┐     │
│  │              外部服务 (只读连接)                     │     │
│  │  PostgreSQL    Redis    LLM API    Vault            │     │
│  └─────────────────────────────────────────────────────┘     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Sandbox 核心组件

| 组件 | 路径 | 功能描述 |
|------|------|----------|
| **Session** | `src/tee/sandbox/session.rs` | 会话生命周期管理（创建/运行/暂停/关闭） |
| **Pool** | `src/tee/sandbox/pool.rs` | NsjailSandbox 热实例池（≤100ms 启动） |
| **Nsjail** | `src/tee/sandbox/nsjail.rs` | nsjail 沙箱封装（Namespaces + cgroups + seccomp） |
| **Credential NS** | `src/tee/sandbox/credential_ns.rs` | 凭证命名空间隔离 |
| **Operation Reviewer** | `src/tee/sandbox/review/operation.rs` | 操作级 AI 审核 |
| **Prompt Injection Detector** | `src/tee/sandbox/review/injection.rs` | 提示词注入检测 |
| **Isolated Prompt Builder** | `src/tee/sandbox/review/prompt.rs` | 安全提示词构建 |
| **Screenshot Reviewer** | `src/tee/sandbox/export/screenshot.rs` | 截图内容审核 |
| **Export Signer** | `src/tee/sandbox/export/signer.rs` | 导出数据签名验证 |

### 安全特性

| 特性 | 实现 | 安全等级 |
|------|------|----------|
| **Namespace 隔离** | PID/Network/Mount/IPC/UTS 隔离 | 🔴 Critical |
| **seccomp 过滤** | 白名单系统调用（~50个安全调用） | 🔴 Critical |
| **cgroups 限制** | CPU/内存/进程/IO 资源限制 | 🟠 High |
| **凭证隔离** | 每会话独立的凭证命名空间 | 🔴 Critical |
| **AI 操作审核** | 每个操作经过 LLM 安全审核 | 🟠 High |
| **提示词安全** | 多层注入检测和隔离构建 | 🟠 High |
| **安全导出** | 截图/数据导出前 AI 审核 + 数字签名 | 🟠 High |
| **无根运行** | 沙箱内以非特权用户运行 | 🟡 Medium |

## 快速开始

### 1. 创建并初始化 Enclave

```rust
use vault_service::tee::{Enclave, EnclaveConfig};

// 使用默认配置
let config = EnclaveConfig::default();
let mut enclave = Enclave::new(config);

// 初始化 Enclave
enclave.initialize().expect("Failed to initialize enclave");
```

### 2. 配置选项

```rust
use vault_service::tee::{EnclaveConfig, SealPolicy};

let config = EnclaveConfig {
    // 用户密钥缓存 TTL（秒）
    user_key_ttl: 300,  // 5 分钟

    // 密封策略
    seal_policy: SealPolicy::Mrsigner,  // 或 SealPolicy::Mrenclave

    // 密封数据存储路径
    sealed_storage_path: ".sealed".to_string(),

    // Enclave 名称
    name: "credbridge-enclave".to_string(),

    // 调试模式（仅开发使用）
    debug_mode: false,
};
```

### 3. 加密凭证

```rust
let plaintext = b"user-password-123";

let encrypted = enclave.encrypt_credential(
    "tenant_123",      // 租户 ID
    "user_456",        // 用户 ID
    "cred_789",        // 凭证 ID
    plaintext
).expect("Encryption failed");
```

### 4. 解密凭证

```rust
let decrypted = enclave.decrypt_credential(
    "tenant_123",
    "user_456",
    "cred_789",
    &encrypted
).expect("Decryption failed");

assert_eq!(decrypted, plaintext);
```

### 5. 关闭 Enclave

```rust
enclave.shutdown().expect("Failed to shutdown enclave");
```

## 远程认证（Remote Attestation）

远程认证允许验证者（Verifier）验证 Enclave 的身份和完整性，建立安全可信通道。

### 快速开始

#### Verifier 端（验证者）

```rust
use vault_service::tee::{
    AttestationService, ChallengeProtocol, SecureChannel
};

// 创建认证服务
let attestation_service = AttestationService::new()
    .allow_mrenclave(expected_mrenclave)
    .allow_mrsigner(expected_mrsigner);

// 创建挑战-响应协议处理器
let verifier = ChallengeProtocol::new(attestation_service);

// 1. 生成挑战
let challenge = verifier.generate_challenge(None, None)?;

// 2. 发送挑战给 Prover
send_to_prover(&challenge);

// 3. 接收响应并验证
let response = receive_from_prover();
let identity = generate_enclave_identity(&expected_enclave);
let result = verifier.verify_response(&response, &identity)?;

// 4. 建立安全通道
let channel = SecureChannel::establish(&result, "channel_1", 3600)?;
```

#### Prover 端（Enclave）

```rust
use vault_service::tee::{
    Enclave, ProverProtocol, AttestationService
};

// 初始化 Enclave
let mut enclave = Enclave::new(EnclaveConfig::default());
enclave.initialize()?;

// 创建 Prover 协议处理器
let attestation_service = AttestationService::new();
let prover = ProverProtocol::new(attestation_service);

// 1. 接收挑战
let challenge = receive_from_verifier();

// 2. 生成响应（生成 Quote）
let response = prover.respond_to_challenge(&enclave, &challenge)?;

// 3. 发送响应给 Verifier
send_to_verifier(&response);
```

### Quote 验证

```rust
use vault_service::tee::AttestationService;

let service = AttestationService::new()
    .allow_simulation(true)  // 仅用于开发测试
    .with_max_quote_age(3600);  // Quote 有效期 1 小时

// 验证 Quote
let result = service.verify_quote(&quote, &challenge, &enclave_identity)?;
assert!(result.success);
```

### 安全通道

```rust
use vault_service::tee::{SecureChannel, AttestationResult};

// 从认证结果建立安全通道
let channel = SecureChannel::establish(
    &attestation_result,
    "secure_channel_1".to_string(),
    3600  // TTL: 1 小时
)?;

// 使用会话密钥进行加密通信
let session_key = channel.session_key();
```

## TypeScript SDK Sandbox 使用

### 安装

```bash
npm install @credbridge/sdk
# 或
yarn add @credbridge/sdk
```

### 快速开始

```typescript
import { CredBridgeSDK, OperationType } from '@credbridge/sdk';

const sdk = new CredBridgeSDK({
  baseUrl: 'https://api.credbridge.io',
  token: 'v4.local.your-paseto-token',
});

async function automateTask() {
  // 1. 创建会话
  const { sessionId } = await sdk.sandbox.createSession({
    serviceId: 'schwab',
    credentialId: 'cred-123',
    startUrl: 'https://www.schwab.com',
  });

  try {
    // 2. 导航到登录页面
    await sdk.sandbox.navigate(sessionId, 'https://www.schwab.com/login');

    // 3. 填写凭证并登录（凭证明文从不出 Enclave）
    await sdk.sandbox.fill(sessionId, '#username', '{{CREDENTIAL.username}}');
    await sdk.sandbox.fill(sessionId, '#password', '{{CREDENTIAL.password}}');
    await sdk.sandbox.click(sessionId, '#login-button');

    // 4. 等待页面加载
    await sdk.sandbox.waitForSelector(sessionId, '.portfolio-summary', {
      timeout: 30000,
    });

    // 5. 获取投资组合余额
    const balanceResult = await sdk.sandbox.getText(sessionId, '.total-balance');
    console.log('Balance:', balanceResult.result);

    // 6. 安全截图（经过 AI 审核）
    const screenshot = await sdk.sandbox.takeScreenshot(sessionId, {
      type: 'png',
      fullPage: true,
    });

  } finally {
    // 7. 关闭会话
    await sdk.sandbox.closeSession(sessionId);
  }
}
```

### WebSocket 实时连接

```typescript
import { CredBridgeSDK, SandboxEventType } from '@credbridge/sdk';

const sdk = new CredBridgeSDK({
  baseUrl: 'https://api.credbridge.io',
  token: 'v4.local.your-paseto-token',
});

async function realtimeAutomation() {
  const { sessionId, wsUrl } = await sdk.sandbox.createSession({
    serviceId: 'example',
    credentialId: 'cred-123',
    useWebSocket: true,  // 启用 WebSocket
  });

  // 建立 WebSocket 连接
  const ws = await sdk.sandbox.connectWebSocket(sessionId, wsUrl);

  // 监听实时事件
  ws.on(SandboxEventType.OPERATION_RESULT, (event) => {
    console.log('操作结果:', event.data);
  });

  ws.on(SandboxEventType.SCREENSHOT_READY, (event) => {
    console.log('截图完成:', event.data.screenshotId);
  });

  ws.on(SandboxEventType.REVIEW_REQUIRED, (event) => {
    console.log('需要人工审核:', event.data.reason);
  });

  ws.on(SandboxEventType.ERROR, (event) => {
    console.error('错误:', event.data.message);
  });

  // 发送操作命令
  await ws.sendOperation({
    type: OperationType.NAVIGATE,
    params: { url: 'https://example.com' },
  });

  // 等待操作完成
  const result = await ws.waitForOperation(operationId, 30000);
}
```

### 详细指南

- [Sandbox SDK 完整指南](docs/SDK_SANDBOX_GUIDE.md)
- [API 文档](docs/API.md)
- [MCP 集成指南](docs/MCP_INTEGRATION.md)

## 密封存储（Sealing）

密封存储允许将敏感数据加密持久化到磁盘，且只能在相同 Enclave 中解封。

### 密封策略

- **MRENCLAVE**: 仅当前版本 Enclave 可解封（严格模式）
- **MRSIGNER**: 同一签名者的不同版本 Enclave 可解封（兼容模式，推荐）

### 使用密封存储

```rust
use vault_service::tee::{SealingService, SealPolicy};

let service = SealingService::new();

// 密封数据
let plaintext = b"sensitive master key";
let aad = b"additional authenticated data";

let sealed = service.seal_data(
    plaintext,
    aad,
    SealPolicy::Mrsigner
).expect("Sealing failed");

// 解封数据
let decrypted = service.unseal_data(&sealed)
    .expect("Unsealing failed");

assert_eq!(decrypted, plaintext);
```

## 项目结构

```
credbridge/
├── src/                          # Rust 后端源码
│   ├── api/                      # API 路由和中间件
│   │   ├── routes/               # REST API 端点
│   │   ├── middleware/           # 认证、审计、限流中间件
│   │   └── websocket/            # WebSocket 实时连接
│   ├── tee/                      # TEE (可信执行环境)
│   │   ├── enclave.rs            # Enclave 生命周期管理
│   │   ├── keys.rs               # 密钥派生与管理
│   │   ├── attestation.rs        # SGX 远程认证
│   │   ├── dcap.rs               # DCAP Quote 生成/验证
│   │   ├── sealing.rs            # 密封存储
│   │   └── sandbox/              # TEE 安全执行沙箱
│   │       ├── session.rs        # 会话管理
│   │       ├── pool.rs           # 热实例池
│   │       ├── nsjail.rs         # nsjail 沙箱封装
│   │       ├── review/           # AI 审核引擎
│   │       │   ├── operation.rs  # 操作审核
│   │       │   └── injection.rs  # 注入检测
│   │       └── export/           # 安全导出通道
│   │           ├── screenshot.rs # 截图审核
│   │           └── signer.rs     # 导出签名
│   ├── crypto/                   # 加密功能
│   ├── vault/                    # 凭证保险库
│   ├── audit/                    # 审计日志
│   ├── services/                 # 业务服务
│   ├── token/                    # PASETO Token
│   ├── tenant/                   # 多租户管理
│   └── main.rs                   # 服务入口
├── frontend/                     # React 前端
│   └── src/
│       ├── features/             # 功能模块
│       ├── components/           # UI 组件
│       ├── hooks/                # React Hooks
│       └── lib/                  # 工具函数
├── sdk-typescript/               # TypeScript SDK
│   └── src/
│       ├── client.ts             # HTTP 客户端
│       ├── sandbox.ts            # Sandbox API
│       └── websocket.ts          # WebSocket 连接
├── sdk-rust/                     # Rust SDK
├── mcp-server/                   # MCP Server 实现
├── tests/                        # Rust 集成测试
├── e2e-test/                     # Playwright E2E 测试
├── migrations/                   # 数据库迁移
├── docker/                       # Docker 配置
└── docs/                         # 文档
    ├── API.md                    # API 文档
    ├── SDK_SANDBOX_GUIDE.md      # Sandbox SDK 指南
    ├── MCP_INTEGRATION.md        # MCP 集成指南
    ├── DEPLOYMENT.md             # 部署指南
    ├── USER_MANUAL.md            # 用户手册
    └── TEE_SANDBOX_DEV_PLAN.md   # 沙箱开发计划
```

## 文档索引

| 文档 | 描述 |
|------|------|
| [API.md](docs/API.md) | RESTful API 完整文档 |
| [openapi/sandbox.yaml](docs/openapi/sandbox.yaml) | OpenAPI 3.0 规范（Sandbox API） |
| [SDK_SANDBOX_GUIDE.md](docs/SDK_SANDBOX_GUIDE.md) | TypeScript Sandbox SDK 使用指南 |
| [MCP_INTEGRATION.md](docs/MCP_INTEGRATION.md) | Model Context Protocol 集成 |
| [DEPLOYMENT.md](docs/DEPLOYMENT.md) | 部署和配置指南 |
| [USER_MANUAL.md](docs/USER_MANUAL.md) | 终端用户手册 |
| [TEE_SANDBOX_DEV_PLAN.md](docs/TEE_SANDBOX_DEV_PLAN.md) | 沙箱详细开发计划 |
| [ACCEPTANCE_TEST_CASES.md](docs/ACCEPTANCE_TEST_CASES.md) | 验收测试用例 |
| [PHASE4_PERFORMANCE_REPORT.md](docs/PHASE4_PERFORMANCE_REPORT.md) | 性能测试报告 |
| [FINAL_GATE_CHECKLIST.md](docs/FINAL_GATE_CHECKLIST.md) | 最终验收清单 |

## API 参考

### Enclave

| 方法 | 说明 |
|------|------|
| `Enclave::new(config)` | 创建新的 Enclave 实例 |
| `initialize()` | 初始化 Enclave，派生 L1 Master Key |
| `shutdown()` | 关闭 Enclave，清理敏感数据 |
| `encrypt_credential(tenant_id, user_id, credential_id, plaintext)` | 加密凭证 |
| `decrypt_credential(tenant_id, user_id, credential_id, blob)` | 解密凭证 |
| `derive_user_vault_key(tenant_id, user_id)` | 派生用户保险库密钥 |
| `state()` | 获取当前 Enclave 状态 |
| `is_running()` | 检查 Enclave 是否运行中 |
| `stats()` | 获取统计信息 |
| `get_cache_stats()` | 获取缓存统计 |

### 远程认证 (AttestationService)

| 方法 | 说明 |
|------|------|
| `AttestationService::new()` | 创建认证服务 |
| `allow_mrenclave(measurement)` | 添加允许的 MRENCLAVE |
| `allow_mrsigner(measurement)` | 添加允许的 MRSIGNER |
| `with_max_quote_age(seconds)` | 设置 Quote 最大有效期 |
| `generate_quote(enclave, challenge)` | 生成 SGX Quote |
| `verify_quote(quote, challenge, identity)` | 验证 Quote |

### 挑战-响应协议 (ChallengeProtocol)

| 方法 | 说明 |
|------|------|
| `ChallengeProtocol::new(service)` | 创建协议处理器 |
| `generate_challenge(enclave_id, metadata)` | 生成挑战 |
| `verify_response(response, identity)` | 验证响应 |
| `cancel_challenge(challenge_id)` | 取消挑战 |
| `cleanup_expired()` | 清理过期挑战 |

### Enclave 状态

```rust
pub enum EnclaveState {
    Uninitialized,   // 未初始化
    Initializing,    // 正在初始化
    Running,         // 已运行
    ShuttingDown,    // 正在关闭
    Shutdown,        // 已关闭
    Error,           // 错误状态
}
```

### 错误处理

```rust
use vault_service::tee::EnclaveError;

match enclave.encrypt_credential(...) {
    Ok(blob) => { /* 处理加密后的数据 */ }
    Err(EnclaveError::NotRunning) => { /* Enclave 未运行 */ }
    Err(EnclaveError::KeyDerivationFailed(msg)) => { /* 密钥派生失败 */ }
    Err(EnclaveError::EncryptionFailed(msg)) => { /* 加密失败 */ }
    Err(e) => { /* 其他错误 */ }
}
```

### Sandbox API

| 方法 | 说明 |
|------|------|
| `SandboxSession::create(config)` | 创建新会话 |
| `start()` | 启动会话（分配沙箱实例） |
| `pause()` | 暂停会话 |
| `resume()` | 恢复会话 |
| `close()` | 关闭会话并释放资源 |
| `execute_operation(op)` | 执行浏览器操作 |
| `take_screenshot(options)` | 安全截图（AI 审核） |
| `export_data(data)` | 安全导出数据 |
| `get_status()` | 获取会话状态 |

### Sandbox 操作类型

```rust
pub enum OperationType {
    Navigate { url: String },           // 导航到 URL
    Click { selector: String },         // 点击元素
    Fill { selector: String, value: String }, // 填充表单
    GetText { selector: String },       // 获取元素文本
    GetAttribute { selector: String, name: String }, // 获取属性
    WaitForSelector { selector: String, timeout: u64 }, // 等待元素
    Scroll { x: i32, y: i32 },          // 滚动页面
    Screenshot { full_page: bool },     // 截图
}
```

### Sandbox 会话状态

```rust
pub enum SessionStatus {
    Creating,    // 正在创建
    Running,     // 运行中
    Paused,      // 已暂停
    Closing,     // 正在关闭
    Closed,      // 已关闭
    Error,       // 错误状态
}
```

## 运行测试

```bash
# 运行所有测试
cargo test

# 运行 TEE 模块测试
cargo test -- tee::

# 运行 Vault 模块测试
cargo test -- vault::

# 运行加密模块测试
cargo test -- crypto::

# 运行集成测试
cargo test --test vault_models_tests

# 显示测试输出
cargo test -- --nocapture
```

## 内存安全与密钥清理 (Story 1.4)

CredBridge 实现了多层内存安全保护机制，确保密钥材料不会残留在内存中。

### ZeroizeOnDrop 自动清理

所有密钥结构体都实现了 `ZeroizeOnDrop` trait，确保在 drop 时自动将密钥材料覆写为零。

```rust
use vault_service::crypto::keys::CredentialKey;
use zeroize::ZeroizeOnDrop;

{
    let key = CredentialKey::new(
        key_material,
        "tenant_123".to_string(),
        "user_hash".to_string(),
        "cred_789".to_string(),
        KeyPurpose::CredentialEncryption,
        timestamp,
    );
    // 使用密钥加密/解密...
} // 离开作用域时，key_material 自动被 zeroize
```

### TTL 缓存自动过期清理

用户密钥缓存（L2）具有 5 分钟 TTL，过期后自动清理：

```rust
use vault_service::tee::{UserKeyCache, CachedKeyEntry, KeyType};

// 创建缓存（5 分钟 TTL）
let mut cache = UserKeyCache::new(300);

// 添加条目
let entry = CachedKeyEntry::new(
    handle,
    "tenant_1".to_string(),
    "user_hash".to_string(),
    key_material,
    KeyType::UserVault,
);
cache.insert("tenant_1", "user_hash", entry);

// 过期后自动清理
// 调用 get() 时检测过期并触发 zeroize
cache.get("tenant_1", "user_hash"); // 返回 None 如果已过期
```

### 密钥清理调度器

后台线程定期清理过期密钥：

```rust
use vault_service::tee::{CleanupScheduler, CleanupConfig};
use std::time::Duration;

// 配置清理调度器
let config = CleanupConfig {
    cleanup_interval: Duration::from_secs(60), // 每分钟清理一次
    deep_cleanup_on_drop: true,
    verify_cleanup: false,
    ..Default::default()
};

// 启动调度器
let mut scheduler = CleanupScheduler::new(config);
scheduler.start(cache.clone());

// 停止调度器
scheduler.stop();
```

### Enclave 重启后的密钥恢复

当 Enclave 重启时，从密封存储恢复 L1 主密钥，并验证 MRSIGNER 兼容性：

```rust
use vault_service::tee::{KeyManager, SealPolicy};

// 创建密钥管理器
let manager = KeyManager::new(
    "/path/to/sealed/storage".to_string(),
    mrsigner,
    mrenclave,
);

// 密封主密钥（Enclave 关闭前）
manager.seal_master_key(&master_key_material, SealPolicy::Mrsigner)?;

// Enclave 重启后恢复密钥
let (recovered_key, metadata) = manager.restore_master_key()?;

// 验证 MRSIGNER 兼容性（自动执行）
// - Mrsigner 模式：验证签名者是否相同
// - Mrenclave 模式：验证 Enclave 身份完全相同
```

### 手动密钥清理

```rust
use vault_service::tee::KeyCleaner;

// 清理字节数组
let mut sensitive_data = vec![0x42u8; 32];
KeyCleaner::clear_bytes(&mut sensitive_data);

// 深度清理（多次覆写）
let mut key_material = [0x42u8; 32];
KeyCleaner::deep_clear(&mut key_material);

// 清理并验证
let mut data = vec![0x42u8; 16];
assert!(KeyCleaner::clear_and_verify(&mut data));
```

### 受保护内存区域

```rust
use vault_service::tee::ProtectedMemory;

// 创建受保护内存
let mut memory = ProtectedMemory::new(64, "master_key_buffer");

// 使用内存
memory.as_mut_slice().copy_from_slice(&key_data);

// 自动清理（drop 时）
drop(memory);

// 或手动清理
memory.secure_clear();
```

## 安全最佳实践

### 1. 密钥生命周期

- L1 Master Key 在 Enclave 初始化时派生，永不出 Enclave 边界
- L2 User Key 缓存 5 分钟，定期清理
- L3 Credential Key 即时派生，用完即焚

### 2. 内存安全

- 使用 `zeroize` crate 自动清理敏感数据
- 密钥结构体实现 `ZeroizeOnDrop` trait
- 所有敏感内存区域在 drop 时自动清零
- 缓存项 TTL 过期后自动移除并 zeroize

### 3. Enclave 重启安全

- L1 主密钥密封存储，绑定到 MRSIGNER/MRENCLAVE
- 重启后验证兼容性，防止未授权 Enclave 访问
- 使用 MRSIGNER 模式允许同一签名者的版本更新

### 4. 防篡改

- AES-256-GCM 提供认证加密
- 篡改密文将导致 `AuthenticationFailed` 错误

### 5. 密封存储

- 敏感数据密封后存储，绑定到 Enclave 测量值
- 支持 MRENCLAVE 和 MRSIGNER 两种策略

### 6. 远程认证安全

- **防重放攻击**: 每次挑战随机生成，仅使用一次
- **身份绑定**: Quote 中的 REPORT_DATA 绑定挑战和 Enclave 身份
- **新鲜性保证**: 挑战严格时间限制（默认 5 分钟）
- **不可否认**: ECDSA P-256 签名提供强不可否认性
- **测量值白名单**: 只允许特定 Enclave 测量值通过认证

## 开发模式 vs 生产模式

### 开发模式（模拟）

```rust
let config = EnclaveConfig {
    debug_mode: true,  // 启用调试模式
    ..Default::default()
};
```

在开发模式下：
- 使用模拟的 Sealing Key
- 输出调试信息
- 不验证硬件 TEE

### 生产模式（SGX 硬件）

```rust
let config = EnclaveConfig {
    debug_mode: false,  // 禁用调试模式
    seal_policy: SealPolicy::Mrsigner,
    ..Default::default()
};
```

在生产模式下：
- 从 Intel SGX 硬件获取 Sealing Key
- 启用远程认证
- 所有操作在硬件安全边界内执行

## 部署到 SGX 硬件

### 前提条件

1. SGX-enabled CPU（Intel Core 6代+ 或 Xeon E3/E-系列）
2. SGX 驱动和 SDK 安装
3. `/dev/sgx_enclave` 设备可访问

### 检测 SGX 支持

```bash
# 检查 CPU 支持
cat /proc/cpuinfo | grep sgx

# 检查设备节点
ls -la /dev/sgx*
```

### 构建 SGX 版本

```bash
# 启用 SGX 特性构建
cargo build --features sgx

# 运行测试
cargo test --features sgx
```

## 性能指标

### 核心加密性能

| 操作 | 延迟（P99） |
|------|------------|
| Enclave 初始化 | ~50ms |
| 密钥派生（L1→L2） | ~5ms |
| 密钥派生（L2→L3） | ~2ms |
| AES-256-GCM 加密 | ~0.1ms |
| 凭证解密 | ~10ms |

### TEE Sandbox 性能

| 指标 | 本地开发 | TEE 硬件 | 测试配置 |
|------|---------|---------|---------|
| **热实例启动** | ≤ 100ms | ≤ 50ms | P99: 85ms |
| **冷启动创建** | ≤ 3s | ≤ 2s | P99: 2.8s |
| **浏览器操作延迟** | ≤ 2s | ≤ 1.5s | P95: 1.8s |
| **AI 审核延迟** | ≤ 500ms | ≤ 500ms | Mock: 50ms |
| **截图导出** | ≤ 3s | ≤ 2s | P95: 2.5s |
| **并发会话数** | 50 | 100 | 实测: 80+ |
| **资源占用（每会话）** | 150MB | 100MB | 实测: 120MB |

### 详细性能报告

- [Phase 4 性能测试报告](docs/PHASE4_PERFORMANCE_REPORT.md) - 包含完整测试方法、结果和分析

## 凭证 Vault 存储 (Story 2.1)

CredBridge Vault 模块实现安全凭证存储，支持 UUID v7 时间排序 ID、多租户隔离、AES-256-GCM 加密。

### 核心特性

- **UUID v7 凭证 ID**: 时间排序，更好的数据库索引性能
- **多租户隔离**: Schema-per-Tenant + 行级安全（RLS）
- **用户 ID 哈希存储**: 保护用户隐私，支持验证
- **加密载荷**: 符合规范的 AES-256-GCM 加密数据
- **软删除与物理删除**: 支持审计和合规清理

### 快速开始

#### 创建 Vault

```rust
use vault_service::vault::CredentialVault;

// 创建内存存储的 Vault（开发/测试）
let vault = CredentialVault::new_in_memory();

// 或使用自定义存储后端
let vault = CredentialVault::with_backend(Box::new(my_backend));
```

#### 创建凭证

```rust
use vault_service::vault::{
    CredentialVault, TenantId, UserId, ServiceId,
    models::{CreateCredentialRequest, EncryptedPayload}
};
use vault_service::models::CredentialType;

let vault = CredentialVault::new_in_memory();

// 创建凭证请求
let request = CreateCredentialRequest {
    tenant_id: TenantId::new("tenant_123"),
    user_id: UserId::new("user_456"),
    service_id: ServiceId::new("schwab"),
    credential_type: CredentialType::UsernamePassword,
    expires_at: None,
};

// 加密载荷（从 TEE 加密获取）
let encrypted_payload = EncryptedPayload::new(
    2,                          // version
    "AES-256-GCM",             // algorithm
    "HKDF-SHA-256",            // kdf
    nonce_bytes,               // 12 bytes
    auth_tag_bytes,            // 16 bytes
    ciphertext_bytes,          // encrypted data
);

// 创建凭证
let entry = vault.create_credential(request, encrypted_payload)
    .expect("Failed to create credential");

println!("Created credential: {}", entry.credential_id.as_str());
```

#### 查询凭证元数据

```rust
// 查询凭证元数据（不含加密载荷）
let metadata = vault.get_credential_metadata(
    &entry.credential_id,
    &TenantId::new("tenant_123"),
    &UserId::new("user_456"),
).expect("Query failed");

if let Some(meta) = metadata {
    println!("Service: {}", meta.service_id);
    println!("Type: {:?}", meta.credential_type);
    println!("Created: {}", meta.created_at);
}
```

#### 列出用户凭证

```rust
use vault_service::vault::models::CredentialFilter;

let result = vault.list_credentials(
    &TenantId::new("tenant_123"),
    &UserId::new("user_456"),
    CredentialFilter::default(),
).expect("Query failed");

println!("Total credentials: {}", result.total);
for cred in result.credentials {
    println!("- {}: {}", cred.credential_id, cred.service_id);
}
```

#### 按条件过滤

```rust
use vault_service::vault::models::CredentialFilter;

// 仅查询特定服务的凭证
let filter = CredentialFilter {
    service_id: Some(ServiceId::new("schwab")),
    credential_type: Some(CredentialType::UsernamePassword),
    include_deleted: false,
    only_valid: true,
};

let result = vault.list_credentials(
    &TenantId::new("tenant_123"),
    &UserId::new("user_456"),
    filter,
).expect("Query failed");
```

#### 删除凭证

```rust
// 软删除（保留审计记录）
let deleted = vault.delete_credential(
    &entry.credential_id,
    &TenantId::new("tenant_123"),
    &UserId::new("user_456"),
).expect("Delete failed");

// 物理删除（合规清理）
let purged = vault.purge_credential(
    &entry.credential_id,
    &TenantId::new("tenant_123"),
    &UserId::new("user_456"),
).expect("Purge failed");
```

### 数据模型

#### VaultEntry 结构

```rust
pub struct VaultEntry {
    pub credential_id: CredentialId,       // UUID v7
    pub tenant_id: TenantId,               // 多租户隔离
    pub user_id: UserId,                   // 哈希存储
    pub service_id: ServiceId,             // 服务标识
    pub credential_type: CredentialType,   // 凭证类型
    pub created_at: u64,                   // 创建时间戳
    pub updated_at: u64,                   // 更新时间戳
    pub expires_at: Option<u64>,           // 过期时间
    pub encrypted_payload: EncryptedPayload, // 加密数据
    pub is_deleted: bool,                  // 软删除标记
}
```

#### 加密载荷格式

```rust
pub struct EncryptedPayload {
    pub version: u8,              // 固定为 2
    pub algorithm: String,        // "AES-256-GCM"
    pub kdf: String,              // "HKDF-SHA-256"
    pub nonce: String,            // Base64, 12 bytes
    pub auth_tag: String,         // Base64, 16 bytes
    pub ciphertext: String,       // Base64, 加密数据
}
```

### 多租户隔离

Vault 实施严格的多租户隔离，确保租户间数据完全隔离：

```rust
// 租户 A 创建凭证
let entry_a = vault.create_credential(
    CreateCredentialRequest {
        tenant_id: TenantId::new("tenant_a"),
        user_id: UserId::new("user_a"),
        ...
    },
    encrypted_payload,
)?;

// 租户 B 无法访问租户 A 的凭证
let result = vault.get_credential_metadata(
    &entry_a.credential_id,
    &TenantId::new("tenant_b"),  // 错误的租户
    &UserId::new("user_a"),
);
assert!(result.is_err());  // TenantIsolationViolation
```

### 存储后端

#### 内存存储（开发/测试）

```rust
use vault_service::vault::InMemoryStorage;

let storage = InMemoryStorage::new();
let vault = CredentialVault::with_backend(Box::new(storage));
```

#### 自定义存储后端

实现 `StorageBackend` trait 以支持 PostgreSQL、HashiCorp Vault 等：

```rust
use vault_service::vault::{StorageBackend, VaultEntry, VaultError};

pub struct PostgresStorage {
    // ...
}

impl StorageBackend for PostgresStorage {
    fn store(&self, entry: &VaultEntry) -> Result<(), VaultError> {
        // 实现存储逻辑
    }

    fn get(&self, credential_id: &CredentialId) -> Result<Option<VaultEntry>, VaultError> {
        // 实现查询逻辑
    }

    // ... 其他方法
}
```

### API 参考

| 方法 | 说明 |
|------|------|
| `CredentialVault::new_in_memory()` | 创建内存存储的 Vault |
| `create_credential(request, payload)` | 创建新凭证 |
| `get_credential_metadata(id, tenant, user)` | 获取凭证元数据 |
| `get_credential(id, tenant, user)` | 获取完整凭证（含加密载荷） |
| `list_credentials(tenant, user, filter)` | 列出用户凭证 |
| `delete_credential(id, tenant, user)` | 软删除凭证 |
| `purge_credential(id, tenant, user)` | 物理删除凭证 |
| `credential_exists(id)` | 检查凭证是否存在 |

## 凭证 CRUD API (Story 2.2)

CredBridge 提供 RESTful API 用于凭证管理，所有端点都需要 PASETO v4.local Token 认证。

### API 端点

| 方法 | 端点 | 描述 | 所需 Scope |
|------|------|------|-----------|
| POST | `/api/v1/credentials` | 创建凭证 | `credential:write` |
| GET | `/api/v1/credentials` | 获取凭证列表 | `credential:read` |
| GET | `/api/v1/credentials/:id` | 获取凭证详情 | `credential:read` |
| POST | `/api/v1/credentials/:id/decrypt` | 解密凭证 | `credential:decrypt` |
| DELETE | `/api/v1/credentials/:id` | 删除凭证 | `credential:write` 或 `admin` |

### 认证

所有 API 请求必须在 `Authorization` 头中包含 Bearer Token：

```http
Authorization: Bearer <paseto_v4_local_token>
```

### Token Scope 权限系统 (EP3-Story3.3)

CredBridge 实现基于 RBAC + 资源级权限的细粒度 Scope 系统，支持受限 Token（credential_ids 白名单）。

#### 支持的 Scope

| Scope | 权限 | 说明 |
|-------|------|------|
| `credential:read` | 读取凭证元数据 | 可访问 ReadCredential 操作 |
| `credential:decrypt` | 解密密文获取明文 | 可访问 DecryptCredential 操作（隐式包含 read） |
| `credential:write` | 创建/更新凭证 | 可访问 CreateCredential 和 UpdateCredential 操作 |
| `credential:delete` | 删除凭证 | 可访问 DeleteCredential 操作 |
| `token:manage` | 管理 Token | 可访问 ManageToken、RevokeToken、RefreshToken 操作 |
| `audit:read` | 读取审计日志 | 可访问 ReadAudit 操作 |
| `admin` | 所有管理权限 | 可访问所有操作 |

#### Scope 权限检查

```rust
use vault_service::token::scope::{Scope, Operation};

// 检查 Scope 是否允许操作
let read_scope = Scope::CredentialRead;
assert!(read_scope.can_access(&Operation::ReadCredential));
assert!(!read_scope.can_access(&Operation::DecryptCredential));

// Admin 拥有所有权限
let admin = Scope::Admin;
assert!(admin.can_access(&Operation::ReadCredential));
assert!(admin.can_access(&Operation::DeleteCredential));
```

#### 受限 Token（资源级权限）

受限 Token 只能访问白名单中指定的凭证：

```rust
use vault_service::token::scope::{ScopeSet, PermissionChecker, RestrictedTokenContext};
use vault_service::token::permission::PermissionEngine;

// 创建受限 Token（只能访问 cred_123 和 cred_456）
let engine = PermissionEngine::with_restricted_credentials(
    "credential:read",
    vec!["cred_123".to_string(), "cred_456".to_string()],
).expect("invalid scope");

// 验证访问权限
let decision = engine.check_credential_access("cred_123");
assert!(decision.is_allowed());

let decision = engine.check_credential_access("cred_999");
assert!(decision.is_denied()); // AccessDenied 错误
```

#### PermissionEngine 权限引擎

```rust
use vault_service::token::permission::{
    PermissionEngine, AccessRequest, AccessContext, Operation, ResourceType
};

// 创建权限引擎
let engine = PermissionEngine::new("credential:read credential:write")
    .expect("invalid scope");

// 检查操作权限
assert!(engine.can_execute(&Operation::ReadCredential));
assert!(engine.can_execute(&Operation::CreateCredential));

// 构建访问请求
let request = AccessRequest::credential(Operation::ReadCredential, "cred_123")
    .with_context(
        AccessContext::new()
            .with_ip("192.168.1.1")
            .with_risk_level(1)
    );

// 检查访问权限
let decision = engine.check_access(&request);
match decision {
    AccessDecision::Allow => println!("允许访问"),
    AccessDecision::Deny(reason) => println!("拒绝访问: {}", reason),
    AccessDecision::RequireApproval(reason) => println!("需要审批: {}", reason),
}
```

#### 批量权限检查

```rust
use vault_service::token::permission::BatchPermissionChecker;

// 创建批量检查器
let mut checker = BatchPermissionChecker::new(engine);

// 批量检查多个凭证
let credential_ids = vec!["cred_1".to_string(), "cred_2".to_string()];
let results = checker.check_all_credentials(
    Operation::ReadCredential,
    &credential_ids
);

// 分析结果
println!("允许: {}", checker.allowed_count());
println!("拒绝: {}", checker.denied_count());
```

#### Scope 权限继承关系

```
credential:decrypt ──┬──→ ReadCredential
                     └──→ DecryptCredential

credential:write ────┬──→ CreateCredential
                     └──→ UpdateCredential

admin ───────────────→ 所有操作
```

### 创建凭证

```bash
curl -X POST http://localhost:8080/api/v1/credentials \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "service_id": "schwab",
    "credential_type": "username_password",
    "plaintext_data": {
      "username": "user@example.com",
      "password": "secret_password"
    },
    "expires_at": 1893456000
  }'
```

**响应（201 Created）**:
```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "created_at": "1709990400",
  "expires_at": "1893456000"
}
```

### 获取凭证列表

```bash
curl -X GET http://localhost:8080/api/v1/credentials \
  -H "Authorization: Bearer <token>"
```

**响应（200 OK）**:
```json
{
  "credentials": [
    {
      "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
      "credential_type": "username_password",
      "user_id_hash": "aBcDeFg...",
      "service_id": "schwab",
      "tenant_id": "tenant_123",
      "created_at": "1709990400Z",
      "expires_at": "1893456000Z",
      "is_deleted": false
    }
  ],
  "total": 1
}
```

### 获取凭证详情

```bash
curl -X GET http://localhost:8080/api/v1/credentials/{credential_id} \
  -H "Authorization: Bearer <token>"
```

### 解密凭证

```bash
curl -X POST http://localhost:8080/api/v1/credentials/{credential_id}/decrypt \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "reason": "用户登录操作"
  }'
```

**响应（200 OK）**:
```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "plaintext_data": {
    "username": "user@example.com",
    "password": "secret_password"
  }
}
```

### 删除凭证

```bash
curl -X DELETE http://localhost:8080/api/v1/credentials/{credential_id} \
  -H "Authorization: Bearer <token>"
```

**响应（200 OK）**:
```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "deleted": true
}
```

### 错误响应

| 状态码 | 错误 | 描述 |
|--------|------|------|
| 400 | `invalid_request` | 请求参数无效 |
| 401 | `missing_token` | 缺少 Authorization 头 |
| 401 | `invalid_token` | Token 无效或过期 |
| 401 | `revoked_token` | Token 已被使用（jti 重复） |
| 403 | `insufficient_scope` | 缺少必需的 Scope |
| 404 | `not_found` | 凭证不存在 |
| 500 | `internal_error` | 服务器内部错误 |

### 安全特性

- **TEE 内加密**: 所有加密操作在 TEE 安全边界内完成
- **Token 15 分钟有效期**: 短期 Token 降低泄露风险
- **jti 单次使用**: 防止 Token 重放攻击
- **审计日志**: 所有凭证操作记录审计日志
- **Scope 细粒度控制**: 最小权限原则

## DCAP 远程认证 API (EP8-Story8.1)

CredBridge 提供 DCAP (Data Center Attestation Primitives) 远程认证 API，支持自动 Quote 生成、Intel PCS 注册和远程验证。

### API 端点

| 方法 | 端点 | 描述 | 所需 Scope |
|------|------|------|-----------|
| GET | `/api/v1/attestation/quote` | 获取当前 DCAP Quote | `health:read` |
| POST | `/api/v1/attestation/verify` | 验证 Quote | `health:read` |
| GET | `/api/v1/attestation/report` | 获取认证报告 | `health:read` |
| POST | `/api/v1/attestation/challenge` | 创建认证挑战 | `health:read` |
| POST | `/api/v1/attestation/refresh` | 刷新 Quote | `health:read` |
| GET | `/api/v1/attestation/health` | 健康检查 | `health:read` |

### 快速开始

#### 创建 DCAP 服务

```rust
use vault_service::tee::{
    dcap::{DcapConfig, DcapService, INTEL_PCS_BASE_URL_PROD},
    enclave::{Enclave, EnclaveConfig},
};

// 创建 DCAP 配置
let dcap_config = DcapConfig {
    pcs_base_url: INTEL_PCS_BASE_URL_PROD.to_string(),
    api_key: Some("your_intel_pcs_api_key".to_string()),
    quote_max_age_seconds: 3600,
    verify_certificate_chain: true,
    allowed_mrenclaves: vec![
        hex::decode("expected_mrenclave_hex").unwrap().try_into().unwrap(),
    ],
    simulation_mode: false,  // 生产环境设为 false
    ..Default::default()
};

// 创建 DCAP 服务
let dcap_service = DcapService::new(dcap_config)?;

// 创建并初始化 Enclave
let mut enclave = Enclave::new(EnclaveConfig::default());
enclave.initialize()?;

// 初始化 DCAP（自动生成 Quote 并向 PCS 注册）
let quote = dcap_service.initialize(&enclave)?;
println!("MRENCLAVE: {}", hex::encode(quote.report_body.mrenclave));
println!("MRSIGNER: {}", hex::encode(quote.report_body.mrsigner));
```

#### 获取 Quote

```bash
curl http://localhost:8080/api/v1/attestation/quote
```

**响应**:
```json
{
  "success": true,
  "data": {
    "version": 3,
    "sign_type": 2,
    "mrenclave": "0123456789abcdef...",
    "mrsigner": "fedcba9876543210...",
    "timestamp": 1640000000,
    "quote_b64": "BASE64_ENCODED_QUOTE"
  },
  "error": null
}
```

#### 验证 Quote

```bash
curl -X POST http://localhost:8080/api/v1/attestation/verify \
  -H "Content-Type: application/json" \
  -d '{
    "quote_b64": "BASE64_ENCODED_QUOTE",
    "nonce": "OPTIONAL_NONCE_BASE64"
  }'
```

**响应**:
```json
{
  "success": true,
  "valid": true,
  "mrenclave": "0123456789abcdef...",
  "mrsigner": "fedcba9876543210...",
  "timestamp": 1640000000,
  "error": null
}
```

#### 获取认证报告

```bash
curl http://localhost:8080/api/v1/attestation/report
```

**响应**:
```json
{
  "success": true,
  "data": {
    "version": "1.0.0",
    "mrenclave": "0123456789abcdef...",
    "mrsigner": "fedcba9876543210...",
    "security_version": 1,
    "product_id": 1,
    "attributes": "0500000000000000...",
    "timestamp": 1640000000,
    "quote_b64": "BASE64_ENCODED_QUOTE",
    "certificate_info": {
      "subject": "CN=Intel SGX PCK Certificate",
      "issuer": "CN=Intel SGX PCK Platform CA",
      "not_before": "2024-01-01T00:00:00Z",
      "not_after": "2025-01-01T00:00:00Z",
      "fingerprint": "abcd1234..."
    }
  },
  "error": null
}
```

### Quote 验证流程

```rust
use vault_service::tee::{
    dcap::DcapService,
    quote::{QuoteParser, QuoteValidator},
};

// 从远程获取 Quote
let quote_b64 = fetch_quote_from_server().await?;
let quote_bytes = base64::decode(&quote_b64)?;

// 解析 Quote
let quote = QuoteParser::parse(&quote_bytes)?;

// 验证 Quote 格式
let validator = QuoteValidator::new();
validator.validate(&quote)?;

// 使用 DCAP 服务验证
let dcap_service = DcapService::new(config)?;
let report = dcap_service.verify_attestation(&quote_bytes, None)?;

if report.result.success {
    println!("Enclave verified! MRENCLAVE: {}", report.mrenclave_hex);
}
```

### 测量值白名单

```rust
use vault_service::tee::dcap::DcapService;

// 创建带白名单的 DCAP 服务
let mut service = DcapService::new(config)?;

// 添加允许的 MRENCLAVE
service.allow_mrenclave(expected_mrenclave);

// 添加允许的 MRSIGNER
service.allow_mrsigner(expected_mrsigner);

// 验证时自动检查白名单
let result = service.verify_attestation(&quote_bytes, None);
```

### 模拟模式（开发测试）

```rust
use vault_service::tee::dcap::DcapConfig;

// 创建模拟模式的 DCAP 服务
let config = DcapConfig {
    simulation_mode: true,
    ..Default::default()
};
let service = DcapService::new(config)?;
```

在模拟模式下：
- 跳过 Intel PCS 注册
- 使用模拟的证书链
- 适用于没有 SGX 硬件的开发环境

### DCAP 配置

```bash
# Intel PCS URL
export INTEL_PCS_URL=https://api.trustedservices.intel.com/sgx/certification/v4/

# Intel PCS API Key
export INTEL_PCS_API_KEY=your_api_key_here

# Quote 最大有效期（秒）
export DCAP_QUOTE_MAX_AGE=3600

# 是否验证证书链
export DCAP_VERIFY_CERT_CHAIN=true
```

详细 DCAP 安装配置指南请参见 [DCAP_SETUP.md](DCAP_SETUP.md)。

## 故障排除

### Enclave 初始化失败

```
Error: KeyInitializationFailed("Sealing key unavailable")
```

**解决方案**:
- 检查 SGX 驱动是否正确安装
- 确认 `/dev/sgx_enclave` 存在且有访问权限
- 使用模拟模式进行开发和测试

### 解密失败

```
Error: AuthenticationFailed
```

**可能原因**:
- 密文被篡改
- 使用了错误的密钥派生路径
- Enclave 版本不匹配（密封数据）

### 缓存过期

```
Key derivation taking longer than expected
```

**解决方案**:
- 增加 `user_key_ttl` 配置
- 调用 `cleanup_expired_cache()` 清理过期条目

## 贡献指南

1. Fork 项目仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add some amazing feature'`)
4. 推送分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

## 审计日志系统 (EP4-Story4.1)

CredBridge 实现了不可篡改的审计日志系统，记录所有凭证访问和敏感操作。

### 核心特性

- **不可篡改**: 使用 Merkle Tree + Ed25519 数字签名确保日志完整性
- **PII 脱敏**: 自动敏感数据脱敏（SSN、密码、API Key 等）
- **UUID v7**: 支持时间排序的事件 ID
- **链式结构**: 每个条目包含前一个条目的哈希，形成不可篡改链

### 审计事件结构

```rust
pub struct AuditEntry {
    pub id: String,                    // UUID v7（时间排序）
    pub user_id_hash: String,          // SHA-256 哈希的用户 ID
    pub timestamp: u64,                // UTC 时间戳（毫秒）
    pub session_id: String,            // 会话标识
    pub service: String,               // 服务名称
    pub action: AuditAction,           // 操作类型
    pub risk_tier: RiskTier,           // 风险等级
    pub outcome: Outcome,              // 操作结果
    pub tee_mrenclave: String,         // TEE MRENCLAVE 测量值
    pub action_token_jti: String,      // Action Token JTI
    pub params: Option<Vec<(String, RedactedParam)>>,  // 脱敏参数
}
```

### 操作类型

| 操作 | 风险等级 | 说明 |
|------|----------|------|
| `CredentialDecrypt` | High | 凭证解密 |
| `CredentialAccess` | Medium | 凭证访问（元数据读取）|
| `CredentialCreate` | Medium | 凭证创建 |
| `CredentialDelete` | High | 凭证删除 |
| `TokenIssue` | Medium | Token 签发 |
| `TokenRevoke` | Medium | Token 撤销 |
| `AuditQuery` | High | 审计日志查询 |
| `KeyRotation` | Critical | 密钥轮换 |
| `AdminLogin` | Critical | 管理员登录 |

### 快速开始

```rust
use vault_service::audit::{
    AuditEntry, AuditAction, Outcome, PiiRedactor,
    create_memory_storage, hash_user_id
};

// 创建审计存储
let storage = create_memory_storage()?;

// 创建审计条目
let entry = AuditEntry::new(
    hash_user_id("user_123"),           // 用户 ID 哈希
    "session_xyz789",                    // 会话 ID
    "vault-service",                     // 服务标识
    AuditAction::CredentialDecrypt,      // 操作类型
    Outcome::Success,                    // 操作结果
    "mrenclave_measurement",             // TEE MRENCLAVE
    "action_token_jti",                  // Action Token JTI
)
.with_param("credential_id", PiiRedactor::plain("cred_123"))
.with_param("ssn", PiiRedactor::redact_ssn("123-45-6789"));

// 记录审计事件
let signed_entry = storage.record(entry)?;

// 验证链完整性
assert!(storage.verify()?);
```

### PII 脱敏

```rust
use vault_service::audit::PiiRedactor;

// 自动根据参数名脱敏
let ssn = PiiRedactor::auto_redact("user_ssn", "123-45-6789");
assert_eq!(ssn.to_string(), "[SSN_REDACTED]");

// 显式脱敏
let password = PiiRedactor::redact_password("secret123");
assert_eq!(password.to_string(), "[PASSWORD_REDACTED]");

// 不脱敏
let normal = PiiRedactor::plain("visible_value");
assert_eq!(normal.to_string(), "visible_value");
```

### 审计报告

```rust
use vault_service::audit::AuditRecorder;

let recorder = AuditRecorder::new(10000)?;

// 生成审计报告
let report = recorder.generate_report(
    Some(start_time),  // 开始时间
    Some(end_time)     // 结束时间
)?;

println!("Total entries: {}", report.total_entries);
println!("Success: {}", report.success_count);
println!("Failure: {}", report.failure_count);
println!("Merkle Root: {}", report.merkle_root);
```

### immudb 持久化存储 (EP4-Story4.2)

CredBridge 支持使用 immudb 作为审计日志的持久化存储后端。immudb 是一个轻量级的不可篡改数据库，基于 Merkle Tree 提供密码学完整性保证。

#### 特性

- **不可篡改**: 所有数据写入后无法修改
- **Merkle Tree**: 密码学验证数据完整性
- **状态哈希**: 每次写入计算新的全局状态哈希
- **验证证明**: 支持包含证明和一致性证明

#### 配置环境变量

```bash
export IMMUDB_HOST=localhost
export IMMUDB_PORT=3322
export IMMUDB_DATABASE=credbridge_audit
export IMMUDB_USERNAME=immudb
export IMMUDB_PASSWORD=your_secure_password
export IMMUDB_USE_TLS=false
```

#### 使用 immudb 存储

```rust
use vault_service::audit::{ImmuDbAuditStore, ImmuDbStoreConfig, ImmuDbConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从环境变量创建存储
    let store = ImmuDbAuditStore::from_env(
        signer_fingerprint,
        public_key,
    ).await?;

    // 存储审计条目
    let stored = store.store(&signed_entry).await?;
    println!("Stored with tx_id: {}", stored.transaction_id);

    // 验证存储完整性
    let is_valid = store.verify().await?;
    assert!(is_valid);

    // 验证单个条目
    let result = store.verify_entry(0).await?;
    assert!(result.verified);

    // 生成审计报告
    let report = store.generate_report(None, None).await?;
    println!("Total entries: {}", report.total_entries);
    println!("State hash: {}", report.state_hash);

    Ok(())
}
```

#### 安装 immudb

```bash
# Docker 安装
docker run -d --name immudb -p 3322:3322 codenotary/immudb:latest

# 或使用 Homebrew (macOS)
brew tap codenotary/tap
brew install immudb
immudb
```

详细配置指南请参见 [IMMUDB_SETUP.md](IMMUDB_SETUP.md)。

## 审计日志查询 API (EP4-Story4.3)

CredBridge 提供 RESTful API 用于审计日志查询和管理，支持分页、过滤、导出和完整性验证。

### API 端点

| 方法 | 端点 | 描述 | 所需 Scope |
|------|------|------|-----------|
| GET | `/api/v1/audit/logs` | 查询审计日志列表 | `audit:read` 或 `admin` |
| GET | `/api/v1/audit/logs/:id` | 获取审计日志详情 | `audit:read` 或 `admin` |
| POST | `/api/v1/audit/export` | 导出审计日志 | `audit:read` 或 `admin` |
| POST | `/api/v1/audit/verify` | 验证审计条目完整性 | `audit:read` 或 `admin` |

### 查询审计日志

```bash
curl -X GET "http://localhost:8080/api/v1/audit/logs?start_time=1704067200&end_time=1706745600&action=CredentialDecrypt&limit=50" \
  -H "Authorization: Bearer <token>"
```

**查询参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| `start_time` | u64 | 开始时间戳（Unix 秒，可选） |
| `end_time` | u64 | 结束时间戳（Unix 秒，可选） |
| `action` | string | 操作类型过滤（可选） |
| `risk_tier` | string | 风险等级过滤：Low/Medium/High/Critical（可选） |
| `user_id_hash` | string | 用户 ID 哈希过滤（可选） |
| `outcome` | string | 结果过滤：Success/Failure/Denied/Timeout/Aborted（可选） |
| `limit` | u32 | 返回条数限制（默认 20，最大 100） |
| `offset` | u32 | 分页偏移量（默认 0） |

**响应（200 OK）**:
```json
{
  "entries": [
    {
      "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
      "user_id_hash": "a1b2c3d4...",
      "timestamp": 1709990400000,
      "session_id": "session_xyz789",
      "service": "vault-service",
      "action": "CredentialDecrypt",
      "risk_tier": "High",
      "outcome": "Success",
      "tee_mrenclave": "9f86d081...",
      "action_token_jti": "jti_abc123",
      "chain_index": 42,
      "merkle_root": "e3b0c442..."
    }
  ],
  "total": 150,
  "limit": 20,
  "offset": 0,
  "has_more": true
}
```

### 获取审计日志详情

```bash
curl -X GET http://localhost:8080/api/v1/audit/logs/018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c \
  -H "Authorization: Bearer <token>"
```

**响应（200 OK）**:
```json
{
  "entry": {
    "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "user_id_hash": "a1b2c3d4...",
    "timestamp": 1709990400000,
    "session_id": "session_xyz789",
    "service": "vault-service",
    "action": "CredentialDecrypt",
    "risk_tier": "High",
    "outcome": "Success",
    "tee_mrenclave": "9f86d081...",
    "action_token_jti": "jti_abc123",
    "chain_index": 42,
    "merkle_root": "e3b0c442..."
  },
  "verification": {
    "verified": true,
    "signature_valid": true,
    "chain_hash_valid": true,
    "merkle_proof": ["abc123...", "def456..."],
    "state_hash": "a1b2c3d4..."
  }
}
```

### 导出审计日志

```bash
curl -X POST http://localhost:8080/api/v1/audit/export \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "start_time": 1704067200,
    "end_time": 1706745600,
    "format": "json",
    "include_verification": true
  }'
```

**请求体**:

| 字段 | 类型 | 说明 |
|------|------|------|
| `start_time` | u64 | 开始时间戳（Unix 秒，可选） |
| `end_time` | u64 | 结束时间戳（Unix 秒，可选） |
| `format` | string | 导出格式：`json` 或 `csv`（默认 json） |
| `include_verification` | bool | 是否包含验证签名（默认 false） |

**响应（200 OK）**:
```json
{
  "format": "json",
  "filename": "audit_export_20240301_120000.json",
  "content": "base64_encoded_content...",
  "entry_count": 150,
  "signature": "signature_base64...",
  "exported_at": "2024-03-01T12:00:00Z",
  "expires_at": "2024-03-08T12:00:00Z"
}
```

**CSV 格式导出**:
当 `format` 为 `csv` 时，返回的 `content` 包含以下列：
```csv
id,timestamp,user_id_hash,session_id,service,action,risk_tier,outcome,tee_mrenclave,action_token_jti,chain_index
```

### 验证审计条目

```bash
curl -X POST http://localhost:8080/api/v1/audit/verify \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "entry_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "include_proof": true
  }'
```

**请求体**:

| 字段 | 类型 | 说明 |
|------|------|------|
| `entry_id` | string | 审计条目 ID（UUID） |
| `include_proof` | bool | 是否包含 Merkle Proof（默认 true） |
| `chain_index` | u64 | 链索引（可选，用于直接索引验证） |

**响应（200 OK）**:
```json
{
  "verified": true,
  "entry_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "chain_index": 42,
  "signature_valid": true,
  "chain_hash_valid": true,
  "merkle_root": "e3b0c442...",
  "state_hash": "a1b2c3d4...",
  "verification_timestamp": "2024-03-01T12:00:00Z",
  "proof": {
    "inclusion_proof": ["abc123...", "def456..."],
    "transaction_id": 12345,
    "root_hash": "a1b2c3d4..."
  }
}
```

### 错误响应

| 状态码 | 错误 | 描述 |
|--------|------|------|
| 400 | `invalid_request` | 请求参数无效 |
| 401 | `missing_token` | 缺少 Authorization 头 |
| 401 | `invalid_token` | Token 无效或过期 |
| 403 | `insufficient_scope` | 缺少必需的 Scope（需要 `audit:read` 或 `admin`） |
| 404 | `not_found` | 审计条目不存在 |
| 500 | `internal_error` | 服务器内部错误 |

### 安全特性

- **不可篡改验证**: 所有条目包含 Ed25519 数字签名和 Merkle Tree 证明
- **PII 脱敏**: 自动脱敏敏感数据（SSN、密码、API Key 等）
- **链式验证**: 每个条目链接到前一个条目，形成完整证据链
- **导出签名**: 导出的日志文件包含数字签名，确保导出内容未被篡改
- **时间范围限制**: 防止一次性导出过多数据

### 审计 API 状态

```rust
use vault_service::api::{AuditApiState, AuditStorage, MemoryAuditStorageAdapter};

// 创建审计 API 状态
let audit_storage = MemoryAuditStorageAdapter::new(10000)?;
let audit_state = AuditApiState::new(audit_storage);

// 在 Axum Router 中注册
let app = Router::new()
    .merge(audit_routes())
    .with_state(audit_state);
```

### 模块结构

```text
audit/
├── mod.rs              - 模块导出和便捷函数
├── events.rs           - 审计事件定义（AuditEntry, AuditAction, RiskTier）
├── recorder.rs         - 审计记录器（Merkle Tree、签名、存储）
├── immudb_client.rs    - immudb 客户端封装
└── immudb_store.rs     - immudb 存储实现
```

### API 参考

| 类型 | 说明 |
|------|------|
| `AuditEntry` | 审计事件条目 |
| `AuditAction` | 审计操作类型枚举 |
| `RiskTier` | 风险等级枚举（Low/Medium/High/Critical）|
| `Outcome` | 操作结果枚举 |
| `PiiRedactor` | PII 数据脱敏工具 |
| `AuditRecorder` | 审计记录器（签名、Merkle Tree）|
| `MemoryAuditStorage` | 内存审计存储后端 |
| `SignedAuditEntry` | 签名后的审计条目 |
| `ImmuDbClient` | immudb 客户端 |
| `ImmuDbAuditStore` | immudb 审计存储 |
| `ImmuDbConfig` | immudb 配置 |
| `QueryOptions` | 查询选项 |

| 类型 | 说明 |
|------|------|
| `AuditEntry` | 审计事件条目 |
| `AuditAction` | 审计操作类型枚举 |
| `RiskTier` | 风险等级枚举（Low/Medium/High/Critical）|
| `Outcome` | 操作结果枚举（Success/Failure/Denied/Timeout/Aborted）|
| `PiiRedactor` | PII 数据脱敏工具 |
| `AuditRecorder` | 审计记录器（签名、Merkle Tree）|
| `MemoryAuditStorage` | 内存审计存储后端 |
| `SignedAuditEntry` | 签名后的审计条目 |

## 最新更新

### Phase 4 完成（2026-03-17）

**TEE 安全执行沙箱（TEE Sandbox）**:
- ✅ 基于 nsjail 的多层隔离（Namespaces + cgroups + seccomp）
- ✅ AI 审核引擎（操作审核、提示词注入检测）
- ✅ 安全导出通道（截图审核、数据签名）
- ✅ TypeScript SDK（WebSocket 实时连接）
- ✅ OpenAPI 规范文档
- ✅ 全面测试覆盖（单元测试 + 集成测试）

**性能优化**:
- 热实例启动 ≤ 100ms（本地）/ ≤ 50ms（TEE）
- 并发会话支持 100+
- 详见 [Phase 4 性能测试报告](docs/PHASE4_PERFORMANCE_REPORT.md)

**文档完善**:
- Sandbox SDK 使用指南
- API 文档更新
- 用户手册

### 验收状态

| 用例 | 状态 | 报告 |
|------|------|------|
| TC-001: 沙箱生命周期管理 | ✅ 通过 | [报告](test-cases/reports/TC-001-acceptance-report-20260318-final.md) |
| TC-002: 沙箱隔离机制 | 🔄 待验收 | - |
| TC-003: LLM 服务接口 | 🔄 待验收 | - |
| TC-004: AI 审核引擎 | 🔄 待验收 | - |
| TC-005: 安全导出通道 | 🔄 待验收 | - |

## 许可证

本项目采用 MIT 许可证 - 详见 [LICENSE](LICENSE) 文件

## 联系方式

- 项目主页: https://github.com/credbridge/credbridge
- 问题反馈: https://github.com/credbridge/credbridge/issues
- 文档: https://docs.credbridge.ai

---

**安全声明**: 本项目涉及密码学和可信计算技术，仅供学习和研究使用。生产环境部署前请进行全面的安全审计。
