# Toani Vault

[中文 README](README_zh.md)

Toani Vault is an AI-native, zero-trust credential vault built around Intel SGX TEE. The current
repository centers on one main service runtime, Rust and TypeScript SDKs, and a CLI for onboarding,
read-only credential metadata lookup, and bearer-token driven sandbox flows.

## What It Provides

- Intel SGX TEE-backed credential handling with hardware and simulation runtime modes
- Four-layer key hierarchy for credential encryption and derivation
- PASETO v4.local authentication plus Redis-backed session/token flows
- Multi-tenant isolation with schema-per-tenant and PostgreSQL RLS
- Audit logging, attestation endpoints, and TEE sandbox execution APIs
- Rust SDK, TypeScript SDK, and CLI tooling

## Runtime Storage Policy (Phase 5)

The service now enforces explicit per-domain storage requirements at startup:

- `vault`: PostgreSQL or HashiCorp Vault (`CREDBRIDGE_STORAGE_BACKEND`)
- `auth`: PostgreSQL (`DATABASE_URL`)
- `tenant config`: PostgreSQL (`DATABASE_URL`)
- `sandbox records`: PostgreSQL (`DATABASE_URL`)
- `audit`: immudb (`IMMUDB_*`) by default
- `token state`: Redis (`REDIS_URL`)
- `rate limit state`: Redis (`REDIS_URL`)
- `attestation challenges`: Redis (`REDIS_URL`)

Production startup fails if required durable backends are missing.

For local development/testing only, memory fallback must be explicitly enabled:

- `CREDBRIDGE_AUTH_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_TENANT_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_SANDBOX_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_TOKEN_ALLOW_MEMORY_FALLBACK=true`

The following runtime-only structures intentionally remain in memory:

- sandbox warm instance pool
- sandbox active session handles
- sandbox credential short-lived cache

These in-memory structures are performance/runtime concerns only and must not be treated as the
sole source of truth for user-visible or compliance-relevant state.

## Architecture Summary

```text
L0: SGX Sealing Key
  -> L1: Enclave Master Key
    -> L2: User Vault Key
      -> L3: Credential Encryption Key
        -> AES-256-GCM encrypted credential
```

Primary implementation areas:

- `src/api/`: HTTP routes and middleware
- `src/tee/`: TEE lifecycle, attestation, sealing, sandbox, hardware runtime bridge
- `src/token/`: PASETO and session handling
- `src/vault/`: credential persistence and storage backends
- `cli/`: CLI for onboarding, read-only credential metadata, and sandbox operations
- `sdk-rust/` and `sdk-typescript/`: client SDKs

## Documentation

- Documentation hub: [docs/README.md](docs/README.md)
- Deployment and operations: [docs/05-部署与运维/README.md](docs/05-部署与运维/README.md)
- API reference: [docs/03-API 参考/README.md](docs/03-API 参考/README.md)
- Developer guides: [docs/07-开发者指南/README.md](docs/07-开发者指南/README.md)
- SGX runner runbook: [docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md](docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md)

Internal requirement captures and acceptance evidence live under `docs/requirements/` and
`docs/qa_reports/`. Those directories are maintained for product and QA handoff, not as the public
onboarding path.

## CLI Usage Guide

Toani Vault provides a CLI for guided onboarding, local config, read-only credential metadata
lookup, and bearer-token authorized sandbox flows.

For the latest CLI install and usage details, see:

- [CLI Install & Usage (npm)](cli/README.md)
- [CLI Skill for AI Agents](cli/SKILL.md)

### Installation

```bash
# Install from npm (recommended)
npm install -g @toani/vault-cli@latest

# Install from source
cd cli
npm install
npm run build
npm pack
npm install -g ./toani-vault-cli-*.tgz

# Verify installation
toani --version
```

### Quick Start

```bash
# Recommended first-run flow
toani login
toani doctor

# Read-only credential metadata lookup
toani --output json credentials list

# Sandbox connectivity / session checks
toani sandbox stats
toani sandbox list-sessions
```

### CLI Commands Overview

The current published CLI exposes these groups:

- `login`
- `doctor`
- `config` (`init`, `show`)
- `credentials` (`list`, `get`)
- `sandbox`

Do not assume the public CLI also exposes `auth`, mutating `credentials`, `tokens`,
`service-accounts`, or `audit` groups unless you have verified a newer build.

`login` is the recommended setup path. It opens the Dashboard, guides credential + token setup,
validates the token, and stores it in the OS Keychain when available. `doctor` verifies CLI
version, Node.js, token storage, token format, base URL reachability, and token validity.

`credentials` is read-only. It lists or fetches credential metadata only; it does not create,
update, decrypt, or delete credentials.

`sandbox` supports:

- `create-session`
- `list-sessions`
- `get-session`
- `terminate`
- `pause`
- `resume`
- `bootstrap-page`
- `execute`
- `export-dom`
- `export-data`
- `get-operation`
- `stats`

For the exact command matrix and examples, use [cli/README.md](cli/README.md) as the detailed
source of truth.

### Global Options

| Option                  | Description                                       |
| ----------------------- | ------------------------------------------------- |
| `--output <format>`     | Output format: `table` or `json` (default: table) |
| `--base-url <url>`      | Override service URL                              |
| `--token <token>`       | Override bearer token                             |
| `-h, --help`            | Show help information                             |
| `-v, --version`         | Show version information                          |

### Environment Variables

| Variable                | Description                                     |
| ----------------------- | ----------------------------------------------- |
| `TOANI_BASE_URL`        | Service URL override                            |
| `CREDBRIDGE_BASE_URL`   | Secondary service URL override                  |
| `TOANI_VAULT_TOKEN`     | Primary token override                          |
| `CREDBRIDGE_TOKEN`      | Secondary token override                        |
| `HOME`                  | Base directory for `~/.toani/config.json`       |

### Configuration File

The CLI stores configuration in `~/.toani/config.json`. Base URL, output mode, and timeout are
saved there. Tokens are stored in the OS Keychain when configured through `toani login` or
`toani config init --token`, with legacy plaintext token reads retained only for compatibility.

---

## Local Development

Core verification gates:

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
```

Hardware compile check on Linux SGX runners:

```bash
cargo test --features tee-hardware --lib --tests --no-run
```

## Deployment Notes

Normal local stack:

```bash
docker compose -f docker/docker-compose.yml up
```

Hardware-mode execution expects at minimum:

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/path/to/credbridge_enclave.signed.so
```

Reference material:

- [docker/sgx/README.md](docker/sgx/README.md)
- [sgx-enclave/README.md](sgx-enclave/README.md)
- [cli/README.md](cli/README.md)
- [sdk-rust/README.md](sdk-rust/README.md)
- [sdk-typescript/README.md](sdk-typescript/README.md)
