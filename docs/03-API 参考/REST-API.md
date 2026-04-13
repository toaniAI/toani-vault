# CredBridge REST API

本文档记录当前 `src/main.rs` 与 `src/api/*.rs` 中实际已注册的 REST 端点。

## 阅读说明

- 基础业务路径为 `/api/v1`
- 不同模块的响应包装并不统一
- 认证中的部分接口、Service Account、部分用户管理与部分 Sandbox 接口使用 `ApiSuccessResponse<T>`
- 凭证、版本、部分 auth 接口返回直接业务对象
- 审计、租户、attestation 使用各自的自定义响应结构
- 如果文档与代码冲突，以源码结构体和测试为准

## 目录

- [API 根](#api-根)
- [健康检查](#健康检查)
- [认证与用户](#认证与用户)
- [Token 管理](#token-管理)
- [Profile Automation Token](#profile-automation-token)
- [通知](#通知)
- [Service Account](#service-account)
- [凭证管理](#凭证管理)
- [凭证版本](#凭证版本)
- [审计日志](#审计日志)
- [Sandbox](#sandbox)
- [专项文档](#专项文档)

## API 根

### `GET /api/v1/`

返回 API 根说明与可发现性信息。

## 健康检查

### `GET /health`

返回简单存活状态。

**响应 (200 OK)**:

```json
{
  "status": "alive",
  "ready": true,
  "version": "1.0.0",
  "timestamp": 1741702800
}
```

### `GET /ready`

与 `GET /health/detail` 使用同一实现，返回详细就绪状态。

### `GET /health/detail`

返回详细健康状态。

**响应 (200 OK 或 503 Service Unavailable)**:

```json
{
  "status": "ready",
  "live": true,
  "ready": true,
  "version": "1.0.0",
  "timestamp": 1741702800,
  "components": {
    "vault": "healthy",
    "enclave": "healthy",
    "audit_log": "healthy",
    "attestation": "ready"
  }
}
```

### `GET /metrics`

返回 Prometheus 文本格式指标。

## 认证与用户

### 概览

已实现端点：

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

### `POST /api/v1/auth/session`

使用 Privy access token 创建会话。

`privy_access_token` 为推荐字段名，接口同时兼容历史别名 `privy_token`。服务端会将 token 长度限制为不超过 `2048` 个字符；超长请求会在进入认证逻辑前直接返回 `400 Bad Request`。

**请求体**:

```json
{
  "privy_access_token": "privy_access_token",
  "invitation_token": "optional_invitation_token"
}
```

兼容旧调用方时，也可以发送：

```json
{
  "privy_token": "privy_access_token"
}
```

**响应 (200 OK)**:

```json
{
  "user": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "display_name": "Alice",
    "status": "active",
    "onboarding_completed": false,
    "default_tenant_id": null,
    "identities": []
  },
  "session": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "session_token": "v4.local.xxx",
    "expires_at": "2026-04-11T12:00:00Z",
    "mfa_status": "not_required"
  },
  "memberships": [],
  "current_tenant": null,
  "current_membership": null
}
```

**错误响应 (400 Bad Request, token 过长)**:

```json
{
  "error": "invalid_request",
  "message": "服务器内部错误",
  "error_description": "服务器内部错误",
  "i18n": {
    "key": "errors.auth.token_too_long"
  },
  "locale": "zh-CN"
}
```

**错误响应说明**:

- 缺少 `privy_access_token` / `privy_token`、请求体 JSON 非法、未知字段：返回 `400 Bad Request`，`error=invalid_request`
- `privy_access_token` 或 `privy_token` 长度大于 `2048`：返回 `400 Bad Request`，`error=invalid_request`，`i18n.key=errors.auth.token_too_long`
- token 格式无效或认证失败：返回 `401 Unauthorized`

### `POST /api/v1/auth/access-token`

从当前用户 token 签发 API access token。

**认证**:
- 需要用户 token
- 需要 active membership
- 需要 `tokens:write` 或 `admin`
- 请求 `scopes` 必须是当前 token scopes 的子集

**请求体**:

```json
{
  "scopes": ["credential:read", "credential:write"],
  "ttl_seconds": 900
}
```

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "access_token": "v4.local.xxx",
    "token_id": "550e8400-e29b-41d4-a716-446655440000",
    "token_type": "Bearer",
    "subject_type": "user",
    "issued_from": "access_token",
    "display_name": null,
    "expires_at": 1741703700,
    "expires_in": 900,
    "granted_scopes": ["credential:read", "credential:write"],
    "revoked_at": null
  }
}
```

### `GET /api/v1/auth/me`

获取当前 web session 用户信息。

**认证**:
- 只接受 web session token

**响应 (200 OK)**:

```json
{
  "userId": "550e8400-e29b-41d4-a716-446655440000",
  "tenantId": "550e8400-e29b-41d4-a716-446655440010",
  "username": "alice@example.com",
  "scopes": ["admin"],
  "locale": "zh-CN",
  "user": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "display_name": "Alice",
    "status": "active",
    "onboarding_completed": true,
    "default_tenant_id": "550e8400-e29b-41d4-a716-446655440010",
    "identities": []
  },
  "current_tenant": {
    "id": "550e8400-e29b-41d4-a716-446655440010",
    "name": "Acme"
  },
  "current_membership": {
    "id": "550e8400-e29b-41d4-a716-446655440020",
    "tenant_id": "550e8400-e29b-41d4-a716-446655440010",
    "role": "owner",
    "status": "active",
    "scopes": ["admin"],
    "joined_at": "2026-04-11T10:00:00Z"
  },
  "memberships": [],
  "mfa_status": "not_required"
}
```

### `GET /api/v1/auth/memberships`

返回当前用户的活跃成员资格列表。

**响应 (200 OK)**:

```json
{
  "memberships": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440020",
      "tenant_id": "550e8400-e29b-41d4-a716-446655440010",
      "role": "owner",
      "status": "active",
      "scopes": ["admin"],
      "joined_at": "2026-04-11T10:00:00Z"
    }
  ]
}
```

### `POST /api/v1/auth/logout`

撤销当前 web session。

**认证**:
- 只接受 web session token

**响应 (200 OK)**:

```json
{
  "success": true
}
```

### `GET /api/v1/auth/mfa-status`

**响应 (200 OK)**:

```json
{
  "enabled": true,
  "verified": true,
  "requires_step_up": false,
  "last_verified_at": "2026-04-11T10:00:00Z",
  "synced_at": "2026-04-11T10:00:00Z"
}
```

### `POST /api/v1/auth/mfa-status/sync`

从 Privy 同步 MFA 状态。

**请求体**:

```json
{
  "privy_access_token": "privy_access_token"
}
```

**响应**: 与 `GET /api/v1/auth/mfa-status` 相同。

### `POST /api/v1/auth/invitations/consume`
### `POST /api/v1/invitations/consume`

两个入口调用同一 handler。

**认证**:
- 只接受 web session token

**请求体**:

```json
{
  "invitation_token": "invite_token"
}
```

兼容旧调用方时，也可以发送：

```json
{
  "code": "invite_token"
}
```

**响应 (200 OK)**:

```json
{
  "membership": {
    "id": "550e8400-e29b-41d4-a716-446655440020",
    "tenant_id": "550e8400-e29b-41d4-a716-446655440010",
    "role": "member",
    "status": "active",
    "scopes": ["credential:read"],
    "joined_at": "2026-04-11T10:00:00Z"
  },
  "tenant": {
    "id": "550e8400-e29b-41d4-a716-446655440010",
    "name": null
  }
}
```

### `GET /api/v1/users/me`

返回 `ApiSuccessResponse<FrontendUserProfile>`。

### `PATCH /api/v1/users/me`

**请求体**:

```json
{
  "displayName": "Alice",
  "defaultTenantId": "550e8400-e29b-41d4-a716-446655440010"
}
```

**响应**:

```json
{
  "success": true,
  "data": {
    "user": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "displayName": "Alice",
      "status": "active",
      "onboardingCompleted": true,
      "defaultTenantId": "550e8400-e29b-41d4-a716-446655440010",
      "identities": []
    }
  }
}
```

### `DELETE /api/v1/users/me`

**响应**:

```json
{
  "success": true,
  "data": {
    "userId": "550e8400-e29b-41d4-a716-446655440000",
    "deletedAt": "2026-04-11T10:00:00Z",
    "message": "User account deleted"
  }
}
```

### `POST /api/v1/users/me/onboarding`

**请求体**:

```json
{
  "displayName": "Alice"
}
```

**响应**: `ApiSuccessResponse<FrontendUserProfile>`。

### 成员与邀请管理

#### `GET /api/v1/members`

**认证**:
- 需要 `members:read`、`members:write` 或 `admin`

**Query**:
- `tenant_id` (UUID, 必填)

**响应**:

```json
{
  "success": true,
  "data": {
    "members": [
      {
        "membership": {
          "id": "550e8400-e29b-41d4-a716-446655440020",
          "tenantId": "550e8400-e29b-41d4-a716-446655440010",
          "userId": "550e8400-e29b-41d4-a716-446655440000",
          "role": "owner",
          "status": "active",
          "invitedBy": null,
          "joinedAt": "2026-04-11T10:00:00Z",
          "source": "manual",
          "scopes": ["admin"],
          "createdAt": "2026-04-11T10:00:00Z",
          "updatedAt": "2026-04-11T10:00:00Z"
        },
        "user": {
          "id": "550e8400-e29b-41d4-a716-446655440000",
          "displayName": "Alice",
          "status": "active",
          "onboardingCompleted": true,
          "defaultTenantId": null,
          "identities": []
        }
      }
    ],
    "total": 1
  }
}
```

#### `PATCH /api/v1/members/:membership_id/role`

**请求体**:

```json
{
  "role": "member"
}
```

**响应**: `ApiSuccessResponse<FrontendMembershipInfo>`。

#### `DELETE /api/v1/members/:membership_id`

**响应**:
- `204 No Content`

#### `GET /api/v1/invitations`

**认证**:
- 需要 `invitations:read`、`members:invite` 或 `admin`

**Query**:
- `tenant_id` (UUID, 必填)
- `limit` (可选，正整数；当前仅做参数校验，不实际分页)

**响应**:

```json
{
  "success": true,
  "data": [
    {
      "invitation": {
        "id": "550e8400-e29b-41d4-a716-446655440030",
        "tenantId": "550e8400-e29b-41d4-a716-446655440010",
        "role": "member",
        "inviteeType": "email",
        "inviteeEmail": "user@example.com",
        "inviteeWallet": null,
        "createdBy": "550e8400-e29b-41d4-a716-446655440000",
        "expiresAt": "2026-04-12T10:00:00Z",
        "consumedAt": null,
        "consumedBy": null,
        "status": "pending",
        "maxUses": 1,
        "useCount": 0,
        "createdAt": "2026-04-11T10:00:00Z"
      },
      "inviteToken": "",
      "inviteUrl": ""
    }
  ]
}
```

#### `POST /api/v1/invitations`

**请求体**:

```json
{
  "tenantId": "550e8400-e29b-41d4-a716-446655440010",
  "role": "member",
  "inviteeType": "email",
  "inviteeEmail": "user@example.com",
  "inviteeWallet": null,
  "expiresInHours": 24
}
```

`expiresInHours` 默认为 `24`，最小值为 `1`。

**响应**:

```json
{
  "success": true,
  "data": {
    "invitation": {
      "id": "550e8400-e29b-41d4-a716-446655440030",
      "tenantId": "550e8400-e29b-41d4-a716-446655440010",
      "role": "member",
      "inviteeType": "email",
      "inviteeEmail": "user@example.com",
      "inviteeWallet": null,
      "createdBy": "550e8400-e29b-41d4-a716-446655440000",
      "expiresAt": "2026-04-12T10:00:00Z",
      "consumedAt": null,
      "consumedBy": null,
      "status": "pending",
      "maxUses": 1,
      "useCount": 0,
      "createdAt": "2026-04-11T10:00:00Z"
    },
    "inviteToken": "invite_token_value",
    "inviteUrl": "/invitation/accept?token=invite_token_value"
  }
}
```

#### `POST /api/v1/invitations/:invitation_id/revoke`

**响应**:

```json
{
  "success": true,
  "data": {
    "invitationId": "550e8400-e29b-41d4-a716-446655440030",
    "status": "revoked"
  }
}
```

## Token 管理

### 端点

- `POST /api/v1/tokens`
- `GET /api/v1/tokens`
- `GET /api/v1/tokens/:token_id`
- `POST /api/v1/tokens/verify`
- `GET /api/v1/tokens/stats`
- `POST /api/v1/tokens/:token_id/revoke`

### `POST /api/v1/tokens`

**认证**:
- 需要用户 token
- 需要 active membership
- 需要 `tokens:write` 或 `admin`
- 请求 `scopes` 必须是当前 token scopes 的子集

**请求体**:

```json
{
  "scopes": ["credential:read"],
  "expires_in": 900
}
```

**响应**:

```json
{
  "access_token": "v4.local.xxx",
  "token": "v4.local.xxx",
  "token_id": "550e8400-e29b-41d4-a716-446655440000",
  "token_type": "Bearer",
  "subject_type": "user",
  "issued_from": "access_token",
  "display_name": null,
  "expires_in": 900,
  "scope": "credential:read",
  "granted_scopes": ["credential:read"],
  "issued_at": 1741702800,
  "expires_at": 1741703700,
  "revoked_at": null
}
```

### `GET /api/v1/tokens`

**认证**:
- 需要 `tokens:read`、`tenant:admin` 或 `admin`

返回当前可见 token 元数据数组 `Vec<TokenMetadataResponse>`。

### `GET /api/v1/tokens/:token_id`

**认证**:
- 需要 `tokens:read`、`tenant:admin` 或 `admin`

返回单个 `TokenMetadataResponse`。

### `POST /api/v1/tokens/verify`

**认证**:
- 公开接口，不要求 `Authorization` 头

**请求体**:

```json
{
  "token": "v4.local.xxx"
}
```

**响应**:

```json
{
  "valid": true,
  "token_id": "550e8400-e29b-41d4-a716-446655440000",
  "user_id": "550e8400-e29b-41d4-a716-446655440001",
  "tenant_id": "550e8400-e29b-41d4-a716-446655440010",
  "scopes": ["credential:read"],
  "expires_at": 1741703700,
  "subject_type": "user",
  "issued_from": "access_token"
}
```

### `GET /api/v1/tokens/stats`

**认证**:
- 允许 `tokens:read`、`tenant:admin`、`admin`

**响应**:

```json
{
  "total_tokens": 10,
  "active_tokens": 8,
  "revoked_tokens": 2
}
```

### `POST /api/v1/tokens/:token_id/revoke`

**认证**:
- 撤销自己的 token：不要求额外 `tokens:*` 权限
- 撤销他人的 token：需要 `tokens:revoke`、`tenant:admin` 或 `admin`
- session token 需要通过 `/api/v1/auth/logout` 撤销

**响应**:

```json
{
  "revoked": true,
  "token_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

## Profile Automation Token

### 端点

- `POST /api/v1/profile/automation-tokens`
- `GET /api/v1/profile/automation-tokens` 当前未实现业务列表，固定返回 `404 Not Found`
- `GET /api/v1/profile/automation-tokens/:token_id`
- `POST /api/v1/profile/automation-tokens/:token_id/revoke`

### `POST /api/v1/profile/automation-tokens`

**认证**:
- 需要用户 token
- 需要 active membership
- 需要 `tokens:write` 或 `admin`
- 请求 `scopes` 必须同时是当前 membership scopes 和当前 token scopes 的子集

**请求体**:

```json
{
  "name": "Nightly Sync",
  "description": "Sync automation token",
  "scopes": ["credential:read"],
  "ttl_seconds": 86400,
  "created_via": "sdk"
}
```

**响应**:

```json
{
  "token_value": "v4.local.xxx",
  "token_preview": "v4.local.xxx123456...",
  "token_id": "550e8400-e29b-41d4-a716-446655440000",
  "token_kind": "user_automation",
  "token_name": "Nightly Sync",
  "token_prefix": "v4.local.xxx123456",
  "token_type": "user_access_token",
  "subject_type": "user",
  "subject_id": "550e8400-e29b-41d4-a716-446655440001",
  "tenant_id": "550e8400-e29b-41d4-a716-446655440010",
  "issued_from": "automation",
  "session_id": "550e8400-e29b-41d4-a716-446655440050",
  "membership_id": "550e8400-e29b-41d4-a716-446655440020",
  "display_name": "Nightly Sync",
  "description": "Sync automation token",
  "granted_scopes": ["credential:read"],
  "issued_membership_role_snapshot": "owner",
  "permission_source": "membership_subset",
  "created_via": "sdk",
  "revoked_reason": null,
  "expires_at": "2026-04-12T10:00:00Z",
  "revoked_at": null,
  "created_at": "2026-04-11T10:00:00Z",
  "last_used_at": null
}
```

详情、撤销接口分别返回：
- `TokenMetadataResponse`
- `TokenMetadataResponse`

## 通知

### `GET /api/v1/notifications`

返回 dashboard 顶部通知列表。

**响应**:

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "welcome-user-id",
        "title": "TEE Runtime Ready",
        "message": "可信执行环境与凭证保护链路当前运行正常。",
        "level": "info",
        "is_read": false,
        "created_at": "2026-04-11T10:00:00Z"
      },
      {
        "id": "audit-tenant-id",
        "title": "Audit Stream Active",
        "message": "审计日志链路已同步，最近操作可在审计页面查看。",
        "level": "success",
        "is_read": true,
        "created_at": "2026-04-11T09:50:00Z"
      }
    ],
    "total": 2
  }
}
```

## Service Account

### 端点

- `POST /api/v1/service-accounts`
- `GET /api/v1/service-accounts`
- `GET /api/v1/service-accounts/:service_account_id`
- `PATCH /api/v1/service-accounts/:service_account_id`
- `POST /api/v1/service-accounts/:service_account_id/tokens`
- `GET /api/v1/service-accounts/:service_account_id/tokens`

### 权限

需要 `tenant:admin` 或 `admin`。Service account subject 自身不能管理 service account。

### `POST /api/v1/service-accounts`

**请求体**:

```json
{
  "name": "CI Bot",
  "description": "automation bot",
  "scope_ceiling": ["credential:read", "tokens:read"]
}
```

**响应**: `ApiSuccessResponse<ServiceAccountResponse>`。

### `GET /api/v1/service-accounts`

返回当前 tenant 下的 Service Account 列表。

**响应**:

```json
{
  "success": true,
  "data": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440100",
      "tenant_id": "550e8400-e29b-41d4-a716-446655440010",
      "name": "CI Bot",
      "description": "automation bot",
      "role": "service_account",
      "scope_ceiling": ["credential:read", "tokens:read"],
      "status": "active",
      "created_by": "550e8400-e29b-41d4-a716-446655440000",
      "created_at": "2026-04-11T10:00:00Z",
      "updated_at": "2026-04-11T10:00:00Z",
      "deleted_at": null
    }
  ]
}
```

### `GET /api/v1/service-accounts/:service_account_id`

返回单个 Service Account。

**响应**: `ApiSuccessResponse<ServiceAccountResponse>`。

### `PATCH /api/v1/service-accounts/:service_account_id`

**请求体**:

```json
{
  "name": "CI Bot",
  "description": "updated description",
  "status": "active",
  "scope_ceiling": ["credential:read"]
}
```

**响应**: `ApiSuccessResponse<ServiceAccountResponse>`。

### `POST /api/v1/service-accounts/:service_account_id/tokens`

**请求体**:

```json
{
  "scopes": ["credential:read"],
  "ttl_seconds": 3600,
  "display_name": "CI job token"
}
```

`ttl_seconds` 与 `expires_in` 都可用；如果两者同时提供，值必须相同。

**响应**:

```json
{
  "success": true,
  "data": {
    "access_token": "v4.local.xxx",
    "token": "v4.local.xxx",
    "token_id": "550e8400-e29b-41d4-a716-446655440200",
    "token_type": "Bearer",
    "subject_type": "service_account",
    "issued_from": "service_account",
    "display_name": "CI job token",
    "expires_in": 3600,
    "scope": "credential:read",
    "granted_scopes": ["credential:read"],
    "issued_at": 1741702800,
    "expires_at": 1741706400,
    "revoked_at": null
  }
}
```

### `GET /api/v1/service-accounts/:service_account_id/tokens`

**响应**: `ApiSuccessResponse<Vec<TokenMetadataResponse>>`。

## 凭证管理

### 端点

- `POST /api/v1/credentials`
- `GET /api/v1/credentials`
- `GET /api/v1/credentials/:id`
- `PUT /api/v1/credentials/:id`
- `DELETE /api/v1/credentials/:id`
- `POST /api/v1/credentials/:id/decrypt`

### `POST /api/v1/credentials`

**认证**:
- 需要 `credential:write`

**请求体**:

```json
{
  "service_id": "schwab",
  "credential_type": "username_password",
  "plaintext_data": {
    "username": "alice",
    "password": "secret"
  },
  "expires_at": 1741706400
}
```

`plaintext_data` 兼容别名 `value`。

**支持的 `credential_type`**:
- `username_password`
- `oauth_refresh`
- `oauth_token`
- `o_auth_refresh`
- `api_key`
- `session_cookie`
- `kyc_document`
- `client_certificate`
- `ssh_key`
- `database_connection`

**响应 (201 Created)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "created_at": "2026-04-11 10:00:00 UTC",
  "expires_at": "2026-04-11 12:00:00 UTC"
}
```

### `GET /api/v1/credentials`

**认证**:
- 需要 `credential:read`

**Query**:
- `service_id` 可选
- `credential_type` 可选
- `only_valid` 可选
- `page` 默认 `1`
- `page_size` 默认 `20`，允许 `1..=100`

**响应**:

```json
{
  "credentials": [],
  "total": 0
}
```

### `GET /api/v1/credentials/:id`

**认证**:
- 需要 `credential:read`

**响应**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "created_at": "2026-04-11T10:00:00Z",
  "expires_at": null,
  "is_deleted": false,
  "status": "active"
}
```

### `PUT /api/v1/credentials/:id`

**认证**:
- 需要 `credential:write`

**请求体**:

```json
{
  "plaintext_data": {
    "username": "alice",
    "password": "new-secret"
  },
  "change_reason": "password rotation"
}
```

**响应**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "version": 2,
  "service_id": "schwab",
  "credential_type": "username_password",
  "updated_at": "2026-04-11T10:00:00Z",
  "previous_version": 1
}
```

