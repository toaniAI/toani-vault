# Failure Classification

## F1: Tester auto-fill rejected the live session token

- Classification: `product`
- Observation: `Developer Center > Tester > Auto-fill` produced `检测到会话 Token 格式异常，请重新登录后重试`.
- Root cause:
  - frontend persists `response.data.session.session_token` from `/auth/session` into the in-memory backend session slot in [frontend/src/shared/api/services.ts](/Users/yvan/AIWorkspace/credbridge/frontend/src/shared/api/services.ts:80)
  - backend `/auth/session` returns a web session token created by `generate_secure_token()` in [src/auth/service.rs](/Users/yvan/AIWorkspace/credbridge/src/auth/service.rs:2390), which is an internal opaque token rather than a guaranteed `v4.local.*` PASETO
  - `DeveloperCenter.apiTester.resolveSessionAccessToken()` only accepts tokens starting with `v4.local.` in [frontend/src/features/developer/pages/DeveloperCenter.apiTester.ts](/Users/yvan/AIWorkspace/credbridge/frontend/src/features/developer/pages/DeveloperCenter.apiTester.ts:292)
  - backend auth middleware explicitly supports `Session Token` in either `PASETO v4.local` or `内部 Token` form in [src/api/middleware.rs](/Users/yvan/AIWorkspace/credbridge/src/api/middleware.rs:13)
- Why it is not `env`: the mismatch is present in local code and explains the observed runtime behavior directly.
- Impact: the tester rejects a token type that the backend itself considers valid for authenticated API access, so the planned “use current login session token” path is structurally broken.

## F2: Session operations list endpoint unavailable on remote target

- Classification: `product`
- Observation: `GET /api/v1/sandbox/sessions/600af5bf-267b-452f-a0f1-e7f0e1a474c2/operations` returned `404`.
- Root cause:
  - route constant exists in [src/api/routes.rs](/Users/yvan/AIWorkspace/credbridge/src/api/routes.rs:82)
  - frontend client and hooks expect the endpoint in [frontend/src/shared/api/services.ts](/Users/yvan/AIWorkspace/credbridge/frontend/src/shared/api/services.ts:363) and [frontend/src/shared/api/hooks.ts](/Users/yvan/AIWorkspace/credbridge/frontend/src/shared/api/hooks.ts:406)
  - actual sandbox router in [src/api/sandbox.rs](/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs:2179) does not register `/sandbox/sessions/:id/operations`
- Why it is not `env`: the 404 aligns with the current backend router implementation.
- Impact: blocked runtime proof for persisted `input_params` redaction.

## F3: Planned screenshots not captured

- Classification: `test_harness`
- Observation: local `screencapture` failed with `could not create image from display`.
- Impact: `03-login-success.png` and `04-credential-created.png` were not produced, though the underlying UI steps did succeed.

## F4: Initial negative requests returned `422 missing field description`

- Classification: `test_harness`
- Observation: the remote execute endpoint requires `description` in the request body.
- Impact: once corrected, all negative-path contract checks returned the expected `400 invalid_request` behavior.
