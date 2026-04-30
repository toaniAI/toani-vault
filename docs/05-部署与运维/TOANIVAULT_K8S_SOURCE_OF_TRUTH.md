# ToaniVault Kubernetes Source of Truth

**Last updated**: 2026-04-30

## Authoritative deployment lane

For the current dev/test ToaniVault workload, the authoritative workload source of truth is this repository's Drone deployment lane plus the upstream shared Helm chart:

- Pipeline: `.drone.yml`
- Dev values: `.values.yaml`
- Test values: `.test.values.yaml`
- Shared workload chart: `dn/go` from `https://charts.bitkinetic.com/`, currently deployed as chart `go-1.3.3`
- Owner-routing materialization/compatibility patch: `scripts/apply-sandbox-owner-headless-service.sh`

The live dev release confirms this ownership:

- Namespace: `zkme-dev`
- Helm release: `credbridge`
- Workload: `Deployment/credbridge-go`
- Main service: `Service/credbridge-go`
- Owner headless service: `Service/credbridge-go-owner`
- Frontend ingress: `Ingress/dev-credbridge.bitkinetic.com-ingress` and `Ingress/test-credbridge.zk.me-ingress`, both routing to `Service/credbridge-frontend-static`

The upstream `dn/go` chart owns the rendered `Deployment` and main `Service`. Because that chart is external to this repository and currently does not render all ToaniVault-specific owner-routing and lifecycle fields, this repo keeps the ToaniVault-specific values in `.values.yaml` / `.test.values.yaml` and applies a post-Helm compatibility patch from `scripts/apply-sandbox-owner-headless-service.sh` during Drone deployment.

## Required workload resources

The deployment lane must create or maintain:

1. `Deployment/credbridge-go`
   - Explicit `replicaCount` in the values file.
   - `livenessProbe` on `/health`.
   - `readinessProbe` on `/ready`.
   - Explicit `terminationGracePeriodSeconds`.
   - Explicit `preStop` sleep window to let readiness removal and draining propagate before SIGTERM.
2. `Service/credbridge-go`
   - ClusterIP service for normal gateway/API traffic.
3. `Service/credbridge-go-owner`
   - Headless service (`clusterIP: None`).
   - `publishNotReadyAddresses: true`.
   - Selector copied from `Deployment/credbridge-go` so pod DNS names can be used for owner forwarding.
4. Frontend ingress resources
   - The dev/test ingress routes public traffic to `credbridge-frontend-static`.
   - Frontend Nginx proxies `/api/` traffic to `credbridge-go:8080`.

## Sandbox owner routing contract

Runtime env must include:

- `SANDBOX_OWNER_ID` from `metadata.name`.
- `POD_NAMESPACE` from `metadata.namespace`.
- `SANDBOX_OWNER_HEADLESS_SERVICE=credbridge-go-owner`.
- `SANDBOX_OWNER_PORT=8080`.

Current application behavior resolves the owner base URL in this order:

1. `SANDBOX_OWNER_BASE_URL`, when explicitly set.
2. `POD_IP` plus `SANDBOX_OWNER_PORT`.
3. `SANDBOX_OWNER_ID.SANDBOX_OWNER_HEADLESS_SERVICE.POD_NAMESPACE.svc.cluster.local:SANDBOX_OWNER_PORT`.

The owner registry is Redis-backed. Startup removes stale mappings for the current owner. During normal session routing, requests for sessions owned by another pod are forwarded to the recorded owner base URL. The headless service remains required for DNS-based owner addressing and for clusters/environments where pod IP direct routing is not the desired path.

## Health semantics

ToaniVault health semantics are:

- `/health`: liveness only. It answers whether the process is alive and should not be tied to all downstream readiness checks.
- `/ready`: readiness. It includes startup/lifecycle, enclave, attestation, sandbox, audit/storage readiness and returns `503` while the pod should not receive traffic.
- `/health/detail`: same readiness detail as `/ready`, for diagnostics.

Environment-specific attestation settings must match actual runtime capability:

- Dev (`.values.yaml`) uses `TEE_MODE=simulation` because the dev lane does not guarantee production DCAP quote materialization. This prevents expected dev-only `attestation=failed` from keeping the pod permanently NotReady.
- Test/prod-like deployments (`.test.values.yaml`) keep `TEE_MODE=hardware` and must provide working DCAP/SGX prerequisites.

Deployment values and post-Helm patching must keep probes aligned with this contract:

- `livenessProbe.httpGet.path: /health`
- `readinessProbe.httpGet.path: /ready`
- Probe timeout is explicit (`timeoutSeconds: 2`) to avoid relying on chart/K8s defaults.

Frontend/gateway behavior must match this:

- Frontend Nginx `/health` is static-container liveness.
- Frontend Nginx should expose `/ready` as frontend readiness and, where possible, include/proxy the backend `/ready` check with short proxy timeouts.
- `/api/` proxying must use explicit connect/read/send timeouts so stalled backend calls fail boundedly during draining or owner forwarding.

The current frontend chart/release is external to this repository (`credbridge-frontend`, chart `static-0.1.65`). Its source of truth is not present in this repo. Future frontend deployment changes must be made in that real frontend deployment lane, not by editing generated live resources only.

## Environment ownership notes

- Dev deployment is executed from `.drone.yml` `deploy-dev`, using `.values.yaml` and `.drone-runtime.values.yaml`, into `zkme-dev`.
- Test deployment is executed from `.drone.yml` `deploy-test`, using `.test.values.yaml` and `.drone-runtime.values.yaml`, into `zkme-test`.
- The real shared chart source (`dn/go`) is external. If chart maintainers add native support for `sandboxOwnerRouting`, `lifecycle`, and `terminationGracePeriodSeconds`, remove the post-Helm patch and let Helm render the complete desired manifest directly.
- The frontend/static chart source is external. Health/proxy semantic changes for Nginx must be upstreamed there, or supplied through that chart's values if it exposes `default.conf` overrides.

## Verification commands

After a dev deploy:

```bash
kubectl -n zkme-dev get deploy credbridge-go -o yaml
kubectl -n zkme-dev get svc credbridge-go credbridge-go-owner -o yaml
kubectl -n zkme-dev rollout status deployment/credbridge-go --timeout=600s
```

Expected checks:

- `Deployment/credbridge-go.spec.replicas` matches `.values.yaml` `replicaCount`.
- `livenessProbe.httpGet.path` is `/health`.
- `readinessProbe.httpGet.path` is `/ready`.
- `terminationGracePeriodSeconds` is explicit.
- Container `lifecycle.preStop` is present.
- `Service/credbridge-go-owner.spec.clusterIP` is `None`.
- `Service/credbridge-go-owner.spec.publishNotReadyAddresses` is `true`.
