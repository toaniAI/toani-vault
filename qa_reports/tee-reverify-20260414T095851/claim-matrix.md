# Claim Matrix

| Issue | Claim | Result | Classification | Evidence |
| --- | --- | --- | --- | --- |
| 1 | Sandbox execute works in live TEE | failed | env | `raw/sandbox-execute.response.json`, `raw/sandbox-operation-get.response.json` |
| 2 | Shipped CLI surface matches README contract | failed | product | `raw/cli-help.txt` equivalent from local build output, `cli/src/index.ts`, `cli/README.md` |
| 3 | `/health`, `/ready`, `/health/detail` expose stable machine-readable health semantics | failed | env | `raw/health.body`, `raw/ready.body`, `raw/health_detail.body`, matching headers |
| 4 | Token verify contract is usable as documented | failed | product | `raw/token-verify.headers.txt`, `raw/token-verify.response` |
| 5 | Developer Center content reflects current onboarding and base URL contract | failed | product | `raw/developer-api-docs.txt`, `raw/developer-sdk-examples.txt` |
| 6 | Token persistence model is clarified or aligned | failed | product | PostgreSQL rows in `api_tokens` and absence in `scope_tokens` for token `019d89bc-a72f-7ad2-b572-33f4ed97e07a` |
| 7 | Redis runtime token/session state exists or the environment reflects the documented model | failed | env | `raw/redis-probe.txt` |
| 8 | Sandbox session ID mapping is made clearer in runtime evidence | partial | product | DB now clearly shows `sandbox_sessions.id` and `tee_context_id`, but the API still returns only the runtime-facing `sandbox_id` alias without an explicit mapping explanation |

Supporting regressions checked:

| Supporting Check | Result | Evidence |
| --- | --- | --- |
| Browser login still succeeds | passed | `raw/login-final.png`, authenticated `/credentials` and `/developer` page captures |
| Token create/list/get/revoke still works | passed | `raw/token-create.response.json`, `raw/tokens-list.response.json`, `raw/token-get.response.json`, `raw/token-revoke.response.json` |
| Sandbox create/get/terminate still works | passed | `raw/sandbox-create.response.json`, `raw/sandbox-get.response.json`, `raw/sandbox-terminate.response.json` |
