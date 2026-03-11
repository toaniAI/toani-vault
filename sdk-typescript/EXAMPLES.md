# CredBridge SDK 使用示例

本文档提供了各种场景下的 SDK 使用示例。

## 目录

- [基础示例](#基础示例)
- [凭证管理示例](#凭证管理示例)
- [Token 管理示例](#token-管理示例)
- [错误处理示例](#错误处理示例)
- [事件监听示例](#事件监听示例)
- [实际应用场景](#实际应用场景)

---

## 基础示例

### 初始化 SDK

```typescript
import { CredBridgeSDK, CredentialType } from '@credbridge/sdk';

// 方式 1：构造函数
const sdk = new CredBridgeSDK({
  baseUrl: 'https://vault.credbridge.io',
  token: process.env.CREDBRIDGE_TOKEN!,
});

// 方式 2：工厂方法
const sdk2 = CredBridgeSDK.create(
  'https://vault.credbridge.io',
  process.env.CREDBRIDGE_TOKEN!
);

// 高级配置
const sdk3 = new CredBridgeSDK({
  baseUrl: 'https://vault.credbridge.io',
  token: process.env.CREDBRIDGE_TOKEN!,
  timeout: 60000,
  maxRetries: 5,
  autoRefreshToken: true,
  tokenRefreshBuffer: 5 * 60 * 1000,
  headers: {
    'X-Environment': 'production',
  },
});
```

---

## 凭证管理示例

### 示例 1：存储金融账户凭证

```typescript
import { CredBridgeSDK, CredentialType, CredBridgeError } from '@credbridge/sdk';

async function storeSchwabCredentials(
  sdk: CredBridgeSDK,
  username: string,
  password: string
): Promise<string> {
  try {
    // 创建用户名密码凭证，90天后过期
    const expiresAt = Math.floor(Date.now() / 1000) + 86400 * 90;

    const credential = await sdk.credentials.createUsernamePassword(
      'schwab',
      username,
      password,
      { expiresAt }
    );

    console.log(`✅ Schwab credentials stored: ${credential.credentialId}`);
    return credential.credentialId;
  } catch (error) {
    if (error instanceof CredBridgeError) {
      console.error(`❌ Failed to store credentials: ${error.message}`);
      throw error;
    }
    throw error;
  }
}
```

### 示例 2：获取并解密凭证进行登录

```typescript
async function performLogin(
  sdk: CredBridgeSDK,
  credentialId: string
): Promise<{ username: string; password: string }> {
  try {
    // 首先获取凭证元数据（不需要解密权限）
    const metadata = await sdk.credentials.get(credentialId);
    console.log(`Logging into ${metadata.serviceId}...`);

    // 解密凭证（需要 credential:decrypt scope）
    const decrypted = await sdk.credentials.decrypt(
      credentialId,
      `User login operation at ${new Date().toISOString()}`
    );

    const { username, password } = decrypted.plaintextData as {
      username: string;
      password: string;
    };

    console.log(`✅ Successfully retrieved credentials for ${decrypted.serviceId}`);
    return { username, password };
  } catch (error) {
    if (error instanceof CredBridgeError) {
      if (error.code === 'not_found') {
        console.error('❌ Credential not found');
      } else if (error.code === 'insufficient_scope') {
        console.error('❌ Missing decrypt permission');
      } else {
        console.error(`❌ Login failed: ${error.message}`);
      }
    }
    throw error;
  }
}
```

### 示例 3：批量存储多个服务的凭证

```typescript
async function setupUserCredentials(sdk: CredBridgeSDK) {
  const services = [
    {
      serviceId: 'schwab',
      type: CredentialType.UsernamePassword,
      data: { username: 'user@example.com', password: 'schwab_pass' },
    },
    {
      serviceId: 'etrade',
      type: CredentialType.UsernamePassword,
      data: { username: 'user@example.com', password: 'etrade_pass' },
    },
    {
      serviceId: 'paypal',
      type: CredentialType.OAuthRefresh,
      data: { refreshToken: '1//0dXj...' },
    },
  ];

  const results = await Promise.allSettled(
    services.map(async (service) => {
      const credential = await sdk.credentials.create({
        serviceId: service.serviceId,
        credentialType: service.type,
        plaintextData: service.data,
      });
      return { serviceId: service.serviceId, credentialId: credential.credentialId };
    })
  );

  const successful = results
    .filter((r): r is PromiseFulfilledResult<unknown> => r.status === 'fulfilled')
    .map((r) => r.value);

  const failed = results
    .filter((r): r is PromiseRejectedResult => r.status === 'rejected')
    .map((r, i) => ({ serviceId: services[i].serviceId, error: r.reason }));

  console.log(`✅ Successfully stored ${successful.length} credentials`);
  if (failed.length > 0) {
    console.error(`❌ Failed to store ${failed.length} credentials:`, failed);
  }

  return { successful, failed };
}
```

### 示例 4：列出并按服务过滤凭证

```typescript
async function listUserCredentials(sdk: CredBridgeSDK) {
  // 获取所有凭证
  const { credentials, total } = await sdk.credentials.list();
  console.log(`Total credentials: ${total}`);

  // 按服务分组
  const byService = credentials.reduce((acc, cred) => {
    acc[cred.serviceId] = acc[cred.serviceId] || [];
    acc[cred.serviceId].push(cred);
    return acc;
  }, {} as Record<string, typeof credentials>);

  console.log('\n📊 Credentials by service:');
  Object.entries(byService).forEach(([serviceId, creds]) => {
    console.log(`  ${serviceId}: ${creds.length} credential(s)`);
  });

  return byService;
}

async function getServiceCredentials(sdk: CredBridgeSDK, serviceId: string) {
  // 只获取特定服务的凭证
  const { credentials } = await sdk.credentials.getByService(serviceId);
  console.log(`Found ${credentials.length} credential(s) for ${serviceId}`);
  return credentials;
}
```

### 示例 5：凭证生命周期管理

```typescript
async function rotateCredentials(
  sdk: CredBridgeSDK,
  credentialId: string,
  newPassword: string
) {
  try {
    // 1. 获取旧凭证信息
    const oldCredential = await sdk.credentials.get(credentialId);
    console.log(`Rotating credentials for ${oldCredential.serviceId}...`);

    // 2. 创建新凭证（新密码）
    const newCredential = await sdk.credentials.create({
      serviceId: oldCredential.serviceId,
      credentialType: CredentialType.UsernamePassword,
      plaintextData: {
        username: oldCredential.serviceId, // 保留用户名
        password: newPassword,
      },
      expiresAt: Math.floor(Date.now() / 1000) + 86400 * 90, // 90天后过期
    });

    // 3. 删除旧凭证
    await sdk.credentials.delete(credentialId);

    console.log(`✅ Credentials rotated successfully`);
    console.log(`   Old ID: ${credentialId}`);
    console.log(`   New ID: ${newCredential.credentialId}`);

    return newCredential.credentialId;
  } catch (error) {
    console.error('❌ Failed to rotate credentials:', error);
    throw error;
  }
}

async function cleanupExpiredCredentials(sdk: CredBridgeSDK) {
  // 获取所有凭证（包含已过期的）
  const { credentials } = await sdk.credentials.list({
    includeDeleted: true,
    onlyValid: false,
  });

  const expiredCount = credentials.filter((c) => {
    if (!c.expiresAt) return false;
    return new Date(c.expiresAt) < new Date();
  }).length;

  console.log(`Found ${expiredCount} expired credentials`);

  // 实际删除操作（软删除）
  for (const credential of credentials) {
    if (credential.expiresAt && new Date(credential.expiresAt) < new Date()) {
      console.log(`Deleting expired credential: ${credential.credentialId}`);
      await sdk.credentials.delete(credential.credentialId);
    }
  }
}
```

---

## Token 管理示例

### 示例 6：Token 有效性监控

```typescript
import { CredBridgeSDK, CredBridgeError } from '@credbridge/sdk';

class TokenMonitor {
  private sdk: CredBridgeSDK;
  private refreshCallback?: (newToken: string) => void;

  constructor(sdk: CredBridgeSDK, refreshCallback?: (newToken: string) => void) {
    this.sdk = sdk;
    this.refreshCallback = refreshCallback;
    this.startMonitoring();
  }

  private startMonitoring() {
    // 每分钟检查一次 Token 状态
    setInterval(() => this.checkToken(), 60 * 1000);
  }

  private async checkToken() {
    try {
      // 检查 Token 是否即将过期
      if (this.sdk.token.isExpiringSoon(5 * 60)) {
        // 5分钟缓冲
        console.log('⚠️ Token expiring soon!');
        console.log(`⏰ Time remaining: ${this.sdk.token.getRemainingTimeFormatted()}`);

        // 触发刷新回调（由外部处理实际的 Token 刷新逻辑）
        if (this.refreshCallback) {
          const newToken = await this.refreshToken();
          this.refreshCallback(newToken);
        }
      }

      // 验证 Token 状态
      const isValid = await this.sdk.token.verify();
      if (!isValid) {
        console.error('❌ Token is invalid or revoked');
      }
    } catch (error) {
      console.error('Token check failed:', error);
    }
  }

  private async refreshToken(): Promise<string> {
    // 这里应该调用你的 Token 刷新逻辑
    // 例如：从认证服务器获取新 Token
    console.log('Refreshing token...');
    throw new Error('Token refresh not implemented');
  }

  public getTokenInfo() {
    return {
      id: this.sdk.token.getTokenId(),
      tenantId: this.sdk.token.getTenantId(),
      userId: this.sdk.token.getUserId(),
      scopes: this.sdk.token.getScopes(),
      expiresAt: this.sdk.token.getExpiresAt(),
      remainingTime: this.sdk.token.getRemainingTimeFormatted(),
    };
  }
}

// 使用示例
const sdk = new CredBridgeSDK({
  baseUrl: 'https://vault.credbridge.io',
  token: process.env.CREDBRIDGE_TOKEN!,
  autoRefreshToken: true,
});

const monitor = new TokenMonitor(sdk, (newToken) => {
  console.log('Token refreshed!');
  // 更新环境变量或存储
});

console.log(monitor.getTokenInfo());
```

### 示例 7：权限检查

```typescript
function checkPermissions(sdk: CredBridgeSDK) {
  const scopes = sdk.token.getScopes();
  console.log('Current token scopes:', scopes);

  // 检查特定权限
  const permissions = {
    canRead: sdk.token.hasScope('credential:read'),
    canWrite: sdk.token.hasScope('credential:write'),
    canDecrypt: sdk.token.hasScope('credential:decrypt'),
    canAudit: sdk.token.hasScope('audit:read'),
    isAdmin: sdk.token.hasScope('admin'),
  };

  console.log('Permissions:', permissions);

  // 检查复合权限
  if (sdk.token.hasAnyScope(['credential:read', 'credential:write'])) {
    console.log('✅ Can access credentials');
  }

  if (sdk.token.hasAllScopes(['credential:read', 'credential:decrypt'])) {
    console.log('✅ Can read and decrypt credentials');
  }

  return permissions;
}

async function performSecureOperation(sdk: CredBridgeSDK) {
  // 在执行敏感操作前检查权限
  if (!sdk.token.hasScope('credential:decrypt')) {
    throw new Error('Insufficient permissions: credential:decrypt scope required');
  }

  // 检查 Token 是否有效
  if (!sdk.token.isValid()) {
    throw new Error('Token is expired');
  }

  // 执行操作
  // ...
}
```

---

## 错误处理示例

### 示例 8：完整的错误处理

```typescript
import {
  CredBridgeSDK,
  CredBridgeError,
  CredBridgeErrorCode,
} from '@credbridge/sdk';

async function safeCredentialOperation(sdk: CredBridgeSDK, credentialId: string) {
  try {
    const credential = await sdk.credentials.get(credentialId);
    return credential;
  } catch (error) {
    if (error instanceof CredBridgeError) {
      switch (error.code) {
        case CredBridgeErrorCode.NotFound:
          console.error(`Credential ${credentialId} not found`);
          return null;

        case CredBridgeErrorCode.Unauthorized:
        case CredBridgeErrorCode.InvalidToken:
        case CredBridgeErrorCode.TokenExpired:
          console.error('Authentication failed, please login again');
          // 重定向到登录页面
          throw error;

        case CredBridgeErrorCode.InsufficientScope:
          console.error(`Missing required permissions: ${error.message}`);
          // 请求额外权限
          throw error;

        case CredBridgeErrorCode.TenantIsolationViolation:
          console.error('Access denied: tenant isolation violation');
          // 记录安全事件
          throw error;

        case CredBridgeErrorCode.NetworkError:
        case CredBridgeErrorCode.Timeout:
          console.error('Network issue, please try again later');
          // 可以选择在这里重试
          throw error;

        case CredBridgeErrorCode.InternalError:
          console.error('Server error:', error.message);
          throw error;

        default:
          console.error('Unexpected error:', error);
          throw error;
      }
    }

    // 非 SDK 错误
    console.error('Unknown error:', error);
    throw error;
  }
}

// 重试包装器
async function withRetry<T>(
  operation: () => Promise<T>,
  maxRetries = 3
): Promise<T> {
  let lastError: Error | undefined;

  for (let i = 0; i < maxRetries; i++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error as Error;

      if (error instanceof CredBridgeError) {
        // 不可重试的错误直接抛出
        if (!error.isRetryable()) {
          throw error;
        }
      }

      if (i < maxRetries - 1) {
        const delay = Math.pow(2, i) * 1000; // 指数退避
        console.log(`Retry ${i + 1}/${maxRetries} after ${delay}ms`);
        await new Promise((resolve) => setTimeout(resolve, delay));
      }
    }
  }

  throw lastError;
}
```

---

## 事件监听示例

### 示例 9：请求日志和监控

```typescript
import { CredBridgeSDK, SdkEventType } from '@credbridge/sdk';

function setupRequestLogging(sdk: CredBridgeSDK) {
  const requestTimings = new Map<string, number>();

  // 请求开始
  sdk.client.on(SdkEventType.RequestStart, (event) => {
    requestTimings.set(event.data.requestId, event.timestamp);
    console.log(`➡️  ${event.data.method} ${event.data.path}`);
  });

  // 请求成功
  sdk.client.on(SdkEventType.RequestSuccess, (event) => {
    const startTime = requestTimings.get(event.data.requestId);
    const duration = startTime ? event.timestamp - startTime : 0;
    requestTimings.delete(event.data.requestId);
    console.log(`✅ Request completed in ${duration}ms`);
  });

  // 请求失败
  sdk.client.on(SdkEventType.RequestError, (event) => {
    const startTime = requestTimings.get(event.data.requestId);
    const duration = startTime ? event.timestamp - startTime : 0;
    requestTimings.delete(event.data.requestId);
    console.error(`❌ Request failed after ${duration}ms:`, event.data.error);
  });

  // 重试事件
  sdk.client.on(SdkEventType.Retry, (event) => {
    console.log(
      `🔄 Retry ${event.data.attempt}/${event.data.maxRetries} for ${event.data.method} ${event.data.path}`
    );
  });

  // Token 刷新
  sdk.client.on(SdkEventType.TokenRefreshed, (event) => {
    console.log('🔑 Token refreshed successfully');
    console.log('   New token info:', event.data.tokenInfo);
  });
}

// 使用
const sdk = new CredBridgeSDK({
  baseUrl: 'https://vault.credbridge.io',
  token: process.env.CREDBRIDGE_TOKEN!,
});

setupRequestLogging(sdk);
```

---

## 实际应用场景

### 示例 10：AI Agent 自动登录系统

```typescript
import { CredBridgeSDK, CredentialType, CredBridgeError } from '@credbridge/sdk';

class AgentLoginSystem {
  private sdk: CredBridgeSDK;

  constructor(token: string) {
    this.sdk = new CredBridgeSDK({
      baseUrl: process.env.CREDBRIDGE_URL!,
      token,
    });
  }

  /**
   * 为用户执行自动登录
   */
  async performLogin(userId: string, serviceId: string): Promise<{
    success: boolean;
    credentials?: { username: string; password: string };
    error?: string;
  }> {
    try {
      // 1. 查找用户的凭证
      const { credentials } = await this.sdk.credentials.getByService(serviceId);

      if (credentials.length === 0) {
        return {
          success: false,
          error: `No credentials found for service ${serviceId}`,
        };
      }

      // 2. 使用最新的凭证
      const latestCredential = credentials[0];

      // 3. 检查是否需要 MFA
      if (!this.sdk.token.hasScope('credential:decrypt')) {
        return {
          success: false,
          error: 'MFA verification required for credential decryption',
        };
      }

      // 4. 解密凭证
      const decrypted = await this.sdk.credentials.decrypt(
        latestCredential.credentialId,
        `Automated login for user ${userId}`
      );

      const { username, password } = decrypted.plaintextData as {
        username: string;
        password: string;
      };

      // 5. 执行登录（这里是伪代码）
      // const session = await loginToService(serviceId, username, password);

      return {
        success: true,
        credentials: { username, password },
      };
    } catch (error) {
      if (error instanceof CredBridgeError) {
        return {
          success: false,
          error: error.message,
        };
      }
      throw error;
    }
  }

  /**
   * 检查服务可用性
   */
  async checkServiceAvailability(serviceId: string): Promise<{
    hasCredentials: boolean;
    canDecrypt: boolean;
    credentialCount: number;
  }> {
    const { credentials } = await this.sdk.credentials.getByService(serviceId);

    return {
      hasCredentials: credentials.length > 0,
      canDecrypt: this.sdk.token.hasScope('credential:decrypt'),
      credentialCount: credentials.length,
    };
  }
}

// 使用示例
async function main() {
  const system = new AgentLoginSystem(process.env.CREDBRIDGE_TOKEN!);

  // 检查服务可用性
  const availability = await system.checkServiceAvailability('schwab');
  console.log('Service availability:', availability);

  // 执行登录
  if (availability.hasCredentials && availability.canDecrypt) {
    const result = await system.performLogin('user123', 'schwab');
    if (result.success) {
      console.log('Login successful!');
      console.log('Username:', result.credentials?.username);
      // 不要记录密码！
    } else {
      console.error('Login failed:', result.error);
    }
  }
}

main().catch(console.error);
```

### 示例 11：多租户管理控制台

```typescript
import { CredBridgeSDK } from '@credbridge/sdk';

interface TenantStats {
  tenantId: string;
  totalCredentials: number;
  servicesUsed: string[];
  expiringSoon: number;
}

async function getTenantStats(sdk: CredBridgeSDK): Promise<TenantStats> {
  const { credentials } = await sdk.credentials.list();

  const now = new Date();
  const thirtyDaysFromNow = new Date(now.getTime() + 30 * 24 * 60 * 60 * 1000);

  const servicesUsed = [...new Set(credentials.map((c) => c.serviceId))];

  const expiringSoon = credentials.filter((c) => {
    if (!c.expiresAt) return false;
    const expiry = new Date(c.expiresAt);
    return expiry <= thirtyDaysFromNow && expiry > now;
  }).length;

  return {
    tenantId: sdk.token.getTenantId() || 'unknown',
    totalCredentials: credentials.length,
    servicesUsed,
    expiringSoon,
  };
}

async function generateCredentialsReport(sdk: CredBridgeSDK) {
  const { credentials } = await sdk.credentials.list({ includeDeleted: false });

  console.log('\n📊 Credentials Report');
  console.log('='.repeat(50));

  // 按服务分组统计
  const serviceStats = credentials.reduce((acc, cred) => {
    acc[cred.serviceId] = acc[cred.serviceId] || { count: 0, types: new Set() };
    acc[cred.serviceId].count++;
    acc[cred.serviceId].types.add(cred.credentialType);
    return acc;
  }, {} as Record<string, { count: number; types: Set<string> }>);

  Object.entries(serviceStats).forEach(([service, stats]) => {
    console.log(`\n📁 ${service}`);
    console.log(`   Count: ${stats.count}`);
    console.log(`   Types: ${[...stats.types].join(', ')}`);
  });

  // 即将过期的凭证
  const now = new Date();
  const expiringSoon = credentials.filter((c) => {
    if (!c.expiresAt) return false;
    const expiry = new Date(c.expiresAt);
    return expiry <= new Date(now.getTime() + 7 * 24 * 60 * 60 * 1000);
  });

  if (expiringSoon.length > 0) {
    console.log('\n⚠️  Credentials expiring within 7 days:');
    expiringSoon.forEach((c) => {
      console.log(`   - ${c.credentialId} (${c.serviceId}) expires ${c.expiresAt}`);
    });
  }
}
```

---

## 最佳实践

### 1. Token 安全

```typescript
// ✅ 正确：从环境变量读取 Token
const sdk = new CredBridgeSDK({
  baseUrl: process.env.CREDBRIDGE_URL!,
  token: process.env.CREDBRIDGE_TOKEN!,
});

// ❌ 错误：硬编码 Token
const sdk = new CredBridgeSDK({
  baseUrl: 'https://vault.credbridge.io',
  token: 'v4.local.hardcoded-token',
});
```

### 2. 错误处理

```typescript
// ✅ 正确：使用 instanceof 检查错误类型
try {
  await sdk.credentials.get('id');
} catch (error) {
  if (error instanceof CredBridgeError) {
    // 处理 SDK 错误
  } else {
    // 处理其他错误
  }
}
```

### 3. 资源清理

```typescript
// ✅ 正确：撤销不再使用的 Token
await sdk.token.revoke();

// ✅ 正确：删除不再需要的凭证
await sdk.credentials.delete(credentialId);
```

### 4. 日志记录

```typescript
// ✅ 正确：使用解密理由进行审计
await sdk.credentials.decrypt(id, '用户登录操作 - 2024-01-15 10:30:00');

// ❌ 错误：不提供理由
await sdk.credentials.decrypt(id);
```

---

这些示例涵盖了 SDK 的主要使用场景。更多详细信息请参考 [README.md](./README.md) 和 API 文档。
