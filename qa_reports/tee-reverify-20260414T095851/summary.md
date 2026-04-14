# TEE Remediation Re-Verification Summary

Run ID: `tee-reverify-20260414T095851`

Target environment:

- Web/API: `https://dev-credbridge.bitkinetic.com/`
- PostgreSQL: `10.11.25.9:15432 / credbridge / credbridge_vault`
- Redis: `dev-r-n9ehhd6lin7zi5x0jwpd.redis.rds.aliyuncs.com:6379 / db 3`

Execution timestamp:

- Started on `2026-04-14 09:58:51 +08:00`
- Key verification requests executed around `2026-04-14 10:05 +08:00`

Overall conclusion:

- Fully fixed: `0`
- Partially improved: `1`
- Still failing / not fixed: `7`

High-signal conclusions:

- Browser login still works. The test account `test-7226@privy.io` successfully completed the Privy OTP flow and reached authenticated pages.
- Baseline health/readiness is still broken. `/ready` still serves the SPA shell, and `/health` plus `/health/detail` still return plain `healthy` with `application/octet-stream`.
- Token core lifecycle still works, but the declared verify contract is still not fixed. `POST /api/v1/tokens/verify` now returns `405 Allow: GET,HEAD`, not a usable verify API.
- Sandbox execute is still broken in the live TEE environment. The failure mode changed from the previous nsjail policy symptom to runtime namespace launch failure: `clone(...) failed: No space left on device`.
- Developer Center is still stale. It still tells users to obtain a bearer token first and call sandbox commands directly, and still uses `https://your-api.example.com` placeholders instead of the live base URL.
- The shipped CLI surface is still not aligned with the README or source command modules. The built `toani` help output still exposes only the `sandbox` group.
- Token persistence still lands in `credbridge_vault.api_tokens`; the newly minted token produced no row in `scope_tokens`.
- Redis db `3` still contains no credbridge token/session runtime keys. The only key remains `CHAIN_ERROR_WHITE_LIST`.

Notable delta versus the prior run:

- The sandbox execute failure signature changed. The old report pointed at an nsjail policy problem; this re-run shows a lower-level runtime failure during namespace clone with `No space left on device`. The claim remains failed, but the proximate cause has shifted.
