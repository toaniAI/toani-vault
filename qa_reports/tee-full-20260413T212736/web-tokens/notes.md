# case04 notes

- status: completed
- run_id: tee-full-20260413T212736
- case_id: case04
- prefix: tee-full-case04-20260413T212736
- target: Tokens UI lifecycle in dashboard
- evidence_dir: /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-tokens

## Timeline

- initialized evidence artifacts
- verified repo-local Playwright under `/Users/yvan/AIWorkspace/credbridge/frontend/node_modules/playwright`
- logged in through Privy email OTP flow with `test-7226@privy.io` and code `450192`
- landed in dashboard and navigated to `/tokens`
- observed Tokens page constraints before submission
- selected one credential and generated a token successfully
- checked post-create UI for revoke/delete and found no supported cleanup control

## Claim Result

- claim_result: passed
- failure_class: none

## Observations

- Tokens page enforces a single scope path: `credential:read`
- The page explicitly states it only issues `credential:read` tokens and requires a credential whitelist
- The submit button is disabled before selecting any credential and becomes enabled after one selection
- The create request sent `credential_ids` as a required whitelist array with one selected credential ID
- The token creation request succeeded with HTTP 200 and returned token `019d8711-29fe-7d61-a4e8-18e21f1b44a4`
- The returned token response had `scope=credential:read`, `issued_from=access_token`, and the selected credential ID only
- No revoke or delete control was visible in the Tokens page post-create success state, so cleanup was not possible from this UI

## Request/Response Summary

- `GET /api/v1/credentials?only_valid=true` returned 3 active credentials
- `POST /api/v1/tokens` payload: `{\"scopes\":[\"credential:read\"],\"expires_in\":3600,\"credential_ids\":[\"019d868b-272a-7333-92b7-b21d5a3241b5\"]}`
- `POST /api/v1/tokens` response: `200 OK`, `token_id=019d8711-29fe-7d61-a4e8-18e21f1b44a4`, `scope=credential:read`, `credential_ids=[019d868b-272a-7333-92b7-b21d5a3241b5]`

## Evidence Paths

- screenshots: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-tokens/screenshots`
- storage state: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-tokens/storage-state.json`
- summaries: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-tokens/request-response-summaries.json`
- observations: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-tokens/playwright-observations.json`
- note: HAR was configured in the runner but did not flush to disk; request/response summaries and raw response bodies were captured instead

## Cleanup

- cleanup_status: ui_not_supported
- created_token_id: `019d8711-29fe-7d61-a4e8-18e21f1b44a4`
