# ToaniVault Vault Service

ToaniVault 凭证保险库服务 - 基于 TEE（可信执行环境）和 HashiCorp Vault 的安全凭证存储系统。

## 功能特性

### 核心功能

- **TEE 内加密**：所有凭证在 Intel SGX Enclave 内使用 AES-256-GCM 加密
- **双重加密**：TEE 加密 + Vault 自带 AES-256 加密存储
- **PASETO Token**：基于 PASETO v4.local 的有限 Scope Token 系统
- **多租户隔离**：Schema-per-Tenant + 行级安全（RLS），详见 [多租户架构文档](../docs/MULTI_TENANCY.md)
- **UUID v7**：时间排序的凭证 ID，优化数据库索引性能
- **内存安全**：密钥使用后立即 zeroize，防止内存泄漏
- **监控与告警**：Prometheus 指标、健康检查、Webhook 告警通知，详见 [监控与告警文档](../docs/MONITORING.md)

### 存储后端

| 后端                | 适用场景  | 特性                          |
| ------------------- | --------- | ----------------------------- |
| InMemoryStorage     | 开发/测试 | 快速启动，数据不持久化        |
| VaultStorageBackend | 生产环境  | HashiCorp Vault KV v2，高可用 |

## 快速开始

### 安装依赖

```bash
# 安装 Rust (如果尚未安装)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装 HashiCorp Vault (开发环境)
brew install hashicorp/tap/vault  # macOS
# 或
sudo apt install vault             # Ubuntu/Debian
```

### 启动 Vault 开发服务器

```bash
vault server -dev -dev-root-token-id="root" -dev-listen-address="127.0.0.1:8200"
```

### 配置环境变量

```bash
export VAULT_ADDR="http://127.0.0.1:8200"
export VAULT_TOKEN="root"
export VAULT_MOUNT_PATH="secret"
```

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行 Vault 集成测试（需要 Vault 服务器）
export INTEGRATION_TESTS_ENABLED=1
cargo test --test vault_backend_tests
```

## 架构设计

### 四层密钥层次

```
┌─────────────────────────────────────────────────────────────┐
│                      四层密钥层次架构                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  L0: SGX Sealing Key（硬件根密钥）                            │
│    │ HKDF-Extract                                           │
│    ▼                                                         │
│  L1: Enclave Master Key（TEE 内派生）                         │
│    │ HKDF-Expand(tenant_id + user_id)                       │
│    ▼                                                         │
│  L2: User Vault Key（每用户独立）                             │
│    │ HKDF-Expand(credential_id + purpose)                   │
│    ▼                                                         │
│  L3: Credential Encryption Key（每条凭证独立）                 │
│    │ AES-256-GCM                                            │
│    ▼                                                         │
│  Encrypted Credential → Vault KV v2                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 存储路径格式

```
secret/credbridge/{tenant_id}/{credential_id}
```

示例：

```
secret/credbridge/tenant_123/uuid-v7-credential-id
```

## 使用示例

### 使用内存存储（开发）

```rust
use vault_service::vault::{CredentialVault, create_credential};
use vault_service::models::CredentialType;
use vault_service::crypto::constants;

// 创建 Vault（内存存储）
let vault = CredentialVault::new_in_memory();

// 创建凭证
let entry = create_credential(
    &vault,
    "tenant_123",
    "user_456",
    "schwab",
    CredentialType::UsernamePassword,
    encrypted_payload,
    None,  // 无过期时间
).unwrap();

println!("Created credential: {}", entry.credential_id.as_str());
```

### 使用 Vault 存储（生产）

```rust
use vault_service::vault::{VaultStorageBackend, VaultConfig, CredentialVault};

#[tokio::main]
async fn main() {
    // 配置 Vault
    let config = VaultConfig::from_env().unwrap();

    // 创建 Vault 存储后端
    let backend = VaultStorageBackend::new(config).await.unwrap();

    // 创建 CredentialVault
    let vault = CredentialVault::with_backend(Box::new(backend));

    // 使用 vault 进行操作
    // ...
}
```

