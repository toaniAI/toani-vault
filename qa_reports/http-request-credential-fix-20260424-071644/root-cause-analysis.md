# Root Cause Analysis

## Finding 1: `Developer Center > Tester > Auto-fill` rejects the current dashboard session by design

### Evidence chain

1. The browser flow logged in successfully and could access protected pages.
2. `Auto-fill` then reported `检测到会话 Token 格式异常，请重新登录后重试`.
3. Frontend login bootstrap persists `response.data.session.session_token` from `/auth/session` into the in-memory backend session slot in [frontend/src/shared/api/services.ts](/Users/yvan/AIWorkspace/credbridge/frontend/src/shared/api/services.ts:80).
4. Backend `/auth/session` creates `session_token` via `generate_secure_token()` in [src/auth/service.rs](/Users/yvan/AIWorkspace/credbridge/src/auth/service.rs:2390).
5. Backend auth middleware documents that a session token may be either `PASETO v4.local` or an `内部 Token` in [src/api/middleware.rs](/Users/yvan/AIWorkspace/credbridge/src/api/middleware.rs:13).
6. `DeveloperCenter.apiTester` rejects any auto-filled token that does not start with `v4.local.` in [frontend/src/features/developer/pages/DeveloperCenter.apiTester.ts](/Users/yvan/AIWorkspace/credbridge/frontend/src/features/developer/pages/DeveloperCenter.apiTester.ts:292).

### Conclusion

`Auto-fill` is validating against a narrower token format than the backend actually supports. The feature assumes "current session token" means "PASETO token", but the login bootstrap stores a web session token that may be opaque. The observed runtime error is therefore consistent with the current source code.

### Practical effect

- Browser login works.
- Regular authenticated API calls through the app work because backend middleware accepts session tokens.
- The tester's convenience button fails because its client-side validation rejects the same token class.

## Finding 2: `/sandbox/sessions/:id/operations` is a dead contract on the current backend

### Evidence chain

1. Route constant exists in [src/api/routes.rs](/Users/yvan/AIWorkspace/credbridge/src/api/routes.rs:82).
2. Frontend service method exists in [frontend/src/shared/api/services.ts](/Users/yvan/AIWorkspace/credbridge/frontend/src/shared/api/services.ts:363).
3. React Query hook exists in [frontend/src/shared/api/hooks.ts](/Users/yvan/AIWorkspace/credbridge/frontend/src/shared/api/hooks.ts:406).
4. Actual backend router in [src/api/sandbox.rs](/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs:2179) registers:
   - `/sandbox/sessions/:id`
   - `/sandbox/sessions/:id/execute`
   - `/sandbox/sessions/:id/pause`
   - `/sandbox/sessions/:id/resume`
   - `/sandbox/sessions/:id/export`
   - `/sandbox/sessions/:id/dom-export`
   but not `/sandbox/sessions/:id/operations`
5. Remote call returned `404`, matching the missing route registration.

### Conclusion

The repo contains a stale or incomplete contract: constants and frontend client code advertise an operations listing endpoint that the backend router does not expose. This is a product issue, not just a deployment drift hypothesis.

## Impact on the original plan

- `C2`, `C3`, and `C5` were still executable via direct API calls and passed.
- `C4` could not be closed because the only observed operation-detail endpoint does not return persisted `input_params`, and the expected list endpoint is absent.

## Most likely minimal fixes

1. Relax `DeveloperCenter.apiTester` token validation to accept both backend web session tokens and PASETO tokens, or remove client-side format validation and let the backend authenticate.
2. Either:
   - register `/sandbox/sessions/:id/operations`, or
   - remove the stale frontend/client contract and expose persisted redacted operation input through the supported detail endpoint instead.
