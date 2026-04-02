# CredBridge Sandbox SDK 使用指南

> **迁移注意**: SDK 包已从 `@credbridge/sdk` 重命名为 `@toani/vault-sdk`，旧包名已弃用。请将 `import { CredBridgeSDK }` 改为 `import { ToaniVaultSDK }`。

**版本**: v1.0
**最后更新**: 2026-03-17
**适用版本**: @toani/vault-sdk >= 0.1.0

---

## 目录

1. [快速开始](#快速开始)
2. [核心概念](#核心概念)
3. [会话管理](#会话管理)
4. [执行操作](#执行操作)
5. [WebSocket 实时连接](#websocket-实时连接)
6. [安全导出](#安全导出)
7. [最佳实践](#最佳实践)
8. [错误处理](#错误处理)
9. [API 参考](#api-参考)

---

## 快速开始

### 安装

```bash
npm install @toani/vault-sdk
# 或
yarn add @toani/vault-sdk
# 或
pnpm add @toani/vault-sdk
```

### 初始化 SDK

```typescript
import { ToaniVaultSDK } from '@toani/vault-sdk';

const sdk = new ToaniVaultSDK({
  baseUrl: 'https://api.toani.io',
  token: 'v4.local.your-paseto-token',
});
```

### 完整示例：自动化登录并获取数据

```typescript
import { ToaniVaultSDK, OperationType } from '@toani/vault-sdk';

const sdk = new ToaniVaultSDK({
  baseUrl: 'https://api.toani.io',
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

    // 3. 填写凭证并登录
    await sdk.sandbox.fill(sessionId, '#username', 'user@example.com');
    await sdk.sandbox.fill(sessionId, '#password', 'your-password');
    await sdk.sandbox.click(sessionId, '#login-button');

    // 4. 等待页面加载完成
    await sdk.sandbox.waitForSelector(sessionId, '.portfolio-summary', {
      timeout: 30000,
    });

    // 5. 获取投资组合余额
    const balanceResult = await sdk.sandbox.getText(sessionId, '.total-balance');
    console.log('Balance:', balanceResult.result);

    // 6. 截图保存
    const screenshot = await sdk.sandbox.takeScreenshot(sessionId, {
      type: 'png',
      fullPage: true,
    });

    // 保存截图到文件
    const fs = require('fs');
    fs.writeFileSync('portfolio.png', Buffer.from(screenshot.data, 'base64'));

  } finally {
    // 7. 关闭会话（确保资源释放）
    await sdk.sandbox.closeSession(sessionId);
  }
}

automateTask().catch(console.error);
```

---

## 核心概念

### 什么是 Sandbox

Sandbox 是 CredBridge 提供的 TEE（可信执行环境）安全浏览器自动化服务。它允许 AI Agent 在隔离环境中执行网页操作，而无需暴露用户的真实凭证。

```
┌─────────────────────────────────────────────────────────────┐
│                    用户应用层                                │
│              (你的 Node.js/TypeScript 应用)                  │
└──────────────────────────┬──────────────────────────────────┘
                           │ SDK API 调用
┌──────────────────────────▼──────────────────────────────────┐
│              CredBridge Gateway 层                           │
│         (API 认证、请求路由、审计日志)                        │
└──────────────────────────┬──────────────────────────────────┘
                           │ 安全通道
┌──────────────────────────▼──────────────────────────────────┐
│              TEE Sandbox 层 (Intel SGX)                      │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐    │
│  │ 浏览器实例   │  │ 凭证解密     │  │ 自动化执行     │    │
│  │ (Chromium)   │  │ (AES-256)    │  │ (Playwright)    │    │
│  └──────────────┘  └──────────────┘  └─────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### 会话生命周期

```
Creating → Running → [Paused] → Closed
              ↓
           Error
```

- **Creating**: 会话正在初始化
- **Running**: 会话正常运行，可以执行操作
- **Paused**: 会话已暂停（可恢复）
- **Closed**: 会话已关闭，资源已释放
- **Error**: 会话发生错误

### 操作类型

| 操作类型 | 说明 | 使用场景 |
|---------|------|---------|
| `navigate` | 导航到 URL | 页面跳转 |
| `click` | 点击元素 | 按钮点击、链接跳转 |
| `fill` | 填充表单 | 输入用户名、密码等 |
| `get_text` | 获取元素文本 | 提取页面数据 |
| `get_attribute` | 获取元素属性 | 获取链接 href、图片 src 等 |
| `execute_script` | 执行 JavaScript | 复杂页面交互 |
| `wait_for_selector` | 等待元素出现 | 等待页面加载完成 |
| `screenshot` | 截图 | 保存页面状态 |
| `export_data` | 导出数据 | 批量提取结构化数据 |

---

## 会话管理

### 创建会话

```typescript
import { SessionStatus } from '@toani/vault-sdk';

// 基础创建
const session = await sdk.sandbox.createSession({
  serviceId: 'schwab',
  credentialId: 'cred-123',
});

console.log('Session ID:', session.sessionId);
console.log('Status:', session.status); // 'creating'

// 高级配置
const sessionWithConfig = await sdk.sandbox.createSession({
  serviceId: 'schwab',
  credentialId: 'cred-123',
  startUrl: 'https://www.schwab.com',
  viewportWidth: 1920,
  viewportHeight: 1080,
  userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36',
  timeout: 60000,
});
```

### 获取会话信息

```typescript
// 获取单个会话
const sessionInfo = await sdk.sandbox.getSession(sessionId);
console.log('Status:', sessionInfo.status);
console.log('Current URL:', sessionInfo.currentUrl);
console.log('Page Title:', sessionInfo.pageTitle);

// 列出所有会话
const { sessions, total } = await sdk.sandbox.listSessions();
console.log(`Total sessions: ${total}`);
sessions.forEach(s => {
  console.log(`${s.sessionId}: ${s.status} - ${s.currentUrl}`);
});
```

### 会话状态管理

```typescript
// 等待会话就绪
await sdk.sandbox.waitForStatus(sessionId, SessionStatus.Running, {
  timeout: 30000,
  interval: 1000,
});

// 暂停会话
await sdk.sandbox.pauseSession(sessionId);

// 恢复会话
await sdk.sandbox.resumeSession(sessionId);

// 检查会话是否存在
const exists = await sdk.sandbox.exists(sessionId);
console.log('Session exists:', exists);
```

### 关闭会话

```typescript
// 正常关闭
await sdk.sandbox.closeSession(sessionId);

// 使用 try-finally 确保会话关闭
const session = await sdk.sandbox.createSession({
  serviceId: 'schwab',
  credentialId: 'cred-123',
});

try {
  // 执行操作...
} finally {
  // 确保会话关闭，释放资源
  await sdk.sandbox.closeSession(session.sessionId);
}
```

---

## 执行操作

### 导航操作

```typescript
// 导航到指定 URL
await sdk.sandbox.navigate(sessionId, 'https://example.com');

// 使用底层 API 执行导航
await sdk.sandbox.executeOperation(sessionId, {
  operationType: OperationType.Navigate,
  url: 'https://example.com',
});
```

### 元素交互

```typescript
// 点击元素
await sdk.sandbox.click(sessionId, '#submit-button');

// 填充表单
await sdk.sandbox.fill(sessionId, '#username', 'user@example.com');
await sdk.sandbox.fill(sessionId, '#password', 'secret-password');

// 获取元素文本
const result = await sdk.sandbox.getText(sessionId, '.price-display');
console.log('Price:', result.result);

// 获取元素属性
const linkResult = await sdk.sandbox.getAttribute(
  sessionId,
  'a.download-link',
  'href'
);
console.log('Download URL:', linkResult.result);
```

### 执行 JavaScript

```typescript
// 执行自定义脚本
const scriptResult = await sdk.sandbox.executeScript(sessionId, `
  // 获取页面数据
  const data = {
    title: document.title,
    url: window.location.href,
    timestamp: new Date().toISOString(),
  };

  // 获取表格数据
  const rows = document.querySelectorAll('table.data tr');
  data.rows = Array.from(rows).map(row => {
    const cells = row.querySelectorAll('td');
    return {
      symbol: cells[0]?.textContent,
      price: cells[1]?.textContent,
    };
  });

  return data;
`);

console.log('Script result:', scriptResult.result);
```

### 等待操作

```typescript
// 等待元素出现
await sdk.sandbox.waitForSelector(sessionId, '.loading-complete', {
  timeout: 30000,
});

// 等待元素可见
await sdk.sandbox.waitForSelector(sessionId, '.modal-dialog', {
  timeout: 10000,
  visible: true,
});

// 使用底层 API 等待
await sdk.sandbox.executeOperation(sessionId, {
  operationType: OperationType.WaitForSelector,
  selector: '.data-loaded',
  timeout: 30000,
  waitCondition: { visible: true },
});
```

### 截图操作

```typescript
// 截取完整页面
const fullPage = await sdk.sandbox.takeScreenshot(sessionId, {
  type: 'png',
  fullPage: true,
});

// 截取特定元素
const elementShot = await sdk.sandbox.takeScreenshot(sessionId, {
  selector: '#chart-container',
  type: 'jpeg',
  quality: 90,
});

// 截取指定区域
const clippedShot = await sdk.sandbox.takeScreenshot(sessionId, {
  type: 'png',
  clip: {
    x: 100,
    y: 100,
    width: 800,
    height: 600,
  },
});

// 保存截图
const fs = require('fs');
fs.writeFileSync('screenshot.png', Buffer.from(fullPage.data, 'base64'));
```

---

## WebSocket 实时连接

WebSocket 连接提供实时控制和监控能力，适用于需要即时反馈的场景。

### 基础用法

```typescript
import { SandboxWebSocketClient, WebSocketState } from '@toani/vault-sdk';

// 创建 WebSocket 客户端
const ws = new SandboxWebSocketClient({
  baseUrl: 'https://api.toani.io',
  token: 'v4.local.your-paseto-token',
  sessionId: 'session-uuid',
  credentialId: 'credential-uuid',
  heartbeatInterval: 30000,      // 心跳间隔（毫秒）
  operationTimeout: 60000,       // 操作超时（毫秒）
  autoReconnect: true,           // 自动重连
  maxReconnectAttempts: 3,       // 最大重连次数
  reconnectDelay: 5000,          // 重连延迟（毫秒）
});

// 设置事件回调
ws.onConnected = (data) => {
  console.log('Connected:', data.session_id);
};

ws.onDisconnected = (code, reason) => {
  console.log('Disconnected:', code, reason);
};

ws.onOperationProgress = (data) => {
  console.log(`Progress: ${data.progress}% - ${data.message}`);
};

ws.onOperationCompleted = (data) => {
  console.log('Operation completed:', data.success);
};

ws.onError = (error) => {
  console.error('Error:', error.code, error.message);
};

ws.onReconnecting = (attempt, maxAttempts) => {
  console.log(`Reconnecting... ${attempt}/${maxAttempts}`);
};

// 连接
await ws.connect();

// 检查连接状态
console.log('Connected:', ws.isConnected());
console.log('State:', ws.getState()); // 'connected'
```

### 执行操作

```typescript
// 执行导航操作
const result = await ws.executeOperation({
  operationType: 'navigate',
  description: 'Navigate to example.com',
  parameters: { url: 'https://example.com' },
});

console.log('Navigation result:', result.success);

// 执行点击操作
const clickResult = await ws.executeOperation({
  operationType: 'click',
  description: 'Click login button',
  parameters: { selector: '#login-button' },
  timeout: 10000,
});
```

### 实时截图

```typescript
// 请求截图
const screenshot = await ws.requestScreenshot(30000);

if (screenshot.success) {
  console.log('Screenshot format:', screenshot.format);
  console.log('Image data length:', screenshot.imageData?.length);

  // 保存截图
  const fs = require('fs');
  fs.writeFileSync('live-screenshot.png', Buffer.from(screenshot.imageData!, 'base64'));
}
```

### 断开连接

```typescript
// 正常断开
ws.disconnect('Task completed');

// 检查状态
console.log('State:', ws.getState()); // 'closed'
```

### 完整 WebSocket 示例

```typescript
import { SandboxWebSocketClient } from '@toani/vault-sdk';

async function executeWithWebSocket(sessionId: string, credentialId: string) {
  const ws = new SandboxWebSocketClient({
    baseUrl: 'https://api.toani.io',
    token: 'v4.local.your-token',
    sessionId,
    credentialId,
  });

  // 设置事件处理
  ws.onConnected = () => console.log('WebSocket connected');
  ws.onOperationProgress = (data) => {
    console.log(`[${data.operation_type}] ${data.progress}%: ${data.message}`);
  };
  ws.onError = (error) => console.error('WebSocket error:', error);

  try {
    await ws.connect();

    // 执行一系列操作
    await ws.executeOperation({
      operationType: 'navigate',
      description: 'Go to login page',
      parameters: { url: 'https://example.com/login' },
    });

    await ws.executeOperation({
      operationType: 'fill',
      description: 'Enter username',
      parameters: { selector: '#username', value: 'user@example.com' },
    });

    await ws.executeOperation({
      operationType: 'click',
      description: 'Click login',
      parameters: { selector: '#login-button' },
    });

    // 获取截图
    const screenshot = await ws.requestScreenshot();
    if (screenshot.success) {
      console.log('Screenshot captured');
    }

  } finally {
    ws.disconnect();
  }
}
```

---

## 安全导出

### 导出页面数据

```typescript
// 导出表格数据为 JSON
const exportResult = await sdk.sandbox.exportData(sessionId, {
  format: 'json',
  selector: '.portfolio-table',
  extractionRules: [
    { name: 'symbol', selector: '.symbol-cell' },
    { name: 'quantity', selector: '.quantity-cell' },
    { name: 'price', selector: '.price-cell' },
    { name: 'value', selector: '.value-cell' },
  ],
});

console.log('Exported data:', exportResult.data);
console.log('Record count:', exportResult.recordCount);

// 导出为 CSV
const csvResult = await sdk.sandbox.exportData(sessionId, {
  format: 'csv',
  selector: '.transactions-table',
  extractionRules: [
    { name: 'date', selector: 'td:nth-child(1)' },
    { name: 'type', selector: 'td:nth-child(2)' },
    { name: 'amount', selector: 'td:nth-child(3)' },
    { name: 'description', selector: 'td:nth-child(4)' },
  ],
});

// 保存 CSV 文件
const fs = require('fs');
fs.writeFileSync('transactions.csv', csvResult.data as string);
```

### 完整数据提取示例

```typescript
async function extractPortfolioData(sessionId: string) {
  // 等待数据加载
  await sdk.sandbox.waitForSelector(sessionId, '.portfolio-loaded', {
    timeout: 30000,
  });

  // 提取投资组合摘要
  const summary = await sdk.sandbox.executeScript(sessionId, `
    return {
      totalValue: document.querySelector('.total-value')?.textContent?.trim(),
      dayChange: document.querySelector('.day-change')?.textContent?.trim(),
      dayChangePercent: document.querySelector('.day-change-percent')?.textContent?.trim(),
    };
  `);

  // 导出持仓明细
  const positions = await sdk.sandbox.exportData(sessionId, {
    format: 'json',
    selector: '.positions-table tbody tr',
    extractionRules: [
      { name: 'symbol', selector: 'td:nth-child(1)' },
      { name: 'name', selector: 'td:nth-child(2)' },
      { name: 'quantity', selector: 'td:nth-child(3)' },
      { name: 'price', selector: 'td:nth-child(4)' },
      { name: 'value', selector: 'td:nth-child(5)' },
      { name: 'weight', selector: 'td:nth-child(6)' },
    ],
  });

  return {
    summary: summary.result,
    positions: positions.data,
    timestamp: new Date().toISOString(),
  };
}
```

---

## 最佳实践

### 1. 始终使用 try-finally 确保会话关闭

```typescript
const session = await sdk.sandbox.createSession({
  serviceId: 'schwab',
  credentialId: 'cred-123',
});

try {
  // 执行操作...
  await sdk.sandbox.navigate(sessionId, 'https://example.com');
  // ...
} finally {
  // 确保会话关闭，释放 TEE 资源
  await sdk.sandbox.closeSession(session.sessionId);
}
```

### 2. 使用 WebSocket 监控长时间操作

```typescript
const ws = new SandboxWebSocketClient({
  baseUrl: 'https://api.toani.io',
  token: 'v4.local.your-token',
  sessionId,
  credentialId,
});

ws.onOperationProgress = (data) => {
  console.log(`Progress: ${data.progress}%`);
  // 可以在这里更新 UI 进度条
};

await ws.connect();
```

### 3. 合理设置超时时间

```typescript
// 根据操作复杂度设置合理的超时
await sdk.sandbox.waitForSelector(sessionId, '.slow-loading-element', {
  timeout: 60000, // 复杂页面可能需要更长时间
});

// 简单操作可以使用较短的超时
await sdk.sandbox.click(sessionId, '#quick-button', {
  timeout: 5000,
});
```

### 4. 处理所有错误情况

```typescript
import { CredBridgeError, CredBridgeErrorCode } from '@toani/vault-sdk';

try {
  await sdk.sandbox.click(sessionId, '#button');
} catch (error) {
  if (error instanceof CredBridgeError) {
    switch (error.code) {
      case CredBridgeErrorCode.NotFound:
        console.log('Element not found');
        break;
      case CredBridgeErrorCode.Timeout:
        console.log('Operation timed out');
        break;
      case CredBridgeErrorCode.Unauthorized:
        console.log('Session expired, need to re-authenticate');
        break;
      default:
        console.error('Sandbox error:', error.message);
    }
  }
}
```

### 5. 及时导出需要的数据

```typescript
// 在会话关闭前导出所有需要的数据
const data = await sdk.sandbox.exportData(sessionId, {
  format: 'json',
  selector: '.data-container',
  extractionRules: [...],
});

// 保存数据后再关闭会话
await sdk.sandbox.closeSession(sessionId);
```

### 6. 使用会话池管理多个会话

```typescript
class SandboxSessionPool {
  private sessions: Map<string, string> = new Map();
  private sdk: ToaniVaultSDK;

  constructor(sdk: ToaniVaultSDK) {
    this.sdk = sdk;
  }

  async acquire(serviceId: string, credentialId: string): Promise<string> {
    const key = `${serviceId}:${credentialId}`;

    if (this.sessions.has(key)) {
      const sessionId = this.sessions.get(key)!;
      const exists = await this.sdk.sandbox.exists(sessionId);
      if (exists) return sessionId;
    }

    const session = await this.sdk.sandbox.createSession({
      serviceId,
      credentialId,
    });

    this.sessions.set(key, session.sessionId);
    return session.sessionId;
  }

  async releaseAll(): Promise<void> {
    for (const [key, sessionId] of this.sessions) {
      try {
        await this.sdk.sandbox.closeSession(sessionId);
      } catch (error) {
        console.error(`Failed to close session ${key}:`, error);
      }
    }
    this.sessions.clear();
  }
}
```

---

## 错误处理

### 错误类型

```typescript
import { CredBridgeError, CredBridgeErrorCode } from '@toani/vault-sdk';

try {
  await sdk.sandbox.click(sessionId, '#button');
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

    if (error.isNetworkError()) {
      console.log('Network error, will retry');
    }

    if (error.isRetryable()) {
      console.log('Error is retryable');
    }
  }
}
```

### 常见错误码

| 错误码 | 说明 | 处理建议 |
|-------|------|---------|
| `not_found` | 会话或元素不存在 | 检查会话 ID 或选择器 |
| `timeout` | 操作超时 | 增加超时时间或检查页面状态 |
| `unauthorized` | 未授权 | 检查 Token 是否有效 |
| `invalid_request` | 请求参数错误 | 检查请求参数 |
| `internal_error` | 服务器内部错误 | 稍后重试或联系支持 |

### 重试策略

```typescript
async function executeWithRetry<T>(
  operation: () => Promise<T>,
  maxRetries = 3
): Promise<T> {
  let lastError: Error | undefined;

  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error as Error;

      if (error instanceof CredBridgeError) {
        // 不重试认证错误
        if (error.isAuthError()) {
          throw error;
        }

        // 不重试客户端错误
        if (error.statusCode && error.statusCode >= 400 && error.statusCode < 500) {
          throw error;
        }
      }

      if (attempt < maxRetries) {
        const delay = Math.pow(2, attempt) * 1000; // 指数退避
        console.log(`Retry ${attempt}/${maxRetries} after ${delay}ms`);
        await new Promise(resolve => setTimeout(resolve, delay));
      }
    }
  }

  throw lastError;
}

// 使用
await executeWithRetry(async () => {
  await sdk.sandbox.click(sessionId, '#unstable-button');
});
```

---

## API 参考

### SandboxService

#### 会话管理

| 方法 | 说明 | 返回值 |
|-----|------|-------|
| `createSession(request)` | 创建新会话 | `CreateSessionResponse` |
| `getSession(sessionId)` | 获取会话信息 | `SessionInfo` |
| `listSessions()` | 列出所有会话 | `{ sessions: SessionInfo[], total: number }` |
| `pauseSession(sessionId)` | 暂停会话 | `SessionInfo` |
| `resumeSession(sessionId)` | 恢复会话 | `SessionInfo` |
| `closeSession(sessionId)` | 关闭会话 | `{ sessionId: string, closed: boolean }` |
| `exists(sessionId)` | 检查会话是否存在 | `boolean` |
| `waitForStatus(sessionId, status, options)` | 等待会话达到指定状态 | `SessionInfo` |

#### 快捷操作

| 方法 | 说明 | 参数 |
|-----|------|-----|
| `navigate(sessionId, url)` | 导航到 URL | `url: string` |
| `click(sessionId, selector)` | 点击元素 | `selector: string` |
| `fill(sessionId, selector, value)` | 填充表单 | `selector: string, value: string` |
| `getText(sessionId, selector)` | 获取元素文本 | `selector: string` |
| `getAttribute(sessionId, selector, attribute)` | 获取元素属性 | `selector: string, attribute: string` |
| `executeScript(sessionId, script)` | 执行 JavaScript | `script: string` |
| `waitForSelector(sessionId, selector, options)` | 等待元素 | `selector: string, options?: { timeout?: number, visible?: boolean }` |

#### 截图和导出

| 方法 | 说明 | 参数 |
|-----|------|-----|
| `takeScreenshot(sessionId, options)` | 截图 | `options?: ScreenshotOptions` |
| `exportData(sessionId, request)` | 导出数据 | `request: ExportDataRequest` |

### SandboxWebSocketClient

#### 配置选项

| 选项 | 类型 | 默认值 | 说明 |
|-----|------|-------|-----|
| `baseUrl` | `string` | 必需 | API 基础 URL |
| `token` | `string` | 必需 | PASETO Token |
| `sessionId` | `string` | 必需 | 会话 ID |
| `credentialId` | `string` | 必需 | 凭证 ID |
| `heartbeatInterval` | `number` | `30000` | 心跳间隔（毫秒） |
| `operationTimeout` | `number` | `60000` | 操作超时（毫秒） |
| `autoReconnect` | `boolean` | `true` | 自动重连 |
| `maxReconnectAttempts` | `number` | `3` | 最大重连次数 |
| `reconnectDelay` | `number` | `5000` | 重连延迟（毫秒） |

#### 方法

| 方法 | 说明 | 返回值 |
|-----|------|-------|
| `connect()` | 连接 WebSocket | `Promise<void>` |
| `disconnect(reason?)` | 断开连接 | `void` |
| `isConnected()` | 检查是否已连接 | `boolean` |
| `getState()` | 获取连接状态 | `WebSocketState` |
| `executeOperation(options)` | 执行操作 | `Promise<ExecuteOperationResult>` |
| `requestScreenshot(timeout?)` | 请求截图 | `Promise<ScreenshotResult>` |

#### 事件回调

| 回调 | 参数 | 说明 |
|-----|------|-----|
| `onConnected` | `(data: ConnectedMessage)` | 连接成功 |
| `onDisconnected` | `(code: number, reason: string)` | 连接断开 |
| `onOperationProgress` | `(data: OperationProgressMessage)` | 操作进度更新 |
| `onOperationCompleted` | `(data: OperationCompletedMessage)` | 操作完成 |
| `onScreenshotResult` | `(data: ScreenshotResultMessage)` | 截图结果 |
| `onSessionStatusUpdate` | `(data: SessionStatusUpdateMessage)` | 会话状态更新 |
| `onError` | `(error: ErrorMessage)` | 错误消息 |
| `onConnectionError` | `(error: Event)` | 连接错误 |
| `onReconnecting` | `(attempt: number, maxAttempts: number)` | 正在重连 |

### 类型定义

```typescript
// 会话状态
enum SessionStatus {
  Creating = 'creating',
  Running = 'running',
  Paused = 'paused',
  Closed = 'closed',
  Error = 'error',
}

// 操作类型
enum OperationType {
  Navigate = 'navigate',
  Click = 'click',
  Fill = 'fill',
  GetText = 'get_text',
  GetAttribute = 'get_attribute',
  ExecuteScript = 'execute_script',
  WaitForSelector = 'wait_for_selector',
  Screenshot = 'screenshot',
  ExportData = 'export_data',
}

// 操作状态
enum OperationStatus {
  Pending = 'pending',
  Running = 'running',
  Success = 'success',
  Failed = 'failed',
  Cancelled = 'cancelled',
}

// WebSocket 状态
enum WebSocketState {
  Disconnected = 'disconnected',
  Connecting = 'connecting',
  Connected = 'connected',
  Reconnecting = 'reconnecting',
  Closed = 'closed',
}
```

---

## 附录

### 完整示例：自动化投资组合查询

```typescript
import { ToaniVaultSDK, SessionStatus, CredBridgeError } from '@toani/vault-sdk';

const sdk = new ToaniVaultSDK({
  baseUrl: process.env.TOANI_VAULT_BASE_URL!,
  token: process.env.TOANI_VAULT_TOKEN!,
});

interface PortfolioData {
  totalValue: string;
  dayChange: string;
  positions: Array<{
    symbol: string;
    name: string;
    quantity: string;
    price: string;
    value: string;
  }>;
  screenshot: string;
  timestamp: string;
}

async function queryPortfolio(
  credentialId: string,
  serviceId: string = 'schwab'
): Promise<PortfolioData> {
  const session = await sdk.sandbox.createSession({
    serviceId,
    credentialId,
    viewportWidth: 1920,
    viewportHeight: 1080,
  });

  try {
    // 等待会话就绪
    await sdk.sandbox.waitForStatus(session.sessionId, SessionStatus.Running, {
      timeout: 60000,
    });

    // 导航到登录页
    await sdk.sandbox.navigate(session.sessionId, 'https://www.schwab.com/login');

    // 等待登录表单
    await sdk.sandbox.waitForSelector(session.sessionId, '#loginId', {
      timeout: 30000,
    });

    // 获取凭证并登录（凭证在 TEE 内自动填充）
    await sdk.sandbox.click(session.sessionId, '#btnLogin');

    // 等待 MFA 或直接进入
    await new Promise(resolve => setTimeout(resolve, 5000));

    // 等待投资组合页面
    await sdk.sandbox.waitForSelector(session.sessionId, '.portfolio-summary', {
      timeout: 60000,
    });

    // 获取投资组合摘要
    const summaryResult = await sdk.sandbox.executeScript(session.sessionId, `
      return {
        totalValue: document.querySelector('.total-value')?.textContent?.trim() || '',
        dayChange: document.querySelector('.day-change')?.textContent?.trim() || '',
      };
    `);

    // 导出持仓数据
    const positionsResult = await sdk.sandbox.exportData(session.sessionId, {
      format: 'json',
      selector: '.positions-table tbody tr',
      extractionRules: [
        { name: 'symbol', selector: 'td:nth-child(1)' },
        { name: 'name', selector: 'td:nth-child(2)' },
        { name: 'quantity', selector: 'td:nth-child(3)' },
        { name: 'price', selector: 'td:nth-child(4)' },
        { name: 'value', selector: 'td:nth-child(5)' },
      ],
    });

    // 截图
    const screenshot = await sdk.sandbox.takeScreenshot(session.sessionId, {
      type: 'png',
      fullPage: true,
    });

    return {
      totalValue: (summaryResult.result as any).totalValue,
      dayChange: (summaryResult.result as any).dayChange,
      positions: positionsResult.data as any[],
      screenshot: screenshot.data,
      timestamp: new Date().toISOString(),
    };

  } finally {
    await sdk.sandbox.closeSession(session.sessionId);
  }
}

// 运行
queryPortfolio('your-credential-id')
  .then(data => {
    console.log('Portfolio:', data);
  })
  .catch((error: CredBridgeError) => {
    console.error('Failed to query portfolio:', error.message);
    process.exit(1);
  });
```

### 术语表

| 术语 | 说明 |
|-----|------|
| TEE | Trusted Execution Environment，可信执行环境 |
| Sandbox | TEE 内的安全浏览器自动化环境 |
| Session | 浏览器会话实例 |
| Operation | 在会话中执行的单个操作 |
| Selector | CSS 选择器或 XPath，用于定位页面元素 |
| WebSocket | 实时双向通信协议 |

---

**© 2026 CredBridge. All rights reserved.**

**安全声明**: 本文档包含 CredBridge Sandbox SDK 的使用指南，请妥善保管您的 API Token，不要在客户端代码中暴露敏感凭证。
