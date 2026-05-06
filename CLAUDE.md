# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Pre-Commit Requirements (Mandatory)

Before every commit, ALL of the following must pass with zero errors and zero warnings:

```bash
# 1. Format Rust code (mandatory, no exceptions)
cargo fmt

# 2. Lint — must produce NO warnings and NO errors (includes test code)
cargo clippy --tests -- -D warnings

# 3. All unit tests must pass
cargo test
```

**These are hard gates. Do not commit if any of these fail.**

---

## Development Commands

### Backend (Rust)

```bash
# Build
cargo build
cargo build --release

# Run server (dev mode)
RUST_LOG=debug cargo run

# Run all tests
cargo test

# Run a single test by name
cargo test <test_name>

# Run a specific test file
cargo test --test credentials_api_tests

# Run with feature flags
cargo test --features rls-tests

# Format code
cargo fmt

# Lint
cargo clippy

# Hardware compile check (Linux SGX runners)
cargo test --features tee-hardware --lib --tests --no-run
```

### Frontend (React + Vite)

```bash
cd frontend

# Dev server (http://localhost:5173)
npm run dev

# Build for production
npm run build

# Run unit tests
npm run test:unit

# Lint
npm run lint
```

### Full Stack (Docker)

```bash
# Start all services (Postgres, Redis, immudb, Vault, app)
docker compose -f docker/docker-compose.yml up

# Prod stack
docker compose -f docker/docker-compose.prod.yml up
```

---

## Architecture

CredBridge is a zero-trust credential vault with hardware-level security via Intel SGX TEE.

### Key Architecture Decisions

**Four-Layer Key Hierarchy:**

```
L0: SGX Sealing Key (hardware root)
 └─ L1: Enclave Master Key
     └─ L2: User Vault Key (per-user, 5min TTL cache)
         └─ L3: Credential Encryption Key (per-credential)
             └─ AES-256-GCM encrypted credential
```

**Multi-tenancy:** Schema-per-tenant + PostgreSQL Row Level Security (RLS). RLS policies are applied at the database layer.

**Token auth:** PASETO v4.local (not JWT). See `src/token/`.

**TEE modes:** `TEE_MODE=simulation` for dev, `TEE_MODE=hardware` for production SGX hardware. Hardware mode requires Intel SGX-capable CPU.

**Storage backend selection** (via `CREDBRIDGE_STORAGE_BACKEND`):

- `auto` (default) — prefers Postgres if `DATABASE_URL` is set, then Vault if `VAULT_ADDR`+`VAULT_TOKEN` are set, otherwise fails
- `postgres` — PostgreSQL backend
- `vault` — HashiCorp Vault backend

**API base path:** All API routes are mounted at `/api/v1`.

### Runtime Storage Policy

| Domain                 | Backend             | Config                       |
| ---------------------- | ------------------- | ---------------------------- |
| vault                  | PostgreSQL or Vault | `CREDBRIDGE_STORAGE_BACKEND` |
| auth                   | PostgreSQL          | `DATABASE_URL`               |
| tenant config          | PostgreSQL          | `DATABASE_URL`               |
| sandbox records        | PostgreSQL          | `DATABASE_URL`               |
| audit                  | immudb              | `IMMUDB_*` (default)         |
| token state            | Redis               | `REDIS_URL`                  |
| rate limit state       | Redis               | `REDIS_URL`                  |
| attestation challenges | Redis               | `REDIS_URL`                  |

Production startup fails if required durable backends are missing. For local development, memory fallback can be enabled via `CREDBRIDGE_*_ALLOW_MEMORY_FALLBACK=true` variables.

### Key Environment Variables

