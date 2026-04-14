#!/bin/zsh
set -u

BASE_URL="https://dev-credbridge.bitkinetic.com"
RUN_ID="tee-full-20260413T212736"
CASE_ID="case08"
PREFIX="tee-full-case08-20260413T212736"
RUN_DIR="/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-tokens"
RAW_DIR="$RUN_DIR/raw"
SESSION_FILE="/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-auth/raw/fresh_session_token.txt"
PRIVY_FILE="/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-auth/raw/fresh_privy_token.txt"
COMMANDS_FILE="$RUN_DIR/commands.txt"
NOTES_FILE="$RUN_DIR/notes.md"
RESULT_FILE="$RUN_DIR/result.json"
RESIDUAL_FILE="/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/residual-test-data.md"
mkdir -p "$RUN_DIR" "$RAW_DIR"
: > "$COMMANDS_FILE"

timestamp_utc() {
  /bin/date -u +"%Y-%m-%dT%H:%M:%SZ"
}

json_escape() {
  printf '%s' "$1" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'
}

append_command() {
  printf '[%s] %s\n' "$(timestamp_utc)" "$1" >> "$COMMANDS_FILE"
}

run_curl() {
  local name="$1"
  local method="$2"
  local auth_kind="$3"
  local path="$4"
  local payload="${5-}"
  local token_var=""

  local headers_file="$RAW_DIR/${name}.headers.txt"
  local body_file="$RAW_DIR/${name}.body"
  local meta_file="$RAW_DIR/${name}.meta.txt"
  local stderr_file="$RAW_DIR/${name}.stderr.txt"

  local -a cmd
  cmd=(curl -sS -X "$method" "$BASE_URL$path" -D "$headers_file" -o "$body_file" -w "http_status=%{http_code}\ncontent_type=%{content_type}\n")

  if [[ "$auth_kind" == "session" ]]; then
    token_var="$SESSION_TOKEN"
    cmd+=(-H "Authorization: Bearer $token_var")
  elif [[ "$auth_kind" == "access" ]]; then
    token_var="$ACCESS_TOKEN"
    cmd+=(-H "Authorization: Bearer $token_var")
  fi

  cmd+=(-H "Accept: application/json")

  if [[ -n "$payload" ]]; then
    cmd+=(-H "Content-Type: application/json" --data "$payload")
  fi

  local rendered="rtk proxy"
  local part
  for part in "${cmd[@]}"; do
    rendered+=" $(printf '%q' "$part")"
  done
  append_command "$rendered"

  if rtk proxy "${cmd[@]}" > "$meta_file" 2> "$stderr_file"; then
    printf 'exit_code=0\n' >> "$meta_file"
  else
    local exit_code=$?
    printf 'exit_code=%s\n' "$exit_code" >> "$meta_file"
  fi
}

meta_value() {
  local file="$1"
  local key="$2"
  awk -F= -v key="$key" '$1==key {print substr($0, index($0, "=")+1)}' "$file" | tail -n 1
}

body_json_field() {
  local file="$1"
  local expr="$2"
  jq -r "$expr // empty" "$file" 2>/dev/null
}

body_json() {
  local file="$1"
  if jq empty "$file" >/dev/null 2>&1; then
    jq -c . "$file"
  else
    printf 'null'
  fi
}

trimmed_body() {
  local file="$1"
  head -c 1200 "$file" 2>/dev/null | tr '\n' ' '
}

SESSION_TOKEN="$(tr -d '\r\n' < "$SESSION_FILE" 2>/dev/null)"
PRIVY_ACCESS_TOKEN="$(tr -d '\r\n' < "$PRIVY_FILE" 2>/dev/null)"
ACCESS_TOKEN=""
CREATED_TOKEN_ID=""
CREATED_CREDENTIAL_ID=""
CREATED_CREDENTIAL_SERVICE_ID=""
NEGATIVE_MISSING_IDS_STATUS=""
NEGATIVE_INVALID_SCOPE_STATUS=""
VERIFY_GET_STATUS=""
VERIFY_LIST_STATUS=""
VERIFY_LIST_TOTAL=""
VERIFY_LIST_FOREIGN_IDS=""
POST_REVOKE_STATUS=""
AUTH_ME_STATUS=""
REBUILT_SESSION="false"
FIRST_FAILURE_STEP=""
FIRST_FAILURE_STATUS=""
FIRST_FAILURE_BODY=""
CLEANUP_TOKEN_STATUS="not_attempted"
CLEANUP_CREDENTIAL_STATUS="not_attempted"
CLAIM_RESULT="passed"
FAILURE_CLASS="null"
FINDINGS=()

register_failure() {
  local step="$1"
  local http_status="$2"
  local body_file="$3"
  if [[ -z "$FIRST_FAILURE_STEP" ]]; then
    FIRST_FAILURE_STEP="$step"
    FIRST_FAILURE_STATUS="$http_status"
    FIRST_FAILURE_BODY="$(trimmed_body "$body_file")"
  fi
}

