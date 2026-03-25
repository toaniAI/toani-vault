# CredBridge 租户管理 API 文档

本文档描述 CredBridge 多租户系统的配置管理 API。

## 概述

CredBridge 采用 **Schema-per-Tenant + RLS** 的多租户隔离架构。每个租户拥有：

- 独立的 PostgreSQL Schema
- 独立的加密密钥层次
- 可配置的功能开关和配额限制
- 自定义安全策略

## 租户层级

系统支持三种预设层级：

| 层级 | 功能特点 | 配额限制 |
|------|----------|----------|
| **Free** | 基础加密、审计日志 | 100凭证, 3Token/用户, 100请求/分钟 |
| **Pro** | +MFA, Webhook, 高级审计 | 10K凭证, 20Token/用户, 10K请求/分钟 |
| **Enterprise** | +SSO, 自定义加密, 全功能 | 100K凭证, 100Token/用户, 100K请求/分钟 |

## API 端点

### 1. 创建租户

创建新租户并初始化所有资源。

```http
POST /api/v1/tenants
Content-Type: application/json
Authorization: Bearer {admin_token}
```

**请求体：**

```json
{
  "name": "Acme Corporation",
  "description": "Enterprise tenant for Acme Corp",
  "tier": "enterprise",
  "admin_email": "admin@acme.com",
  "create_default_roles": true,
  "init_schema": true,
  "generate_keys": true,
  "custom_config": {
    "feature_flags": {
      "enable_mfa": true,
      "enable_webhooks": true
    },
    "settings": {
      "timezone": "America/New_York"
    }
  }
}
```

**响应：**

```json
{
  "success": true,
  "data": {
    "id": "018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
    "name": "Acme Corporation",
    "status": "active",
    "tier": "enterprise",
    "created_at": "2024-03-11T10:30:00Z",
    "updated_at": "2024-03-11T10:30:00Z"
  },
  "initialization": [
    { "step": "create_config", "success": true },
    { "step": "init_schema", "success": true },
    { "step": "generate_keys", "success": true },
    { "step": "create_roles", "success": true }
  ],
  "meta": {
    "request_id": "req_018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
    "timestamp": "2024-03-11T10:30:00Z"
  }
}
```

### 2. 获取租户配置

获取指定租户的完整配置信息。

```http
GET /api/v1/tenants/{tenant_id}/config
Authorization: Bearer {token}
```

**响应：**

```json
{
  "success": true,
  "data": {
    "tenant_id": "018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
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
      "timezone": "America/New_York",
      "language": "zh-CN"
    },
    "version": 1,
    "updated_at": "2024-03-11T10:30:00Z",
    "updated_by": null
  },
  "meta": {
    "request_id": "req_018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
    "timestamp": "2024-03-11T10:30:00Z"
  }
}
```

### 3. 更新租户配置

更新租户的部分配置，支持增量更新。

```http
PUT /api/v1/tenants/{tenant_id}/config
Content-Type: application/json
Authorization: Bearer {admin_token}
```

**请求体：**

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

**响应：** 同获取配置响应，version 会递增

### 4. 激活租户

激活待处理的租户。

```http
POST /api/v1/tenants/{tenant_id}/activate
Authorization: Bearer {admin_token}
```

**响应：**

```json
{
  "success": true,
  "data": {
    "id": "018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
    "status": "active",
    "activated_at": "2024-03-11T10:35:00Z"
  },
  "meta": {
    "request_id": "req_018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
    "timestamp": "2024-03-11T10:35:00Z"
  }
}
```

### 5. 暂停租户

暂停活跃的租户。

```http
POST /api/v1/tenants/{tenant_id}/suspend
Content-Type: application/json
Authorization: Bearer {admin_token}
```

**请求体：**

```json
{
  "reason": "Security review required"
}
```

**响应：**

```json
{
  "success": true,
  "data": {
    "id": "018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
    "status": "suspended",
    "suspended_at": "2024-03-11T10:40:00Z"
  },
  "meta": {
    "request_id": "req_018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
    "timestamp": "2024-03-11T10:40:00Z"
  }
}
```

### 6. 删除租户

软删除租户（保留审计日志）。

```http
DELETE /api/v1/tenants/{tenant_id}
Authorization: Bearer {admin_token}
```

**响应：** HTTP 204 No Content

### 7. 列出租户

列出所有租户（管理员权限）。

```http
GET /api/v1/tenants
Authorization: Bearer {admin_token}
```

**响应：**

```json
{
  "success": true,
  "data": {
    "tenants": [
      {
        "id": "018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
        "name": "Acme Corporation",
        "status": "active",
        "tier": "enterprise",
        "created_at": "2024-03-11T10:30:00Z",
        "updated_at": "2024-03-11T10:30:00Z"
      }
    ],
    "total": 1
  },
  "meta": {
    "request_id": "req_018e3b7c-6a7f-7a8b-9c0d-1e2f3a4b5c6d",
    "timestamp": "2024-03-11T10:45:00Z"
  }
}
```

