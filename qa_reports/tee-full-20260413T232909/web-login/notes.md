# Case02 Notes

- Claim: `C02`
- Target: `https://dev-credbridge.bitkinetic.com/`
- Status: `passed`
- Failure class: `none`

## Outcome

- Final landing URL: `https://dev-credbridge.bitkinetic.com/credentials`
- Final title: `Toani Vault`
- Dashboard shell visible: `true`
- Summary: Privy OTP login succeeded and landed on https://dev-credbridge.bitkinetic.com/credentials. Dashboard shell was visible and post-login route checks were recorded.

## Navigation Findings

- `/` -> `https://dev-credbridge.bitkinetic.com/credentials` (redirected)
- `/credentials` -> `https://dev-credbridge.bitkinetic.com/credentials`
- `/tokens` -> `https://dev-credbridge.bitkinetic.com/tokens`
- `/developer` -> `https://dev-credbridge.bitkinetic.com/developer`
- `/profile` -> `https://dev-credbridge.bitkinetic.com/profile`
- `/users` -> `https://dev-credbridge.bitkinetic.com/users`
- `/audit` -> `https://dev-credbridge.bitkinetic.com/credentials` (redirected)
- `/tenants` -> `https://dev-credbridge.bitkinetic.com/credentials` (redirected)
- `/settings` -> `https://dev-credbridge.bitkinetic.com/credentials` (redirected)

## Session Context

- Storage state: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-login/storage-state.json`
- Cookies: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-login/cookies.json`
- Local storage: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-login/localstorage.json`
- Session storage: `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-login/sessionstorage.json`
- Bearer candidates (raw local artifact only): `/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-login/bearer-candidates.json`

## Contract Findings

- `/` redirected to `https://dev-credbridge.bitkinetic.com/credentials`
- `/audit` redirected to `https://dev-credbridge.bitkinetic.com/credentials`
- `/tenants` redirected to `https://dev-credbridge.bitkinetic.com/credentials`
- `/settings` redirected to `https://dev-credbridge.bitkinetic.com/credentials`

## Raw Evidence

- Screenshots: `before-login.png`, `post-submit.png`, `after-login.png`
- Trace: `trace.trace`
- Network: `network-log.txt`
- Observation: `playwright-observation.json`
