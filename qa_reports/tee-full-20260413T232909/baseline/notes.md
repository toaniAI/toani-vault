# Case01 Baseline Notes

- Claim: `C01`
- Target: `https://dev-credbridge.bitkinetic.com/`
- Observation timestamp: `2026-04-13T23:32:24+0800`
- Overall outcome: homepage reachable; attestation/TEE endpoints reachable and internally coherent; base health/readiness endpoints are not coherent with repo docs or current source routing contract.

## Homepage

- `GET /` returned `200 OK`
- `Content-Type: text/html`
- HTML title: `Toani Vault`
- Body summary: static SPA shell only (`<div id="root"></div>`)

## Health Endpoints

- `GET /health` returned `200 OK`, body `healthy`, `Content-Type: application/octet-stream`
- `GET /ready` returned `200 OK`, but served the same HTML shell as `/`
- `GET /health/detail` returned `200 OK`, body `healthy`, `Content-Type: application/octet-stream`
- `GET /api/v1/` returned `200 OK` and advertises `/health` and `/ready` as liveness/readiness endpoints

## Attestation / TEE

- `GET /api/v1/attestation/health` returned `200 OK`
- `ready: true`
- `requested_mode: hardware`
- `effective_mode: hardware`
- `detected_type: Intel SGX`
- `enclave_state: running`
- `quote_valid: true`

- `GET /api/v1/attestation/status` returned `200 OK`
- `status: authenticated`
- `health_status: ready`
- `ready: true`
- `hardware_available: true`
- `remote_attestation_available: true`
- `enclave_state: running`

- `GET /api/v1/attestation/quote` returned `200 OK`
- `GET /api/v1/attestation/report` returned `200 OK`

## Semantics Assessment

- Attestation endpoints are mutually coherent: hardware mode, Intel SGX detected, enclave running, quote valid, and authenticated status all align.
- Base health endpoints are not coherent with docs or current source:
  - Repo docs expect JSON for `/health`, `/ready`, `/health/detail`.
  - Current source maps `/ready` and `/health/detail` to detailed JSON handlers.
  - Live environment returns HTML for `/ready` and a plain `healthy` string for `/health/detail`.
- This points more strongly to deployment or ingress routing behavior than to the current repository contract, because `/api/v1/` and attestation API routes are behaving as registered JSON APIs.

## Blocker Classification

- Primary blocker for `C01`: `env`
- Reason: deployed routing/serving behavior for base health endpoints does not match the documented and source-defined semantics.

## Runtime Log Need

- `runtime_log_needed: false`
- Reason: current evidence is sufficient to show the mismatch on HTTP surface; no server-side log was needed to establish the inconsistency.