### 配置 Vault

```rust
use vault_service::vault::VaultConfig;

let config = VaultConfig {
    addr: "https://vault.credbridge.internal:8200".to_string(),
    token: "hvs.xxx...".to_string(),  // 仅 TEE Enclave 持有
    mount_path: "secret".to_string(),
    namespace: None,
    ca_cert_path: Some("/etc/vault/ca.crt".to_string()),
    client_cert_path: Some("/etc/vault/client.crt".to_string()),
    client_key_path: Some("/etc/vault/client.key".to_string()),
    timeout_seconds: 30,
    max_retries: 3,
};
```

## 租户隔离中间件

ToaniVault 实现了基于 Token 的租户隔离中间件，自动从 PASETO Token 中提取 `tenant_id` 并验证租户隔离。

### 工作原理

```rust
// 中间件自动注入 RequestContext
pub async fn tenant_isolation_middleware(
    mut request: Request,
    next: Next,
) -> Response {
    // 1. 从 Token 提取 tenant_id
    // 2. 创建 RequestContext
    // 3. 注入到请求扩展
    // 4. 验证跨租户访问
}
```

### 使用示例

```rust
use vault_service::api::{TenantId, RequestContext};

// 在处理器中提取租户ID
async fn handler(
    TenantId(tenant_id): TenantId,
) -> impl IntoResponse {
    // tenant_id 自动从 Token 中提取
}

// 或提取完整上下文
async fn handler_with_context(
    context: RequestContext,
) -> impl IntoResponse {
    // 检查权限
    if !context.has_scope("credential:write") {
        return StatusCode::FORBIDDEN;
    }
    // ...
}
```

### 跨租户访问防护

```rust
// 验证路径参数中的租户ID
pub fn validate_path_tenant_id(
    ctx: &RequestContext,
    path_tenant_id: &str,
) -> Result<(), TenantIsolationError> {
    if ctx.tenant_id() != path_tenant_id {
        return Err(TenantIsolationError::CrossTenantAccessDenied {
            requested: ctx.tenant_id().to_string(),
            actual: path_tenant_id.to_string(),
        });
    }
    Ok(())
}
```

跨租户访问被拒绝时返回 `403 Forbidden`。

更多详情查看 [多租户架构文档](../docs/MULTI_TENANCY.md)。

## 租户管理模块

ToaniVault 提供了完整的租户管理模块，支持多租户配置管理、生命周期管理和资源隔离。

### 快速开始

```rust
use vault_service::tenant::{
    CreateTenantRequest, TenantManager, TenantConfig,
    TenantConfigManager, MemoryTenantConfigStore,
};

// 初始化租户管理器
let store = MemoryTenantConfigStore::new();
let manager = TenantManager::new_simple(store);

// 创建租户
let request = CreateTenantRequest::new("Acme Corp")
    .with_tier("enterprise")
    .with_admin_email("admin@acme.com");

let result = manager.create_tenant(request, Some("creator".to_string())).await?;
println!("Created tenant: {}", result.tenant.id);

// 获取租户配置
let config = manager.get_config(&result.tenant.id).await?;
println!("Max credentials: {}", config.quota_limits.max_credentials);
```

### 核心功能

- **租户配置管理**: 功能开关、配额限制、自定义设置
- **租户生命周期**: 创建、激活、暂停、删除
- **资源初始化**: 数据库 Schema、加密密钥、默认角色
- **配置版本控制**: 乐观锁防止并发冲突
- **审计日志**: 所有操作记录审计日志

### 租户层级

```rust
// 免费版
let free = TenantConfig::free_tier();
// - 100 凭证, 3 Token/用户
// - 基础功能（加密、审计）

// 专业版
let pro = TenantConfig::pro_tier();
// - 10K 凭证, 20 Token/用户
// - +MFA, Webhook, 高级审计

// 企业版
let enterprise = TenantConfig::enterprise_tier();
// - 100K 凭证, 100 Token/用户
// - +SSO, 自定义加密, 全功能
```

