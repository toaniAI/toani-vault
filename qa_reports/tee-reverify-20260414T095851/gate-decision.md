# Gate Decision

Decision: `failed`

Why:

1. P0 issue `Sandbox execute` is still failing in the real TEE environment.
2. P0 issue `baseline health/readiness contract` is still failing on the public surface.
3. P0 issue `CLI surface drift` is still failing in the built artifact.
4. P1 issue `token verify contract` is still failing.
5. P1 issue `Developer Center stale guidance` is still failing.
6. P1/P2 contract reconciliation issues for token persistence and Redis runtime state remain unresolved.

Decision notes:

- This was a real-environment re-verification, not a source-only review.
- The environment still cannot be considered fully remediated because the most important runtime execution path, health contract surface, and developer onboarding contract remain inconsistent.
- The presence of passing sub-flows such as browser login, token issuance, sandbox session creation, and DB persistence does not offset the failed P0 claims.

Concrete evidence highlights:

- `POST /api/v1/sandbox/sessions/{id}/execute` returned `success: true` at the envelope level but `data.success: false`, with runtime error `clone(...) failed: No space left on device`.
- `GET /ready` returned HTML instead of readiness JSON on `2026-04-14`.
- `GET /health` and `GET /health/detail` returned body `healthy` and `content-type: application/octet-stream` on `2026-04-14`.
- Built CLI help still shows only:
  - `sandbox      create-session/list-sessions/get-session/terminate/execute/get-operation/stats`
- `POST /api/v1/tokens/verify` returned `405` with `Allow: GET,HEAD`.
- Developer Center still instructs:
  - `toani --base-url https://your-api.example.com --token <BEARER_TOKEN> sandbox stats`
  - and still uses `https://your-api.example.com` placeholders in SDK examples.
