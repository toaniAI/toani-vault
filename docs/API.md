# CredBridge API 文档

**版本**: v1.0
**最后更新**: 2026-03-17
**适用版本**: CredBridge MVP 1.0+

---

## 目录

1. [概述](#1-概述)
2. [认证](#2-认证)
3. [凭证管理 API](#3-凭证管理-api)
4. [审计日志 API](#4-审计日志-api)
5. [租户管理 API](#5-租户管理-api)
6. [Sandbox API](#6-sandbox-api)
7. [TEE 认证 API](#7-tee-认证-api)
8. [错误处理](#8-错误处理)
9. [速率限制](#9-速率限制)

---

## 1. 概述

CredBridge API 提供 RESTful 接口用于凭证管理、审计日志查询、租户管理和 TEE 安全执行沙箱操作。

### 基础 URL

```
https://api.credbridge.io/api/v1
```

### 请求格式

所有请求和响应均使用 JSON 格式，编码为 UTF-8。

```http
Content-Type: application/json
Accept: application/json
```

### 响应格式

标准响应结构：

```json
{
  "success": true,
  "data": { ... },
  "meta": {
    "request_id": "req_xxx",
    "timestamp": "2024-03-17T10:30:00Z"
  }
}
```

---

## 2. 认证

### 2.1 Bearer Token

所有 API 请求必须在 `Authorization` 头中包含 Bearer Token：

```http
Authorization: Bearer <paseto_v4_local_token>
```

### 2.2 Token Scope 权限

| Scope | 权限说明 |
|-------|----------|
| `credential:read` | 读取凭证元数据 |
| `credential:decrypt` | 解密凭证获取明文 |
| `credential:write` | 创建/更新/删除凭证 |
| `audit:read` | 读取审计日志 |
| `tenant:admin` | 租户管理权限 |
| `sandbox:read` | 读取沙箱会话信息 |
| `sandbox:write` | 创建/控制沙箱会话 |
| `admin` | 所有管理权限 |

---

## 3. 凭证管理 API

### 3.1 创建凭证

```http
POST /api/v1/credentials
Authorization: Bearer <token>
Content-Type: application/json

{
  "service_id": "schwab",
  "credential_type": "username_password",
  "plaintext_data": {
    "username": "user@example.com",
    "password": "secret_password"
  },
  "expires_at": 1893456000
}
```

**响应**:

```json
{
  "success": true,
  "data": {
    "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "service_id": "schwab",
    "credential_type": "username_password",
    "created_at": "1709990400",
    "expires_at": "1893456000"
  },
  "meta": {
    "request_id": "req_xxx",
    "timestamp": "2024-03-17T10:30:00Z"
  }
}
```

### 3.2 获取凭证列表

```http
GET /api/v1/credentials?service_id=schwab&only_valid=true
Authorization: Bearer <token>
```

**响应**:

```json
{
  "success": true,
  "data": {
    "credentials": [
      {
        "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
        "credential_type": "username_password",
        "service_id": "schwab",
        "created_at": "1709990400Z",
        "expires_at": "1893456000Z"
      }
    ],
    "total": 1
  },
  "meta": {
    "request_id": "req_xxx",
    "timestamp": "2024-03-17T10:30:00Z"
  }
}
```

### 3.3 获取凭证详情

```http
GET /api/v1/credentials/:id
Authorization: Bearer <token>
```

### 3.4 解密凭证

```http
POST /api/v1/credentials/:id/decrypt
Authorization: Bearer <token>
Content-Type: application/json

{
  "reason": "用户登录操作"
}
```

**响应**:

```json
{
  "success": true,
  "data": {
    "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "service_id": "schwab",
    "credential_type": "username_password",
    "plaintext_data": {
      "username": "user@example.com",
      "password": "secret_password"
    }
  },
  "meta": {
    "request_id": "req_xxx",
    "timestamp": "2024-03-17T10:30:00Z"
  }
}
```

### 3.5 删除凭证

```http
DELETE /api/v1/credentials/:id
Authorization: Bearer <token>
```

---

## 4. 审计日志 API

### 4.1 查询审计日志

```http
GET /api/v1/audit/logs?start_time=1704067200&end_time=1706745600&limit=20
Authorization: Bearer <token>
```

**响应**:

```json
{
  "success": true,
  "data": {
    "entries": [
      {
        "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
        "user_id_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "timestamp": 1709990400000,
        "action": "CredentialDecrypt",
        "risk_tier": "High",
        "outcome": "Success",
        "tee_mrenclave": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
      }
    ],
    "total": 150,
    "has_more": true
  },
  "meta": {
    "request_id": "req_xxx",
    "timestamp": "2024-03-17T10:30:00Z"
  }
}
```

### 4.2 导出审计日志

```http
POST /api/v1/audit/export
Authorization: Bearer <token>
Content-Type: application/json

{
  "start_time": 1704067200,
  "end_time": 1706745600,
  "format": "json",
  "include_verification": true
}
```

---

## 5. 租户管理 API

详见 [TENANT_API.md](./TENANT_API.md)

---

## 6. Sandbox API

### 6.1 概述

Sandbox API 提供基于 TEE（可信执行环境）的安全浏览器自动化能力。所有操作在隔离的沙箱环境中执行，确保凭证安全。

**主要功能**：
- 创建和管理沙箱会话
- 在 TEE 环境中执行浏览器自动化操作
- 实时 WebSocket 流控制
- 截图和数据导出

**认证方式**：使用 PASETO v4.local 令牌

**完整 OpenAPI 规范**：[docs/openapi/sandbox.yaml](./openapi/sandbox.yaml)

**SDK 使用指南**：[docs/SDK_SANDBOX_GUIDE.md](./SDK_SANDBOX_GUIDE.md)

### 6.2 API 端点列表

| 方法 | 端点 | 描述 | 所需 Scope |
|------|------|------|------------|
| POST | `/sandbox/sessions` | 创建沙箱会话 | `sandbox:write` |
| GET | `/sandbox/sessions` | 列出所有会话 | `sandbox:read` |
| GET | `/sandbox/sessions/{id}` | 获取会话详情 | `sandbox:read` |
| DELETE | `/sandbox/sessions/{id}` | 关闭会话 | `sandbox:write` |
| POST | `/sandbox/sessions/{id}/execute` | 执行操作 | `sandbox:write` |
| POST | `/sandbox/sessions/{id}/pause` | 暂停会话 | `sandbox:write` |
| POST | `/sandbox/sessions/{id}/resume` | 恢复会话 | `sandbox:write` |
| POST | `/sandbox/sessions/{id}/screenshot` | 截图导出 | `sandbox:write` |
| POST | `/sandbox/sessions/{id}/export` | 数据导出 | `sandbox:write` |
| WS | `/sandbox/sessions/{id}/ws/{credential_id}` | WebSocket 实时流 | `sandbox:write` |

### 6.3 创建会话

```http
POST /api/v1/sandbox/sessions
Authorization: Bearer <token>
Content-Type: application/json

{
  "credential_id": "550e8400-e29b-41d4-a716-446655440000",
  "original_intent": "查询投资组合",
  "metadata": {
    "source": "mobile_app",
    "priority": "high"
  }
}
```

**响应**:

```json
{
  "success": true,
  "data": {
    "session_id": "550e8400-e29b-41d4-a716-446655440001",
    "sandbox_id": "550e8400-e29b-41d4-a716-446655440002",
    "status": "ready",
    "created_at": "2024-01-15T10:30:00Z",
    "expires_at": "2024-01-15T11:00:00Z"
  },
  "meta": {
    "request_id": "req_xxx",
    "timestamp": "2024-03-17T10:30:00Z"
  }
}
```

### 6.4 执行操作

```http
POST /api/v1/sandbox/sessions/{id}/execute
Authorization: Bearer <token>
Content-Type: application/json

{
  "operation_type": "navigate",
  "description": "导航到登录页面",
  "parameters": {
    "url": "https://example.com/login"
  }
}
```

**支持的操作类型**：
- `navigate` - 页面导航
- `click` - 点击元素
- `fill` - 填写表单
- `get_text` - 获取文本内容
- `screenshot` - 截图
- `export` - 导出数据
- `execute_script` - 执行 JavaScript
- `wait` - 等待元素
- `custom` - 自定义操作

### 6.5 WebSocket 连接

WebSocket 端点提供实时控制和监控能力：

```
ws://api.credbridge.io/api/v1/sandbox/sessions/{id}/ws/{credential_id}
```

**客户端消息类型**：
- `execute` - 执行操作
- `screenshot` - 请求截图
- `heartbeat` - 心跳保持
- `close` - 关闭连接

**服务端消息类型**：
- `connected` - 连接成功
- `operation_progress` - 操作进度更新
- `operation_completed` - 操作完成
- `screenshot_result` - 截图结果
- `heartbeat_ack` - 心跳确认
- `session_status_update` - 会话状态更新
- `error` - 错误消息

**使用示例**（TypeScript SDK）：

```typescript
import { SandboxWebSocketClient } from '@credbridge/sdk';

const ws = new SandboxWebSocketClient({
  baseUrl: 'https://api.credbridge.io',
  token: 'v4.local.your-token',
  sessionId: 'session-uuid',
  credentialId: 'credential-uuid',
});

ws.onOperationProgress = (data) => {
  console.log(`Progress: ${data.progress}%`);
};

await ws.connect();

const result = await ws.executeOperation({
  operationType: 'navigate',
  description: 'Navigate to example.com',
  parameters: { url: 'https://example.com' },
});
```

### 6.6 截图和导出

**截图**：

```http
POST /api/v1/sandbox/sessions/{id}/screenshot
Authorization: Bearer <token>
Content-Type: application/json

{
  "full_page": true,
  "type": "png"
}
```

**数据导出**：

```http
POST /api/v1/sandbox/sessions/{id}/export
Authorization: Bearer <token>
Content-Type: application/json

{
  "format": "json",
  "selectors": [".data-table", ".portfolio-item"],
  "extraction_rules": [
    { "name": "symbol", "selector": ".symbol-cell" },
    { "name": "price", "selector": ".price-cell" }
  ]
}
```

---

## 7. TEE 认证 API

### 7.1 获取 TEE 状态

```http
GET /api/v1/tee/status
Authorization: Bearer <token>
```

**响应**:

```json
{
  "success": true,
  "data": {
    "tee_enabled": true,
    "tee_type": "SGX",
    "mrenclave": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    "quote_valid": true,
    "remote_attestation": "passed"
  },
  "meta": {
    "request_id": "req_xxx",
    "timestamp": "2024-03-17T10:30:00Z"
  }
}
```

### 7.2 远程认证

```http
POST /api/v1/tee/attest
Authorization: Bearer <token>
```

---

## 8. 错误处理

### 8.1 错误响应格式

```json
{
  "success": false,
  "error": "invalid_request",
  "message": "Missing required field: credential_id",
  "meta": {
    "request_id": "req_xxx",
    "timestamp": "2024-03-17T10:30:00Z"
  }
}
```

### 8.2 HTTP 状态码

| 状态码 | 描述 | 场景 |
|--------|------|------|
| 200 | 成功 | 请求成功完成 |
| 201 | 已创建 | 资源创建成功 |
| 204 | 无内容 | 删除成功 |
| 400 | 请求参数无效 | 缺少必需字段或格式错误 |
| 401 | 未授权 | 缺少或无效的认证令牌 |
| 403 | 禁止访问 | 权限不足（Scope 不匹配） |
| 404 | 资源不存在 | 凭证或会话不存在 |
| 409 | 状态冲突 | 会话状态不允许该操作 |
| 429 | 请求过于频繁 | 超出速率限制 |
| 500 | 服务器内部错误 | 服务器端错误 |
| 503 | 服务暂不可用 | 系统过载或维护中 |

### 8.3 错误码列表

| 错误码 | 描述 |
|--------|------|
| `invalid_request` | 请求参数无效或缺失 |
| `unauthorized` | 未提供有效的认证令牌 |
| `forbidden` | 权限不足 |
| `not_found` | 请求的资源不存在 |
| `rate_limited` | 请求频率超限 |
| `service_unavailable` | 服务暂时不可用 |
| `credential_not_found` | 凭证不存在 |
| `session_not_found` | 会话不存在 |
| `session_expired` | 会话已过期 |
| `invalid_session_state` | 会话状态无效 |
| `tee_not_available` | TEE 环境不可用 |
| `internal_error` | 内部服务器错误 |

---

## 9. 速率限制

### 9.1 默认限制

| 端点类型 | 限制 |
|----------|------|
| 凭证创建 | 100/分钟 |
| 凭证读取 | 300/分钟 |
| 凭证解密 | 60/分钟 |
| 审计日志查询 | 60/分钟 |
| 沙箱会话创建 | 30/分钟 |
| 沙箱操作执行 | 120/分钟 |

### 9.2 限制响应

当超出速率限制时，API 返回 429 状态码：

```json
{
  "success": false,
  "error": "rate_limited",
  "message": "Too many requests, please try again later",
  "meta": {
    "request_id": "req_xxx",
    "timestamp": "2024-03-17T10:30:00Z",
    "retry_after": 60
  }
}
```

---

## 相关文档

- [用户手册](./USER_MANUAL.md) - 详细使用指南
- [SDK 使用指南](./SDK_GUIDE.md) - TypeScript/Rust SDK 文档
- [Sandbox SDK 指南](./SDK_SANDBOX_GUIDE.md) - Sandbox SDK 专用文档
- [租户管理 API](./TENANT_API.md) - 多租户管理接口
- [OpenAPI 规范](./openapi/sandbox.yaml) - Sandbox API 完整规范

---

**© 2026 CredBridge. All rights reserved.**
