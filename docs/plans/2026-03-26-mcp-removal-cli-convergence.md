# MCP Removal and CLI Convergence Plan

**Date:** 2026-03-26

**Goal:** Remove the standalone MCP server and converge all AI-facing integration onto the main CredBridge service plus the official CLI.

**Non-goal:** This document does not cover backward compatibility or migration. The system is not yet released, so the target architecture can be adopted directly.

## Target Architecture

Adopt a single business authority:

```text
AI Agent
  -> credbridge CLI
      -> credbridge-sdk
          -> main CredBridge HTTP service
              -> auth / token / vault / tee / audit / tenant / sandbox
```

The main service becomes the only runtime that owns:

- configuration and environment loading
- storage backend initialization
- TEE lifecycle and key hierarchy
- token issuance and validation
- audit logging
- tenant isolation
- credential CRUD and decrypt rules

The CLI becomes the only AI-facing tool surface:

- stable commands
- stable machine-readable output
- explicit exit codes
- no duplicated business logic

The standalone `mcp-server/` crate is removed.

## Why This Direction

The current repository has two competing runtime entry points:

- the main service in `src/main.rs`
- the standalone MCP server in `mcp-server/src/main.rs`

That split is acceptable only if MCP is a very thin adapter. It is not thin today.

Current MCP code owns its own:

- transport layer
- token validation
- session lifecycle
- tool routing
- server state
- credential business operations

Most importantly, the MCP server currently initializes its own in-memory runtime state through `McpServerState::new_in_memory()` in `mcp-server/src/lib.rs`, instead of reusing the main service runtime. That creates an architectural fork:

- different initialization path
- different storage assumptions
- duplicated auth surface
- duplicated audit path
- duplicated capability surface

Since the intended future interaction model is "AI uses CLI", the MCP layer no longer provides strategic value.

## Current State Summary

### Main service

The main service starts in `src/main.rs` and owns the real application boot path:

- config loading
- app state initialization
- Axum router assembly
- HTTP serving

Relevant files:

- `src/main.rs`
- `src/api/*`
- `src/token/*`
- `src/tee/*`
- `src/vault/*`
- `src/audit/*`
- `src/tenant/*`

### CLI

The CLI already exists and already talks to the service through the Rust SDK.

Relevant files:

- `cli/src/main.rs`
- `cli/src/cli.rs`
- `cli/src/commands/auth.rs`
- `cli/src/commands/credentials.rs`
- `cli/src/commands/tokens.rs`
- `sdk-rust/src/*`

This is the correct AI integration direction, because the CLI is already an adapter over the service API rather than an independent business host.

### MCP server

The standalone MCP implementation currently lives here:

- `mcp-server/src/main.rs`
- `mcp-server/src/lib.rs`
- `mcp-server/src/handlers.rs`
- `mcp-server/src/tools.rs`
- `mcp-server/src/sse.rs`
- `mcp-server/src/auth.rs`
- `mcp-server/src/message_queue.rs`

This code currently mixes protocol concerns with business concerns.

### MCP token storage inside main library

The main library also exposes MCP-specific token storage:

- `src/mcp/mod.rs`
- `src/mcp/token_storage.rs`

This module should be evaluated as part of the removal. If the token storage is only useful for MCP workflows, it should be deleted. If any parts are generically useful, they should be renamed and moved under a non-MCP namespace.

## Design Principles

1. The main service is the only business authority.
2. The CLI is a thin operator and agent interface.
3. Business logic is never implemented in protocol adapters.
4. AI workflows must be scriptable through stable CLI commands.
5. Machine-readable output is a first-class requirement.
6. Authentication, authorization, audit, and tenant isolation remain server-side concerns.
7. No new MCP-specific abstractions should survive the refactor.

## Target Code Ownership

### Main service owns

- credential CRUD rules
- decrypt authorization rules
- token scope enforcement
- audit event emission
- TEE-backed decrypt execution
- tenant/user scoping
- service-side validation and error mapping

Primary code areas:

- `src/api/`
- `src/services/`
- `src/token/`
- `src/vault/`
- `src/tee/`
- `src/audit/`

### CLI owns

- command parsing
- local config and login state
- output formatting
- non-interactive automation UX
- human-friendly wrappers over the SDK

Primary code areas:

- `cli/src/cli.rs`
- `cli/src/commands/*`
- `cli/src/output.rs`
- `cli/src/config.rs`

### SDK owns

- HTTP client transport
- request/response serialization
- auth header wiring
- structured error mapping

Primary code area:

- `sdk-rust/src/*`

### Delete entirely

- `mcp-server/`
- `docs/mcp-examples/`

Likely delete or relocate:

- `src/mcp/`

## Refactor Strategy

The refactor should be executed in this order.

## Phase 1: Freeze the Target Interface

Define the CLI as the long-term agent surface.

Required CLI principles:

- every agent-relevant command supports `--json`
- stdout is reserved for the result payload
- stderr is reserved for diagnostics
- exit codes are deterministic
- interactive prompts can be disabled or bypassed

Required command families:

- `credbridge auth ...`
- `credbridge credentials list ...`
- `credbridge credentials get ...`
- `credbridge credentials create ...`
- `credbridge credentials update ...`
- `credbridge credentials delete ...`
- `credbridge credentials decrypt ...`
- `credbridge tokens ...`
- `credbridge audit ...`
- `credbridge sandbox ...`

Immediate gap to close:

- token creation and token listing in `cli/src/commands/tokens.rs` are still unimplemented
- credential update/version workflows in `cli/src/commands/credentials.rs` are still placeholders

## Phase 2: Move Business Capability to the Main Service Boundary

Audit all business behavior currently embedded in `mcp-server/src/tools.rs`.

