# CredBridge 远程认证 API 文档

## 概述

CredBridge 远程认证 API 提供基于 Intel SGX DCAP 的远程认证服务，支持挑战-响应协议来验证 Enclave 身份。

## 架构

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          远程认证流程                                      │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   客户端                        CredBridge API                  Enclave  │
│     │                                │                            │     │
│     │  1. POST /attestation/challenge │                            │     │
│     │───────────────────────────────>│                            │     │
│     │                                │                            │     │
│     │  2. 返回 {nonce, quote}        │                            │     │
│     │<───────────────────────────────│                            │     │
│     │                                │                            │     │
│     │  3. POST /attestation/verify-response                      │     │
│     │───────────────────────────────│───────────────────────────>│     │
│     │                                │                            │     │
│     │  4. 返回 {verified, mrenclave} │                            │     │
│     │<───────────────────────────────│────────────────────────────│     │
│     │                                │                            │     │
│     │  5. GET /attestation/status    │                            │     │
│     │───────────────────────────────>│                            │     │
│     │                                │                            │     │
│     │  6. 返回 {status, quote_valid} │                            │     │
│     │<───────────────────────────────│                            │     │
│     │                                │                            │     │
│     │  7. POST /attestation/verify   │                            │     │
│     │───────────────────────────────>│                            │     │
│     │                                │                            │     │
│     │  8. 返回 {valid, mrenclave}    │                            │     │
│     │<───────────────────────────────│                            │     │
│     │                                │                            │     │
│     │  9. GET /attestation/report    │                            │     │
│     │───────────────────────────────>│                            │     │
│     │                                │                            │     │
│     │  10. 返回 {report data}        │                            │     │
│     │<───────────────────────────────│                            │     │
│     │                                │                            │     │
│     │  11. POST /attestation/refresh │                            │     │
│     │───────────────────────────────>│                            │     │
│     │                                │                            │     │
│     │  12. 返回 {new_quote}          │                            │     │
│     │<───────────────────────────────│                            │     │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## API 端点

### 1. 创建认证挑战

创建一个新的认证挑战，返回挑战 nonce 和 SGX Quote。

**请求**

```http
POST /api/v1/attestation/challenge
Content-Type: application/json

{
  "enclave_id": "optional_enclave_identifier"
}
```

**响应**

```json
{
  "success": true,
  "challenge_id": "chal_abc123",
  "nonce": "a1b2c3d4e5f6...",
  "quote_b64": "base64_encoded_quote",
  "expires_at": 1709654400,
  "mrenclave": "e3b0c44298fc1c149afbf4c8996fb924...",
  "mrsigner": "7d865e959b2466918c9863afca942d0f...",
  "error": null
}
```

**字段说明**

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | boolean | 请求是否成功 |
| `challenge_id` | string | 挑战唯一标识符 |
| `nonce` | string | 32字节随机挑战值（hex编码） |
| `quote_b64` | string | SGX Quote（base64编码） |
| `expires_at` | number | 挑战过期时间戳（Unix秒） |
| `mrenclave` | string | Enclave 测量值（MRENCLAVE） |
| `mrsigner` | string | 签名者测量值（MRSIGNER） |
| `error` | string | 错误信息（如果失败） |

**错误码**

| HTTP 状态码 | 错误场景 |
|-------------|----------|
| 500 | Enclave 未运行或内部错误 |
| 503 | 服务暂时不可用 |

---

### 2. 验证挑战响应

验证客户端返回的挑战响应，确认客户端拥有正确的密钥。

**请求**

```http
POST /api/v1/attestation/verify-response
Content-Type: application/json

{
  "challenge_id": "chal_abc123",
  "quote_b64": "base64_encoded_quote",
  "signature_b64": "optional_signature"
}
```

**响应**

```json
{
  "success": true,
  "verified": true,
  "mrenclave": "e3b0c44298fc1c149afbf4c8996fb924...",
  "mrsigner": "7d865e959b2466918c9863afca942d0f...",
  "timestamp": 1709654400,
  "error": null
}
```

**字段说明**

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | boolean | API 调用是否成功 |
| `verified` | boolean | 认证是否通过 |
| `mrenclave` | string | 验证通过的 MRENCLAVE |
| `mrsigner` | string | 验证通过的 MRSIGNER |
| `timestamp` | number | 验证时间戳 |
| `error` | string | 验证失败原因 |

