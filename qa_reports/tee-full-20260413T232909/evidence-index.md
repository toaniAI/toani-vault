# Evidence Index

| Case | Directory | Scope | Status |
| --- | --- | --- | --- |
| case01 | `baseline/` | Environment baseline and TEE health | failed |
| case02 | `web-login/` | Dashboard login and navigation | passed |
| case03 | `web-credentials/` | Credentials UI lifecycle | passed |
| case04 | `web-tokens/` | Tokens UI lifecycle | passed |
| case05 | `web-developer-center/` | Developer Center contract review | failed |
| case06 | `api-auth/` | Auth API chain | passed |
| case07 | `api-credentials/` | Credential API chain | passed |
| case08 | `api-tokens/` | Token API chain | failed |
| case09 | `api-sandbox/` | Sandbox API chain | failed |
| case10 | `cli/` | CLI chain | failed |
| case11 | `db/` | PostgreSQL reconciliation | passed |
| case12 | `redis/` | Redis reconciliation | failed |

## Expected Per-case Files

- `notes.md`
- `commands.txt`
- `result.json`
- optional raw artifacts: screenshots, HAR, request/response bodies, stdout/stderr, logs
