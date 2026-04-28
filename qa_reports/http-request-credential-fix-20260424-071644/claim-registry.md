# Claim Registry

| Claim | Status | Evidence | Notes |
| --- | --- | --- | --- |
| C1 | passed | static | Local backend, TS SDK, and CLI docs match the planned implementation surface. |
| C2 | passed | contract + e2e + runtime | Remote `http_request` resolved `{"$credential":"api_key","prefix":"Bearer "}` and sent the expected Authorization header upstream. |
| C3 | passed | contract + e2e + runtime | `api_key`, `key`, `apiKey`, and `suffix` all resolved to the same credential value in remote execution. |
| C4 | inconclusive | static only | Runtime redaction proof for persisted `input_params` could not be observed from the remote environment. |
| C5 | passed | contract + runtime | All five rejection cases returned the expected `400 invalid_request` semantics after correcting the request harness to include `description`. |

## Ancillary findings

- AF1: `Developer Center > Tester > Auto-fill` reported `检测到会话 Token 格式异常，请重新登录后重试` even though the dashboard session was live. This blocked the exact plan path that expected the current login session token to be auto-filled.
- AF2: `GET /api/v1/sandbox/sessions/:id/operations` returned `404` on the remote environment, even though the local codebase contains that route constant and frontend client method.

See [root-cause-analysis.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/http-request-credential-fix-20260424-071644/root-cause-analysis.md) for the product-level explanation of both blockers.
