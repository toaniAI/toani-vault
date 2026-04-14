# Residual Test Data

`<case-id> | <object-type> | <object-id> | <cleanup-status> | <notes>`

case04 | token | 019d8711-29fe-7d61-a4e8-18e21f1b44a4 | ui_not_supported | created via Tokens UI; no revoke/delete control visible after creation
case07 | credential | 019d8711-394c-7422-a805-68adc2dcd8c1 | deleted | service_id=tee-full-case07-20260413T212736
case07 | credential | 019d8713-52b0-7ae2-8785-0e48119da368 | deleted_and_absent_from_filtered_list | service_id=tee-full-case07-20260413T212736
case08 | credential | 019d8711-de54-76c1-a7f4-9fe8c93013e5 | deleted | service_id=tee-full-case08-20260413T212736
case08 | token | 019d8711-dee1-7e02-953d-5fdf8725f158 | revoked | created via /api/v1/tokens
case09 | credential | 019d8712-a9a5-77c2-bb22-8438ecf659fc | deleted | service_id=tee-full-case09-20260413T212736
case09 | session | a5526d1b-937e-4b45-8966-0b70560da684 | terminated | main-thread sandbox lifecycle
case09 | operation | 946c3d60-18ab-46b4-abab-56a2a72a79e1 | observed_failed | nsjail policy compile error
case09 | session | 0036f1ad-83a4-42e8-b884-6ade1ef6541d | terminated | subagent-created sandbox lifecycle
case09 | session | 685de783-9a69-4c07-9617-7664a9cf604a | terminated | subagent-created sandbox lifecycle
case09 | operation | ef82b293-3956-4ac4-9b72-b1548d8803ff | observed | subagent-created sandbox lifecycle
case10 | credential | 019d8713-e583-7531-9d46-4e0ce643873d | deleted | service_id=tee-full-case10-20260413T212736
