## Case 06 - API Auth

- Claim: `C06`
- Base URL: `https://dev-credbridge.bitkinetic.com`
- Auth material source:
  - Privy access token extracted from `web-login/storage-state.json`
  - Fresh backend web session token minted via `POST /api/v1/auth/session`
  - Fresh API access token minted via `POST /api/v1/auth/access-token`

### Outcome

- `C06` passed for runtime behavior.
- Core auth paths were reachable and coherent:
  - `POST /api/v1/auth/session` returned a valid session and membership context.
  - `GET /api/v1/auth/me` succeeded with a web session token.
  - `GET /api/v1/auth/memberships` succeeded with both the web session token and the issued access token.
  - `POST /api/v1/auth/logout` revoked the created web session token.
- Negative cases behaved sensibly:
  - Missing token on `/auth/me` -> `401 missing_token`
  - Invalid token on `/auth/me` -> `401 invalid_token`
  - Access token on `/auth/me` -> `403 forbidden`
  - Logged-out web session token on `/auth/me` -> `401 invalid_token`

### Contract Findings

- `POST /api/v1/auth/access-token` does not match the documented request example:
  - Omitting `credential_ids` returns `400 invalid_request` with message `At least one credential_id is required`.
  - Requesting `tokens:read` scope returns `400 invalid_request` with message `Only credential:read scope is supported for dashboard-issued tokens`.
- Live behavior indicates the currently accepted dashboard-issued access token shape is narrower than the docs claim:
  - `credential_ids` is effectively required.
  - `credential:read` is the accepted scope in this flow.
- `GET /api/v1/auth/memberships` accepts the issued access token, while `GET /api/v1/auth/me` explicitly requires a web session token.

### Created / Used IDs

- Reused credential id for token minting: `019d8714-3949-7f32-8804-9c593f7c7a3e`
- Created session id: `019d8782-40de-7db1-9e11-d133749ba8a0`
- Created access token id: `019d8783-4eb7-7d51-94c1-6a0c86ce49fe`

### Runtime Log Need

- `runtime_log_needed = false`
- No runtime-side failure required `logmcp` confirmation for this case.
