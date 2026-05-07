# ToaniVault TypeScript SDK - API 参考

## 目录

- [CredBridgeClient](#credbridgeclient)
- [CredentialsService](#credentialsservice)
- [TokenManager](#tokenmanager)
- [类型定义](#类型定义)
- [错误处理](#错误处理)

---

## CredBridgeClient

SDK 核心客户端类，提供 HTTP 请求、错误重试、Token 管理等功能。

### 构造函数

```typescript
new CredBridgeClient(config: CredBridgeConfig)
```

### 配置选项 (CredBridgeConfig)

| 属性                 | 类型                     | 必填 | 默认值 | 描述                                  |
| -------------------- | ------------------------ | ---- | ------ | ------------------------------------- |
| `baseUrl`            | `string`                 | 是   | -      | API 基础 URL                          |
| `token`              | `string`                 | 否   | -      | PASETO v4.local Token                 |
| `tenantId`           | `string`                 | 否   | -      | 租户 ID                               |
| `userId`             | `string`                 | 否   | -      | 用户 ID                               |
| `timeout`            | `number`                 | 否   | 30000  | 请求超时时间（毫秒）                  |
| `maxRetries`         | `number`                 | 否   | 3      | 最大重试次数                          |
| `autoRefreshToken`   | `boolean`                | 否   | true   | 是否自动刷新 Token                    |
| `tokenRefreshBuffer` | `number`                 | 否   | 300000 | Token 刷新缓冲时间（毫秒，默认5分钟） |
| `headers`            | `Record<string, string>` | 否   | {}     | 自定义请求头                          |
| `signingKey`         | `string`                 | 否   | -      | 请求签名密钥                          |

### 方法

#### getConfig()

获取当前配置。

```typescript
getConfig(): Required<CredBridgeConfig>
```

#### setToken(token)

更新 Token。

```typescript
setToken(token: string): void
```

#### getToken()

获取当前 Token。

```typescript
getToken(): string | undefined
```

#### getTokenInfo()

获取 Token 信息。

```typescript
getTokenInfo(): TokenInfo | undefined
```

#### isTokenExpiringSoon()

检查 Token 是否即将过期。

```typescript
isTokenExpiringSoon(): boolean
```

#### isTokenExpired()

检查 Token 是否已过期。

```typescript
isTokenExpired(): boolean
```

#### on(event, listener)

添加事件监听器。

```typescript
on<T>(event: SdkEventType, listener: EventListener<T>): () => void
```

返回取消订阅函数。

#### off(event, listener)

移除事件监听器。

```typescript
off<T>(event: SdkEventType, listener: EventListener<T>): void
```

#### HTTP 方法

```typescript
// GET 请求
get<T>(path: string, options?: RequestOptions): Promise<T>

// POST 请求
post<T>(path: string, body: unknown, options?: RequestOptions): Promise<T>

// PUT 请求
put<T>(path: string, body: unknown, options?: RequestOptions): Promise<T>

// DELETE 请求
delete<T>(path: string, options?: RequestOptions): Promise<T>

// PATCH 请求
patch<T>(path: string, body: unknown, options?: RequestOptions): Promise<T>
```

---

## CredentialsService

凭证管理服务，提供凭证的 CRUD 操作和解密功能。

### 访问方式

```typescript
const credentials = client.credentials;
```

### 方法

#### create(request, options)

创建新凭证。

```typescript
create(
  request: CreateCredentialRequest,
  options?: RequestOptions
): Promise<CreateCredentialResponse>
```

**CreateCredentialRequest:**

| 属性             | 类型                      | 必填 | 描述                    |
| ---------------- | ------------------------- | ---- | ----------------------- |
| `serviceId`      | `string`                  | 是   | 服务 ID                 |
| `credentialType` | `CredentialType`          | 是   | 凭证类型                |
| `plaintextData`  | `Record<string, unknown>` | 是   | 明文凭证内容            |
| `provider`       | `'okx' \| 'binance' \| 'custom'` | 否   | API Key 凭证使用的 provider |
| `allowedDomains` | `string[]`                | 否   | `sandbox http_request` 域名白名单 |
| `customFunctions` | `CredentialCustomFunction[]` | 否 | 自定义 TypeScript 模板函数 |
| `expiresAt`      | `number`                  | 否   | 过期时间（Unix 时间戳） |

**CreateCredentialResponse:**

| 属性               | 类型                                | 描述         |
| ------------------ | ----------------------------------- | ------------ |
| `credential_id`    | `string`                            | 凭证 ID      |
| `service_id`       | `string`                            | 服务 ID      |
| `credential_type`  | `string`                            | 凭证类型     |
| `created_at`       | `string`                            | 创建时间     |
| `provider`         | `CredentialProvider \| undefined`   | Provider     |
| `allowed_domains`  | `string[] \| undefined`             | 域名白名单   |
| `custom_functions` | `CredentialCustomFunction[] \| undefined` | 自定义模板函数 |
| `expires_at`       | `string \| undefined`               | 过期时间     |

#### createUsernamePassword(serviceId, username, password, options)

创建用户名密码凭证（快捷方法）。

```typescript
createUsernamePassword(
  serviceId: string,
  username: string,
  password: string,
  options?: { expiresAt?: number; requestOptions?: RequestOptions }
): Promise<CreateCredentialResponse>
```

#### createApiKey(serviceId, apiKey, secretKey, options)

创建 API Key 凭证（快捷方法）。

```typescript
createApiKey(
  serviceId: string,
  apiKey: string,
  secretKey?: string,
  options?: {
    expiresAt?: number;
    provider?: 'okx' | 'binance' | 'custom';
    allowedDomains?: string[];
    customFunctions?: CredentialCustomFunction[];
    passphrase?: string;
    requestOptions?: RequestOptions;
  }
): Promise<CreateCredentialResponse>
```

示例：

```typescript
await sdk.credentials.createApiKey("okx-trading", "okx_api_key", "okx_secret_key", {
  provider: "okx",
  passphrase: "okx_passphrase",
  allowedDomains: ["www.okx.com:443", "*.okx.com:443"],
});
```

`createApiKey()` 会同时写入 `secret_key` 和兼容字段 `api_secret`，便于 `${credential.secret_key}`、`${functions.okx_sign()}`、`${functions.binance_sign()}` 直接使用。

#### createOAuthRefresh(serviceId, refreshToken, options)

创建 OAuth 刷新令牌凭证（快捷方法）。

```typescript
createOAuthRefresh(
  serviceId: string,
  refreshToken: string,
  options?: { expiresAt?: number; requestOptions?: RequestOptions }
): Promise<CreateCredentialResponse>
```

#### list(filter, options)

获取凭证列表。

```typescript
list(
  filter?: CredentialFilter,
  options?: RequestOptions
): Promise<{ credentials: CredentialMetadata[]; total: number }>
```

**CredentialFilter:**

| 属性             | 类型             | 描述               |
| ---------------- | ---------------- | ------------------ |
| `serviceId`      | `string`         | 按服务 ID 过滤     |
| `credentialType` | `CredentialType` | 按凭证类型过滤     |
| `includeDeleted` | `boolean`        | 包含已删除的凭证   |
| `onlyValid`      | `boolean`        | 仅返回未过期的凭证 |

**CredentialMetadata:**

| 属性             | 类型                  | 描述         |
| ---------------- | --------------------- | ------------ |
| `credentialId`   | `string`              | 凭证 ID      |
| `credentialType` | `CredentialType`      | 凭证类型     |
| `userIdHash`     | `string`              | 用户 ID 哈希 |
| `serviceId`      | `string`              | 服务 ID      |
| `tenantId`       | `string`              | 租户 ID      |
| `createdAt`      | `string`              | 创建时间     |
| `expiresAt`      | `string \| undefined` | 过期时间     |
| `isDeleted`      | `boolean`             | 是否已删除   |

#### get(credentialId, options)

获取单个凭证详情。

```typescript
get(
  credentialId: string,
  options?: RequestOptions
): Promise<GetCredentialResponse>
```

#### decrypt(credentialId, reason, options)

解密凭证。

```typescript
decrypt(
  credentialId: string,
  reason?: string,
  options?: RequestOptions
): Promise<DecryptCredentialResponse>
```

**DecryptCredentialResponse:**

| 属性             | 类型                      | 描述           |
| ---------------- | ------------------------- | -------------- |
| `credentialId`   | `string`                  | 凭证 ID        |
| `serviceId`      | `string`                  | 服务 ID        |
| `credentialType` | `string`                  | 凭证类型       |
| `plaintextData`  | `Record<string, unknown>` | 解密的明文数据 |

#### delete(credentialId, options)

删除凭证。

```typescript
delete(
  credentialId: string,
  options?: RequestOptions
): Promise<DeleteCredentialResponse>
```

#### getByService(serviceId, options)

获取指定服务的所有凭证。

```typescript
getByService(
  serviceId: string,
  options?: RequestOptions
): Promise<{ credentials: CredentialMetadata[]; total: number }>
```

#### getByType(credentialType, options)

获取指定类型的所有凭证。

```typescript
getByType(
  credentialType: CredentialType,
  options?: RequestOptions
): Promise<{ credentials: CredentialMetadata[]; total: number }>
```

#### exists(credentialId, options)

检查凭证是否存在。

```typescript
exists(
  credentialId: string,
  options?: RequestOptions
): Promise<boolean>
```

---

## TokenManager

Token 管理类，提供 Token 验证、刷新和管理功能。

### 访问方式

```typescript
const token = client.token;
```

### 方法

#### getTokenInfo()

获取当前 Token 信息。

```typescript
getTokenInfo(): TokenInfo | undefined
```

**TokenInfo:**

| 属性        | 类型           | 描述                    |
| ----------- | -------------- | ----------------------- |
| `tokenId`   | `string`       | Token ID                |
| `subject`   | `string`       | 主题（租户ID:用户ID）   |
| `tenantId`  | `string`       | 租户 ID                 |
| `userId`    | `string`       | 用户 ID                 |
| `expiresAt` | `number`       | 过期时间（Unix 时间戳） |
| `scopes`    | `TokenScope[]` | 授权 Scope 列表         |
| `issuedAt`  | `number`       | 颁发时间                |

#### getToken()

获取当前 Token。

```typescript
getToken(): string | undefined
```

#### setToken(token)

设置新的 Token。

```typescript
setToken(token: string): void
```

#### isValid()

检查 Token 是否有效。

```typescript
isValid(): boolean
```

#### isExpiringSoon(bufferSeconds)

检查 Token 是否即将过期。

```typescript
isExpiringSoon(bufferSeconds?: number): boolean
```

- `bufferSeconds`: 过期前缓冲时间（秒，默认 300 秒 = 5 分钟）

#### getRemainingTime()

获取 Token 剩余有效时间。

```typescript
getRemainingTime(): number
```

返回剩余秒数（如果 Token 无效则返回 0）。

#### getRemainingTimeFormatted()

获取 Token 剩余有效时间的友好显示字符串。

```typescript
getRemainingTimeFormatted(): string
```

返回格式如："5分钟", "2小时", "3天", "已过期"

#### verify(options)

验证当前 Token（向服务器确认）。

```typescript
verify(options?: RequestOptions): Promise<boolean>
```

#### revoke(options)

撤销当前 Token。

```typescript
revoke(options?: RequestOptions): Promise<boolean>
```

#### hasScope(scope)

检查 Token 是否具有指定的 Scope。

```typescript
hasScope(scope: TokenScope | string): boolean
```

#### hasAnyScope(scopes)

检查 Token 是否具有指定的任一 Scope。

```typescript
hasAnyScope(scopes: TokenScope[] | string[]): boolean
```

#### hasAllScopes(scopes)

检查 Token 是否具有所有指定的 Scope。

```typescript
hasAllScopes(scopes: TokenScope[] | string[]): boolean
```

#### getScopes()

获取 Token 中的所有 Scope。

```typescript
getScopes(): TokenScope[]
```

#### getTenantId()

获取租户 ID。

```typescript
getTenantId(): string | undefined
```

#### getUserId()

获取用户 ID。

```typescript
getUserId(): string | undefined
```

#### getTokenId()

获取 Token ID。

```typescript
getTokenId(): string | undefined
```

#### getIssuedAt()

获取 Token 颁发时间。

```typescript
getIssuedAt(): number | undefined
```

#### getExpiresAt()

获取 Token 过期时间。

```typescript
getExpiresAt(): number | undefined
```

---

## 类型定义

### CredentialType 枚举

```typescript
enum CredentialType {
  UsernamePassword = "username_password",
  OAuthRefresh = "oauth_refresh",
  ApiKey = "api_key",
  SessionCookie = "session_cookie",
  KycDocument = "kyc_document",
}
```

### TokenScope 枚举

```typescript
enum TokenScope {
  CredentialRead = "credential:read",
  CredentialDecrypt = "credential:decrypt",
  CredentialWrite = "credential:write",
  AuditRead = "audit:read",
  Admin = "admin",
}
```

### SdkEventType 枚举

```typescript
enum SdkEventType {
  TokenExpiring = "token_expiring",
  TokenRefreshed = "token_refreshed",
  RequestStart = "request_start",
  RequestSuccess = "request_success",
  RequestError = "request_error",
  Retry = "retry",
}
```

### RequestOptions 接口

```typescript
interface RequestOptions {
  timeout?: number; // 请求超时时间（毫秒）
  skipRetry?: boolean; // 是否跳过重试
  retries?: number; // 重试次数
  headers?: Record<string, string>; // 自定义请求头
  requestId?: string; // 请求 ID
}
```

---

## 错误处理

### CredBridgeError 类

```typescript
class CredBridgeError extends Error {
  readonly code: CredBridgeErrorCode;
  readonly statusCode?: number;
  readonly details?: Record<string, unknown>;
  readonly requestId?: string;

  isNetworkError(): boolean;
  isAuthError(): boolean;
  isRetryable(): boolean;
}
```

### CredBridgeErrorCode 枚举

```typescript
enum CredBridgeErrorCode {
  Unknown = "unknown",
  NetworkError = "network_error",
  Timeout = "timeout",
  Unauthorized = "unauthorized",
  Forbidden = "forbidden",
  NotFound = "not_found",
  InvalidRequest = "invalid_request",
  InternalError = "internal_error",
  TokenExpired = "token_expired",
  InvalidToken = "invalid_token",
  TokenRevoked = "token_revoked",
  InsufficientScope = "insufficient_scope",
  TenantIsolationViolation = "tenant_isolation_violation",
  CredentialExpired = "credential_expired",
  DecryptionFailed = "decryption_failed",
  EncryptionFailed = "encryption_failed",
}
```

---

## 常量

```typescript
// SDK 版本
const VERSION: string;
```
