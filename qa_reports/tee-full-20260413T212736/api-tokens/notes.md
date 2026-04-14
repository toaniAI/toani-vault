# case08 Token API chain notes

- Run ID: `tee-full-20260413T212736`
- Case: `case08`
- Prefix: `tee-full-case08-20260413T212736`
- Environment: `https://dev-credbridge.bitkinetic.com/`
- Session source: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-auth/raw/fresh_session_token.txt`
- Session rebuilt via /auth/session: `false`

## Claims

1. Session-authenticated owner can create a credential-bound token through `POST /api/v1/tokens`.
2. Token metadata is observable through `GET /api/v1/tokens` and `GET /api/v1/tokens/:id`.
3. Issued token can read credential metadata for its bound credential and should not expand beyond its allowed `credential_ids`.
4. Missing `credential_ids` and unsupported scopes are rejected.
5. Revoked token stops working.

## Execution summary

- `GET /api/v1/auth/me`: HTTP ``
- `POST /api/v1/credentials`: credential_id ``
- `POST /api/v1/tokens`: token_id ``
- Verify get credential with issued token: HTTP ``
- Verify list credentials with issued token: HTTP ``, total=``, foreign_ids=`[]`
- Negative missing credential_ids: HTTP ``
- Negative invalid scope: HTTP ``
- Revoke token cleanup: `not_attempted`
- Delete credential cleanup: `not_attempted`
- Post-revoke credential read: HTTP ``

## First unexpected failure

- Step: `auth_me`
- HTTP status: `none`
- Body preview: `none`

## Evidence

- Commands: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-tokens/commands.txt`
- Raw responses: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-tokens/raw/`
- Result: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-tokens/result.json`
