# Failure Classification

## Product

### 1. `/credentials` 页面不满足交易所 API Key 凭证创建需求

- 现象：API Key 弹窗仅暴露 `Credential Name`、`Credential Type`、`API Key`、`API Secret`、`Expiration Time`。
- 影响：无法经 UI 填写 `provider`、`allowed_domains`、`passphrase`、`custom_functions`。
- 结论：`CLAIM-CRED-001` 失败。
- 证据：`browser/probe-credentials-modal.png`、`logs/probe-credentials-modal.json`

### 2. 白名单拒绝路径没有持久化 `sandbox_operations`

- 现象：`https://evil-okx.com/...` 被正确拦截，但 API 响应没有 `operation_id`，数据库也没有对应 operation 行。
- 影响：功能层面满足拒绝，但 runtime 可复核性不足。
- 结论：`CLAIM-RUNTIME-001` 失败，`CLAIM-GUARD-001` 仅能给 `conditional_pass`。
- 证据：`api/sandbox-domain-reject-execute.json`、`db/sandbox_operations_rows.json`

### 3. Dashboard-issued token 不返回显式 `credential:write`

- 现象：`/api/v1/auth/access-token` 返回 `credential:read` 派生出的 `credential:decrypt + sandbox:*`，不含 `credential:write`。
- 影响：不阻塞本轮 sandbox 验收，但与计划文字不完全一致。
- 结论：`CLAIM-TOKEN-001` 为 `conditional_pass`。
- 证据：`api/auth-access-token.json`、`db/api_tokens_rows.json`

## Env

### 1. 本机直连 OKX 失败，但不构成产品失败

- 现象：本机 `curl https://www.okx.com/...` 失败。
- 对照：服务端 sandbox 对 OKX 私有 REST 调用成功。
- 结论：本机网络异常不阻塞产品链路，归为环境噪音。
- 证据：`logs/connectivity.txt`、`api/sandbox-okx-positive-execute.json`

### 2. Redis CLI 缺失

- 现象：本机没有 `redis-cli`。
- 影响：未采集 Redis 补充证据。
- 结论：计划已声明 Redis 非通过门槛，不单独判失败。
- 证据：`logs/baseline.txt`

## Data

无单独 data blocker。

测试账号具备有效 membership，且交易所凭证可成功调用真实私有 REST。