### 功能开关

| 功能                           | 描述         | Free | Pro | Enterprise |
| ------------------------------ | ------------ | ---- | --- | ---------- |
| `enable_credential_encryption` | 凭证加密     | ✅   | ✅  | ✅         |
| `enable_audit_logging`         | 审计日志     | ✅   | ✅  | ✅         |
| `enable_mfa`                   | 多因素认证   | ❌   | ✅  | ✅         |
| `enable_webhooks`              | Webhook 通知 | ❌   | ✅  | ✅         |
| `enable_sso`                   | SSO 集成     | ❌   | ❌  | ✅         |
| `enable_custom_crypto`         | 自定义加密   | ❌   | ❌  | ✅         |

### 配置更新

```rust
use vault_service::tenant::{
    PartialTenantConfig, FeatureFlags, QuotaLimits
};

// 部分更新配置
let partial = PartialTenantConfig {
    feature_flags: Some(FeatureFlags {
        enable_mfa: true,
        ..Default::default()
    }),
    quota_limits: Some(QuotaLimits {
        max_credentials: 50000,
        ..Default::default()
    }),
    settings: None,
};

let updated = manager.update_config(
    &tenant_id,
    partial,
    "admin_user"
).await?;

println!("Updated to version: {}", updated.version);
```

更多详情查看 [租户管理文档](../src/tenant/README.md) 和 [租户 API 文档](../docs/TENANT_API.md)。

## 远程认证 API

ToaniVault 提供基于 Intel SGX DCAP 的远程认证服务 API，支持挑战-响应协议来验证 Enclave 身份。

### 认证流程

```
1. POST /attestation/challenge     → 返回 {nonce, quote}
2. POST /attestation/verify-response → 验证挑战响应
3. GET  /attestation/status        → 查询认证状态
```

### 快速示例

```bash
# 1. 创建认证挑战
curl -X POST http://localhost:3000/api/v1/attestation/challenge \
  -H "Content-Type: application/json" \
  -d '{}'

# 响应: {"challenge_id": "chal_abc123", "nonce": "...", "quote_b64": "..."}

# 2. 验证挑战响应
curl -X POST http://localhost:3000/api/v1/attestation/verify-response \
  -H "Content-Type: application/json" \
  -d '{
    "challenge_id": "chal_abc123",
    "quote_b64": "base64_encoded_quote"
  }'

# 3. 查询认证状态
curl http://localhost:3000/api/v1/attestation/status
```

### 安全特性

- **防重放攻击**: 每个挑战只能使用一次，验证后自动失效
- **时间限制**: 挑战默认5分钟过期
- **测量值验证**: 验证 MRENCLAVE/MRSIGNER 在白名单中
- **Quote 绑定**: Quote 中的 REPORT_DATA 绑定挑战和 Enclave 身份

更多详情查看 [远程认证 API 文档](../docs/ATTESTATION_API.md)。

## Token 系统

ToaniVault 使用基于 **PASETO v4.local** 的有限 Scope Token 系统，在 TEE Enclave 内完成所有 Token 的签发和验证。

### Token 格式

```
v4.local.{base64url(payload)}
```

- **版本**: PASETO v4（XChaCha20-Poly1305 加密）
- **模式**: local（对称加密）
- **有效期**: 默认 15 分钟（符合 SA-003 架构约束）

### Token Claims

| 字段           | 描述                        | 示例               |
| -------------- | --------------------------- | ------------------ |
| `iss`          | 签发者                      | `credbridge-vault` |
| `sub`          | 用户 ID                     | `user_123`         |
| `aud`          | 租户 ID                     | `tenant_456`       |
| `exp`          | 过期时间（Unix 时间戳）     | `1710123456`       |
| `jti`          | Token 唯一标识符（UUID v7） | `018e...`          |
| `scope`        | 权限范围                    | `credential:read`  |
| `mfa_verified` | MFA 验证状态                | `true`             |

