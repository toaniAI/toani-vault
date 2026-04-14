# Case04 Web Tokens

## Outcome

`C04` passed. The live `/tokens` flow enforces the intended least-privilege contract, requires explicit credential whitelist selection before issuance, and successfully issued a real token in the TEE environment.

## What was verified

- The page hard-locks scope to `credential:read`.
- The `生成 Token` action is disabled until at least one credential is selected.
- Existing credentials were available, so no dependency credential had to be created in this case.
- After issuance, the page entered a one-time display state and showed:
  - a full access token once
  - `Token ID`
  - scope
  - selected credential count
  - issued / expiry timestamps
- No revoke/delete affordance was visible on the issuance success state that was captured.

## Dependency used

- Selected existing credential:
  - service: `tee-full-case10-20260413T212736`
  - credential_id: `019d8714-3949-7f32-8804-9c593f7c7a3e`

## Contract findings

- Positive:
  - UI copy explicitly states least-privilege and whitelist enforcement.
  - Token is described as sandbox-only and one-time display, which matches the observed success screen.
- Mismatch worth carrying forward:
  - The post-issuance curl example still targets `http://localhost:8080/api/v1/sandbox/sessions` instead of the live environment host. This is a contract-content drift, but it did not block or invalidate the issuance workflow itself.

## Created object

- token_id: `019d8783-468b-7ff3-ad32-2ec324442ecf`
- display hint: `v4.local...QYbZJs`

## Artifact highlights

- Before state screenshot: `raw/tokens-before.png`
- After issuance screenshot: `raw/tokens-after.png`
- Raw Playwright result: `raw/create-token.raw.txt`
- Network log: `raw/network-log.txt`
- Trace: `raw/trace.trace`, `raw/trace.network`
