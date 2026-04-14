# Case02 Notes

- Scope: Dashboard login and navigation check for `https://dev-credbridge.bitkinetic.com/` using Playwright, isolated to this evidence directory.
- Execution status: Playwright run completed successfully and wrote screenshots, HAR, and raw observation JSON.
- Claim result: `failed`
- Failure class: `product`

## Observed behavior

- Login started at `/login` and completed with Privy OTP using the assigned test identity.
- Backend session creation was observed repeatedly via `POST /api/v1/auth/session` with `200` responses.
- Landing route after login was `https://dev-credbridge.bitkinetic.com/credentials` with title `Toani Vault`.
- `GET /api/v1/auth/me` returned `200`, showing tenant `019d8414-bf83-7641-a3fb-ad93d58715bd`, role `owner`, locale `zh-CN`, and 23 scopes.
- Page access results:
  - `Credentials`: accessible at `/credentials`
  - `Tokens`: accessible at `/tokens`
  - `Profile`: accessible at `/profile`
  - `Developer Center`: accessible at `/developer`
  - `Audit`: requesting `/audit` ended at `/credentials`, and no `/api/v1/audit` request was observed in the HAR

## Session findings

- Final captured backend session ID: `019d870a-b682-7e21-bb87-7aebae048909`
- Final captured backend session token prefix: `DBeD-J1R...`
- Session expiry from final auth/session response: `2026-04-13T14:31:59.490930273+00:00`
- Browser storage contained `auth-storage`, `privy:token`, and related Privy artifacts after login.
- Browser cookies included `privy-session` and `privy-token` for `dev-credbridge.bitkinetic.com`.
- Browser console showed only `debug` and `log` messages; no console error entry was captured.

## First failure evidence

- First functional failure in scope: Audit navigation/access did not land on `/audit`; it redirected to `/credentials`.
- Evidence:
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/audit.png`
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/playwright-observation.json`
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/session.har`

## Evidence files

- Screenshots:
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/before-login.png`
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/after-login.png`
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/credentials.png`
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/tokens.png`
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/audit.png`
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/profile.png`
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/developer-center.png`
- HAR:
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/session.har`
- Raw structured observation:
  - `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login/playwright-observation.json`
