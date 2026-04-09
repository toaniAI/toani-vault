# @toani/vault-sdk

Toani Vault SDK - TypeScript client for secure credential management.

[![npm version](https://img.shields.io/npm/v/@toani/vault-sdk.svg)](https://www.npmjs.com/package/@toani/vault-sdk)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.3-blue.svg)](https://www.typescriptlang.org/)
[![Node.js](https://img.shields.io/badge/Node.js-22+-green.svg)](https://nodejs.org/)

## 认证模式

> **重要**: 此 SDK 支持两种认证模式，请根据使用场景选择正确的方式。

### 认证模式对比

| 认证类型 | 适用场景 | 认证方式 |
|---------|---------|---------|
| **用户认证** | 最终用户访问 | Web 界面 Privy 钱包登录 |
| **服务账户认证** | 自动化、CI/CD、后台服务 | Platform API Token (此 SDK) |

### 用户认证 (Privy 钱包)

用户认证通过 Web 界面完成，使用 Privy 钱包登录：

1. 访问 https://vault.toani.io
2. 点击"使用钱包登录"
3. 通过 Privy 支持的钱包（如 MetaMask、Phantom）完成认证
4. 认证成功后获得用户 Session Token（仅表示登录态）

**注意**: `Privy Access Token` 仅用于换取 `Session Token`，不是 API 调用 token。

### 服务账户认证 (Platform API Token)

此 SDK 用于服务账户认证，适用于：

- CI/CD 管道自动化
- 后台服务/微服务
- 管理脚本和自动化工具
- 跨系统集成

使用 CLI 登录服务账户：

```bash
# 登录服务账户（需要 Platform API Token）
toani auth login --url https://vault.toani.io --token <your-platform-token> --service-account
```

或在代码中直接设置：

```typescript
const sdk = new ToaniVaultSDK({
  baseUrl: 'https://vault.toani.io',
  token: 'v4.local.your-platform-api-token',  // Platform API Token
});
```

### 如何获取 Platform API Token

Platform API Token 需通过管理界面或 API 创建：

1. 使用管理员账户登录 Web 界面
2. 进入"开发者中心" > "API Tokens"
3. 创建新的服务账户 Token，设置所需权限范围

## Token 与 Service Account API（新增）

```typescript
// 1) 用 Session Token 换 API Access Token（默认 900 秒）
const issued = await sdk.auth.createAccessToken({
  scopes: ['tokens:read', 'tokens:write'],
  ttlSeconds: 900,
});

// 2) token 元数据列表/详情/按 ID 撤销
const tokenList = await sdk.token.list();
const tokenMeta = await sdk.token.get(tokenList[0].tokenId);
await sdk.token.revokeById(tokenMeta.tokenId);

// 3) Service Account 生命周期
const sa = await sdk.serviceAccounts.create({
  name: 'ci-bot',
  scopeCeiling: ['credential:read', 'tokens:read'],
});
const saToken = await sdk.serviceAccounts.createToken(sa.id, {
  scopes: ['credential:read'],
  ttlSeconds: 3600,
  displayName: 'ci-job-token',
});
const saTokenMetadata = await sdk.serviceAccounts.listTokens(sa.id);
```

---

## 特性

- 🔐 **完整类型支持** - 100% TypeScript 类型定义
- 🔄 **自动 Token 刷新** - 支持 Token 过期前自动刷新
- 🔄 **智能重试** - 指数退避算法自动重试网络错误
- 🔑 **请求签名** - 支持请求签名验证
- 📊 **事件系统** - 完整的请求生命周期事件监听
- 🔒 **零信任架构** - 专为 TEE 凭证保险库设计

## 安装

```bash
npm install @toani/vault-sdk
# 或
yarn add @toani/vault-sdk
# 或
pnpm add @toani/vault-sdk
```

## 快速开始

```typescript
import { ToaniVaultSDK, CredentialType } from '@toani/vault-sdk';

// 初始化 SDK
const sdk = new ToaniVaultSDK({
  baseUrl: 'https://vault.toani.io',
  token: 'v4.local.your-paseto-token',
});

// 创建凭证
const credential = await sdk.credentials.create({
  serviceId: 'schwab',
  credentialType: CredentialType.UsernamePassword,
  plaintextData: {
    username: 'user@example.com',
    password: 'secret_password',
  },
});

console.log('Created credential:', credential.credentialId);

// 解密凭证
const decrypted = await sdk.credentials.decrypt(credential.credentialId);
console.log('Username:', decrypted.plaintextData.username);
```

## 迁移说明

> **注意**: 如果你之前使用的是 `@credbridge/sdk` 和 `CredBridgeSDK`，它们仍然可用但已标记为弃用。
>
> 旧名称将在 v1.0.0 版本中移除，建议尽快迁移到新名称。

```typescript
// ✅ 新名称（推荐）
import { ToaniVaultSDK } from '@toani/vault-sdk';
const sdk = new ToaniVaultSDK({ ... });

// ⚠️ 旧名称（已弃用，仍兼容）
import { CredBridgeSDK } from '@toani/vault-sdk';
const sdk = new CredBridgeSDK({ ... }); // 等同于 ToaniVaultSDK
```

## 配置选项

```typescript
const sdk = new ToaniVaultSDK({
  // 必需
  baseUrl: 'https://vault.toani.io',
  token: 'v4.local.your-token',

  // 可选
  timeout: 30000,                    // 请求超时（毫秒，默认 30000）
  maxRetries: 3,                     // 最大重试次数（默认 3）
  autoRefreshToken: true,            // 自动刷新 Token（默认 true）
  tokenRefreshBuffer: 5 * 60 * 1000, // Token 刷新缓冲时间（毫秒，默认 5分钟）
  headers: {                         // 自定义请求头
    'X-Custom-Header': 'value',
  },
  signingKey: 'your-signing-key',    // 请求签名密钥（可选）
});
```

## 凭证管理

### 创建凭证

```typescript
// 用户名密码凭证
const credential = await sdk.credentials.create({
  serviceId: 'schwab',
  credentialType: CredentialType.UsernamePassword,
  plaintextData: {
    username: 'user@example.com',
    password: 'secret',
  },
  expiresAt: Math.floor(Date.now() / 1000) + 86400 * 30, // 30天后过期
});

// 快捷方式：创建用户名密码凭证
const credential2 = await sdk.credentials.createUsernamePassword(
  'schwab',
  'user@example.com',
  'secret',
  { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 30 }
);

// 快捷方式：创建 API Key 凭证
const apiKeyCred = await sdk.credentials.createApiKey(
  'stripe',
  'sk_live_...',
  'secret_key'
);

// 快捷方式：创建 OAuth 刷新令牌
const oauthCred = await sdk.credentials.createOAuthRefresh(
  'google',
  '1//0d...'
);
```

### 获取凭证列表

```typescript
// 获取所有凭证
const { credentials, total } = await sdk.credentials.list();

// 按服务 ID 过滤
const { credentials } = await sdk.credentials.list({
  serviceId: 'schwab',
});

// 按类型过滤
const { credentials } = await sdk.credentials.list({
  credentialType: CredentialType.ApiKey,
});

// 快捷方式
const { credentials } = await sdk.credentials.getByService('schwab');
const { credentials } = await sdk.credentials.getByType(CredentialType.ApiKey);
```

### 获取凭证详情

```typescript
const credential = await sdk.credentials.get('credential-id');
console.log('Service:', credential.serviceId);
console.log('Type:', credential.credentialType);
```

### 解密凭证

```typescript
const decrypted = await sdk.credentials.decrypt(
  'credential-id',
  '用户登录操作' // 解密理由（用于审计）
);

console.log('Plaintext:', decrypted.plaintextData);
```

### 删除凭证

```typescript
const result = await sdk.credentials.delete('credential-id');
if (result.deleted) {
  console.log('Credential deleted');
}
```

### 检查凭证是否存在

```typescript
const exists = await sdk.credentials.exists('credential-id');
```

## Token 管理

```typescript
// 获取 Token 信息
const tokenInfo = sdk.token.getTokenInfo();
console.log('Token ID:', tokenInfo?.tokenId);
console.log('Tenant ID:', tokenInfo?.tenantId);
console.log('User ID:', tokenInfo?.userId);
console.log('Scopes:', tokenInfo?.scopes);

// 检查 Token 有效性
if (sdk.token.isValid()) {
  console.log('Token is valid');
}

// 检查 Token 是否即将过期
if (sdk.token.isExpiringSoon()) {
  console.log('Token will expire soon, refreshing...');
}

// 获取剩余时间
const remainingSeconds = sdk.token.getRemainingTime();
console.log(`Token expires in ${sdk.token.getRemainingTimeFormatted()}`);

// 检查 Scope
if (sdk.token.hasScope('credential:decrypt')) {
  console.log('Can decrypt credentials');
}

if (sdk.token.hasAnyScope(['credential:read', 'credential:write'])) {
  console.log('Can read or write credentials');
}

// 验证 Token（向服务器验证）
const isValid = await sdk.token.verify();

// 撤销 Token
await sdk.token.revoke();

// 更新 Token
sdk.token.setToken('v4.local.new-token');
```

## 事件监听

```typescript
// 监听 Token 即将过期
sdk.client.on('token_expiring', (event) => {
  console.log('Token expiring:', event.data);
});

// 监听 Token 刷新
sdk.client.on('token_refreshed', (event) => {
  console.log('Token refreshed:', event.data.tokenInfo);
});

// 监听请求事件
sdk.client.on('request_start', (event) => {
  console.log('Request started:', event.data.method, event.data.path);
});

sdk.client.on('request_success', (event) => {
  console.log('Request succeeded:', event.data.requestId);
});

sdk.client.on('request_error', (event) => {
  console.log('Request failed:', event.data.error);
});

// 监听重试事件
sdk.client.on('retry', (event) => {
  console.log(`Retrying ${event.data.attempt}/${event.data.maxRetries}`);
});
```

## 错误处理

```typescript
import { CredBridgeError, CredBridgeErrorCode } from '@toani/vault-sdk';

try {
  const credential = await sdk.credentials.get('invalid-id');
} catch (error) {
  if (error instanceof CredBridgeError) {
    console.log('Error code:', error.code);
    console.log('Error message:', error.message);
    console.log('Status code:', error.statusCode);
    console.log('Request ID:', error.requestId);

    // 检查错误类型
    if (error.isAuthError()) {
      console.log('Authentication error, please re-authenticate');
    }

    if (error.isRetryable()) {
      console.log('Network error, will retry');
    }

    // 根据错误码处理
    switch (error.code) {
      case CredBridgeErrorCode.NotFound:
        console.log('Credential not found');
        break;
      case CredBridgeErrorCode.Unauthorized:
        console.log('Invalid token');
        break;
      case CredBridgeErrorCode.InsufficientScope:
        console.log('Insufficient permissions');
        break;
    }
  }
}
```

## 高级用法

### 自定义请求选项

```typescript
const credential = await sdk.credentials.get('id', {
  timeout: 10000,      // 10秒超时
  skipRetry: true,     // 禁用重试
  requestId: 'custom-request-id',
  headers: {
    'X-Custom-Header': 'value',
  },
});
```

### 直接使用 HTTP 客户端

```typescript
// GET 请求
const data = await sdk.client.get('/some-endpoint');

// POST 请求
const result = await sdk.client.post('/some-endpoint', { key: 'value' });

// PUT 请求
await sdk.client.put('/some-endpoint', { key: 'value' });

// DELETE 请求
await sdk.client.delete('/some-endpoint');

// PATCH 请求
await sdk.client.patch('/some-endpoint', { key: 'value' });
```

### 检查兼容性

```typescript
const compatibility = await sdk.checkCompatibility();
console.log('Compatible:', compatibility.compatible);
console.log('API Version:', compatibility.apiVersion);
console.log('Message:', compatibility.message);
```

## 凭证类型

```typescript
enum CredentialType {
  UsernamePassword = 'username_password',  // 用户名密码
  OAuthRefresh = 'oauth_refresh',          // OAuth 刷新令牌
  ApiKey = 'api_key',                      // API 密钥
  SessionCookie = 'session_cookie',        // 会话 Cookie
  KycDocument = 'kyc_document',            // KYC 文档
}
```

## Token Scope

```typescript
enum TokenScope {
  CredentialRead = 'credential:read',      // 读取凭证元数据
  CredentialDecrypt = 'credential:decrypt', // 解密凭证
  CredentialWrite = 'credential:write',    // 创建/删除凭证
  AuditRead = 'audit:read',                // 读取审计日志
  Admin = 'admin',                         // 所有管理权限
}
```

## 开发

```bash
# 安装依赖
npm install

# 开发模式（热重载）
npm run dev

# 构建
npm run build

# 运行测试
npm test

# 运行测试（监视模式）
npm run test:watch

# 类型检查
npm run typecheck

# 代码检查
npm run lint
```

## 环境要求

- Node.js >= 22.0.0
- TypeScript >= 5.3.0

## 许可证

MIT License © Toani Team
