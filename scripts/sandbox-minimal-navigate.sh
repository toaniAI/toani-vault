#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${CREDBRIDGE_BASE_URL:-${TOANI_BASE_URL:-http://localhost:8080}}"
API_BASE="${BASE_URL%/}/api/v1"
TOKEN="${CREDBRIDGE_SANDBOX_TOKEN:-${TOANI_VAULT_TOKEN:-}}"
SERVICE_ID="${CREDBRIDGE_SANDBOX_SERVICE_ID:-example-service}"
INTENT="${CREDBRIDGE_SANDBOX_INTENT:-minimal navigate smoke test}"
TARGET_URL="${CREDBRIDGE_SANDBOX_TARGET_URL:-https://example.com}"

if [[ -z "${TOKEN}" ]]; then
  echo "Missing sandbox token. Set CREDBRIDGE_SANDBOX_TOKEN or TOANI_VAULT_TOKEN." >&2
  exit 1
fi

auth_header="Authorization: Bearer ${TOKEN}"
content_header="Content-Type: application/json"

session_payload="$(jq -n \
  --arg service_id "${SERVICE_ID}" \
  --arg original_intent "${INTENT}" \
  '{service_id: $service_id, original_intent: $original_intent}')"

create_response="$(curl -fsS \
  -H "${auth_header}" \
  -H "${content_header}" \
  -d "${session_payload}" \
  "${API_BASE}/sandbox/sessions")"
session_id="$(jq -r '.session_id // .id' <<<"${create_response}")"

if [[ -z "${session_id}" || "${session_id}" == "null" ]]; then
  echo "Failed to create sandbox session: ${create_response}" >&2
  exit 1
fi

echo "Created session: ${session_id}"
curl -fsS -H "${auth_header}" "${API_BASE}/sandbox/sessions/${session_id}" >/dev/null

execute_payload="$(jq -n \
  --arg url "${TARGET_URL}" \
  '{operation_type: "navigate", parameters: {url: $url}}')"

execute_response="$(curl -fsS \
  -H "${auth_header}" \
  -H "${content_header}" \
  -d "${execute_payload}" \
  "${API_BASE}/sandbox/sessions/${session_id}/execute")"
operation_id="$(jq -r '.operation_id // .id // .data.operation_id // .data.id' <<<"${execute_response}")"

if [[ -z "${operation_id}" || "${operation_id}" == "null" ]]; then
  echo "Failed to start navigate operation: ${execute_response}" >&2
  exit 1
fi

echo "Operation: ${operation_id}"

completed=false
for _ in {1..30}; do
  operation_response="$(curl -fsS \
    -H "${auth_header}" \
    "${API_BASE}/sandbox/operations/${operation_id}")"
  operation_status="$(jq -r '.status // .data.status' <<<"${operation_response}")"

  if [[ "${operation_status}" == "completed" || "${operation_status}" == "success" ]]; then
    echo "Navigate completed: ${operation_response}"
    completed=true
    break
  fi

  if [[ "${operation_status}" == "failed" ]]; then
    echo "Navigate failed: ${operation_response}" >&2
    exit 1
  fi

  sleep 1
done

if [[ "${completed}" != "true" ]]; then
  echo "Navigate did not complete within timeout" >&2
  exit 1
fi

curl -fsS -X DELETE -H "${auth_header}" "${API_BASE}/sandbox/sessions/${session_id}" >/dev/null
echo "Terminated session: ${session_id}"
