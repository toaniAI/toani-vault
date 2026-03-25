**English** | [中文](README_CN.md)

# CredBridge

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)
![Rust](https://img.shields.io/badge/rust-2024-orange.svg)

**CredBridge** is an AI-native, zero-trust credential vault with hardware-level security via Intel SGX TEE (Trusted Execution Environment). It implements a four-layer key hierarchy to ensure credential data is processed exclusively within hardware-isolated secure boundaries.

## Features

| Feature | Status | Description |
|---------|--------|-------------|
| **TEE Secure Vault** | ✅ | Intel SGX TEE hardware-level encrypted isolation |
| **Four-Layer Key Hierarchy** | ✅ | L0–L3 key derivation chain |
| **Credential Encryption** | ✅ | AES-256-GCM authenticated encryption |
| **PASETO Token Auth** | ✅ | PASETO v4.local secure tokens (not JWT) |
| **Audit Log** | ✅ | Tamper-proof audit trail via immudb |
| **Remote Attestation** | ✅ | SGX DCAP remote attestation |
| **Multi-Tenancy** | ✅ | Schema-per-tenant + PostgreSQL Row Level Security |
| **Web Console** | ✅ | React management UI |
| **TypeScript SDK** | ✅ | TypeScript client SDK |
| **Rust SDK** | ✅ | Rust client SDK |
| **MCP Server** | ✅ | Model Context Protocol integration |
| **TEE Sandbox API** | ✅ | Secure browser automation inside TEE (nsjail + seccomp + cgroups) |
| **OpenAPI Spec** | ✅ | Full REST API documentation |
| **CLI Tool** | ✅ | Command-line management for credentials, tokens, audit, sandbox |

## Architecture

### Four-Layer Key Hierarchy

```text
L0: Hardware Root Key (SGX Sealing Key)
  │ HKDF-Extract
  ▼
L1: Enclave Master Key  (derived inside the Enclave)
  │ HKDF-Expand(tenant_id + user_id)
  ▼
L2: User Vault Key      (per-user, 5-minute TTL cache)
  │ HKDF-Expand(credential_id + purpose)
  ▼
L3: Credential Encryption Key  (per-credential, ephemeral)
  │ AES-256-GCM
  ▼
Encrypted Credential
```

### Core Modules

| Module | Path | Responsibility |
|--------|------|---------------|
| Enclave | `src/tee/enclave.rs` | Enclave lifecycle, encrypt/decrypt operations |
| Keys | `src/tee/keys.rs` | Key management (TTL cache, restart recovery, MRSIGNER verification) |
| Sealing | `src/tee/sealing.rs` | SGX Sealing Key retrieval and sealed storage |
| Attestation | `src/tee/attestation.rs` | SGX DCAP remote attestation protocol |
| DCAP | `src/tee/dcap.rs` | DCAP Quote generation and verification |
| Sandbox | `src/tee/sandbox/` | TEE secure execution sandbox (browser automation) |
| Review | `src/tee/sandbox/review/` | AI review engine (operation review, prompt injection detection) |
| Export | `src/tee/sandbox/export/` | Secure export channel (screenshot review, signed data export) |
| HKDF | `src/crypto/hkdf.rs` | Key derivation implementation |
| Vault Storage | `src/vault/storage.rs` | Credential storage (in-memory / PostgreSQL) |

### SGX DCAP Remote Attestation Flow

```text
┌──────────────────────────────────────────────────────────────┐
│                  SGX DCAP Attestation Flow                   │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│   Verifier                          Prover (Enclave)         │
│      │                                    │                  │
│      │  1. Generate random challenge       │                  │
│      │ ──────────────────────────────────> │                  │
│      │                                    │                  │
│      │                                    │ 2. Generate Quote │
│      │                                    │    REPORT_DATA = │
│      │                                    │    hash(challenge│
│      │                                    │    + identity)   │
│      │  3. Return Quote                   │                  │
│      │ <────────────────────────────────── │                  │
│      │                                    │                  │
│      │  4. Verify Quote                   │                  │
│      │     - Verify ECDSA signature       │                  │
│      │     - Verify MRENCLAVE             │                  │
│      │     - Verify challenge binding     │                  │
│      │                                    │                  │
│      │  5. Establish secure channel       │                  │
│      │ ──────────────────────────────────> │                  │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### TEE Sandbox Architecture

CredBridge TEE Sandbox provides "credentials never leave the Enclave" browser automation, enabling AI agents to perform sensitive operations inside the trusted execution environment.

```text
┌───────────────────────────────────────────────────────────────┐
│                    TEE Sandbox Architecture                   │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │             SDK / API Client                            │  │
│  │       (TypeScript SDK / REST API / MCP)                 │  │
│  └──────────────────────────┬──────────────────────────────┘  │
│                             │  WebSocket / HTTPS              │
│  ┌──────────────────────────▼──────────────────────────────┐  │
│  │              CredBridge API Gateway                     │  │
│  │     (auth, routing, audit log, rate limiting)           │  │
│  └──────────────────────────┬──────────────────────────────┘  │
│                             │                                 │
│  ╔══════════════════════════╧══════════════════════════════╗  │
│  ║                    Intel SGX TEE                        ║  │
│  ║  ┌─────────────────────────────────────────────────┐   ║  │
│  ║  │           TEE Sandbox Manager                   │   ║  │
│  ║  │  ┌─────────────┐ ┌─────────────┐ ┌──────────┐  │   ║  │
│  ║  │  │ Session Pool│ │NsjailSandbox│ │AI Review │  │   ║  │
│  ║  │  │ (warm pool) │ │(isolation)  │ │ Engine   │  │   ║  │
│  ║  │  └──────┬──────┘ └──────┬──────┘ └────┬─────┘  │   ║  │
│  ║  │         └───────────────┴──────────────┘        │   ║  │
│  ║  │                  Secure Export Channel           │   ║  │
│  ║  └─────────────────────────────────────────────────┘   ║  │
│  ║         Chromium (nsjail isolated)                      ║  │
│  ║         • PID/Network/Mount/IPC Namespaces              ║  │
│  ║         • seccomp-bpf syscall filtering (~50 calls)     ║  │
│  ║         • cgroups v2 resource limits                    ║  │
│  ╚═════════════════════════════════════════════════════════╝  │
│                             │                                 │
│  ┌──────────────────────────▼──────────────────────────────┐  │
│  │         External Services (read-only connections)       │  │
│  │     PostgreSQL   Redis   LLM API   HashiCorp Vault      │  │
│  └─────────────────────────────────────────────────────────┘  │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

## Quick Start

### Option 1: Docker Compose (Recommended)

Start all services (PostgreSQL, Redis, immudb, Vault, and the app) with a single command:

```bash
git clone https://github.com/your-org/credbridge.git
cd credbridge

# Start full stack
docker compose -f docker/docker-compose.yml up
```

The API server will be available at `http://localhost:8080`.

The web console will be available at `http://localhost:5173` (when running in development mode).

### Option 2: Local Development

**Prerequisites:**

- Rust (edition 2024) — [rustup.rs](https://rustup.rs)
- PostgreSQL 14+
- Redis 7+
- immudb (optional, falls back to in-memory audit log)
- HashiCorp Vault (optional)

**Backend:**

```bash
# Copy and edit environment configuration
cp .env.example .env

# Key environment variables:
# DATABASE_URL=postgres://user:pass@localhost/credbridge
# REDIS_URL=redis://localhost:6379
# TEE_MODE=simulation          # use 'hardware' on SGX-capable machines
# CREDBRIDGE_STORAGE_BACKEND=postgres

# Run database migrations
cargo sqlx migrate run

# Start the server
RUST_LOG=debug cargo run
```

**Frontend:**

```bash
cd frontend
npm install
npm run dev
# Available at http://localhost:5173
```

**Verify SGX support (production only):**

```bash
# Check CPU support
grep sgx /proc/cpuinfo

# Check device nodes
ls -la /dev/sgx*

# Build with SGX feature
cargo build --features sgx
```

## Development

```bash
# Run all tests
cargo test

# Run a specific module's tests
cargo test -- tee::
cargo test -- vault::
cargo test -- crypto::

# Run integration tests
cargo test --test credentials_api_tests

# Run with RLS tests (requires PostgreSQL)
cargo test --features rls-tests

# Format code (mandatory before commit)
cargo fmt

# Lint with zero warnings (mandatory before commit)
cargo clippy --tests -- -D warnings
```

## API Reference

All API routes are mounted at `/api/v1`. All requests require a PASETO v4.local Bearer token:

```http
Authorization: Bearer <paseto_v4_local_token>
```

### Credentials

| Method | Endpoint | Description | Required Scope |
|--------|----------|-------------|----------------|
| POST | `/api/v1/credentials` | Create a credential | `credential:write` |
| GET | `/api/v1/credentials` | List credentials | `credential:read` |
| GET | `/api/v1/credentials/:id` | Get credential details | `credential:read` |
| POST | `/api/v1/credentials/:id/decrypt` | Decrypt a credential | `credential:decrypt` |
| DELETE | `/api/v1/credentials/:id` | Delete a credential | `credential:write` or `admin` |

### Token Scopes

| Scope | Permission |
|-------|-----------|
| `credential:read` | Read credential metadata |
| `credential:decrypt` | Decrypt ciphertext to plaintext (implies read) |
| `credential:write` | Create / update credentials |
| `credential:delete` | Delete credentials |
| `token:manage` | Manage tokens (revoke, refresh) |
| `audit:read` | Read audit logs |
| `admin` | All operations |

### Remote Attestation

```http
POST /api/v1/attestation/challenge   — Generate a challenge
POST /api/v1/attestation/verify      — Verify a Quote and establish secure channel
```

### TEE Sandbox

```http
POST   /api/v1/sandbox/sessions              — Create a sandbox session
DELETE /api/v1/sandbox/sessions/:id          — Close a session
POST   /api/v1/sandbox/sessions/:id/navigate — Navigate to URL
POST   /api/v1/sandbox/sessions/:id/click    — Click an element
POST   /api/v1/sandbox/sessions/:id/fill     — Fill a form field
POST   /api/v1/sandbox/sessions/:id/screenshot — Take a screenshot (AI-reviewed)
WS     /api/v1/sandbox/sessions/:id/ws       — WebSocket real-time connection
```

Full OpenAPI specification: [`docs/openapi/sandbox.yaml`](docs/openapi/sandbox.yaml)

### Enclave Rust API

```rust
use vault_service::tee::{Enclave, EnclaveConfig};

let mut enclave = Enclave::new(EnclaveConfig::default());
enclave.initialize()?;

// Encrypt a credential
let encrypted = enclave.encrypt_credential(
    "tenant_123",
    "user_456",
    "cred_789",
    b"secret-password",
)?;

// Decrypt a credential
let plaintext = enclave.decrypt_credential(
    "tenant_123",
    "user_456",
    "cred_789",
    &encrypted,
)?;

enclave.shutdown()?;
```

### TypeScript SDK

```bash
npm install @credbridge/sdk
```

```typescript
import { CredBridgeSDK } from '@credbridge/sdk';

const sdk = new CredBridgeSDK({
  baseUrl: 'https://api.credbridge.io',
  token: 'v4.local.your-paseto-token',
});

// Create a sandbox session (credentials never leave the Enclave)
const { sessionId } = await sdk.sandbox.createSession({
  serviceId: 'example-service',
  credentialId: 'cred-123',
  startUrl: 'https://example.com',
});

// Fill credentials using template substitution
await sdk.sandbox.fill(sessionId, '#username', '{{CREDENTIAL.username}}');
await sdk.sandbox.fill(sessionId, '#password', '{{CREDENTIAL.password}}');
await sdk.sandbox.click(sessionId, '#login-button');

// Take an AI-reviewed screenshot
const screenshot = await sdk.sandbox.takeScreenshot(sessionId, { type: 'png' });

await sdk.sandbox.closeSession(sessionId);
```

### CLI

```bash
# Install
cargo install --path cli

# Authenticate
credbridge auth login --url https://api.credbridge.io --token <token>

# Manage credentials
credbridge credentials list
credbridge credentials create --name prod-db --type database --value "postgres://..."
credbridge credentials decrypt <credential-id>

# Token operations
credbridge tokens verify
credbridge tokens revoke
```

## Project Structure

```
credbridge/
├── src/                     # Rust backend
│   ├── api/                 # Axum HTTP routes and middleware
│   ├── tee/                 # TEE enclave lifecycle, keys, attestation, sandbox
│   │   └── sandbox/         # nsjail isolation, AI review, secure export
│   ├── crypto/              # HKDF key derivation, AES-GCM encryption
│   ├── vault/               # Credential storage (memory / PostgreSQL / Vault)
│   ├── audit/               # Immutable audit log (immudb)
│   ├── token/               # PASETO token generation / validation
│   ├── tenant/              # Multi-tenant isolation
│   └── main.rs
├── frontend/                # React 19 + TypeScript + Vite + Tailwind CSS
│   └── src/
│       ├── features/        # Feature modules (auth, credentials, audit, ...)
│       ├── components/      # shadcn/ui based reusable components
│       └── hooks/           # TanStack Query data-fetching hooks
├── sdk-typescript/          # TypeScript client SDK + WebSocket sandbox client
├── sdk-rust/                # Rust client SDK
├── mcp-server/              # Model Context Protocol server
├── cli/                     # CLI management tool
├── tests/                   # Rust integration tests
├── migrations/              # SQLx database migrations
├── docker/                  # Docker Compose configurations
└── docs/                    # Documentation
```

## Key Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CREDBRIDGE_PORT` | `8080` | HTTP server port |
| `CREDBRIDGE_HOST` | `0.0.0.0` | HTTP server host |
| `CREDBRIDGE_ENV` | `development` | `development` or `production` |
| `TEE_MODE` | `hardware` | `simulation` (dev) or `hardware` (SGX) |
| `CREDBRIDGE_STORAGE_BACKEND` | `auto` | `auto`, `memory`, `postgres`, `vault` |
| `DATABASE_URL` | — | PostgreSQL connection string |
| `VAULT_ADDR` / `VAULT_TOKEN` | — | HashiCorp Vault connection |
| `CREDBRIDGE_ALLOWED_ORIGINS` | — | Comma-separated CORS origins (production) |
| `RUST_LOG` | `info` | Log level |

## Performance

### Core Cryptography (P99 latency)

| Operation | Latency |
|-----------|---------|
| Enclave initialization | ~50 ms |
| Key derivation (L1 → L2) | ~5 ms |
| Key derivation (L2 → L3) | ~2 ms |
| AES-256-GCM encrypt | ~0.1 ms |
| Credential decrypt (end-to-end) | ~10 ms |

### TEE Sandbox

| Metric | Local Dev | TEE Hardware |
|--------|-----------|--------------|
| Warm instance startup | ≤ 100 ms | ≤ 50 ms |
| Cold start creation | ≤ 3 s | ≤ 2 s |
| Browser operation latency | ≤ 2 s | ≤ 1.5 s |
| AI review latency | ≤ 500 ms | ≤ 500 ms |
| Concurrent sessions | 50 | 100 |

See [docs/PHASE4_PERFORMANCE_REPORT.md](docs/PHASE4_PERFORMANCE_REPORT.md) for the full report.

## Security

- **Hardware isolation**: All key material and credential plaintext are processed exclusively inside the SGX enclave.
- **Memory safety**: All key structures implement `ZeroizeOnDrop` — key material is overwritten with zeros on drop.
- **Replay protection**: Each attestation challenge is randomly generated and single-use with a 5-minute TTL.
- **Tamper detection**: AES-256-GCM provides authenticated encryption; any ciphertext modification raises `AuthenticationFailed`.
- **Multi-tenant isolation**: Schema-per-tenant + PostgreSQL RLS enforced at the database layer.
- **Least privilege tokens**: PASETO tokens carry fine-grained scopes; restricted tokens can be locked to specific credential IDs.
- **Sandbox isolation**: Browser automation runs inside nsjail with PID/Network/Mount/IPC namespace isolation, seccomp-bpf syscall filtering, and cgroups v2 resource limits.

## Documentation

| Document | Description |
|----------|-------------|
| [docs/API.md](docs/API.md) | Full RESTful API reference |
| [docs/openapi/sandbox.yaml](docs/openapi/sandbox.yaml) | OpenAPI 3.0 spec (Sandbox API) |
| [docs/SDK_SANDBOX_GUIDE.md](docs/SDK_SANDBOX_GUIDE.md) | TypeScript Sandbox SDK guide |
| [docs/MCP_INTEGRATION.md](docs/MCP_INTEGRATION.md) | Model Context Protocol integration |
| [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) | Deployment and configuration guide |
| [docs/USER_MANUAL.md](docs/USER_MANUAL.md) | End-user manual |
| [docs/PHASE4_PERFORMANCE_REPORT.md](docs/PHASE4_PERFORMANCE_REPORT.md) | Performance test report |

## Contributing

Contributions are welcome. Please follow these steps:

1. Fork the repository and create a feature branch.
2. Ensure all pre-commit checks pass before submitting a PR:

   ```bash
   cargo fmt
   cargo clippy --tests -- -D warnings
   cargo test
   ```

3. Open a pull request with a clear description of the change and its motivation.
4. All security-related changes require a dedicated negative test case.

## License

This project is licensed under the [MIT License](LICENSE).
