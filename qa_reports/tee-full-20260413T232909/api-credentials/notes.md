# Case07 API Credentials

Claim: `C07`

Outcome: `passed`

Summary:
- Reused prior auth evidence, but the session token from `api-auth` had already been revoked by that case's logout step, so a fresh backend session was minted from the preserved Privy access token.
- `POST /api/v1/credentials` succeeded against the live TEE environment and returned credential metadata only.
- `GET /api/v1/credentials?service_id=...` returned the created object and did not expose plaintext.
- `GET /api/v1/credentials/:id` returned metadata only and reflected `active` before delete.
- `DELETE /api/v1/credentials/:id` succeeded.
- `GET /api/v1/credentials/:id` after delete still returned `200` with `is_deleted=true` and `status=deleted`, so delete is currently soft-delete/tombstone, not hard disappearance.
- `POST /api/v1/credentials/:id/decrypt` is blocked in the live service even with a full-scope session token and also with a read-only access token. Live response: `403 forbidden` with `Direct credential decryption is disabled; use sandbox execution instead`.

Created IDs:
- credential_id: `019d878b-f659-7bc0-8aaa-a252f8a8a02f`
- fresh_session_id: `019d878b-f5f5-7b61-8925-58c82d141ae1`
- access_token_id: `019d878b-f77e-7220-a5de-65ace7aab707`

Contract findings:
- Live decrypt endpoint exists on the route surface but is intentionally disabled, which is stricter than older docs that still describe direct plaintext retrieval.
- Create response `created_at` came back as epoch-seconds string (`1776095589`) rather than the RFC3339 example in the REST docs.
- Delete behavior is soft-delete with retrievable tombstone (`200` after delete), not disappearance/`404`.
- `/api/v1/auth/access-token` returned the issued token under `data.access_token`; an earlier assumption based on prior raw artifacts expected a top-level `access_token`.

Raw evidence:
- `raw/auth-session.*`
- `raw/create-valid.*`
- `raw/list-valid.*`
- `raw/get-valid.*`
- `raw/decrypt-session-valid.*`
- `raw/decrypt-access-valid-fixed.*`
- `raw/delete-valid.*`
- `raw/get-after-delete-valid.*`
- Preserved precondition failure evidence from revoked prior session under `raw/create.*`, `raw/list.*`, `raw/get.*`, `raw/decrypt-session.*`, `raw/delete.*`
