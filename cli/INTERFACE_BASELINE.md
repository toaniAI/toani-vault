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

- `POST /sandbox/sessions`
- `GET /sandbox/sessions`
- `GET /sandbox/sessions/:id`
- `POST /sandbox/sessions/:id/execute`
- `GET /sandbox/operations/:operation_id`
- `POST /sandbox/sessions/:id/pause`
- `POST /sandbox/sessions/:id/resume`
- `DELETE /sandbox/sessions/:id`
- `POST /sandbox/sessions/:id/screenshot`
- `POST /sandbox/sessions/:id/export`
- `GET /sandbox/stats`

### Audit

- `GET /audit/logs`
- `GET /audit/logs/:id`
- `POST /audit/export`
- `POST /audit/verify`

## 2) SDK Coverage Snapshot (Before Migration)

### Implemented in `sdk-typescript`

- Credentials: create/list/get/decrypt/delete
- Tokens: verify/revoke
- Sandbox: create/list/get/execute/pause/resume/close/export/dom-export

### Missing Before Migration

- Auth service: `session/me/logout/memberships`
- Audit service: `logs/export/verify`
- Token service: `create/stats`
- Sandbox service: `get operation` and `stats`

## 3) Current npm CLI Coverage Snapshot

### Command groups

- `login`
- `doctor`
- `config`
- `credentials`
- `sandbox`

### Known drift

- CLI surface drift:
  - Current npm CLI exposes `login`, `doctor`, `config`, read-only `credentials` (`list`, `get`), `sandbox`, plus global flags/version
  - Historical docs still referenced broader unshipped `auth`, mutating `credentials`, `tokens`, and `audit` groups
- Sandbox terminate naming drift:
  - Real close-session route is `DELETE /sandbox/sessions/:id`
  - Some historical constants still referenced `/sandbox/sessions/:id/terminate`
  - Screenshot is not exposed by the current backend sandbox router
- Auth status/login behavior drift:
  - Current `login` / `doctor` validation is based on `GET /sandbox/stats`, not `auth/session` or `auth/me`

## 4) Migration Rules

- Do not trust old route constants or old CLI behavior over backend mounted routes.
- CLI adapter layer should call SDK services, not duplicate raw HTTP logic.
- Keep command names aligned to `toani` branding and npm package `@toani/vault-cli`.