The following capabilities must exist only once, in the main service stack:

- list credentials
- get credential metadata
- create credential
- update credential
- delete credential
- decrypt credential
- tee status retrieval

Expected implementation direction:

- keep HTTP handlers in `src/api/`
- move reusable business orchestration into `src/services/`
- ensure both HTTP handlers and future internal callers use the same service layer

Recommended new module:

- `src/services/credentials/`

Recommended responsibilities for this module:

- request validation beyond transport syntax
- tenant/user scoping
- authorization hooks
- audit emission
- orchestration over vault + tee + token
- transport-agnostic domain errors

Do not move business logic into the CLI.

## Phase 3: Remove MCP-Specific Runtime Concepts

Delete MCP runtime concerns that will no longer exist in the product:

- tool schemas
- JSON-RPC request dispatch
- SSE session manager
- MCP bearer token validator
- MCP message queue
- stdio transport

Files to remove:

- `mcp-server/src/main.rs`
- `mcp-server/src/lib.rs`
- `mcp-server/src/handlers.rs`
- `mcp-server/src/tools.rs`
- `mcp-server/src/sse.rs`
- `mcp-server/src/auth.rs`
- `mcp-server/src/message_queue.rs`
- `mcp-server/Cargo.toml`
- `mcp-server/README.md`

Also clean product and developer docs that describe MCP setup.

## Phase 4: Collapse MCP-Specific Shared Library Code

Review `src/mcp/mod.rs` and `src/mcp/token_storage.rs`.

Decision rule:

- if the code only exists to support MCP session/token workflows, delete it
- if part of the code is generically useful for agent token storage, rename and move it under a neutral namespace such as `src/token/agent_storage.rs`

Default recommendation:

- delete the `src/mcp/` module unless a concrete non-MCP use case exists

Also remove exports from:

- `src/lib.rs`

## Phase 5: Strengthen the CLI as the Official Agent Interface

The CLI must become reliable enough for direct AI use without MCP.

### Command behavior requirements

- every mutating command supports non-interactive execution
- every command supports `--output json`
- sensitive output is deliberate and explicit
- errors are structured enough for agents to recover
- commands avoid decorative text in JSON mode

### Recommended additions

- `credbridge credentials list --service <id> --json`
- `credbridge credentials get <id> --json`
- `credbridge credentials decrypt <id> --json`
- `credbridge tokens create --name <name> --scopes ... --json`
- `credbridge tokens list --json`
- `credbridge auth status --json`

### Recommended AI ergonomics

- `--no-confirm`
- `--stdin`
- `--quiet`
- stable field names
- explicit error codes in JSON mode

### SDK alignment

The CLI should continue to call the main service through `credbridge-sdk`.
Avoid direct coupling from CLI into internal server modules.

## Phase 6: Remove Documentation and Build References

Remove references to MCP from:

- product docs
- developer docs
- user guides
- examples
- Docker or build scaffolding if present

Known areas to review:

- `docs/08-用户指南/USER_MANUAL.md`
- `docs/design/mcp-sse-design.md`
- `docs/mcp-examples/`
- `AGENTS.md`
- `CLAUDE.md`
- `Dockerfile`

Also search for:

- `CREDBRIDGE_MCP_`
- `credbridge-mcp-server`
- `mcp-server`
- `mcp:connect`

## Recommended File-Level Work Breakdown

### Workstream A: Main service capability cleanup

Files likely touched:

- `src/main.rs`
- `src/services/mod.rs`
- `src/api/credentials/*`
- `src/token/*`
- `src/audit/*`
- `src/vault/*`
- `src/lib.rs`

Outcome:

- one canonical business path for credential and token operations

### Workstream B: CLI completion

Files likely touched:

- `cli/src/cli.rs`
- `cli/src/main.rs`
- `cli/src/commands/auth.rs`
- `cli/src/commands/credentials.rs`
- `cli/src/commands/tokens.rs`
- `cli/src/output.rs`
- `cli/src/config.rs`
- `sdk-rust/src/*`

Outcome:

- CLI is sufficient for direct AI usage

### Workstream C: MCP deletion

Files likely touched:

- delete `mcp-server/`
- remove `src/mcp/` or relocate any generic remnants
- remove docs and examples

Outcome:

- repository no longer exposes MCP as a product surface

## Acceptance Criteria

The refactor is complete when all of the following are true:

1. There is no standalone `mcp-server/` crate in the repository.
2. The main service is the only server runtime that owns credential business logic.
3. The CLI can perform all AI-relevant credential workflows against the main service.
4. The CLI supports machine-readable output for those workflows.
5. No MCP-specific environment variables or docs remain.
6. `src/lib.rs` does not export MCP-specific modules.
7. Repository documentation describes CLI as the official AI integration path.

## Explicitly Out of Scope

- backward compatibility
- adapter shims for old MCP clients
- data migration plans
- deprecation windows
- dual-stack operation

Those are intentionally excluded because the product is not yet released.

## Proposed Execution Checklist

1. Complete missing CLI command coverage and JSON behavior.
2. Extract or normalize canonical credential business orchestration under `src/services/`.
3. Remove any remaining MCP-owned business logic by moving or deleting it.
4. Delete `mcp-server/`.
5. Delete or relocate `src/mcp/`.
6. Remove MCP docs, examples, build references, and environment variables.
7. Run formatting, lint, and tests across the main service and CLI.

## Verification

At the end of the work, verify at minimum:

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test

cd cli
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
```

## Final Recommendation

Treat this as a convergence refactor, not a feature migration.

The correct end state is:

- one service runtime
- one SDK transport path
- one CLI tool surface for agents
- zero MCP-specific runtime or product concepts

That architecture is simpler, easier to secure, easier to test, and better aligned with the intended future interaction model.
