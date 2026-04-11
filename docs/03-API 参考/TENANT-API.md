# CredBridge Tenant API

本文档描述当前 `src/api/tenant.rs` 实际实现的租户管理接口。

## 重要说明

当前 tenant 路由整体挂在受保护路由组下，因此所有 `/api/v1/tenants*` 请求都需要先通过路由级认证中间件。

同时，tenant 模块的 handler 层并没有统一执行 `tenant:*` 或 `admin` scope 校验。当前真实情况是：

- 所有 `/api/v1/tenants*` 端点都要求已认证请求
- `POST /api/v1/tenants` 还会显式读取 `Extension<ValidatedToken>` 中的 `user_id`
- 其余 tenant handler 大多没有再做 handler 级 scope 校验

这不是推荐的安全模型，而是当前代码行为。文档按实现记录，不按理想设计记录。

## 端点总览

- `POST /api/v1/tenants`
- `GET /api/v1/tenants`
- `GET /api/v1/tenants/:id`
- `GET /api/v1/tenants/:id/config`
- `PUT /api/v1/tenants/:id/config`
- `POST /api/v1/tenants/:id/activate`
- `POST /api/v1/tenants/:id/suspend`
- `DELETE /api/v1/tenants/:id`

## 通用响应形状

成功响应通常为：

```json
{
  "success": true,
  "data": {},
  "meta": {
    "request_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "timestamp": "2026-04-11T10:00:00Z"
  }
}
```

错误响应通常为：

```json
{
  "success": false,
  "error": {
    "code": "TENANT_NOT_FOUND",
    "message": "租户不存在"
  },
  "meta": {
    "request_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "timestamp": "2026-04-11T10:00:00Z"
  }
}
```

## 创建租户

**Endpoint**: `POST /api/v1/tenants`

**当前实现要求**:
- 需要可用的 `ValidatedToken`
- 当前 handler 未显式校验 `admin` scope
- handler 会强制把当前 token 的 `user_id` 写入 `owner_user_id`

**请求体**:

```json
{
  "name": "Acme Corporation",
  "description": "Enterprise tenant for Acme Corp",
  "tier": "enterprise",
  "create_default_roles": true,
  "init_schema": true,
  "generate_keys": true,
  "bind_owner": true,
  "custom_config": {
    "feature_flags": {
      "enable_mfa": true,
      "enable_webhooks": true,
      "enable_sso": true
    },
    "settings": {
      "timezone": "Asia/Shanghai"
    }
  }
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `name` | string | 是 | 租户名称 |
| `description` | string | 否 | 租户描述 |
| `tier` | string | 否 | 层级，默认 `free` |
| `owner_user_id` | string | 否 | 请求可传，但 handler 会用当前 token 的 `user_id` 覆盖 |
| `owner_external_identity` | object | 否 | owner 外部身份信息 |
| `create_default_roles` | bool | 否 | 默认 `true` |
| `init_schema` | bool | 否 | 默认 `true` |
| `generate_keys` | bool | 否 | 默认 `true` |
| `bind_owner` | bool | 否 | 默认 `true` |
| `custom_config` | object | 否 | 初始租户配置 |

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "name": "Acme Corporation",
    "status": "active",
    "tier": "enterprise",
    "created_at": "2026-04-11T10:00:00Z",
    "updated_at": "2026-04-11T10:00:00Z"
  },
  "initialization": [
    { "step": "create_config", "success": true },
    { "step": "init_schema", "success": true },
    { "step": "generate_keys", "success": true }
  ],
  "meta": {
    "request_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "timestamp": "2026-04-11T10:00:00Z"
  }
}
```

**可能的错误状态**:
- `400 Bad Request`: 名称为空、名称冲突、创建失败兜底分支
- `401 Unauthorized`: 当前 token 中的 `user_id` 无法解析为 UUID
- `500 Internal Server Error`: 租户持久化失败、owner membership 创建失败、用户默认租户更新失败等

## 获取租户列表

**Endpoint**: `GET /api/v1/tenants`

**当前实现要求**:
- 需要已认证请求
- 当前 handler 未显式校验额外 scope

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "tenants": [
      {
        "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
        "name": "Acme Corporation",
        "status": "active",
        "tier": "enterprise",
        "created_at": "2026-04-11T10:00:00Z",
        "updated_at": "2026-04-11T10:00:00Z"
      }
    ],
    "total": 1
  },
  "meta": {
    "request_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "timestamp": "2026-04-11T10:00:00Z"
  }
}
```

## 获取单个租户

**Endpoint**: `GET /api/v1/tenants/:id`

**当前实现要求**:
- 需要已认证请求
- 当前 handler 未显式校验额外 scope

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "name": "Acme Corporation",
    "status": "active",
    "tier": "enterprise",
    "created_at": "2026-04-11T10:00:00Z",
    "updated_at": "2026-04-11T10:00:00Z"
  },
  "meta": {
    "request_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "timestamp": "2026-04-11T10:00:00Z"
  }
}
```