run_curl "00-auth-me" "GET" "session" "/api/v1/auth/me"
AUTH_ME_STATUS="$(meta_value "$RAW_DIR/00-auth-me.meta.txt" "http_status")"

if [[ "$AUTH_ME_STATUS" != "200" && -n "$PRIVY_ACCESS_TOKEN" ]]; then
  local_session_payload="$(jq -cn --arg token "$PRIVY_ACCESS_TOKEN" '{privy_access_token:$token}')"
  run_curl "00b-auth-session-rebuild" "POST" "none" "/api/v1/auth/session" "$local_session_payload"
  if [[ "$(meta_value "$RAW_DIR/00b-auth-session-rebuild.meta.txt" "http_status")" == "200" ]]; then
    SESSION_TOKEN="$(body_json_field "$RAW_DIR/00b-auth-session-rebuild.body" '.session.session_token')"
    REBUILT_SESSION="true"
    run_curl "00c-auth-me-after-rebuild" "GET" "session" "/api/v1/auth/me"
    AUTH_ME_STATUS="$(meta_value "$RAW_DIR/00c-auth-me-after-rebuild.meta.txt" "http_status")"
  fi
fi

if [[ "$AUTH_ME_STATUS" != "200" ]]; then
  CLAIM_RESULT="failed"
  FAILURE_CLASS='"env"'
  register_failure "auth_me" "$AUTH_ME_STATUS" "$RAW_DIR/00-auth-me.body"