### `DELETE /api/v1/credentials/:id`

**认证**:
- 需要 `credential:write` 或 `admin`

**响应**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "deleted": true
}
```

### `POST /api/v1/credentials/:id/decrypt`

**认证**:
- 需要 `credential:decrypt`

**请求体**:

```json
{
  "reason": "support-debug"
}
```

`reason` 可省略。

**响应**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "plaintext_data": {
    "username": "alice",
    "password": "secret"
  }
}
```

## 凭证版本

### 端点

- `GET /api/v1/credentials/:id/versions`
- `GET /api/v1/credentials/:id/versions/:version`
- `POST /api/v1/credentials/:id/rollback`

### `GET /api/v1/credentials/:id/versions`

**认证**:
- 需要 `credential:read`

**响应**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "current_version": 3,
  "total": 3,
  "versions": [
    {
      "version": 1,
      "created_at": "2026-04-11T10:00:00Z",
      "changed_by": "user-id",
      "change_reason": "initial create"
    }
  ]
}
```

### `GET /api/v1/credentials/:id/versions/:version`

**认证**:
- 需要 `credential:read`

**响应**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "version": 1,
  "created_at": "2026-04-11T10:00:00Z",
  "changed_by": "user-id",
  "change_reason": "initial create",
  "metadata": {
    "service_id": "schwab",
    "credential_type": "username_password",
    "algorithm": "AES-256-GCM"
  }
}
```

