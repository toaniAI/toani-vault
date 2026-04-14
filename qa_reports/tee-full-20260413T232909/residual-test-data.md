# Residual Test Data

Record all created runtime objects from this run:

- credentials
- tokens
- sandbox sessions
- sandbox operations
- audit references

Do not delete entries from this file. Append identifiers with timestamps and owning case.

- 2026-04-13T23:43+08:00 case04-web-tokens token_id=`019d8783-468b-7ff3-ad32-2ec324442ecf` label=`not-exposed-in-ui` depends_on_credential_id=`019d8714-3949-7f32-8804-9c593f7c7a3e` service=`tee-full-case10-20260413T212736`
- credential | 019d8787-9b6e-7ff2-b785-1b04f9a5132d | tee-full-20260413T232909-webcred-20260413154821
- 2026-04-13T23:53+08:00 case07-api-credentials credential_id=`019d878b-f659-7bc0-8aaa-a252f8a8a02f` service=`tee-full-case07-20260413T232909-service-20260413T155309Z` status=`deleted-soft`
- 2026-04-13T23:54+08:00 case08-api-tokens token_id=`019d878c-ae5f-7f70-a8b6-38d63fd47173` depends_on_credential_id=`019d8714-3949-7f32-8804-9c593f7c7a3e` session_id=`019d878b-ba47-7962-9180-95a0a541a870` status=`revoked`
- 2026-04-14T00:07+08:00 case09-api-sandbox sandbox_session_id=`1f481c4c-2a4e-4900-a92a-953bd60c0405` sandbox_id=`3bb759de-662d-4fee-b458-d604552ff237` operation_id=`a187c233-2098-4ce8-bfef-35bc8eb9193b` session_id=`019d878e-e873-7c21-9e24-030937ed5509` status=`terminated-after-failed-execute`
