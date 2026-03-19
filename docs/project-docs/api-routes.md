# CredBridge API 路由文档

**生成日期**: 2026-03-18
**API 版本**: v1
**基础路径**: `/api/v1`

---

## 路由概览

| 模块 | 路径前缀 | 描述 |
|------|----------|------|
| 凭证管理 | `/credentials` | 凭证的 CRUD 和解密操作 |
| Token 管理 | `/tokens` | PASETO Token 创建和验证 |
| 审计日志 | `/audit` | 审计日志查询和验证 |
| 沙箱 | `/sandbox` | TEE 沙箱会话管理 |
| 多租户 | `/tenants` | 租户管理 |
| 健康检查 | `/health` | 系统健康状态 |

---

## 凭证管理路由

### 创建凭证
```
POST /api/v1/credentials
```
创建新的加密凭证。

**请求体**:
```json
{
  "credential_type": "username_password",
  "user_id": "user123",
  "service_id": "service456",
  "payload": {
    "username": "john",
    "password": "secret"
  }
}
```

**响应**:
```json
{
  "credential_id": "uuid",
  "created_at": "2026-03-18T10:00:00Z",
  "version": 1
}
```

### 获取凭证列表
```
GET /api/v1/credentials
```
获取当前租户的凭证列表。

**查询参数**:
- `service_id` (可选): 按服务ID筛选
- `user_id` (可选): 按用户ID筛选
- `page` (可选): 页码，默认 1
- `limit` (可选): 每页数量，默认 20

### 获取单个凭证
```
GET /api/v1/credentials/:id
```
获取凭证元数据（不含加密内容）。

### 解密凭证
```
POST /api/v1/credentials/:id/decrypt
```
解密并返回凭证内容。

**响应**:
```json
{
  "credential_id": "uuid",
  "credential_type": "username_password",
  "payload": {
    "username": "john",
    "password": "secret"
  }
}
```

### 更新凭证
```
PUT /api/v1/credentials/:id
```
更新凭证内容，自动创建新版本。

### 删除凭证
```
DELETE /api/v1/credentials/:id
```
软删除凭证（标记为已删除）。

### 获取版本历史
```
GET /api/v1/credentials/:id/versions
```
获取凭证的所有版本历史。

### 回滚凭证
```
POST /api/v1/credentials/:id/rollback
```
回滚到指定版本。

**请求体**:
```json
{
  "version": 2
}
```

---

## Token 管理路由

### 创建 Token
```
POST /api/v1/tokens
```
创建新的访问令牌。

**请求体**:
```json
{
  "tenant_id": "tenant123",
  "scopes": ["credentials:read", "credentials:write"],
  "expires_in": 3600
}
```

**响应**:
```json
{
  "access_token": "paseto.token.here",
  "refresh_token": "refresh.token.here",
  "expires_in": 3600,
  "token_type": "Bearer"
}
```

### 验证 Token
```
POST /api/v1/tokens/verify
```
验证 Token 的有效性。

**请求体**:
```json
{
  "token": "paseto.token.here"
}
```

**响应**:
```json
{
  "valid": true,
  "tenant_id": "tenant123",
  "scopes": ["credentials:read"],
  "expires_at": "2026-03-18T11:00:00Z"
}
```

### 撤销 Token
```
POST /api/v1/tokens/:id/revoke
```
撤销指定的 Token。

---

## 审计日志路由

### 查询审计日志
```
GET /api/v1/audit/logs
```
查询审计日志列表。

**查询参数**:
- `start_time`: 开始时间 (ISO 8601)
- `end_time`: 结束时间 (ISO 8601)
- `event_type`: 事件类型筛选
- `user_id`: 用户ID筛选
- `page`: 页码
- `limit`: 每页数量

**响应**:
```json
{
  "logs": [
    {
      "id": "log-id",
      "event_type": "credential_created",
      "tenant_id": "tenant123",
      "user_id": "user456",
      "timestamp": "2026-03-18T10:00:00Z",
      "details": {...}
    }
  ],
  "total": 100,
  "page": 1,
  "limit": 20
}
```

### 导出审计日志
```
POST /api/v1/audit/export
```
导出审计日志为 CSV 或 JSON 格式。

