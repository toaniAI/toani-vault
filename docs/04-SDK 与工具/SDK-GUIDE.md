# CredBridge SDK 综合指南

> **迁移注意**: SDK 包已从 `@credbridge/sdk` (TypeScript) 和 `credbridge-sdk` (Rust) 重命名为 `@toani/vault-sdk` 和 `toani-vault-sdk`。旧包名已弃用。请将 `import { CredBridgeClient }` 改为 `import { CredBridgeClient } from '@toani/vault-sdk'`（类名保持不变），Rust 中使用 `use toani_vault_sdk::`。

本文档提供 CredBridge TypeScript 和 Rust SDK 的完整使用指南。

## 目录

- [概述](#概述)
- [快速开始](#快速开始)
  - [TypeScript](#typescript)
  - [Rust](#rust)
- [核心概念](#核心概念)
- [API 对比](#api-对比)
- [常见用例](#常见用例)
- [最佳实践](#最佳实践)
- [故障排除](#故障排除)

---

## 概述

CredBridge SDK 提供两种语言的官方实现：

- **TypeScript SDK**: 适用于 Node.js 和浏览器环境
- **Rust SDK**: 适用于高性能后端服务和系统级应用

两者提供一致的 API 设计和功能集，方便在不同技术栈间切换。

---

## 快速开始

### TypeScript

```bash
npm install @toani/vault-sdk
```

```typescript
import { CredBridgeClient, CredentialType } from "@toani/vault-sdk";

const client = new CredBridgeClient({
  baseUrl: "https://api.toani.io",
  token: "your-api-token",
});

// 创建凭证
const credential = await client.credentials.createUsernamePassword(
  "schwab",
  "user@example.com",
  "secret_password",
);

// 解密凭证
const decrypted = await client.credentials.decrypt(
  credential.credentialId,
  "用户登录操作",
);
```

### Rust

```toml
[dependencies]
toani-vault-sdk = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

```rust
use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CredBridgeConfig::new("https://api.toani.io")
        .with_token("your-api-token");

    let sdk = ToaniVaultSDK::new(config)?;

    // 创建凭证
    let credential = sdk.credentials()
        .create_username_password("schwab", "user@example.com", "secret_password", None, None)
        .await?;

    // 解密凭证
    let decrypted = sdk.credentials()
        .decrypt(&credential.credential_id, Some("用户登录操作"), None)
        .await?;

    Ok(())
}
```

---

## 核心概念

### 凭证类型

| 类型               | 描述           | 适用场景   |
| ------------------ | -------------- | ---------- |
| `UsernamePassword` | 用户名密码     | 传统登录   |
| `ApiKey`           | API 密钥       | 服务间调用 |
| `OAuthRefresh`     | OAuth 刷新令牌 | 第三方集成 |
| `SessionCookie`    | 会话 Cookie    | Web 应用   |
| `KycDocument`      | KYC 文档       | 合规验证   |

### Token Scope

| Scope                | 权限         | 描述             |
| -------------------- | ------------ | ---------------- |
| `credential:read`    | 读取凭证     | 查看凭证元数据   |
| `credential:decrypt` | 解密凭证     | 获取明文凭证内容 |
| `credential:write`   | 写入凭证     | 创建和删除凭证   |
| `audit:read`         | 读取审计日志 | 查看审计记录     |
| `admin`              | 管理员       | 所有权限         |

---

## API 对比

### 初始化

| 操作            | TypeScript                     | Rust                         |
| --------------- | ------------------------------ | ---------------------------- |
| 创建客户端      | `new CredBridgeClient(config)` | `ToaniVaultSDK::new(config)` |
| 设置 Token      | `client.setToken(token)`       | `client.set_token(token)`    |
| 获取 Token 信息 | `client.getTokenInfo()`        | `client.get_token_info()`    |

### 凭证管理

| 操作     | TypeScript                        | Rust                                                           |
| -------- | --------------------------------- | -------------------------------------------------------------- |
| 创建凭证 | `credentials.create(request)`     | `credentials.create(service_id, type, data, expires, options)` |
| 获取列表 | `credentials.list(filter)`        | `credentials.list(filter, options)`                            |
| 获取详情 | `credentials.get(id)`             | `credentials.get(id, options)`                                 |
| 解密凭证 | `credentials.decrypt(id, reason)` | `credentials.decrypt(id, reason, options)`                     |
| 删除凭证 | `credentials.delete(id)`          | `credentials.delete(id, options)`                              |

### Token 管理

| 操作         | TypeScript                     | Rust                             |
| ------------ | ------------------------------ | -------------------------------- |
| 检查有效期   | `token.isValid()`              | `token.is_valid()`               |
| 检查即将过期 | `token.isExpiringSoon(buffer)` | `token.is_expiring_soon(buffer)` |
| 获取剩余时间 | `token.getRemainingTime()`     | `token.get_remaining_time()`     |
| 检查权限     | `token.hasScope(scope)`        | `token.has_scope(scope)`         |
| 验证 Token   | `token.verify()`               | `token.verify(options)`          |

---

## 常见用例

### 1. 存储银行凭证

<Tabs>
<TabItem value="ts" label="TypeScript">

```typescript
// 存储银行登录凭证
const credential = await client.credentials.createUsernamePassword(
  "schwab",
  "user@example.com",
  "SecureBankPass123!",
  { expiresAt: Date.now() / 1000 + 86400 * 90 },
);

// 后续使用时解密
const decrypted = await client.credentials.decrypt(
  credential.credentialId,
  "执行转账操作",
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// 存储银行登录凭证
let credential = sdk.credentials()
    .create_username_password(
        "schwab",
        "user@example.com",
        "SecureBankPass123!",
        Some(chrono::Utc::now().timestamp() + 86400 * 90),
        None,
    )
    .await?;

// 后续使用时解密
let decrypted = sdk.credentials()
    .decrypt(&credential.credential_id, Some("执行转账操作"), None)
    .await?;
```

</TabItem>
</Tabs>

### 2. 管理 API 密钥

<Tabs>
<TabItem value="ts" label="TypeScript">

```typescript
// 存储 Stripe API 密钥
const apiKey = await client.credentials.createApiKey(
  "stripe",
  "sk_live_51H...",
  "sk_secret_...",
  { expiresAt: Date.now() / 1000 + 86400 * 365 },
);

// 获取所有 API Key 凭证
const { credentials } = await client.credentials.list({
  credentialType: CredentialType.ApiKey,
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// 存储 Stripe API 密钥
let api_key = sdk.credentials()
    .create_api_key(
        "stripe",
        "sk_live_51H...",
        Some("sk_secret_..."),
        Some(chrono::Utc::now().timestamp() + 86400 * 365),
        None,
    )
    .await?;

// 获取所有 API Key 凭证
let filter = CredentialFilter {
    credential_type: Some(CredentialType::ApiKey),
    ..Default::default()
};
let (credentials, _) = sdk.credentials().list(Some(filter), None).await?;
```

</TabItem>
</Tabs>

### 3. Token 权限检查

<Tabs>
<TabItem value="ts" label="TypeScript">

```typescript
// 检查权限
if (!client.token.hasScope("credential:decrypt")) {
  throw new Error("缺少解密权限");
}

// 检查多个权限
if (!client.token.hasAllScopes(["credential:read", "credential:decrypt"])) {
  throw new Error("权限不足");
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use toani_vault_sdk::types::TokenScope;

// 检查权限
if !sdk.token().has_scope(TokenScope::CredentialDecrypt) {
    return Err("缺少解密权限".into());
}

// 检查多个权限
let required = vec![TokenScope::CredentialRead, TokenScope::CredentialDecrypt];
if !sdk.token().has_all_scopes(&required) {
    return Err("权限不足".into());
}
```

</TabItem>
</Tabs>

---

## 最佳实践

### 1. 安全配置

```typescript
// 使用环境变量存储敏感信息
const client = new CredBridgeClient({
  baseUrl: process.env.TOANI_VAULT_API_URL!,
  token: process.env.CREDBRIDGE_TOKEN!,
  timeout: 30000,
  maxRetries: 3,
});
```

### 2. 错误处理

```typescript
import { CredBridgeError, CredBridgeErrorCode } from "@toani/vault-sdk";

try {
  const credential = await client.credentials.get(id);
} catch (error) {
  if (error instanceof CredBridgeError) {
    // 根据错误码处理
    switch (error.code) {
      case CredBridgeErrorCode.NotFound:
        // 凭证不存在
        break;
      case CredBridgeErrorCode.Unauthorized:
        // Token 无效，需要重新认证
        break;
      case CredBridgeErrorCode.TokenExpired:
        // Token 过期，需要刷新
        break;
    }
  }
}
```

### 3. 审计理由

始终提供清晰的解密理由，用于审计追踪：

```typescript
// 好的做法
await client.credentials.decrypt(id, "用户 user@example.com 执行转账");

// 不好的做法
await client.credentials.decrypt(id, "使用凭证");
```

### 4. 凭证过期管理

```typescript
// 设置合理的过期时间
const expiresAt = Math.floor(Date.now() / 1000) + 86400 * 90; // 90天

// 定期轮换凭证
async function rotateExpiringCredentials() {
  const { credentials } = await client.credentials.list({ onlyValid: true });

  for (const cred of credentials) {
    if (
      cred.expiresAt &&
      new Date(cred.expiresAt) < new Date(Date.now() + 7 * 86400 * 1000)
    ) {
      // 凭证将在7天内过期，执行轮换
      await rotateCredential(cred.credentialId);
    }
  }
}
```

---

## 故障排除

### 常见问题

#### 1. Token 过期

**错误**: `TokenExpired` 或 `Unauthorized`

**解决方案**:

```typescript
// 检查 Token 有效期
if (client.token.isExpiringSoon()) {
  // 刷新 Token
  const newToken = await refreshToken();
  client.setToken(newToken);
}
```

#### 2. 网络错误

**错误**: `NetworkError` 或 `Timeout`

**解决方案**:

```typescript
const client = new CredBridgeClient({
  baseUrl: "https://api.toani.io",
  token: "your-token",
  timeout: 60000, // 增加超时时间
  maxRetries: 5, // 增加重试次数
});
```

#### 3. 权限不足

**错误**: `InsufficientScope` 或 `Forbidden`

**解决方案**:

```typescript
// 检查权限
const scopes = client.token.getScopes();
console.log("Granted scopes:", scopes);

// 确认所需权限
const requiredScopes = ["credential:read", "credential:decrypt"];
if (!client.token.hasAllScopes(requiredScopes)) {
  console.error(
    "Missing scopes:",
    requiredScopes.filter((s) => !client.token.hasScope(s)),
  );
}
```

---

## 参考文档

- [TypeScript SDK 快速入门](../sdk-typescript/docs/QUICKSTART.md)
- [TypeScript SDK API 参考](../sdk-typescript/docs/API_REFERENCE.md)
- [TypeScript SDK 高级示例](../sdk-typescript/docs/EXAMPLES.md)
- [Rust SDK 快速入门](../sdk-rust/docs/QUICKSTART.md)
- [Rust SDK API 参考](../sdk-rust/docs/API_REFERENCE.md)
- [Rust SDK 高级示例](../sdk-rust/docs/EXAMPLES.md)

---

## 支持

如有问题或建议，请联系：

- 邮箱: support@credbridge.io
- GitHub Issues: https://github.com/credbridge/sdk/issues
