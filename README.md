# Toani Vault

[中文 README](README_zh.md)

Toani Vault is an AI-native, zero-trust credential vault built around Intel SGX TEE. The current
repository centers on one main service runtime, a React console, Rust and TypeScript SDKs, and a
CLI for operators and agent-facing automation.

## What It Provides

- Intel SGX TEE-backed credential handling with hardware and simulation runtime modes
- Four-layer key hierarchy for credential encryption and derivation
- PASETO v4.local authentication plus Redis-backed session/token flows
- Multi-tenant isolation with schema-per-tenant and PostgreSQL RLS
- Audit logging, attestation endpoints, and TEE sandbox execution APIs
- Rust SDK, TypeScript SDK, CLI, and a React frontend

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
- `cli/`: operator and automation CLI
- `sdk-rust/` and `sdk-typescript/`: client SDKs
- `frontend/`: React web console

## Documentation

- Documentation hub: [docs/README.md](docs/README.md)
- Deployment and operations: [docs/05-部署与运维/README.md](docs/05-部署与运维/README.md)
- API reference: [docs/03-API 参考/README.md](docs/03-API 参考/README.md)
- Developer guides: [docs/07-开发者指南/README.md](docs/07-开发者指南/README.md)
- SGX runner runbook: [docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md](docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md)

## CLI Usage Guide

Toani Vault provides a powerful CLI tool for operators and automation workflows.

For the latest CLI install and usage details, see:
- [CLI Install & Usage (npm)](cli/README.md)
- [CLI Skill for AI Agents](cli/SKILL.md)

### Installation

```bash
# Install from npm (recommended)
npm install -g @toani/vault-cli@0.0.1

# Install from source
cargo install --path cli

# Verify installation
toani --version
```

### Quick Start

```bash
# 1. Configure CLI with your Toani Vault server
toani config init --url https://api.toani.ai --token <your-token>

# Or use interactive mode
toani config init

# 2. Verify connection
toani auth status

# 3. Start managing credentials
toani credentials list
```

### CLI Commands Overview

#### Authentication (`auth`)

| Command | Description |
|---------|-------------|
| `toani auth login --url <url> --token <token>` | Login to Toani Vault service |
| `toani auth status` | Check login status |
| `toani auth logout` | Logout and remove local config |

#### Credential Management (`credentials`)

| Command | Description |
|---------|-------------|
| `toani credentials list` | List all credentials |
| `toani credentials get <id>` | Get credential details |
| `toani credentials create` | Create a new credential |
| `toani credentials update <id>` | Update existing credential |
| `toani credentials delete <id>` | Delete a credential |
| `toani credentials decrypt <id>` | Decrypt and view credential value |
| `toani credentials versions <id>` | List version history |
| `toani credentials rollback <id> <version>` | Rollback to specific version |

**Credential Types Supported:**
- `api_key` - API keys
- `username_password` - Username/password pairs
- `oauth_refresh` - OAuth refresh tokens
- `session_cookie` - Session cookies
- `ssh_key` - SSH keys
- `certificate` - TLS/SSL certificates
- `database_connection` - Database connection strings
- `kyc_document` - KYC documents

**Examples:**

```bash
# Create an API key credential
toani credentials create \
  --name "production-api-key" \
  --type api_key \
  --value "sk-live-xxx"

# Create credential with metadata
toani credentials create \
  --name "db-credentials" \
  --type username_password \
  --value "secret-password" \
  --metadata "username=admin" \
  --metadata "host=db.example.com"

# List credentials with type filter
toani credentials list --type api_key

# Decrypt and view credential
toani credentials decrypt <credential-id>

# Delete with confirmation
toani credentials delete <credential-id>

# Force delete without confirmation
toani credentials delete <credential-id> --force

# View version history
toani credentials versions <credential-id>

# Rollback to previous version
toani credentials rollback <credential-id> 3
```

