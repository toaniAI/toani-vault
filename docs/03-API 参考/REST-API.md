# Toani Vault API 文档

本文档详细描述了 Toani Vault 服务的所有 RESTful API 端点。

## 目录

- [健康检查 API](#健康检查-api)
- [认证 API](#认证-api)
- [凭证管理 API](#凭证管理-api)
- [Token API](#token-api)
- [Service Account API](#service-account-api)
- [租户管理 API](#租户管理-api)
- [沙箱会话 API](#沙箱会话-api)
- [审计日志 API](#审计日志-api)
- [远程认证 API](#远程认证-api)
- [错误处理](#错误处理)

---

## 健康检查 API

### 简单健康检查

检查服务基本运行状态。

**Endpoint**: `GET /health`

**认证**: 不需要

**响应 (200 OK)**:

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "timestamp": 1741702800
}
```

**响应字段说明**:

| 字段        | 类型   | 说明                               |
| ----------- | ------ | ---------------------------------- |
| `status`    | string | 当前主服务实现恒为字面量 `healthy` |
| `version`   | string | 服务版本号（`CARGO_PKG_VERSION`）  |
| `timestamp` | u64    | Unix 时间戳（秒）                  |

---

### 详细健康检查

检查服务及各组件摘要状态（根目录主服务实现，非 `vault-service`）。

**Endpoint**: `GET /health/detail`

**认证**: 不需要

**响应 (200 OK)**:

`TEE_MODE=hardware` 时，`components.enclave` 为 `healthy`：

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "timestamp": 1741702800,
  "components": {
    "vault": "healthy",
    "enclave": "healthy",
    "audit_log": "healthy"
  }
}
```

**响应 (503 Service Unavailable)**:

当整体 `status` 为 `degraded` 时返回（实现上由 `vault` 与 `audit_log` 子状态是否均为 `healthy` 决定；与 `enclave` 取值无关）。当前源码中 `vault`、`audit_log` 占位为 `healthy`，因此常见部署下只会得到 200；若未来接入真实探测，失败时仍为此 JSON 形状，仅字符串取值变化。

```json
{
  "status": "degraded",
  "version": "1.0.0",
  "timestamp": 1741702800,
  "components": {
    "vault": "degraded",
    "enclave": "healthy",
    "audit_log": "healthy"
  }
}
```

**响应字段说明**:

| 字段                   | 类型   | 说明                                                                              |
| ---------------------- | ------ | --------------------------------------------------------------------------------- |
| `status`               | string | 整体状态：`healthy` 或 `degraded`                                                 |
| `version`              | string | 服务版本号（`CARGO_PKG_VERSION`）                                                 |
| `timestamp`            | u64    | Unix 时间戳（秒）                                                                 |
| `components`           | object | 各子系统状态摘要                                                                  |
| `components.vault`     | string | 保险库/存储相关摘要（当前占位为 `healthy`）                                       |
| `components.enclave`   | string | `TEE_MODE=hardware` 时为 `healthy`；`TEE_MODE=simulation` 时为字面量 `simulation` |
| `components.audit_log` | string | 审计日志子系统摘要（当前占位为 `healthy`）                                        |

---

## TEE 运行模式

Toani Vault 通过 `TEE_MODE` 显式选择运行模式，不再根据环境隐式推断：

```bash
TEE_MODE=hardware cargo run
TEE_MODE=simulation cargo run
```

- `TEE_MODE=hardware` 用于真实 SGX/DCAP 路径；当 SGX/DCAP/AESM/PCCS（或 Intel PCS）等前置条件缺失时，服务会 fail-closed，而不是自动回退到 simulation。
- `TEE_MODE=simulation` 用于 simulation-safe 开发与测试；`GET /health/detail` 通过 `components.enclave` 的字面量 `simulation` 标明仿真运行时（无单独的 `simulation_mode` 顶层字段，也不返回 `tee_details` 等扩展块）。
- 当前 Drone 配置将 simulation-safe 后端测试与 hardware-only 测试分层为不同的 cron 流水线；hardware-only 测试仍在专用 SGX runner 或 staging 环境执行，不会随普通 push/PR 自动触发。

**`TEE_MODE=simulation` 示例 (`GET /health/detail`)**：

整体仍为 `healthy`，`enclave` 为 `simulation`：

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "timestamp": 1741702800,
  "components": {
    "vault": "healthy",
    "enclave": "simulation",
    "audit_log": "healthy"
  }
}
```

---

## 认证 API

提供用户认证、会话管理、成员管理和邀请功能。

- [创建会话](#创建会话)
- [创建访问令牌](#创建访问令牌)
- [获取当前用户信息](#获取当前用户信息)
- [获取用户成员资格](#获取用户成员资格)
- [注销会话](#注销会话)
- [获取 MFA 状态](#获取-mfa-状态)
- [同步 MFA 状态](#同步-mfa-状态)
- [创建邀请](#创建邀请)
- [获取邀请列表](#获取邀请列表)
- [撤销邀请](#撤销邀请)
- [消费邀请](#消费邀请)
- [完成用户引导](#完成用户引导)
- [列出成员](#列出成员)
- [更新成员角色](#更新成员角色)
- [移除成员](#移除成员)

### 创建会话

使用 Privy Token 创建会话，自动创建/更新用户和成员资格。

**Endpoint**: `POST /api/v1/auth/session`

**认证**: 需要 Privy Access Token

**请求头**:
```http
Authorization: Bearer <privy_token>
Content-Type: application/json
```

**请求体**:

```json
{
  "invitation_token": "inv_xxx",
  "onboarding_completed": false
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `invitation_token` | string | 否 | 邀请 Token（如果有） |
| `onboarding_completed` | bool | 否 | 是否已完成引导 |

**响应 (200 OK)**:

```json
{
  "access_token": "v4.local.xxx",
  "token_type": "Bearer",
  "expires_in": 7200,
  "user": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "email": "user@example.com",
    "wallet_address": "0x...",
    "status": "active"
  },
  "tenant": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "name": "My Tenant"
  },
  "membership": {
    "id": "550e8400-e29b-41d4-a716-446655440002",
    "role": "owner",
    "status": "active"
  }
}
```

---

### 创建访问令牌

从当前 User Bearer 创建 API Access Token。

**Endpoint**: `POST /api/v1/auth/access-token`

**Scope**: `tokens:write` 或 `admin`

**请求体**:

```json
{
  "scopes": ["credential:read", "credential:write"],
  "expires_in": 900
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `scopes` | array | 是 | 请求的权限范围列表 |
| `expires_in` | u64 | 否 | Token 有效期（秒），默认 900 |

**响应 (201 Created)**:

```json
{
  "access_token": "v4.local.xxx",
  "token_id": "550e8400-e29b-41d4-a716-446655440000",
  "token_type": "Bearer",
  "subject_type": "user",
  "issued_from": "access_token",
  "display_name": null,
  "expires_in": 900,
  "scope": "credential:read credential:write",
  "granted_scopes": ["credential:read", "credential:write"],
  "issued_at": 1709990400,
  "expires_at": 1709991300,
  "revoked_at": null
}
```

---

### 获取当前用户信息

获取当前登录用户的详细信息，包含租户上下文。

**Endpoint**: `GET /api/v1/auth/me`

**认证**: 需要 Bearer Token

**响应 (200 OK)**:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "email": "user@example.com",
  "wallet_address": "0x...",
  "name": "John Doe",
  "avatar_url": "https://...",
  "status": "active",
  "mfa_status": "enabled",
  "created_at": "2024-01-15T10:30:00Z",
  "last_login_at": "2024-03-01T12:00:00Z",
  "default_tenant_id": "550e8400-e29b-41d4-a716-446655440001"
}
```

---

### 获取用户成员资格

获取当前用户在所有租户中的成员资格列表。

**Endpoint**: `GET /api/v1/auth/memberships`

**认证**: 需要 Bearer Token

**响应 (200 OK)**:

```json
{
  "memberships": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440002",
      "tenant_id": "550e8400-e29b-41d4-a716-446655440001",
      "tenant_name": "My Tenant",
      "role": "owner",
      "status": "active",
      "joined_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

---

### 注销会话

撤销当前会话，使 Token 失效。

**Endpoint**: `POST /api/v1/auth/logout`

**认证**: 需要 Bearer Token

**响应 (200 OK)**:

```json
{
  "success": true,
  "message": "Logged out successfully"
}
```

---

### 获取 MFA 状态

获取当前用户的 MFA（多因素认证）状态。

**Endpoint**: `GET /api/v1/auth/mfa-status`

**认证**: 需要 Bearer Token

**响应 (200 OK)**:

```json
{
  "enabled": true,
  "methods": ["totp", "sms"],
  "last_verified_at": "2024-03-01T12:00:00Z"
}
```

---

### 同步 MFA 状态

从 Privy 同步 MFA 状态到本地。

**Endpoint**: `POST /api/v1/auth/mfa-status/sync`

**认证**: 需要 Privy Token

**请求体**:

```json
{
  "privy_token": "..."
}
```

**响应 (200 OK)**:

```json
{
  "success": true,
  "mfa_status": "enabled",
  "synced_at": "2024-03-01T12:00:00Z"
}
```

---

### 创建邀请

创建租户成员邀请（邮箱/钱包/开放链接）。

**Endpoint**: `POST /api/v1/invitations`

**Scope**: `invitations:write` 或 `admin`

**请求体**:

```json
{
  "invitee_type": "email",
  "invitee_value": "user@example.com",
  "role": "member",
  "expires_in_hours": 168
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `invitee_type` | string | 是 | 邀请类型：`email`/`wallet`/`open` |
| `invitee_value` | string | 否 | 邮箱或钱包地址（open 类型可为空） |
| `role` | string | 是 | 角色：`owner`/`admin`/`member`/`readonly` |
| `expires_in_hours` | u32 | 否 | 有效期（小时），默认 168 |

**响应 (201 Created)**:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "token_hash": "abc123...",
  "invitee_type": "email",
  "invitee_email": "user@example.com",
  "role": "member",
  "status": "pending",
  "expires_at": "2024-03-08T12:00:00Z",
  "created_at": "2024-03-01T12:00:00Z"
}
```

---

### 获取邀请列表

获取当前租户的所有邀请。

**Endpoint**: `GET /api/v1/invitations`

**Scope**: `invitations:read` 或 `admin`

**响应 (200 OK)**:

```json
{
  "invitations": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "invitee_type": "email",
      "invitee_email": "user@example.com",
      "role": "member",
      "status": "pending",
      "expires_at": "2024-03-08T12:00:00Z"
    }
  ]
}
```

---

### 撤销邀请

撤销指定的邀请，使其失效。

**Endpoint**: `POST /api/v1/invitations/:invitation_id/revoke`

**Scope**: `invitations:write` 或 `admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `invitation_id` | string | 邀请 ID |

**请求体**:

```json
{
  "reason": "不再需要访问"
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `reason` | string | 否 | 撤销原因 |

**响应 (200 OK)**:

```json
{
  "success": true,
  "invitation_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "revoked",
  "revoked_at": "2024-03-01T12:00:00Z"
}
```

---

### 消费邀请

接受邀请并创建成员资格。

**Endpoint**: `POST /api/v1/invitations/consume`

**认证**: 需要 Privy Token

**请求体**:

```json
{
  "invitation_token": "inv_xxx",
  "privy_token": "..."
}
```

**响应 (200 OK)**:

```json
{
  "success": true,
  "membership": {
    "id": "550e8400-e29b-41d4-a716-446655440002",
    "tenant_id": "550e8400-e29b-41d4-a716-446655440001",
    "role": "member",
    "status": "active"
  }
}
```

---

### 完成用户引导

标记用户引导流程已完成。

**Endpoint**: `POST /api/v1/users/me/onboarding`

**认证**: 需要 Bearer Token

**请求体**:

```json
{
  "steps_completed": ["profile", "mfa", "tenant_setup"]
}
```

**响应 (200 OK)**:

```json
{
  "success": true,
  "onboarding_completed": true,
  "completed_at": "2024-03-01T12:00:00Z"
}
```

---

### 列出成员

获取当前租户的所有成员列表。

**Endpoint**: `GET /api/v1/members`

**Scope**: `members:read` 或 `admin`

**响应 (200 OK)**:

```json
{
  "members": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440002",
      "user_id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "user@example.com",
      "name": "John Doe",
      "role": "owner",
      "status": "active",
      "joined_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

---

### 更新成员角色

更新租户成员的角色。

**Endpoint**: `PATCH /api/v1/members/:membership_id/role`

**Scope**: `members:write` 或 `admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `membership_id` | string | 成员资格 ID |

**请求体**:

```json
{
  "role": "admin"
}
```

**响应 (200 OK)**:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440002",
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "role": "admin",
  "status": "active",
  "updated_at": "2024-03-01T12:00:00Z"
}
```

---

### 移除成员

从租户中移除成员。

**Endpoint**: `DELETE /api/v1/members/:membership_id`

**Scope**: `members:write` 或 `admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `membership_id` | string | 成员资格 ID |

**响应 (204 No Content)**:

无响应体

---

## 凭证管理 API

- [创建凭证](#创建凭证)
- [获取凭证列表](#获取凭证列表)
- [获取凭证详情](#获取凭证详情)
- [解密凭证](#解密凭证)
- [更新凭证](#更新凭证)
- [删除凭证](#删除凭证)
- [获取凭证版本历史](#获取凭证版本历史)
- [获取指定版本详情](#获取指定版本详情)
- [回滚凭证](#回滚凭证)

### 创建凭证

创建新的加密凭证。

**Endpoint**: `POST /api/v1/credentials`

**Scope**: `credential:write`

**请求体**:

```json
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

**请求字段说明**:

| 字段              | 类型   | 必填 | 说明                      |
| ----------------- | ------ | ---- | ------------------------- |
| `service_id`      | string | 是   | 服务标识                  |
| `credential_type` | string | 是   | 凭证类型，见下方枚举      |
| `plaintext_data`  | object | 是   | 明文凭证内容（将被加密）  |
| `expires_at`      | u64    | 否   | 过期时间（Unix 时间戳秒） |

**支持的凭证类型**:

| 类型                            | 说明           |
| ------------------------------- | -------------- |
| `username_password`             | 用户名密码     |
| `oauth_token` / `oauth_refresh` | OAuth 刷新令牌 |
| `api_key`                       | API 密钥       |
| `session_cookie`                | 会话 Cookie    |
| `kyc_document`                  | KYC 文档       |
| `client_certificate`            | 客户端证书     |
| `ssh_key`                       | SSH 密钥       |
| `database_connection`           | 数据库连接     |

**响应 (201 Created)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "created_at": "1709990400",
  "expires_at": "1893456000"
}
```

---

### 获取凭证列表

获取当前用户的所有凭证元数据列表。

**Endpoint**: `GET /api/v1/credentials`

**Scope**: `credential:read`

**查询参数**:

| 参数              | 类型   | 说明               |
| ----------------- | ------ | ------------------ |
| `service_id`      | string | 按服务 ID 过滤     |
| `credential_type` | string | 按凭证类型过滤     |
| `include_deleted` | bool   | 包含已删除的凭证   |
| `only_valid`      | bool   | 仅返回未过期的凭证 |

**响应 (200 OK)**:

```json
{
  "credentials": [
    {
      "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
      "credential_type": "username_password",
      "user_id_hash": "aBcDeFg...",
      "service_id": "schwab",
      "tenant_id": "tenant_123",
      "created_at": "1709990400Z",
      "expires_at": "1893456000Z",
      "is_deleted": false,
      "version": 1,
      "status": "active"
    }
  ],
  "total": 1
}
```

---

### 获取凭证详情

获取指定凭证的完整信息（不含明文）。

**Endpoint**: `GET /api/v1/credentials/:id`

**Scope**: `credential:read`

**路径参数**:

| 参数 | 类型   | 说明              |
| ---- | ------ | ----------------- |
| `id` | string | 凭证 ID (UUID v7) |

**响应 (200 OK)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "created_at": "1709990400Z",
  "expires_at": "1893456000Z",
  "is_deleted": false,
  "status": "active"
}
```

---

### 解密凭证

解密指定凭证并返回明文数据。

**Endpoint**: `POST /api/v1/credentials/:id/decrypt`

**Scope**: `credential:decrypt`

**路径参数**:

| 参数 | 类型   | 说明              |
| ---- | ------ | ----------------- |
| `id` | string | 凭证 ID (UUID v7) |

**请求体**:

```json
{
  "reason": "用户登录操作"
}
```

**响应 (200 OK)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "plaintext_data": {
    "username": "user@example.com",
    "password": "secret_password"
  }
}
```

---

### 删除凭证

软删除指定凭证。

**Endpoint**: `DELETE /api/v1/credentials/:id`

**Scope**: `credential:write` 或 `admin`

**路径参数**:

| 参数 | 类型   | 说明              |
| ---- | ------ | ----------------- |
| `id` | string | 凭证 ID (UUID v7) |

**响应 (200 OK)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "deleted": true
}
```

---

### 更新凭证

更新指定凭证的内容（创建新版本）。

**Endpoint**: `PUT /api/v1/credentials/:id`

**Scope**: `credential:write`

**路径参数**:

| 参数 | 类型   | 说明              |
| ---- | ------ | ----------------- |
| `id` | string | 凭证 ID (UUID v7) |

**请求体**:

```json
{
  "plaintext_data": {
    "username": "user@example.com",
    "password": "new_secret_password"
  },
  "change_reason": "密码定期更换"
}
```

**请求字段说明**:

| 字段             | 类型   | 必填 | 说明                 |
| ---------------- | ------ | ---- | -------------------- |
| `plaintext_data` | object | 是   | 新的明文凭证内容     |
| `change_reason`  | string | 否   | 变更原因（用于审计） |

**响应 (200 OK)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "version": 2,
  "service_id": "schwab",
  "credential_type": "username_password",
  "updated_at": "2024-03-01T12:00:00Z",
  "previous_version": 1
}
```

---

### 获取凭证版本历史

获取指定凭证的所有版本历史。

**Endpoint**: `GET /api/v1/credentials/:id/versions`

**Scope**: `credential:read`

**路径参数**:

| 参数 | 类型   | 说明              |
| ---- | ------ | ----------------- |
| `id` | string | 凭证 ID (UUID v7) |

**响应 (200 OK)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "versions": [
    {
      "version": 2,
      "created_at": "2024-03-01T12:00:00Z",
      "change_reason": "密码定期更换"
    },
    {
      "version": 1,
      "created_at": "2024-01-15T10:30:00Z",
      "change_reason": null
    }
  ]
}
```

---

### 获取指定版本详情

获取凭证指定版本的详细信息。

**Endpoint**: `GET /api/v1/credentials/:id/versions/:version`

**Scope**: `credential:read`

**路径参数**:

| 参数      | 类型   | 说明              |
| --------- | ------ | ----------------- |
| `id`      | string | 凭证 ID (UUID v7) |
| `version` | u32    | 版本号            |

**响应 (200 OK)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "version": 1,
  "service_id": "schwab",
  "credential_type": "username_password",
  "created_at": "2024-01-15T10:30:00Z",
  "change_reason": null
}
```

---

### 回滚凭证

将凭证回滚到指定版本。

**Endpoint**: `POST /api/v1/credentials/:id/rollback`

**Scope**: `credential:write`

**路径参数**:

| 参数 | 类型   | 说明              |
| ---- | ------ | ----------------- |
| `id` | string | 凭证 ID (UUID v7) |

**请求体**:

```json
{
  "version": 1,
  "reason": "新版本配置错误"
}
```

**请求字段说明**:

| 字段      | 类型   | 必填 | 说明       |
| --------- | ------ | ---- | ---------- |
| `version` | u32    | 是   | 目标版本号 |
| `reason`  | string | 否   | 回滚原因   |

**响应 (200 OK)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "version": 3,
  "service_id": "schwab",
  "credential_type": "username_password",
  "updated_at": "2024-03-01T12:00:00Z",
  "rolled_back_from": 2,
  "rolled_back_to": 1
}
```

---

## Token API

- [从 Session 创建 API Access Token](#从-session-创建-api-access-token)
- [创建通用 Token](#创建通用-token)
- [验证 Token](#验证-token)
- [列出 Token 元数据](#列出-token-元数据)
- [获取 Token 元数据详情](#获取-token-元数据详情)
- [撤销 Token](#撤销-token)
- [获取 Token 统计](#获取-token-统计)

### 从当前 User Bearer 创建 API Access Token

**Endpoint**: `POST /api/v1/auth/access-token`

**Scope**: `tokens:write` 或 `admin`

**说明**:

- 接受具备足够 scope 的 user bearer
- 默认 TTL 为 `900s`
- 响应会返回 `subject_type`、`issued_from`、`granted_scopes`

### 创建通用 Token

**Endpoint**: `POST /api/v1/tokens`

**Scope**: `tokens:write` 或 `admin`

**说明**:

- 对 user bearer 行为与 `/auth/access-token` 等价
- service account bearer 不允许再次签发下级 token

### 验证 Token

**Endpoint**: `POST /api/v1/tokens/verify`

**Scope**: `tokens:read` 或 `tenant:admin` 或 `admin`

### 列出 Token 元数据

**Endpoint**: `GET /api/v1/tokens`

**说明**:

- 普通 user 仅可见自身 subject 的 token
- admin/owner 可见当前 tenant 全部 token 元数据

### 获取 Token 元数据详情

**Endpoint**: `GET /api/v1/tokens/:token_id`

**说明**:

- 仅返回元数据，不返回明文 token

### 撤销 Token

**Endpoint**: `POST /api/v1/tokens/:token_id/revoke`

**说明**:

- bearer 可自撤销
- admin/owner 可按 token_id 撤销当前 tenant 任意 token
- 撤销同步更新 blacklist 与 `api_tokens.revoked_at`

### Service Account API

**Endpoints**:

- `POST /api/v1/service-accounts`
- `GET /api/v1/service-accounts`
- `GET /api/v1/service-accounts/:id`
- `PATCH /api/v1/service-accounts/:id`
- `POST /api/v1/service-accounts/:id/tokens`
- `GET /api/v1/service-accounts/:id/tokens`

**说明**:

- service account 是独立主体 (`subject_type=service_account`)
- service account token 默认 TTL `3600s`
- token scopes 必须是 `scope_ceiling` 子集

### 获取 Token 统计

获取当前活跃的 Token 数量统计。

**Endpoint**: `GET /api/v1/tokens/stats`

**Scope**: `admin`

**响应 (200 OK)**:

```json
{
  "total_tokens": 42,
  "active_tokens": 40,
  "revoked_tokens": 2
}
```

---

## Service Account API

Service Account 是独立的主体 (`subject_type=service_account`)，用于自动化和集成场景。

- [创建 Service Account](#创建-service-account)
- [列出 Service Accounts](#列出-service-accounts)
- [获取 Service Account 详情](#获取-service-account-详情)
- [更新 Service Account](#更新-service-account)
- [创建 Service Account Token](#创建-service-account-token)
- [列出 Service Account Tokens](#列出-service-account-tokens)

### 创建 Service Account

创建新的 Service Account。

**Endpoint**: `POST /api/v1/service-accounts`

**Scope**: `admin` 或 `tenant:admin`

**请求体**:

```json
{
  "name": "ci-cd-service",
  "description": "CI/CD 部署服务账号",
  "scope_ceiling": ["credential:read", "credential:write"]
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `name` | string | 是 | Service Account 名称 |
| `description` | string | 否 | 描述 |
| `scope_ceiling` | array | 否 | 允许的最大权限范围 |

**响应 (201 Created)**:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "550e8400-e29b-41d4-a716-446655440001",
  "name": "ci-cd-service",
  "description": "CI/CD 部署服务账号",
  "role": "service_account",
  "scope_ceiling": ["credential:read", "credential:write"],
  "status": "active",
  "created_by": "550e8400-e29b-41d4-a716-446655440002",
  "created_at": "2024-03-01T12:00:00Z",
  "updated_at": "2024-03-01T12:00:00Z",
  "deleted_at": null
}
```

---

### 列出 Service Accounts

获取当前租户的所有 Service Accounts。

**Endpoint**: `GET /api/v1/service-accounts`

**Scope**: `admin` 或 `tenant:admin`

**响应 (200 OK)**:

```json
{
  "data": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "tenant_id": "550e8400-e29b-41d4-a716-446655440001",
      "name": "ci-cd-service",
      "description": "CI/CD 部署服务账号",
      "role": "service_account",
      "scope_ceiling": ["credential:read", "credential:write"],
      "status": "active",
      "created_by": "550e8400-e29b-41d4-a716-446655440002",
      "created_at": "2024-03-01T12:00:00Z",
      "updated_at": "2024-03-01T12:00:00Z",
      "deleted_at": null
    }
  ]
}
```

---

### 获取 Service Account 详情

获取指定 Service Account 的详细信息。

**Endpoint**: `GET /api/v1/service-accounts/:id`

**Scope**: `admin` 或 `tenant:admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | Service Account ID |

**响应 (200 OK)**:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "550e8400-e29b-41d4-a716-446655440001",
  "name": "ci-cd-service",
  "description": "CI/CD 部署服务账号",
  "role": "service_account",
  "scope_ceiling": ["credential:read", "credential:write"],
  "status": "active",
  "created_by": "550e8400-e29b-41d4-a716-446655440002",
  "created_at": "2024-03-01T12:00:00Z",
  "updated_at": "2024-03-01T12:00:00Z",
  "deleted_at": null
}
```

---

### 更新 Service Account

更新 Service Account 信息。

**Endpoint**: `PATCH /api/v1/service-accounts/:id`

**Scope**: `admin` 或 `tenant:admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | Service Account ID |

**请求体**:

```json
{
  "name": "updated-name",
  "description": "Updated description",
  "status": "inactive",
  "scope_ceiling": ["credential:read"]
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `name` | string | 否 | 新名称 |
| `description` | string | 否 | 新描述 |
| `status` | string | 否 | 状态：`active`/`inactive` |
| `scope_ceiling` | array | 否 | 新的权限范围上限 |

**响应 (200 OK)**:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "550e8400-e29b-41d4-a716-446655440001",
  "name": "updated-name",
  "description": "Updated description",
  "role": "service_account",
  "scope_ceiling": ["credential:read"],
  "status": "inactive",
  "created_by": "550e8400-e29b-41d4-a716-446655440002",
  "created_at": "2024-03-01T12:00:00Z",
  "updated_at": "2024-03-01T12:30:00Z",
  "deleted_at": null
}
```

---

### 创建 Service Account Token

为 Service Account 创建访问 Token。

**Endpoint**: `POST /api/v1/service-accounts/:id/tokens`

**Scope**: `admin` 或 `tenant:admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | Service Account ID |

**请求体**:

```json
{
  "scopes": ["credential:read"],
  "ttl_seconds": 3600,
  "display_name": "Production Token"
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `scopes` | array | 是 | Token 权限范围（必须是 scope_ceiling 子集） |
| `ttl_seconds` | u64 | 否 | Token 有效期（秒），默认 3600 |
| `display_name` | string | 否 | Token 显示名称 |

**响应 (201 Created)**:

```json
{
  "access_token": "v4.local.xxx",
  "token_id": "550e8400-e29b-41d4-a716-446655440003",
  "token_type": "Bearer",
  "subject_type": "service_account",
  "issued_from": "service_account",
  "display_name": "Production Token",
  "expires_in": 3600,
  "scope": "credential:read",
  "granted_scopes": ["credential:read"],
  "issued_at": 1709990400,
  "expires_at": 1709994000,
  "revoked_at": null
}
```

---

### 列出 Service Account Tokens

获取 Service Account 的所有 Token 元数据列表。

**Endpoint**: `GET /api/v1/service-accounts/:id/tokens`

**Scope**: `admin` 或 `tenant:admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | Service Account ID |

**响应 (200 OK)**:

```json
{
  "data": [
    {
      "token_id": "550e8400-e29b-41d4-a716-446655440003",
      "token_kind": "service_account",
      "token_name": null,
      "token_prefix": null,
      "token_type": "service_account_token",
      "subject_type": "service_account",
      "subject_id": "550e8400-e29b-41d4-a716-446655440000",
      "tenant_id": "550e8400-e29b-41d4-a716-446655440001",
      "issued_from": "service_account",
      "session_id": null,
      "membership_id": null,
      "display_name": "Production Token",
      "description": null,
      "granted_scopes": ["credential:read"],
      "issued_membership_role_snapshot": null,
      "permission_source": null,
      "created_via": null,
      "revoked_reason": null,
      "expires_at": "2024-03-01T13:00:00Z",
      "revoked_at": null,
      "created_at": "2024-03-01T12:00:00Z",
      "last_used_at": null
    }
  ]
}
```

---

沙箱会话 API 提供 TEE 安全执行环境，用于安全地执行浏览器自动化操作。

- [创建沙箱会话](#创建沙箱会话)
- [列出沙箱会话](#列出沙箱会话)
- [获取会话详情](#获取会话详情)
- [执行操作](#执行操作)
- [暂停会话](#暂停会话)
- [恢复会话](#恢复会话)
- [关闭会话](#关闭会话)
- [截取屏幕截图](#截取屏幕截图)
- [导出数据](#导出数据)
- [获取沙箱统计](#获取沙箱统计)

### 创建沙箱会话

创建一个新的 TEE 沙箱会话。

**Endpoint**: `POST /api/v1/sandbox/sessions`

**Scope**: `sandbox:write`

**请求体**:

```json
{
  "credential_id": "550e8400-e29b-41d4-a716-446655440000",
  "original_intent": "查询投资组合",
  "metadata": {
    "source": "mobile_app",
    "priority": "high"
  }
}
```

**请求字段说明**:

| 字段              | 类型          | 必填 | 说明                                            |
| ----------------- | ------------- | ---- | ----------------------------------------------- |
| `credential_id`   | string (UUID) | 是   | 凭证 ID，用于在沙箱中安全访问凭证               |
| `original_intent` | string        | 是   | 原始意图描述，用于审计和 AI 审核，最大 500 字符 |
| `metadata`        | object        | 否   | 可选的会话元数据                                |

**响应 (201 Created)**:

```json
{
  "session_id": "550e8400-e29b-41d4-a716-446655440001",
  "sandbox_id": "550e8400-e29b-41d4-a716-446655440002",
  "status": "ready",
  "created_at": "2024-01-15T10:30:00Z",
  "expires_at": "2024-01-15T11:00:00Z"
}
```

---

### 列出沙箱会话

获取当前租户的所有活跃沙箱会话列表。

**Endpoint**: `GET /api/v1/sandbox/sessions`

**Scope**: `sandbox:read`

**响应 (200 OK)**:

```json
{
  "sessions": [
    {
      "session_id": "550e8400-e29b-41d4-a716-446655440001",
      "sandbox_id": "550e8400-e29b-41d4-a716-446655440002",
      "credential_id": "550e8400-e29b-41d4-a716-446655440000",
      "status": "ready",
      "original_intent": "查询投资组合",
      "created_at": "2024-01-15T10:30:00Z",
      "expires_at": "2024-01-15T11:00:00Z",
      "is_expired": false
    }
  ],
  "total": 1
}
```

---

### 获取会话详情

获取指定沙箱会话的详细信息。

**Endpoint**: `GET /api/v1/sandbox/sessions/:id`

**Scope**: `sandbox:read`

**路径参数**:

| 参数 | 类型          | 说明    |
| ---- | ------------- | ------- |
| `id` | string (UUID) | 会话 ID |

**响应 (200 OK)**:

```json
{
  "session_id": "550e8400-e29b-41d4-a716-446655440001",
  "sandbox_id": "550e8400-e29b-41d4-a716-446655440002",
  "tenant_id": "550e8400-e29b-41d4-a716-446655440003",
  "user_id": "550e8400-e29b-41d4-a716-446655440004",
  "credential_id": "550e8400-e29b-41d4-a716-446655440000",
  "original_intent": "查询投资组合",
  "status": "ready",
  "created_at": "2024-01-15T10:30:00Z",
  "expires_at": "2024-01-15T11:00:00Z",
  "last_activity_at": "2024-01-15T10:35:00Z",
  "is_expired": false
}
```

**会话状态**:

| 状态        | 说明                 |
| ----------- | -------------------- |
| `creating`  | 会话创建中           |
| `ready`     | 会话就绪，可执行操作 |
| `executing` | 正在执行操作         |
| `paused`    | 会话已暂停           |
| `closed`    | 会话已关闭           |

---

### 执行操作

在沙箱会话中执行浏览器自动化操作。

**Endpoint**: `POST /api/v1/sandbox/sessions/:id/execute`

**Scope**: `sandbox:execute`

**请求体**:

```json
{
  "operation_type": "navigate",
  "description": "导航到登录页面",
  "parameters": {
    "url": "https://example.com/login"
  }
}
```

**请求字段说明**:

| 字段             | 类型   | 必填 | 说明                                  |
| ---------------- | ------ | ---- | ------------------------------------- |
| `operation_type` | string | 是   | 操作类型（见下表）                    |
| `description`    | string | 是   | 操作描述，用于审计日志，最大 500 字符 |
| `parameters`     | object | 否   | 操作参数，根据操作类型不同而变化      |

**操作类型**:

| 类型             | 说明           |
| ---------------- | -------------- |
| `navigate`       | 导航到 URL     |
| `click`          | 点击元素       |
| `fill`           | 填充表单字段   |
| `get_text`       | 获取文本内容   |
| `screenshot`     | 截取屏幕截图   |
| `export`         | 导出数据       |
| `execute_script` | 执行自定义脚本 |
| `wait`           | 等待条件       |
| `custom`         | 自定义操作     |

**响应 (200 OK)**:

```json
{
  "operation_id": "550e8400-e29b-41d4-a716-446655440005",
  "success": true,
  "data": null,
  "error": null,
  "execution_time_ms": 150
}
```

---

### 暂停会话

暂停指定的沙箱会话。

**Endpoint**: `POST /api/v1/sandbox/sessions/:id/pause`

**Scope**: `sandbox:write`

**响应 (200 OK)**:

```json
{
  "session_id": "550e8400-e29b-41d4-a716-446655440001",
  "success": true,
  "status": "paused",
  "message": "Session paused successfully"
}
```

---

### 恢复会话

恢复已暂停的沙箱会话。

**Endpoint**: `POST /api/v1/sandbox/sessions/:id/resume`

**Scope**: `sandbox:write`

**响应 (200 OK)**:

```json
{
  "session_id": "550e8400-e29b-41d4-a716-446655440001",
  "success": true,
  "status": "ready",
  "message": "Session resumed successfully"
}
```

---

### 关闭会话

关闭并清理指定的沙箱会话。

**Endpoint**: `DELETE /api/v1/sandbox/sessions/:id`

**Scope**: `sandbox:write`

**响应 (200 OK)**:

```json
{
  "session_id": "550e8400-e29b-41d4-a716-446655440001",
  "success": true,
  "status": "closed",
  "message": "Session closed successfully"
}
```

---

### 截取屏幕截图

截取沙箱会话的屏幕截图。

**Endpoint**: `POST /api/v1/sandbox/sessions/:id/screenshot`

**Scope**: `sandbox:execute`

**响应 (200 OK)**:

```json
{
  "screenshot_base64": "iVBORw0KGgoAAAANSUhEUgAA...",
  "format": "png",
  "width": 1920,
  "height": 1080
}
```

---

### 导出数据

从沙箱会话导出数据。

**Endpoint**: `POST /api/v1/sandbox/sessions/:id/export`

**Scope**: `sandbox:execute`

**请求体**:

```json
{
  "format": "json",
  "selectors": [".data-table", ".portfolio-item"]
}
```

**请求字段说明**:

| 字段        | 类型   | 必填 | 说明                                 |
| ----------- | ------ | ---- | ------------------------------------ |
| `format`    | string | 是   | 导出格式：`json`、`csv` 或 `html`    |
| `selectors` | array  | 否   | CSS 选择器列表，指定要导出的数据区域 |

**响应 (200 OK)**:

```json
{
  "export_id": "550e8400-e29b-41d4-a716-446655440006",
  "data_base64": "eyJkYXRhIjogW119...",
  "format": "json",
  "filename": "export_550e8400.json",
  "size_bytes": 1024
}
```

---

### 获取沙箱统计

获取沙箱池的统计信息和健康状态。

**Endpoint**: `GET /api/v1/sandbox/stats`

**Scope**: `sandbox:read`

**响应 (200 OK)**:

```json
{
  "pool_status": "healthy",
  "active_sessions": 5,
  "warm_instances": 3,
  "healthy": true,
  "error": null
}
```

---

## 租户管理 API

提供租户生命周期管理和配置管理功能。

- [创建租户](#创建租户)
- [列出租户](#列出租户)
- [获取租户信息](#获取租户信息)
- [删除租户](#删除租户)
- [获取租户配置](#获取租户配置)
- [更新租户配置](#更新租户配置)
- [激活租户](#激活租户)
- [暂停租户](#暂停租户)

### 创建租户

创建新的租户，当前用户自动成为所有者。

**Endpoint**: `POST /api/v1/tenants`

**Scope**: `admin`

**请求体**:

```json
{
  "name": "My Organization",
  "description": "Production tenant",
  "owner_user_id": "550e8400-e29b-41d4-a716-446655440000",
  "config": {
    "feature_flags": {
      "enable_credential_encryption": true,
      "enable_audit_logging": true
    },
    "quota_limits": {
      "max_credentials": 1000,
      "max_tokens_per_user": 10
    }
  }
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `name` | string | 是 | 租户名称（唯一） |
| `description` | string | 否 | 租户描述 |
| `owner_user_id` | string | 否 | 所有者用户 ID（默认为当前用户） |
| `config` | object | 否 | 初始配置 |

**响应 (201 Created)**:

```json
{
  "success": true,
  "data": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "name": "My Organization",
    "status": "active",
    "tier": "pro",
    "created_at": "2024-03-01T12:00:00Z",
    "updated_at": "2024-03-01T12:00:00Z"
  },
  "initialization": [
    {
      "step": "schema_creation",
      "success": true,
      "error": null
    },
    {
      "step": "config_initialization",
      "success": true,
      "error": null
    }
  ],
  "meta": {
    "request_id": "550e8400-e29b-41d4-a716-446655440002",
    "timestamp": "2024-03-01T12:00:00Z"
  }
}
```

---

### 列出租户

获取所有租户列表（管理员）。

**Endpoint**: `GET /api/v1/tenants`

**Scope**: `admin`

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "tenants": [
      {
        "id": "550e8400-e29b-41d4-a716-446655440001",
        "name": "My Organization",
        "status": "active",
        "tier": "pro",
        "created_at": "2024-03-01T12:00:00Z",
        "updated_at": "2024-03-01T12:00:00Z"
      }
    ],
    "total": 1
  },
  "meta": {
    "request_id": "550e8400-e29b-41d4-a716-446655440002",
    "timestamp": "2024-03-01T12:00:00Z"
  }
}
```

---

### 获取租户信息

获取指定租户的详细信息。

**Endpoint**: `GET /api/v1/tenants/:id`

**Scope**: `tenant:read` 或 `admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | 租户 ID |

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "name": "My Organization",
    "status": "active",
    "tier": "pro",
    "created_at": "2024-03-01T12:00:00Z",
    "updated_at": "2024-03-01T12:00:00Z"
  },
  "meta": {
    "request_id": "550e8400-e29b-41d4-a716-446655440002",
    "timestamp": "2024-03-01T12:00:00Z"
  }
}
```

---

### 删除租户

删除指定租户（软删除）。

**Endpoint**: `DELETE /api/v1/tenants/:id`

**Scope**: `admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | 租户 ID |

**响应 (204 No Content)**:

无响应体

---

### 获取租户配置

获取租户的完整配置信息。

**Endpoint**: `GET /api/v1/tenants/:id/config`

**Scope**: `tenant:read` 或 `admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | 租户 ID |

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "tenant_id": "550e8400-e29b-41d4-a716-446655440001",
    "feature_flags": {
      "enable_credential_encryption": true,
      "enable_audit_logging": true,
      "enable_token_revocation": true,
      "enable_mfa": true,
      "enable_remote_attestation": true,
      "enable_auto_rotation": false,
      "allow_cors": true,
      "enable_ip_whitelist": false,
      "enable_webhooks": false,
      "enable_sso": false,
      "enable_custom_crypto": false,
      "enable_advanced_audit": false
    },
    "quota_limits": {
      "max_credentials": 1000,
      "max_tokens_per_user": 10,
      "max_requests_per_minute": 1000,
      "max_users": 100,
      "max_connectors": 10,
      "max_webhooks": 5,
      "storage_quota_mb": 1024,
      "audit_retention_days": 90,
      "max_token_ttl_seconds": 86400,
      "max_batch_size": 100
    },
    "settings": {
      "token_ttl_seconds": 7200,
      "session_timeout_seconds": 3600,
      "max_login_attempts": 5,
      "lockout_duration_seconds": 900,
      "password_min_length": 12,
      "require_password_complexity": true,
      "require_mfa": false,
      "timezone": "UTC",
      "language": "zh-CN"
    },
    "version": 1,
    "updated_at": "2024-03-01T12:00:00Z",
    "updated_by": "admin"
  },
  "meta": {
    "request_id": "550e8400-e29b-41d4-a716-446655440002",
    "timestamp": "2024-03-01T12:00:00Z"
  }
}
```

---

### 更新租户配置

更新租户的配置信息（部分更新）。

**Endpoint**: `PUT /api/v1/tenants/:id/config`

**Scope**: `tenant:write` 或 `admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | 租户 ID |

**请求体**:

```json
{
  "feature_flags": {
    "enable_mfa": true,
    "enable_webhooks": true
  },
  "quota_limits": {
    "max_credentials": 2000,
    "max_users": 200
  },
  "settings": {
    "require_mfa": true,
    "timezone": "Asia/Shanghai"
  }
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `feature_flags` | object | 否 | 功能开关更新 |
| `quota_limits` | object | 否 | 配额限制更新 |
| `settings` | object | 否 | 设置更新 |

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "tenant_id": "550e8400-e29b-41d4-a716-446655440001",
    "feature_flags": {
      "enable_credential_encryption": true,
      "enable_audit_logging": true,
      "enable_token_revocation": true,
      "enable_mfa": true,
      "enable_remote_attestation": true,
      "enable_auto_rotation": false,
      "allow_cors": true,
      "enable_ip_whitelist": false,
      "enable_webhooks": true,
      "enable_sso": false,
      "enable_custom_crypto": false,
      "enable_advanced_audit": false
    },
    "quota_limits": {
      "max_credentials": 2000,
      "max_tokens_per_user": 10,
      "max_requests_per_minute": 1000,
      "max_users": 200,
      "max_connectors": 10,
      "max_webhooks": 5,
      "storage_quota_mb": 1024,
      "audit_retention_days": 90,
      "max_token_ttl_seconds": 86400,
      "max_batch_size": 100
    },
    "settings": {
      "token_ttl_seconds": 7200,
      "session_timeout_seconds": 3600,
      "max_login_attempts": 5,
      "lockout_duration_seconds": 900,
      "password_min_length": 12,
      "require_password_complexity": true,
      "require_mfa": true,
      "timezone": "Asia/Shanghai",
      "language": "zh-CN"
    },
    "version": 2,
    "updated_at": "2024-03-01T12:30:00Z",
    "updated_by": "api_user"
  },
  "meta": {
    "request_id": "550e8400-e29b-41d4-a716-446655440002",
    "timestamp": "2024-03-01T12:30:00Z"
  }
}
```

---

### 激活租户

激活已暂停的租户。

**Endpoint**: `POST /api/v1/tenants/:id/activate`

**Scope**: `admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | 租户 ID |

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "status": "active",
    "activated_at": "2024-03-01T12:00:00Z"
  },
  "meta": {
    "request_id": "550e8400-e29b-41d4-a716-446655440002",
    "timestamp": "2024-03-01T12:00:00Z"
  }
}
```

---

### 暂停租户

暂停租户（禁止访问）。

**Endpoint**: `POST /api/v1/tenants/:id/suspend`

**Scope**: `admin`

**路径参数**:

| 参数 | 类型 | 说明 |
| ---- | ---- | ---- |
| `id` | string | 租户 ID |

**请求体**:

```json
{
  "reason": "Security investigation"
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `reason` | string | 否 | 暂停原因 |

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "status": "suspended",
    "suspended_at": "2024-03-01T12:00:00Z"
  },
  "meta": {
    "request_id": "550e8400-e29b-41d4-a716-446655440002",
    "timestamp": "2024-03-01T12:00:00Z"
  }
}
```

---

- [查询审计日志](#查询审计日志)
- [获取审计日志详情](#获取审计日志详情)
- [导出审计日志](#导出审计日志)
- [验证审计条目](#验证审计条目)

### 查询审计日志

分页查询审计日志，支持多种过滤条件。

**Endpoint**: `GET /api/v1/audit/logs`

**Scope**: `audit:read` 或 `admin`

**查询参数**:

| 参数           | 类型   | 必填 | 说明                                         |
| -------------- | ------ | ---- | -------------------------------------------- |
| `start_time`   | u64    | 否   | 开始时间戳（Unix 毫秒）                      |
| `end_time`     | u64    | 否   | 结束时间戳（Unix 毫秒）                      |
| `action`       | string | 否   | 操作类型过滤                                 |
| `risk_tier`    | string | 否   | 风险等级：Low/Medium/High/Critical           |
| `user_id_hash` | string | 否   | 用户 ID 哈希过滤                             |
| `outcome`      | string | 否   | 结果：Success/Failure/Denied/Timeout/Aborted |
| `service`      | string | 否   | 服务标识过滤                                 |
| `page`         | usize  | 否   | 页码（从 1 开始，默认 1）                    |
| `page_size`    | usize  | 否   | 每页数量（默认 20，最大 1000）               |

**支持的操作类型 (AuditAction)**:

| 操作                 | 风险等级 | 说明         |
| -------------------- | -------- | ------------ |
| `CredentialDecrypt`  | High     | 凭证解密     |
| `CredentialAccess`   | Medium   | 凭证访问     |
| `CredentialCreate`   | Medium   | 凭证创建     |
| `CredentialDelete`   | High     | 凭证删除     |
| `TokenIssue`         | Medium   | Token 签发   |
| `TokenRevoke`        | Medium   | Token 撤销   |
| `TokenValidate`      | Low      | Token 验证   |
| `AuditQuery`         | High     | 审计日志查询 |
| `KeyRotation`        | Critical | 密钥轮换     |
| `AdminLogin`         | Critical | 管理员登录   |
| `SystemConfigChange` | High     | 系统配置变更 |
| `FailedAuth`         | High     | 认证失败     |

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
        "timestamp": 1709990400000,
        "user_id_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "session_id": "session_xyz789",
        "service": "vault-service",
        "action": "CredentialDecrypt",
        "risk_tier": "High",
        "outcome": "Success",
        "log_index": 42
      }
    ],
    "total": 150,
    "page": 1,
    "page_size": 20,
    "total_pages": 8
  }
}
```

**字段说明**:

| 字段               | 类型   | 说明                  |
| ------------------ | ------ | --------------------- |
| `id`               | string | UUID v7 格式的事件 ID |
| `user_id_hash`     | string | SHA-256 哈希的用户 ID |
| `timestamp`        | u64    | UTC 时间戳（毫秒）    |
| `session_id`       | string | 会话标识              |
| `service`          | string | 服务名称              |
| `action`           | string | 操作类型              |
| `risk_tier`        | string | 风险等级              |
| `outcome`          | string | 操作结果              |
| `tee_mrenclave`    | string | TEE MRENCLAVE 测量值  |
| `action_token_jti` | string | Action Token JTI      |
| `chain_index`      | u64    | 审计链中的索引位置    |
| `merkle_root`      | string | Merkle Tree 根哈希    |

---

### 获取审计日志详情

获取单个审计日志条目的完整信息和验证证明。

**Endpoint**: `GET /api/v1/audit/logs/:id`

**Scope**: `audit:read` 或 `admin`

**路径参数**:

| 参数 | 类型   | 说明                  |
| ---- | ------ | --------------------- |
| `id` | string | 审计条目 ID (UUID v7) |

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "timestamp": 1709990400000,
    "user_id_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "session_id": "session_xyz789",
    "service": "vault-service",
    "action": "CredentialDecrypt",
    "risk_tier": "High",
    "outcome": "Success",
    "tee_mrenclave": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    "action_token_jti": "jti_abc123",
    "log_index": 42,
    "content_hash": "a1b2c3d4...",
    "previous_hash": "e5f6a7b8...",
    "merkle_root": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "signer_fingerprint": "signer_fp...",
    "proof": {
      "inclusion_proof": ["abc123...", "def456..."],
      "consistency_proof": [],
      "tree_size": 100,
      "root_hash": "root_hash...",
      "transaction_id": 12345
    }
  }
}
```

**验证字段说明**:

| 字段               | 类型   | 说明                 |
| ------------------ | ------ | -------------------- |
| `verified`         | bool   | 整体验证结果         |
| `signature_valid`  | bool   | Ed25519 签名验证结果 |
| `chain_hash_valid` | bool   | 链式哈希验证结果     |
| `merkle_proof`     | array  | Merkle Tree 包含证明 |
| `state_hash`       | string | immudb 状态哈希      |

---

### 导出审计日志

导出审计日志为 JSON 或 CSV 格式，包含数字签名确保完整性。

**Endpoint**: `POST /api/v1/audit/export`

**Scope**: `audit:read` 或 `admin`

**请求体**:

```json
{
  "start_time": 1704067200000,
  "end_time": 1706745600000,
  "format": "json",
  "user_id_hash": "e3b0c442...",
  "action": "CredentialDecrypt"
}
```

**请求字段说明**:

| 字段           | 类型   | 必填 | 说明                                   |
| -------------- | ------ | ---- | -------------------------------------- |
| `start_time`   | u64    | 否   | 开始时间戳（Unix 毫秒）                |
| `end_time`     | u64    | 否   | 结束时间戳（Unix 毫秒）                |
| `format`       | string | 否   | 导出格式：`json` 或 `csv`（默认 json） |
| `user_id_hash` | string | 否   | 用户 ID 哈希过滤                       |
| `action`       | string | 否   | 操作类型过滤                           |

**注意**: 导出时间范围不能超过 90 天。

**响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "export_id": "550e8400-e29b-41d4-a716-446655440001",
    "format": "json",
    "content": "eyJpZCI6IC4uLn0=...",
    "integrity_hash": "a1b2c3d4e5f6...",
    "count": 150,
    "generated_at": 1709990400000
  }
}
```

**字段说明**:

| 字段                  | 类型   | 说明                      |
| --------------------- | ------ | ------------------------- |
| `success`             | bool   | 请求是否成功              |
| `data.export_id`      | string | 导出唯一标识              |
| `data.format`         | string | 导出格式：json 或 csv     |
| `data.content`        | string | Base64 编码的导出内容     |
| `data.integrity_hash` | string | 内容完整性哈希（SHA-256） |
| `data.count`          | u64    | 导出的条目数量            |
| `data.generated_at`   | u64    | 生成时间戳（毫秒）        |

**JSON 导出格式**:

```json
{
  "export_metadata": {
    "exported_at": "2024-03-01T12:00:00Z",
    "start_time": 1704067200,
    "end_time": 1706745600,
    "entry_count": 150,
    "signature": "base64_encoded_signature"
  },
  "entries": [
    {
      "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
      "user_id_hash": "...",
      "timestamp": 1709990400000,
      "action": "CredentialDecrypt",
      "risk_tier": "High",
      "outcome": "Success"
    }
  ]
}
```

**CSV 导出格式**:

```csv
id,timestamp,user_id_hash,session_id,service,action,risk_tier,outcome,tee_mrenclave,action_token_jti,chain_index
018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c,1709990400000,e3b0c442...,session_xyz789,vault-service,CredentialDecrypt,High,Success,9f86d081...,jti_abc123,42
```

---

### 验证审计条目

验证特定审计条目的完整性和签名。

**Endpoint**: `POST /api/v1/audit/verify`

**Scope**: `audit:read` 或 `admin`

**请求体**:

```json
{
  "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "log_index": 42
}
```

**请求字段说明**:

| 字段        | 类型   | 必填 | 说明                              |
| ----------- | ------ | ---- | --------------------------------- |
| `id`        | string | 可选 | 审计条目 ID (与 log_index 二选一) |
| `log_index` | u64    | 可选 | 日志索引（优先于 ID）             |

**响应 (200 OK)**:

```json
{
  "success": true,
  "status": "valid",
  "data": {
    "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "log_index": 42,
    "verified": true,
    "content_hash_match": true,
    "signature_valid": true,
    "merkle_proof_valid": true,
    "details": [
      {
        "step": "内容哈希验证",
        "passed": true,
        "message": "哈希: a1b2c3d4..."
      },
      {
        "step": "数字签名验证",
        "passed": true,
        "message": "签名者: signer_fp..."
      },
      {
        "step": "Merkle Tree 验证",
        "passed": true,
        "message": "Merkle 根: e3b0c442..."
      }
    ],
    "verified_at": 1709990400000
  }
}
```

**响应字段说明**:

| 字段                      | 类型   | 说明                                            |
| ------------------------- | ------ | ----------------------------------------------- |
| `success`                 | bool   | 请求是否成功处理                                |
| `status`                  | string | 验证状态：`valid`/`invalid`/`not_found`/`error` |
| `data.id`                 | string | 验证的条目 ID                                   |
| `data.log_index`          | u64    | 日志索引位置                                    |
| `data.verified`           | bool   | 整体验证结果                                    |
| `data.content_hash_match` | bool   | 内容哈希匹配结果                                |
| `data.signature_valid`    | bool   | Ed25519 签名验证结果                            |
| `data.merkle_proof_valid` | bool   | Merkle Tree 验证结果                            |
| `data.details`            | array  | 验证详情列表                                    |
| `data.details[].step`     | string | 验证步骤名称                                    |
| `data.details[].passed`   | bool   | 该步骤是否通过                                  |
| `data.details[].message`  | string | 步骤详情信息                                    |
| `data.verified_at`        | u64    | 验证时间戳（毫秒）                              |

---

## 远程认证 API

提供 TEE（可信执行环境）远程认证功能，用于验证运行环境的真实性和完整性。

- [获取当前 Quote](#获取当前-quote)
- [验证 Quote](#验证-quote)
- [获取认证报告](#获取认证报告)
- [创建认证挑战](#创建认证挑战)
- [验证挑战响应](#验证挑战响应)
- [获取认证状态](#获取认证状态)
- [刷新 Quote](#刷新-quote)
- [认证健康检查](#认证健康检查)

### 获取当前 Quote

获取当前运行环境的 SGX Quote。

**Endpoint**: `GET /api/v1/attestation/quote`

**Scope**: `admin` 或 `audit:read`

**响应 (200 OK)**:

```json
{
  "quote": "base64_encoded_quote_data",
  "quote_version": 3,
  "tee_type": "SGX",
  "timestamp": 1709990400
}
```

**响应字段说明**:

| 字段 | 类型 | 说明 |
| ---- | ---- | ---- |
| `quote` | string | Base64 编码的 Quote 数据 |
| `quote_version` | u32 | Quote 版本 |
| `tee_type` | string | TEE 类型（SGX/TDX） |
| `timestamp` | u64 | 生成时间戳 |

---

### 验证 Quote

验证指定的 Quote 是否有效。

**Endpoint**: `POST /api/v1/attestation/verify`

**Scope**: `admin`

**请求体**:

```json
{
  "quote": "base64_encoded_quote_data",
  "nonce": "random_nonce_value"
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `quote` | string | 是 | Base64 编码的 Quote 数据 |
| `nonce` | string | 否 | 随机数，防止重放攻击 |

**响应 (200 OK)**:

```json
{
  "valid": true,
  "verification_result": {
    "signature_valid": true,
    "mrenclave": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    "mrsigner": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "report_data": "...",
    "tcb_level": "upToDate",
    "advisory_ids": []
  }
}
```

**响应字段说明**:

| 字段 | 类型 | 说明 |
| ---- | ---- | ---- |
| `valid` | bool | Quote 是否有效 |
| `verification_result.signature_valid` | bool | 签名验证结果 |
| `verification_result.mrenclave` | string | MRENCLAVE 测量值 |
| `verification_result.mrsigner` | string | MRSIGNER 值 |
| `verification_result.tcb_level` | string | TCB 级别 |
| `verification_result.advisory_ids` | array | 安全公告 ID 列表 |

---

### 获取认证报告

获取完整的认证报告，包含验证证明。

**Endpoint**: `GET /api/v1/attestation/report`

**Scope**: `admin`

**响应 (200 OK)**:

```json
{
  "report_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": 1709990400,
  "tee_info": {
    "tee_type": "SGX",
    "mrenclave": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    "mrsigner": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "isv_prod_id": 1,
    "isv_svn": 1
  },
  "verification": {
    "status": "verified",
    "verified_at": 1709990400,
    "verifier": "Intel DCAP"
  }
}
```

---

### 创建认证挑战

创建新的认证挑战（挑战-响应协议）。

**Endpoint**: `POST /api/v1/attestation/challenge`

**Scope**: `admin`

**响应 (200 OK)**:

```json
{
  "challenge_id": "550e8400-e29b-41d4-a716-446655440001",
  "nonce": "random_32_byte_nonce_base64",
  "quote": "base64_encoded_quote_with_nonce",
  "expires_at": 1709994000
}
```

**响应字段说明**:

| 字段 | 类型 | 说明 |
| ---- | ---- | ---- |
| `challenge_id` | string | 挑战 ID |
| `nonce` | string | Base64 编码的随机数 |
| `quote` | string | 包含 nonce 的 Quote |
| `expires_at` | u64 | 挑战过期时间 |

---

### 验证挑战响应

验证客户端的挑战响应。

**Endpoint**: `POST /api/v1/attestation/verify-response`

**Scope**: `admin`

**请求体**:

```json
{
  "challenge_id": "550e8400-e29b-41d4-a716-446655440001",
  "response": "base64_encoded_response_data",
  "client_quote": "base64_encoded_client_quote"
}
```

**请求字段说明**:

| 字段 | 类型 | 必填 | 说明 |
| ---- | ---- | ---- | ---- |
| `challenge_id` | string | 是 | 挑战 ID |
| `response` | string | 是 | 客户端响应数据 |
| `client_quote` | string | 是 | 客户端 Quote |

**响应 (200 OK)**:

```json
{
  "valid": true,
  "challenge_id": "550e8400-e29b-41d4-a716-446655440001",
  "verified_at": 1709990400,
  "verification_details": {
    "nonce_match": true,
    "quote_valid": true,
    "tcb_up_to_date": true
  }
}
```

---

### 获取认证状态

获取当前认证子系统的状态。

**Endpoint**: `GET /api/v1/attestation/status`

**Scope**: `admin` 或 `audit:read`

**响应 (200 OK)**:

```json
{
  "status": "ready",
  "tee_mode": "hardware",
  "capabilities": {
    "dcap_enabled": true,
    "epid_enabled": false,
    "quote_generation": true,
    "quote_verification": true
  },
  "last_quote_generated_at": 1709990400,
  "quote_valid_until": 1709997600
}
```

**响应字段说明**:

| 字段 | 类型 | 说明 |
| ---- | ---- | ---- |
| `status` | string | 认证状态：`ready`/`error`/`simulation` |
| `tee_mode` | string | TEE 模式：`hardware`/`simulation` |
| `capabilities.dcap_enabled` | bool | DCAP 是否启用 |
| `capabilities.epid_enabled` | bool | EPID 是否启用 |
| `capabilities.quote_generation` | bool | 是否支持 Quote 生成 |
| `capabilities.quote_verification` | bool | 是否支持 Quote 验证 |

---

### 刷新 Quote

刷新当前的 Quote。

**Endpoint**: `POST /api/v1/attestation/refresh`

**Scope**: `admin`

**响应 (200 OK)**:

```json
{
  "success": true,
  "quote": "base64_encoded_new_quote",
  "timestamp": 1709990400,
  "message": "Quote refreshed successfully"
}
```

---

### 认证健康检查

检查认证子系统的健康状态。

**Endpoint**: `GET /api/v1/attestation/health`

**认证**: 不需要

**响应 (200 OK)**:

```json
{
  "healthy": true,
  "tee_available": true,
  "dcap_available": true,
  "pcs_reachable": true,
  "message": "Attestation subsystem is healthy"
}
```

**响应 (503 Service Unavailable)**:

```json
{
  "healthy": false,
  "tee_available": false,
  "dcap_available": false,
  "pcs_reachable": false,
  "message": "TEE not available"
}
```

---

### 错误响应格式

所有错误响应使用以下统一格式：

```json
{
  "error": {
    "code": "invalid_request",
    "message": "请求参数验证失败",
    "details": {
      "field": "start_time",
      "issue": "开始时间不能大于结束时间"
    }
  }
}
```

### 错误码列表

| HTTP 状态码 | 错误码                  | 说明                   |
| ----------- | ----------------------- | ---------------------- |
| 400         | `invalid_request`       | 请求参数无效或缺失     |
| 400         | `invalid_time_range`    | 时间范围无效           |
| 401         | `missing_token`         | 缺少 Authorization 头  |
| 401         | `invalid_token`         | Token 格式无效或过期   |
| 401         | `revoked_token`         | Token 已被撤销         |
| 403         | `insufficient_scope`    | Token 缺少必需的 Scope |
| 403         | `access_denied`         | 访问被拒绝（权限不足） |
| 404         | `not_found`             | 资源不存在             |
| 404         | `audit_entry_not_found` | 审计条目不存在         |
| 404         | `credential_not_found`  | 凭证不存在             |
| 409         | `conflict`              | 资源冲突               |
| 422         | `unprocessable_entity`  | 请求语义错误           |
| 429         | `rate_limited`          | 请求频率超限           |
| 500         | `internal_error`        | 服务器内部错误         |
| 500         | `storage_error`         | 审计存储错误           |
| 503         | `service_unavailable`   | 服务暂时不可用         |

### 认证错误示例

**缺少 Token (401)**:

```json
{
  "error": {
    "code": "missing_token",
    "message": "缺少 Authorization 头"
  }
}
```

**Token 过期 (401)**:

```json
{
  "error": {
    "code": "invalid_token",
    "message": "Token 已过期"
  }
}
```

**权限不足 (403)**:

```json
{
  "error": {
    "code": "insufficient_scope",
    "message": "需要 audit:read 或 admin Scope",
    "details": {
      "required": ["audit:read", "admin"],
      "provided": ["credential:read"]
    }
  }
}
```

### 审计 API 错误示例

**条目不存在 (404)**:

```json
{
  "error": {
    "code": "audit_entry_not_found",
    "message": "审计条目不存在",
    "details": {
      "entry_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c"
    }
  }
}
```

**验证失败 (400)**:

```json
{
  "error": {
    "code": "verification_failed",
    "message": "审计条目验证失败",
    "details": {
      "entry_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
      "signature_valid": false,
      "chain_valid": true
    }
  }
}
```

---

## 速率限制

API 实施速率限制以防止滥用：

| 端点                                      | 限制     |
| ----------------------------------------- | -------- |
| `POST /api/v1/credentials`                | 100/分钟 |
| `GET /api/v1/credentials`                 | 300/分钟 |
| `POST /api/v1/credentials/*/decrypt`      | 60/分钟  |
| `POST /api/v1/sandbox/sessions`           | 60/分钟  |
| `GET /api/v1/sandbox/sessions`            | 120/分钟 |
| `POST /api/v1/sandbox/sessions/*/execute` | 100/分钟 |
| `GET /api/v1/audit/logs`                  | 60/分钟  |
| `POST /api/v1/audit/export`               | 10/分钟  |
| `POST /api/v1/audit/verify`               | 120/分钟 |

超出限制的请求将返回 `429 Too Many Requests` 状态码。

---

## 版本控制

API 版本通过 URL 路径前缀指定：

```
/api/v1/...
```

当前版本: **v1**

---

## 附录

### 时间戳格式

- **查询参数**: 根据端点不同，可能使用 Unix 秒 或 Unix 毫秒
  - 审计查询：`start_time`/`end_time` 使用 **Unix 毫秒** (u64)
  - 凭证创建：`expires_at` 使用 **Unix 秒** (u64)
- **API 响应**: 时间戳使用 **ISO 8601** 格式字符串
- 审计导出元数据使用 ISO 8601 格式

### ID 格式

- **凭证 ID**: UUID v7 (时间排序)
- **审计条目 ID**: UUID v7 (时间排序)
- **会话 ID**: 字符串格式
- **用户 ID 哈希**: SHA-256 十六进制字符串

### 枚举值

**RiskTier**:

- `Low` - 低风险操作
- `Medium` - 中等风险操作
- `High` - 高风险操作
- `Critical` - 关键风险操作

**Outcome**:

- `Success` - 操作成功
- `Failure` - 操作失败
- `Denied` - 操作被拒绝
- `Timeout` - 操作超时
- `Aborted` - 操作中止

**CredentialType**:

- `username_password` - 用户名密码
- `oauth_token` (别名: `oauth_refresh`, `o_auth_refresh`) - OAuth 刷新令牌
- `api_key` - API 密钥
- `session_cookie` - 会话 Cookie
- `kyc_document` - KYC 文档
- `client_certificate` - 客户端证书
- `ssh_key` - SSH 密钥
- `database_connection` - 数据库连接
