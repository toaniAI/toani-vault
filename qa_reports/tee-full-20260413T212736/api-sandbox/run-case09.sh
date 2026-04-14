#!/usr/bin/env bash
set -uo pipefail

RUN_ID="tee-full-20260413T212736"
CASE_ID="case09"
PREFIX="tee-full-case09-20260413T212736"
BASE_URL="https://dev-credbridge.bitkinetic.com"
RUN_DIR="/Users/yvan/AIWorkspace/credbridge/qa_reports/${RUN_ID}"
OUT_DIR="${RUN_DIR}/api-sandbox"
RAW_DIR="${OUT_DIR}/raw"
COMMANDS_FILE="${OUT_DIR}/commands.txt"
RESULT_FILE="${OUT_DIR}/result.json"
NOTES_FILE="${OUT_DIR}/notes.md"
RESIDUAL_FILE="${RUN_DIR}/residual-test-data.md"
STORAGE_STATE_FILE="${RUN_DIR}/web-credentials/storage-state.json"
API_AUTH_RAW_DIR="${RUN_DIR}/api-auth/raw"

mkdir -p "${RAW_DIR}"
: > "${COMMANDS_FILE}"

STARTED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
FINISHED_AT=""

SESSION_TOKEN=""
SESSION_ID=""
TENANT_ID=""
USER_ID=""
CREDENTIAL_ID=""
CREDENTIAL_SERVICE_ID=""
SANDBOX_SESSION_ID=""
SANDBOX_STATUS=""
OPERATION_ID=""
OPERATION_SUCCESS=""
OPERATION_RESULT_URL=""

CLAIM_RESULT="inconclusive"
FAILURE_CLASS=""
FIRST_FAILURE_STEP=""
FIRST_FAILURE_REASON=""
FIRST_FAILURE_HTTP=""
FIRST_FAILURE_BODY=""
TERMINATE_HTTP=""
CLEANUP_STATUS="not_started"

AUTH_ME_HTTP=""
CREDENTIAL_LIST_HTTP=""
CREATE_SESSION_HTTP=""
GET_SESSION_HTTP=""
LIST_SESSION_HTTP=""
EXECUTE_HTTP=""
GET_OPERATION_HTTP=""

ts() {
  date -u '+%Y-%m-%dT%H:%M:%SZ'
}

log_cmd() {
  printf '[%s] %s\n' "$(ts)" "$*" >> "${COMMANDS_FILE}"
}

record_failure() {
  local step="$1"
  local failure_class="$2"
  local http_code="${3:-}"
  local body_path="${4:-}"
  local reason="${5:-}"

  if [[ -z "${FIRST_FAILURE_STEP}" ]]; then
    FIRST_FAILURE_STEP="${step}"
    FAILURE_CLASS="${failure_class}"
    FIRST_FAILURE_HTTP="${http_code}"
    FIRST_FAILURE_BODY="${body_path}"
    FIRST_FAILURE_REASON="${reason}"
  fi
}

extract_json() {
  local file="$1"
  local query="$2"
  jq -er "${query}" "${file}" 2>/dev/null || true
}

json_path() {
  local file="$1"
  local query="$2"
  local value
  value="$(extract_json "${file}" "${query}")"
  if [[ "${value}" == "null" ]]; then
    value=""
  fi
  printf '%s' "${value}"
}