#### Token Management (`tokens`)

| Command | Description |
|---------|-------------|
| `toani tokens create --name <name>` | Create new access token |
| `toani tokens list` | List all tokens |
| `toani tokens revoke <id>` | Revoke a token |
| `toani tokens verify [token]` | Verify token validity |

**Examples:**

```bash
# Create token with specific scopes and expiration
toani tokens create \
  --name "ci-token" \
  --expires-in 7200 \
  --scopes "credential:read,credential:write"

# Verify current token
toani tokens verify

# Verify specific token
toani tokens verify "v4.local.xxx"
```

#### Sandbox Operations (`sandbox`)

Execute credential-consuming operations in isolated TEE sandbox sessions.

| Command | Description |
|---------|-------------|
| `toani sandbox create-session --credential-id <id> --original-intent <desc>` | Create sandbox session |
| `toani sandbox list-sessions` | List active sessions |
| `toani sandbox get-session <id>` | Get session details |
| `toani sandbox terminate <id>` | Terminate session |
| `toani sandbox execute <session-id> --operation-type <type>` | Execute operation |
| `toani sandbox get-operation <operation-id>` | Get operation result |
| `toani sandbox stats` | View sandbox statistics |

**Examples:**

```bash
# Create a sandbox session for a credential
toani sandbox create-session \
  --credential-id <cred-id> \
  --original-intent "Database backup operation"

# Execute operation in sandbox
toani sandbox execute <session-id> \
  --operation-type "database_query" \
  --params '{"query": "SELECT * FROM users"}'

# View sandbox statistics
toani sandbox stats
```

#### Audit Logs (`audit`)

| Command | Description |
|---------|-------------|
| `toani audit logs` | Query audit logs |
| `toani audit export <file>` | Export audit logs |
| `toani audit verify` | Verify audit log integrity |

**Examples:**

```bash
# Query recent audit logs
toani audit logs --limit 100

# Query with time range and filter
toani audit logs \
  --from "2024-01-01T00:00:00Z" \
  --to "2024-01-31T23:59:59Z" \
  --action "credential_access"

# Export to JSON file
toani audit export audit-export.json

# Export to CSV
toani audit export audit-export.csv --format csv

# Verify audit integrity
toani audit verify
```

#### Configuration (`config`)

| Command | Description |
|---------|-------------|
| `toani config init` | Initialize configuration |
| `toani config show` | Display current configuration |
| `toani config set <key> <value>` | Set configuration value |
| `toani config get <key>` | Get configuration value |

**Configuration Keys:**
- `url` - Toani Vault service URL
- `token` - API authentication token
- `output_format` - Output format: `table` or `json`
- `timeout` - Request timeout in seconds

**Examples:**

```bash
# Interactive initialization
toani config init

# Initialize with parameters
toani config init \
  --url https://api.toani.ai \
  --token "v4.local.xxx"

# Set output format to JSON
toani config set output_format json

# Set timeout to 60 seconds
toani config set timeout 60

# View current config
toani config show
```

### Global Options

| Option | Description |
|--------|-------------|
| `-o, --output <format>` | Output format: `table` or `json` (default: table) |
| `-c, --config <path>` | Custom configuration file path |
| `-v, --verbose` | Enable verbose logging |
| `-h, --help` | Show help information |
| `-V, --version` | Show version information |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `CREDBRIDGE_URL` | Service URL override |
| `CREDBRIDGE_TOKEN` | API token override |
| `HOME` | Configuration directory (default: `~/.config/credbridge/`) |

### Configuration File

The CLI stores configuration in `~/.config/credbridge/config.toml`:

```toml
url = "https://api.toani.ai"
token = "v4.local.xxx"
output_format = "table"
timeout = 30
```

Configuration file permissions are automatically set to `0600` (user read/write only).

---

## Local Development

Core verification gates:

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
```

Frontend build:

```bash
cd frontend
npm run build
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