## 功能开关说明

| 功能 | 描述 | 默认状态 |
|------|------|----------|
| `enable_credential_encryption` | 启用凭证加密 | ✅ |
| `enable_audit_logging` | 启用审计日志 | ✅ |
| `enable_token_revocation` | 启用 Token 撤销 | ✅ |
| `enable_mfa` | 启用多因素认证 | ❌ |
| `enable_remote_attestation` | 启用远程认证 | ❌ |
| `enable_auto_rotation` | 启用凭证自动轮换 | ❌ |
| `allow_cors` | 允许跨域请求 | ❌ |
| `enable_ip_whitelist` | 启用 IP 白名单 | ❌ |
| `enable_webhooks` | 启用 Webhook 通知 | ❌ |
| `enable_sso` | 启用 SSO 集成 | ❌ |
| `enable_custom_crypto` | 启用自定义加密策略 | ❌ |
| `enable_advanced_audit` | 启用高级审计分析 | ❌ |

## 配额限制说明

| 配额 | 描述 | Free | Pro | Enterprise |
|------|------|------|-----|------------|
| `max_credentials` | 最大凭证数量 | 100 | 10,000 | 100,000 |
| `max_tokens_per_user` | 每用户最大 Token 数 | 3 | 20 | 100 |
| `max_requests_per_minute` | 每分钟最大请求数 | 100 | 10,000 | 100,000 |
| `max_users` | 最大用户数 | 5 | 1,000 | 10,000 |
| `max_connectors` | 最大服务连接器数 | 5 | 100 | 1,000 |
| `max_webhooks` | 最大 Webhook 数 | 0 | 20 | 100 |
| `storage_quota_mb` | 存储配额 (MB) | 100 | 10,240 | 102,400 |
| `audit_retention_days` | 审计日志保留天数 | 7 | 90 | 365 |
| `max_token_ttl_seconds` | Token 最大有效期 (秒) | 3,600 | 604,800 | 2,592,000 |
| `max_batch_size` | 批量操作最大数量 | 10 | 500 | 1,000 |

## 错误码

| 错误码 | 描述 | HTTP 状态码 |
|--------|------|-------------|
| `TENANT_NOT_FOUND` | 租户不存在 | 404 |
| `NAME_EXISTS` | 租户名称已被使用 | 400 |
| `INVALID_NAME` | 无效的租户名称 | 400 |
| `VALIDATION_ERROR` | 配置验证失败 | 400 |
| `INTERNAL_ERROR` | 内部服务器错误 | 500 |

## 权限控制

| 操作 | 所需 Scope |
|------|-----------|
| 创建租户 | `admin` |
| 查看租户配置 | `admin` 或租户成员 |
| 更新租户配置 | `admin` 或 `tenant:admin` |
| 激活/暂停/删除租户 | `admin` |
| 列出租户 | `admin` |

## 审计日志

所有租户管理操作都会记录审计日志：

- **创建租户**: `risk_tier: High`, 记录创建者、租户ID、层级
- **更新配置**: `risk_tier: Medium`, 记录变更字段、更新者
- **激活/暂停/删除**: `risk_tier: High`, 记录操作原因、执行者

## 示例代码

### Rust SDK 示例

```rust
use vault_service::tenant::{
    CreateTenantRequest, TenantConfig, TenantManager
};

// 创建租户
let request = CreateTenantRequest::new("My Organization")
    .with_tier("pro")
    .with_admin_email("admin@example.com");

let result = tenant_manager.create_tenant(request, Some("creator".to_string())).await?;
println!("Tenant created: {}", result.tenant.id);

// 获取配置
let config = tenant_manager.get_config(&tenant_id).await?;
println!("Max credentials: {}", config.quota_limits.max_credentials);

// 更新配置
let partial = PartialTenantConfig {
    feature_flags: Some(FeatureFlags {
        enable_mfa: true,
        ..Default::default()
    }),
    ..Default::default()
};
let updated = tenant_manager.update_config(&tenant_id, partial, "admin"
).await?;
```

### cURL 示例

```bash
# 创建租户
curl -X POST http://localhost:8080/api/v1/tenants \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  -d '{
    "name": "Test Tenant",
    "tier": "pro"
  }'

# 获取配置
curl http://localhost:8080/api/v1/tenants/${TENANT_ID}/config \
  -H "Authorization: Bearer ${TOKEN}"

# 更新配置
curl -X PUT http://localhost:8080/api/v1/tenants/${TENANT_ID}/config \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  -d '{
    "feature_flags": {
      "enable_mfa": true
    }
  }'
```

## 相关文档

- [设计规范](./CredBridge_CN_设计规范_v1.0.md)
- [API 文档](./API.md)
- [架构设计](../_bmad-output/planning-artifacts/architecture.md)