http_request() {
  local name="$1"
  local method="$2"
  local path="$3"
  local bearer="${4:-}"
  local request_file="${5:-}"

  local url="${BASE_URL}${path}"
  local headers_file="${RAW_DIR}/${name}.headers"
  local body_file="${RAW_DIR}/${name}.body.json"
  local stderr_file="${RAW_DIR}/${name}.stderr.txt"
  local meta_file="${RAW_DIR}/${name}.meta.json"
  local http_code=""
  local curl_exit=0
  local auth_label="none"

  : > "${stderr_file}"

  if [[ -n "${request_file}" && -f "${request_file}" && "${request_file}" != "${RAW_DIR}/${name}.request.json" ]]; then
    cp "${request_file}" "${RAW_DIR}/${name}.request.json"
  fi

  if [[ -n "${bearer}" ]]; then
    auth_label="bearer"
  fi

  log_cmd "curl -sS -D ${headers_file} -o ${body_file} -X ${method} ${url} auth=${auth_label} request=$(basename "${request_file:-none}")"

  local -a curl_args=(
    -sS
    -D "${headers_file}"
    -o "${body_file}"
    -w '%{http_code}'
    -X "${method}"
    -H 'Accept: application/json'
    "${url}"
  )

  if [[ -n "${bearer}" ]]; then
    curl_args+=(-H "Authorization: Bearer ${bearer}")
  fi

  if [[ -n "${request_file}" ]]; then
    curl_args+=(-H 'Content-Type: application/json' --data-binary "@${request_file}")
  fi

  http_code="$(curl "${curl_args[@]}" 2>"${stderr_file}")" || curl_exit=$?

  jq -n \
    --arg name "${name}" \
    --arg method "${method}" \
    --arg url "${url}" \
    --arg timestamp "$(ts)" \
    --arg http_code "${http_code}" \
    --argjson curl_exit "${curl_exit}" \
    --arg headers_file "${headers_file}" \
    --arg body_file "${body_file}" \
    --arg stderr_file "${stderr_file}" \
    '{
      name: $name,
      method: $method,
      url: $url,
      timestamp: $timestamp,
      http_code: $http_code,
      curl_exit: $curl_exit,
      headers_file: $headers_file,
      body_file: $body_file,
      stderr_file: $stderr_file
    }' > "${meta_file}"

  printf '%s' "${http_code}"
}

append_residual_records() {
  if [[ -n "${SANDBOX_SESSION_ID}" ]]; then
    printf '%s | session | %s | %s | %s\n' "${CASE_ID}" "${SANDBOX_SESSION_ID}" "${CLEANUP_STATUS}" "${PREFIX}" >> "${RESIDUAL_FILE}"
  fi
  if [[ -n "${OPERATION_ID}" ]]; then
    printf '%s | operation | %s | %s | %s\n' "${CASE_ID}" "${OPERATION_ID}" "observed" "${PREFIX}" >> "${RESIDUAL_FILE}"
  fi
}

