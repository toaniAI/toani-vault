# C11 Notes

- Outcome: `passed`
- PostgreSQL reconciliation confirmed the current-run objects are persisted in `credbridge_vault`.
- Confirmed tables with current-run rows:
  - `credentials`: 3 credential IDs present; 2 are soft-deleted (`is_deleted=true`), 1 remains active.
  - `auth_sessions`: 4 backend auth session IDs present.
  - `api_tokens`: 4 access token IDs present, including one revoked token with `revoked_at` populated.
  - `sandbox_sessions`: 1 sandbox session row present for `1f481c4c-2a4e-4900-a92a-953bd60c0405`.
  - `sandbox_operations`: 1 failed execute operation row present for `a187c233-2098-4ce8-bfef-35bc8eb9193b`.
  - `audit_logs`: rows reference the current-run credential/session/token IDs in `event_data`.
- `sandbox_id=3bb759de-662d-4fee-b458-d604552ff237` is not a `sandbox_sessions.id`; it maps to `sandbox_sessions.tee_context_id` for session `1f481c4c-2a4e-4900-a92a-953bd60c0405`.
- `scope_tokens` returned zero rows for the recorded token IDs; live token persistence is in `api_tokens`.
- `auth_audit_logs` returned zero rows for the recorded session IDs.

Main mismatches:

- Persistence shape drift: issued access tokens land in `credbridge_vault.api_tokens`, not `credbridge_vault.scope_tokens`.
- Runtime object naming drift: the sandbox runtime's exposed `sandbox_id` corresponds to `credbridge_vault.sandbox_sessions.tee_context_id`, not the primary key column.
- Audit coverage is asymmetric: `audit_logs` captures these objects, but `auth_audit_logs` has no rows for the recorded session IDs.
