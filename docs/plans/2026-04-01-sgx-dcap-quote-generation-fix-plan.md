# SGX DCAP Quote Generation Fix

## Design note

Host responsibilities:

- Load `libsgx_dcap_ql` and resolve `sgx_qe_get_target_info`, `sgx_qe_get_quote_size`, `sgx_qe_get_quote`
- Call `sgx_qe_get_target_info()` first so QE attestation key initialization happens before quote generation
- Pass QE target info into the enclave/runtime and request a targeted report
- Feed the targeted report into `sgx_qe_get_quote_size()` and `sgx_qe_get_quote()`
- Surface stage-specific errors without silently falling back to simulation

Enclave/runtime responsibilities:

- Accept `target_info + report_data`
- Produce an SGX report that is explicitly targeted to the QE target info
- Preserve challenge binding through `REPORT_DATA`

The production hardware path is now:

`sgx_qe_get_target_info -> targeted report generation -> sgx_qe_get_quote_size -> sgx_qe_get_quote`

Legacy environment-driven report and quote generators are retained only as debug fallbacks. They are intentionally not the default hardware path.

## Demo commands

Legacy anti-pattern demo:

```bash
cargo run --features tee-hardware --bin sgx_dcap_quote_legacy_demo
```

Standard Intel-order demo:

```bash
TEE_ENCLAVE_PATH=/path/to/credbridge_enclave.signed.so \
cargo run --features tee-hardware --bin sgx_dcap_quote_standard_demo
```

The legacy demo intentionally skips `sgx_qe_get_target_info()`. The standard demo exercises the corrected order and prints the stage return codes plus the resolved `.so` path.
