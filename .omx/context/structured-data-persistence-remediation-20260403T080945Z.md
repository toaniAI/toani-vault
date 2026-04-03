## Task Statement

Execute `/Users/yvan/AIWorkspace/credbridge/docs/plans/2026-04-03-structured-data-persistence-remediation-plan.md` end to end, then verify and publish the completed work.

## Desired Outcome

Replace restart-losing in-memory runtime paths for auth, audit, tenant config, and sandbox persistence with durable backends already used by the repo, with tests and release-ready verification.

## Known Facts / Evidence

- `/Users/yvan/AIWorkspace/credbridge/src/main.rs` still wires `MemoryAuditStorage`, `AuthServiceImpl::new_in_memory(...)`, and `MemoryTenantConfigStore` into the main boot path.
- `/Users/yvan/AIWorkspace/credbridge/src/auth/service.rs` already contains a PostgreSQL-aware service shape via `db_pool: Option<PgPool>`, but the main runtime defaults to the memory constructor.
- `/Users/yvan/AIWorkspace/credbridge/src/tenant/config.rs` defines a tenant config abstraction surface, while the runtime still relies on the memory store.
- `/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/pool.rs` keeps active session state only in memory.
- The repo is already dirty in unrelated files; do not revert user work.
- `omx` is installed, but the current leader session is not inside tmux, so team execution should use the programmatic tmux team runtime rather than assuming direct `omx team` pane control from the leader.

## Constraints

- Follow `/Users/yvan/AIWorkspace/credbridge/AGENTS.md` and nested instructions.
- No new dependencies without explicit request.
- Keep diffs reviewable and reversible.
- Before publish, run `cargo fmt`, `cargo clippy --tests -- -D warnings`, and `cargo test` with zero warnings/errors.
- Publish only after verification evidence exists.

## Unknowns / Open Questions

- How much of the auth and tenant persistence implementation already exists beyond the constructors.
- Whether immudb and sandbox persistence glue code are partially implemented elsewhere.
- Whether the full plan is feasible in a single pass without intermediate defect fixes from validation.

## Likely Touchpoints

- `/Users/yvan/AIWorkspace/credbridge/src/main.rs`
- `/Users/yvan/AIWorkspace/credbridge/src/auth/`
- `/Users/yvan/AIWorkspace/credbridge/src/audit/`
- `/Users/yvan/AIWorkspace/credbridge/src/tenant/`
- `/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/`
- `/Users/yvan/AIWorkspace/credbridge/tests/`
- `/Users/yvan/AIWorkspace/credbridge/docs/05-部署与运维/`
- `/Users/yvan/AIWorkspace/credbridge/README.md`
- `/Users/yvan/AIWorkspace/credbridge/README_zh.md`
