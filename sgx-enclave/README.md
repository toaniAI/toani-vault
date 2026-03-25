# CredBridge SGX Enclave

This directory contains the Phase A skeleton for the real SGX enclave integration.

## Scope

Phase A keeps the enclave surface intentionally small:

- `credbridge_enclave_get_identity`
- `credbridge_enclave_get_report`
- `credbridge_enclave_get_sealing_key`

The implementation in this repository is a compile-safe host integration skeleton. The real SGX SDK build, EDL code generation, signing, and trusted execution verification must run on a Linux SGX builder or SGX-enabled Drone runner.

## Expected artifacts

The Linux SGX build pipeline should produce:

- `target/sgx-enclave/libcredbridge_enclave.so`
- `target/sgx-enclave/credbridge_enclave.signed.so`

The host runtime expects `TEE_ENCLAVE_PATH` to point at the signed shared object.

## Linux SGX build flow

Use the repository scripts:

```bash
scripts/build-sgx-enclave.sh
scripts/sign-sgx-enclave.sh
```

These scripts validate the Linux SGX prerequisites and then build/sign the enclave skeleton. On macOS they fail fast by design.

## Notes

- The enclave code here is intentionally minimal and deterministic so the host-side bridge can be developed on macOS.
- In later phases, sealing, key derivation, and credential crypto should move behind additional ECALLs.
