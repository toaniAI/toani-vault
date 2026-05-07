# Dev TEE Sandbox OKX/Binance E2E 前端缺口

## 2026-05-07 状态更新

基于当前仓库中的 `frontend/src/features/credentials/pages/CredentialsPage.tsx`，原始创建弹窗缺口已部分关闭：

- 已实现：`provider` 选择器（`okx` / `binance` / `custom`）
- 已实现：`allowed_domains` 文本输入并在提交前序列化为数组
- 已实现：`provider=okx` 时的 `passphrase` 显示与必填校验
- 已实现：`provider=custom` 时的 `custom_functions` 编辑区与基础校验

仍未完成的部分：

- 凭证详情弹窗虽然会请求 `GET /api/v1/credentials/:id`，但当前 UI 仍只展示基础元数据，
  尚未展示 `provider`、`allowed_domains`、`custom_functions` 摘要

下文保留的是 2026-05-06 的原始缺口记录与验收上下文，用于追踪需求来源；若与当前代码状态冲突，
以上状态更新为准。

## 背景

本轮验收目标要求用户在 `/credentials` 页面直接创建可用于 OKX / Binance sandbox `http_request` 的 `api_key` 凭证，并在页面侧完整填写：

- `provider`
- `allowed_domains`
- `plaintext_data.passphrase`（OKX 必填）
- 可选 `custom_functions`

验收计划来源：`/Users/yvan/Downloads/PLAN-Verify2.md`

## 2026-05-06 运行时观察

2026-05-06 在 `https://dev-credbridge.bitkinetic.com/credentials` 打开“Create Credential”弹窗后，`API Key` 类型下当前可见字段只有：

- `Credential Name`
- `Credential Type`
- `API Key`
- `API Secret (optional)`
- `Expiration Time (Optional)`

证据：

- 页面截图：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/browser/probe-credentials-modal.png`
- 字段结构导出：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/logs/probe-credentials-modal.json`

当日弹窗未暴露以下必要输入：

- `provider` 选择（至少 `okx` / `binance` / `custom`）
- `allowed_domains` 文本域
- `passphrase`（当 `provider=okx` 时）
- `custom_functions` 编辑区域

## 问题定义

当日页面能力无法直接创建满足 OKX / Binance 私有 REST sandbox 验收要求的凭证，因此以下计划项不能通过 UI 路径完成：

- `CLAIM-CRED-001`
- `CLAIM-TOKEN-001` 中“限制到本次创建的 credential IDs”的前置 UI 建凭证路径
- `CLAIM-OKX-001`
- `CLAIM-BINANCE-001`
- `CLAIM-GUARD-001`
- `CLAIM-GUARD-002`

说明：

- 后端接口是否支持这些字段，需要通过 API 路径另行验证。
- 该文档仅描述前端缺口，不对后端能力做结论。

## 需求

`API Key` 凭证创建弹窗需要支持以下输入和交互：

1. 新增 `provider` 选择器。
2. 当 `provider=okx` 时显示并校验 `passphrase`。
3. 新增 `allowed_domains` 文本域，支持逗号或换行分隔。
4. 新增 `custom_functions` 配置区，支持添加多条函数定义。
5. 提交 `POST /api/v1/credentials` 时透传：
   - `provider`
   - `allowed_domains`
   - `custom_functions`
   - `plaintext_data.passphrase`（当适用）
6. 创建成功后，凭证详情页需要展示上述元数据，但不得泄露明文。

## 相关接口

### 1. 创建凭证

- `POST /api/v1/credentials`
- 认证：当前 web session bearer token
- 目的：创建可用于 sandbox `http_request` 的交易所凭证

请求体示例：

```json
{
  "service_id": "verify2-api-okx-1778039160337",
  "credential_type": "api_key",
  "provider": "okx",
  "allowed_domains": ["www.okx.com:443", "*.okx.com:443"],
  "custom_functions": [],
  "plaintext_data": {
    "api_key": "xxx",
    "secret_key": "xxx",
    "passphrase": "xxx"
  }
}
```

Binance 请求体示例：

```json
{
  "service_id": "verify2-api-binance-1778039160337",
  "credential_type": "api_key",
  "provider": "binance",
  "allowed_domains": ["api.binance.com:443"],
  "custom_functions": [],
  "plaintext_data": {
    "api_key": "xxx",
    "secret_key": "xxx"
  }
}
```

成功响应示例：

```json
{
  "credential_id": "019dfb64-7e69-7981-b08a-8cfba12b5fb4",
  "service_id": "verify2-api-okx-1778039160337",
  "credential_type": "api_key",
  "created_at": "1778039160",
  "expires_at": null,
  "provider": "okx",
  "allowed_domains": ["www.okx.com:443", "*.okx.com:443"],
  "custom_functions": []
}
```

前端要求：

