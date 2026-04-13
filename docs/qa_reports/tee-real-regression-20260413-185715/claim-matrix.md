# Claim Matrix

| Claim ID | Claim | Layer | Evidence | Result | Failure Class |
| --- | --- | --- | --- | --- | --- |
| CLAIM-BASE-001 | 站点首页可访问并返回当前前端壳 | Runtime | `baseline/health.*`, `/` 首页探针, `playwright/login.png` | PASS | |
| CLAIM-BASE-002 | 环境可证明为 TEE 硬件运行链的一部分 | Runtime | `baseline/attestation-health.body` (`effective_mode=hardware`, `quote_valid=true`) | PASS | |
| CLAIM-BASE-003 | `/ready` 可作为 backend readiness 探针 | Runtime | `baseline/ready.body` 返回 HTML 而非 readiness JSON | FAIL | env |
| CLAIM-AUTH-001 | 真实 Privy OTP 登录可完成并建立 backend session | E2E / Runtime | `playwright/ui-observations.json` 中 `authSessionStatus=200`、截图、HAR | PASS | |
| CLAIM-UI-001 | Credentials 页面不再展示直接 decrypt/view 明文入口 | E2E | `playwright/credentials.png`, `playwright/ui-observations.json` | PASS | |
| CLAIM-UI-002 | Profile 页面不再展示 automation token 区块 | E2E | `playwright/profile.png`, `playwright/ui-observations.json` | PASS | |
| CLAIM-UI-003 | Tokens 页面只允许 `credential:read + credential_ids` 方式发 token | E2E | `playwright/tokens.png`, `playwright/ui-observations.json`, `playwright/ui-mutations.json` | PASS | |
| CLAIM-UI-004 | 用户可通过 Credentials 页真实创建凭证 | E2E | `playwright/ui-mutations.json`, `playwright/credentials-after-ui-create.png` | PASS | |
| CLAIM-UI-005 | 用户可通过 Tokens 页真实生成 token | E2E | `playwright/ui-mutations.json`, `playwright/tokens-after-ui-create.png` | PASS | |
| CLAIM-UI-006 | Developer Center 已移除 automation/decrypt 等旧示例与旧文案 | E2E | `playwright/developer-tabs.json` 显示 `API Docs.hasAutomation=true`, `SDK Examples.hasDecrypt=true` | FAIL | product |
| CLAIM-API-001 | 创建 credential 后列表/详情仅返回元数据，不含明文 | Contract / Integration | `api/credentials-create.json`, `api/credentials-list.json`, `api/credential-detail.json` | PASS | |
| CLAIM-API-002 | 直接 decrypt API 已被禁用 | Contract / Integration | `api/credential-decrypt.headers`, `api/credential-decrypt.json` | PASS | |
| CLAIM-API-003 | `/tokens` 只接受 `credential:read` 且 `credential_ids` 必填 | Contract / Integration | `api/token-create.json`, `api/token-create-empty.json`, `api/token-create-illegal-scope.json` | PASS | |
| CLAIM-API-004 | Dashboard 手工 token 可用于 `/auth/access-token` | Integration / Runtime | `api/access-token-create.json` 返回 `401 invalid_token`, 零 UUID session | FAIL | product |
| CLAIM-API-005 | Dashboard 手工 token 可用于 `/sandbox/sessions` | Integration / Runtime | `api/sandbox-create-allowed.json` 返回 `401 invalid_token`, 零 UUID session | FAIL | product |
| CLAIM-API-006 | session_token 可创建和查询 sandbox session | Integration / Runtime | `api/sandbox-create-session-token.json`, `api/sandbox-get-session.json`, `api/sandbox-list.json` | PASS | |
| CLAIM-API-007 | sandbox 操作执行链在 TEE 环境可正常运行 | Integration / Runtime | `api/sandbox-execute.json`, `api/sandbox-operation.json` 显示 `nsjail --disable_clone_newmnt` 错误 | FAIL | env |
| CLAIM-CLI-001 | 当前 CLI 主入口只暴露 `sandbox` | Contract | `cli/help.txt`, `cli/unknown-group.txt` | PASS | |
| CLAIM-CLI-002 | CLI 可用 Dashboard 手工 token 完成 sandbox 主路径 | Integration | `cli/sandbox-stats-dashboard-token-exit.txt`, `cli/sandbox-create-dashboard-token-exit.txt`, `cli/direct-dashboard-token-stats.http` | FAIL | product |
| CLAIM-DATA-001 | 新增 credential / token / sandbox session / operation 可在 PostgreSQL 对账 | Runtime | `db/runtime_reconciliation.txt`, `db/test_data_inventory.txt` | PASS | |
| CLAIM-REDIS-001 | Redis DB 3 可读且可提供运行时侧缓存证据 | Runtime | `redis/redis_probe.txt`, `redis/redis_keys.txt` | PASS | |

## Failure Notes

- `CLAIM-API-004` 与 `CLAIM-API-005` 共享同一根因迹象：手工签发 token 被中间件解析成 `session_id = 00000000-0000-0000-0000-000000000000`。
- `CLAIM-API-007` 更像环境/部署兼容问题，而不是页面或 token 约束问题；session 已成功创建，失败发生在执行期。
- `CLAIM-CLI-002` 与 API 层 `401 invalid_token` 证据一致，CLI 只是将后端问题暴露出来，没有形成单独的 CLI-only 缺陷证据。
