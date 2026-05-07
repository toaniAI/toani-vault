# ToaniVault API 文档

本文档详细描述了 ToaniVault 保险库服务的所有 RESTful API 端点。

## 目录

- [健康检查 API](#健康检查-api)
- [认证](#认证)
- [凭证管理 API](#凭证管理-api)
- [Sandbox API](#sandbox-api)
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

ToaniVault 通过 `TEE_MODE` 显式选择运行模式，不再根据环境隐式推断：

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

| Scope                | 权限说明         |
| -------------------- | ---------------- |
| `credential:read`    | 读取凭证元数据   |
| `credential:decrypt` | 解密凭证获取明文 |
| `credential:write`   | 创建/更新凭证    |
| `sandbox:execute`    | 执行沙箱操作     |
| `audit:read`         | 读取审计日志     |
| `admin`              | 所有管理权限     |

---

## 凭证管理 API

### 创建凭证

创建新的加密凭证。

**Endpoint**: `POST /api/v1/credentials`

**Scope**: `credential:write`

**请求体**:

```json
{
  "service_id": "okx_trading",
  "credential_type": "api_key",
  "plaintext_data": {
    "api_key": "okx_api_key",
    "secret_key": "okx_secret_key",
    "passphrase": "okx_passphrase"
  },
  "expires_at": 1893456000,
  "provider": "okx",
  "allowed_domains": ["www.okx.com:443", "*.okx.com:443"],
  "custom_functions": [
    {
      "function_name": "recv_window",
      "function_description": "Return a fixed recvWindow for exchange requests",
      "function_body": "export default function main() { return \"5000\"; }"
    }
  ]
}
```

**请求字段说明**:

| 字段               | 类型   | 必填 | 说明 |
| ------------------ | ------ | ---- | ---- |
| `service_id`       | string | 是   | 业务侧服务标识 |
| `credential_type`  | string | 是   | 凭证类型。交易所 REST 模板常用 `api_key` |
| `plaintext_data`   | object | 是   | 加密存储的敏感字段。OKX 常见字段为 `api_key`、`secret_key`、`passphrase`；Binance 常见字段为 `api_key`、`secret_key` |
| `expires_at`       | u64    | 否   | 必须是未来时间的 Unix 秒时间戳 |
| `provider`         | string | 否   | 仅支持 `okx`、`binance`、`custom` |
| `allowed_domains`  | array  | 否   | `http_request` 最终 URL 的域名白名单；空数组表示不额外限制 |
| `custom_functions` | array  | 否   | 模板函数列表。每项包含 `function_name`、可选 `function_description`、必填 `function_body` |

**响应 (201 Created)**:

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "okx_trading",
  "credential_type": "api_key",
  "created_at": "1709990400",
  "expires_at": "1893456000",
  "provider": "okx",
  "allowed_domains": ["www.okx.com:443", "*.okx.com:443"],
  "custom_functions": [
    {
      "function_name": "recv_window",
      "function_description": "Return a fixed recvWindow for exchange requests",
      "function_body": "export default function main() { return \"5000\"; }"
    }
  ]
}
```

**白名单规则**:

- 校验发生在模板渲染完成之后，按最终 URL 的 `host:port` 判断。
- `allowed_domains` 条目可带或不带 scheme；未显式写端口时默认按 `443` 处理。
- 仅支持前导子域通配，例如 `*.okx.com:443`。
- `*.okx.com:443` 允许 `www.okx.com:443`，但不允许 `okx.com:443`、`evil-okx.com:443`、`www.okx.com.evil.com:443`。
- 条目不能包含 path，例如 `https://www.okx.com/path` 会被拒绝。

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
      "credential_type": "api_key",
      "user_id_hash": "aBcDeFg...",
      "service_id": "okx_trading",
      "tenant_id": "tenant_123",
      "created_at": "1709990400Z",
      "expires_at": "1893456000Z",
      "is_deleted": false,
      "version": 1,
      "status": "active",
      "provider": "okx",
      "allowed_domains": ["www.okx.com:443", "*.okx.com:443"],
      "custom_functions": [
        {
          "function_name": "recv_window",
          "function_description": "Return a fixed recvWindow for exchange requests",
          "function_body": "export default function main() { return \"5000\"; }"
        }
      ]
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
  "service_id": "okx_trading",
  "credential_type": "api_key",
  "created_at": "1709990400Z",
  "expires_at": "1893456000Z",
  "is_deleted": false,
  "status": "active",
  "provider": "okx",
  "allowed_domains": ["www.okx.com:443", "*.okx.com:443"],
  "custom_functions": [
    {
      "function_name": "recv_window",
      "function_description": "Return a fixed recvWindow for exchange requests",
      "function_body": "export default function main() { return \"5000\"; }"
    }
  ]
}
```

---

### 解密凭证

直接解密接口当前被禁用。请改为创建绑定凭证的 sandbox session，再通过受控 `fill` 或 `http_request` 在 TEE 内消费密钥。

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

**响应 (403 Forbidden)**:

```json
{
  "error": "forbidden",
  "message": "Direct credential decryption is disabled; use sandbox execution instead"
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

## Sandbox API

### 执行沙箱操作

对已创建的 sandbox session 执行操作。交易所 REST API 调用使用 `operation_type=http_request`。

**Endpoint**: `POST /api/v1/sandbox/sessions/:id/execute`

**Scope**: `sandbox:execute`

**路径参数**:

| 参数 | 类型   | 说明 |
| ---- | ------ | ---- |
| `id` | string | sandbox 会话 ID |

**请求体通用结构**:

```json
{
  "operation_type": "http_request",
  "description": "Fetch exchange data with credential-backed templates",
  "parameters": {
    "method": "GET",
    "url": "https://www.okx.com/api/v5/account/balance",
    "headers": {
      "OK-ACCESS-KEY": "${credential.api_key}"
    }
  }
}
```

**模板能力**:

- `http_request` 支持在 `url`、`query`、`headers`、`body` 中渲染 `${credential.xxx}`。
- 内置函数包括 `${functions.okx_timestamp()}`、`${functions.okx_sign()}`、`${functions.binance_timestamp()}`、`${functions.binance_sign()}`。
- 自定义函数通过凭证上的 `custom_functions` 提供，必须返回字符串。
- 模板渲染完成后才执行 `allowed_domains` 校验。

**OKX 示例**:

```json
{
  "operation_type": "http_request",
  "description": "OKX balance request",
  "parameters": {
    "method": "GET",
    "url": "https://www.okx.com/api/v5/account/balance",
    "headers": {
      "OK-ACCESS-KEY": "${credential.api_key}",
      "OK-ACCESS-TIMESTAMP": "${functions.okx_timestamp()}",
      "OK-ACCESS-PASSPHRASE": "${credential.passphrase}",
      "OK-ACCESS-SIGN": "${functions.okx_sign()}"
    }
  }
}
```

**Binance 示例**:

```json
{
  "operation_type": "http_request",
  "description": "Binance signed account request",
  "parameters": {
    "method": "GET",
    "url": "https://api.binance.com/api/v3/account",
    "query": {
      "recvWindow": "${functions.recv_window()}",
      "timestamp": "${functions.binance_timestamp()}",
      "signature": "${functions.binance_sign()}"
    },
    "headers": {
      "X-MBX-APIKEY": "${credential.api_key}"
    }
  }
}
```

**自定义函数执行约束**:

- `function_body` 必须 `export default` 一个返回字符串的函数。
- 运行时可读取 `credential`、`provider`、`method`、`url`、`query`、`headers`、`body`。
- `fetch`、`XMLHttpRequest`、`WebSocket`、`process.env` 在运行时被禁用。

---

## 审计日志 API

### 查询审计日志

分页查询审计日志，支持多种过滤条件。

**Endpoint**: `GET /api/v1/audit/logs`

**Scope**: `audit:read` 或 `admin`

**查询参数**:

| 参数           | 类型   | 必填 | 说明                                         |
| -------------- | ------ | ---- | -------------------------------------------- |
| `start_time`   | u64    | 否   | 开始时间戳（Unix 秒）                        |
| `end_time`     | u64    | 否   | 结束时间戳（Unix 秒）                        |
| `action`       | string | 否   | 操作类型过滤                                 |
| `risk_tier`    | string | 否   | 风险等级：Low/Medium/High/Critical           |
| `user_id_hash` | string | 否   | 用户 ID 哈希过滤                             |
| `outcome`      | string | 否   | 结果：Success/Failure/Denied/Timeout/Aborted |
| `limit`        | u32    | 否   | 返回条数限制（默认 20，最大 100）            |
| `offset`       | u32    | 否   | 分页偏移量（默认 0）                         |

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
  "entries": [
    {
      "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
      "user_id_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "timestamp": 1709990400000,
      "session_id": "session_xyz789",
      "service": "vault-service",
      "action": "CredentialDecrypt",
      "risk_tier": "High",
      "outcome": "Success",
      "tee_mrenclave": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
      "action_token_jti": "jti_abc123",
      "chain_index": 42,
      "merkle_root": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    }
  ],
  "total": 150,
  "limit": 20,
  "offset": 0,
  "has_more": true
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
  "entry": {
    "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "user_id_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "timestamp": 1709990400000,
    "session_id": "session_xyz789",
    "service": "vault-service",
    "action": "CredentialDecrypt",
    "risk_tier": "High",
    "outcome": "Success",
    "tee_mrenclave": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    "action_token_jti": "jti_abc123",
    "chain_index": 42,
    "merkle_root": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  },
  "verification": {
    "verified": true,
    "signature_valid": true,
    "chain_hash_valid": true,
    "merkle_proof": ["abc123...", "def456..."],
    "state_hash": "a1b2c3d4..."
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
  "start_time": 1704067200,
  "end_time": 1706745600,
  "format": "json",
  "include_verification": true
}
```

**请求字段说明**:

| 字段                   | 类型   | 必填 | 说明                                   |
| ---------------------- | ------ | ---- | -------------------------------------- |
| `start_time`           | u64    | 否   | 开始时间戳（Unix 秒）                  |
| `end_time`             | u64    | 否   | 结束时间戳（Unix 秒）                  |
| `format`               | string | 否   | 导出格式：`json` 或 `csv`（默认 json） |
| `include_verification` | bool   | 否   | 是否包含验证签名（默认 false）         |

**响应 (200 OK)**:

```json
{
  "format": "json",
  "filename": "audit_export_20240301_120000.json",
  "content": "base64_encoded_export_content",
  "entry_count": 150,
  "signature": "base64_encoded_signature",
  "exported_at": "2024-03-01T12:00:00Z",
  "expires_at": "2024-03-08T12:00:00Z"
}
```

**字段说明**:

| 字段          | 类型   | 说明                         |
| ------------- | ------ | ---------------------------- |
| `format`      | string | 导出格式                     |
| `filename`    | string | 建议的文件名                 |
| `content`     | string | Base64 编码的导出内容        |
| `entry_count` | u64    | 导出的条目数量               |
| `signature`   | string | 导出内容的数字签名（可选）   |
| `exported_at` | string | 导出时间（ISO 8601）         |
| `expires_at`  | string | 导出文件过期时间（ISO 8601） |

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
  "entry_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "include_proof": true
}
```

**请求字段说明**:

| 字段            | 类型   | 必填 | 说明                               |
| --------------- | ------ | ---- | ---------------------------------- |
| `entry_id`      | string | 是   | 审计条目 ID (UUID v7)              |
| `include_proof` | bool   | 否   | 是否包含 Merkle Proof（默认 true） |
| `chain_index`   | u64    | 否   | 链索引（可选，用于直接索引验证）   |

**响应 (200 OK)**:

```json
{
  "verified": true,
  "entry_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "chain_index": 42,
  "signature_valid": true,
  "chain_hash_valid": true,
  "merkle_root": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "state_hash": "a1b2c3d4e5f6...",
  "verification_timestamp": "2024-03-01T12:00:00Z",
  "proof": {
    "inclusion_proof": ["abc123...", "def456..."],
    "transaction_id": 12345,
    "root_hash": "a1b2c3d4e5f6..."
  }
}
```

**响应字段说明**:

| 字段                     | 类型   | 说明                     |
| ------------------------ | ------ | ------------------------ |
| `verified`               | bool   | 整体验证结果             |
| `entry_id`               | string | 验证的条目 ID            |
| `chain_index`            | u64    | 链索引位置               |
| `signature_valid`        | bool   | Ed25519 签名验证         |
| `chain_hash_valid`       | bool   | 链式哈希验证             |
| `merkle_root`            | string | Merkle Tree 根哈希       |
| `state_hash`             | string | immudb 状态哈希          |
| `verification_timestamp` | string | 验证时间（ISO 8601）     |
| `proof`                  | object | 验证证明详情             |
| `proof.inclusion_proof`  | array  | Merkle Tree 包含证明路径 |
| `proof.transaction_id`   | u64    | immudb 事务 ID           |
| `proof.root_hash`        | string | 验证时的根哈希           |

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

| 端点                                 | 限制     |
| ------------------------------------ | -------- |
| `POST /api/v1/credentials`           | 100/分钟 |
| `GET /api/v1/credentials`            | 300/分钟 |
| `POST /api/v1/credentials/*/decrypt` | 60/分钟  |
| `GET /api/v1/audit/logs`             | 60/分钟  |
| `POST /api/v1/audit/export`          | 10/分钟  |
| `POST /api/v1/audit/verify`          | 120/分钟 |

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

- 查询参数使用 **Unix 秒** (u64)
- API 响应使用 **Unix 毫秒** (u64)
- ISO 8601 格式用于导出元数据

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

- `username_password`
- `api_key`
- `oauth_token`
- `certificate`
- `ssh_key`
- `database_connection`
