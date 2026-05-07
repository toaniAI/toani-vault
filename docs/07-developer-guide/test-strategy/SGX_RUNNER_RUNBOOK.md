# SGX Runner Execution Runbook

This runbook is the authoritative execution guide for the real SGX host integration path on Linux
SGX runners and TEE-oriented Docker environments.

It is intentionally short and operational. It describes the commands that match the current
repository state, especially:

- `.drone.yml`
- `scripts/check_sgx_environment.sh`
- `scripts/build-sgx-enclave.sh`
- `scripts/sign-sgx-enclave.sh`
- `docker/sgx/Dockerfile`
- `tests/sgx_hardware_tests.rs`

## Scope

Use this runbook for:

- Drone `backend-hardware-attestation` pipeline execution
- dedicated Linux SGX runner validation
- TEE Docker compile checks
- manual enclave build/sign/test debugging

Do not use this runbook for macOS. The enclave build and signing scripts are Linux-only by design.

## Required runtime contract

The real hardware path must use these settings:

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/drone/src/target/sgx-enclave/credbridge_enclave.signed.so
```

In non-Drone local Linux runs, replace `/drone/src` with the repository root:

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=$PWD/target/sgx-enclave/credbridge_enclave.signed.so
```

## Runner prerequisites

The SGX runner must provide all of the following:

- Linux host
- Intel SGX-capable CPU with SGX/FLC enabled in BIOS
- `/dev/sgx_enclave`
- `/dev/sgx_provision`
- Intel SGX SDK if `sgx_sign` or SDK helpers are expected from `/opt/intel/sgxsdk`
- Intel SGX DCAP libraries
- `aesmd` running
- access to Intel PCS or a configured PCCS

Optional but recommended:

- `SGX_SIGNING_KEY` pointing to the enclave signing key
- `TEE_SGX_DCAP_QL_LIB_PATH` if DCAP quote library resolution is non-standard
- `TEE_SGX_DCAP_QV_LIB_PATH` if DCAP quote verification library resolution is non-standard

## Drone pipeline mapping

The current Drone hardware pipeline is [.drone.yml](/Users/yvan/AIWorkspace/credbridge/.drone.yml)
pipeline `backend-hardware-attestation`. It runs three steps in order:

1. `sgx-enclave-build`
2. `hardware-compile-check`
3. `hardware-only-tests`

The exact commands executed there are:

### Step 1: enclave build and sign

```bash
apt-get update && apt-get install -y libssl-dev pkg-config protobuf-compiler libsodium-dev cmake build-essential
bash scripts/check_sgx_environment.sh
bash scripts/build-sgx-enclave.sh
bash scripts/sign-sgx-enclave.sh
```

Expected artifacts:

- `target/sgx-enclave/libcredbridge_enclave.so`
- `target/sgx-enclave/credbridge_enclave.signed.so`
- `target/sgx-enclave/libcredbridge_sgx_urts_bridge.so`

### Step 2: hardware compile check

```bash
apt-get update && apt-get install -y libssl-dev pkg-config protobuf-compiler libsodium-dev cmake build-essential
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/drone/src/target/sgx-enclave/credbridge_enclave.signed.so
cargo test --features tee-hardware --lib --tests --no-run
```

Purpose:

- verify the host runtime builds with `tee-hardware`
- verify the signed enclave artifact path is wired into the build/test environment
- fail closed if the hardware path cannot be constructed

### Step 3: hardware-only test suite

```bash
apt-get update && apt-get install -y libssl-dev pkg-config protobuf-compiler libsodium-dev cmake build-essential
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/drone/src/target/sgx-enclave/credbridge_enclave.signed.so
cargo test --features tee-hardware --test sgx_hardware_tests -- --ignored --test-threads=1
```

Purpose:

- run explicit SGX-only tests
- keep hardware tests off the default simulation-safe CI path
- ensure quote/sealing/runtime behavior is validated only on dedicated SGX infrastructure

## Manual SGX runner command checklist

If you are logged into the SGX runner directly, run this sequence from the repository root:

```bash
pwd
uname -a
ls -l /dev/sgx_enclave /dev/sgx_provision
systemctl status aesmd --no-pager

bash scripts/check_sgx_environment.sh

bash scripts/build-sgx-enclave.sh
bash scripts/sign-sgx-enclave.sh

export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=$PWD/target/sgx-enclave/credbridge_enclave.signed.so

cargo test --features tee-hardware --lib --tests --no-run
cargo test --features tee-hardware --test sgx_hardware_tests -- --ignored --test-threads=1
```

