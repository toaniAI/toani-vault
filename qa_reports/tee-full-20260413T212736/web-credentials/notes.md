# Case03 Notes

- Run ID: `tee-full-20260413T212736`
- Case: `case03`
- Base URL: https://dev-credbridge.bitkinetic.com/
- Actor: `test-7226@privy.io`
- Object prefix: `tee-full-case03-20260413T212736`
- Claim result: `failed`
- Failure class: `env`
- Created credential ID: `not captured`
- Cleanup: attempted=`false`, supportedByUi=`false`, deleted=`false`

## Contract observations
- Credentials page reached: `true`
- Create succeeded: `false`
- List showed metadata only: `false`
- Detail entry present: `false`
- Detail request observed: `false`
- Decrypt entry present: `false`
- Decrypt request observed: `false`

## Notes
- locator.waitFor: Timeout 30000ms exceeded.
Call log:
[2m  - waiting for getByRole('dialog') to be visible[22m

    at createCredential (/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-credentials/run-case03.cjs:241:34)
    at async main (/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-credentials/run-case03.cjs:407:5)

## Evidence
- /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-credentials/01-login-page.png
- /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-credentials/03-credentials-page-before-create.png
- /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-credentials/99-failure.png