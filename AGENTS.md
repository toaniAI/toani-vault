# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

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

### Backend Structure (`src/`)

| Module | Purpose |
|--------|---------|
| `api/` | Axum HTTP routes — credentials, attestation, audit, connector, sandbox, versioning |
| `tee/` | TEE enclave lifecycle, keys, sealing, DCAP attestation, sandbox execution |
| `crypto/` | HKDF key derivation, AES-GCM encryption, key structures |
| `vault/` | Credential storage models and DB operations |
| `token/` | PASETO token generation/validation, Redis session store |
| `services/` | Business logic layer |
| `audit/` | Immutable audit log via immudb |
| `models/` | Shared data models |
| `tenant/` | Multi-tenant isolation logic |
| `connector/` | External system connectors |
| `mcp/` | Model Context Protocol server integration |
| `bin/` | Additional binary entry points |

### Frontend Structure (`frontend/src/`)

React 19 + TypeScript + Vite + Tailwind CSS + shadcn/ui.

- `components/` — Reusable UI components (shadcn/ui based)
- `pages/` — Route-level page components
- `hooks/` — Custom React hooks (data fetching via TanStack Query)
- `stores/` — Zustand global state
- `lib/` — Utilities (API client, `cn()`, etc.)

### External Services (required for full operation)

- **PostgreSQL** — Primary database (credentials, audit, tenant data)
- **Redis** — Session/token store
- **immudb** — Immutable audit log
- **HashiCorp Vault** — Secrets management (`VAULT_ADDR`, `VAULT_TOKEN`)

See `docker/docker-compose.yml` for default connection settings and env vars.

### Integration Tests

Tests in `tests/` are organized by domain and use `[[test]]` entries in `Cargo.toml`. Key test files:
- `credentials_api_tests.rs` — Credential CRUD via HTTP
- `rls_integration_test.rs` — Row-level security enforcement
- `sgx_hardware_tests.rs` — SGX hardware attestation (requires SGX)
- `vault_backend_tests.rs` — Vault storage backend

### SDKs

- `sdk-typescript/` — TypeScript client SDK + Sandbox WebSocket client
- `sdk-rust/` — Rust client SDK
- `cli/` — CLI management tool for operators and automation

---

## MetaBot Workspace

This workspace is managed by **MetaBot** — an AI assistant accessible via Feishu/Telegram that runs Codex with full tool access.

### /metaskill — AI Agent Team Generator

```
/metaskill ios app          → generates full .Codex/ agent team
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

@RTK.md
