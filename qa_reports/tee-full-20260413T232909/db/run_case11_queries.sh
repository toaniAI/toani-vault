#!/usr/bin/env bash
set -euo pipefail

RUN="/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909"
DBDIR="$RUN/db"
RAW="$DBDIR/raw"
mkdir -p "$RAW"

export PGPASSWORD="dnXcdYxcv56H"

psql_cmd() {
  psql -h 10.11.25.9 -p 15432 -U dn -d credbridge -v ON_ERROR_STOP=1 -At -F $'\t' -c "$1"
}

psql_cmd "select table_name, string_agg(column_name, ', ' order by ordinal_position) from information_schema.columns where table_schema='credbridge_vault' and table_name in ('credentials','auth_sessions','api_tokens','scope_tokens','audit_logs','auth_audit_logs','sandbox_sessions','sandbox_operations') group by table_name order by table_name;" > "$RAW/schema-columns.tsv"

psql_cmd "select credential_id, service_id, credential_type, version, created_at, updated_at, expires_at, is_deleted from credbridge_vault.credentials where credential_id in ('019d8714-3949-7f32-8804-9c593f7c7a3e','019d8787-9b6e-7ff2-b785-1b04f9a5132d','019d878b-f659-7bc0-8aaa-a252f8a8a02f') order by created_at;" > "$RAW/credentials.tsv"

psql_cmd "select id, user_id, identity_id, active_membership_id, created_at, expires_at, revoked_at, revoked_reason from credbridge_vault.auth_sessions where id in ('019d8782-40de-7db1-9e11-d133749ba8a0','019d878b-f5f5-7b61-8925-58c82d141ae1','019d878b-ba47-7962-9180-95a0a541a870','019d878e-e873-7c21-9e24-030937ed5509','1f481c4c-2a4e-4900-a92a-953bd60c0405') order by created_at;" > "$RAW/auth_sessions.tsv"

psql_cmd "select id, token_type, subject_type, subject_id, session_id, display_name, scopes::text, token_kind, token_name, token_prefix, credential_ids::text, expires_at, revoked_at, created_at, created_via from credbridge_vault.api_tokens where id in ('019d8783-468b-7ff3-ad32-2ec324442ecf','019d8783-4eb7-7d51-94c1-6a0c86ce49fe','019d878b-f77e-7220-a5de-65ace7aab707','019d878c-ae5f-7f70-a8b6-38d63fd47173') order by created_at;" > "$RAW/api_tokens.tsv"

psql_cmd "select token_id, credential_id, scopes::text, constraints::text, issued_at, expires_at, revoked_at, usage_count from credbridge_vault.scope_tokens where token_id in ('019d8783-468b-7ff3-ad32-2ec324442ecf','019d8783-4eb7-7d51-94c1-6a0c86ce49fe','019d878b-f77e-7220-a5de-65ace7aab707','019d878c-ae5f-7f70-a8b6-38d63fd47173') order by issued_at;" > "$RAW/scope_tokens.tsv"

psql_cmd "select id, status, created_by, credential_id, original_intent, started_at, expires_at, terminated_at, termination_reason, created_at, updated_at from credbridge_vault.sandbox_sessions where id in ('1f481c4c-2a4e-4900-a92a-953bd60c0405','3bb759de-662d-4fee-b458-d604552ff237') order by created_at;" > "$RAW/sandbox_sessions.tsv"

psql_cmd "select id, session_id, operation_type, status, credential_id, error_message, started_at, completed_at, execution_duration_ms, created_at from credbridge_vault.sandbox_operations where id in ('a187c233-2098-4ce8-bfef-35bc8eb9193b') order by created_at;" > "$RAW/sandbox_operations.tsv"

psql_cmd "select id, created_at, event_type, action, event_category, service, outcome, left(coalesce(event_data::text,''), 500) from credbridge_vault.audit_logs where event_data::text like any (array['%019d8714-3949-7f32-8804-9c593f7c7a3e%','%019d8787-9b6e-7ff2-b785-1b04f9a5132d%','%019d878b-f659-7bc0-8aaa-a252f8a8a02f%','%019d8783-468b-7ff3-ad32-2ec324442ecf%','%019d8783-4eb7-7d51-94c1-6a0c86ce49fe%','%019d878b-f77e-7220-a5de-65ace7aab707%','%019d878c-ae5f-7f70-a8b6-38d63fd47173%','%019d8782-40de-7db1-9e11-d133749ba8a0%','%019d878b-f5f5-7b61-8925-58c82d141ae1%','%019d878b-ba47-7962-9180-95a0a541a870%','%019d878e-e873-7c21-9e24-030937ed5509%','%1f481c4c-2a4e-4900-a92a-953bd60c0405%','%3bb759de-662d-4fee-b458-d604552ff237%','%a187c233-2098-4ce8-bfef-35bc8eb9193b%']) order by created_at;" > "$RAW/audit_logs.tsv"

psql_cmd "select id, created_at, event_type, severity, session_id, membership_id, user_id, tenant_id, left(coalesce(event_data::text,''), 500) from credbridge_vault.auth_audit_logs where session_id in ('019d8782-40de-7db1-9e11-d133749ba8a0','019d878b-f5f5-7b61-8925-58c82d141ae1','019d878b-ba47-7962-9180-95a0a541a870','019d878e-e873-7c21-9e24-030937ed5509','1f481c4c-2a4e-4900-a92a-953bd60c0405') order by created_at;" > "$RAW/auth_audit_logs.tsv"

psql_cmd "select 'api_tokens' as table_name, count(*)::text from credbridge_vault.api_tokens where id in ('019d8783-468b-7ff3-ad32-2ec324442ecf','019d8783-4eb7-7d51-94c1-6a0c86ce49fe','019d878b-f77e-7220-a5de-65ace7aab707','019d878c-ae5f-7f70-a8b6-38d63fd47173') union all select 'scope_tokens', count(*)::text from credbridge_vault.scope_tokens where token_id in ('019d8783-468b-7ff3-ad32-2ec324442ecf','019d8783-4eb7-7d51-94c1-6a0c86ce49fe','019d878b-f77e-7220-a5de-65ace7aab707','019d878c-ae5f-7f70-a8b6-38d63fd47173') union all select 'credentials', count(*)::text from credbridge_vault.credentials where credential_id in ('019d8714-3949-7f32-8804-9c593f7c7a3e','019d8787-9b6e-7ff2-b785-1b04f9a5132d','019d878b-f659-7bc0-8aaa-a252f8a8a02f') union all select 'auth_sessions', count(*)::text from credbridge_vault.auth_sessions where id in ('019d8782-40de-7db1-9e11-d133749ba8a0','019d878b-f5f5-7b61-8925-58c82d141ae1','019d878b-ba47-7962-9180-95a0a541a870','019d878e-e873-7c21-9e24-030937ed5509') union all select 'sandbox_sessions', count(*)::text from credbridge_vault.sandbox_sessions where id in ('1f481c4c-2a4e-4900-a92a-953bd60c0405','3bb759de-662d-4fee-b458-d604552ff237') union all select 'sandbox_operations', count(*)::text from credbridge_vault.sandbox_operations where id in ('a187c233-2098-4ce8-bfef-35bc8eb9193b');" > "$RAW/table_counts.tsv"
