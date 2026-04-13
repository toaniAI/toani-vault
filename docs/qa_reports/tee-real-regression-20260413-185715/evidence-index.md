# Evidence Index

## Baseline

- `baseline/health.headers`
- `baseline/health.body`
- `baseline/ready.headers`
- `baseline/ready.body`
- `baseline/attestation-health.headers`
- `baseline/attestation-health.body`

## Web / Playwright

- `playwright/login.png`
- `playwright/session.har`
- `playwright/ui-observations.json`
- `playwright/credentials.png`
- `playwright/tokens.png`
- `playwright/profile.png`
- `playwright/developer-center.png`
- `playwright/ui-mutations.json`
- `playwright/credentials-after-ui-create.png`
- `playwright/tokens-after-ui-create.png`
- `playwright/developer-tabs.json`
- 调试辅助截图:
  - `playwright/login-after-click.png`
  - `playwright/post-otp-debug.png`
  - `playwright/new-credential-modal.png`

## API

- 认证:
  - `api/session_token.txt`
  - `api/auth-me.headers`
  - `api/auth-me.json`
- Credential:
  - `api/credentials-create.headers`
  - `api/credentials-create.json`
  - `api/credentials-list.headers`
  - `api/credentials-list.json`
  - `api/credential-detail.headers`
  - `api/credential-detail.json`
  - `api/credential-decrypt.headers`
  - `api/credential-decrypt.json`
- Token:
  - `api/tokens-list.headers`
  - `api/tokens-list.json`
  - `api/token-create.headers`
  - `api/token-create.json`
  - `api/token-get.headers`
  - `api/token-get.json`
  - `api/token-create-empty.headers`
  - `api/token-create-empty.json`
  - `api/token-create-illegal-scope.headers`
  - `api/token-create-illegal-scope.json`
  - `api/access-token-create.headers`
  - `api/access-token-create.json`
- Sandbox:
  - `api/sandbox-stats-session.headers`
  - `api/sandbox-stats-session.json`
  - `api/sandbox-create-allowed.headers`
  - `api/sandbox-create-allowed.json`
  - `api/sandbox-create-denied.headers`
  - `api/sandbox-create-denied.json`
  - `api/sandbox-create-session-token.headers`
  - `api/sandbox-create-session-token.json`
  - `api/sandbox-get-session.headers`
  - `api/sandbox-get-session.json`
  - `api/sandbox-list.headers`
  - `api/sandbox-list.json`
  - `api/sandbox-execute.headers`
  - `api/sandbox-execute.json`
  - `api/sandbox-operation.headers`
  - `api/sandbox-operation.json`

## CLI

- `cli/help.txt`
- `cli/version.json`
- `cli/unknown-group.txt`
- `cli/sandbox-stats-session.json`
- `cli/sandbox-list-session.json`
- `cli/sandbox-create-session-token.json`
- `cli/sandbox-stats-session-exit.txt`
- `cli/sandbox-stats-dashboard-token-exit.txt`
- `cli/sandbox-create-dashboard-token-exit.txt`
- `cli/direct-dashboard-token-stats.http`

说明:

- `session_token` 不是 CLI 可接受的 PASETO 形态，因此 CLI 用它会在本地直接失败。
- Dashboard 手工 token 的 CLI 失败证据以退出码和对应的直连 HTTP `401 invalid_token` 为主。

## PostgreSQL / Redis

- `db/psql_probe.txt`
- `db/runtime_columns.txt`
- `db/runtime_reconciliation.txt`
- `db/test_data_inventory.txt`
- `redis/redis_probe.txt`
- `redis/redis_keys.txt`

## 关键交叉证据

- 手工签发 token 创建成功:
  - `playwright/ui-mutations.json`
  - `api/token-create.json`
  - `db/test_data_inventory.txt`
- 手工 token 无法被 sandbox / access-token 使用:
  - `api/access-token-create.json`
  - `api/sandbox-create-allowed.json`
  - `cli/direct-dashboard-token-stats.http`
- sandbox 执行链失败:
  - `api/sandbox-execute.json`
  - `api/sandbox-operation.json`
  - `db/runtime_reconciliation.txt`
- Developer Center 残留旧文案/示例:
  - `playwright/developer-tabs.json`
