# CredBridge TypeScript SDK - 高级示例

> **迁移说明**: 本 SDK 已从 `@credbridge/sdk` 重命名为 `@toani/vault-sdk`，主类从 `CredBridgeSDK` 重命名为 `ToaniVaultSDK`。旧名称 `CredBridgeSDK` 仍然作为兼容性别名保留，但建议使用新名称。`CredBridgeClient`、`CredBridgeError`、`CredBridgeErrorCode` 等类名保持不变。

## 目录

1. [基础凭证操作](#1-基础凭证操作)
2. [批量凭证管理](#2-批量凭证管理)
3. [Token 管理](#3-token-管理)
4. [错误处理与重试](#4-错误处理与重试)
5. [审计日志](#5-审计日志)
6. [Webhook 集成](#6-webhook-集成)
7. [Express 中间件](#7-express-中间件)
8. [React Hook](#8-react-hook)
9. [多租户管理](#9-多租户管理)
10. [自动刷新 Token](#10-自动刷新-token)

---

## 1. 基础凭证操作

### 1.1 创建不同类型的凭证

```typescript
import { CredBridgeClient, CredentialType } from "@toani/vault-sdk";

const client = new CredBridgeClient({
  baseUrl: "https://api.toani.io",
  token: process.env.TOANI_VAULT_TOKEN!,
});

// 创建用户名密码凭证
async function createUserCredentials() {
  const credential = await client.credentials.createUsernamePassword(
    "schwab",
    "user@example.com",
    "SecurePassword123!",
    { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 90 }, // 90天过期
  );
  return credential.credentialId;
}

// 创建 API Key 凭证
async function createApiCredentials() {
  const credential = await client.credentials.createApiKey(
    "stripe",
    "sk_live_51H...",
    "sk_secret_...",
    { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 365 }, // 1年过期
  );
  return credential.credentialId;
}

// 创建 OAuth 刷新令牌
async function createOAuthCredentials() {
  const credential = await client.credentials.createOAuthRefresh(
    "google",
    "1//0dYVjK7V7V7V7V7V7V7V7V7V7V7V...",
    { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 180 }, // 180天过期
  );
  return credential.credentialId;
}

// 创建自定义凭证
async function createCustomCredential() {
  const credential = await client.credentials.create({
    serviceId: "custom-service",
    credentialType: CredentialType.SessionCookie,
    plaintextData: {
      sessionId: "sess_123456",
      csrfToken: "csrf_abcdef",
      userAgent: "Mozilla/5.0...",
    },
    expiresAt: Math.floor(Date.now() / 1000) + 3600, // 1小时过期
  });
  return credential.credentialId;
}
```

### 1.2 凭证生命周期管理

```typescript
class CredentialLifecycleManager {
  constructor(private client: CredBridgeClient) {}

  // 安全获取凭证（带解密）
  async getCredentialSecure(credentialId: string, reason: string) {
    // 先获取元数据
    const metadata = await this.client.credentials.get(credentialId);

    // 检查凭证是否过期
    if (metadata.expiresAt && new Date(metadata.expiresAt) < new Date()) {
      throw new Error("Credential has expired");
    }

    // 解密凭证
    const decrypted = await this.client.credentials.decrypt(
      credentialId,
      reason,
    );

    return {
      metadata,
      data: decrypted.plaintextData,
    };
  }

  // 批量更新过期凭证
  async renewExpiringCredentials(daysBeforeExpiry: number = 7) {
    const { credentials } = await this.client.credentials.list({
      onlyValid: true,
    });
    const now = Date.now();
    const threshold = now + daysBeforeExpiry * 24 * 60 * 60 * 1000;

    const expiringCredentials = credentials.filter((cred) => {
      if (!cred.expiresAt) return false;
      return new Date(cred.expiresAt).getTime() < threshold;
    });

    console.log(
      `Found ${expiringCredentials.length} credentials expiring soon`,
    );

    return expiringCredentials;
  }

  // 安全删除凭证
  async deleteCredentialSecure(credentialId: string) {
    // 验证凭证存在
    const exists = await this.client.credentials.exists(credentialId);
    if (!exists) {
      throw new Error("Credential does not exist");
    }

    // 删除凭证
    const result = await this.client.credentials.delete(credentialId);

    if (result.deleted) {
      console.log(`Credential ${credentialId} deleted successfully`);
    }

    return result;
  }
}
```

---

## 2. 批量凭证管理

```typescript
class BulkCredentialManager {
  constructor(private client: CredBridgeClient) {}

  // 批量创建凭证
  async bulkCreate(
    credentials: Array<{
      serviceId: string;
      type: CredentialType;
      data: Record<string, unknown>;
      expiresAt?: number;
    }>,
  ) {
    const results = [];
    const errors = [];

    for (const cred of credentials) {
      try {
        const result = await this.client.credentials.create({
          serviceId: cred.serviceId,
          credentialType: cred.type,
          plaintextData: cred.data,
          expiresAt: cred.expiresAt,
        });
        results.push({ success: true, credentialId: result.credentialId });
      } catch (error) {
        errors.push({ success: false, error, credential: cred });
      }
    }

    return { results, errors };
  }

  // 批量删除凭证
  async bulkDelete(credentialIds: string[]) {
    const results = await Promise.allSettled(
      credentialIds.map((id) => this.client.credentials.delete(id)),
    );

    return results.map((result, index) => ({
      credentialId: credentialIds[index],
      success: result.status === "fulfilled",
      result: result.status === "fulfilled" ? result.value : undefined,
      error: result.status === "rejected" ? result.reason : undefined,
    }));
  }

  // 批量轮换凭证
  async rotateCredentials(serviceId: string) {
    const { credentials } =
      await this.client.credentials.getByService(serviceId);
    const rotated = [];

    for (const cred of credentials) {
      // 获取并解密旧凭证
      const old = await this.client.credentials.decrypt(
        cred.credentialId,
        "Credential rotation",
      );

      // 创建新凭证
      const newCred = await this.client.credentials.create({
        serviceId,
        credentialType: cred.credentialType,
        plaintextData: old.plaintextData,
        expiresAt: Math.floor(Date.now() / 1000) + 86400 * 90, // 新凭证90天过期
      });

      // 删除旧凭证
      await this.client.credentials.delete(cred.credentialId);

      rotated.push({
        oldId: cred.credentialId,
        newId: newCred.credentialId,
      });
    }

    return rotated;
  }
}
```

---

## 3. Token 管理

```typescript
class TokenManager {
  private client: CredBridgeClient;

  constructor(config: { baseUrl: string; token: string }) {
    this.client = new CredBridgeClient(config);
    this.setupTokenMonitoring();
  }

  // 设置 Token 监控
  private setupTokenMonitoring() {
    // Token 即将过期提醒
    this.client.on("token_expiring", () => {
      console.warn("Token is expiring soon, refreshing...");
    });

    // Token 刷新成功
    this.client.on("token_refreshed", (event) => {
      console.log("Token refreshed successfully");
      // 更新环境变量或配置文件
      process.env.TOANI_VAULT_TOKEN = event.data.token;
    });
  }

  // 检查并刷新 Token
  async checkAndRefreshToken() {
    const token = this.client.token;

    // 检查 Token 是否有效
    if (!token.isValid()) {
      throw new Error("Token is invalid or expired");
    }

    // 检查 Token 是否即将过期
    if (token.isExpiringSoon(600)) {
      // 10分钟内过期
      console.log(`Token expires in ${token.getRemainingTimeFormatted()}`);

      // 这里可以实现实际的 Token 刷新逻辑
      // const newToken = await this.refreshToken();
      // this.client.setToken(newToken);
    }

    // 本地检查 Token 是否仍在有效期内
    const isValid = token.isValid();
    if (!isValid) {
      throw new Error("Token has been revoked");
    }

    return {
      isValid: true,
      expiresIn: token.getRemainingTime(),
      expiresFormatted: token.getRemainingTimeFormatted(),
      scopes: token.getScopes(),
    };
  }

  // 检查权限
  checkPermissions(requiredScopes: string[]) {
    const token = this.client.token;

    const hasAll = token.hasAllScopes(requiredScopes as any);
    const hasAny = token.hasAnyScope(requiredScopes as any);
    const missing = requiredScopes.filter((s) => !token.hasScope(s as any));

    return {
      hasAll,
      hasAny,
      missing,
      granted: token.getScopes(),
    };
  }
}
```

---

## 4. 错误处理与重试

```typescript
import { CredBridgeError, CredBridgeErrorCode } from "@toani/vault-sdk";

class SafeCredentialClient {
  constructor(private client: CredBridgeClient) {}

  // 带重试的凭证操作
  async withRetry<T>(
    operation: () => Promise<T>,
    maxRetries: number = 3,
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

          // 认证错误需要特殊处理
          if (error.isAuthError()) {
            console.error("Authentication error, please check your token");
            throw error;
          }
        }

        // 指数退避
        const delay = Math.pow(2, i) * 1000;
        console.log(`Retry ${i + 1}/${maxRetries} after ${delay}ms`);
        await new Promise((resolve) => setTimeout(resolve, delay));
      }
    }

    throw lastError;
  }

  // 处理特定错误
  handleCredentialError(error: unknown): {
    success: false;
    error: string;
    code: string;
  } {
    if (error instanceof CredBridgeError) {
      switch (error.code) {
        case CredBridgeErrorCode.NotFound:
          return { success: false, error: "凭证不存在", code: "NOT_FOUND" };
        case CredBridgeErrorCode.Unauthorized:
          return { success: false, error: "未授权访问", code: "UNAUTHORIZED" };
        case CredBridgeErrorCode.Forbidden:
          return { success: false, error: "禁止访问", code: "FORBIDDEN" };
        case CredBridgeErrorCode.InsufficientScope:
          return {
            success: false,
            error: "权限不足",
            code: "INSUFFICIENT_SCOPE",
          };
        case CredBridgeErrorCode.TokenExpired:
          return {
            success: false,
            error: "Token 已过期",
            code: "TOKEN_EXPIRED",
          };
        case CredBridgeErrorCode.CredentialExpired:
          return {
            success: false,
            error: "凭证已过期",
            code: "CREDENTIAL_EXPIRED",
          };
        case CredBridgeErrorCode.NetworkError:
        case CredBridgeErrorCode.Timeout:
          return {
            success: false,
            error: "网络错误，请稍后重试",
            code: "NETWORK_ERROR",
          };
        default:
          return { success: false, error: error.message, code: error.code };
      }
    }

    return {
      success: false,
      error: error instanceof Error ? error.message : "未知错误",
      code: "UNKNOWN",
    };
  }

  // 安全获取凭证
  async safeGetCredential(credentialId: string) {
    try {
      const credential = await this.withRetry(() =>
        this.client.credentials.get(credentialId),
      );
      return { success: true, data: credential };
    } catch (error) {
      return this.handleCredentialError(error);
    }
  }

  // 安全解密凭证
  async safeDecryptCredential(credentialId: string, reason: string) {
    try {
      const decrypted = await this.withRetry(() =>
        this.client.credentials.decrypt(credentialId, reason),
      );
      return { success: true, data: decrypted };
    } catch (error) {
      return this.handleCredentialError(error);
    }
  }
}
```

---

## 5. 审计日志

```typescript
class AuditLogger {
  constructor(private client: CredBridgeClient) {}

  // 记录凭证访问
  async logCredentialAccess(
    credentialId: string,
    action: "read" | "decrypt" | "create" | "delete",
    success: boolean,
    metadata?: Record<string, unknown>,
  ) {
    // 这里可以实现自定义审计日志逻辑
    // 例如发送到日志服务或数据仓库
    console.log(
      `[AUDIT] ${action} credential ${credentialId}: ${success ? "success" : "failed"}`,
      metadata,
    );
  }

  // 获取凭证使用统计
  async getCredentialUsageStats(credentialId: string, days: number = 30) {
    // 实现从审计日志服务获取统计信息
    // 这里仅为示例
    return {
      credentialId,
      period: `${days} days`,
      accessCount: 42,
      decryptCount: 15,
      lastAccessed: new Date().toISOString(),
    };
  }
}
```

---

## 6. Webhook 集成

```typescript
import express from "express";
import crypto from "crypto";

class ToaniVaultWebhookHandler {
  constructor(private webhookSecret: string) {}

  // 验证 Webhook 签名
  verifySignature(payload: string, signature: string): boolean {
    const expectedSignature = crypto
      .createHmac("sha256", this.webhookSecret)
      .update(payload)
      .digest("hex");

    return crypto.timingSafeEqual(
      Buffer.from(signature),
      Buffer.from(`sha256=${expectedSignature}`),
    );
  }

  // 处理 Webhook 事件
  async handleEvent(event: { type: string; data: unknown; timestamp: string }) {
    switch (event.type) {
      case "credential.created":
        await this.handleCredentialCreated(event.data);
        break;
      case "credential.decrypted":
        await this.handleCredentialDecrypted(event.data);
        break;
      case "credential.deleted":
        await this.handleCredentialDeleted(event.data);
        break;
      case "token.revoked":
        await this.handleTokenRevoked(event.data);
        break;
      default:
        console.log(`Unhandled event type: ${event.type}`);
    }
  }

  private async handleCredentialCreated(data: any) {
    console.log("Credential created:", data.credentialId);
    // 发送通知或更新缓存
  }

  private async handleCredentialDecrypted(data: any) {
    console.log("Credential decrypted:", data.credentialId, "by", data.userId);
    // 记录审计日志或发送安全警报
  }

  private async handleCredentialDeleted(data: any) {
    console.log("Credential deleted:", data.credentialId);
    // 清理相关资源
  }

  private async handleTokenRevoked(data: any) {
    console.log("Token revoked:", data.tokenId);
    // 清除会话或通知用户
  }
}

// Express 中间件
export function createWebhookMiddleware(webhookSecret: string) {
  const handler = new ToaniVaultWebhookHandler(webhookSecret);

  return express.json({
    verify: (req: any, _res, buf) => {
      req.rawBody = buf.toString();
    },
  });
}
```

---

## 7. Express 中间件

```typescript
import { Request, Response, NextFunction } from "express";
import {
  CredBridgeClient,
  CredBridgeError,
  CredBridgeErrorCode,
} from "@toani/vault-sdk";

// 扩展 Express Request 类型
declare global {
  namespace Express {
    interface Request {
      toaniVault?: CredBridgeClient;
      tenantId?: string;
      userId?: string;
    }
  }
}

// 初始化 Toani Vault 客户端中间件
export function initToaniVault(config: {
  baseUrl: string;
  getToken: (req: Request) => string | undefined;
}) {
  return (req: Request, _res: Response, next: NextFunction) => {
    const token = config.getToken(req);

    if (!token) {
      return next(new Error("Missing Toani Vault token"));
    }

    req.toaniVault = new CredBridgeClient({
      baseUrl: config.baseUrl,
      token,
    });

    // 从 Token 中提取租户和用户 ID
    const tokenInfo = req.toaniVault.getTokenInfo();
    if (tokenInfo) {
      req.tenantId = tokenInfo.tenantId;
      req.userId = tokenInfo.userId;
    }

    next();
  };
}

// 权限检查中间件
export function requireScopes(...scopes: string[]) {
  return (req: Request, res: Response, next: NextFunction) => {
    if (!req.toaniVault) {
      return res
        .status(500)
        .json({ error: "Toani Vault client not initialized" });
    }

    const token = req.toaniVault.token;
    const hasScopes = token.hasAllScopes(scopes as any);

    if (!hasScopes) {
      return res.status(403).json({
        error: "Insufficient permissions",
        required: scopes,
        granted: token.getScopes(),
      });
    }

    next();
  };
}

// 错误处理中间件
export function toaniVaultErrorHandler(
  err: Error,
  _req: Request,
  res: Response,
  _next: NextFunction,
) {
  if (err instanceof CredBridgeError) {
    const statusMap: Record<CredBridgeErrorCode, number> = {
      [CredBridgeErrorCode.Unauthorized]: 401,
      [CredBridgeErrorCode.Forbidden]: 403,
      [CredBridgeErrorCode.NotFound]: 404,
      [CredBridgeErrorCode.InvalidRequest]: 400,
      [CredBridgeErrorCode.InternalError]: 500,
      [CredBridgeErrorCode.TokenExpired]: 401,
      [CredBridgeErrorCode.InsufficientScope]: 403,
      [CredBridgeErrorCode.CredentialExpired]: 410,
      [CredBridgeErrorCode.NetworkError]: 503,
      [CredBridgeErrorCode.Timeout]: 504,
      // 其他错误码映射为 500
    };

    const statusCode = statusMap[err.code] || 500;

    return res.status(statusCode).json({
      error: err.message,
      code: err.code,
      requestId: err.requestId,
      details: err.details,
    });
  }

  res.status(500).json({ error: err.message });
}

// 使用示例
/*
import express from 'express';

const app = express();

app.use(initToaniVault({
  baseUrl: 'https://api.toani.io',
  getToken: (req) => req.headers.authorization?.replace('Bearer ', ''),
}));

// 需要特定权限的路由
app.get('/api/credentials',
  requireScopes('credential:read'),
  async (req, res) => {
    const { credentials } = await req.toaniVault!.credentials.list();
    res.json(credentials);
  }
);

app.post('/api/credentials',
  requireScopes('credential:write'),
  async (req, res) => {
    const credential = await req.toaniVault!.credentials.create(req.body);
    res.json(credential);
  }
);

app.use(toaniVaultErrorHandler);
*/
```

---

## 8. React Hook

```typescript
import { useState, useEffect, useCallback } from "react";
import { CredBridgeClient, CredentialType } from "@toani/vault-sdk";

interface UseCredentialsOptions {
  baseUrl: string;
  token: string;
  serviceId?: string;
}

export function useCredentials({
  baseUrl,
  token,
  serviceId,
}: UseCredentialsOptions) {
  const [client] = useState(() => new CredBridgeClient({ baseUrl, token }));
  const [credentials, setCredentials] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  // 加载凭证列表
  const loadCredentials = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await client.credentials.list(
        serviceId ? { serviceId } : undefined,
      );
      setCredentials(result.credentials);
    } catch (err) {
      setError(err as Error);
    } finally {
      setLoading(false);
    }
  }, [client, serviceId]);

  // 创建凭证
  const createCredential = useCallback(
    async (
      type: CredentialType,
      data: Record<string, unknown>,
      expiresAt?: number,
    ) => {
      const result = await client.credentials.create({
        serviceId: serviceId || "default",
        credentialType: type,
        plaintextData: data,
        expiresAt,
      });
      await loadCredentials();
      return result;
    },
    [client, serviceId, loadCredentials],
  );

  // 解密凭证
  const decryptCredential = useCallback(
    async (credentialId: string, reason: string) => {
      return client.credentials.decrypt(credentialId, reason);
    },
    [client],
  );

  // 删除凭证
  const deleteCredential = useCallback(
    async (credentialId: string) => {
      await client.credentials.delete(credentialId);
      await loadCredentials();
    },
    [client, loadCredentials],
  );

  useEffect(() => {
    loadCredentials();
  }, [loadCredentials]);

  return {
    credentials,
    loading,
    error,
    refresh: loadCredentials,
    create: createCredential,
    decrypt: decryptCredential,
    delete: deleteCredential,
  };
}

// 使用示例
/*
function CredentialManager() {
  const { credentials, loading, create, delete: deleteCred } = useCredentials({
    baseUrl: 'https://api.toani.io',
    token: 'your-token',
    serviceId: 'schwab',
  });

  if (loading) return <div>Loading...</div>;

  return (
    <div>
      {credentials.map(cred => (
        <div key={cred.credentialId}>
          {cred.serviceId} - {cred.credentialType}
          <button onClick={() => deleteCred(cred.credentialId)}>Delete</button>
        </div>
      ))}
    </div>
  );
}
*/
```

---

## 9. 多租户管理

```typescript
class MultiTenantCredentialManager {
  private clients: Map<string, CredBridgeClient> = new Map();

  constructor(
    private baseUrl: string,
    private getTenantToken: (tenantId: string) => string,
  ) {}

  // 获取或创建租户客户端
  private getClient(tenantId: string): CredBridgeClient {
    if (!this.clients.has(tenantId)) {
      const token = this.getTenantToken(tenantId);
      const client = new CredBridgeClient({
        baseUrl: this.baseUrl,
        token,
      });
      this.clients.set(tenantId, client);
    }
    return this.clients.get(tenantId)!;
  }

  // 跨租户操作
  async getAllCredentials(tenantIds: string[]) {
    const results = await Promise.allSettled(
      tenantIds.map(async (tenantId) => {
        const client = this.getClient(tenantId);
        const { credentials } = await client.credentials.list();
        return { tenantId, credentials };
      }),
    );

    return results.map((result, index) => ({
      tenantId: tenantIds[index],
      success: result.status === "fulfilled",
      data:
        result.status === "fulfilled" ? result.value.credentials : undefined,
      error: result.status === "rejected" ? result.reason : undefined,
    }));
  }

  // 租户隔离验证
  async validateTenantAccess(
    tenantId: string,
    credentialId: string,
  ): Promise<boolean> {
    try {
      const client = this.getClient(tenantId);
      const credential = await client.credentials.get(credentialId);
      return credential.tenantId === tenantId;
    } catch {
      return false;
    }
  }
}
```

---

## 10. 自动刷新 Token

```typescript
class AutoRefreshTokenClient {
  private refreshTimer?: NodeJS.Timeout;
  private isRefreshing: boolean = false;

  constructor(
    private client: CredBridgeClient,
    private refreshCallback: () => Promise<string>,
  ) {
    this.startAutoRefresh();
  }

  // 启动自动刷新
  private startAutoRefresh() {
    // 每分钟检查一次 Token 状态
    this.refreshTimer = setInterval(() => {
      this.checkAndRefresh();
    }, 60000);
  }

  // 停止自动刷新
  stopAutoRefresh() {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = undefined;
    }
  }

  // 检查并刷新 Token
  private async checkAndRefresh() {
    if (this.isRefreshing) return;

    const token = this.client.token;

    // 如果 Token 将在 5 分钟内过期，触发刷新
    if (token.isExpiringSoon(300)) {
      this.isRefreshing = true;
      try {
        console.log("Refreshing token...");
        const newToken = await this.refreshCallback();
        this.client.setToken(newToken);
        console.log("Token refreshed successfully");
      } catch (error) {
        console.error("Failed to refresh token:", error);
      } finally {
        this.isRefreshing = false;
      }
    }
  }

  // 手动刷新 Token
  async refreshToken(): Promise<string> {
    if (this.isRefreshing) {
      throw new Error("Token refresh already in progress");
    }

    this.isRefreshing = true;
    try {
      const newToken = await this.refreshCallback();
      this.client.setToken(newToken);
      return newToken;
    } finally {
      this.isRefreshing = false;
    }
  }
}

// 使用示例
/*
const client = new CredBridgeClient({
  baseUrl: 'https://api.toani.io',
  token: initialToken,
});

const autoRefresh = new AutoRefreshTokenClient(
  client,
  async () => {
    // 调用你的 Token 刷新端点
    const response = await fetch('/api/refresh-token', {
      method: 'POST',
      headers: { Authorization: `Bearer ${client.getToken()}` },
    });
    const { token } = await response.json();
    return token;
  }
);

// 清理时停止自动刷新
// autoRefresh.stopAutoRefresh();
*/
```