| Variable                      | Default       | Description                               |
| ----------------------------- | ------------- | ----------------------------------------- |
| `CREDBRIDGE_PORT`             | `8080`        | HTTP server port                          |
| `CREDBRIDGE_HOST`             | `0.0.0.0`     | HTTP server host                          |
| `CREDBRIDGE_ENV`              | `development` | `development` or `production`             |
| `TEE_MODE`                    | `hardware`    | `simulation` or `hardware`                |
| `CREDBRIDGE_STORAGE_BACKEND`  | `auto`        | `auto`, `postgres`, `vault`               |
| `DATABASE_URL`                | —             | PostgreSQL connection string              |
| `REDIS_URL`                   | —             | Redis connection string                   |
| `VAULT_ADDR` / `VAULT_TOKEN`  | —             | HashiCorp Vault connection                |
| `IMMUDB_HOST` / `IMMUDB_PORT` | —             | immudb connection                         |
| `CREDBRIDGE_ALLOWED_ORIGINS`  | —             | Comma-separated CORS origins (production) |
| `RUST_LOG`                    | `info`        | Log level                                 |

### Backend Structure (`src/`)

| Module       | Purpose                                                                                                             |
| ------------ | ------------------------------------------------------------------------------------------------------------------- |
| `api/`       | Axum HTTP routes and middleware — credentials, attestation, audit, auth, sandbox, tenant, i18n                      |
| `tee/`       | TEE enclave lifecycle, keys, sealing, DCAP attestation, sandbox execution (nsjail + seccomp + cgroups + namespaces) |
| `crypto/`    | HKDF key derivation, AES-GCM encryption, key structures, constant-time comparison                                   |
| `vault/`     | Credential storage: `CredentialVault` abstraction over pluggable backends (memory, Postgres, HashiCorp Vault)       |
| `token/`     | PASETO token generation/validation, Redis session store, token revocation                                           |
| `services/`  | Business logic: `db/` (connection pool, schema), `llm/` (multi-provider AI: OpenAI, Azure, Claude)                  |
| `audit/`     | Immutable audit log via immudb + in-memory fallback                                                                 |
| `models/`    | Shared data models                                                                                                  |
| `tenant/`    | Multi-tenant isolation logic, tenant config store                                                                   |
| `connector/` | External system connectors, HTTP connector, registry                                                                |
| `mcp/`       | Model Context Protocol server integration                                                                           |
| `bin/`       | Additional binary entry points (`generate_test_token`, `db-verify`)                                                 |

The Rust crate is named `vault-service` (`vault_service` when used as a library import).

### Frontend Structure (`frontend/src/`)

React 19 + TypeScript + Vite + Tailwind CSS + shadcn/ui.

The frontend uses a **feature-based** folder structure:

```
app/                    — router, layout, providers, route wrappers, NotFound
components/ui/          — reusable shadcn/ui components
components/zkme/        — branded page shells and shared presentation building blocks
features/auth/          — LoginPage, OnboardingPage, InvitationAcceptPage, ProfilePage
features/credentials/   — CredentialsPage and related helpers/contracts
features/tokens/        — TokensPage and verification helpers
features/developer/     — DeveloperCenter and API tester helpers
features/audit/         — audit page modules kept in code, not currently routed
features/tenants/       — tenants/settings/users page modules kept in code, not currently routed
shared/                 — api, auth/session helpers, config, i18n, Zustand stores
hooks/                  — shared hooks such as toast helpers
```

Current routed surface from `frontend/src/app/router.tsx`:

- Public: `/login`, `/invitation/accept`
- Protected: `/credentials`, `/tokens`, `/developer`, `/onboarding`
- Redirects to `/credentials`: `/audit`, `/tenants`, `/settings`, `/users`, `/profile`

All routed pages are lazy-loaded via `React.lazy`; auth gating is handled by `ProtectedRoute` and `PublicRoute`.

### API Structure

Key endpoints (all under `/api/v1`):

