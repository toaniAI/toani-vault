# Toani CLI Interface Baseline (Source of Truth)

This document captures the API baseline for the npm CLI migration.
Source of truth is backend Axum route mounting in:

- `/Users/yvan/AIWorkspace/credbridge/src/main.rs`
- `/Users/yvan/AIWorkspace/credbridge/src/api/*.rs`

API base path: `/api/v1`.

## 1) Backend Truth (Axum)

### Auth

- `POST /auth/session`
- `GET /auth/me`
- `GET /auth/memberships`
- `POST /auth/logout`

### Credentials

- `POST /credentials`
- `GET /credentials`
- `GET /credentials/:id`
- `PUT /credentials/:id`
- `POST /credentials/:id/decrypt`
- `DELETE /credentials/:id`
- `GET /credentials/:id/versions`
- `GET /credentials/:id/versions/:version`
- `POST /credentials/:id/rollback`

### Tokens

- `POST /tokens`
- `GET /tokens`
- `GET /tokens/:token_id`
- `GET /tokens/stats`
- `POST /tokens/:token_id/revoke`

### Sandbox

- `POST /sandbox/http-requests`
- `GET /sandbox/http-requests/:operation_id`

### Approvals

- `POST /approvals`
- `GET /approvals/:approval_id`
- `POST /approvals/:approval_id/approve`
- `POST /approvals/:approval_id/reject`
- `POST /approvals/:approval_id/cancel`

### Audit

- `GET /audit/logs`
- `GET /audit/logs/:id`
- `POST /audit/export`
- `POST /audit/verify`

## 2) SDK Coverage Snapshot (Before Migration)

### Implemented in `sdk-typescript`

- Credentials: create/list/get/decrypt/delete
- Tokens: verify/revoke
- Sandbox: request/get-request (broker-only)
- Approvals: create/get

### Missing Before Migration

- Auth service: `session/me/logout/memberships`
- Audit service: `logs/export/verify`
- Token service: `create/stats`
- Sandbox service: legacy session lifecycle methods retired in Slice 4

## 3) Current npm CLI Coverage Snapshot

### Command groups

- `login`
- `doctor`
- `config`
- `credentials`
- `approvals`
- `sandbox`

### Known drift

- CLI surface drift:
  - Current npm CLI exposes `login`, `doctor`, `config`, read-only `credentials` (`list`, `get`), `sandbox`, plus global flags/version
  - Historical docs still referenced broader unshipped `auth`, mutating `credentials`, `tokens`, and `audit` groups
- Sandbox lifecycle drift:
  - Legacy session-lifecycle and stats sandbox APIs are retired (`410 Gone`)
  - Current CLI/SDK only use broker routes: `POST /sandbox/http-requests` and `GET /sandbox/http-requests/:operation_id`
- Auth status/login behavior drift:
  - Current `login` / `doctor` validation is based on `GET /auth/me`

## 4) Migration Rules

- Do not trust old route constants or old CLI behavior over backend mounted routes.
- CLI adapter layer should call SDK services, not duplicate raw HTTP logic.
- Keep command names aligned to `toani-vault` branding and npm package `@toani/vault-cli`.