### `POST /api/v1/credentials/:id/rollback`

**认证**:
- 需要 `credential:write`

**请求体**:

```json
{
  "target_version": 1,
  "reason": "rollback to stable version"
}
```

`reason` 是必填。

**响应**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "previous_version": 3,
  "current_version": 4,
  "rollback_to_version": 1,
  "rollback_at": "2026-04-11T10:00:00Z",
  "reason": "rollback to stable version"
}
```

## 审计日志

### 端点

- `GET /api/v1/audit/logs`
- `GET /api/v1/audit/logs/:id`
- `POST /api/v1/audit/export`
- `POST /api/v1/audit/verify`

### 认证

- 需要 `audit:read` 或 `admin`

### `GET /api/v1/audit/logs`

**Query**:
- `start_time` 可选，毫秒时间戳
- `end_time` 可选，毫秒时间戳
- `user_id_hash` 可选
- `action` 可选
- `risk_tier` 可选
- `outcome` 可选
- `service` 可选
- `page` 默认 `1`
- `page_size` 默认 `20`，允许 `1..=1000`

**响应**:

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "audit-id",
        "timestamp": 1741702800000,
        "user_id_hash": "user-hash",
        "session_id": "session-id",
        "service": "vault",
        "action": "credential_decrypt",
        "risk_tier": "high",
        "outcome": "success",
        "log_index": 1
      }
    ],
    "total": 1,
    "page": 1,
    "page_size": 20,
    "total_pages": 1
  }
}
```

