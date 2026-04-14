#!/usr/bin/env bash
set -euo pipefail

OUT="/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/api-auth"
RAW="$OUT/raw"
BASE="https://dev-credbridge.bitkinetic.com"

FIRST_UNEXPECTED=""
LAST_STATUS=""

log_command() {
  printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$1" >> "$OUT/commands.txt"
}

record_unexpected_failure() {
  local label="$1"
  if [[ -n "$FIRST_UNEXPECTED" ]]; then
    return
  fi
  FIRST_UNEXPECTED="$label"
  cp "$RAW/${label}.headers.txt" "$RAW/first-unexpected-failure.headers.txt" 2>/dev/null || true
  cp "$RAW/${label}.body.json" "$RAW/first-unexpected-failure.body.json" 2>/dev/null || true
  cp "$RAW/${label}.body.txt" "$RAW/first-unexpected-failure.body.txt" 2>/dev/null || true
  cp "$RAW/${label}.curl-meta.txt" "$RAW/first-unexpected-failure.curl-meta.txt" 2>/dev/null || true
  printf '%s\n' "$label" > "$RAW/first-unexpected-failure.label.txt"
}

run_request() {
  local label="$1"
  local method="$2"
  local url="$3"
  local auth_kind="$4"
  local body_file="${5:-}"
  local expected_status="$6"
  local log_cmd="$7"

  local body_tmp="$RAW/${label}.body.tmp"
  local headers_out="$RAW/${label}.headers.txt"
  local meta_out="$RAW/${label}.curl-meta.txt"
  local json_out="$RAW/${label}.body.json"
  local text_out="$RAW/${label}.body.txt"

  local -a curl_args
  curl_args=(-sS -X "$method" "$url" -H "accept: application/json")

  if [[ -n "$body_file" ]]; then
    curl_args+=(-H "content-type: application/json" --data @"$body_file")
  fi

  case "$auth_kind" in
    none) ;;
    session) curl_args+=(-H "Authorization: Bearer $SESSION_TOKEN") ;;
    invalid) curl_args+=(-H "Authorization: Bearer invalid-case06-token") ;;
    access) curl_args+=(-H "Authorization: Bearer $ACCESS_TOKEN") ;;
    *)
      echo "unknown auth kind: $auth_kind" >&2
      exit 2
      ;;
  esac

  log_command "$log_cmd"
  curl "${curl_args[@]}" \
    -D "$headers_out" \
    -o "$body_tmp" \
    -w 'http_code=%{http_code}\ncontent_type=%{content_type}\nurl_effective=%{url_effective}\nremote_ip=%{remote_ip}\ntime_total=%{time_total}\n' \
    > "$meta_out"

  if python3 - <<PY >/dev/null 2>&1
import json
json.load(open("$body_tmp"))
PY
  then
    mv "$body_tmp" "$json_out"
  else
    mv "$body_tmp" "$text_out"
  fi

  LAST_STATUS="$(awk -F= '/^http_code=/{print $2}' "$meta_out")"
  if [[ "$expected_status" != "any" && "$LAST_STATUS" != "$expected_status" ]]; then
    record_unexpected_failure "$label"
  fi
}

PRIVY_TOKEN="$(cat "$RAW/fresh_privy_token.txt")"

cat > "$RAW/01_auth_session.request.json" <<JSON
{"privy_access_token":"$PRIVY_TOKEN"}
JSON
run_request \
  "01_auth_session" \
  "POST" \
  "$BASE/api/v1/auth/session" \
  "none" \
  "$RAW/01_auth_session.request.json" \
  "200" \
  "curl -sS -X POST '$BASE/api/v1/auth/session' -H 'accept: application/json' -H 'content-type: application/json' --data @'$RAW/01_auth_session.request.json' -D '$RAW/01_auth_session.headers.txt' -o '$RAW/01_auth_session.body.json' -w 'http_code=%{http_code} content_type=%{content_type} url_effective=%{url_effective} remote_ip=%{remote_ip} time_total=%{time_total}'"

SESSION_TOKEN="$(python3 - <<PY
import json
print(json.load(open("$RAW/01_auth_session.body.json"))["session"]["session_token"])
PY
)"
printf '%s' "$SESSION_TOKEN" > "$RAW/01_auth_session.session_token.txt"

