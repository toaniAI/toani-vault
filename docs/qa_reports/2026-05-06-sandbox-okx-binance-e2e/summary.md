# 2026-05-06 Dev TEE Sandbox OKX/Binance E2E Summary

## 结论

本轮验收未通过最终 gate。

原因不是单点故障，而是两类事实同时成立：

1. 后端与 sandbox runtime 主链路已跑通：
   - 真实 OTP 登录成功。
   - API 创建 OKX / Binance 凭证成功。
   - 受限 access token 签发成功，并限制到本次 credential IDs。
   - OKX / Binance 私有 REST 均通过 sandbox `http_request` 成功返回真实账户数据。
   - 篡改 Binance signature 后，远端正确返回 `-1022 Signature for this request is not valid.`。
2. 仍存在阻塞最终通过的产品缺口：
   - `/credentials` 页面当前无法填写 `provider`、`allowed_domains`、`passphrase`、`custom_functions`，因此无法按计划完成 UI 建凭证路径。
   - 域名白名单负例虽然功能上被系统拦截，但没有生成可复核的 `sandbox_operations` 记录，导致 runtime 持久化 claim 不成立。

## 关键结果

- 登录与会话：通过。
- UI 建凭证：失败，前端能力缺口。
- API 建凭证：通过。
- 受限 token 签发：基本通过，返回 scope 为 `credential:read` 派生出的 `credential:decrypt + sandbox:*`，但不包含显式 `credential:write`。
- OKX 正例：通过。
- Binance 正例：通过。
- 非白名单域名拦截：功能通过，持久化证据不足。
- 错签名拦截：通过。
- 审计：通过，API 可见 `credential_create` / `credential_access` / `token_issue`，`auth_audit_logs` 可见 `session_created`。

## 主要证据

- UI 缺口文档：`/Users/yvan/AIWorkspace/credbridge/docs/requirements/frontend/2026-05-06-sandbox-okx-binance-e2e-ui-gap.md`
- API 汇总：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/logs/api-only-summary.json`
- 登录截图：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/browser/login-before.png`
- 凭证页截图：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/browser/credentials-landing.png`
- 凭证弹窗探针：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/logs/probe-credentials-modal.json`
- DB 凭证行：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/db/credentials_rows.json`
- DB sandbox operation：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/db/sandbox_operations_rows.json`
- 审计 API：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/api/audit-logs.json`
- 审计 DB：`/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/2026-05-06-sandbox-okx-binance-e2e/db/audit_logs_rows.json`

## 非阻塞备注

- 本机到 `www.okx.com:443` 的直接连接失败，但服务端 sandbox 对 OKX 的真实调用成功，因此本地网络异常未阻塞产品验证。
- 本机缺少 `redis-cli`，Redis 只作为补充证据，本轮未作为通过门槛。