| Endpoint                        | Purpose                 | Scope                |
| ------------------------------- | ----------------------- | -------------------- |
| `POST /credentials`             | Create credential       | `credential:write`   |
| `GET /credentials`              | List credentials        | `credential:read`    |
| `GET /credentials/:id`          | Get credential metadata | `credential:read`    |
| `POST /credentials/:id/decrypt` | Decrypt credential      | `credential:decrypt` |
| `GET /audit/logs`               | Query audit logs        | `audit:read`         |
| `POST /audit/export`            | Export audit logs       | `audit:read`         |

See `API.md` for complete API documentation.

### External Services (required for full operation)

- **PostgreSQL** — Primary database (credentials, audit, tenant data)
- **Redis** — Session/token store, rate limiting, attestation challenges
- **immudb** — Immutable audit log with cryptographic verification
- **HashiCorp Vault** — Optional secrets management backend

See `docker/docker-compose.yml` for default connection settings and env vars.

### Integration Tests

Tests in `tests/` use `[[test]]` entries in `Cargo.toml`. Key test files:

| Test                      | Path                                                      |
| ------------------------- | --------------------------------------------------------- |
| `paseto_tests`            | `tests/token/paseto_tests.rs`                             |
| `redis_store_tests`       | `tests/token/redis_store_tests.rs`                        |
| `audit_api_tests`         | `tests/api/audit_tests.rs`                                |
| `tenant_middleware_tests` | `tests/api/tenant_middleware_tests.rs`                    |
| `rls_integration`         | `tests/rls_integration.rs` (requires `rls-tests` feature) |
| `sandbox_export_tests`    | `tests/tee/sandbox_export_tests.rs`                       |
| `dcap_tests`              | `tests/tee/dcap_tests.rs`                                 |
| `sgx_hardware_tests`      | `tests/` (requires SGX hardware)                          |

### SDKs

- `sdk-typescript/` — TypeScript client SDK + Sandbox WebSocket client
- `sdk-rust/` — Rust client SDK
- `cli/` — CLI management tool for operators and automation

---

## Code Standards

### Rust

- Use `thiserror` for structured error handling
- Use `anyhow` for application-level errors with `context()`
- Async functions use native `async fn` or `async-trait`
- Database queries use SQLx with compile-time checking
- Sensitive data uses `zeroize` for secure clearing
- Constant-time comparison for secrets (`constant_time_eq`)
- Never use `unwrap()` or `expect()` in production code
- Naming: modules/functions/variables use `snake_case`, types use `PascalCase`, constants use `SCREAMING_SNAKE_CASE`

### React/TypeScript

- TypeScript strict mode enabled
- Function components with Hooks
- Zustand for state management
- TanStack Query for data fetching
- shadcn/ui + Radix UI for components
- Tailwind CSS for styling
- Naming: Components use `PascalCase`, hooks use `camelCase` starting with `use`, props interfaces use `[ComponentName]Props`

---

## Security Considerations

1. **Credential Management**: This is a credential vault — all changes must consider security impact
2. **TEE Environment**: Code runs in trusted execution environment with special restrictions
3. **Encryption**: AES-256-GCM for credential encryption, HKDF for key derivation
4. **Token Handling**: PASETO (not JWT) for authentication tokens
5. **Audit Logging**: All sensitive operations logged to immutable immudb store
6. **Sandbox**: Credential-consuming operations run in isolated nsjail sandbox with seccomp, cgroups, namespaces
7. **Input Validation**: Validate all inputs; use parameterized queries

---

## Additional Documentation

- `README.md` — Project overview and CLI usage guide
- `API.md` — Complete REST API documentation
- `docs/README.md` — Documentation hub
- `IMMUDB_SETUP.md` — immudb setup instructions
- `INTEL_SGX_DEPLOYMENT_REQUIREMENTS.md` — SGX deployment guide
- `sdk-rust/README.md` — Rust SDK documentation
- `sdk-typescript/README.md` — TypeScript SDK documentation
- `cli/README.md` — CLI documentation
- `.claude/rules/rust-coding-standards.md` — Detailed Rust coding standards
- `.claude/rules/react-coding-standards.md` — Detailed React coding standards