**错误码**

| HTTP 状态码 | 错误场景 |
|-------------|----------|
| 400 | 无效的 base64 编码或 Quote 格式 |
| 500 | 内部错误 |

**安全特性**

- **重放攻击防护**: 每个挑战只能使用一次，验证后自动失效
- **挑战过期**: 挑战默认5分钟后过期
- **测量值验证**: 验证 MRENCLAVE/MRSIGNER 是否在白名单中

---

### 3. 获取认证状态

查询当前 Enclave 的认证状态。

**请求**

```http
GET /api/v1/attestation/status
```

**响应**

```json
{
  "success": true,
  "status": "authenticated",
  "enclave_state": "running",
  "mrenclave": "e3b0c44298fc1c149afbf4c8996fb924...",
  "mrsigner": "7d865e959b2466918c9863afca942d0f...",
  "quote_valid": true,
  "quote_expires_at": 1709658000,
  "last_verified_at": 1709654400,
  "error": null
}
```

**状态值**

| 状态 | 说明 |
|------|------|
| `authenticated` | 已认证，Quote 有效 |
| `pending_verification` | 待验证，Quote 未生成或无效 |
| `expired` | Quote 已过期 |
| `uninitialized` | Enclave 未初始化 |

**字段说明**

| 字段 | 类型 | 说明 |
|------|------|------|
| `status` | string | 当前认证状态 |
| `enclave_state` | string | Enclave 运行状态 |
| `quote_valid` | boolean | 当前 Quote 是否有效 |
| `quote_expires_at` | number | Quote 过期时间 |
| `last_verified_at` | number | 最后验证时间 |

---

### 4. 获取 Quote

获取当前 Enclave 的 SGX Quote。

**请求**

```http
GET /api/v1/attestation/quote
```

**响应**

```json
{
  "success": true,
  "data": {
    "version": 3,
    "sign_type": 2,
    "mrenclave": "e3b0c44298fc1c149afbf4c8996fb924...",
    "mrsigner": "7d865e959b2466918c9863afca942d0f...",
    "timestamp": 1709654400,
    "quote_b64": "base64_encoded_quote"
  },
  "error": null
}
```

---

### 5. 验证 Quote

验证提供的 SGX Quote 的有效性。

**请求**

```http
POST /api/v1/attestation/verify
Content-Type: application/json

{
  "quote_b64": "base64_encoded_quote",
  "nonce": "optional_nonce_hex"
}
```

**响应**

```json
{
  "success": true,
  "valid": true,
  "mrenclave": "e3b0c44298fc1c149afbf4c8996fb924...",
  "mrsigner": "7d865e959b2466918c9863afca942d0f...",
  "timestamp": 1709654400,
  "error": null
}
```

**字段说明**

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | boolean | API 调用是否成功 |
| `valid` | boolean | Quote 验证是否通过 |
| `mrenclave` | string | Quote 中的 MRENCLAVE 值（hex 编码） |
| `mrsigner` | string | Quote 中的 MRSIGNER 值（hex 编码） |
| `timestamp` | number | 验证时间戳（Unix 秒） |
| `error` | string | 验证失败原因（如果失败） |

**请求参数**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `quote_b64` | string | 是 | SGX Quote（base64 编码） |
| `nonce` | string | 否 | 可选的挑战 nonce（hex 编码），用于验证 Quote 与挑战的绑定 |

**错误码**

| HTTP 状态码 | 错误场景 |
|-------------|----------|
| 400 | 无效的 base64 编码或 Quote 格式 |
| 500 | 内部错误 |
| 503 | 服务暂时不可用 |

**安全特性**

- **Quote 格式验证**: 验证 Quote 符合 SGX DCAP 规范
- **签名验证**: 验证 Quote 的 ECDSA 签名
- **测量值验证**: 验证 MRENCLAVE/MRSIGNER 是否在白名单中
- **时间戳验证**: 验证 Quote 未过期（默认最大 1 小时）
- **挑战绑定验证**: 如果提供 nonce，验证 Quote 与 nonce 的绑定

---

### 6. 获取认证报告

获取详细的认证报告，包含 Enclave 的完整信息和证书链。

**请求**

