# CredBridge - TEE Enclave Core Module

[中文 README](README_zh.md)

## Overview

CredBridge is an AI-native zero-trust credential vault built around Intel SGX TEE. It uses a
layered key hierarchy and a hardware-backed execution boundary so credential material can be
derived, sealed, attested, and, on the hardware path, encrypted inside the enclave-backed flow.

## Documentation

- Full docs: [docs/README.md](docs/README.md)
- Deployment and operations: [docs/05-部署与运维/README.md](docs/05-部署与运维/README.md)
- Developer guides: [docs/07-开发者指南/README.md](docs/07-开发者指南/README.md)
- SGX runner runbook: [docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md](docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md)

## Core Capabilities

- Intel SGX TEE isolation
- Four-layer key hierarchy
- AES-256-GCM credential encryption
- PASETO v4.local token authentication
- SGX DCAP attestation and quote verification
- Multi-tenant isolation with schema-per-tenant and RLS
- React web console
- TypeScript SDK, Rust SDK, MCP server, and CLI

## Architecture Summary

```text
L0: SGX Sealing Key
  -> L1: Enclave Master Key
    -> L2: User Vault Key
      -> L3: Credential Encryption Key
        -> AES-256-GCM encrypted credential
```

Key modules:

- `src/tee/enclave.rs`: enclave lifecycle and enclave-facing crypto orchestration
- `src/tee/host_runtime.rs`: host runtime bridge for loading the signed enclave artifact
- `src/tee/provider.rs`: runtime-backed identity and root material bootstrap
- `src/tee/sealing.rs`: sealing key retrieval and sealing flow
- `src/tee/dcap.rs`: DCAP quote generation and verification
- `sgx-enclave/`: Linux SGX enclave build target and ECALL surface

## Real SGX Host Integration

The repository now splits the real hardware path into two layers:

- Host-side runtime integration in Rust
- Linux-only enclave build and signing artifacts

### How it works

When `TEE_MODE=hardware` is enabled, the production path is expected to use a signed enclave
artifact and the host runtime bridge instead of defaulting to simulated root key or report inputs.

The flow is:

1. The host reads `TEE_ENCLAVE_PATH`.
2. `SgxHostRuntime` loads the signed enclave shared object.
3. The host calls enclave-facing FFI/ECALL entry points to retrieve:
   - enclave identity
   - report data
   - sealing key
   - hardware-path credential encrypt/decrypt operations
4. `DcapService` uses the report path to generate and verify quotes.
5. If the enclave artifact, runtime load, report generation, quote generation, or DCAP dependency
   chain is missing, `TEE_MODE=hardware` fails closed instead of falling back to simulation.

On macOS, this repository only validates code changes, simulation-safe paths, and compile-safe
integration. Real SGX build, signing, quote generation, and hardware tests must run on a Linux SGX
runner or TEE-oriented Docker environment.

## Deployment

### Required environment contract

Minimum hardware-mode configuration:

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/path/to/credbridge_enclave.signed.so
```

Common optional settings:

```bash
export SGX_SIGNING_KEY=/path/to/sgx-signing-key.pem
export TEE_SGX_DCAP_QL_LIB_PATH=/usr/lib/x86_64-linux-gnu/libsgx_dcap_ql.so
export TEE_SGX_DCAP_QV_LIB_PATH=/usr/lib/x86_64-linux-gnu/libsgx_dcap_quoteverify.so
```

### Drone SGX runner deployment

The hardware pipeline in [.drone.yml](.drone.yml), `backend-hardware-attestation`, is the current
reference for SGX runner execution. It performs:

```bash
bash scripts/check_sgx_environment.sh
bash scripts/build-sgx-enclave.sh
bash scripts/sign-sgx-enclave.sh

export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/drone/src/target/sgx-enclave/credbridge_enclave.signed.so

cargo test --features tee-hardware --lib --tests --no-run
cargo test --features tee-hardware --test sgx_hardware_tests -- --ignored --test-threads=1
```

Expected artifacts:

- `target/sgx-enclave/libcredbridge_enclave.so`
- `target/sgx-enclave/credbridge_enclave.signed.so`

### TEE Docker deployment

The project includes a dedicated SGX build image at [docker/sgx/Dockerfile](docker/sgx/Dockerfile).
It is for enclave build, signing, and hardware compile/test checks, not for the normal application
runtime image.

Build and run it like this:

```bash
docker build -f docker/sgx/Dockerfile -t credbridge-sgx-build .

docker run --rm -it \
  --device /dev/sgx_enclave \
  --device /dev/sgx_provision \
  -v "$PWD":/workspace \
  -w /workspace \
  -e TEE_MODE=hardware \
  -e TEE_ENCLAVE_PATH=/workspace/target/sgx-enclave/credbridge_enclave.signed.so \
  credbridge-sgx-build
```

To run hardware tests from the same container:

```bash
docker run --rm -it \
  --device /dev/sgx_enclave \
  --device /dev/sgx_provision \
  -v "$PWD":/workspace \
  -w /workspace \
  -e TEE_MODE=hardware \
  -e TEE_ENCLAVE_PATH=/workspace/target/sgx-enclave/credbridge_enclave.signed.so \
  credbridge-sgx-build \
  bash -lc 'bash scripts/check_sgx_environment.sh && bash scripts/build-sgx-enclave.sh && bash scripts/sign-sgx-enclave.sh && cargo test --features tee-hardware --test sgx_hardware_tests -- --ignored --test-threads=1'
```

## Local Development

Standard local verification:

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
```

Hardware compile-only verification on Linux:

```bash
cargo test --features tee-hardware --lib --tests --no-run
```

## Additional References

- [SGX build image notes](docker/sgx/README.md)
- [SGX enclave skeleton notes](sgx-enclave/README.md)
- [CLI README](cli/README.md)
- [TypeScript SDK README](sdk-typescript/README.md)
- [Rust SDK README](sdk-rust/README.md)
