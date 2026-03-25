# Real SGX Enclave Host Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current host-side logical TEE model with a real Intel SGX enclave integration that supports in-process report generation, DCAP quote generation, and hardware sealing without external report injection.

**Architecture:** Add a dedicated SGX enclave project that owns `EREPORT`/`EGETKEY` operations and exposes a minimal ECALL surface to the Rust host. Keep the existing public `vault-service` APIs and `src/tee/*` orchestration layer, but refactor the host-side `Enclave` object into a proxy over a real enclave runtime handle plus existing key hierarchy, attestation, and sealing services. Use DCAP quote verification in the host, and generate quotes from real enclave reports instead of environment-provided report blobs.

**Tech Stack:** Rust host application, Intel SGX SDK / URTS-style enclave runtime, SGX EDL + signed enclave shared object, DCAP quote library, DCAP quote verify library, Kubernetes SGX runtime environment.

---

## Current State Summary

This repo already has:
- Host-side hardware bootstrap plumbing in [`/Users/yvan/AIWorkspace/credbridge/src/tee/provider.rs`](/Users/yvan/AIWorkspace/credbridge/src/tee/provider.rs)
- DCAP service orchestration in [`/Users/yvan/AIWorkspace/credbridge/src/tee/dcap.rs`](/Users/yvan/AIWorkspace/credbridge/src/tee/dcap.rs)
- Host-side hardware backend adapters in [`/Users/yvan/AIWorkspace/credbridge/src/tee/hardware.rs`](/Users/yvan/AIWorkspace/credbridge/src/tee/hardware.rs)
- A logical in-process enclave model in [`/Users/yvan/AIWorkspace/credbridge/src/tee/enclave.rs`](/Users/yvan/AIWorkspace/credbridge/src/tee/enclave.rs)

This repo does **not** currently have:
- A real enclave project
- `.edl` files
- ECALL/OCALL bindings
- A signed enclave binary
- A host loader using `sgx_urts` or equivalent
- An enclave-side implementation of `sgx_get_key` / report generation

That missing enclave-side project is the hard blocker for “fully real SGX” completion.

## Decision Record

Use a split architecture:
- Host app remains the network-facing process and keeps Axum, DB, Vault, Redis, metrics, and orchestration.
- New enclave project provides only the minimum trusted computing base:
  - `get_identity`
  - `get_report(report_data)`
  - `get_sealing_key(policy)`
  - optional future `seal/unseal`
- Keep AES-GCM credential crypto and key hierarchy in host **for this phase** only if the business requirement accepts “hardware-rooted key derivation” rather than “all crypto executes inside enclave”. If product/security requires all credential crypto inside enclave, add follow-up ECALLs later.

Recommended milestone boundary:
- **Phase A:** real enclave load + real report + real sealing key + host-side DCAP quote + host-side verification
- **Phase B:** move sealing and key derivation fully behind enclave ECALLs
- **Phase C:** move encryption/decryption operations fully behind enclave ECALLs

## Workspace Layout To Add

Create a new SGX subtree:
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/Cargo.toml`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/Enclave.edl`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/src/lib.rs`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/src/ecalls.rs`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/build.rs`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/README.md`
- Create: `/Users/yvan/AIWorkspace/credbridge/scripts/build-sgx-enclave.sh`

Create a host runtime bridge:
- Create: `/Users/yvan/AIWorkspace/credbridge/src/tee/host_runtime.rs`
- Create: `/Users/yvan/AIWorkspace/credbridge/src/tee/ffi_types.rs`