**失败状态**:
- `404 Not Found`: `TENANT_NOT_FOUND`
- `500 Internal Server Error`: `INTERNAL_ERROR`

## 获取租户配置

**Endpoint**: `GET /api/v1/tenants/:id/config`

**当前实现要求**:
- 需要已认证请求
- 当前 handler 未显式校验额外 scope

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "tenant_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "feature_flags": {
      "enable_credential_encryption": true,
      "enable_audit_logging": true,
      "enable_token_revocation": true,
      "enable_mfa": true,
      "enable_remote_attestation": false,
      "enable_auto_rotation": false,
      "allow_cors": false,
      "enable_ip_whitelist": false,
      "enable_webhooks": true,
      "enable_sso": false,
      "enable_custom_crypto": false,
      "enable_advanced_audit": true
    },
    "quota_limits": {
      "max_credentials": 10000,
      "max_tokens_per_user": 20,
      "max_requests_per_minute": 10000,
      "max_users": 1000,
      "max_connectors": 100,
      "max_webhooks": 20,
      "storage_quota_mb": 10240,
      "audit_retention_days": 90,
      "max_token_ttl_seconds": 604800,
      "max_batch_size": 500
    },
    "settings": {
      "token_ttl_seconds": 900,
      "session_timeout_seconds": 3600,
      "max_login_attempts": 5,
      "lockout_duration_seconds": 900,
      "password_min_length": 8,
      "require_password_complexity": true,
      "require_mfa": false,
      "timezone": "Asia/Shanghai",
      "language": "zh-CN"
    },
    "version": 1,
    "updated_at": null,
    "updated_by": null
  },
  "meta": {
    "request_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "timestamp": "2026-04-11T10:00:00Z"
  }
}
```

## 更新租户配置

**Endpoint**: `PUT /api/v1/tenants/:id/config`

**当前实现要求**:
- 需要已认证请求
- 当前 handler 未显式校验额外 scope
- 支持部分更新
- 更新时 `updated_by` 由 handler 固定写为 `api_user`

**请求体**:

```json
{
  "feature_flags": {
    "enable_mfa": true,
    "enable_sso": true
  },
  "quota_limits": {
    "max_credentials": 50000
  },
  "settings": {
    "timezone": "Asia/Shanghai",
    "require_mfa": true
  }
}
```

`feature_flags`、`quota_limits`、`settings` 下的字段全部都是可选增量字段。

**成功响应**: 与“获取租户配置”相同。

**失败状态**:
- `404 Not Found`: `TENANT_NOT_FOUND`
- `400 Bad Request`: `VALIDATION_ERROR`
- `500 Internal Server Error`: `INTERNAL_ERROR`

## 激活租户

**Endpoint**: `POST /api/v1/tenants/:id/activate`

**当前实现要求**:
- 需要已认证请求
- 当前 handler 未显式校验额外 scope

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "status": "active",
    "activated_at": "2026-04-11T10:00:00Z"
  },
  "meta": {
    "request_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "timestamp": "2026-04-11T10:00:00Z"
  }
}
```

## 暂停租户

**Endpoint**: `POST /api/v1/tenants/:id/suspend`

**当前实现要求**:
- 需要已认证请求
- 当前 handler 未显式校验额外 scope
- 请求体可以是任意 JSON；handler 只会尝试读取可选的 `reason` 字符串

**请求体**:

```json
{
  "reason": "Security review required"
}
```

`reason` 可省略。

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "status": "suspended",
    "suspended_at": "2026-04-11T10:00:00Z"
  },
  "meta": {
    "request_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "timestamp": "2026-04-11T10:00:00Z"
  }
}
```

## 删除租户

**Endpoint**: `DELETE /api/v1/tenants/:id`

**当前实现要求**:
- 需要已认证请求
- 当前 handler 未显式校验额外 scope

**成功响应**:
- `204 No Content`

**失败状态**:
- `404 Not Found`: `TENANT_NOT_FOUND`
- `500 Internal Server Error`: `INTERNAL_ERROR`

**更新时间**: 2026-04-11
