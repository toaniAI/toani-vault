# SGX Build Image

This image is for Linux SGX builders and Drone SGX runners.

It is not intended to be used as the normal application runtime image. Its job is to:

- build the enclave artifact in `sgx-enclave/`
- sign the enclave artifact into `target/sgx-enclave/credbridge_enclave.signed.so`
- run `tee-hardware` compile checks
- provide a consistent TEE-oriented build container for later hardware tests

## Expected environment

Run this image only on Linux. For real hardware validation, the runner should expose:

- `/dev/sgx_enclave`
- `/dev/sgx_provision`
- Intel SGX DCAP libraries
- `aesmd`
- optional PCCS access if the environment does not use Intel PCS directly

The host-side runtime uses:

- `TEE_MODE=hardware`
- `TEE_ENCLAVE_PATH=/workspace/target/sgx-enclave/credbridge_enclave.signed.so`
- `TEE_SGX_HOST_BRIDGE_LIB_PATH=/workspace/target/sgx-enclave/libcredbridge_sgx_urts_bridge.so`

If `sgx_sign` is available, set `SGX_SIGNING_KEY` to the signing key path. If it is not set,
`scripts/sign-sgx-enclave.sh` copies the unsigned artifact as a placeholder so compile checks can
continue, but that placeholder must not be treated as production-ready.

## Build and compile-check commands

Inside the container or on the SGX runner:

```bash
bash scripts/check_sgx_environment.sh
bash scripts/build-sgx-enclave.sh
bash scripts/sign-sgx-enclave.sh

export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/workspace/target/sgx-enclave/credbridge_enclave.signed.so

cargo test --features tee-hardware --lib --tests --no-run
```

## Hardware test command

On a dedicated SGX runner:

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/workspace/target/sgx-enclave/credbridge_enclave.signed.so

cargo test --features tee-hardware --test sgx_hardware_tests -- --ignored --test-threads=1
```

## Example Docker build image usage

```bash
docker build -f docker/sgx/Dockerfile -t credbridge-sgx-build .

docker run --rm -it \
  --device /dev/sgx_enclave \
  --device /dev/sgx_provision \
  -v "$(pwd)":/workspace \
  -w /workspace \
  -e TEE_MODE=hardware \
  -e TEE_ENCLAVE_PATH=/workspace/target/sgx-enclave/credbridge_enclave.signed.so \
  credbridge-sgx-build
```

For a fuller operator sequence, see
[`docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md`](../../docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md).
