# Test Execution Report

## Environment

- Repo: `/Users/yvan/AIWorkspace/credbridge`
- Remote target: `https://dev-credbridge.bitkinetic.com/`
- Test account: `test-7226@privy.io`
- Execution window: 2026-04-24 UTC
- Run ID: `20260424-071644`

## Execution summary

### Phase 1

- Homepage baseline passed: remote homepage returned `HTTP 200` and title `Toani Vault`.
- Static verification passed for backend wrapper parsing, alias support, TS SDK canonical `api_key` serialization, and CLI docs.

### Phase 2

- Remote login succeeded with the provided OTP.
- Protected navigation was reachable.
- A new API key credential was created:
  - service id: `e2e-http-request-credential-20260424-071644`
  - credential id: `019dbe5b-4da9-7563-8af9-2d39473ce7c6`

### Phase 3

- `POST /api/v1/sandbox/sessions` succeeded and created session `600af5bf-267b-452f-a0f1-e7f0e1a474c2`.
- Positive matrix execution succeeded through remote runtime.
- Upstream echo proved:
  - `Authorization == "Bearer cb_e2e_key_20260424-071644"`
  - `X-Alias-Key == "cb_e2e_key_20260424-071644"`
  - `X-Alias-Camel == "cb_e2e_key_20260424-071644"`
  - `X-With-Suffix == "Bearer cb_e2e_key_20260424-071644#suffix"`
- `GET /api/v1/sandbox/operations/:operation_id` returned only execution summary fields and did not expose persisted `input_params`.
- `GET /api/v1/sandbox/sessions/:id/operations` returned `404` on the remote environment, so runtime redaction proof for persisted parameters could not be closed.

### Phase 4

All rejection cases passed once the harness matched the remote request schema and included `description`.

- Unknown wrapper key: `400 invalid_request`
- Non-string prefix: `400 invalid_request`
- Non-string suffix: `400 invalid_request`
- `execute_script` credential reference: `400 invalid_request`
- `bootstrap_page` credential reference: `400 invalid_request`

## Harness deviations from the written plan

- The exact browser path `Developer Center > Tester > Auto-fill current session token` failed because the page reported the current session token format as invalid.
- To continue runtime verification, a fresh PASETO token was minted from the `Token 管理` page using the newly created credential, then used for direct API execution.
- This workaround preserved the main runtime target but means the plan's narrower statement about using the current login session token was not satisfied as written.