### Scope 权限

| Scope                | 描述                    |
| -------------------- | ----------------------- |
| `credential:read`    | 读取凭证元数据          |
| `credential:decrypt` | 解密凭证内容            |
| `credential:write`   | 创建/更新凭证           |
| `credential:delete`  | 删除凭证                |
| `token:manage`       | 管理 Token（撤销/刷新） |
| `audit:read`         | 读取审计日志            |
| `admin`              | 所有管理权限            |

### 使用示例

```rust
use vault_service::token::{TokenClaims, PasetoToken, scopes};

// 生成 Token 密钥
let key = PasetoToken::generate_key();

// 创建 Claims（15 分钟有效期）
let claims = TokenClaims::with_default_ttl(
    "user_123",           // sub: 用户 ID
    "tenant_456",         // aud: 租户 ID
    "credential:read",    // scope: 权限范围
    true,                 // mfa_verified
);

// 签发 Token
let token = PasetoToken::sign(&claims, &key)
    .expect("Token signing failed");

// 验证 Token
let validated = PasetoToken::verify(&token, &key, "tenant_456")
    .expect("Token verification failed");

// 检查 Scope
assert!(validated.has_scope("credential:read"));
```

### 密钥派生

Token 密钥可以从 L2 用户保险库密钥派生：

```rust
// 从 L2 密钥派生 Token 密钥
let token_key = PasetoToken::derive_key_from_master(
    &l2_key,
    &format!("token:{}", tenant_id)
).expect("Key derivation failed");
```

## Token 状态管理（Redis）

ToaniVault 使用 Redis 管理 Token 状态，支持撤销检查、元数据查询和批量撤销操作。

### Redis 数据结构

| 数据类型   | Key 格式                                | 用途                           |
| ---------- | --------------------------------------- | ------------------------------ |
| Sorted Set | `credbridge:tokens:{tenant_id}:active`  | 活跃 Token 集合（score = exp） |
| Set        | `credbridge:tokens:{tenant_id}:revoked` | 已撤销 Token 集合              |
| Hash       | `credbridge:token:{jti}`                | Token 元数据                   |

### Token 元数据

```rust
pub struct TokenMetadata {
    pub user_id: String,        // 用户 ID
    pub tenant_id: String,      // 租户 ID
    pub scope: String,          // 权限范围
    pub issued_at: u64,         // 签发时间
    pub expires_at: u64,        // 过期时间
    pub revoked: bool,          // 是否已撤销
    pub revoked_at: Option<u64>, // 撤销时间
}
```

### 基本使用

```rust
use vault_service::token::{RedisTokenStore, TokenRevoker};

// 创建 Redis Token 存储
let client = redis::Client::open("redis://127.0.0.1:6379/").unwrap();
let store = RedisTokenStore::new(client);

// 存储新 Token
store.store_token(
    "tenant_123",
    "jti_uuid",
    "user_456",
    "credential:read",
    1700000000, // exp timestamp
).await.unwrap();

// 检查是否撤销
let is_revoked = store.is_revoked("tenant_123", "jti_uuid").await.unwrap();

// 撤销 Token
store.revoke_token("tenant_123", "jti_uuid").await.unwrap();

// 获取元数据
let metadata = store.get_metadata("jti_uuid").await.unwrap();
```

### Token 撤销管理

```rust
use vault_service::token::{TokenRevoker, RevocationPolicy};

// 创建撤销管理器
let revoker = TokenRevoker::new(store);

// 撤销单个 Token
revoker.revoke_token("tenant_123", "jti_abc").await.unwrap();

// 批量撤销
revoker.revoke_batch(
    "tenant_123",
    &vec!["jti_1".to_string(), "jti_2".to_string()],
).await.unwrap();

// 撤销用户的所有 Token
revoker.revoke_by_user("tenant_123", "user_456").await.unwrap();

// 撤销特定 scope 的所有 Token
revoker.revoke_by_scope("tenant_123", "admin").await.unwrap();

// 撤销策略配置
let strict_policy = RevocationPolicy::strict();
let revoker = TokenRevoker::with_policy(store, strict_policy);
```

