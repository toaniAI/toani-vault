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
```

### Frontend (React + Vite)

```bash
cd frontend

# Dev server (http://localhost:5173)
npm run dev

# Build for production
npm run build

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
- `memory` — in-process only, for dev/test
- `postgres` — PostgreSQL backend
- `vault` — HashiCorp Vault backend

**API base path:** All API routes are mounted at `/api/v1`.

### Key Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CREDBRIDGE_PORT` | `8080` | HTTP server port |
| `CREDBRIDGE_HOST` | `0.0.0.0` | HTTP server host |
| `CREDBRIDGE_ENV` | `development` | `development` or `production` |
| `TEE_MODE` | `hardware` | `simulation` or `hardware` |
| `CREDBRIDGE_STORAGE_BACKEND` | `auto` | `auto`, `memory`, `postgres`, `vault` |
| `DATABASE_URL` | — | PostgreSQL connection string |
| `VAULT_ADDR` / `VAULT_TOKEN` | — | HashiCorp Vault connection |
| `CREDBRIDGE_ALLOWED_ORIGINS` | — | Comma-separated CORS origins (production) |
| `RUST_LOG` | `info` | Log level |

### Backend Structure (`src/`)

| Module | Purpose |
|--------|---------|
| `api/` | Axum HTTP routes and middleware — credentials, attestation, audit, auth, sandbox, tenant, i18n |
| `tee/` | TEE enclave lifecycle, keys, sealing, DCAP attestation, sandbox execution (nsjail + seccomp + cgroups + namespaces) |
| `crypto/` | HKDF key derivation, AES-GCM encryption, key structures, constant-time comparison |
| `vault/` | Credential storage: `CredentialVault` abstraction over pluggable backends (memory, Postgres, HashiCorp Vault) |
| `token/` | PASETO token generation/validation, Redis session store, token revocation |
| `services/` | Business logic: `db/` (connection pool, schema), `llm/` (multi-provider AI: OpenAI, Azure, Claude) |
| `audit/` | Immutable audit log via immudb + in-memory fallback |
| `models/` | Shared data models |
| `tenant/` | Multi-tenant isolation logic, tenant config store |
| `connector/` | External system connectors, HTTP connector, registry |
| `mcp/` | Model Context Protocol server integration |
| `bin/` | Additional binary entry points (`generate_test_token`, `db-verify`) |

The Rust crate is named `vault-service` (`vault_service` when used as a library import).

### Frontend Structure (`frontend/src/`)

React 19 + TypeScript + Vite + Tailwind CSS + shadcn/ui.

The frontend uses a **feature-based** folder structure:

```
features/
├── auth/pages/       — LoginPage, ProfilePage
├── audit/pages/      — AuditPage
├── credentials/pages/ — CredentialsPage
├── dashboard/pages/  — DashboardPage
├── developer/pages/  — DeveloperCenter
├── tenants/pages/    — SettingsPage, UsersPage
└── tokens/pages/     — TokensPage
```

- `components/ui/` — Reusable shadcn/ui components
- `hooks/` — Custom React hooks (data fetching via TanStack Query)
- `stores/` — Zustand global state
- `lib/` — Utilities (API client, `cn()`, etc.)
- `shared/` — Cross-feature utilities (i18n, audit log presentation)
- `app/` — Router, Layout, providers, App root

All routes are protected via `ProtectedRoute`; public routes use `PublicRoute`. Pages are lazy-loaded via `React.lazy`.

### External Services (required for full operation)

- **PostgreSQL** — Primary database (credentials, audit, tenant data)
- **Redis** — Session/token store
- **immudb** — Immutable audit log
- **HashiCorp Vault** — Secrets management (`VAULT_ADDR`, `VAULT_TOKEN`)

See `docker/docker-compose.yml` for default connection settings and env vars.

### Integration Tests

Tests in `tests/` use `[[test]]` entries in `Cargo.toml`. Key test files:

| Test | Path |
|------|------|
| `paseto_tests` | `tests/token/paseto_tests.rs` |
| `redis_store_tests` | `tests/token/redis_store_tests.rs` |
| `audit_api_tests` | `tests/api/audit_tests.rs` |
| `tenant_middleware_tests` | `tests/api/tenant_middleware_tests.rs` |
| `rls_integration` | `tests/rls_integration.rs` (requires `rls-tests` feature) |
| `sandbox_export_tests` | `tests/tee/sandbox_export_tests.rs` |
| `dcap_tests` | `tests/tee/dcap_tests.rs` |
| `sgx_hardware_tests` | `tests/` (requires SGX hardware) |

### SDKs

- `sdk-typescript/` — TypeScript client SDK + Sandbox WebSocket client
- `sdk-rust/` — Rust client SDK
- `mcp-server/` — MCP server for AI agent integration
- `cli/` — CLI management tool

---

## MetaBot Workspace

This workspace is managed by **MetaBot** — an AI assistant accessible via Feishu/Telegram that runs Claude Code with full tool access.

### /metaskill — AI Agent Team Generator

```
/metaskill ios app          → generates full .claude/ agent team
/metaskill a security agent → creates a single agent
/metaskill a deploy skill   → creates a custom slash command
```

### /metamemory — Shared Knowledge Store

```bash
mm search <query>       # Search documents
mm get <doc_id>         # Get document by ID
mm list [folder_id]     # List documents
mm folders              # Browse folder tree
```

### /metabot — Agent Bus, Scheduling & Bot Management

```bash
mb bots                                    # List all bots
mb task <botName> <chatId> <prompt>        # Delegate task
mb schedule list                           # List scheduled tasks
mb schedule add <bot> <chatId> <sec> <prompt>  # Schedule a task
mb health                                  # Health check
```

### Guidelines

- **Search before creating** — always check if a file or document already exists before creating new ones.
- **Use metamemory** — when you discover important knowledge, project patterns, or user preferences, save them to memory so future sessions can benefit.
- **Output files** — when generating files the user needs (images, PDFs, reports), copy them to the outputs directory provided in the system prompt so they get sent to the chat automatically.
- **Be concise in chat** — responses appear as Feishu/Telegram cards with limited space. Keep answers focused and use markdown formatting.