else
  CREATED_CREDENTIAL_SERVICE_ID="${PREFIX}-svc"
  credential_payload="$(jq -cn \
    --arg service_id "$CREATED_CREDENTIAL_SERVICE_ID" \
    --arg label "$PREFIX" \
    '{service_id:$service_id, credential_type:"api_key", plaintext_data:{apiKey:"cb-case08-demo-key", label:$label}}')"
  run_curl "01-create-credential" "POST" "session" "/api/v1/credentials" "$credential_payload"
  create_credential_status="$(meta_value "$RAW_DIR/01-create-credential.meta.txt" "http_status")"
  CREATED_CREDENTIAL_ID="$(body_json_field "$RAW_DIR/01-create-credential.body" '.credential_id')"

  if [[ "$create_credential_status" != "201" || -z "$CREATED_CREDENTIAL_ID" ]]; then
    CLAIM_RESULT="failed"
    FAILURE_CLASS='"product"'
    register_failure "create_credential" "$create_credential_status" "$RAW_DIR/01-create-credential.body"
  else
    token_payload="$(jq -cn \
      --arg credential_id "$CREATED_CREDENTIAL_ID" \
      '{scopes:["credential:read"], expires_in:1800, credential_ids:[$credential_id]}')"
    run_curl "02-create-token" "POST" "session" "/api/v1/tokens" "$token_payload"
    create_token_status="$(meta_value "$RAW_DIR/02-create-token.meta.txt" "http_status")"
    ACCESS_TOKEN="$(body_json_field "$RAW_DIR/02-create-token.body" '.access_token')"
    CREATED_TOKEN_ID="$(body_json_field "$RAW_DIR/02-create-token.body" '.token_id')"

    if [[ "$create_token_status" != "200" || -z "$ACCESS_TOKEN" || -z "$CREATED_TOKEN_ID" ]]; then
      CLAIM_RESULT="failed"
      FAILURE_CLASS='"product"'
      register_failure "create_token" "$create_token_status" "$RAW_DIR/02-create-token.body"
    else
      run_curl "03-list-tokens" "GET" "session" "/api/v1/tokens"
      run_curl "04-get-token" "GET" "session" "/api/v1/tokens/$CREATED_TOKEN_ID"
      run_curl "05-verify-get-credential" "GET" "access" "/api/v1/credentials/$CREATED_CREDENTIAL_ID"
      run_curl "06-verify-list-credentials" "GET" "access" "/api/v1/credentials"

      VERIFY_GET_STATUS="$(meta_value "$RAW_DIR/05-verify-get-credential.meta.txt" "http_status")"
      VERIFY_LIST_STATUS="$(meta_value "$RAW_DIR/06-verify-list-credentials.meta.txt" "http_status")"
      VERIFY_LIST_TOTAL="$(body_json_field "$RAW_DIR/06-verify-list-credentials.body" '.total')"
      VERIFY_LIST_FOREIGN_IDS="$(jq -rc --arg id "$CREATED_CREDENTIAL_ID" '[.credentials[]?.credential_id | select(. != $id)]' "$RAW_DIR/06-verify-list-credentials.body" 2>/dev/null)"

      if [[ "$VERIFY_GET_STATUS" != "200" ]]; then
        CLAIM_RESULT="failed"
        FAILURE_CLASS='"product"'
        register_failure "verify_get_credential" "$VERIFY_GET_STATUS" "$RAW_DIR/05-verify-get-credential.body"
      fi

      if [[ "$VERIFY_LIST_STATUS" == "200" && "$VERIFY_LIST_FOREIGN_IDS" != "[]" && -n "$VERIFY_LIST_FOREIGN_IDS" ]]; then
        CLAIM_RESULT="failed"
        FAILURE_CLASS='"product"'
        register_failure "verify_list_credential_whitelist" "$VERIFY_LIST_STATUS" "$RAW_DIR/06-verify-list-credentials.body"
      fi

      missing_ids_payload='{"scopes":["credential:read"],"expires_in":1800,"credential_ids":[]}'
      run_curl "07-negative-missing-credential-ids" "POST" "session" "/api/v1/tokens" "$missing_ids_payload"
      NEGATIVE_MISSING_IDS_STATUS="$(meta_value "$RAW_DIR/07-negative-missing-credential-ids.meta.txt" "http_status")"

      invalid_scope_payload="$(jq -cn \
        --arg credential_id "$CREATED_CREDENTIAL_ID" \
        '{scopes:["credential:decrypt"], expires_in:1800, credential_ids:[$credential_id]}')"
      run_curl "08-negative-invalid-scope" "POST" "session" "/api/v1/tokens" "$invalid_scope_payload"
      NEGATIVE_INVALID_SCOPE_STATUS="$(meta_value "$RAW_DIR/08-negative-invalid-scope.meta.txt" "http_status")"

      run_curl "09-revoke-token" "POST" "session" "/api/v1/tokens/$CREATED_TOKEN_ID/revoke"
      revoke_status="$(meta_value "$RAW_DIR/09-revoke-token.meta.txt" "http_status")"
      if [[ "$revoke_status" == "200" ]]; then
        CLEANUP_TOKEN_STATUS="revoked"
      else
        CLEANUP_TOKEN_STATUS="revoke_failed"
        CLAIM_RESULT="failed"
        FAILURE_CLASS='"product"'
        register_failure "revoke_token" "$revoke_status" "$RAW_DIR/09-revoke-token.body"
      fi

      run_curl "10-post-revoke-verify-get" "GET" "access" "/api/v1/credentials/$CREATED_CREDENTIAL_ID"
      POST_REVOKE_STATUS="$(meta_value "$RAW_DIR/10-post-revoke-verify-get.meta.txt" "http_status")"

      run_curl "11-delete-credential" "DELETE" "session" "/api/v1/credentials/$CREATED_CREDENTIAL_ID"
      delete_status="$(meta_value "$RAW_DIR/11-delete-credential.meta.txt" "http_status")"
      if [[ "$delete_status" == "200" ]]; then
        CLEANUP_CREDENTIAL_STATUS="deleted"
      else
        CLEANUP_CREDENTIAL_STATUS="delete_failed"
      fi

      if [[ "$NEGATIVE_MISSING_IDS_STATUS" != "400" ]]; then
        CLAIM_RESULT="failed"
        FAILURE_CLASS='"product"'
        register_failure "negative_missing_credential_ids" "$NEGATIVE_MISSING_IDS_STATUS" "$RAW_DIR/07-negative-missing-credential-ids.body"
      fi

      if [[ "$NEGATIVE_INVALID_SCOPE_STATUS" != "400" ]]; then
        CLAIM_RESULT="failed"
        FAILURE_CLASS='"product"'
        register_failure "negative_invalid_scope" "$NEGATIVE_INVALID_SCOPE_STATUS" "$RAW_DIR/08-negative-invalid-scope.body"
      fi

      if [[ "$POST_REVOKE_STATUS" == "200" ]]; then
        CLAIM_RESULT="failed"
        FAILURE_CLASS='"product"'
        register_failure "post_revoke_token_use" "$POST_REVOKE_STATUS" "$RAW_DIR/10-post-revoke-verify-get.body"
      fi
    fi
  fi
fi

if [[ "$CLAIM_RESULT" == "passed" ]]; then
  FAILURE_CLASS="null"
fi

auth_me_body_file="$RAW_DIR/00-auth-me.body"
if [[ "$REBUILT_SESSION" == "true" ]]; then
  auth_me_body_file="$RAW_DIR/00c-auth-me-after-rebuild.body"
fi

cat > "$NOTES_FILE" <<EOF
# case08 Token API chain notes

