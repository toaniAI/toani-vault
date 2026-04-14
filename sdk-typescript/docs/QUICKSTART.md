# Toani Vault TypeScript SDK - 快速入门

## 安装

```bash
npm install @toani/vault-sdk
# 或
yarn add @toani/vault-sdk
# 或
pnpm add @toani/vault-sdk
```

## 迁移说明

如果你之前使用的是 `@credbridge/sdk` 包，`CredBridgeSDK` 类仍然可用但已标记为弃用。建议迁移到新的 `ToaniVaultSDK` 类。

```typescript
// ✅ 新名称（推荐）
import { ToaniVaultSDK } from '@toani/vault-sdk';
const sdk = new ToaniVaultSDK({ ... });

// ⚠️ 旧名称（已弃用，仍兼容）
import { CredBridgeSDK } from '@toani/vault-sdk';
const sdk = new CredBridgeSDK({ ... }); // 等同于 ToaniVaultSDK
```

## 初始化 SDK

```typescript
import { CredBridgeClient } from "@toani/vault-sdk";

const client = new CredBridgeClient({
  baseUrl: "https://api.toani.io",
  token: "your-api-token", // PASETO v4.local Token
  timeout: 30000, // 请求超时时间（毫秒）
  maxRetries: 3, // 最大重试次数
});
```

## 基础使用

### 创建凭证

```typescript
import { CredentialType } from "@toani/vault-sdk";

// 创建用户名密码凭证
const credential = await client.credentials.create({
  serviceId: "schwab",
  credentialType: CredentialType.UsernamePassword,
  plaintextData: {
    username: "user@example.com",
    password: "secret_password",
  },
  expiresAt: Math.floor(Date.now() / 1000) + 86400 * 30, // 30天后过期
});

console.log("Created:", credential.credentialId);
```

### 创建快捷方法

```typescript
// 创建用户名密码凭证
const cred1 = await client.credentials.createUsernamePassword(
  "schwab",
  "user@example.com",
  "secret_password",
);

// 创建 API Key 凭证
const cred2 = await client.credentials.createApiKey(
  "stripe",
  "sk_live_...",
  "sk_secret_...", // 可选
);

// 创建 OAuth 刷新令牌
const cred3 = await client.credentials.createOAuthRefresh("google", "1//0d...");
```

### 获取凭证列表

```typescript
// 获取所有凭证
const { credentials, total } = await client.credentials.list();

// 按服务过滤
const { credentials: schwabCreds } = await client.credentials.list({
  serviceId: "schwab",
});

// 按类型过滤
const { credentials: apiKeys } = await client.credentials.list({
  credentialType: CredentialType.ApiKey,
});
```

### 获取和解密凭证

```typescript
// 获取凭证详情（不包含明文）
const credential = await client.credentials.get("credential-id");

// 解密凭证
const decrypted = await client.credentials.decrypt(
  "credential-id",
  "用户登录操作", // 解密理由（用于审计）
);

console.log(decrypted.plaintextData.username);
console.log(decrypted.plaintextData.password);
```

### 删除凭证

```typescript
const result = await client.credentials.delete("credential-id");
if (result.deleted) {
  console.log("删除成功");
}
```

## Token 管理

```typescript
// 获取当前 Token 信息
const tokenInfo = client.getTokenInfo();
console.log("Tenant:", tokenInfo?.tenantId);
console.log("User:", tokenInfo?.userId);
console.log("Scopes:", tokenInfo?.scopes);

// 检查 Token 权限
if (client.token.hasScope("credential:read")) {
  console.log("有读取权限");
}

// 检查 Token 是否即将过期
if (client.token.isExpiringSoon()) {
  console.log("Token 即将过期");
}

// 获取剩余时间
const remainingSeconds = client.token.getRemainingTime();
console.log(`剩余时间: ${client.token.getRemainingTimeFormatted()}`);

// 本地检查 Token 是否仍在有效期内
const isValid = client.token.isValid();
```

## 错误处理

```typescript
import { CredBridgeError, CredBridgeErrorCode } from "@toani/vault-sdk";

try {
  const credential = await client.credentials.get("invalid-id");
} catch (error) {
  if (error instanceof CredBridgeError) {
    switch (error.code) {
      case CredBridgeErrorCode.NotFound:
        console.log("凭证不存在");
        break;
      case CredBridgeErrorCode.Unauthorized:
        console.log("未授权");
        break;
      case CredBridgeErrorCode.Forbidden:
        console.log("禁止访问");
        break;
      default:
        console.log("错误:", error.message);
    }
  }
}
```

## 事件监听

```typescript
// 监听 Token 刷新事件
const unsubscribe = client.on("token_refreshed", (event) => {
  console.log("Token 已刷新:", event.data.token);
});

// 监听请求事件
client.on("request_start", (event) => {
  console.log("请求开始:", event.data.method, event.data.path);
});

client.on("request_success", (event) => {
  console.log("请求成功:", event.data.requestId);
});

client.on("request_error", (event) => {
  console.error("请求失败:", event.data.error);
});

// 取消订阅
unsubscribe();
```

## 配置选项

```typescript
const client = new CredBridgeClient({
  baseUrl: "https://api.toani.io",
  token: "your-token",
  tenantId: "tenant-123", // 可选
  userId: "user-456", // 可选
  timeout: 30000,
  maxRetries: 3,
  autoRefreshToken: true, // 自动刷新 Token
  tokenRefreshBuffer: 5 * 60 * 1000, // 5分钟缓冲
  headers: {
    // 自定义请求头
    "X-Custom-Header": "value",
  },
  signingKey: "secret-key", // 请求签名密钥（可选）
});
```

## 请求选项

```typescript
// 单次请求选项
const credential = await client.credentials.get("id", {
  timeout: 10000, // 单次请求超时
  skipRetry: true, // 跳过重试
  retries: 5, // 自定义重试次数
  headers: {
    // 自定义请求头
    "X-Request-Source": "mobile-app",
  },
});
```

## 下一步

- 查看 [API 参考](./API_REFERENCE.md) 了解完整的 API 文档
- 查看 [高级示例](./EXAMPLES.md) 学习更多使用场景