Likely modify:
- Modify: `/Users/yvan/AIWorkspace/credbridge/Cargo.toml`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/enclave.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/provider.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/sealing.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/hardware.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/dcap.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/mod.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/main.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/tests/sgx_hardware_tests.rs`

## Environment Contract

Add or standardize these runtime inputs:
- `TEE_MODE=hardware`
- `TEE_ENCLAVE_PATH=/path/to/credbridge_enclave.signed.so`
- `TEE_SGX_DCAP_QL_LIB_PATH` optional
- `TEE_SGX_DCAP_QV_LIB_PATH` optional
- `TEE_SGX_AESM_SOCKET` optional if runtime needs explicit socket path

Remove as final-state dependencies for production:
- `TEE_SGX_REPORT_B64`
- `TEE_SGX_REPORT_HEX`
- `TEE_SGX_REPORT_PATH`
- `TEE_SGX_REPORT_GENERATOR_CMD`
- `TEE_SGX_ROOT_KEY_HEX`
- `TEE_SGX_ROOT_KEY_PATH`

These may remain as transitional debug fallbacks, but production `hardware` mode should reject them once real enclave path is proven stable.

## Task 1: Introduce the enclave subproject skeleton

**Files:**
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/Cargo.toml`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/Enclave.edl`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/src/lib.rs`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/src/ecalls.rs`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/build.rs`
- Create: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/README.md`

**Step 1: Write the failing build integration check**

Add a host-side test skeleton in:
- Create: `/Users/yvan/AIWorkspace/credbridge/tests/tee/host_runtime_compile_tests.rs`

Test intent:
```rust
#[test]
fn hardware_runtime_requires_enclave_artifact_path() {
    // placeholder until host runtime exists
    assert!(true);
}
```

**Step 2: Create the enclave project manifest**

`sgx-enclave/Cargo.toml` should define:
- `staticlib` or SGX-compatible enclave output form
- dependencies for SGX trusted runtime
- no host-only crates like Tokio, Axum, SQLx

Minimum skeleton:
```toml
[package]
name = "credbridge-sgx-enclave"
version = "0.1.0"
edition = "2021"

[lib]
name = "credbridge_enclave"
crate-type = ["staticlib"]
```

**Step 3: Define minimal EDL**

`Enclave.edl` should export:
- `ecall_get_identity`
- `ecall_get_report`
- `ecall_get_sealing_key`

Pseudo-shape:
```c
public sgx_status_t ecall_get_identity([out, size=32] uint8_t* mrenclave,
                                       [out, size=32] uint8_t* mrsigner);
public sgx_status_t ecall_get_report([in, size=64] uint8_t* report_data,
                                     [out, size=432] uint8_t* report_bytes);
public sgx_status_t ecall_get_sealing_key(uint32_t policy,
                                          [out, size=32] uint8_t* key_bytes);
```

**Step 4: Add enclave-side stubs**

`sgx-enclave/src/ecalls.rs` should compile with stub implementations returning explicit SGX errors until wired.

**Step 5: Add README**

Document:
- enclave build requirements
- signing step
- artifact output path
- expected host runtime path

**Step 6: Run compile check**

Run:
```bash
cargo test --lib --tests --no-run
```

Expected:
- host still compiles
- enclave project exists
- no production behavior change yet

**Step 7: Commit**

```bash
git add sgx-enclave tests/tee/host_runtime_compile_tests.rs
git commit -m "feat: add sgx enclave project skeleton"
```

## Task 2: Add host-side enclave runtime loader

**Files:**
- Create: `/Users/yvan/AIWorkspace/credbridge/src/tee/host_runtime.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/mod.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/Cargo.toml`

**Step 1: Write the failing unit tests**

Add tests in `host_runtime.rs`:
```rust
#[test]
fn hardware_runtime_rejects_missing_enclave_path() {}

#[test]
fn hardware_runtime_rejects_nonexistent_enclave_artifact() {}
```

**Step 2: Add host runtime abstraction**

Expose:
```rust
pub struct SgxHostRuntime { /* enclave id / handle */ }

