# Gate Decision

Current status: `failed`

## Basis

- P0 claims failed on baseline health semantics, token verify contract, sandbox execute runtime, CLI surface, and Redis token-state reconciliation.
- Runtime evidence is attached for the sandbox failure path via API response, PostgreSQL persistence, and Loki logs from `zkme-dev` / `credbridge`.
- Web happy-path evidence exists for login, credentials, and token issuance, but that is not sufficient for full-system acceptance.

## Runtime usability decision

- `fail` for full-system acceptance. The deployed environment is usable for core dashboard auth/credentials/token issuance, but it is not runtime-complete because sandbox execute is broken and Redis token-state behavior does not match the expected contract.

## Contract compliance decision

- `fail`. Developer Center guidance is stale, CLI README does not match the shipped binary, baseline health/readiness endpoints drift from documented semantics, and the token verify/persistence surfaces differ from the asserted contract.
