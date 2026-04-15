# Sandbox Problem Log

## 2026-04-16 - remote exec remains blocked at 06:01 CST

- Problem: Rerun needed to prove the remote CredBridge container can run `nsjail` + Lightpanda + `sandbox_executor.cjs` against `https://dashboard.zk.me/login`, especially whether login input fields are operable.
- Prior context absorbed: Existing entries already captured the React controlled-input cause and the source fix, plus repeated failure to reach the Kubernetes API host from this automation runner.
- Remote evidence: Kubernetes MCP sees backend pod `credbridge-go-55489d5d6-987x6` in namespace `zkme-dev`, deployment revision `110`, image `hub.bitkinetic.com/zkme/credbridge:alpha-261`, ready `1/1`. Deployment status is healthy. Recent backend logs visible through MCP are only `/health` probe traffic; no live sandbox execution attempt was observable.
- Constraint: Helper-based `kubectl` with the MCP kubeconfig token still fails before pod selection because this runner cannot resolve `rcp.bitkinetic.com`. Local `curl -I https://dashboard.zk.me/login` also fails with `Could not resolve host`. Kubernetes MCP can inspect objects/logs/events but still does not expose arbitrary pod exec, so in-container Lightpanda/nsjail/dashboard input operation remains unproven.
- Additional finding: Deployment still sets `CREDBRIDGE_SKIP_NSJAIL_SMOKE_TEST=true`, so startup preflight cannot substitute for the requested live nsjail browser proof.
- Code status: No new code-level defect was identified in this run. Current source still contains the known fixes: React-compatible native setter + `InputEvent` fill semantics in `sandbox_executor.cjs`, and `NsjailConfig` emits `--disable_clone_newns`.
- CommitId: No new commit; no push. I did not close the automation because the requested success condition was not proven, and I did not create a corrective code commit because the observed blocker is the local automation runner DNS/exec path rather than a newly isolated application bug.
- Verification: `node --check src/tee/sandbox/scripts/sandbox_executor.cjs`; `cargo test --test sandbox_executor_contract_tests`; Kubernetes MCP pod/deployment/log/event inspection; helper-based `kubectl` attempt failed with DNS resolution for `rcp.bitkinetic.com`; local dashboard curl failed DNS resolution.

## 2026-04-16 - remote exec remains blocked at 05:01 CST

- Problem: Rerun needed to prove the remote CredBridge container can run `nsjail` + Lightpanda + `sandbox_executor.cjs` against `https://dashboard.zk.me/login`, especially whether login input fields are operable.
- Prior context absorbed: The existing log already captured the likely React controlled-input issue and the fix in `sandbox_executor.cjs`; this run focused on whether the deployed remote container could now be exercised directly.
- Remote evidence: Kubernetes MCP still sees backend pod `credbridge-go-55489d5d6-987x6` in namespace `zkme-dev`, deployment revision `110`, image `hub.bitkinetic.com/zkme/credbridge:alpha-261`, ready `1/1`, with healthy `/health` probe logs.
- Constraint: Local `kubectl` has no current context, and this automation runner still cannot resolve either `rcp.bitkinetic.com` or `dashboard.zk.me` (`curl: (6) Could not resolve host`). DNS tools also fail under the sandbox with socket bind permission errors. Kubernetes MCP can inspect objects/logs but still does not expose arbitrary pod exec, so live in-container Lightpanda/nsjail/dashboard input operation remains unproven.
- Additional finding: Deployment still sets `CREDBRIDGE_SKIP_NSJAIL_SMOKE_TEST=true`, so startup preflight continues to skip the nsjail smoke test and cannot substitute for the requested live exec proof.
- Code status: No new code-level defect was identified in this run. Current source still contains the known fixes: React-compatible `InputEvent` fill semantics in `sandbox_executor.cjs`, and `NsjailConfig` emits `--disable_clone_newns`.
- CommitId: No new commit; no push. I did not close the automation because the requested success condition was not proven, and I did not create a corrective code commit because the observed blocker is the local automation runner DNS/exec path rather than a newly isolated application bug.
- Verification: `node --check src/tee/sandbox/scripts/sandbox_executor.cjs`; `cargo test --test sandbox_executor_contract_tests`; Kubernetes MCP pod/deployment/log/event inspection. Live `kubectl exec` and dashboard input operation could not be executed from this runner.

## 2026-04-16 - remote exec remains blocked at 04:01 CST