impl SgxHostRuntime {
    pub fn load(enclave_path: &str, debug: bool) -> Result<Self, HostRuntimeError>;
    pub fn get_identity(&self) -> Result<RuntimeIdentity, HostRuntimeError>;
    pub fn get_report(&self, report_data: [u8; 64]) -> Result<Vec<u8>, HostRuntimeError>;
    pub fn get_sealing_key(&self, policy: SealPolicy) -> Result<[u8; 32], HostRuntimeError>;
}
```

**Step 3: Add runtime error model**

Include:
- enclave load failed
- ECALL failed
- invalid return size
- artifact missing

**Step 4: Wire module export**

Expose host runtime under `crate::tee`.

**Step 5: Run tests**

Run:
```bash
cargo test hardware_runtime_rejects_missing_enclave_path --lib -- --nocapture
cargo test hardware_runtime_rejects_nonexistent_enclave_artifact --lib -- --nocapture
```

**Step 6: Commit**

```bash
git add Cargo.toml src/tee/mod.rs src/tee/host_runtime.rs
git commit -m "feat: add sgx host runtime loader abstraction"
```

## Task 3: Implement enclave-side identity and report ECALLs

**Files:**
- Modify: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/src/ecalls.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/sgx-enclave/src/lib.rs`

**Step 1: Write enclave-focused host integration tests**

Add ignored hardware tests in:
- Modify: `/Users/yvan/AIWorkspace/credbridge/tests/sgx_hardware_tests.rs`

Tests:
```rust
#[test]
#[ignore]
fn test_real_enclave_identity_via_host_runtime() {}

#[test]
#[ignore]
fn test_real_enclave_report_generation() {}
```

**Step 2: Implement `ecall_get_identity`**

Inside enclave, return:
- `MRENCLAVE`
- `MRSIGNER`

**Step 3: Implement `ecall_get_report`**

Generate a real report using enclave-side report instruction or trusted runtime helper.
Output must be the raw SGX report bytes expected by the host quote library bridge.

**Step 4: Validate return sizes**

Host runtime must reject:
- report not 432 bytes
- identity arrays not 32 bytes

**Step 5: Run hardware compile check**

Run:
```bash
cargo test --features tee-hardware --lib --tests --no-run
```

**Step 6: Commit**

```bash
git add sgx-enclave src/tee/host_runtime.rs tests/sgx_hardware_tests.rs
git commit -m "feat: add real enclave identity and report ecalls"
```

## Task 4: Replace hardware report injection with real runtime report generation

**Files:**
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/hardware.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/dcap.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/provider.rs`

**Step 1: Write failing tests**

Add tests that assert hardware mode no longer accepts report injection in strict production mode.

Example:
```rust
#[test]
fn hardware_mode_prefers_real_report_runtime_over_env_report() {}
```

**Step 2: Change quote generation source of truth**

`load_or_generate_hardware_quote()` should:
1. request report from `SgxHostRuntime`
2. call DCAP quote library FFI
3. only fall back to env-based report paths when an explicit compatibility flag is set

**Step 3: Change provider identity source of truth**

`load_hardware_measurements()` should prefer runtime identity from enclave ECALL, not quote/env parsing.

**Step 4: Tighten fail-closed behavior**

In `TEE_MODE=hardware`:
- if no enclave artifact path: fail
- if runtime load fails: fail
- if report generation fails: fail
- if DCAP quote generation fails: fail

**Step 5: Run focused tests**

Run:
```bash
cargo test test_hardware_mode_quote_generation_fails_closed --test dcap_tests -- --nocapture
cargo test test_sgx_hardware_availability --test sgx_hardware_tests -- --ignored --nocapture
```

**Step 6: Commit**

```bash
git add src/tee/hardware.rs src/tee/dcap.rs src/tee/provider.rs tests
git commit -m "feat: generate hardware quotes from real enclave reports"
```

## Task 5: Replace simulated sealing with enclave-backed sealing key retrieval

**Files:**
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/sealing.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/host_runtime.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/enclave.rs`

**Step 1: Write failing tests**

Add tests that assert hardware sealing path no longer calls `simulate_egetkey`.

Suggested test strategy:
- unit test with mocked host runtime
- ignored hardware test with real enclave

**Step 2: Introduce runtime-backed `get_sealing_key`**

`SealingService::get_sealing_key()` should:
- in simulation mode: keep current simulated behavior
- in hardware mode: call `SgxHostRuntime::get_sealing_key(policy)`

