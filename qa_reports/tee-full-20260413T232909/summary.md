# Summary

Run status: `completed`

## Outcome

- Passed: `6`
- Conditional pass: `0`
- Failed: `6`

## High-signal findings

- Dashboard login, credential create/delete, token issuance, auth API, and credential API all passed in the live TEE environment.
- PostgreSQL reconciliation also passed once live token persistence was matched to `credbridge_vault.api_tokens`.
- Baseline health/readiness contract failed: `/ready` served the SPA shell and `/health` plus `/health/detail` returned plain `healthy` instead of the documented JSON semantics.
- Developer Center and CLI contract drift are material: the page is stale, and the shipped CLI exposes only `sandbox` while README-documented `config/auth/credentials/tokens/audit` groups are absent.
- Sandbox lifecycle partially works, but `execute` fails at runtime. API body, PostgreSQL, and Loki evidence all point to an nsjail policy problem in the deployed `credbridge` service.
- Redis DB 3 still had none of the expected token runtime keys; the only observed key was unrelated to credbridge token/session runtime state.
