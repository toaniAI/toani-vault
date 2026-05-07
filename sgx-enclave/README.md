# ToaniVault SGX Enclave

This directory now contains the real Intel SGX SDK build inputs for hardware attestation:

- `Enclave.edl` - the enclave ECALL contract
- `trusted/Enclave.c` - trusted SGX code that generates reports and sealing keys inside the enclave
- `untrusted/host_runtime_bridge.c` - URTS bridge that creates the enclave and performs ECALLs
- `Enclave.config.xml` - enclave signing configuration

## Build outputs

The Linux SGX build/sign flow produces three runtime artifacts under `target/sgx-enclave/`:

- `libcredbridge_enclave.so` - unsigned enclave image
- `credbridge_enclave.signed.so` - signed enclave image used by `TEE_ENCLAVE_PATH`
- `libcredbridge_sgx_urts_bridge.so` - host-side URTS bridge loaded by the Rust runtime

## Build requirements

This flow requires a Linux SGX build environment with:

- Intel SGX SDK (`sgx_edger8r`, `sgx_sign`, headers, trusted/untrusted libraries)
- Intel SGX DCAP runtime libraries on the execution host
- a signing key via `SGX_SIGNING_KEY`, or the Intel sample key under `/opt/intel/sgxsdk/SampleCode/SampleEnclave/Enclave_private.pem`

## Commands

```bash
bash scripts/build-sgx-enclave.sh
bash scripts/sign-sgx-enclave.sh
```

Then run the hardware path with:

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=$PWD/target/sgx-enclave/credbridge_enclave.signed.so
# optional if the bridge is not adjacent to the enclave artifact
export TEE_SGX_HOST_BRIDGE_LIB_PATH=$PWD/target/sgx-enclave/libcredbridge_sgx_urts_bridge.so
```

## Scope note

Phase A is now real for:

- enclave load via URTS
- identity retrieval
- targeted report generation for DCAP quote generation
- hardware-rooted sealing-key retrieval

Credential encrypt/decrypt remains host-side in this phase; later phases can move those operations
behind enclave ECALLs.
