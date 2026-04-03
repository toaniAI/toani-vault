# Structured Data Persistence Remediation Plan

**Date:** 2026-04-03

**Goal:** Eliminate restart-loss for core business state by moving active runtime paths from in-memory/file placeholders onto the repository's existing durable backends: PostgreSQL, Redis, and immudb.

**Primary outcome:** After this work, the main service should persist all core business state that affects authentication, authorization, auditability, tenant behavior, and sandbox traceability.

**Non-goals:**
- Do not move developer-tool workspace state such as `.omx/`, automation memory files, or local CLI preferences into the application database.
- Do not introduce new infrastructure beyond the storage systems already adopted by this repository.
- Do not redesign the auth domain, tenant model, or sandbox domain from scratch. This plan assumes the current schema direction is correct and focuses on wiring and stabilization.

## Progress Update

### Status

In progress. Phase 1 remains partially complete, and Phase 2 is now implemented and verified. The full remediation plan is not complete yet because sandbox persistence and remaining startup-policy hardening work are still open.

### Completed in this pass

- Main runtime now initializes a shared PostgreSQL `DatabasePool` when `DATABASE_URL` is present.
- Production startup now fails fast if `DATABASE_URL` is missing for auth and tenant-config persistence requirements.
- Auth boot wiring in `src/main.rs` no longer defaults to `AuthServiceImpl::new_in_memory()`; it now prefers the database-backed constructor when PostgreSQL is configured.
- Tenant configuration boot wiring no longer hardcodes `MemoryTenantConfigStore`; runtime now chooses PostgreSQL-backed tenant config storage when a database is available, with explicit in-memory fallback only for development-style startup.
- Locale resolution and tenant routes were updated to depend on `Arc<dyn TenantConfigStore>` instead of the memory-only store type.
- A PostgreSQL-backed tenant config store over `tenants.config` was added.
- Focused tests were added for the new tenant-config abstraction boundary.
- Main runtime audit boot wiring no longer defaults to `MemoryAuditStorage`; it now prefers an immudb-backed audit store when `IMMUDB_*` configuration is present.
- Audit read and write paths were unified behind a backend-neutral API adapter so auth, credentials, and `/audit` routes no longer depend on the memory-only storage type.
- The immudb-backed audit path now persists and reloads signed audit entries across client/store restart in the repository's current simulated immudb implementation.
- Audit signing key material is now persisted and reused so post-restart verification continues to use a stable public key.
- Development-only audit fallback is now explicit through `CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK`; the runtime no longer silently downgrades audit persistence to memory.
- immudb-focused tests were extended to cover restart persistence and store recreation behavior.

### Verification completed

