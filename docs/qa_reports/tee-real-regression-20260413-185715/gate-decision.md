# Gate Decision

结论: **FAIL**

日期: 2026-04-13
环境: `https://dev-credbridge.bitkinetic.com/`

## 判定依据

本轮验收目标不是“页面能打开”，而是验证改版后真实环境的三条核心链路：

1. Dashboard 手工发 token
2. 手工 token 驱动 access token / sandbox / CLI
3. TEE sandbox 真实执行

其中第 2、3 条都未成立，因此不能放行。

## 关键阻塞项

### Blocker 1: 手工签发 token 不可用

- 证据:
  - `api/access-token-create.json`
  - `api/sandbox-create-allowed.json`
  - `cli/direct-dashboard-token-stats.http`
- 现象:
  - token 创建本身成功
  - 但后续调用 `/auth/access-token`、`/sandbox/sessions`、`/sandbox/stats` 时统一返回 `401 invalid_token`
  - 错误细节稳定为 `会话未找到: 00000000-0000-0000-0000-000000000000`
- 影响:
  - 改版的核心目标“用户从 Dashboard 手工拿 token 后用于 sandbox / CLI”没有在真实环境成立

### Blocker 2: sandbox execute 不可执行

- 证据:
  - `api/sandbox-execute.json`
  - `api/sandbox-operation.json`
- 现象:
  - session 能创建
  - execute 返回 `success=false`
  - 错误为 `nsjail: unrecognized option '--disable_clone_newmnt'`
- 影响:
  - 即使 token 问题被绕开，TEE 浏览器自动化主链仍然跑不通

### Blocker 3: Developer Center 仍暴露旧改版前内容

- 证据:
  - `playwright/developer-tabs.json`
- 现象:
  - `API Docs` 仍出现 `Automation Token` / `Profile > Automation Access`
  - `SDK Examples` 仍保留旧 credential create/decrypt 示例
- 影响:
  - 文档和产品约束不一致，容易误导真实用户与 CLI/SDK 集成方

## 非阻塞但应记录

- `/ready` 当前返回前端 HTML，而不是 readiness JSON。
- `Overview` 文案仍含 “credential encryption, decryption, token management” 旧能力描述，虽不是直接交互入口，但与改版收敛方向仍存在认知漂移。
- CLI 对后端 `401` 失败未输出清晰错误文本，只体现为 `exit 1`。

## 放行建议

在再次验收前，至少需要满足：

1. 手工 token 能成功调用 `/auth/access-token` 与 `/sandbox/sessions`
2. sandbox execute 至少完成一次基础 `navigate` 成功
3. Developer Center 清理 `Automation Token` 与旧 decrypt 示例
4. `/ready` 的实际暴露路径与预期保持一致，或更新运行时探针约定

## 当前建议

- 不放行当前“改版后真实环境功能已验收通过”的结论。
- 将本次结果作为一轮 **失败但证据充分** 的真实环境回归报告归档。
