# Gate Decision

Current status: `failed`

## Basis

- Multiple P0 claims failed across baseline, web navigation, token/runtime consistency, sandbox execution, CLI behavior, and data reconciliation.
- The target environment shows contradictory state across surfaces:
  - token issuance succeeds in UI and API
  - PostgreSQL does not show matching `scope_tokens` rows for issued token IDs from this run
  - Redis DB 3 does not show expected token-state keys for those IDs
- Sandbox lifecycle is reachable, but actual execution fails with an `nsjail` policy compilation error.

## Release decision

- Do not treat this environment as having passed full-system acceptance.
- Safe conclusions are limited to:
  - attestation health is wired to hardware mode
  - credential API contract is mostly aligned with the intended metadata-only model
  - tokens UI enforces the constrained `credential:read + credential whitelist` flow
- Blockers remain on runtime health probes, Audit navigation, stale Developer Center guidance, token persistence consistency, CLI contract drift, and sandbox execute.