- Run ID: \`$RUN_ID\`
- Case: \`$CASE_ID\`
- Prefix: \`$PREFIX\`
- Environment: \`$BASE_URL/\`
- Session source: \`$SESSION_FILE\`
- Session rebuilt via /auth/session: \`$REBUILT_SESSION\`

## Claims

1. Session-authenticated owner can create a credential-bound token through \`POST /api/v1/tokens\`.
2. Token metadata is observable through \`GET /api/v1/tokens\` and \`GET /api/v1/tokens/:id\`.
3. Issued token can read credential metadata for its bound credential and should not expand beyond its allowed \`credential_ids\`.
4. Missing \`credential_ids\` and unsupported scopes are rejected.
5. Revoked token stops working.

## Execution summary

- \`GET /api/v1/auth/me\`: HTTP \`$AUTH_ME_STATUS\`
- \`POST /api/v1/credentials\`: credential_id \`$CREATED_CREDENTIAL_ID\`
- \`POST /api/v1/tokens\`: token_id \`$CREATED_TOKEN_ID\`
- Verify get credential with issued token: HTTP \`$VERIFY_GET_STATUS\`
- Verify list credentials with issued token: HTTP \`$VERIFY_LIST_STATUS\`, total=\`${VERIFY_LIST_TOTAL:-}\`, foreign_ids=\`${VERIFY_LIST_FOREIGN_IDS:-[]}\`
- Negative missing credential_ids: HTTP \`$NEGATIVE_MISSING_IDS_STATUS\`
- Negative invalid scope: HTTP \`$NEGATIVE_INVALID_SCOPE_STATUS\`
- Revoke token cleanup: \`$CLEANUP_TOKEN_STATUS\`
- Delete credential cleanup: \`$CLEANUP_CREDENTIAL_STATUS\`
- Post-revoke credential read: HTTP \`$POST_REVOKE_STATUS\`

## First unexpected failure

- Step: \`${FIRST_FAILURE_STEP:-none}\`
- HTTP status: \`${FIRST_FAILURE_STATUS:-none}\`
- Body preview: \`${FIRST_FAILURE_BODY:-none}\`

## Evidence

- Commands: \`$COMMANDS_FILE\`
- Raw responses: \`$RAW_DIR/\`
- Result: \`$RESULT_FILE\`
EOF

cat > "$RESULT_FILE" <<EOF
{
  "run_id": "$RUN_ID",
  "case_id": "$CASE_ID",
  "status": "completed",
  "claim_result": "$CLAIM_RESULT",
  "failure_class": $FAILURE_CLASS,
  "environment": "$BASE_URL/",
  "session_rebuilt": $REBUILT_SESSION,
  "created_credential_id": $(json_escape "$CREATED_CREDENTIAL_ID"),
  "created_token_id": $(json_escape "$CREATED_TOKEN_ID"),
  "cleanup_status": {
    "token": $(json_escape "$CLEANUP_TOKEN_STATUS"),
    "credential": $(json_escape "$CLEANUP_CREDENTIAL_STATUS")
  },
  "http_statuses": {
    "auth_me": $(json_escape "$AUTH_ME_STATUS"),
    "verify_get_credential": $(json_escape "$VERIFY_GET_STATUS"),
    "verify_list_credentials": $(json_escape "$VERIFY_LIST_STATUS"),
    "negative_missing_credential_ids": $(json_escape "$NEGATIVE_MISSING_IDS_STATUS"),
    "negative_invalid_scope": $(json_escape "$NEGATIVE_INVALID_SCOPE_STATUS"),
    "post_revoke_verify_get": $(json_escape "$POST_REVOKE_STATUS")
  },
  "verify_observation": {
    "listed_total": $(json_escape "$VERIFY_LIST_TOTAL"),
    "foreign_credential_ids": ${VERIFY_LIST_FOREIGN_IDS:-[]}
  },
  "first_unexpected_failure": {
    "step": $(json_escape "$FIRST_FAILURE_STEP"),
    "http_status": $(json_escape "$FIRST_FAILURE_STATUS"),
    "body_preview": $(json_escape "$FIRST_FAILURE_BODY")
  },
  "evidence_paths": {
    "notes": "$NOTES_FILE",
    "commands": "$COMMANDS_FILE",
    "raw_dir": "$RAW_DIR"
  }
}
EOF

{
  printf '%s | %s | %s | %s | %s\n' "$CASE_ID" "token" "${CREATED_TOKEN_ID:-none}" "$CLEANUP_TOKEN_STATUS" "prefix=$PREFIX"
  printf '%s | %s | %s | %s | %s\n' "$CASE_ID" "credential" "${CREATED_CREDENTIAL_ID:-none}" "$CLEANUP_CREDENTIAL_STATUS" "service_id=${CREATED_CREDENTIAL_SERVICE_ID:-none}"
} >> "$RESIDUAL_FILE"

printf 'case08 complete: claim_result=%s failure_class=%s token_id=%s credential_id=%s\n' "$CLAIM_RESULT" "$FAILURE_CLASS" "$CREATED_TOKEN_ID" "$CREATED_CREDENTIAL_ID"
