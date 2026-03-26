# CredBridge

[中文 README](README_zh.md)

CredBridge is an AI-native, zero-trust credential vault built around Intel SGX TEE. The current
repository centers on one main service runtime, a React console, Rust and TypeScript SDKs, and a
CLI for operators and agent-facing automation.

## What It Provides

- Intel SGX TEE-backed credential handling with hardware and simulation runtime modes
- Four-layer key hierarchy for credential encryption and derivation
- PASETO v4.local authentication plus Redis-backed session/token flows
- Multi-tenant isolation with schema-per-tenant and PostgreSQL RLS
- Audit logging, attestation endpoints, and TEE sandbox execution APIs
- Rust SDK, TypeScript SDK, CLI, and a React frontend

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
