#!/usr/bin/env bash
set -euo pipefail

RUN_DIR="/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-sandbox"
RAW_DIR="${RUN_DIR}/raw"
AUTH_RAW_DIR="/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-auth/raw"
BASE_URL="https://dev-credbridge.bitkinetic.com"
API_BASE="${BASE_URL}/api/v1"
PREFIX="tee-full-case09-20260413T232909"

mkdir -p "${RAW_DIR}"

PRIVY_ACCESS_TOKEN="$(jq -r '.privy_access_token' "${AUTH_RAW_DIR}/session.request.json")"
CREDENTIAL_ID="$(jq -r '.credentials[0].credential_id' "${AUTH_RAW_DIR}/credentials-list.response.json")"
TENANT_ID="$(jq -r '.current_tenant.id' "${AUTH_RAW_DIR}/session.response.json")"

if [[ -z "${PRIVY_ACCESS_TOKEN}" || "${PRIVY_ACCESS_TOKEN}" == "null" ]]; then
  echo "missing privy access token" >&2
  exit 1
fi

if [[ -z "${CREDENTIAL_ID}" || "${CREDENTIAL_ID}" == "null" ]]; then
  echo "missing credential id" >&2
  exit 1
fi

cat > "${RAW_DIR}/session-refresh.request.json" <<EOF
{
  "privy_access_token": "${PRIVY_ACCESS_TOKEN}"
}
EOF

curl -sS -D "${RAW_DIR}/session-refresh.headers.txt" \
  -o "${RAW_DIR}/session-refresh.response.json" \
  -w '%{http_code}\n' \
  -H "Content-Type: application/json" \
  -X POST \
  --data @"${RAW_DIR}/session-refresh.request.json" \
  "${API_BASE}/auth/session" \
  > "${RAW_DIR}/session-refresh.status.txt"

SESSION_TOKEN="$(jq -r '.session.session_token' "${RAW_DIR}/session-refresh.response.json")"
SESSION_AUTH_ID="$(jq -r '.session.id // empty' "${RAW_DIR}/session-refresh.response.json")"

if [[ -z "${SESSION_TOKEN}" || "${SESSION_TOKEN}" == "null" ]]; then
  echo "failed to mint session token" >&2
  exit 1
fi

AUTH_HEADER="Authorization: Bearer ${SESSION_TOKEN}"
CONTENT_HEADER="Content-Type: application/json"

cat > "${RAW_DIR}/create-session.request.json" <<EOF
{
  "credential_id": "${CREDENTIAL_ID}",
  "original_intent": "${PREFIX}-navigate-smoke",
  "metadata": {
    "run_prefix": "${PREFIX}",
    "case_id": "C09",
    "tenant_id_hint": "${TENANT_ID}"
  }
}
EOF

curl -sS -D "${RAW_DIR}/create-session.headers.txt" \
  -o "${RAW_DIR}/create-session.response.json" \
  -w '%{http_code}\n' \
  -H "${AUTH_HEADER}" \
  -H "${CONTENT_HEADER}" \
  -X POST \
  --data @"${RAW_DIR}/create-session.request.json" \
  "${API_BASE}/sandbox/sessions" \
  > "${RAW_DIR}/create-session.status.txt"

SESSION_ID="$(jq -r '.data.session_id // .session_id // empty' "${RAW_DIR}/create-session.response.json")"
SANDBOX_ID="$(jq -r '.data.sandbox_id // .sandbox_id // empty' "${RAW_DIR}/create-session.response.json")"

if [[ -n "${SESSION_ID}" ]]; then
  curl -sS -D "${RAW_DIR}/get-session.headers.txt" \
    -o "${RAW_DIR}/get-session.response.json" \
    -w '%{http_code}\n' \
    -H "${AUTH_HEADER}" \
    "${API_BASE}/sandbox/sessions/${SESSION_ID}" \
    > "${RAW_DIR}/get-session.status.txt"
fi

cat > "${RAW_DIR}/stats.before.response.placeholder" <<'EOF'
stats not yet queried
EOF

curl -sS -D "${RAW_DIR}/stats.before.headers.txt" \
  -o "${RAW_DIR}/stats.before.response.json" \
  -w '%{http_code}\n' \
  -H "${AUTH_HEADER}" \
  "${API_BASE}/sandbox/stats" \
  > "${RAW_DIR}/stats.before.status.txt"

if [[ -n "${SESSION_ID}" ]]; then
  cat > "${RAW_DIR}/execute.request.json" <<EOF
{
  "operation_type": "navigate",
  "description": "${PREFIX}-navigate-example",
  "parameters": {
    "url": "https://example.com"
  }
}
EOF

  curl -sS -D "${RAW_DIR}/execute.headers.txt" \
    -o "${RAW_DIR}/execute.response.json" \
    -w '%{http_code}\n' \
    -H "${AUTH_HEADER}" \
    -H "${CONTENT_HEADER}" \
    -X POST \
    --data @"${RAW_DIR}/execute.request.json" \
    "${API_BASE}/sandbox/sessions/${SESSION_ID}/execute" \
    > "${RAW_DIR}/execute.status.txt" || true
fi

OPERATION_ID=""
if [[ -f "${RAW_DIR}/execute.response.json" ]]; then
  OPERATION_ID="$(jq -r '.data.operation_id // .operation_id // empty' "${RAW_DIR}/execute.response.json")"
fi

if [[ -n "${OPERATION_ID}" ]]; then
  curl -sS -D "${RAW_DIR}/get-operation.headers.txt" \
    -o "${RAW_DIR}/get-operation.response.json" \
    -w '%{http_code}\n' \
    -H "${AUTH_HEADER}" \
    "${API_BASE}/sandbox/operations/${OPERATION_ID}" \
    > "${RAW_DIR}/get-operation.status.txt"
fi

if [[ -n "${SESSION_ID}" ]]; then
  curl -sS -D "${RAW_DIR}/terminate.headers.txt" \
    -o "${RAW_DIR}/terminate.response.json" \
    -w '%{http_code}\n' \
    -H "${AUTH_HEADER}" \
    -X DELETE \
    "${API_BASE}/sandbox/sessions/${SESSION_ID}" \
    > "${RAW_DIR}/terminate.status.txt" || true
fi

curl -sS -D "${RAW_DIR}/stats.after.headers.txt" \
  -o "${RAW_DIR}/stats.after.response.json" \
  -w '%{http_code}\n' \
  -H "${AUTH_HEADER}" \
  "${API_BASE}/sandbox/stats" \
  > "${RAW_DIR}/stats.after.status.txt"

printf '%s\n' "${SESSION_ID}" > "${RAW_DIR}/session_id.txt"
printf '%s\n' "${SANDBOX_ID}" > "${RAW_DIR}/sandbox_id.txt"
printf '%s\n' "${OPERATION_ID}" > "${RAW_DIR}/operation_id.txt"
printf '%s\n' "${SESSION_AUTH_ID}" > "${RAW_DIR}/session_auth_id.txt"