```http
GET /api/v1/attestation/report
```

**响应**

```json
{
  "success": true,
  "data": {
    "version": "3",
    "mrenclave": "e3b0c44298fc1c149afbf4c8996fb924...",
    "mrsigner": "7d865e959b2466918c9863afca942d0f...",
    "security_version": 1,
    "product_id": 1234,
    "attributes": "0x0000000000000000",
    "timestamp": 1709654400,
    "quote_b64": "base64_encoded_quote",
    "certificate_info": {
      "subject": "CN=Intel SGX Attestation Key",
      "issuer": "CN=Intel SGX Root CA",
      "not_before": "2024-01-01T00:00:00Z",
      "not_after": "2025-01-01T00:00:00Z",
      "fingerprint": "SHA256:abcdef123456..."
    }
  },
  "error": null
}
```

**字段说明**

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | boolean | API 调用是否成功 |
| `data.version` | string | Quote 版本号 |
| `data.mrenclave` | string | Enclave 测量值（MRENCLAVE） |
| `data.mrsigner` | string | 签名者测量值（MRSIGNER） |
| `data.security_version` | number | 安全版本号 |
| `data.product_id` | number | 产品 ID |
| `data.attributes` | string | Enclave 属性（hex 编码） |
| `data.timestamp` | number | Quote 生成时间戳（Unix 秒） |
| `data.quote_b64` | string | SGX Quote（base64 编码） |
| `data.certificate_info.subject` | string | 证书主题 |
| `data.certificate_info.issuer` | string | 证书颁发者 |
| `data.certificate_info.not_before` | string | 证书有效期起始 |
| `data.certificate_info.not_after` | string | 证书有效期结束 |
| `data.certificate_info.fingerprint` | string | 证书指纹（SHA256） |

**错误码**

| HTTP 状态码 | 错误场景 |
|-------------|----------|
| 500 | 内部错误 |
| 503 | 服务暂时不可用（Quote 不可用） |

**使用场景**

- 客户端需要完整的 Enclave 信息进行审计
- 验证证书链的有效性
- 获取 Quote 用于离线验证

---

### 7. 刷新 Quote

生成新的 SGX Quote 并更新缓存。

**请求**

```http
POST /api/v1/attestation/refresh
Content-Type: application/json

{
  "challenge": "optional_challenge_data"
}
```

**响应**

```json
{
  "success": true,
  "new_quote_b64": "base64_encoded_new_quote",
  "timestamp": 1709654400,
  "error": null
}
```

**字段说明**

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | boolean | API 调用是否成功 |
| `new_quote_b64` | string | 新的 SGX Quote（base64 编码） |
| `timestamp` | number | Quote 生成时间戳（Unix 秒） |
| `error` | string | 错误信息（如果失败） |

**请求参数**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `challenge` | string | 否 | 可选的挑战数据，用于绑定新的 Quote |

**错误码**

| HTTP 状态码 | 错误场景 |
|-------------|----------|
| 500 | 内部错误（Enclave 未运行等） |
| 503 | 服务暂时不可用 |

**使用场景**

- Quote 即将过期，需要刷新
- Enclave 重启后重新生成 Quote
- 响应新的挑战请求

**注意事项**

- 刷新操作会替换缓存中的旧 Quote
- 建议设置合理的 Quote 刷新策略（如到期前 5 分钟）
- 频繁刷新可能影响性能

---

### 8. 健康检查

检查认证服务健康状态。

**请求**

```http
GET /api/v1/attestation/health
```

**响应**

```json
{
  "status": "healthy",
  "enclave_state": "running",
  "dcap_version": "1.0.0",
  "quote_valid": true
}
```

```

## API 端点列表

| 端点 | 方法 | 描述 | 认证要求 |
|------|------|------|----------|
| `/api/v1/attestation/challenge` | POST | 创建认证挑战 | 公开 |
| `/api/v1/attestation/verify-response` | POST | 验证挑战响应 | 公开 |
| `/api/v1/attestation/status` | GET | 获取认证状态 | 公开 |
| `/api/v1/attestation/quote` | GET | 获取当前 Quote | 公开 |
| `/api/v1/attestation/verify` | POST | 验证 Quote | 公开 |
| `/api/v1/attestation/report` | GET | 获取认证报告 | 公开 |
| `/api/v1/attestation/refresh` | POST | 刷新 Quote | 公开 |
| `/api/v1/attestation/health` | GET | 健康检查 | 公开 |

## 使用示例

### 完整认证流程

```rust
use reqwest;

