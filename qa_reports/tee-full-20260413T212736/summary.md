# Summary

Run status: `completed`

## Outcome

- Passed: `case04`, `case07`
- Conditional pass: `case06`
- Failed: `case01`, `case02`, `case03`, `case05`, `case08`, `case09`, `case10`, `case11`, `case12`

## High-signal findings

- Hardware TEE and attestation endpoints are healthy, but `/health`, `/ready`, and `/health/detail` do not match the documented runtime contract.
- Web login and most dashboard navigation work, but `Audit` navigation redirects away and Developer Center still exposes stale token-exchange guidance.
- Credentials API is aligned with the intended metadata-only model: create/list/get/delete work and direct decrypt is blocked.
- Tokens UI works under its constrained contract, but token API verification and downstream persistence consistency are not fully aligned.
- Sandbox session lifecycle works, but `execute` fails at runtime because `nsjail` policy compilation breaks in the target environment.
- CLI behavior drifts from its README and rejects the captured web session token as non-PASETO input.
- PostgreSQL lacks `scope_tokens` rows for token IDs created during this run, and Redis DB 3 lacks the expected token metadata and active-set keys.
