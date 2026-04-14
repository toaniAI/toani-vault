# Execution Context

- Run ID: `tee-full-20260413T212736`
- Started at: `2026-04-13T21:27:36+08:00`
- Repository: `/Users/yvan/AIWorkspace/credbridge`
- Target environment: `https://dev-credbridge.bitkinetic.com/`
- Execution mode: real-environment acceptance, evidence-driven
- TEE expectation: hardware path should be observable from attestation/health evidence
- Web actor: `test-7226@privy.io`
- OTP seed for current run: `450192`
- Required command transport: `rtk`
- Web harness: Playwright CLI / local Playwright scripts
- Log analysis path: `logmcp` against `zkme-dev/credbridge`

## Naming Rule

Use this prefix for created test objects:

`tee-full-<case-id>-20260413T212736`

Examples:

- `tee-full-case07-20260413T212736`
- `tee-full-case09-20260413T212736`

## Evidence Rules

- Every case writes only inside its assigned directory.
- Preserve first-failure evidence before retries.
- Record commands, timestamps, request/response summaries, IDs, and cleanup state.
- If a case creates test data, append object IDs to `../residual-test-data.md` with cleanup status.
