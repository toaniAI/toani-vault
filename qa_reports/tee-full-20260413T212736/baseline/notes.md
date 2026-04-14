# Case01 Environment Baseline and TEE Health

- Run: `tee-full-20260413T212736`
- Case: `case01`
- Target: `https://dev-credbridge.bitkinetic.com/`
- Evidence dir: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/baseline`
- Scope: homepage, `/health`, `/ready`, `/health/detail`, `/api/v1/attestation/health`, `/api/v1/attestation/status`
- Constraint: only this case executed; no unrelated test cases run

## Repo contract used for verification

- `src/main.rs` defines backend routes as:
  - `/health` -> `health_check`
  - `/ready` -> `health_check_detail`
  - `/health/detail` -> `health_check_detail`
- `src/main.rs` implements `/health` as JSON with `status: "alive"`, `ready`, `version`, `timestamp`.
- `src/main.rs` implements `/ready` and `/health/detail` as JSON detailed readiness responses and returns `503` when not ready.
- `src/api/attestation.rs` implements `/api/v1/attestation/health` as JSON with `ready`, `requested_mode`, `effective_mode`, `root_key_source`, `detected_type`, `enclave_state`, `quote_valid`, `quote_expires_at`, `error`, and returns `503` when not ready.
- Documentation aligns with the source:
  - `docs/03-API 参考/REST-API.md`
  - `docs/03-API 参考/ATTESTATION-API.md`

## First failure preserved

- First unexpected response observed at `2026-04-13T13:30:10Z` on `GET /health`.
- Expected per repo contract: JSON body with `status: "alive"` and readiness metadata.
- Observed: `HTTP 200`, `content-type: application/octet-stream`, body `healthy`.
- Raw evidence:
  - `health.headers.txt`
  - `health.body`
  - `health.curl-meta.txt`

## Probe results

| Check | Expected | Observed | Verdict |
| --- | --- | --- | --- |
| Homepage `/` | Reachable homepage; case only requires availability baseline | `HTTP 200`, `text/html`, title `Toani Vault` | Pass |
| `/health` | Backend JSON liveness response | `HTTP 200`, `application/octet-stream`, body `healthy` | Fail |
| `/ready` | Backend JSON readiness response | `HTTP 200`, `text/html`, same HTML payload as homepage | Fail |
| `/health/detail` | Backend JSON readiness detail | `HTTP 200`, `application/octet-stream`, body `healthy` | Fail |
| `/api/v1/attestation/health` | JSON TEE/attestation health response | `HTTP 200`, JSON `status=ready`, `ready=true`, `requested_mode=hardware`, `effective_mode=hardware`, `root_key_source=sgx_sealing_key`, `detected_type=Intel SGX`, `enclave_state=running`, `quote_valid=true`, `error=null` | Pass |
| `/api/v1/attestation/status` | JSON attestation runtime status | `HTTP 200`, JSON `success=true`, `status=authenticated`, `health_status=ready`, `hardware_available=true`, `remote_attestation_available=true`, `quote_valid=true` | Pass |

## Raw response summaries

- Homepage
  - Timestamp: `2026-04-13T13:30:10Z`
  - Response: `HTTP 200`
  - Content-Type: `text/html`
  - Summary: static SPA shell with `<title>Toani Vault</title>`
- `/health`
  - Timestamp: `2026-04-13T13:30:10Z`
  - Response: `HTTP 200`
  - Content-Type: `application/octet-stream`
  - Summary: plain text `healthy`
- `/ready`
  - Timestamp: `2026-04-13T13:30:10Z`
  - Response: `HTTP 200`
  - Content-Type: `text/html`
  - Summary: homepage HTML instead of readiness JSON
- `/health/detail`
  - Timestamp: `2026-04-13T13:30:10Z`
  - Response: `HTTP 200`
  - Content-Type: `application/octet-stream`
  - Summary: plain text `healthy`
- `/api/v1/attestation/health`
  - Timestamp: `2026-04-13T13:30:10Z`
  - Response: `HTTP 200`
  - Content-Type: `application/json`
  - Summary: hardware TEE path appears active; quote is valid
  - `quote_expires_at=1776088900` -> `2026-04-13T14:01:40Z`
- `/api/v1/attestation/status`
  - Timestamp: `2026-04-13T13:30:10Z`
  - Response: `HTTP 200`
  - Content-Type: `application/json`
  - Summary: attestation service authenticated and healthy; remote attestation available
  - `last_quote_generated_at=1776085300` -> `2026-04-13T13:01:40Z`

## Blocker classification

- Overall result: `fail`
- Failure class: `env`
- Why `env`: the public deployment at the tested host is not exposing the backend health/readiness contract described in the repo. The API attestation endpoints are healthy, but the top-level health/readiness probes appear routed to non-backend handlers or fallback content.
- Secondary observation: there is also a `contract` mismatch relative to repository docs because `/health` and `/health/detail` do not return the documented JSON shapes.

## Discovered route details

- Backend source declares:
  - `/health`
  - `/ready`
  - `/health/detail`
  - `/api/v1/attestation/health`
  - `/api/v1/attestation/status`
- Target environment behavior suggests:
  - top-level `/health` and `/health/detail` are reachable but not serving backend JSON
  - `/ready` is currently serving the SPA shell
  - `/api/v1/attestation/health` and `/api/v1/attestation/status` are serving backend JSON correctly

## Key evidence files

- `commands.txt`
- `notes.md`
- `result.json`
- `homepage.headers.txt`
- `homepage.body`
- `health.headers.txt`
- `health.body`
- `ready.headers.txt`
- `ready.body`
- `health-detail.headers.txt`
- `health-detail.body`
- `attestation-health.headers.txt`
- `attestation-health.body`
- `attestation-status.headers.txt`
- `attestation-status.body`
