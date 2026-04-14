# case07 Credential API chain notes

- Run ID: `tee-full-20260413T212736`
- Case: `case07`
- Environment: `https://dev-credbridge.bitkinetic.com/`
- Object prefix: `tee-full-case07-20260413T212736`
- Auth carrier: fresh backend session token minted from a real Privy access token, then used as `Authorization: Bearer <session_token>`
- Actor user: `019d7b1d-42e7-7071-bdae-63b777db2b6c`
- Actor tenant: `019d8414-bf83-7641-a3fb-ad93d58715bd`
- Created credential ID: `019d8713-52b0-7ae2-8785-0e48119da368`

## Claims

1. `POST /api/v1/credentials` can create a credential in the real environment.
2. `GET /api/v1/credentials` remains metadata-only.
3. `GET /api/v1/credentials/:id` remains metadata-only.
4. `POST /api/v1/credentials/:id/decrypt` is rejected for direct use.
5. `DELETE /api/v1/credentials/:id` succeeds and cleanup is observable.

## Execution Summary

- `POST /api/v1/auth/session` returned `200`.
- `POST /api/v1/credentials` returned `201`.
- `GET /api/v1/credentials?service_id=tee-full-case07-20260413T212736&page=1&page_size=20` returned `200` and contained the created ID without `plaintext_data`.
- `GET /api/v1/credentials/019d8713-52b0-7ae2-8785-0e48119da368` returned `200` and contained no `plaintext_data`.
- `POST /api/v1/credentials/019d8713-52b0-7ae2-8785-0e48119da368/decrypt` returned `403` with `error=forbidden` and message `Direct credential decryption is disabled; use sandbox execution instead`.
- `DELETE /api/v1/credentials/019d8713-52b0-7ae2-8785-0e48119da368` returned `200` with `deleted=true`.
- Post-delete filtered list returned `200` with `total=0`.

## Result

- Claim result: `passed`
- Failure class: `none`
- Cleanup status: `deleted_and_absent_from_filtered_list`
- First failure evidence: `none`

## Evidence

- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/01-auth-session.body`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/02-create.request.json`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/02-create.body`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/03-list.body`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/04-get.body`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/05-decrypt.body`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/06-delete.body`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/07-post-delete-list.body`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/_commands.generated.txt`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-credentials/raw/_execution-summary.json`
