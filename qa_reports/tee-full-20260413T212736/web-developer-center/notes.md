# case05 notes

- Status: completed
- Scope: Developer Center contract/content only
- Target: `https://dev-credbridge.bitkinetic.com`
- Login result: success via Privy email OTP; backend session bridge returned `200` on `/api/v1/auth/session`
- Final page: `https://dev-credbridge.bitkinetic.com/developer`
- Claim result: failed
- Failure class: product

## Findings

- Stale dashboard/manual token guidance remains in `API Docs`.
  Excerpt: `Dashboard Manual Token: issued from the Dashboard Tokens page, then exchanged for short-lived sandbox access tokens.`
  Evidence: `api-docs.txt`, `browser-findings.json`, `03-api-docs.png`
- Outdated CLI onboarding guidance remains in `API Docs`.
  Excerpt: `The CLI cannot complete Privy login directly and there is no username/password login endpoint. Sign in on the web first, issue a manual token from the Dashboard Tokens page, exchange it via /api/v1/auth/access-token, then call sandbox APIs.`
  Evidence: `api-docs.txt`, `browser-findings.json`, `03-api-docs.png`
- Stale dashboard/manual token flow remains in `SDK Examples`.
  Excerpts:
  `Exchange dashboard token for short-lived sandbox access token`
  `// Exchange a dashboard-issued manual token for a short-lived access token`
  Evidence: `sdk-examples.txt`, `browser-findings.json`, `04-sdk-examples.png`
- No `decrypt` guidance was surfaced in the captured `API Docs` or `SDK Examples` page text for this run.
  Evidence: `browser-findings.json`

## Evidence paths

- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-developer-center/01-login.png`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-developer-center/02-developer-overview.png`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-developer-center/03-api-docs.png`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-developer-center/04-sdk-examples.png`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-developer-center/api-docs.txt`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-developer-center/sdk-examples.txt`
- `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-developer-center/browser-findings.json`
