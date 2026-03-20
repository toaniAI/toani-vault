# API 参考

本目录包含 CredBridge 完整的 API 接口文档。

## API 列表

### REST API
- [REST API](REST-API.md) - 完整的 RESTful API 接口文档
- [Tenant API](TENANT-API.md) - 租户管理 API
- [Attestation API](ATTESTATION-API.md) - 远程认证 API

## API 端点

**基础路径**: `/api/v1`

### 认证 API
- `POST /api/v1/auth/login` - 登录
- `POST /api/v1/auth/refresh` - 刷新 Token
- `POST /api/v1/auth/me` - 获取当前用户信息

### Token 管理 API
- `POST /api/v1/tokens` - 创建 Token
- `POST /api/v1/tokens/verify` - 验证 Token
- `POST /api/v1/tokens/:id/revoke` - 撤销 Token

### 凭证管理 API
- `POST /api/v1/credentials` - 创建凭证
- `GET /api/v1/credentials` - 获取凭证列表
- `GET /api/v1/credentials/:id` - 获取凭证详情
- `PUT /api/v1/credentials/:id` - 更新凭证
- `DELETE /api/v1/credentials/:id` - 删除凭证
- `POST /api/v1/credentials/:id/decrypt` - 解密凭证
- `GET /api/v1/credentials/:id/versions` - 获取版本历史
- `GET /api/v1/credentials/:id/versions/:version` - 获取版本详情
- `POST /api/v1/credentials/:id/rollback` - 回滚凭证

### 审计日志 API
- `GET /api/v1/audit/logs` - 查询审计日志列表
- `GET /api/v1/audit/logs/:id` - 获取审计日志详情
- `POST /api/v1/audit/export` - 导出审计日志
- `POST /api/v1/audit/verify` - 验证审计日志

### Sandbox API
- `POST /api/v1/sandbox/sessions` - 创建沙箱会话
- `GET /api/v1/sandbox/sessions` - 获取会话列表
- `GET /api/v1/sandbox/sessions/:id` - 获取会话详情
- `POST /api/v1/sandbox/sessions/:id/execute` - 执行操作
- `POST /api/v1/sandbox/sessions/:id/pause` - 暂停会话
- `POST /api/v1/sandbox/sessions/:id/resume` - 恢复会话
- `DELETE /api/v1/sandbox/sessions/:id` - 关闭会话
- `POST /api/v1/sandbox/sessions/:id/screenshot` - 截图
- `POST /api/v1/sandbox/sessions/:id/export` - 数据导出
- `GET /api/v1/sandbox/sessions/:id/ws/:credential_id` - WebSocket 连接

### 远程认证 API
- `GET /api/v1/attestation/quote` - 获取 Quote
- `POST /api/v1/attestation/verify` - 验证 Quote
- `GET /api/v1/attestation/report` - 获取认证报告
- `POST /api/v1/attestation/challenge` - 创建挑战
- `POST /api/v1/attestation/verify-response` - 验证挑战响应
- `GET /api/v1/attestation/status` - 获取认证状态
- `POST /api/v1/attestation/refresh` - 刷新 Quote
- `GET /api/v1/attestation/health` - 健康检查

### 健康检查 (不在 /api/v1 下)
- `GET /health` - 健康检查
- `GET /health/detail` - 详细健康检查
- `GET /metrics` - Prometheus 指标

## 错误处理

所有 API 错误遵循统一格式：

```json
{
  "error": {
    "code": "error_code",
    "message": "人类可读的错误信息",
    "details": {}
  }
}
```

### HTTP 状态码说明

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 OK | 请求成功 |
| 201 Created | 资源创建成功 |
| 204 No Content | 请求成功，无返回内容 |
| 400 Bad Request | 请求参数错误或缺失 |
| 401 Unauthorized | 未认证或 Token 无效 |
| 403 Forbidden | 权限不足或 Scope 不够 |
| 404 Not Found | 资源不存在 |
| 409 Conflict | 资源冲突（如凭证已存在） |
| 422 Unprocessable Entity | 请求语义错误 |
| 429 Too Many Requests | 速率限制 |
| 500 Internal Server Error | 服务器内部错误 |
| 503 Service Unavailable | 服务暂时不可用 |

### 常见错误码

| 错误码 | HTTP 状态码 | 说明 |
|--------|-------------|------|
| `invalid_request` | 400 | 请求参数无效或缺失 |
| `invalid_time_range` | 400 | 时间范围无效 |
| `verification_failed` | 400 | 验证失败 |
| `missing_token` | 401 | 缺少 Authorization 头 |
| `invalid_token` | 401 | Token 格式无效或过期 |
| `revoked_token` | 401 | Token 已被撤销 |
| `insufficient_scope` | 403 | Token 缺少必需的 Scope |
| `access_denied` | 403 | 访问被拒绝（权限不足） |
| `not_found` | 404 | 资源不存在 |
| `credential_not_found` | 404 | 凭证不存在 |
| `audit_entry_not_found` | 404 | 审计条目不存在 |
| `conflict` | 409 | 资源冲突 |
| `rate_limited` | 429 | 请求频率超限 |
| `internal_error` | 500 | 服务器内部错误 |
| `storage_error` | 500 | 存储错误 |
| `service_unavailable` | 503 | 服务暂时不可用 |

## Token Scope 权限

| Scope | 权限说明 |
|-------|----------|
| `credential:read` | 读取凭证元数据 |
| `credential:decrypt` | 解密凭证获取明文 |
| `credential:write` | 创建/更新/删除凭证 |
| `credential:delete` | 删除凭证（可与 write 互换）|
| `audit:read` | 读取审计日志 |
| `token:manage` | 管理 Token |
| `sandbox:read` | 读取沙箱会话信息 |
| `sandbox:write` | 创建/控制沙箱会话 |
| `admin` | 所有管理权限 |

## 速率限制

- 默认：100 请求/分钟
- 认证端点：10 请求/分钟
- Sandbox 端点：20 请求/分钟

---

**更新时间**: 2026-03-20