If you only need a quick availability probe:

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=$PWD/target/sgx-enclave/credbridge_enclave.signed.so

cargo test --features tee-hardware --test sgx_hardware_tests test_sgx_hardware_availability -- --exact --ignored --nocapture
```

## TEE Docker command checklist

The Docker image in [`docker/sgx/Dockerfile`](/Users/yvan/AIWorkspace/credbridge/docker/sgx/Dockerfile)
is for build/test environments, not the normal runtime image.

Build the image:

```bash
docker build -f docker/sgx/Dockerfile -t credbridge-sgx-build .
```

Run compile checks in a TEE-oriented container:

```bash
docker run --rm -it \
  --device /dev/sgx_enclave \
  --device /dev/sgx_provision \
  -v "$PWD":/workspace \
  -w /workspace \
  -e TEE_MODE=hardware \
  -e TEE_ENCLAVE_PATH=/workspace/target/sgx-enclave/credbridge_enclave.signed.so \
  credbridge-sgx-build
```

Run hardware tests from the same image:

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

If the runner has a signing key available, mount and export it explicitly:

```bash
docker run --rm -it \
  --device /dev/sgx_enclave \
  --device /dev/sgx_provision \
  -v "$PWD":/workspace \
  -v /path/to/sgx-signing-key.pem:/keys/sgx-signing-key.pem:ro \
  -w /workspace \
  -e SGX_SIGNING_KEY=/keys/sgx-signing-key.pem \
  -e TEE_MODE=hardware \
  -e TEE_ENCLAVE_PATH=/workspace/target/sgx-enclave/credbridge_enclave.signed.so \
  credbridge-sgx-build \
  bash -lc 'bash scripts/build-sgx-enclave.sh && bash scripts/sign-sgx-enclave.sh'
```

## Environment variables used by the hardware path

Required:

- `TEE_MODE=hardware`
- `TEE_ENCLAVE_PATH`

Optional and environment-dependent:

- `SGX_SIGNING_KEY`
- `TEE_SGX_DCAP_QL_LIB_PATH`
- `TEE_SGX_DCAP_QV_LIB_PATH`
- `TEE_SGX_QUOTE_VERIFY_CMD`
- `TEE_SGX_PCS_REGISTER_CMD`
- `TEE_PCS_BASE_URL`

Compatibility and fallback variables still present in code, but not intended as the primary
production contract:

- `TEE_SGX_REPORT_B64`
- `TEE_SGX_REPORT_HEX`
- `TEE_SGX_REPORT_PATH`
- `TEE_SGX_REPORT_GENERATOR_CMD`
- `TEE_SGX_ROOT_KEY_HEX`
- `TEE_SGX_ROOT_KEY_PATH`

Production hardware execution should prefer the runtime-backed enclave/report/sealing path instead
of relying on those compatibility variables.

## Expected outcomes

Success means all of the following are true:

- `scripts/check_sgx_environment.sh` reports the SGX environment as ready
- `target/sgx-enclave/libcredbridge_enclave.so` exists
- `target/sgx-enclave/credbridge_enclave.signed.so` exists
- `cargo test --features tee-hardware --lib --tests --no-run` succeeds
- `cargo test --features tee-hardware --test sgx_hardware_tests -- --ignored --test-threads=1` succeeds on the dedicated SGX runner

Failure should be treated as fail-closed if any of these occur:

- enclave artifact missing
- runner missing SGX device nodes
- `aesmd` unavailable
- DCAP library resolution fails
- quote generation or verification cannot be completed

## Fast troubleshooting checklist

Runner sanity:

```bash
ls -l /dev/sgx_enclave /dev/sgx_provision
systemctl status aesmd --no-pager
ldconfig -p | grep -E 'sgx|dcap'
```

Enclave artifact sanity:

```bash
ls -lh target/sgx-enclave/
file target/sgx-enclave/libcredbridge_enclave.so
file target/sgx-enclave/credbridge_enclave.signed.so
```

Compile-only sanity:

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=$PWD/target/sgx-enclave/credbridge_enclave.signed.so
cargo test --features tee-hardware --lib --tests --no-run
```

Single hardware probe:

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=$PWD/target/sgx-enclave/credbridge_enclave.signed.so
cargo test --features tee-hardware --test sgx_hardware_tests test_sgx_hardware_availability -- --exact --ignored --nocapture
```