write_reports() {
  FINISHED_AT="$(ts)"

  local create_session_body="${RAW_DIR}/05-sandbox-create-session.body.json"
  local execute_body="${RAW_DIR}/08-sandbox-execute.body.json"

  jq -n \
    --arg run_id "${RUN_ID}" \
    --arg case_id "${CASE_ID}" \
    --arg prefix "${PREFIX}" \
    --arg started_at "${STARTED_AT}" \
    --arg finished_at "${FINISHED_AT}" \
    --arg claim_result "${CLAIM_RESULT}" \
    --arg failure_class "${FAILURE_CLASS}" \
    --arg first_failure_step "${FIRST_FAILURE_STEP}" \
    --arg first_failure_reason "${FIRST_FAILURE_REASON}" \
    --arg first_failure_http "${FIRST_FAILURE_HTTP}" \
    --arg first_failure_body "${FIRST_FAILURE_BODY}" \
    --arg session_id "${SESSION_ID}" \
    --arg tenant_id "${TENANT_ID}" \
    --arg user_id "${USER_ID}" \
    --arg credential_id "${CREDENTIAL_ID}" \
    --arg credential_service_id "${CREDENTIAL_SERVICE_ID}" \
    --arg sandbox_session_id "${SANDBOX_SESSION_ID}" \
    --arg sandbox_status "${SANDBOX_STATUS}" \
    --arg operation_id "${OPERATION_ID}" \
    --arg operation_success "${OPERATION_SUCCESS}" \
    --arg operation_result_url "${OPERATION_RESULT_URL}" \
    --arg cleanup_status "${CLEANUP_STATUS}" \
    --arg auth_me_http "${AUTH_ME_HTTP}" \
    --arg credential_list_http "${CREDENTIAL_LIST_HTTP}" \
    --arg create_session_http "${CREATE_SESSION_HTTP}" \
    --arg get_session_http "${GET_SESSION_HTTP}" \
    --arg list_session_http "${LIST_SESSION_HTTP}" \
    --arg execute_http "${EXECUTE_HTTP}" \
    --arg get_operation_http "${GET_OPERATION_HTTP}" \
    --arg terminate_http "${TERMINATE_HTTP}" \
    --arg commands_file "${COMMANDS_FILE}" \
    --arg notes_file "${NOTES_FILE}" \
    --arg create_session_body "${create_session_body}" \
    --arg execute_body "${execute_body}" \
    '{
      run_id: $run_id,
      case_id: $case_id,
      prefix: $prefix,
      environment: "https://dev-credbridge.bitkinetic.com/",
      started_at: $started_at,
      finished_at: $finished_at,
      claim_result: $claim_result,
      failure_class: ($failure_class | select(. != "")),
      identity: {
        user_id: ($user_id | select(. != "")),
        tenant_id: ($tenant_id | select(. != "")),
        session_id: ($session_id | select(. != "")),
        credential_id: ($credential_id | select(. != "")),
        credential_service_id: ($credential_service_id | select(. != ""))
      },
      sandbox: {
        session_id: ($sandbox_session_id | select(. != "")),
        status: ($sandbox_status | select(. != "")),
        operation_id: ($operation_id | select(. != "")),
        operation_success: ($operation_success | select(. != "")),
        operation_result_url: ($operation_result_url | select(. != "")),
        cleanup_status: $cleanup_status
      },
      first_failure: {
        step: ($first_failure_step | select(. != "")),
        reason: ($first_failure_reason | select(. != "")),
        http_code: ($first_failure_http | select(. != "")),
        body_path: ($first_failure_body | select(. != ""))
      },
      http_statuses: {
        auth_me: ($auth_me_http | select(. != "")),
        credential_list: ($credential_list_http | select(. != "")),
        sandbox_create_session: ($create_session_http | select(. != "")),
        sandbox_get_session: ($get_session_http | select(. != "")),
        sandbox_list_sessions: ($list_session_http | select(. != "")),
        sandbox_execute: ($execute_http | select(. != "")),
        sandbox_get_operation: ($get_operation_http | select(. != "")),
        sandbox_terminate: ($terminate_http | select(. != ""))
      },
      evidence_paths: {
        notes: $notes_file,
        commands: $commands_file,
        create_session_response: $create_session_body,
        execute_response: $execute_body
      }
    }' > "${RESULT_FILE}"

  cat > "${NOTES_FILE}" <<EOF
# ${CASE_ID} Sandbox API chain