- Problem: Rerun needed to prove the remote CredBridge container can run `nsjail` + Lightpanda + `sandbox_executor.cjs` against `https://dashboard.zk.me/login`, especially whether page input fields are operable.
- Prior context absorbed: The prior entries already captured the React-controlled-input cause and the current deployment/image state, including image `alpha-261` and skipped nsjail smoke test.
- Remote evidence: Kubernetes MCP still sees backend pod `credbridge-go-55489d5d6-987x6` in namespace `zkme-dev`, deployment revision `110`, image `hub.bitkinetic.com/zkme/credbridge:alpha-261`, ready `1/1`. Deployment status is healthy.
- Constraint: Direct `kubectl exec` using the helper and MCP kubeconfig token still fails before pod selection because this runner cannot resolve `rcp.bitkinetic.com`. Local `curl https://dashboard.zk.me/login` also fails DNS resolution. Kubernetes MCP can list/get/log objects but still does not expose arbitrary pod exec.
- Additional finding: Deployment still sets `CREDBRIDGE_SKIP_NSJAIL_SMOKE_TEST=true`, so preflight remains insufficient to prove actual nsjail execution.
- Code status: No new code-level cause found in this run. Current source still contains the known fixes: React-compatible fill semantics in `sandbox_executor.cjs`, and `NsjailConfig` uses `--disable_clone_newns`.
- CommitId: No new commit; no push. Runtime success remains unproven from this automation environment, but the observed blocker is the local automation runner DNS/exec path rather than a newly identified application bug.
- Verification: `node --check src/tee/sandbox/scripts/sandbox_executor.cjs`; `cargo test --test sandbox_executor_contract_tests`; Kubernetes MCP pod/deployment/log/event inspection. Live in-container Lightpanda/nsjail/dashboard input operation could not be executed.

## 2026-04-16 - remote exec still blocked; deployed image advanced to alpha-261

- Problem: Rerun needed to prove the remote CredBridge container can run `nsjail` + Lightpanda + `sandbox_executor.cjs` against `https://dashboard.zk.me/login`, especially whether page input fields are operable.
- Prior context absorbed: The previous entry already identified the likely React-controlled-input failure mode and fixed `sandbox_executor.cjs` to use native input/textarea value setters plus bubbling `InputEvent`/`change` events.
- Remote evidence: Kubernetes MCP sees backend pod `credbridge-go-55489d5d6-987x6` in namespace `zkme-dev`, deployment revision `110`, image `hub.bitkinetic.com/zkme/credbridge:alpha-261`, ready `1/1`. Events show `alpha-261` pulled and started at `2026-04-16 02:09:29 +0800`, replacing `alpha-260`.
- Constraint: Direct `kubectl exec` from the local shell still cannot reach the cluster because DNS lookup for `rcp.bitkinetic.com` fails. Local `curl` to `dev-credbridge.bitkinetic.com` also fails DNS resolution. Kubernetes MCP can list/get/log cluster objects but does not expose arbitrary pod exec, so live in-container browser operation remains unproven from this automation environment.
- Additional finding: The deployment still sets `CREDBRIDGE_SKIP_NSJAIL_SMOKE_TEST=true`, so startup preflight does not prove an actual nsjail launch even though other runtime checks are healthy.
- Code status: Current source already contains the prior fixes for both known sandbox blockers: `sandbox_executor.cjs` has React-compatible fill semantics, and `NsjailConfig` emits `--disable_clone_newns` rather than the invalid historical `--disable_clone_newmnt`.
- CommitId: No new commit; no push. `origin/branch_dev` already contains the sandbox input fix commit `9f2bc7ad633e078294d968b93cc5ccaf4d4ad780` and log commit `0b8e8caab8cb13a842c0b17bc0a463b2a425b883`.
- Verification: Kubernetes MCP object/event inspection only. Live `kubectl exec`, public ingress curl, and dashboard input operation could not be executed because this local sandbox cannot resolve required network hosts.

## 2026-04-16 - dashboard.zk.me login input fill not reliably observable

- Problem: The remote sandbox path for Lightpanda + `sandbox_executor.cjs` needed verification against `https://dashboard.zk.me/login`, especially whether login inputs can be operated.
- Remote evidence: Kubernetes connector access to `zkme-dev` found backend pod `credbridge-go-798976f8dc-vcfvt` running image `hub.bitkinetic.com/zkme/credbridge:alpha-259`. Runtime preflight logs showed Node, `puppeteer-core`, Lightpanda, uidmap helpers, user namespaces, cgroup mount, and SGX/DCAP prerequisites passed. The deployment still skips the nsjail smoke test through `CREDBRIDGE_SKIP_NSJAIL_SMOKE_TEST=true`.
- Constraint: Direct `kubectl exec` from the local shell could not run because the sandboxed shell could not resolve the Kubernetes API host (`rcp.bitkinetic.com`), so live in-container browser operation could not be proven in this run.
- Cause found: The deployed executor filled fields through direct `element.value = ...` plus generic `input`/`change` events. React-style controlled inputs, including modern login forms, can miss this because framework value tracking expects the native input/textarea value setter and a real input event.
- Fix: Updated `src/tee/sandbox/scripts/sandbox_executor.cjs` so direct `fill` and credential-backed `credbridge.fill` use the native `HTMLInputElement`/`HTMLTextAreaElement` value setter, focus the element, and dispatch bubbling `InputEvent` and `change` events. Added `tests/tee/sandbox_executor_contract_tests.rs` to lock that behavior. Adjusted the HTTP-request sandbox unit test to skip only when this runner denies local loopback bind with `PermissionDenied`.
- CommitId: `9f2bc7ad633e078294d968b93cc5ccaf4d4ad780`
- Verification: `node --check src/tee/sandbox/scripts/sandbox_executor.cjs`; `cargo fmt`; `cargo clippy --tests -- -D warnings`; `cargo test`.