**Step 3: Thread runtime through enclave initialization**

`Enclave::initialize()` should own or reference the loaded host runtime so sealing and quote generation use the same real enclave instance.

**Step 4: Remove duplicated hardware root source hacks**

Do not source L0 from env root key in strict hardware mode once enclave-backed sealing key is working.

**Step 5: Run tests**

Run:
```bash
cargo test test_seal_and_restore_master_key_closed_loop --lib -- --nocapture
cargo test test_sgx_sealing_key_derivation --test sgx_hardware_tests -- --ignored --nocapture
```

**Step 6: Commit**

```bash
git add src/tee/sealing.rs src/tee/enclave.rs src/tee/host_runtime.rs
git commit -m "feat: source hardware sealing keys from real sgx enclave"
```

## Task 6: Refactor host `Enclave` into a proxy over real runtime state

**Files:**
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/enclave.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/provider.rs`

**Step 1: Add runtime handle field**

Add:
```rust
runtime: Option<Arc<SgxHostRuntime>>
```

**Step 2: Ensure initialization order**

Initialization sequence in hardware mode must be:
1. load signed enclave
2. fetch identity
3. fetch sealing key
4. bootstrap key hierarchy
5. initialize DCAP quote

**Step 3: Keep simulation path untouched**

Do not regress the existing simulation tests.

**Step 4: Update debug/status output**

Expose in `Debug`/metrics:
- runtime loaded
- enclave artifact path
- root key source

**Step 5: Run tests**

Run:
```bash
cargo test test_enclave_lifecycle --lib -- --nocapture
cargo test test_hardware_mode_fails_closed_without_provider --lib -- --nocapture
```

**Step 6: Commit**

```bash
git add src/tee/enclave.rs src/tee/provider.rs
git commit -m "refactor: proxy host enclave state to real sgx runtime"
```

## Task 7: Promote hardware verification to strict production path

**Files:**
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/hardware.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/dcap.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/main.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/api/attestation.rs`

**Step 1: Remove production dependence on quote verifier commands**

Hardware mode should prefer:
1. native DCAP verifier FFI
2. explicit emergency command backend only when compatibility mode is enabled

**Step 2: Update logs**

Startup logs must state:
- hardware feature enabled
- enclave artifact loaded
- report source = enclave
- quote verification backend = dcap-ffi

**Step 3: Update runtime snapshot / health**

Status should distinguish:
- no enclave artifact
- enclave loaded but quote failed
- quote generated but verification failed
- fully attested

**Step 4: Run tests**

Run:
```bash
cargo test test_get_attestation_status_endpoint --test attestation_api_tests -- --nocapture
cargo test test_health_check_endpoint --test attestation_api_tests -- --nocapture
```

**Step 5: Commit**

```bash
git add src/main.rs src/api/attestation.rs src/tee/hardware.rs src/tee/dcap.rs
git commit -m "feat: promote real dcap verification to production hardware path"
```

## Task 8: Update SGX hardware integration tests to assert success, not placeholders

**Files:**
- Modify: `/Users/yvan/AIWorkspace/credbridge/tests/sgx_hardware_tests.rs`

**Step 1: Rewrite hardware tests**

Replace old fail-closed placeholder expectations with real success-path assertions:
- enclave loads
- measurements are non-zero and stable
- DCAP quote generation succeeds
- quote verification succeeds
- sealing key round-trip succeeds

**Step 2: Split tests**

Keep:
- success-path hardware tests
- negative-path tests for missing artifact / missing libraries / bad collateral

**Step 3: Add explicit prerequisites helper**

Create a helper that checks:
- `/dev/sgx_enclave`
- `/dev/sgx_provision`
- enclave artifact path
- DCAP libraries resolvable

**Step 4: Run hardware suite on SGX runner**

Run:
```bash
TEE_MODE=hardware cargo test --features tee-hardware --test sgx_hardware_tests -- --ignored --test-threads=1
```

Expected:
- success-path tests pass on SGX runner
- fail fast with explicit prerequisite message otherwise

