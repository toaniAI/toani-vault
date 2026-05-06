# Claim Matrix

| Claim | Status | Classification | Notes | Evidence |
| --- | --- | --- | --- | --- |
| `CLAIM-AUTH-001` | `passed` | `product` | 真实 OTP 登录成功，`auth_me` 返回 owner 级 scopes；`auth_audit_logs` 可见 `session_created`。 | `api/auth-me.json`, `db/auth_audit_logs_rows.json`, `browser/login-before.png` |
| `CLAIM-CRED-001` | `failed` | `product` | `/credentials` 的 API Key 弹窗缺少 `provider`、`allowed_domains`、`passphrase`、`custom_functions`，无法按计划经 UI 创建 OKX/Binance 凭证。 | `browser/probe-credentials-modal.png`, `logs/probe-credentials-modal.json`, `docs/requirements/frontend/2026-05-06-sandbox-okx-binance-e2e-ui-gap.md` |
| `CLAIM-CRED-002` | `passed` | `product` | API 创建后的 `credentials` 行正确持久化 `provider`、`allowed_domains`、`custom_functions=[]`；敏感值仅在 `encrypted_payload` 中。 | `api/api-credential-okx-create.json`, `api/api-credential-binance-create.json`, `db/credentials_rows.json` |
| `CLAIM-TOKEN-001` | `conditional_pass` | `product` | 当前 web session 成功签发受限 token，并限制到两条 credential IDs；返回 scopes 为 `credential:read` 派生出的 `credential:decrypt + sandbox:*`。未返回显式 `credential:write`。 | `api/auth-access-token.json`, `db/api_tokens_rows.json` |
| `CLAIM-OKX-001` | `passed` | `product` | OKX sandbox session 创建成功，`http_request` 返回 `code=0` 和账户余额结构。 | `api/sandbox-okx-positive-create-session.json`, `api/sandbox-okx-positive-execute.json`, `db/sandbox_operations_rows.json` |
| `CLAIM-BINANCE-001` | `passed` | `product` | Binance sandbox session 创建成功，`http_request` 返回真实账户信息。 | `api/sandbox-binance-positive-create-session.json`, `api/sandbox-binance-positive-execute.json`, `db/sandbox_operations_rows.json` |
| `CLAIM-GUARD-001` | `conditional_pass` | `product` | 非白名单域名在系统侧被拦截，消息明确为 `http_request blocked by allowed_domains policy`；但没有生成 `sandbox_operations` 行。 | `api/sandbox-domain-reject-execute.json`, `logs/api-only-summary.json`, `db/sandbox_sessions_rows.json` |
| `CLAIM-GUARD-002` | `passed` | `product` | 错签名请求通过 sandbox 发出后，被 Binance 以 `-1022` 拒绝。 | `api/sandbox-signature-reject-execute.json`, `db/sandbox_operations_rows.json` |
| `CLAIM-RUNTIME-001` | `failed` | `product` | 正例与错签名负例的 `input_params` 已脱敏并持久化；但域名白名单负例未生成 operation 记录，因此“sandbox operation 会落库”不成立。 | `db/sandbox_operations_rows.json`, `api/sandbox-domain-reject-execute.json` |
| `CLAIM-AUDIT-001` | `passed` | `product` | `auth_audit_logs` 可复核 `session_created`；审计 API / DB 可复核 `token_issue` 与 credential 相关事件。 | `db/auth_audit_logs_rows.json`, `api/audit-logs.json`, `db/audit_logs_rows.json` |