run_request \
  "02_auth_me" \
  "GET" \
  "$BASE/api/v1/auth/me" \
  "session" \
  "" \
  "200" \
  "SESSION_TOKEN=\$(cat '$RAW/01_auth_session.session_token.txt'); curl -sS -X GET '$BASE/api/v1/auth/me' -H 'accept: application/json' -H 'Authorization: Bearer \$SESSION_TOKEN' -D '$RAW/02_auth_me.headers.txt' -o '$RAW/02_auth_me.body.json' -w 'http_code=%{http_code} content_type=%{content_type} url_effective=%{url_effective} remote_ip=%{remote_ip} time_total=%{time_total}'"

run_request \
  "03_auth_memberships" \
  "GET" \
  "$BASE/api/v1/auth/memberships" \
  "session" \
  "" \
  "200" \
  "SESSION_TOKEN=\$(cat '$RAW/01_auth_session.session_token.txt'); curl -sS -X GET '$BASE/api/v1/auth/memberships' -H 'accept: application/json' -H 'Authorization: Bearer \$SESSION_TOKEN' -D '$RAW/03_auth_memberships.headers.txt' -o '$RAW/03_auth_memberships.body.json' -w 'http_code=%{http_code} content_type=%{content_type} url_effective=%{url_effective} remote_ip=%{remote_ip} time_total=%{time_total}'"

run_request \
  "04_invalid_token_auth_me" \
  "GET" \
  "$BASE/api/v1/auth/me" \
  "invalid" \
  "" \
  "401" \
  "curl -sS -X GET '$BASE/api/v1/auth/me' -H 'accept: application/json' -H 'Authorization: Bearer invalid-case06-token' -D '$RAW/04_invalid_token_auth_me.headers.txt' -o '$RAW/04_invalid_token_auth_me.body.json' -w 'http_code=%{http_code} content_type=%{content_type} url_effective=%{url_effective} remote_ip=%{remote_ip} time_total=%{time_total}'"

cat > "$RAW/05_missing_param_auth_session.request.json" <<JSON
{}
JSON
run_request \
  "05_missing_param_auth_session" \
  "POST" \
  "$BASE/api/v1/auth/session" \
  "none" \
  "$RAW/05_missing_param_auth_session.request.json" \
  "400" \
  "curl -sS -X POST '$BASE/api/v1/auth/session' -H 'accept: application/json' -H 'content-type: application/json' --data @'$RAW/05_missing_param_auth_session.request.json' -D '$RAW/05_missing_param_auth_session.headers.txt' -o '$RAW/05_missing_param_auth_session.body.json' -w 'http_code=%{http_code} content_type=%{content_type} url_effective=%{url_effective} remote_ip=%{remote_ip} time_total=%{time_total}'"

cat > "$RAW/06_access_token_create.request.json" <<JSON
{"scopes":["tokens:read"],"ttl_seconds":300}
JSON
run_request \
  "06_access_token_create" \
  "POST" \
  "$BASE/api/v1/auth/access-token" \
  "session" \
  "$RAW/06_access_token_create.request.json" \
  "200" \
  "SESSION_TOKEN=\$(cat '$RAW/01_auth_session.session_token.txt'); curl -sS -X POST '$BASE/api/v1/auth/access-token' -H 'accept: application/json' -H 'content-type: application/json' -H 'Authorization: Bearer \$SESSION_TOKEN' --data @'$RAW/06_access_token_create.request.json' -D '$RAW/06_access_token_create.headers.txt' -o '$RAW/06_access_token_create.body.json' -w 'http_code=%{http_code} content_type=%{content_type} url_effective=%{url_effective} remote_ip=%{remote_ip} time_total=%{time_total}'"

CURRENT_EQ_CREDENTIAL_ID="019d868b-272a-7333-92b7-b21d5a3241b5"
cat > "$RAW/07_access_token_create_current_equivalent.request.json" <<JSON
{"scopes":["credential:read"],"credential_ids":["$CURRENT_EQ_CREDENTIAL_ID"],"ttl_seconds":300}
JSON

