# CredBridge Attestation API

本文档描述当前 `src/api/attestation.rs` 实际实现的远程认证接口。

## 端点总览

- `GET /api/v1/attestation/quote`
- `POST /api/v1/attestation/verify`
- `GET /api/v1/attestation/report`
- `POST /api/v1/attestation/challenge`
- `POST /api/v1/attestation/verify-response`
- `GET /api/v1/attestation/status`
- `POST /api/v1/attestation/refresh`
- `GET /api/v1/attestation/health`

## 认证要求

当前 attestation 路由是公开访问的，handler 层没有 token 或 scope 校验。

## 获取 Quote

**Endpoint**: `GET /api/v1/attestation/quote`

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "version": 3,
    "sign_type": 2,
    "mrenclave": "...",
    "mrsigner": "...",
    "timestamp": 1741702800,
    "quote_b64": "base64_encoded_quote"
  },
  "error": null
}
```

**失败状态**:
- `500 Internal Server Error`: DCAP/内部序列化失败
- `503 Service Unavailable`: 当前没有可用 quote

## 验证 Quote

**Endpoint**: `POST /api/v1/attestation/verify`

**请求体**:

```json
{
  "quote_b64": "base64_encoded_quote",
  "nonce": "base64_encoded_nonce"
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `quote_b64` | string | 是 | Base64 编码的 quote |
| `nonce` | string | 否 | Base64 编码的 nonce，不是 hex |

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "valid": true,
  "mrenclave": "...",
  "mrsigner": "...",
  "timestamp": 1741702800,
  "error": null
}
```

注意：当 quote 内容非法或验证失败时，当前实现通常仍返回 `200 OK`，只是 `valid: false` 且 `error` 带失败原因。

**失败状态**:
- `400 Bad Request`: `quote_b64` 不是合法 base64

## 获取认证报告

**Endpoint**: `GET /api/v1/attestation/report`

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "data": {
    "version": "3",
    "mrenclave": "...",
    "mrsigner": "...",
    "security_version": 1,
    "product_id": 1,
    "attributes": "...",
    "timestamp": 1741702800,
    "quote_b64": "base64_encoded_quote",
    "certificate_info": {
      "subject": "CN=Intel SGX Attestation",
      "issuer": "CN=Intel SGX Root CA",
      "not_before": "2026-01-01T00:00:00Z",
      "not_after": "2027-01-01T00:00:00Z",
      "fingerprint": "..."
    }
  },
  "error": null
}
```

**失败状态**:
- `500 Internal Server Error`
- `503 Service Unavailable`

## 创建认证挑战

**Endpoint**: `POST /api/v1/attestation/challenge`

**请求体**:

```json
{
  "enclave_id": "optional_enclave_identifier"
}
```

`enclave_id` 可省略。

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "challenge_id": "chal_abc123",
  "nonce": "a1b2c3d4e5f6...",
  "quote_b64": "base64_encoded_quote",
  "expires_at": 1741703100,
  "mrenclave": "...",
  "mrsigner": "...",
  "error": null
}
```

注意：这里返回的 `nonce` 是 hex 字符串。

**失败状态**:
- `503 Service Unavailable`: enclave 未运行或 challenge store 不可用
- `500 Internal Server Error`: 其他内部错误

## 验证挑战响应

**Endpoint**: `POST /api/v1/attestation/verify-response`

**请求体**:

```json
{
  "challenge_id": "chal_abc123",
  "quote_b64": "base64_encoded_quote",
  "signature_b64": "optional_signature"
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `challenge_id` | string | 是 | 挑战 ID |
| `quote_b64` | string | 是 | Base64 编码的 quote |
| `signature_b64` | string | 否 | 当前实现未实际使用该字段 |

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "verified": true,
  "mrenclave": "...",
  "mrsigner": "...",
  "timestamp": 1741702800,
  "error": null
}
```

注意：当前实现里，很多业务失败路径也返回 `200 OK`，只是 `verified: false` 并通过 `error` 字段表达失败原因，例如：

- challenge 不存在
- challenge 已过期
- quote 校验失败

**失败状态**:
- `400 Bad Request`: `quote_b64` 不是合法 base64

## 获取认证状态

**Endpoint**: `GET /api/v1/attestation/status`

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "status": "authenticated",
  "health_status": "ready",
  "ready": true,
  "requested_mode": "hardware",
  "effective_mode": "hardware",
  "root_key_source": "sgx_seal",
  "detected_type": "intel_sgx",
  "hardware_available": true,
  "remote_attestation_available": true,
  "enclave_state": "running",
  "mrenclave": "...",
  "mrsigner": "...",
  "quote_valid": true,
  "quote_expires_at": 1741706400,
  "last_quote_generated_at": 1741702800,
  "last_verified_at": 1741702800,
  "error": null
}
```

**`status` 可取值**:
- `authenticated`
- `pending_verification`
- `expired`
- `failed`
- `uninitialized`

## 刷新 Quote

**Endpoint**: `POST /api/v1/attestation/refresh`

当前实现不读取请求体。旧文档里的 `challenge` 字段不是当前契约。

**成功响应 (200 OK)**:

```json
{
  "success": true,
  "new_quote_b64": "base64_encoded_quote",
  "timestamp": 1741702800,
  "error": null
}
```

**失败状态**:
- `500 Internal Server Error`

## 健康检查

**Endpoint**: `GET /api/v1/attestation/health`

**成功响应 (200 OK)**:

```json
{
  "status": "ready",
  "ready": true,
  "requested_mode": "hardware",
  "effective_mode": "hardware",
  "root_key_source": "sgx_seal",
  "detected_type": "intel_sgx",
  "enclave_state": "running",
  "dcap_version": "1.0.0",
  "quote_valid": true,
  "quote_expires_at": 1741706400,
  "error": null
}
```

**字段说明**:

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `status` | string | 来自运行时自检，当前实现使用 `ready`、`degraded`、`failed` |
| `ready` | bool | 是否 ready |
| `requested_mode` | string | 请求的运行模式 |
| `effective_mode` | string | 实际生效的运行模式 |
| `root_key_source` | string | 根密钥来源 |
| `detected_type` | string | 检测到的 TEE 类型 |
| `enclave_state` | string | enclave 当前状态 |
| `dcap_version` | string | DCAP 版本 |
| `quote_valid` | bool | quote 是否仍有效 |
| `quote_expires_at` | number/null | quote 过期时间 |
| `error` | string/null | 错误信息 |

**状态码**:
- `200 OK`: `ready == true`
- `503 Service Unavailable`: `ready == false`

**更新时间**: 2026-04-11