### Redis 配置

```bash
# 环境变量
export REDIS_URL="redis://127.0.0.1:6379"

# 或者带认证的连接
export REDIS_URL="redis://:password@127.0.0.1:6379/0"
```

## API 参考

### Vault 客户端

```rust
use vault_service::vault::VaultKvClient;

// 创建客户端
let client = VaultKvClient::new(config).await?;

// 初始化 KV v2 引擎
client.init_kv_engine().await?;

// 写入凭证
client.write_secret(
    "tenant_123",
    "cred_456",
    &json_data,
).await?;

// 读取凭证
let data = client.read_secret("tenant_123", "cred_456").await?;

// 删除凭证
client.delete_secret("tenant_123", "cred_456").await?;
```

### 健康检查

```rust
use vault_service::vault::backend::check_vault_health;

match check_vault_health(&config).await? {
    VaultHealthStatus::Healthy => println!("Vault is healthy"),
    VaultHealthStatus::Unauthenticated => println!("Authentication failed"),
    VaultHealthStatus::Unhealthy(msg) => println!("Vault unhealthy: {}", msg),
    VaultHealthStatus::Timeout => println!("Connection timeout"),
}
```

## 目录结构

```
vault-service/
├── src/
│   ├── api/                # API 模块
│   │   ├── mod.rs          # 模块导出
│   │   ├── attestation.rs  # 远程认证 API ⭐
│   │   ├── middleware.rs   # 认证中间件
│   │   ├── tenant_middleware.rs  # 租户隔离中间件 ⭐
│   │   ├── context.rs      # 请求上下文 ⭐
│   │   ├── routes.rs       # 路由定义
│   │   └── credentials.rs  # 凭证处理器
│   ├── token/              # Token 模块
│   │   ├── mod.rs          # 模块导出
│   │   ├── claims.rs       # Token Claims 定义
│   │   ├── paseto.rs       # PASETO 实现
│   │   ├── redis_store.rs  # Redis Token 状态管理
│   │   └── revocation.rs   # Token 撤销逻辑
│   ├── vault/              # 保险库模块
│   │   ├── mod.rs          # 模块导出
│   │   ├── models.rs       # 数据模型
│   │   ├── storage.rs      # 存储后端 trait
│   │   ├── client.rs       # Vault 客户端
│   │   └── backend.rs      # Vault 后端实现
│   └── ...
├── tests/
│   ├── api/
│   │   ├── attestation_tests.rs    # 认证 API 测试 ⭐
│   │   ├── audit_tests.rs          # 审计 API 测试
│   │   └── tenant_middleware_tests.rs  # 租户隔离测试 ⭐
│   ├── token/
│   │   ├── paseto_tests.rs         # PASETO 测试
│   │   └── redis_store_tests.rs    # Redis 集成测试
│   └── vault_backend_tests.rs      # Vault 集成测试
├── docs/
│   ├── ATTESTATION_API.md          # 远程认证 API 文档 ⭐
│   ├── MULTI_TENANCY.md            # 多租户架构文档 ⭐
│   └── architecture.md             # 架构设计文档
├── VAULT_SETUP.md          # Vault 安装配置指南
└── README.md               # 本文档
```

## 安全考虑

### Token 安全

- **仅 TEE Enclave 持有**：Vault Token 永远不会离开 TEE 安全边界
- **短期 Token**：设置合理的 TTL，定期轮换
- **专用策略**：使用最小权限原则，为 ToaniVault 创建专用策略

### 双重加密

```
原始凭证数据
    │
    ▼ AES-256-GCM (TEE 内)
加密密文
    │
    ▼ Vault AES-256 存储加密
Vault 存储
```

即使 Vault 服务器被攻破，攻击者也只能获得 TEE 加密后的密文，无法解密原始凭证。

