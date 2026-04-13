# Token-First Real-Chain Verification

Date: 2026-04-10

Environment

- Local service: `http://127.0.0.1:8080`
- Auth mode: `PRIVY_MOCK_ENABLED=true`
- TEE mode: `simulation`
- Dependencies: local PostgreSQL, Redis, immudb, Vault via `docker compose`
- Local DB fix applied before verification:
  `migrations/20260410110000_extend_api_tokens_for_automation_tokens.sql`

Evidence files

- `00_health.json`
- `01_auth_session.json`
- `02_automation_token_create.json`
- `03_automation_auth_me.json`
- `04_automation_memberships.json`
- `05_automation_token_list.json`
- `06_access_token_create.json`
- `07_access_auth_me.json`
- `08_access_memberships.json`
- `09_access_token_list.json`
- `10_access_create_denied.json`
- `11_automation_create_denied.json`
- `12_logout_with_automation_denied.json`
- `13_mfa_sync_with_automation_denied.json`
- `14_verify_automation_token.json`
- `15_verify_cli_access_token.json`
- `16_access_token_from_session.json`
- `17_verify_session_access_token.json`
- `18_invalid_access_metadata.json`
- `19_valid_access_metadata.json`
- `20_cli_config_init.json`
- `21_cli_auth_status.json`
- `22_cli_auth_me_automation.json`
- `23_cli_auth_memberships_automation.json`
- `24_cli_token_list_automation.json`
- `25_cli_access_create.json`
- `26_cli_auth_me_access.json`
- `27_cli_token_list_access.json`
- `29_cli_logout_local_only.json`
- `30_ts_sdk_chain.json`
- `32_access_token_from_automation_retry.json`
- `33_verify_retry_access_token.json`

Verified claims

- Frontend-style login path still works: `01_auth_session.json` shows a real session created through `/auth/session`.
- Automation token issuance works from a user session: `02_automation_token_create.json` shows `issued_from = "automation"` and the expected subset scopes.
- Automation token is valid as an external bearer token:
  `03_automation_auth_me.json`, `04_automation_memberships.json`, `05_automation_token_list.json`, and `14_verify_automation_token.json`.
- Automation token can issue an access token:
  `32_access_token_from_automation_retry.json` and `33_verify_retry_access_token.json`.
- Access token works on the same bearer call path:
  `07_access_auth_me.json`, `08_access_memberships.json`, and `09_access_token_list.json`.
- Access token capability boundaries are enforced by scope, not by a different usage model:
  `10_access_create_denied.json` shows missing `tokens:write`;
  `11_automation_create_denied.json` shows missing automation-token-management scope.
- CLI works with the unified bearer token model:
  `21_cli_auth_status.json`, `22_cli_auth_me_automation.json`, `23_cli_auth_memberships_automation.json`, `24_cli_token_list_automation.json`, `25_cli_access_create.json`, `26_cli_auth_me_access.json`, `27_cli_token_list_access.json`.
- CLI logout is local-only and no longer depends on a reachable backend:
  `29_cli_logout_local_only.json`.
- TypeScript SDK works on the token-first chain:
  `30_ts_sdk_chain.json` shows automation-token calls, access-token calls, and a 403-denied scope reduction attempt.

Observed gaps

- `/auth/logout` now correctly rejects automation token usage with HTTP 403, but the localized message body is still misleading:
  `12_logout_with_automation_denied.json` returns `forbidden` with message `服务器内部错误`.
- `/auth/mfa-status/sync` still accepts an automation token in the live chain:
  `13_mfa_sync_with_automation_denied.json`.
  This means the intended web-only isolation for MFA sync is still not enforced at runtime.
- Rust SDK real-chain smoke is not yet green.
  The example path still hits a 403 and did not produce a final positive evidence artifact.

Notes

- Building the service container from Docker failed due upstream Debian package mirror `500/502` responses during `apt-get`.
  Verification therefore used a locally compiled `cargo run --bin vault-service` process instead of the containerized app image.
