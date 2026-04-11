# API 参考

本目录记录当前 CredBridge 服务中已经实现的 HTTP API。本文档以 `src/main.rs` 与 `src/api/*.rs` 的实际路由、请求/响应结构为准，不以历史设计稿或旧示例为准。

## 文档列表

- [REST API](REST-API.md): 健康检查、认证、Token、Profile Automation Token、通知、Service Account、凭证、审计、Sandbox
- [Tenant API](TENANT-API.md): 租户管理接口
- [Attestation API](ATTESTATION-API.md): 远程认证接口

## 基础路径

- 业务 API: `/api/v1`
- 健康检查: `/health`
- 就绪检查: `/ready`
- 详细健康检查: `/health/detail`
- 指标: `/metrics`

## 当前已实现端点总览

### 服务级端点

- `GET /`
- `GET /api/v1/`
- `GET /health`
- `GET /ready`
- `GET /health/detail`
- `GET /metrics`

### 认证与用户

- `POST /api/v1/auth/session`
- `POST /api/v1/auth/access-token`
- `GET /api/v1/auth/me`
- `GET /api/v1/auth/memberships`
- `POST /api/v1/auth/logout`
- `GET /api/v1/auth/mfa-status`
- `POST /api/v1/auth/mfa-status/sync`
- `POST /api/v1/auth/invitations/consume`
- `POST /api/v1/invitations/consume`
- `GET /api/v1/users/me`
- `PATCH /api/v1/users/me`
- `DELETE /api/v1/users/me`
- `POST /api/v1/users/me/onboarding`
- `GET /api/v1/members`
- `PATCH /api/v1/members/:membership_id/role`
- `DELETE /api/v1/members/:membership_id`
- `GET /api/v1/invitations`
- `POST /api/v1/invitations`
- `POST /api/v1/invitations/:invitation_id/revoke`

### Token 管理

- `POST /api/v1/tokens`
- `GET /api/v1/tokens`
- `GET /api/v1/tokens/:token_id`
- `POST /api/v1/tokens/verify`
- `GET /api/v1/tokens/stats`
- `POST /api/v1/tokens/:token_id/revoke`

### Profile Automation Token

- `POST /api/v1/profile/automation-tokens`
- `GET /api/v1/profile/automation-tokens`
- `GET /api/v1/profile/automation-tokens/:token_id`
- `POST /api/v1/profile/automation-tokens/:token_id/revoke`

### 通知

- `GET /api/v1/notifications`

### Service Account

- `POST /api/v1/service-accounts`
- `GET /api/v1/service-accounts`
- `GET /api/v1/service-accounts/:service_account_id`
- `PATCH /api/v1/service-accounts/:service_account_id`
- `POST /api/v1/service-accounts/:service_account_id/tokens`
- `GET /api/v1/service-accounts/:service_account_id/tokens`

### 凭证管理

- `POST /api/v1/credentials`
- `GET /api/v1/credentials`
- `GET /api/v1/credentials/:id`
- `PUT /api/v1/credentials/:id`
- `DELETE /api/v1/credentials/:id`
- `POST /api/v1/credentials/:id/decrypt`
- `GET /api/v1/credentials/:id/versions`
- `GET /api/v1/credentials/:id/versions/:version`
- `POST /api/v1/credentials/:id/rollback`

### 审计日志

- `GET /api/v1/audit/logs`
- `GET /api/v1/audit/logs/:id`
- `POST /api/v1/audit/export`
- `POST /api/v1/audit/verify`

### Sandbox

- `POST /api/v1/sandbox/sessions`
- `GET /api/v1/sandbox/sessions`
- `GET /api/v1/sandbox/sessions/:id`
- `DELETE /api/v1/sandbox/sessions/:id`
- `POST /api/v1/sandbox/sessions/:id/execute`
- `POST /api/v1/sandbox/sessions/:id/pause`
- `POST /api/v1/sandbox/sessions/:id/resume`
- `POST /api/v1/sandbox/sessions/:id/screenshot`
- `POST /api/v1/sandbox/sessions/:id/export`
- `GET /api/v1/sandbox/operations/:operation_id`
- `GET /api/v1/sandbox/stats`
- `GET /api/v1/sandbox/sessions/:id/ws/:credential_id`

### 专项文档

- Tenant: 见 [Tenant API](TENANT-API.md)
- Attestation: 见 [Attestation API](ATTESTATION-API.md)

## 响应约定

当前代码里并不存在单一的全局响应格式，至少有以下 4 类：

1. 直接返回业务对象
- 例如 `POST /api/v1/auth/session`、`GET /api/v1/auth/me`、凭证 API、版本 API、token 成功响应

2. `ApiSuccessResponse<T>` 包装
- 形状为：
```json
{
  "success": true,
  "data": {}
}
```
- 例如 `/api/v1/auth/access-token`、`/api/v1/service-accounts`、`/api/v1/users/me`、`/api/v1/members`、`/api/v1/invitations`、`/api/v1/notifications`、大多数 Sandbox 接口

3. 自定义成功/失败对象
- 例如审计、租户、远程认证接口会同时返回 `success` 和自定义 `data`/`error`

4. `ApiErrorResponse`
- 典型形状为：
```json
{
  "success": false,
  "error": "invalid_request",
  "message": "用户友好的错误描述",
  "locale": "zh-CN"
}
```

因此阅读具体接口时，应以该接口所在模块的结构体定义为准，而不要假设所有模块都共享同一响应包装。

## 常见 Scope

以下 Scope 名称可在当前代码中看到被实际使用：

- `credential:read`
- `credential:decrypt`
- `credential:write`
- `credential:delete`
- `audit:read`
- `sandbox:execute`
- `sandbox:read`
- `sandbox:write`
- `tenant:admin`
- `tenant:read`
- `tenant:write`
- `tenant:delete`
- `members:read`
- `members:write`
- `members:invite`
- `invitations:read`
- `invitations:write`
- `tokens:read`
- `tokens:write`
- `tokens:revoke`
- `users:manage`
- `roles:manage`
- `admin`

注意：`TENANT-API.md` 中记录的是当前 `tenant` 模块 handler 的真实行为。该模块现在大多没有在 handler 层执行 scope 校验，这属于当前实现事实，不代表最终安全设计目标。

## 维护规则

更新本目录文档时，请同时核对：

- `src/main.rs`
- `src/api/*.rs`
- `src/api/response.rs`
- 相关 `tests/` 中的 API 测试

不要只依据旧文档做增量修补，否则会继续保留错误字段名、过时状态码和不存在的鉴权规则。

**更新时间**: 2026-04-11
