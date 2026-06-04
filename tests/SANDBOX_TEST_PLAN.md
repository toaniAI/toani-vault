# Sandbox Test Plan (Broker-Only)

## 1. Scope

This plan validates the current public sandbox contract after Slice 4 retirement.

Supported endpoints:

- `POST /api/v1/sandbox/http-requests`
- `GET /api/v1/sandbox/http-requests/{operation_id}`

Out of scope (retired and must not be treated as success path):

- legacy session lifecycle APIs and commands

## 2. Preconditions

- Backend service is running.
- A valid bearer token is available.
- Sandbox runtime dependencies are healthy (nsjail/TEE prerequisites).

## 3. Contract Cases

### TC-API-001 Submit Broker Request

Request:

```http
POST /api/v1/sandbox/http-requests
Authorization: Bearer <token>
Content-Type: application/json
```

Body:

```json
{
  "operation_type": "http_request",
  "description": "broker smoke",
  "parameters": {
    "method": "GET",
    "url": "https://api.example.com/health"
  }
}
```

Expectations:

- HTTP 200
- `success=true`
- response includes `operation_id`

### TC-API-002 Query Broker Request Detail

Request:

```http
GET /api/v1/sandbox/http-requests/{operation_id}
Authorization: Bearer <token>
```

Expectations:

- HTTP 200
- response includes `operation_id`, `operation_type`, `status`

### TC-API-003 Polling Final State

Steps:

1. Submit request with TC-API-001.
2. Poll TC-API-002 every 1-2 seconds.

Expectations:

- status transitions are observable
- final status is `completed` or `failed`

### TC-API-004 Bad Request Semantics

Steps:

1. Submit request without `operation_type`.

Expectations:

- HTTP 400
- `success=false`
- error message is present

### TC-API-005 Not Found Semantics

Steps:

1. Query unknown `operation_id`.

Expectations:

- HTTP 404
- `success=false`

## 4. CLI Contract Cases

### TC-CLI-001 Broker Commands Available

Command:

```bash
toani-vault sandbox --help
```

Expectations:

- includes `request`
- includes `get-request`
- does not document retired session lifecycle commands

### TC-CLI-002 Skill Contract Consistency

Command:

```bash
cd cli && npm test -- tests/skill.test.ts
```

Expectations:

- passes
- assertions align with broker-only skill text

## 5. Smoke Script

Command:

```bash
scripts/sandbox-minimal-navigate.sh
```

Expectations:

- submits broker request through `/api/v1/sandbox/http-requests`
- polls `/api/v1/sandbox/http-requests/{operation_id}`
- exits non-zero on failed/timeout states

## 6. Exit Criteria

- All contract cases pass.
- No active test asset treats retired session lifecycle paths as success criteria.
- Evidence bundle records request/response payloads and command logs.
