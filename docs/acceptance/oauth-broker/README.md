# OAuth Broker Acceptance Docs

This folder contains stage-level acceptance documents for the `ZKM-100` OAuth Broker breakdown.

## Task mapping

- `ZKM-162` -> `zkm-162-v1-boundary-and-naming-acceptance.md`
- `ZKM-171` -> `zkm-171-broker-resource-skeleton-acceptance.md`
- `ZKM-172` -> `zkm-172-provider-registry-and-callback-governance-acceptance.md`
- `ZKM-173` -> `zkm-173-binding-handle-token-boundary-acceptance.md`
- `ZKM-174` -> `zkm-174-lark-app-or-tenant-token-binding-acceptance.md`
- `ZKM-175` -> `zkm-175-google-service-account-binding-acceptance.md`
- `ZKM-176` -> `zkm-176-lark-delegated-user-binding-acceptance.md`
- `ZKM-177` -> `zkm-177-google-workspace-delegated-user-binding-acceptance.md`
- `ZKM-178` -> `zkm-178-runtime-hardening-and-lifecycle-acceptance.md`

## Lark doc mapping

| Issue | Local file | Lark doc |
| --- | --- | --- |
| `ZKM-162` | `zkm-162-v1-boundary-and-naming-acceptance.md` | `https://digitalpulse.sg.larksuite.com/docx/YECkdikZiobomQxAKdfl978UgWe` |
| `ZKM-171` | `zkm-171-broker-resource-skeleton-acceptance.md` | `https://digitalpulse.sg.larksuite.com/docx/Pw64dGSgroa8i5xDxfUlDjFigjb` |
| `ZKM-172` | `zkm-172-provider-registry-and-callback-governance-acceptance.md` | `https://digitalpulse.sg.larksuite.com/docx/FD0rd1ptwoFf76xliJnlu3c7gNe` |
| `ZKM-173` | `zkm-173-binding-handle-token-boundary-acceptance.md` | `https://digitalpulse.sg.larksuite.com/docx/LvxZdncgYoOjTIx62ICl4mSRgge` |
| `ZKM-174` | `zkm-174-lark-app-or-tenant-token-binding-acceptance.md` | `https://digitalpulse.sg.larksuite.com/docx/IOi1dYuEkoWZXgxXh5AlXqkbgUf` |
| `ZKM-175` | `zkm-175-google-service-account-binding-acceptance.md` | `https://digitalpulse.sg.larksuite.com/docx/X3GMdy3swocoehxoTZgl7XvegWh` |
| `ZKM-176` | `zkm-176-lark-delegated-user-binding-acceptance.md` | `https://digitalpulse.sg.larksuite.com/docx/N71odDmhwo8puExaLJrl57SHgB6` |
| `ZKM-177` | `zkm-177-google-workspace-delegated-user-binding-acceptance.md` | `https://digitalpulse.sg.larksuite.com/docx/HBind8oWcoemJQx5OeDlw2ybgAf` |
| `ZKM-178` | `zkm-178-runtime-hardening-and-lifecycle-acceptance.md` | `https://digitalpulse.sg.larksuite.com/docx/MimGdqTeYoV4CWxnR33l20QFgYd` |

## Shared evidence rules

- Prefer verification from local development environment first.
- Every passed scenario must retain evidence, not just a verbal conclusion.
- Evidence should come from at least one of:
  - local command output
  - API response payload
  - database query result
  - audit log snapshot
  - screenshot
  - provider-side or remote invocation record
- For integration scenarios that require third-party providers, keep local orchestration and system evidence local, then add the smallest necessary remote record as supplemental proof.