### `GET /api/v1/audit/logs/:id`

`id` 可以是审计条目 ID，也可以是数字形式的 `log_index`。

**响应**: `AuditLogDetailResponse`，包含 `content_hash`、`previous_hash`、`merkle_root`、`signer_fingerprint` 与可选 `proof`。

### `POST /api/v1/audit/export`

**请求体**:

```json
{
  "start_time": 1741702800000,
  "end_time": 1741706400000,
  "format": "json",
  "user_id_hash": "user-hash",
  "action": "credential_decrypt"
}
```

`format` 仅支持 `json` 和 `csv`。

**响应**:

```json
{
  "success": true,
  "data": {
    "export_id": "export-id",
    "format": "json",
    "content": "base64-content",
    "integrity_hash": "sha256-hash",
    "count": 10,
    "generated_at": 1741702800000
  }
}
```

### `POST /api/v1/audit/verify`

**请求体**:

```json
{
  "id": "audit-id",
  "log_index": 1
}
```

`log_index` 与 `id` 二选一；若同时存在，`log_index` 优先。

**响应**:

```json
{
  "success": true,
  "status": "valid",
  "data": {
    "id": "audit-id",
    "log_index": 1,
    "verified": true,
    "content_hash_match": true,
    "signature_valid": true,
    "merkle_proof_valid": true,
    "details": [
      {
        "step": "内容哈希验证",
        "passed": true,
        "message": "..."
      }
    ],
    "verified_at": 1741702800000
  }
}
```