The following required gates passed after the code changes in this pass:

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test --test immudb_tests
cargo test
```

### Remaining work

- Phase 1 is only partially complete.
  - Boot-time auth wiring is switched to database-first.
  - Restart-durability coverage for auth flows still needs explicit integration tests.
  - Auth audit persistence acceptance still needs to be confirmed against PostgreSQL-backed rows.
- Phase 2 is complete for the repository's current immudb integration surface.
  - Main runtime now initializes the default audit path with an immudb-backed store when immudb is configured.
  - Audit API and producer paths now use a backend-neutral adapter boundary.
  - Silent downgrade to memory has been removed from the default runtime path.
  - Remaining future work, if desired, is to replace the repository's simulated immudb client with a real immudb network client rather than to finish Phase 2 wiring itself.
- Phase 3 has not been completed.
  - Sandbox session and operation persistence still need a repository layer and crash-recovery reconciliation.
- Phase 4 is partially complete.
  - PostgreSQL-backed tenant config storage is implemented.
  - Fresh-environment bootstrap parity and broader tenant-config acceptance coverage still need to be finished.
- Phase 5 is partially complete.
  - Startup hard-fail behavior now exists for missing PostgreSQL in production for the auth/tenant slice.
  - Per-domain backend reporting and strict policy normalization across audit/sandbox/Redis are still pending.

### Changed files in this pass

- [src/main.rs](/Users/yvan/AIWorkspace/credbridge/src/main.rs)
- [src/api/audit.rs](/Users/yvan/AIWorkspace/credbridge/src/api/audit.rs)
- [src/api/auth.rs](/Users/yvan/AIWorkspace/credbridge/src/api/auth.rs)
- [src/api/credentials.rs](/Users/yvan/AIWorkspace/credbridge/src/api/credentials.rs)
- [src/api/i18n.rs](/Users/yvan/AIWorkspace/credbridge/src/api/i18n.rs)
- [src/api/mod.rs](/Users/yvan/AIWorkspace/credbridge/src/api/mod.rs)
- [src/audit/immudb_client.rs](/Users/yvan/AIWorkspace/credbridge/src/audit/immudb_client.rs)
- [src/audit/immudb_store.rs](/Users/yvan/AIWorkspace/credbridge/src/audit/immudb_store.rs)
- [src/audit/recorder.rs](/Users/yvan/AIWorkspace/credbridge/src/audit/recorder.rs)
- [src/auth/service.rs](/Users/yvan/AIWorkspace/credbridge/src/auth/service.rs)
- [src/tenant/config.rs](/Users/yvan/AIWorkspace/credbridge/src/tenant/config.rs)
- [src/tenant/mod.rs](/Users/yvan/AIWorkspace/credbridge/src/tenant/mod.rs)
- [tests/api/audit_tests.rs](/Users/yvan/AIWorkspace/credbridge/tests/api/audit_tests.rs)
- [tests/audit/immudb_tests.rs](/Users/yvan/AIWorkspace/credbridge/tests/audit/immudb_tests.rs)
- [docs/05-部署与运维/IMMUDB_SETUP.md](/Users/yvan/AIWorkspace/credbridge/docs/05-%E9%83%A8%E7%BD%B2%E4%B8%8E%E8%BF%90%E7%BB%B4/IMMUDB_SETUP.md)

## Problem Statement

The repository already defines durable schemas for most structured business data:

- Auth domain tables exist in [migrations/20260402161446_create_privy_auth_tables.sql](/Users/yvan/AIWorkspace/credbridge/migrations/20260402161446_create_privy_auth_tables.sql).
- Tenant-scoped tables exist in [src/services/db/schema.rs](/Users/yvan/AIWorkspace/credbridge/src/services/db/schema.rs).
- Sandbox persistence tables exist in [migrations/20260317000001_create_sandbox_tables.sql](/Users/yvan/AIWorkspace/credbridge/migrations/20260317000001_create_sandbox_tables.sql).
- Vault persistence already supports PostgreSQL and HashiCorp Vault through [src/vault/storage.rs](/Users/yvan/AIWorkspace/credbridge/src/vault/storage.rs) and [src/vault/postgres.rs](/Users/yvan/AIWorkspace/credbridge/src/vault/postgres.rs).

However, the main runtime still initializes several critical modules with in-memory stores:

- Audit uses `MemoryAuditStorage` in [src/main.rs](/Users/yvan/AIWorkspace/credbridge/src/main.rs#L391).
- Auth uses `AuthServiceImpl::new_in_memory()` in [src/main.rs](/Users/yvan/AIWorkspace/credbridge/src/main.rs#L457).
- Tenant config uses `MemoryTenantConfigStore` in [src/main.rs](/Users/yvan/AIWorkspace/credbridge/src/main.rs#L444).
- Sandbox session state is maintained in-memory inside [src/tee/sandbox/pool.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/pool.rs#L41).

As a result, restart behavior is inconsistent:

- Some business state is durable.
- Some business state is modeled durably but not actually written.
- Some operational state is intentionally transient.
- Some tooling state is correctly file-based and should remain that way.

This plan closes the gap.

## Storage Classification

### Must be durable

These records affect correctness, user-visible behavior, access control, compliance, or incident response:

- users
- external identities
- tenant memberships
- tenant invitations
- auth sessions
- auth audit logs
- tenant configuration
- sandbox session records
- sandbox operation records
- durable audit ledger

### May remain transient in memory

These are caches, warm pools, or active-process runtime internals that can be reconstructed:

- warm nsjail instances
- live sandbox process handles
- in-process rate-limit counters, unless product requirements demand cross-node consistency
- enclave key caches with bounded TTL
- recently-read audit cache in front of immudb

### Should remain file-local, not service-database data

- `.omx/state`, `.omx/logs`, `.omx/plans`
- `automations/*/memory.md`
- CLI config in `~/.config/credbridge/config.toml`
- local operator/dev artifacts

## Current State Inventory

| Domain | Desired durable backend | Schema exists | Runtime writes durably today | Current gap |
| --- | --- | --- | --- | --- |
| Vault credentials | PostgreSQL or Vault | Yes | Yes, depending on backend selection | No major gap |
| Token revocation / TTL state | Redis | Yes by implementation design | Yes where Redis store is used | Verify main auth path integration |
| Auth users / identities / memberships / invitations / sessions | PostgreSQL | Yes | No, main boot path is in-memory | High |
| Auth audit logs | PostgreSQL | Yes | No guaranteed durable path in main boot | High |
| Audit ledger | immudb plus optional DB/query mirror | Yes for immudb path in code | No, main boot path uses in-memory audit | High |
| Tenant config | PostgreSQL preferred | No dedicated table yet | No, in-memory only | Medium |
| Sandbox sessions / operations | PostgreSQL | Yes | No, main runtime keeps them in-memory | High |
| Developer tool state | Files | N/A | Yes | No change needed |

## Target Architecture

```text
Main service
  -> PostgreSQL
     - auth domain
     - tenant domain
     - sandbox records
     - tenant configuration
  -> Redis
     - token TTL / revocation / short-lived session-index state
  -> immudb
     - tamper-evident audit ledger
  -> in-memory caches
     - warm pools
     - read-through caches
     - short-lived process state
```

Design rules:

1. Every user-visible or compliance-relevant state transition must have a durable write path.
2. In-memory state may accelerate execution but must not be the sole source of truth for core business records.
3. Caches must be reconstructible from durable state.
4. Audit writes must be append-only and durable before the corresponding API call is considered successful where policy requires it.
5. Sandbox live-process management may stay in memory, but session metadata and operation traces must be written durably.

## Delivery Phases

## Phase 1: Wire Auth Runtime to PostgreSQL

### Goal

Replace the current in-memory auth initialization with database-backed service initialization.

### Scope

- `users`
- `external_identities`
- `tenant_memberships`
- `tenant_invitations`
- `auth_sessions`
- `auth_audit_logs`

### Files to change

- [src/main.rs](/Users/yvan/AIWorkspace/credbridge/src/main.rs)
- [src/auth/service.rs](/Users/yvan/AIWorkspace/credbridge/src/auth/service.rs)
- [src/api/auth.rs](/Users/yvan/AIWorkspace/credbridge/src/api/auth.rs)
- [src/api/i18n.rs](/Users/yvan/AIWorkspace/credbridge/src/api/i18n.rs)
- [src/services/db/pool.rs](/Users/yvan/AIWorkspace/credbridge/src/services/db/pool.rs)
- [scripts/init-database.sql](/Users/yvan/AIWorkspace/credbridge/scripts/init-database.sql) only if initialization parity is missing

### Implementation steps

1. Add a database-backed `AuthServiceImpl::new(...)` boot path in `src/main.rs`.
2. Load the PostgreSQL pool during startup and fail fast if auth persistence is required but unavailable.
3. Remove the `without database for now` temporary path from the default service boot flow.
4. Ensure all auth mutations already implemented in [src/auth/service.rs](/Users/yvan/AIWorkspace/credbridge/src/auth/service.rs) are exercised through the database-backed constructor.
5. Ensure `verify_session`, `revoke_session`, invitation consumption, and MFA sync all read and write durable session state.
6. Add integration tests that restart service state between create and read assertions.

### Acceptance criteria

- Creating a user from a Privy token survives process restart.
- Invitations survive restart and can be consumed after restart.
- Auth sessions survive restart until expiry/revocation.
- Session revocation remains effective after restart.
- Auth audit rows are queryable from PostgreSQL.

### Risks

- Duplicate logic paths between in-memory and database-backed auth may drift.
- Old tests may still assume memory semantics.

### Required mitigation

- Keep `new_in_memory()` for unit tests only.
- Make the production boot path use the database-backed constructor unconditionally when `DATABASE_URL` is configured.

## Phase 2: Replace Memory Audit with immudb-backed Audit Storage

### Goal

Move the default audit implementation in the main runtime from `MemoryAuditStorage` to `ImmuDbAuditStore`, with an optional PostgreSQL query mirror only if needed for operational querying.

### Files to change

- [src/main.rs](/Users/yvan/AIWorkspace/credbridge/src/main.rs)
- [src/api/audit.rs](/Users/yvan/AIWorkspace/credbridge/src/api/audit.rs)
- [src/audit/immudb_store.rs](/Users/yvan/AIWorkspace/credbridge/src/audit/immudb_store.rs)
- [src/audit/recorder.rs](/Users/yvan/AIWorkspace/credbridge/src/audit/recorder.rs)
- [docker/docker-compose.yml](/Users/yvan/AIWorkspace/credbridge/docker/docker-compose.yml) if startup wiring is incomplete
- [docs/05-部署与运维/IMMUDB_SETUP.md](/Users/yvan/AIWorkspace/credbridge/docs/05-部署与运维/IMMUDB_SETUP.md) if operator instructions need refresh

### Implementation steps

1. Introduce a durable audit initialization branch in `src/main.rs`.
2. Use `ImmuDbAuditStore::from_env(...)` as the default durable backend when immudb is configured.
3. Preserve a bounded in-memory read cache only as an optimization layer.
4. Keep `MemoryAuditStorage` only for tests or explicit development fallback.
5. Update audit API state and adapters so API handlers do not depend on the memory-specific storage type.
6. Add boot-time readiness checks for immudb connectivity.

### Acceptance criteria

- Audit entries survive service restart.
- `audit verify` still works after restart.
- Recent-entry queries return persisted results, not only in-process results.
- Failure to write the audit ledger is surfaced explicitly and does not silently downgrade to memory.

### Risks

- The current API adapter shape is memory-specific.
- immudb connectivity failures may block startup.

### Required mitigation

- Introduce a backend-neutral adapter trait boundary in the API layer.
- Make fallback policy explicit through configuration rather than implicit silent downgrade.

## Phase 3: Persist Sandbox Sessions and Operations

### Goal

Use the existing `sandbox_sessions` and `sandbox_operations` tables as the durable record of sandbox activity while keeping live process handles in memory.

### Files to change

- [src/main.rs](/Users/yvan/AIWorkspace/credbridge/src/main.rs)
- [src/api/sandbox.rs](/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs)
- [src/tee/sandbox/pool.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/pool.rs)
- [src/tee/sandbox/session.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/session.rs)
- new repository module under `src/tee/sandbox/` for persistence, for example `repository.rs`
- [migrations/20260317000001_create_sandbox_tables.sql](/Users/yvan/AIWorkspace/credbridge/migrations/20260317000001_create_sandbox_tables.sql) only if columns are missing

### Implementation steps

1. Add a sandbox repository abstraction over `sandbox_sessions` and `sandbox_operations`.
2. Write a durable session row when a session is created.
3. Write durable operation rows on operation start, completion, failure, and cancellation.
4. Update session termination and expiry to mark durable terminal states.
5. Keep `active_sessions` in memory only as the live-process index.
6. On service startup, reconcile active DB session rows that were left `active` by a crash and mark them `expired` or `terminated` with a recovery reason.

### Acceptance criteria

- Session creation is visible in PostgreSQL before operation execution begins.
- Operation history survives restart.
- Crash recovery marks orphaned active sessions consistently.
- Audit and sandbox timelines can be correlated by tenant, session, operation, and timestamp.

### Risks

- The current pool and session types are tightly coupled to in-memory ownership.
- There may be race conditions between live session state and durable writes.

### Required mitigation

- Treat PostgreSQL as the source of truth for historical state.
- Treat in-memory `active_sessions` as a live index only.
- Write session and operation state transitions transactionally where possible.

## Phase 4: Persist Tenant Configuration

### Goal

Replace `MemoryTenantConfigStore` for primary runtime behavior with durable tenant config storage.

### Scope decision

The repository does not currently expose a dedicated `tenant_configs` table. Implement this by extending the existing `tenants.config JSONB` column in [scripts/init-database.sql](/Users/yvan/AIWorkspace/credbridge/scripts/init-database.sql#L24), unless a stronger separation becomes necessary.

### Files to change

- [src/main.rs](/Users/yvan/AIWorkspace/credbridge/src/main.rs)
- [src/tenant/config.rs](/Users/yvan/AIWorkspace/credbridge/src/tenant/config.rs)
- [src/tenant/service.rs](/Users/yvan/AIWorkspace/credbridge/src/tenant/service.rs)
- [scripts/init-database.sql](/Users/yvan/AIWorkspace/credbridge/scripts/init-database.sql) if tenant config shape must be formalized
- optionally add a new migration if `tenants.config` needs constraints or indexes

### Implementation steps

1. Define a PostgreSQL-backed tenant config store implementing the existing trait surface.
2. Read and write tenant settings from `tenants.config`.
3. Use memory store only for unit tests.
4. Seed the default tenant config through SQL/bootstrap, not only through process memory at startup.

### Acceptance criteria

- Tenant language and settings survive restart.
- Default tenant configuration is reproducible in fresh environments.
- API consumers observe the same tenant settings after restart.

## Phase 5: Normalize Boot-Time Storage Policy

### Goal

Make storage mode explicit and safe across all domains.

### Files to change

- [src/main.rs](/Users/yvan/AIWorkspace/credbridge/src/main.rs)
- [src/config.rs](/Users/yvan/AIWorkspace/credbridge/src/config.rs)
- [README.md](/Users/yvan/AIWorkspace/credbridge/README.md)
- [README_zh.md](/Users/yvan/AIWorkspace/credbridge/README_zh.md)
- deployment docs under [docs/05-部署与运维](/Users/yvan/AIWorkspace/credbridge/docs/05-部署与运维/README.md)

### Implementation steps

1. Define per-domain storage requirements:
   - vault: postgres or vault
   - auth: postgres
   - audit: immudb
   - token state: redis
   - sandbox records: postgres
2. Fail startup when required backends are unavailable in non-development environments.
3. Allow explicit memory fallbacks only in development/test, behind named environment flags.
4. Log the resolved storage backend for each domain at startup.

### Acceptance criteria

- Startup logs show the selected backend per domain.
- Production mode cannot silently run auth/audit/sandbox persistence in memory.
- Development mode can still opt into in-memory fallbacks for local iteration.

## Detailed Work Breakdown

## Workstream A: Auth persistence

### Deliverables

- Database-backed service initialization
- integration coverage for auth restart durability
- removal of runtime dependence on in-memory auth in main boot path

### Tests

- create user -> restart -> get user
- create invitation -> restart -> consume invitation
- create session -> restart -> verify session
- revoke session -> restart -> verify revoked

## Workstream B: Audit durability

### Deliverables

- immudb-backed audit startup path
- backend-neutral audit API adapter
- explicit fallback policy

### Tests

- record audit entry -> restart -> query recent
- verify audit chain after multiple writes and restart
- immudb unavailable -> startup failure or explicit configured fallback

## Workstream C: Sandbox persistence

### Deliverables

- repository for sandbox session/operation writes
- crash-recovery reconciliation
- API list/get endpoints reading durable records where applicable

### Tests

- create session -> DB row exists
- start operation -> operation row exists
- operation success/failure updates terminal state
- simulated crash -> startup reconciliation updates orphan sessions

## Workstream D: Tenant config durability

### Deliverables

- Postgres-backed tenant config store
- bootstrap path that seeds defaults durably

### Tests

- save config -> restart -> load config
- fresh environment bootstrap creates expected default config

## Suggested Implementation Order

1. Phase 1 auth persistence
2. Phase 2 audit persistence
3. Phase 4 tenant config persistence
4. Phase 3 sandbox persistence
5. Phase 5 storage policy normalization

Reasoning:

- Auth and audit are the highest-risk restart-loss gaps.
- Tenant config is low-complexity and removes a hidden memory-only dependency.
- Sandbox persistence is important but more invasive because live process management and durable record-keeping must be separated carefully.

## Verification Plan

For each phase:

1. Add or update unit tests around repository/service boundaries.
2. Add integration tests that prove restart durability.
3. Run:

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
```

4. For persistence-sensitive paths, add targeted manual verification:

```bash
# Example shape, exact commands to be finalized during implementation
docker compose -f docker/docker-compose.yml up -d postgres redis immudb
cargo run
# perform mutation
# restart service
# verify state still exists
```

## Rollout Strategy

### Development rollout

- Implement feature-complete durable paths.
- Keep explicit in-memory fallbacks for tests and local-only development.

### Staging rollout

- Enable PostgreSQL + Redis + immudb together.
- Execute restart-resilience test suite.
- Validate startup hard-fail behavior when a required backend is unavailable.

### Production rollout

- Enable strict durable backend requirements.
- Remove any implicit memory fallback from production config.
- Monitor startup backend-resolution logs and write-path failures.

## Open Decisions

These decisions should be resolved before implementation starts:

1. Whether auth audit events should be written only to PostgreSQL, or to both PostgreSQL and immudb.
2. Whether sandbox records need an immutable audit copy in addition to PostgreSQL rows.
3. Whether tenant config should remain inside `tenants.config` or move to a dedicated `tenant_configs` table later.
4. Whether rate limiting should remain single-node in-memory or move to Redis in a later phase.

Recommended answers for this plan:

- Auth audit: write to PostgreSQL for domain querying, and emit critical security events to immudb as well.
- Sandbox records: keep primary history in PostgreSQL; emit security-sensitive operations to audit.
- Tenant config: use `tenants.config` now.
- Rate limiting: defer unless multi-node requirements are active.

## Completion Definition

This remediation is complete when all of the following are true:

- Main runtime no longer uses in-memory auth by default.
- Main runtime no longer uses in-memory audit by default.
- Tenant configuration survives restart.
- Sandbox session and operation records survive restart.
- Production startup fails if required durable backends are missing.
- Restart-resilience tests exist and pass for auth, audit, tenant config, and sandbox records.

## Appendix: Explicitly Out of Scope

The following structured data should not be moved into the application database as part of this effort:

- Codex/OMX workspace state under `.omx/`
- automation scratch memory under `automations/*/memory.md`
- developer-local CLI config under `~/.config/credbridge/config.toml`
- ephemeral warm instance queues and live child-process handles

Those are operational or developer-tool artifacts, not product business records.
