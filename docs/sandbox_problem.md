# Sandbox Problem Log

## 2026-04-16 - dashboard.zk.me login input fill not reliably observable

- Problem: The remote sandbox path for Lightpanda + `sandbox_executor.cjs` needed verification against `https://dashboard.zk.me/login`, especially whether login inputs can be operated.
- Remote evidence: Kubernetes connector access to `zkme-dev` found backend pod `credbridge-go-798976f8dc-vcfvt` running image `hub.bitkinetic.com/zkme/credbridge:alpha-259`. Runtime preflight logs showed Node, `puppeteer-core`, Lightpanda, uidmap helpers, user namespaces, cgroup mount, and SGX/DCAP prerequisites passed. The deployment still skips the nsjail smoke test through `CREDBRIDGE_SKIP_NSJAIL_SMOKE_TEST=true`.
- Constraint: Direct `kubectl exec` from the local shell could not run because the sandboxed shell could not resolve the Kubernetes API host (`rcp.bitkinetic.com`), so live in-container browser operation could not be proven in this run.
- Cause found: The deployed executor filled fields through direct `element.value = ...` plus generic `input`/`change` events. React-style controlled inputs, including modern login forms, can miss this because framework value tracking expects the native input/textarea value setter and a real input event.
- Fix: Updated `src/tee/sandbox/scripts/sandbox_executor.cjs` so direct `fill` and credential-backed `credbridge.fill` use the native `HTMLInputElement`/`HTMLTextAreaElement` value setter, focus the element, and dispatch bubbling `InputEvent` and `change` events. Added `tests/tee/sandbox_executor_contract_tests.rs` to lock that behavior. Adjusted the HTTP-request sandbox unit test to skip only when this runner denies local loopback bind with `PermissionDenied`.
- CommitId: `9f2bc7ad633e078294d968b93cc5ccaf4d4ad780`
- Verification: `node --check src/tee/sandbox/scripts/sandbox_executor.cjs`; `cargo fmt`; `cargo clippy --tests -- -D warnings`; `cargo test`.