## Sandbox

### 端点

- `POST /api/v1/sandbox/sessions`
- `GET /api/v1/sandbox/sessions`
- `GET /api/v1/sandbox/sessions/:id`
- `POST /api/v1/sandbox/sessions/:id/execute`
- `POST /api/v1/sandbox/sessions/:id/pause`
- `POST /api/v1/sandbox/sessions/:id/resume`
- `DELETE /api/v1/sandbox/sessions/:id`
- `POST /api/v1/sandbox/sessions/:id/screenshot`
- `POST /api/v1/sandbox/sessions/:id/export`
- `GET /api/v1/sandbox/operations/:operation_id`
- `GET /api/v1/sandbox/stats`
- `GET /api/v1/sandbox/sessions/:id/ws/:credential_id`

### 认证摘要

- 创建 session: 需要 `sandbox:write` 且同时需要 `credential:decrypt`
- 读取 session/stats: `sandbox:read`
- `execute` / `screenshot` / `export`: `sandbox:execute`
- `pause` / `resume` / `close`: `sandbox:write`

### `POST /api/v1/sandbox/sessions`

**请求体**:

```json
{
  "credential_id": "550e8400-e29b-41d4-a716-446655440000",
  "original_intent": "Fetch account balances",
  "metadata": {
    "source": "dashboard"
  }
}
```