### 审计日志

所有凭证操作都记录到不可篡改的审计日志（immudb）和 Vault 审计日志。

## 测试

### 单元测试

```bash
# 运行所有单元测试
cargo test

# 运行特定模块测试
cargo test vault::models::tests
cargo test vault::storage::tests
cargo test token::claims::tests
cargo test token::paseto::tests
```

### Vault 集成测试

```bash
# 启动 Vault 开发服务器
vault server -dev -dev-root-token-id="root"

# 运行集成测试
export INTEGRATION_TESTS_ENABLED=1
export VAULT_ADDR="http://127.0.0.1:8200"
export VAULT_TOKEN="root"
cargo test --test vault_backend_tests
```

### Redis Token 集成测试

```bash
# 启动 Redis 服务器
redis-server

# 运行 Redis Token 集成测试
cargo test --test redis_store_tests -- --nocapture

# 或者跳过 Redis 测试（如果没有 Redis）
cargo test --test redis_store_tests -- --skip
```

## 部署

### Docker 部署

```yaml
# docker-compose.yml
version: "3.8"

services:
  vault:
    image: hashicorp/vault:latest
    container_name: credbridge-vault
    ports:
      - "8200:8200"
    environment:
      - VAULT_DEV_ROOT_TOKEN_ID=root
      - VAULT_DEV_LISTEN_ADDRESS=0.0.0.0:8200
    cap_add:
      - IPC_LOCK
    command: server -dev

  credbridge:
    build: .
    environment:
      - VAULT_ADDR=http://vault:8200
      - VAULT_TOKEN=root
    depends_on:
      - vault
```

### Kubernetes 部署

参考 `k8s/` 目录下的 Helm Chart 配置。

## 贡献指南

1. Fork 仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

## 许可证

MIT License - 查看 [LICENSE](../LICENSE) 文件了解详情

## 监控与告警

ToaniVault Vault Service 内置了完整的监控与告警系统，支持 Prometheus 指标导出、健康检查和告警通知。

### 健康检查端点

```bash
# 基础健康检查
GET /health

# 详细健康检查（包含系统信息）
GET /health/detail
```

### Prometheus 指标端点

```bash
# 获取所有指标（Prometheus 格式）
GET /metrics
```

支持的指标类型：

- `credbridge_http_requests_total` - HTTP 请求总数
- `credbridge_http_error_rate_percentage` - HTTP 错误率
- `credbridge_http_request_duration_bucket` - 请求延迟分布
- `credbridge_tokens_active` - 当前活跃 Token 数
- `credbridge_tee_encryption_ops_total` - TEE 加密操作数
- `credbridge_alerts_triggered_total` - 触发告警总数

### 快速配置

```rust
use vault_service::metrics::MetricsCollector;
use vault_service::alerting::{AlertManager, AlertManagerConfig, WebhookConfig};
use std::sync::Arc;

// 创建指标收集器
let metrics = Arc::new(MetricsCollector::new());

// 配置告警管理器
let config = AlertManagerConfig {
    webhooks: vec![
        WebhookConfig {
            url: "https://hooks.slack.com/services/xxx".to_string(),
            method: "POST".to_string(),
            ..Default::default()
        },
    ],
    ..Default::default()
};
let (alert_manager, alert_rx) = AlertManager::new(config);
```

更多详情查看 [监控与告警文档](../docs/MONITORING.md)。

## 参考

- [Architecture](../docs/architecture.md) - 架构设计文档
- [ATTESTATION_API.md](../docs/ATTESTATION_API.md) - 远程认证 API 文档 ⭐
- [Multi-Tenancy](../docs/MULTI_TENANCY.md) - 多租户架构设计文档 ⭐
- [MONITORING.md](../docs/MONITORING.md) - 监控与告警文档 ⭐
- [VAULT_SETUP.md](./VAULT_SETUP.md) - Vault 安装配置指南
- [HashiCorp Vault 官方文档](https://developer.hashicorp.com/vault/docs)