- `credential_type=api_key` 时允许提交 `provider`
- `provider=okx` 时必须带 `plaintext_data.passphrase`
- `allowed_domains` 支持按换行或逗号输入，提交前序列化为字符串数组
- `custom_functions` 允许为空数组

### 2. 查询凭证详情

- `GET /api/v1/credentials/:id`
- 目的：创建成功后在详情页展示交易所相关元数据

前端展示要求：

- 展示 `provider`
- 展示 `allowed_domains`
- 展示 `custom_functions` 数量或内容摘要
- 不展示 `plaintext_data` 明文
- 不展示 `encrypted_payload` 原文

### 3. 签发受限 access token

- `POST /api/v1/auth/access-token`
- 目的：Developer / Tester 页面从当前 web session 签发仅绑定当前 credential IDs 的 token

请求体示例：

```json
{
  "scopes": ["credential:read"],
  "ttl_seconds": 900,
  "credential_ids": [
    "019dfb64-7e69-7981-b08a-8cfba12b5fb4",
    "019dfb64-7ea8-7411-ac38-2a735e0b44a2"
  ]
}
```

响应示例：

```json
{
  "data": {
    "access_token": "v4.local....",
    "token_id": "019dfb64-7ef7-7912-a5d8-fd711ed80240",
    "token_type": "Bearer",
    "subject_type": "user",
    "issued_from": "access_token",
    "expires_at": 1778040060,
    "expires_in": 900,
    "granted_scopes": [
      "credential:read",
      "credential:decrypt",
      "sandbox:write",
      "sandbox:read",
      "sandbox:execute"
    ],
    "credential_ids": [
      "019dfb64-7e69-7981-b08a-8cfba12b5fb4",
      "019dfb64-7ea8-7411-ac38-2a735e0b44a2"
    ],
    "revoked_at": null
  },
  "success": true
}
```

前端要求：

- Developer 页面签发 token 时，必须能从凭证列表中挑选 credential IDs
- 至少支持把本次新建的 OKX / Binance 凭证一起带入签发请求

### 4. 创建 sandbox session

- `POST /api/v1/sandbox/sessions`
- 目的：后续用受限 token 验证交易所 `http_request`

请求体示例：

```json
{
  "credential_id": "019dfb64-7e69-7981-b08a-8cfba12b5fb4",
  "original_intent": "Verify OKX private balance through sandbox http_request"
}
```

响应示例：

```json
{
  "data": {
    "session_id": "dfbef135-2fc3-410c-bc64-d2bb6cbb850d",
    "sandbox_id": "2e63f61c-84b7-4d05-9b13-509966130f69",
    "runtime_context_id": "2e63f61c-84b7-4d05-9b13-509966130f69",
    "status": "ready",
    "created_at": "2026-05-06T03:46:00.603607+00:00",
    "expires_at": "2026-05-06T04:16:00.603607+00:00"
  },
  "success": true
}
```

### 5. 执行 sandbox `http_request`

- `POST /api/v1/sandbox/sessions/:id/execute`
- 目的：验证交易所私有 REST 调用

OKX 请求体示例：

```json
{
  "operation_type": "http_request",
  "description": "GET OKX private balance",
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

Binance 请求体示例：

```json
{
  "operation_type": "http_request",
  "description": "GET Binance private account",
  "parameters": {
    "method": "GET",
    "url": "https://api.binance.com/api/v3/account",
    "query": {
      "timestamp": "${functions.binance_timestamp()}",
      "recvWindow": "5000",
      "signature": "${functions.binance_sign()}"
    },
    "headers": {
      "X-MBX-APIKEY": "${credential.api_key}"
    }
  }
}
```

### 6. 查询 sandbox operation 详情

- `GET /api/v1/sandbox/operations/:operation_id`
- 目的：前端或测试页展示最终执行结果

前端要求：

- 能展示 `status`
- 能展示 `execution_time_ms`
- 能展示返回体中的远端状态码和摘要
- 不能把敏感字段明文回显到页面

## 前端改动建议

- `/credentials` 页面优先补齐交易所 API Key 凭证创建能力
- `/developer` 页面补齐“从新建凭证直接签发 token”的联动
- 若后续需要在页面中跑 sandbox 验证，可直接复用上述 `sandbox/sessions` 与 `execute` 接口

## 验收标准

- 用户可在 `/credentials` 页面创建 `provider=okx` 的 API Key 凭证，并填写 `api_key`、`secret_key`、`passphrase`、`allowed_domains`。
- 用户可在 `/credentials` 页面创建 `provider=binance` 的 API Key 凭证，并填写 `api_key`、`secret_key`、`allowed_domains`。
- 创建请求网络包中可观察到 `provider`、`allowed_domains`、`custom_functions`，敏感字段仅存在于 `plaintext_data`。
- 创建后凭证详情页展示 `provider` 与 `allowed_domains`，不展示原始 secret/passphrase 明文。