`credential_id` 在 Rust 请求模型里是 `Option<Uuid>`，但 handler 会在运行时校验缺失并报错，因此当前 API 契约应将其视为必填。

**响应**: `ApiSuccessResponse<CreateSessionResponse>`，状态码 `201 Created`。

### `GET /api/v1/sandbox/sessions`

**Query**:
- `status` 可选，允许值为 `creating`、`ready`、`executing`、`paused`、`closed`

**响应**: `ApiSuccessResponse<ListSessionsResponse>`。

### `GET /api/v1/sandbox/sessions/:id`

**响应**: `ApiSuccessResponse<SessionDetailResponse>`。

### `POST /api/v1/sandbox/sessions/:id/execute`

**请求体**:

```json
{
  "operation_type": "navigate",
  "description": "Open dashboard",
  "parameters": {
    "url": "https://example.com"
  }
}
```

**响应**: `ApiSuccessResponse<ExecuteOperationResponse>`。

### `POST /api/v1/sandbox/sessions/:id/pause`
### `POST /api/v1/sandbox/sessions/:id/resume`
### `DELETE /api/v1/sandbox/sessions/:id`

均返回 `ApiSuccessResponse<SessionActionResponse>`。

### `POST /api/v1/sandbox/sessions/:id/screenshot`

