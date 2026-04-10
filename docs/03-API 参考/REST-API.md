# Toani Vault API 文档

本文档详细描述了 Toani Vault 服务的所有 RESTful API 端点。

## 目录

- [健康检查 API](#健康检查-api)
- [认证](#认证)
- [凭证管理 API](#凭证管理-api)
- [Token API](#token-api)
- [沙箱会话 API](#沙箱会话-api)
- [审计日志 API](#审计日志-api)
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

## 认证

所有 API 请求必须在 `Authorization` 头中包含 Bearer Token：

```http
Authorization: Bearer <paseto_v4_local_token>
```

### Token Scope 权限

| Scope                | 权限说明                    |
| -------------------- | --------------------------- |
| `credential:read`    | 读取凭证元数据              |
| `credential:decrypt` | 解密凭证获取明文            |
| `credential:write`   | 创建/更新凭证               |
| `sandbox:read`       | 读取沙箱会话信息            |
| `sandbox:write`      | 创建/暂停/恢复/关闭沙箱会话 |
| `sandbox:execute`    | 在沙箱中执行操作            |
| `audit:read`         | 读取审计日志                |
| `admin`              | 所有管理权限                |

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

### 从 Session 创建 API Access Token

**Endpoint**: `POST /api/v1/auth/access-token`

**Scope**: `tokens:write` 或 `admin`

**说明**:

- 仅接受用户 Session bearer
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

## 沙箱会话 API

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

## 审计日志 API

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

## 错误处理

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