run_request \
  "07_access_token_create_current_equivalent" \
  "POST" \
  "$BASE/api/v1/auth/access-token" \
  "session" \
  "$RAW/07_access_token_create_current_equivalent.request.json" \
  "200" \
  "SESSION_TOKEN=\$(cat '$RAW/01_auth_session.session_token.txt'); curl -sS -X POST '$BASE/api/v1/auth/access-token' -H 'accept: application/json' -H 'content-type: application/json' -H 'Authorization: Bearer \$SESSION_TOKEN' --data @'$RAW/07_access_token_create_current_equivalent.request.json' -D '$RAW/07_access_token_create_current_equivalent.headers.txt' -o '$RAW/07_access_token_create_current_equivalent.body.json' -w 'http_code=%{http_code} content_type=%{content_type} url_effective=%{url_effective} remote_ip=%{remote_ip} time_total=%{time_total}'"

ACCESS_TOKEN="$(python3 - <<PY
import json
print(json.load(open("$RAW/07_access_token_create_current_equivalent.body.json"))["data"]["access_token"])
PY
)"
printf '%s' "$ACCESS_TOKEN" > "$RAW/07_access_token_create_current_equivalent.access_token.txt"

run_request \
  "08_current_equivalent_token_auth_me" \
  "GET" \
  "$BASE/api/v1/auth/me" \
  "access" \
  "" \
  "any" \
  "ACCESS_TOKEN=\$(cat '$RAW/07_access_token_create_current_equivalent.access_token.txt'); curl -sS -X GET '$BASE/api/v1/auth/me' -H 'accept: application/json' -H 'Authorization: Bearer \$ACCESS_TOKEN' -D '$RAW/08_current_equivalent_token_auth_me.headers.txt' -o '$RAW/08_current_equivalent_token_auth_me.body.json' -w 'http_code=%{http_code} content_type=%{content_type} url_effective=%{url_effective} remote_ip=%{remote_ip} time_total=%{time_total}'"

run_request \
  "09_current_equivalent_token_access_token_retry" \
  "POST" \
  "$BASE/api/v1/auth/access-token" \
  "access" \
  "$RAW/07_access_token_create_current_equivalent.request.json" \
  "403" \
  "ACCESS_TOKEN=\$(cat '$RAW/07_access_token_create_current_equivalent.access_token.txt'); curl -sS -X POST '$BASE/api/v1/auth/access-token' -H 'accept: application/json' -H 'content-type: application/json' -H 'Authorization: Bearer \$ACCESS_TOKEN' --data @'$RAW/07_access_token_create_current_equivalent.request.json' -D '$RAW/09_current_equivalent_token_access_token_retry.headers.txt' -o '$RAW/09_current_equivalent_token_access_token_retry.body.json' -w 'http_code=%{http_code} content_type=%{content_type} url_effective=%{url_effective} remote_ip=%{remote_ip} time_total=%{time_total}'"

python3 - <<PY > "$RAW/execution-summary.json"
import json
from pathlib import Path

raw = Path("$RAW")
labels = [
    "01_auth_session",
    "02_auth_me",
    "03_auth_memberships",
    "04_invalid_token_auth_me",
    "05_missing_param_auth_session",
    "06_access_token_create",
    "07_access_token_create_current_equivalent",
    "08_current_equivalent_token_auth_me",
    "09_current_equivalent_token_access_token_retry",
]
summary = []
for label in labels:
    meta = {}
    for line in raw.joinpath(f"{label}.curl-meta.txt").read_text().splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            meta[key] = value
    body_json = raw / f"{label}.body.json"
    body_txt = raw / f"{label}.body.txt"
    sample = None
    if body_json.exists():
        try:
            sample = json.loads(body_json.read_text())
        except Exception:
            sample = body_json.read_text()[:1200]
    elif body_txt.exists():
        sample = body_txt.read_text()[:1200]
    summary.append({"label": label, "meta": meta, "sample": sample})
raw.joinpath("execution-summary.json").write_text(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))
PY

if [[ -n "$FIRST_UNEXPECTED" ]]; then
  printf '%s\n' "$FIRST_UNEXPECTED" > "$RAW/first-unexpected-failure.runtime.txt"
fi
