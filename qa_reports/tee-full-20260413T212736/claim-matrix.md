# Claim Matrix

| Claim ID | Case | Result | Failure Class | Evidence |
| --- | --- | --- | --- | --- |
| C01 | baseline | failed | env | `baseline/result.json`, `baseline/*.body` |
| C02 | web-login | failed | product | `web-login/result.json`, `web-login/session.har` |
| C03 | web-credentials | failed | env | `web-credentials/result.json`, `web-credentials/99-failure.png` |
| C04 | web-tokens | passed |  | `web-tokens/result.json`, `web-tokens/request-response-summaries.json` |
| C05 | web-developer-center | failed | product | `web-developer-center/result.json`, `api-docs.txt`, `sdk-examples.txt` |
| C06 | api-auth | conditional_pass | product | `api-auth/result.json`, `api-auth/raw/*` |
| C07 | api-credentials | passed |  | `api-credentials/result.json`, `api-credentials/raw/*` |
| C08 | api-tokens | failed | product | `api-tokens/result.json`, `api-tokens/raw/*` |
| C09 | api-sandbox | failed | env | `api-sandbox/result.json`, `api-sandbox/raw/*` |
| C10 | cli | failed | product | `cli/result.json`, `cli/raw/*` |
| C11 | db | failed | product | `db/result.json`, `db/runtime_reconciliation.txt` |
| C12 | redis | failed | env | `redis/result.json`, `redis/redis_probe.json` |