### 验证审计日志
```
POST /api/v1/audit/verify
```
验证审计日志的完整性和不可篡改性（使用 immudb）。

---

## 沙箱路由

### 创建沙箱会话
```
POST /api/v1/sandbox/sessions
```
创建新的 TEE 沙箱会话。

**请求体**:
```json
{
  "runtime": "python3.11",
  "timeout_seconds": 300,
  "memory_limit_mb": 512,
  "cpu_limit": 1.0
}
```

**响应**:
```json
{
  "session_id": "session-uuid",
  "status": "initializing",
  "attestation_report": {...},
  "created_at": "2026-03-18T10:00:00Z"
}
```

### 获取沙箱会话列表
```
GET /api/v1/sandbox/sessions
```
获取所有沙箱会话列表。

### 获取单个会话
```
GET /api/v1/sandbox/sessions/:id
```
获取沙箱会话详情。

### 终止会话
```
POST /api/v1/sandbox/sessions/:id/terminate
```
终止指定的沙箱会话。

### 执行操作
```
POST /api/v1/sandbox/sessions/:id/execute
```
在沙箱中执行代码或命令。

**请求体**:
```json
{
  "operation_type": "execute_code",
  "code": "print('Hello, TEE!')",
  "language": "python"
}
```

### 验证证明报告
```
POST /api/v1/sandbox/attestation/verify
```
验证 TEE 证明报告的真实性。

---

## 多租户路由

### 创建租户
```
POST /api/v1/tenants
```
创建新租户。

### 获取租户列表
```
GET /api/v1/tenants
```
获取租户列表（管理员权限）。

### 获取租户详情
```
GET /api/v1/tenants/:id
```
获取租户配置和统计信息。

### 更新租户配置
```
PUT /api/v1/tenants/:id
```
更新租户配置（配额、TTL等）。

---

## 健康检查路由

### 基础健康检查
```
GET /api/v1/health
```
基础健康检查端点。

**响应**:
```json
{
  "status": "healthy",
  "timestamp": "2026-03-18T10:00:00Z"
}
```

### 详细健康检查
```
GET /api/v1/health/detail
```
详细健康状态，包含各组件状态。

**响应**:
```json
{
  "status": "healthy",
  "components": {
    "database": {"status": "healthy", "latency_ms": 5},
    "redis": {"status": "healthy", "latency_ms": 2},
    "vault": {"status": "healthy", "latency_ms": 10},
    "immudb": {"status": "healthy", "latency_ms": 8}
  }
}
```

### 指标端点
```
GET /api/v1/metrics
```
Prometheus 格式的系统指标。

---

## 中间件

### 认证中间件
所有 API 路由（除健康检查外）都需要有效的 PASETO Token。

**请求头**:
```
Authorization: Bearer <paseto_token>
```

### 租户隔离中间件
自动从 Token 中提取租户ID，并应用到 RLS 上下文。

### 速率限制中间件
基于 Token 或 IP 的速率限制。

### 审计中间件
自动记录所有 API 请求到审计日志。

---

## 错误响应格式

```json
{
  "error": {
    "code": "CREDENTIAL_NOT_FOUND",
    "message": "凭证不存在",
    "details": {
      "credential_id": "uuid"
    }
  }
}
```

### 常见错误码

| 错误码 | HTTP 状态 | 描述 |
|--------|-----------|------|
| `UNAUTHORIZED` | 401 | 未授权，Token 无效或过期 |
| `FORBIDDEN` | 403 | 无权限访问该资源 |
| `NOT_FOUND` | 404 | 资源不存在 |
| `VALIDATION_ERROR` | 422 | 请求参数验证失败 |
| `RATE_LIMITED` | 429 | 请求频率超限 |
| `INTERNAL_ERROR` | 500 | 服务器内部错误 |

---

## WebSocket API

### 实时审计日志
```
WS /api/v1/ws/audit
```
订阅实时审计日志流。

### 沙箱输出流
```
WS /api/v1/ws/sandbox/:id/output
```
实时接收沙箱输出。

---

*本文档由 BMAD document-project 工作流自动生成*
