# case06 Auth API chain notes

- Run ID: `tee-full-20260413T212736`
- Case: `case06`
- Environment: `https://dev-credbridge.bitkinetic.com/`
- Actor: `test-7226@privy.io`
- OTP seed: `450192`
- Execution mode: real-environment login via existing Playwright repo script, then direct API replay with `rtk`

## Scope executed

- `POST /api/v1/auth/session`
- `GET /api/v1/auth/me`
- `GET /api/v1/auth/memberships`
- `POST /api/v1/auth/access-token`
- manual bearer reuse of the issued current-equivalent token
- negative coverage:
  invalid token
  missing parameter
  permission/authorization boundary

## Status summary

- `01_auth_session`: `200`
  Evidence: `raw/01_auth_session.body.json`, `raw/01_auth_session.headers.txt`, `raw/01_auth_session.curl-meta.txt`
- `02_auth_me`: `200`
  Evidence: `raw/02_auth_me.body.json`
- `03_auth_memberships`: `200`
  Evidence: `raw/03_auth_memberships.body.json`
- `04_invalid_token_auth_me`: `401`
  Evidence: `raw/04_invalid_token_auth_me.body.json`
- `05_missing_param_auth_session`: `400`
  Evidence: `raw/05_missing_param_auth_session.body.json`
- `06_access_token_create`: `400`
  First meaningful failure preserved at:
  `raw/first-unexpected-failure.label.txt`
  `raw/first-unexpected-failure.body.json`
  `raw/first-unexpected-failure.headers.txt`
  `raw/first-unexpected-failure.curl-meta.txt`
- `07_access_token_create_current_equivalent`: `200`
  Runtime requires `credential_ids` and succeeded with `credential:read` + allowlisted credential.
- `08_current_equivalent_token_auth_me`: `403`
  Current-equivalent token is not accepted for `/api/v1/auth/me`; runtime returns `errors.auth.insufficient_permissions`.
- `09_current_equivalent_token_access_token_retry`: `403`
  Current-equivalent token cannot mint more tokens; runtime returns `Missing required scope: tokens:write`.

## Key findings

- Fresh real login worked. The Playwright run captured a fresh Privy access token and backend session creation in:
  `playwright/ui-observations.json`
  `playwright/session.har`
- Replayed `POST /api/v1/auth/session` with the fresh Privy token succeeded and minted a new backend session token.
- `GET /api/v1/auth/me` and `GET /api/v1/auth/memberships` both succeeded with that session token and returned the same active owner membership under tenant `019d8414-bf83-7641-a3fb-ad93d58715bd`.
- Invalid bearer token path is active and returns `401 invalid_token`.
- Missing `privy_access_token` in `/api/v1/auth/session` returns `400 invalid_request`.
- The first meaningful failure is a live contract drift on `/api/v1/auth/access-token`:
  the old generic request body `{"scopes":["tokens:read"],"ttl_seconds":300}` does not work in dev; runtime returns `400` with message `At least one credential_id is required`.
- The current live equivalent does work:
  `{"scopes":["credential:read"],"credential_ids":["019d868b-272a-7333-92b7-b21d5a3241b5"],"ttl_seconds":300}`
  returns `200` and issues a bearer token.
- That issued token is constrained:
  it cannot call `/api/v1/auth/me` successfully and cannot call `/api/v1/auth/access-token` again because it lacks `tokens:write`.

## Failure classification

- First meaningful failure: `product`
- Reason:
  live `/api/v1/auth/access-token` behavior differs from the documented generic token-subset contract and instead enforces the newer credential-whitelist flow.

## Evidence index

- Exact commands: `commands.txt`
- Raw request/response artifacts: `raw/`
- Login/HAR capture: `playwright/`
- Machine summary: `result.json`