**Step 5: Commit**

```bash
git add tests/sgx_hardware_tests.rs
git commit -m "test: validate real sgx enclave hardware success path"
```

## Task 9: Tighten deployment and artifact pipeline

**Files:**
- Modify: `/Users/yvan/AIWorkspace/credbridge/Dockerfile`
- Modify: `/Users/yvan/AIWorkspace/credbridge/.drone.yml`
- Modify: `/Users/yvan/AIWorkspace/credbridge/docker/*` as needed
- Modify: `/Users/yvan/AIWorkspace/credbridge/INTEL_SGX_DEPLOYMENT_REQUIREMENTS.md`
- Modify: `/Users/yvan/AIWorkspace/credbridge/README.md`

**Step 1: Produce enclave artifact during build**

Build pipeline must generate and package:
- host binary
- signed enclave artifact

**Step 2: Ensure runtime image carries both**

Image should include:
- `/app/vault-service`
- `/app/credbridge_enclave.signed.so`

**Step 3: Document runtime env**

Document:
- `TEE_ENCLAVE_PATH`
- DCAP library mounting or image packaging
- required SGX device nodes

**Step 4: Validate image contract**

Run:
```bash
docker build -t credbridge-sgx-test .
```

**Step 5: Commit**

```bash
git add Dockerfile .drone.yml docker INTEL_SGX_DEPLOYMENT_REQUIREMENTS.md README.md
git commit -m "build: package real sgx enclave artifact with host service"
```

## Task 10: Remove transitional compatibility backdoors

**Files:**
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/hardware.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/mod.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/main.rs`

**Step 1: Reject env-injected report/root-key in strict production**

In strict hardware production:
- reject `TEE_SGX_ROOT_KEY_HEX`
- reject `TEE_SGX_ROOT_KEY_PATH`
- reject report injection env vars

**Step 2: Keep compatibility mode explicit**

If compatibility fallback is still needed, gate it behind a separate env such as:
```text
TEE_HARDWARE_COMPAT_ALLOW_EXTERNAL_MATERIALS=true
```

Default must be false.

**Step 3: Update tests**

Add tests that ensure strict production rejects injected materials.

**Step 4: Run full quality gate**

Run:
```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
cargo test --features tee-hardware --lib --tests --no-run
```

**Step 5: Commit**

```bash
git add src/tee/hardware.rs src/tee/mod.rs src/main.rs tests
git commit -m "chore: remove transitional hardware material injection backdoors"
```

## Definition of Done

All of the following are true:
- `TEE_MODE=hardware` uses a real signed SGX enclave artifact
- host runtime loads enclave successfully
- MRENCLAVE/MRSIGNER come from real enclave identity ECALLs
- report bytes come from enclave-side report generation
- quote bytes come from DCAP quote library FFI
- quote verification uses DCAP verifier FFI
- sealing key comes from enclave-side real hardware path, not simulation
- SGX hardware tests assert success on real hardware
- no production reliance on report/root-key injection env vars

## Risks

- SGX SDK choice may require a toolchain split from the main Rust workspace.
- EDL binding generation can complicate CI and local builds.
- If enclave-side crypto is later required, Phase B/C will widen the trusted codebase and change more host interfaces.
- Sealing identity policy must remain stable across upgrades or master key recovery will break.

## Out of Scope For This Plan

- Moving all credential encryption into enclave
- TDX or SEV-SNP support
- Full QVE supplemental data parsing in API responses
- Cross-platform enclave loaders beyond Linux SGX

## Handoff Notes

Recommended execution order:
1. Task 1-3 first to make the enclave project real
2. Task 4-6 to replace host mock paths
3. Task 7-10 to harden and productionize

Do not start by editing `src/tee/dcap.rs` again in isolation. The remaining blocker is no longer “attestation glue”; it is “missing enclave-side runtime project”.

Plan complete and saved to `docs/plans/2026-03-25-real-sgx-enclave-host-integration.md`. Two execution options:

**1. Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

Which approach?