- Run ID: \`${RUN_ID}\`
- Prefix: \`${PREFIX}\`
- Environment: \`${BASE_URL}/\`
- Started at (UTC): \`${STARTED_AT}\`
- Finished at (UTC): \`${FINISHED_AT}\`
- Claim result: \`${CLAIM_RESULT}\`
- Failure class: \`${FAILURE_CLASS:-none}\`

## Execution summary

- Backend session: \`${SESSION_ID:-missing}\`
- Tenant: \`${TENANT_ID:-missing}\`
- Credential: \`${CREDENTIAL_ID:-missing}\` (\`${CREDENTIAL_SERVICE_ID:-unknown}\`)
- Sandbox session: \`${SANDBOX_SESSION_ID:-missing}\`
- Final sandbox status: \`${SANDBOX_STATUS:-unknown}\`
- Operation ID: \`${OPERATION_ID:-missing}\`
- Operation success: \`${OPERATION_SUCCESS:-unknown}\`
- Operation final URL: \`${OPERATION_RESULT_URL:-unknown}\`
- Cleanup status: \`${CLEANUP_STATUS}\`

## First failure evidence

- Step: \`${FIRST_FAILURE_STEP:-none}\`
- HTTP: \`${FIRST_FAILURE_HTTP:-n/a}\`
- Reason: ${FIRST_FAILURE_REASON:-none}
- Body: ${FIRST_FAILURE_BODY:-none}

## Primary artifacts

- \`commands.txt\`
- \`result.json\`
- \`raw/01-auth-session.body.json\`
- \`raw/05-sandbox-create-session.body.json\`
- \`raw/08-sandbox-execute.body.json\`
- \`raw/09-sandbox-operation.body.json\`
- \`raw/10-sandbox-terminate.body.json\`
EOF
}

cleanup() {
  if [[ -n "${SANDBOX_SESSION_ID}" && "${CLEANUP_STATUS}" == "not_started" ]]; then
    TERMINATE_HTTP="$(http_request "10-sandbox-terminate" "DELETE" "/api/v1/sandbox/sessions/${SANDBOX_SESSION_ID}" "${SESSION_TOKEN}")"
    if [[ "${TERMINATE_HTTP}" =~ ^20[0-9]$ ]]; then
      CLEANUP_STATUS="terminated"
    else
      CLEANUP_STATUS="terminate_failed"
      record_failure "sandbox_terminate" "product" "${TERMINATE_HTTP}" "${RAW_DIR}/10-sandbox-terminate.body.json" "terminate sandbox session failed"
    fi
  fi

  append_residual_records
  write_reports
}

trap cleanup EXIT

PRIVY_TOKEN="$(jq -r '
  .origins[]?
  | select(.origin == "https://dev-credbridge.bitkinetic.com")
  | .localStorage[]?
  | select(.name == "privy:token")
  | .value
' "${STORAGE_STATE_FILE}" 2>/dev/null | sed 's/^"//; s/"$//')"

if [[ -z "${PRIVY_TOKEN}" || "${PRIVY_TOKEN}" == "null" ]]; then
  PRIVY_TOKEN="$(sed -n '1p' "${API_AUTH_RAW_DIR}/fresh_privy_token.txt" 2>/dev/null || true)"
fi

if [[ -z "${PRIVY_TOKEN}" ]]; then
  CLAIM_RESULT="failed"
  record_failure "bootstrap_privy_token" "test_harness" "" "" "missing Privy token source"
  exit 0
fi

cat > "${RAW_DIR}/01-auth-session.request.json" <<EOF
{"privy_access_token":"${PRIVY_TOKEN}"}
EOF

SESSION_CREATE_HTTP="$(http_request "01-auth-session" "POST" "/api/v1/auth/session" "" "${RAW_DIR}/01-auth-session.request.json")"

if [[ ! "${SESSION_CREATE_HTTP}" =~ ^20[0-9]$ ]]; then
  CLAIM_RESULT="failed"
  record_failure "auth_session" "product" "${SESSION_CREATE_HTTP}" "${RAW_DIR}/01-auth-session.body.json" "auth session creation failed"
  exit 0
fi

SESSION_TOKEN="$(json_path "${RAW_DIR}/01-auth-session.body.json" '.session.session_token // .data.session.session_token // empty')"
SESSION_ID="$(json_path "${RAW_DIR}/01-auth-session.body.json" '.session.id // .data.session.id // empty')"
TENANT_ID="$(json_path "${RAW_DIR}/01-auth-session.body.json" '.current_tenant.id // .data.current_tenant.id // empty')"
USER_ID="$(json_path "${RAW_DIR}/01-auth-session.body.json" '.user.id // .data.user.id // empty')"

if [[ -z "${SESSION_TOKEN}" ]]; then
  CLAIM_RESULT="failed"
  record_failure "auth_session_parse" "product" "${SESSION_CREATE_HTTP}" "${RAW_DIR}/01-auth-session.body.json" "session token missing in auth/session response"
  exit 0
fi

AUTH_ME_HTTP="$(http_request "02-auth-me" "GET" "/api/v1/auth/me" "${SESSION_TOKEN}")"
if [[ ! "${AUTH_ME_HTTP}" =~ ^20[0-9]$ ]]; then
  CLAIM_RESULT="failed"
  record_failure "auth_me" "product" "${AUTH_ME_HTTP}" "${RAW_DIR}/02-auth-me.body.json" "auth/me failed for fresh session"
  exit 0
fi

CREDENTIAL_LIST_HTTP="$(http_request "03-credentials-list" "GET" "/api/v1/credentials" "${SESSION_TOKEN}")"
if [[ ! "${CREDENTIAL_LIST_HTTP}" =~ ^20[0-9]$ ]]; then
  CLAIM_RESULT="failed"
  record_failure "credentials_list" "product" "${CREDENTIAL_LIST_HTTP}" "${RAW_DIR}/03-credentials-list.body.json" "credential list failed"
  exit 0
fi

CREDENTIAL_ID="$(json_path "${RAW_DIR}/03-credentials-list.body.json" '.credentials[0].credential_id // .data.credentials[0].credential_id // empty')"
CREDENTIAL_SERVICE_ID="$(json_path "${RAW_DIR}/03-credentials-list.body.json" '.credentials[0].service_id // .data.credentials[0].service_id // empty')"

if [[ -z "${CREDENTIAL_ID}" ]]; then
  CLAIM_RESULT="failed"
  record_failure "credentials_selection" "data" "${CREDENTIAL_LIST_HTTP}" "${RAW_DIR}/03-credentials-list.body.json" "no credential available for sandbox session"
  exit 0
fi

cat > "${RAW_DIR}/05-sandbox-create-session.request.json" <<EOF
{
  "credential_id": "${CREDENTIAL_ID}",
  "original_intent": "${PREFIX} sandbox api chain",
  "metadata": {
    "case_id": "${CASE_ID}",
    "run_id": "${RUN_ID}",
    "prefix": "${PREFIX}"
  }
}
EOF

CREATE_SESSION_HTTP="$(http_request "05-sandbox-create-session" "POST" "/api/v1/sandbox/sessions" "${SESSION_TOKEN}" "${RAW_DIR}/05-sandbox-create-session.request.json")"

if [[ ! "${CREATE_SESSION_HTTP}" =~ ^20[0-9]$ ]]; then
  CLAIM_RESULT="failed"
  record_failure "sandbox_create_session" "product" "${CREATE_SESSION_HTTP}" "${RAW_DIR}/05-sandbox-create-session.body.json" "sandbox session creation failed"
  exit 0
fi

SANDBOX_SESSION_ID="$(json_path "${RAW_DIR}/05-sandbox-create-session.body.json" '.session_id // .data.session_id // empty')"
SANDBOX_STATUS="$(json_path "${RAW_DIR}/05-sandbox-create-session.body.json" '.status // .data.status // empty')"

if [[ -z "${SANDBOX_SESSION_ID}" ]]; then
  CLAIM_RESULT="failed"
  record_failure "sandbox_create_session_parse" "product" "${CREATE_SESSION_HTTP}" "${RAW_DIR}/05-sandbox-create-session.body.json" "sandbox session id missing in create response"
  exit 0
fi

for attempt in 1 2 3 4 5 6 7 8 9 10; do
  GET_SESSION_HTTP="$(http_request "06-sandbox-get-session-${attempt}" "GET" "/api/v1/sandbox/sessions/${SANDBOX_SESSION_ID}" "${SESSION_TOKEN}")"
  if [[ ! "${GET_SESSION_HTTP}" =~ ^20[0-9]$ ]]; then
    CLAIM_RESULT="failed"
    record_failure "sandbox_get_session" "product" "${GET_SESSION_HTTP}" "${RAW_DIR}/06-sandbox-get-session-${attempt}.body.json" "sandbox get-session failed"
    exit 0
  fi

  SANDBOX_STATUS="$(json_path "${RAW_DIR}/06-sandbox-get-session-${attempt}.body.json" '.status // .data.status // empty')"
  if [[ "${SANDBOX_STATUS}" == "ready" || "${SANDBOX_STATUS}" == "active" ]]; then
    cp "${RAW_DIR}/06-sandbox-get-session-${attempt}.body.json" "${RAW_DIR}/06-sandbox-get-session.body.json"
    cp "${RAW_DIR}/06-sandbox-get-session-${attempt}.headers" "${RAW_DIR}/06-sandbox-get-session.headers"
    cp "${RAW_DIR}/06-sandbox-get-session-${attempt}.meta.json" "${RAW_DIR}/06-sandbox-get-session.meta.json"
    cp "${RAW_DIR}/06-sandbox-get-session-${attempt}.stderr.txt" "${RAW_DIR}/06-sandbox-get-session.stderr.txt"
    break
  fi
  sleep 2
done

if [[ "${SANDBOX_STATUS}" != "ready" && "${SANDBOX_STATUS}" != "active" ]]; then
  CLAIM_RESULT="failed"
  record_failure "sandbox_wait_ready" "product" "${GET_SESSION_HTTP}" "${RAW_DIR}/06-sandbox-get-session.body.json" "sandbox session never reached ready state"
  exit 0
fi

LIST_SESSION_HTTP="$(http_request "07-sandbox-list-sessions" "GET" "/api/v1/sandbox/sessions" "${SESSION_TOKEN}")"
if [[ ! "${LIST_SESSION_HTTP}" =~ ^20[0-9]$ ]]; then
  CLAIM_RESULT="failed"
  record_failure "sandbox_list_sessions" "product" "${LIST_SESSION_HTTP}" "${RAW_DIR}/07-sandbox-list-sessions.body.json" "sandbox list-sessions failed"
  exit 0
fi

cat > "${RAW_DIR}/08-sandbox-execute.request.json" <<EOF
{
  "operation_type": "navigate",
  "description": "${PREFIX} navigate example.com",
  "parameters": {
    "url": "https://example.com"
  }
}
EOF

EXECUTE_HTTP="$(http_request "08-sandbox-execute" "POST" "/api/v1/sandbox/sessions/${SANDBOX_SESSION_ID}/execute" "${SESSION_TOKEN}" "${RAW_DIR}/08-sandbox-execute.request.json")"

if [[ ! "${EXECUTE_HTTP}" =~ ^20[0-9]$ ]]; then
  CLAIM_RESULT="failed"
  record_failure "sandbox_execute" "product" "${EXECUTE_HTTP}" "${RAW_DIR}/08-sandbox-execute.body.json" "sandbox execute returned non-2xx"
  exit 0
fi

OPERATION_ID="$(json_path "${RAW_DIR}/08-sandbox-execute.body.json" '.operation_id // .data.operation_id // empty')"
OPERATION_SUCCESS="$(json_path "${RAW_DIR}/08-sandbox-execute.body.json" '.success // .data.success // empty')"
OPERATION_RESULT_URL="$(json_path "${RAW_DIR}/08-sandbox-execute.body.json" '.data.final_url // .data.data.final_url // empty')"

if [[ "${OPERATION_SUCCESS}" != "true" ]]; then
  CLAIM_RESULT="failed"
  record_failure "sandbox_execute_result" "product" "${EXECUTE_HTTP}" "${RAW_DIR}/08-sandbox-execute.body.json" "sandbox execute completed with success=false"
  exit 0
fi

if [[ -z "${OPERATION_ID}" ]]; then
  CLAIM_RESULT="failed"
  record_failure "sandbox_execute_parse" "product" "${EXECUTE_HTTP}" "${RAW_DIR}/08-sandbox-execute.body.json" "operation id missing after execute"
  exit 0
fi

GET_OPERATION_HTTP="$(http_request "09-sandbox-operation" "GET" "/api/v1/sandbox/operations/${OPERATION_ID}" "${SESSION_TOKEN}")"
if [[ ! "${GET_OPERATION_HTTP}" =~ ^20[0-9]$ ]]; then
  CLAIM_RESULT="failed"
  record_failure "sandbox_get_operation" "product" "${GET_OPERATION_HTTP}" "${RAW_DIR}/09-sandbox-operation.body.json" "sandbox operation fetch failed"
  exit 0
fi

CLAIM_RESULT="passed"

exit 0