async fn perform_attestation() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:3000/api/v1/attestation";

    // 步骤 1: 创建挑战
    let challenge_resp = client
        .post(format!("{}/challenge", base_url))
        .json(&serde_json::json!({}))
        .send()
        .await?;

    let challenge_data = challenge_resp.json::<serde_json::Value>().await?;
    let challenge_id = challenge_data["challenge_id"].as_str().unwrap();
    let nonce = challenge_data["nonce"].as_str().unwrap();
    let quote = challenge_data["quote_b64"].as_str().unwrap();

    println!("收到挑战: {}", challenge_id);
    println!("MRENCLAVE: {}", challenge_data["mrenclave"].as_str().unwrap());

    // 步骤 2: 验证挑战响应（通常在客户端完成签名后）
    let verify_resp = client
        .post(format!("{}/verify-response", base_url))
        .json(&serde_json::json!({
            "challenge_id": challenge_id,
            "quote_b64": quote
        }))
        .send()
        .await?;

    let verify_data = verify_resp.json::<serde_json::Value>().await?;

    if verify_data["verified"].as_bool().unwrap() {
        println!("✅ 认证成功！");
        println!("MRENCLAVE: {}", verify_data["mrenclave"].as_str().unwrap());
    } else {
        println!("❌ 认证失败: {}", verify_data["error"].as_str().unwrap());
    }

    // 步骤 3: 检查认证状态
    let status_resp = client
        .get(format!("{}/status", base_url))
        .send()
        .await?;

    let status_data = status_resp.json::<serde_json::Value>().await?;
    println!("认证状态: {}", status_data["status"].as_str().unwrap());

    Ok(())
}
```

### cURL 示例

```bash
# 1. 创建挑战
curl -X POST http://localhost:3000/api/v1/attestation/challenge \
  -H "Content-Type: application/json" \
  -d '{}'

# 2. 验证响应
curl -X POST http://localhost:3000/api/v1/attestation/verify-response \
  -H "Content-Type: application/json" \
  -d '{
    "challenge_id": "chal_abc123",
    "quote_b64": "..."
  }'

# 3. 获取状态
curl http://localhost:3000/api/v1/attestation/status
```

## 安全考虑

### 挑战-响应协议

1. **随机挑战**: 每次挑战使用 256 位随机 nonce
2. **一次性使用**: 挑战验证后自动失效，防止重放攻击
3. **时间限制**: 挑战默认5分钟过期
4. **Quote 绑定**: Quote 中的 REPORT_DATA 绑定挑战和 Enclave 身份

### Quote 验证

1. **测量值验证**: 验证 MRENCLAVE/MRSIGNER 在白名单中
2. **签名验证**: 验证 Quote 的 ECDSA 签名
3. **时间戳验证**: 验证 Quote 未过期
4. **挑战绑定验证**: 验证 Quote 包含正确的挑战哈希

### 生产环境建议

1. 启用 TLS 加密所有 API 通信
2. 配置 API 密钥或 Token 认证
3. 限制 Quote 最大有效期（建议15-60分钟）
4. 实施 IP 白名单或速率限制
5. 记录所有认证事件到审计日志

## 错误处理

所有 API 返回统一的错误格式：

```json
{
  "success": false,
  "error": "详细错误信息",
  "data": null
}
```

常见错误：

| 错误 | 说明 | 处理方式 |
|------|------|----------|
| `Challenge not found` | 挑战不存在或已过期 | 重新创建挑战 |
| `Challenge already used` | 挑战已被使用 | 重新创建挑战 |
| `Measurement mismatch` | MRENCLAVE/MRSIGNER 不在白名单 | 检查 Enclave 配置 |
| `Quote expired` | Quote 已过期 | 刷新 Quote |
| `Enclave not running` | Enclave 未启动 | 检查服务状态 |

## 相关文档

- [CredBridge 架构设计](../_bmad-output/planning-artifacts/architecture.md)
- [TEE 安全架构](../src/tee/README.md)
- [DCAP 远程认证](../src/tee/dcap.md)
