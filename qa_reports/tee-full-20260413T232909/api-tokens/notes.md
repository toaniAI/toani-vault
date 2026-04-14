## Case08 API Tokens

- Claim: `C08`
- Base URL: `https://dev-credbridge.bitkinetic.com/api/v1`
- Fresh auth context was re-minted from the saved Privy token because the earlier shared backend session had already been revoked by `C06`.

### Positive coverage

- `POST /auth/session` succeeded and produced a fresh backend session.
- `POST /tokens` succeeded when called with:
  - `scopes=["credential:read"]`
  - `credential_ids=["019d8714-3949-7f32-8804-9c593f7c7a3e"]`
  - `expires_in=900`
- `GET /tokens` returned a top-level array and included the newly created token metadata.
- `GET /tokens/:token_id` returned the created token metadata.
- `GET /tokens/stats` succeeded.
- `POST /tokens/:token_id/revoke` succeeded.
- `GET /tokens/:token_id` after revoke still returned metadata, now with populated `revoked_at`.
- `GET /tokens` after revoke still listed the token with populated `revoked_at`.

### Negative coverage

- Invalid scope (`tokens:read`) returned `400` with `Only credential:read scope is supported for dashboard-issued tokens`.
- Missing `credential_ids` returned `400` with `At least one credential_id is required`.
- Inaccessible/random `credential_ids` returned `403` with `Requested credential_ids must be accessible to the current user`.
- Invalid bearer token on list/get/revoke returned `401 invalid_token`.

### Contract findings

- Live `/tokens` create contract requires `credential_ids`, which is stricter than the older REST doc snippet that only showed `scopes` plus `expires_in`.
- Live `/tokens` only accepts `credential:read` for dashboard-issued tokens.
- Documented `POST /api/v1/tokens/verify` did not behave as a public verify endpoint in live. The response was `401`, with `Allow: GET,HEAD`, which indicates the documented contract is no longer exposed as described.

### Verdict

- Core token lifecycle (`create`, `list`, `get`, `revoke`) is working in the live environment.
- The claim is marked `failed` overall because `verify` is part of the declared contract for this case and is not available in live as documented.
