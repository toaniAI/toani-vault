# TEE 实环境改版回归测试摘要

日期: 2026-04-13
环境: `https://dev-credbridge.bitkinetic.com/`
测试账号: `test-7226@privy.io`
执行约束: 共享环境仅新增，不做删除/撤销清理

## 总结

本轮“改版功能全量 + 核心烟测”已完成 Web、API、CLI、PostgreSQL、Redis 五个面的真实环境验收取证。结果为 **FAIL**。

主要原因不是单点失败，而是三条核心能力链存在真实运行问题：

1. Dashboard 手工签发 token 虽然能创建，但无法被后端正常鉴权使用。
2. TEE sandbox session 可以创建，但执行操作时 `nsjail` 参数与运行环境不兼容，导致操作立即失败。
3. Developer Center 仍暴露旧改版前文案与示例，和当前产品约束不一致。

## 已通过

- 站点入口可达，首页返回 `HTTP 200`，标题为 `Toani Vault`。
- `/health` 返回 `healthy`。
- `/api/v1/attestation/health` 返回 `effective_mode=hardware`、`detected_type=Intel SGX`、`quote_valid=true`。
- PostgreSQL 可连接，目标 schema 为 `credbridge_vault`，核心表可读。
- Redis 可连接，目标 DB `3` 可读，当前探测到 `CHAIN_ERROR_WHITE_LIST` 键。
- Privy 邮箱 OTP 登录真实成功，`POST /api/v1/auth/session` 返回 `200`，浏览器成功进入 `/credentials`。
- `CredentialsPage` 不再展示“查看/解密明文”入口，列表仅展示元数据。
- `ProfilePage` 不再展示 automation token 区块。
- `TokensPage` 不再展示 verify tab，且 UI 只能以 `credential:read + credential_ids` 创建 token。
- 通过 API 创建 credential 成功，`GET /credentials` 和 `GET /credentials/:id` 只返回元数据。
- `POST /credentials/:id/decrypt` 返回 `403 forbidden`，消息为 “Direct credential decryption is disabled; use sandbox execution instead”。
- `POST /tokens` 正向成功，空 `credential_ids` 和非法 scope 都会被拒绝。
- 通过 UI 成功新增一条 credential，并成功从 Tokens 页生成 token。
- 通过 `session_token` 可以创建、查询、列出 sandbox session。
- CLI 主入口只暴露 `sandbox`，`tokens` 组对外不可用。

## 失败与阻塞

- `POST /api/v1/auth/access-token` 使用 Dashboard 手工签发 token 调用时返回 `401 invalid_token`，错误原因为 `会话未找到: 00000000-0000-0000-0000-000000000000`。
- `POST /api/v1/sandbox/sessions` 使用 Dashboard 手工签发 token 调用时同样返回上述 `401 invalid_token`。
- 直接 `curl` 使用该 token 调用 `/api/v1/sandbox/stats` 也返回相同 `401`，说明不是 CLI 特有问题。
- CLI 使用 Dashboard 手工 token 运行 `sandbox stats` / `sandbox create-session` 稳定 `exit 1`，无法完成计划中的 token 驱动 sandbox 闭环。
- `POST /api/v1/sandbox/sessions/:id/execute` 虽返回 `200`，但 `success=false`，错误为 `nsjail: unrecognized option '--disable_clone_newmnt'`，说明 TEE sandbox 执行面不可用。
- `GET /ready` 当前返回前端 HTML 壳而不是 backend readiness JSON，和源码中的后端 readiness 路由不一致。
- Developer Center 的 `API Docs` 标签仍出现 `Automation Token` / `Profile > Automation Access` 旧文案。
- Developer Center 的 `SDK Examples` 标签仍出现旧 credential create/decrypt 示例，未完全收敛到“token 仅用于 sandbox”语义。

## 新增测试数据

- Credential: `019d8684-529b-7753-8e4f-34d9a99296b0` / `tee-real-20260413-185715`
- Credential: `019d868b-272a-7333-92b7-b21d5a3241b5` / `ui-tee-real-20260413-185715`
- Token: `019d8684-d98f-7d63-9304-4029fd7d716f`
- Token: `019d868b-39e4-70f1-8f2a-ba39dd9ed732`
- Sandbox session: `0017c2dd-3430-4b4e-bba9-58d8eb301525`
- Sandbox operation: `34fa8fc3-2119-45fc-86ea-f2239df54e81`

## 结论

当前环境可证明：

- 改版后的前端主页面结构大体已落地。
- token 创建约束和 decrypt 禁用约束在 UI/API 层大体成立。

当前环境也明确证明：

- “Dashboard 手工 token -> access token / sandbox / CLI” 这条目标主链 **未打通**。
- “TEE sandbox session -> execute operation” 这条目标主链 **未打通**。
- Developer Center 内容仍与改版要求 **不一致**。

因此本轮门禁结论为 **FAIL**，不建议宣称改版后的真实环境功能已完成全量验收。