返回 `ApiSuccessResponse<ScreenshotResponse>`。

### `POST /api/v1/sandbox/sessions/:id/export`

**请求体**:

```json
{
  "format": "pdf",
  "selectors": ["balances", "positions"]
}
```

当前 `format` 支持：
- `json`
- `csv`
- `pdf`

返回 `ApiSuccessResponse<ExportDataResponse>`。

### `GET /api/v1/sandbox/operations/:operation_id`

返回 `ApiSuccessResponse<OperationDetailResponse>`。

### `GET /api/v1/sandbox/stats`

返回 `ApiSuccessResponse<SandboxStatsApiResponse>`。

### `GET /api/v1/sandbox/sessions/:id/ws/:credential_id`

建立 Sandbox WebSocket 连接。

**认证**:
- 需要 `sandbox:execute`

## 专项文档

- 租户管理: [Tenant API](TENANT-API.md)
- 远程认证: [Attestation API](ATTESTATION-API.md)

## 错误说明

不同模块错误响应并不完全相同：

- `ApiErrorResponse` 模块通常是：
  ```json
  {
    "success": false,
    "error": "invalid_request",
    "message": "错误描述",
    "locale": "zh-CN"
  }
  ```
- 凭证模块使用 `ApiError`，字段为 `code`、`message`，可能附带 `details`
- 审计、租户、attestation 使用各自的 `success/data/error` 结构

更新文档时不要假设整个服务只有一种错误格式。

**更新时间**: 2026-04-11
